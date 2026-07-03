//! WebView-driven SERP fetch — the **only** search network path.
//!
//! The app already ships a real browser engine (WebView2/Chromium on Windows).
//! Instead of imitating one with a static HTTP client (which can't run JS and
//! gets fingerprint-/challenge-blocked), we navigate a **hidden, reused** window
//! to the search URL. The document origin *is* the engine (so no CORS), Chromium
//! renders + clears any anti-bot/JS challenge with its genuine TLS/JA3
//! fingerprint and cookie jar, and we read the rendered `outerHTML` back. The
//! existing [`super::parsers`] / [`super::engines`] then run on it, unchanged.
//!
//! ## Why a singleton, reused window
//! One long-lived hidden window keeps the cookie jar + any Cloudflare clearance
//! cookie warm across queries (re-solving the challenge every time would be both
//! slow and more bot-like), avoids per-query window-create cost, and never
//! flashes UI. Navigations are serialized by [`WebviewSerp`]`::nav` — search is
//! low-frequency (≤1 / 5 s, chat-triggered), so one-at-a-time is fine and
//! sidesteps concurrent-navigation races.
//!
//! ## How the page reports back
//! A *remote* page can't call arbitrary app commands in Tauri v2 (only
//! permissioned plugin commands), so the injected init-script reports the
//! rendered HTML via the core **event** plugin (`plugin:event|emit`, granted to
//! the `serp-fetcher` window in `capabilities/webview-serp.json`), which Rust
//! listens for. Each navigation carries a request id in the URL **query**
//! (`&__serpid=…` — a query param, not a fragment, so it survives a redirect that
//! wraps the URL in `continue=`); the init-script reads it at document-start and
//! echoes it back to attribute the emit. When a redirect *drops* the param
//! (Baidu), the script reports with an empty id and [`resolve_pending_key`] routes
//! it via the sole in-flight navigation (they're serialized, so there's only one).
//!
//! ## Resilience (no fallback catches a miss now)
//! * **Readiness wait** — the init-script polls for a known results container
//!   before grabbing, up to a hard cap, instead of a blind fixed settle.
//! * **Challenge-gated retry** — [`is_challenge_page`] detects Cloudflare/CAPTCHA
//!   interstitials; [`should_retry`] permits exactly one re-navigation of the
//!   same warm window when challenged. A page that merely parses to *empty* is
//!   **not** retried (no benefit, keeps latency bounded).
//! * **Self-heal** — a closed/wedged window is rebuilt before failing.
//! Every error path degrades to "no web context"; chat is never blocked.
//!
//! ## Test builds
//! The window-creation path ([`mod@real`]) is compiled out under `cfg(test)`: it
//! links native windowing imports (tao/comctl32) that only bind when the process
//! carries the bundled app's common-controls/DPI manifest — which the
//! `cargo test` harness exe does not, so linking them would make the whole
//! unit-test binary fail to load (`STATUS_ENTRYPOINT_NOT_FOUND`). The offline
//! tests never reach the WebView, so a stub [`fetch_serp`] stands in. The pure
//! helpers below ([`serp_url`], [`init_script`], [`is_challenge_page`],
//! [`should_retry`]) stay always-compiled so the URL/correlation/retry contract
//! *is* unit-tested without a browser.

use super::{engines, SearchEngine};

/// Event the rendered page emits with its extracted HTML.
const SERP_EVENT: &str = "serp-html";
/// URL-fragment key carrying the per-navigation request id.
const ID_KEY: &str = "__serpid";
/// Hard cap (ms) the in-page script waits for a results container before it
/// grabs whatever is there (covers challenge pages that never render results).
const READY_MAX_MS: u64 = 4000;
/// Poll interval (ms) while waiting for the results container.
const POLL_MS: u64 = 150;
/// Short tail (ms) after the container appears, to let late rows paint.
const SETTLE_MS: u64 = 400;
/// Max navigate→render→extract attempts (1 original + 1 challenge retry).
const MAX_ATTEMPTS: u8 = 2;

/// Managed state for the singleton hidden fetch window. Its internals only exist
/// in non-test builds (see the module's "Test builds" note).
#[derive(Default)]
pub struct WebviewSerp {
    /// Serializes navigations through the one shared window (keeps the cookie
    /// jar warm; avoids concurrent-navigation races).
    #[cfg(not(test))]
    nav: tokio::sync::Mutex<()>,
    /// In-flight extract channels keyed by request id.
    #[cfg(not(test))]
    pending: std::sync::Mutex<
        std::collections::HashMap<String, tokio::sync::oneshot::Sender<String>>,
    >,
    /// Whether the app-lifetime event listener has been installed yet.
    #[cfg(not(test))]
    listener_installed: std::sync::atomic::AtomicBool,
    /// Engines whose cookie jar has been warmed this session (homepage visited
    /// once before the first search).
    #[cfg(not(test))]
    warmed: std::sync::Mutex<std::collections::HashSet<SearchEngine>>,
    /// When the last manual-solve prompt failed — drives the cooldown so we don't
    /// re-pop the window on every search after the user dismissed it.
    #[cfg(not(test))]
    manual_cooldown: std::sync::Mutex<Option<std::time::Instant>>,
    /// Whether a manual-solve session is currently surfaced. At most one runs at
    /// a time — a burst of challenged queries must not stack windows.
    #[cfg(not(test))]
    manual_active: std::sync::atomic::AtomicBool,
}

/// Percent-encode a query value for the SERP URL query string.
fn enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Build the SERP URL for `engine`/`query`, tagging it with the per-navigation
/// request id as a query parameter (`&__serpid=<id>`).
///
/// The id rides in the **query string** (not the fragment) so it survives an
/// anti-bot **redirect**: when Google bounces `/search?…` to
/// `/sorry/index?continue=<original-url>`, the original URL — including our id —
/// is preserved, percent-encoded, inside `continue`. The init-script recovers it
/// from the (decoded) redirected URL, so a redirect-style challenge is still
/// correlated and detected instead of silently timing out. Search engines ignore
/// the unknown parameter.
fn serp_url(engine: SearchEngine, query: &str, id: &str) -> String {
    let (base, param) = engines::endpoint(engine);
    format!("{base}?{param}={}&{ID_KEY}={id}", enc(query))
}

/// Engine homepage — navigated once per session before the first search to warm
/// the cookie jar (a human lands on the site, then searches), which makes the
/// engine markedly less likely to serve a bot challenge.
fn homepage_url(engine: SearchEngine) -> &'static str {
    match engine {
        SearchEngine::BingCn => "https://cn.bing.com/",
        SearchEngine::Bing => "https://www.bing.com/",
        SearchEngine::Google => "https://www.google.com/",
        SearchEngine::Baidu => "https://www.baidu.com/",
        SearchEngine::DuckDuckGo => "https://duckduckgo.com/",
    }
}

/// Pick which in-flight request an emitted HTML belongs to. Normally the exact
/// id matches; but because navigations are serialized (≤1 pending at a time), a
/// still-unattributed emit with the **sole** pending entry is routed there too —
/// a belt-and-suspenders fallback for any case where the id didn't survive.
/// Returns `None` if it can't be attributed. Pure — unit-tested.
fn resolve_pending_key(pending_ids: &[String], emitted_id: &str) -> Option<String> {
    if pending_ids.iter().any(|k| k == emitted_id) {
        return Some(emitted_id.to_string());
    }
    if pending_ids.len() == 1 {
        return Some(pending_ids[0].clone());
    }
    None
}

/// Init-script injected at document-start (WebView2 runs embedder scripts before
/// page scripts and outside the page CSP). Top frame only; recovers its request
/// id from the **decoded full URL** (so it's found even when a challenge redirect
/// wraps the original URL in a `continue=` param), then polls for a known results
/// container (up to [`READY_MAX_MS`]) and emits the rendered `outerHTML` via the
/// core event plugin.
///
/// If the id is **absent** — an engine (Baidu) redirects `/s?…&__serpid=…` to a
/// URL that drops the param — it reports anyway with an empty id rather than
/// bailing silently (which would time out invisibly). Navigations are serialized,
/// so [`resolve_pending_key`]'s sole-pending fallback still attributes it to the
/// in-flight fetch, turning a mystery timeout into a real outcome (results, or a
/// visible challenge). The window only ever loads our own top-frame navigations,
/// and the warm-up homepage has no pending entry, so a stray emit is dropped.
fn init_script() -> String {
    // Runs at document-start, after Tauri's IPC init-script, before page scripts.
    format!(
        r#"(function() {{
  if (window.top !== window.self) return;          // ignore ad/iframe sub-frames
  var __ti = window.__TAURI_INTERNALS__;           // capture the IPC entry point
  var invoke = (__ti && __ti.invoke) ? __ti.invoke.bind(__ti) : null;
  var href = location.href;
  try {{ href = decodeURIComponent(href); }} catch (e) {{}}
  var m = href.match(/{key}=([0-9]+)/);            // survives a `continue=`-wrapped redirect
  var ID = m ? m[1] : '';                          // no id (redirect dropped it)? still report
  // Result containers across Bing / Google / Baidu / DuckDuckGo.
  var SELECTORS = ['#b_results', '#rso', '#search', '#content_left', '#links', '.result'];
  var MAX = {max}, POLL = {poll}, SETTLE = {settle}, waited = 0, done = false;
  function ready() {{
    for (var i = 0; i < SELECTORS.length; i++) {{
      if (document.querySelector(SELECTORS[i])) return true;
    }}
    return false;
  }}
  function grab() {{
    if (done) return; done = true;
    try {{
      var html = document.documentElement.outerHTML;
      if (invoke) invoke('plugin:event|emit', {{ event: {event:?}, payload: {{ id: ID, html: html }} }});
    }} catch (e) {{ console.error('[serp] emit failed', e); }}
  }}
  function tick() {{
    if (done) return;
    if (ready()) {{ setTimeout(grab, SETTLE); return; }}
    waited += POLL;
    if (waited >= MAX) {{ grab(); return; }}
    setTimeout(tick, POLL);
  }}
  tick();
}})();"#,
        key = ID_KEY,
        event = SERP_EVENT,
        max = READY_MAX_MS,
        poll = POLL_MS,
        settle = SETTLE_MS,
    )
}

/// Outcome of one SERP fetch — the unit of the bot-detection benchmark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Parsed N results (the good path).
    Results(usize),
    /// An anti-bot interstitial (Cloudflare / reCAPTCHA / "unusual traffic").
    Challenge,
    /// A real page that parsed to zero results (layout miss / genuinely empty).
    Empty,
    /// Fetch errored (timeout / setup failure).
    Failed,
}

impl Outcome {
    pub fn label(&self) -> &'static str {
        match self {
            Outcome::Results(_) => "results",
            Outcome::Challenge => "challenge",
            Outcome::Empty => "empty",
            Outcome::Failed => "failed",
        }
    }
}

/// Classify a fetch result for benchmarking / logging. Pure — unit-tested.
pub fn classify_outcome(engine: SearchEngine, fetch: &Result<String, String>) -> Outcome {
    match fetch {
        Ok(html) => match super::parse_rendered(engine, html).len() {
            // Results win. A page we can parse into results IS a SERP, even if its
            // HTML also carries a challenge-y token — Google embeds reCAPTCHA
            // assets on ordinary results pages, so checking the marker first
            // misclassifies working SERPs as challenges (it did). Only a page that
            // yielded **no** results and shows a challenge marker is a real block.
            0 if is_challenge_page(html) => Outcome::Challenge,
            0 => Outcome::Empty,
            n => Outcome::Results(n),
        },
        Err(_) => Outcome::Failed,
    }
}

/// ASCII challenge markers (matched case-insensitively). Deliberately specific to
/// challenge chrome so a result snippet that merely mentions "captcha" won't trip.
const CHALLENGE_ASCII: &[&str] = &[
    "just a moment",
    "cf-browser-verification",
    "/cdn-cgi/challenge",
    "cf_chl_",
    "challenge-platform",
    "unusual traffic",
    "/sorry/index",          // Google interstitial
    "g-recaptcha",
    "id=\"recaptcha\"",
    "h-captcha",
];
/// CJK challenge markers (matched as-is).
const CHALLENGE_CJK: &[&str] = &["人机验证", "安全验证", "异常流量", "请完成验证", "滑动验证", "百度安全验证"];

/// Which challenge markers appear in `html`. Basis of [`is_challenge_page`], and
/// surfaced by the benchmark as diagnostic detail (`/sorry/index` or "unusual
/// traffic" means a real Google block; a lone `g-recaptcha` on a large page is a
/// stray asset on a working SERP). Pure — unit-tested.
pub fn challenge_markers(html: &str) -> Vec<&'static str> {
    let lower = html.to_lowercase();
    let mut hits: Vec<&'static str> =
        CHALLENGE_ASCII.iter().copied().filter(|m| lower.contains(m)).collect();
    hits.extend(CHALLENGE_CJK.iter().copied().filter(|m| html.contains(m)));
    hits
}

/// True when `html` is an anti-bot interstitial (Cloudflare / CAPTCHA / "unusual
/// traffic"), not a real SERP. Pure — unit-tested.
pub fn is_challenge_page(html: &str) -> bool {
    !challenge_markers(html).is_empty()
}

/// A challenge only *blocks* when the page also produced no results. A real SERP
/// that merely carries a stray challenge token (Google embeds reCAPTCHA assets on
/// ordinary results pages) is **not** a block — it must serve its results, never
/// trigger a retry or the manual-solve window. Pure — unit-tested.
pub fn is_blocking_challenge(has_results: bool, html: &str) -> bool {
    !has_results && is_challenge_page(html)
}

/// Whether to re-navigate after a failed attempt: only when the page was a
/// *blocking* challenge (marker **and** no results) and attempts remain. A
/// merely-empty/unparseable page, or a real SERP with a stray token, is **not**
/// retried. Pure — unit-tested. (`attempt` is 1-based.)
pub fn should_retry(attempt: u8, blocking_challenge: bool, max_attempts: u8) -> bool {
    attempt < max_attempts && blocking_challenge
}

/// Policy for [`manual_solve_enabled`], factored out to be unit-testable without
/// mutating process env. Benchmarks force it **off** (a benchmark run is
/// unattended — it must never pop a window). Otherwise it defaults **on**: when a
/// search hits a challenge page, surface the window so the user can clear it
/// once; the clearance cookie then persists in the profile for later searches.
/// An explicit off-ish `MUTSUMI_SERP_MANUAL` value disables it. Pure.
fn manual_solve_policy(bench_active: bool, manual_env: Option<&str>) -> bool {
    if bench_active {
        return false;
    }
    match manual_env {
        Some(v) => v != "0" && !v.eq_ignore_ascii_case("false") && !v.eq_ignore_ascii_case("off"),
        None => true,
    }
}

/// Whether the **manual-solve backstop** is enabled. Default on (a challenge
/// surfaces the window for the user to clear); disable with
/// `MUTSUMI_SERP_MANUAL=0` (or `false`/`off`). Suppressed automatically during an
/// unattended benchmark run (`MUTSUMI_SERP_BENCH` set). The chat search budget is
/// short, so the *first* challenged query may already have answered without web
/// context by the time the window appears — solving it still warms the clearance
/// cookie so subsequent searches succeed.
pub fn manual_solve_enabled() -> bool {
    manual_solve_policy(
        std::env::var("MUTSUMI_SERP_BENCH").is_ok(),
        std::env::var("MUTSUMI_SERP_MANUAL").ok().as_deref(),
    )
}

/// Localized (title, instruction) for the challenge-solve window. Rust-side
/// (mirrors [`crate::tray`]'s `labels_for_locale`) because the window navigates
/// to a *remote* page and so can't reach the Vue i18n system. Supported locales:
/// `zh` | `ja` | else English. Pure — unit-tested.
fn solve_strings(locale: &str) -> (&'static str, &'static str) {
    match locale {
        "zh" => (
            "请完成验证",
            "该搜索引擎要求验证。请在此窗口内完成验证；完成后窗口会自动关闭并继续搜索。",
        ),
        "ja" => (
            "認証を完了してください",
            "検索エンジンが認証を求めています。このウィンドウで認証を完了してください。完了すると自動的に閉じ、検索を続行します。",
        ),
        _ => (
            "Complete verification",
            "This search engine needs a quick verification. Please complete it in this window — it closes automatically and the search continues.",
        ),
    }
}

/// JS that pins a fixed instruction banner to the top of the (remote) challenge
/// page so the surfaced window explains itself. Uses CSSOM property assignment
/// (never a `style` attribute) to stay clear of the page's `style-src` CSP, and a
/// sentinel id so a re-inject is idempotent. Pure — unit-tested.
fn solve_banner_js(instruction: &str) -> String {
    let text = serde_json::to_string(instruction).unwrap_or_else(|_| "\"\"".into());
    format!(
        r#"(function(){{
  try {{
    if (document.getElementById('__mutsumi_hint')) return;
    var b = document.createElement('div');
    b.id = '__mutsumi_hint';
    b.textContent = {text};
    b.style.position = 'fixed'; b.style.top = '0'; b.style.left = '0'; b.style.right = '0';
    b.style.zIndex = '2147483647'; b.style.padding = '12px 16px';
    b.style.font = '14px system-ui, -apple-system, sans-serif'; b.style.lineHeight = '1.4';
    b.style.background = '#1f2937'; b.style.color = '#ffffff';
    b.style.textAlign = 'center'; b.style.boxShadow = '0 2px 8px rgba(0,0,0,.3)';
    (document.body || document.documentElement).appendChild(b);
  }} catch(e) {{}}
}})();"#,
        text = text,
    )
}

/// Decide whether to surface the window for a manual solve: only on a genuine
/// challenge, and not while a recent failed prompt's cooldown is active (so we
/// don't nag on every subsequent search). Pure — unit-tested.
fn should_prompt_manual(is_challenge: bool, cooldown_active: bool) -> bool {
    is_challenge && !cooldown_active
}

/// Test stub — no window creation, so the unit-test harness doesn't link the
/// native windowing imports. `search()` only reaches here with a real
/// `AppHandle`, which the offline tests never have, so it is never invoked.
#[cfg(test)]
pub async fn fetch_serp(
    _app: &tauri::AppHandle,
    _engine: SearchEngine,
    _query: &str,
) -> Result<String, String> {
    Err("webview SERP fetch is disabled under cfg(test)".into())
}

#[cfg(not(test))]
pub use real::fetch_serp;

#[cfg(not(test))]
mod real {
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use tauri::{AppHandle, Listener, Manager, WebviewUrl, WebviewWindowBuilder};
    use tokio::sync::oneshot;

    use super::{
        homepage_url, init_script, is_blocking_challenge, manual_solve_enabled,
        resolve_pending_key, serp_url, should_prompt_manual, should_retry, solve_banner_js,
        solve_strings, WebviewSerp, MAX_ATTEMPTS, SERP_EVENT,
    };
    use crate::search::SearchEngine;

    /// Fixed label of the singleton hidden fetch window.
    const SERP_WINDOW: &str = "serp-fetcher";
    /// Hard cap on one navigate→render→extract round-trip. Sized to absorb the
    /// cold-start cost of creating the window on the first query; warm
    /// navigations finish far under it.
    const WEBVIEW_TIMEOUT: Duration = Duration::from_secs(7);
    /// Pause before a challenge retry, giving the in-page JS challenge a moment
    /// to clear on the now-warm window.
    const RETRY_BACKOFF: Duration = Duration::from_millis(800);
    /// How long to let the homepage warm-up navigation settle its cookies before
    /// the first search navigates away.
    const WARMUP_SETTLE: Duration = Duration::from_millis(1500);
    /// How long to wait for the user to clear a surfaced challenge.
    const MANUAL_TIMEOUT: Duration = Duration::from_secs(150);
    /// After a dismissed/failed manual prompt, don't re-pop the window for this long.
    const MANUAL_COOLDOWN: Duration = Duration::from_secs(300);

    #[derive(serde::Deserialize)]
    struct SerpHtml {
        id: String,
        html: String,
    }

    fn uid() -> String {
        let n = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        format!("{n}")
    }

    /// Install the app-lifetime listener that routes emitted HTML back to the
    /// awaiting fetch by request id. Idempotent — runs at most once.
    fn ensure_listener(app: &AppHandle, state: &WebviewSerp) {
        if state.listener_installed.swap(true, Ordering::SeqCst) {
            return;
        }
        let app_l = app.clone();
        app.listen_any(SERP_EVENT, move |event| {
            let raw = event.payload();
            // Payload arrives as JSON; accept either an object or a JSON-encoded
            // string of one (depending on how the event layer wraps it).
            let parsed = serde_json::from_str::<SerpHtml>(raw).ok().or_else(|| {
                serde_json::from_str::<String>(raw)
                    .ok()
                    .and_then(|s| serde_json::from_str::<SerpHtml>(&s).ok())
            });
            if let Some(msg) = parsed {
                let state = app_l.state::<WebviewSerp>();
                let mut pending = state.pending.lock().unwrap();
                let keys: Vec<String> = pending.keys().cloned().collect();
                if let Some(key) = resolve_pending_key(&keys, &msg.id) {
                    if let Some(tx) = pending.remove(&key) {
                        let _ = tx.send(msg.html);
                    }
                }
            }
        });
    }

    /// Create the hidden window. Must run on the main (UI) thread.
    ///
    /// Incognito (ephemeral) — a plain Chromium tab with the browser's own
    /// genuine fingerprint. We tried fingerprint/profile hardening; a benchmark
    /// (see `docs/benchmarks/search-bot-detection.md`) showed it changed nothing,
    /// because the engines return results to this WebView as-is. Cookies still
    /// persist for the window's session (it's long-lived + reused), which is all
    /// the manual-solve backstop needs.
    fn build_window(app: &AppHandle, url: tauri::Url) -> Result<(), String> {
        // MUTSUMI_SERP_VISIBLE shows the rendered fetch page so you can watch what
        // an engine actually returns. Devtools is a *separate* opt-in
        // (MUTSUMI_SERP_DEVTOOLS) so the console doesn't cover the page you're
        // trying to see.
        let visible = std::env::var("MUTSUMI_SERP_VISIBLE").is_ok();
        let devtools = std::env::var("MUTSUMI_SERP_DEVTOOLS").is_ok();

        let built = WebviewWindowBuilder::new(app, SERP_WINDOW, WebviewUrl::External(url))
            .title("search")
            .visible(visible)
            .skip_taskbar(true)
            .inner_size(1000.0, 800.0)
            .initialization_script(init_script().as_str())
            .incognito(true)
            .build()
            .map_err(|e| format!("build: {e}"))?;
        #[cfg(debug_assertions)]
        if devtools {
            built.open_devtools();
        }
        let _ = built;
        Ok(())
    }

    /// Create the hidden window (first call) or navigate the existing one. On a
    /// navigate error the window is assumed wedged: close + rebuild it once
    /// before giving up (self-heal across long sessions / OS suspend-resume).
    /// Must run on the main (UI) thread.
    fn build_or_navigate(app: &AppHandle, url: tauri::Url) -> Result<(), String> {
        if let Some(w) = app.get_webview_window(SERP_WINDOW) {
            if w.navigate(url.clone()).is_ok() {
                return Ok(());
            }
            let _ = w.close();
        }
        build_window(app, url)
    }

    /// Navigate the shared window to the engine homepage once per session to warm
    /// its cookie jar before the first search (fire-and-forget: no id, so the
    /// homepage never emits — we just give it a moment to set cookies). Assumes
    /// the caller holds the nav lock.
    async fn warm_up(app: &AppHandle, engine: SearchEngine) {
        let Ok(url) = homepage_url(engine).parse::<tauri::Url>() else { return };
        let app2 = app.clone();
        let (setup_tx, setup_rx) = oneshot::channel::<Result<(), String>>();
        if app
            .run_on_main_thread(move || {
                let _ = setup_tx.send(build_or_navigate(&app2, url));
            })
            .is_err()
        {
            return;
        }
        // Wait for the navigation to be scheduled, then let cookies settle.
        let _ = setup_rx.await;
        tokio::time::sleep(WARMUP_SETTLE).await;
    }

    /// Hide the singleton fetch window (main thread).
    fn hide_window(app: &AppHandle) {
        let app2 = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Some(w) = app2.get_webview_window(SERP_WINDOW) {
                let _ = w.hide();
            }
        });
    }

    /// Surface the fetch window for a manual solve (main thread): set a localized
    /// title, show + focus it, and inject the instruction banner so the otherwise
    /// hidden window explains what the user needs to do.
    fn surface_for_solve(app: &AppHandle, title: &str, banner_js: &str) {
        let app2 = app.clone();
        let title = title.to_string();
        let banner = banner_js.to_string();
        let _ = app.run_on_main_thread(move || {
            if let Some(w) = app2.get_webview_window(SERP_WINDOW) {
                let _ = w.set_title(&title);
                let _ = w.show();
                let _ = w.set_focus();
                let _ = w.eval(&banner);
            }
        });
    }

    /// Backstop: the window is already on a challenge page — surface it so the
    /// user clears the reCAPTCHA in place, then wait for the solved SERP to report
    /// back (its `continue=`-preserved id, or the single-pending fallback, routes
    /// the emit here). Returns the solved HTML, or `Err` if the user didn't solve
    /// in time. Assumes the caller holds the nav lock.
    async fn manual_solve(
        app: &AppHandle,
        state: &WebviewSerp,
        engine: SearchEngine,
    ) -> Result<String, String> {
        let id = uid();
        let (tx, rx) = oneshot::channel::<String>();
        state.pending.lock().unwrap().insert(id.clone(), tx);

        // Localize the window chrome from the frontend-reported locale (the SERP
        // page is remote, so it can't use the Vue i18n system).
        let locale = app
            .try_state::<crate::app_state::LocaleState>()
            .map(|s| s.get())
            .unwrap_or_default();
        let (title, hint) = solve_strings(&locale);

        log::info!(
            "search: {engine:?} still challenged — surfacing window for manual solve (≤{}s)",
            MANUAL_TIMEOUT.as_secs()
        );
        surface_for_solve(app, title, &solve_banner_js(hint));
        let outcome = tokio::time::timeout(MANUAL_TIMEOUT, rx).await;
        hide_window(app);
        state.pending.lock().unwrap().remove(&id);

        match outcome {
            Ok(Ok(html)) => Ok(html),
            _ => Err("manual solve timed out".into()),
        }
    }

    /// One navigate→render→extract round-trip. Returns the rendered HTML or an
    /// error (timeout / setup failure). Assumes the caller holds the nav lock.
    async fn navigate_once(
        app: &AppHandle,
        state: &WebviewSerp,
        engine: SearchEngine,
        query: &str,
    ) -> Result<String, String> {
        let id = uid();
        let full_url = serp_url(engine, query, &id);
        let url: tauri::Url = full_url.parse().map_err(|e| format!("bad url {full_url}: {e}"))?;

        let (tx, rx) = oneshot::channel::<String>();
        state.pending.lock().unwrap().insert(id.clone(), tx);

        // Window create / navigate happens on the main thread; report its result
        // so a build/navigate error fails fast instead of waiting out the
        // timeout.
        let app2 = app.clone();
        let (setup_tx, setup_rx) = oneshot::channel::<Result<(), String>>();
        let scheduled = app.run_on_main_thread(move || {
            let _ = setup_tx.send(build_or_navigate(&app2, url));
        });

        let result = async {
            scheduled.map_err(|e| format!("run_on_main_thread: {e}"))?;
            match setup_rx.await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => return Err(e),
                Err(_) => return Err("setup channel dropped".into()),
            }
            match tokio::time::timeout(WEBVIEW_TIMEOUT, rx).await {
                Ok(Ok(html)) => Ok(html),
                Ok(Err(_)) => Err("extract channel dropped".into()),
                Err(_) => Err("timed out waiting for rendered HTML".into()),
            }
        }
        .await;

        // Drop the pending entry if we bailed before the emit arrived.
        state.pending.lock().unwrap().remove(&id);
        result
    }

    /// Drive a SERP fetch through the hidden WebView; returns the rendered HTML.
    /// Retries exactly once on a detected anti-bot challenge (never on a plain
    /// empty page). Best-effort — every error path is recoverable by the
    /// caller degrading to no web context.
    pub async fn fetch_serp(
        app: &AppHandle,
        engine: SearchEngine,
        query: &str,
    ) -> Result<String, String> {
        let state = app.state::<WebviewSerp>();
        ensure_listener(app, &state);

        // One navigation at a time through the shared window.
        let _guard = state.nav.lock().await;

        // Warm this engine's cookie jar once per session (reduces bot challenges).
        if !state.warmed.lock().unwrap().contains(&engine) {
            warm_up(app, engine).await;
            state.warmed.lock().unwrap().insert(engine);
        }

        let mut last = navigate_once(app, &state, engine, query).await?;
        for attempt in 1..MAX_ATTEMPTS {
            let has_results = !crate::search::parse_rendered(engine, &last).is_empty();
            if !should_retry(attempt, is_blocking_challenge(has_results, &last), MAX_ATTEMPTS) {
                break;
            }
            log::info!("search: webview {engine:?} hit a blocking challenge, retrying once");
            tokio::time::sleep(RETRY_BACKOFF).await;
            last = navigate_once(app, &state, engine, query).await?;
        }

        // Backstop: if still challenged, hand the solve to a **detached** task.
        // The caller's search budget is short (chat caps it and cancels this
        // future on timeout), so blocking here would tear the surfaced window down
        // before the user could finish — and orphan it, since the cleanup runs
        // after the wait. The task re-acquires the nav lock (serializing with
        // other searches, so nothing navigates the window away mid-solve) and
        // reserves the window for the user. This turn returns the challenge HTML
        // as-is — it parses to no results, so chat proceeds without web context;
        // the cleared cookie then warms subsequent searches. Cooldown-gated so a
        // dismissed prompt doesn't re-pop on every search.
        if manual_solve_enabled() {
            let has_results = !crate::search::parse_rendered(engine, &last).is_empty();
            let blocked = is_blocking_challenge(has_results, &last);
            let cooldown_active = state
                .manual_cooldown
                .lock()
                .unwrap()
                .map(|t| t.elapsed() < MANUAL_COOLDOWN)
                .unwrap_or(false);
            if should_prompt_manual(blocked, cooldown_active) {
                spawn_manual_solve(app.clone(), engine, query.to_string());
            }
        }

        Ok(last)
    }

    /// Spawn a detached solve session so the caller's (short) search budget can't
    /// cancel it mid-solve. At most one runs at a time (guarded by
    /// `manual_active`). The task owns the nav lock for its duration, so no
    /// concurrent search can navigate the window away while the user is solving.
    fn spawn_manual_solve(app: AppHandle, engine: SearchEngine, query: String) {
        {
            let state = app.state::<WebviewSerp>();
            if state.manual_active.swap(true, Ordering::SeqCst) {
                return; // a solve session is already up
            }
        }
        tauri::async_runtime::spawn(async move {
            let state = app.state::<WebviewSerp>();
            let _guard = state.nav.lock().await;
            let outcome = run_manual_solve_session(&app, &state, engine, &query).await;
            match outcome {
                Ok(()) => *state.manual_cooldown.lock().unwrap() = None, // solved → clear
                Err(_) => *state.manual_cooldown.lock().unwrap() = Some(Instant::now()),
            }
            state.manual_active.store(false, Ordering::SeqCst);
        });
    }

    /// Under the nav lock: re-navigate to the query so the window is on a fresh
    /// challenge page it owns, then surface it and wait for the user to clear it.
    /// `Ok` if the challenge was already gone (cookie warm) or the user solved;
    /// `Err` on timeout. Assumes the caller holds the nav lock.
    async fn run_manual_solve_session(
        app: &AppHandle,
        state: &WebviewSerp,
        engine: SearchEngine,
        query: &str,
    ) -> Result<(), String> {
        let html = navigate_once(app, state, engine, query).await?;
        let has_results = !crate::search::parse_rendered(engine, &html).is_empty();
        if !is_blocking_challenge(has_results, &html) {
            return Ok(()); // results present (or no block) — nothing to solve
        }
        manual_solve(app, state, engine).await.map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    /// Mirror of the init-script's recovery logic (`decodeURIComponent(href)` then
    /// `/__serpid=([0-9]+)/`), so the tests prove the same id the browser reads
    /// back — including when a challenge redirect wraps our URL in `continue=`.
    fn serpid_from_url(url: &str) -> Option<String> {
        let decoded = crate::search::tracking::percent_decode(url);
        let re = Regex::new(&format!(r"{ID_KEY}=([0-9]+)")).unwrap();
        re.captures(&decoded).map(|c| c[1].to_string())
    }

    #[test]
    fn serp_url_uses_engine_endpoint_and_param() {
        assert!(serp_url(SearchEngine::BingCn, "hello", "1").starts_with("https://cn.bing.com/search?q="));
        assert!(serp_url(SearchEngine::Bing, "hello", "1").starts_with("https://www.bing.com/search?q="));
        assert!(serp_url(SearchEngine::Google, "hello", "1").starts_with("https://www.google.com/search?q="));
        assert!(serp_url(SearchEngine::Baidu, "hello", "1").starts_with("https://www.baidu.com/s?wd="));
        assert!(serp_url(SearchEngine::DuckDuckGo, "hello", "1").starts_with("https://html.duckduckgo.com/html/?q="));
    }

    #[test]
    fn serp_url_percent_encodes_cjk_and_spaces() {
        let u = serp_url(SearchEngine::DuckDuckGo, "福安 天气", "9");
        assert!(u.contains("%20"), "space not encoded: {u}");
        assert!(!u.contains('福'), "raw CJK leaked: {u}");
        assert!(u.parse::<tauri::Url>().is_ok(), "unparseable: {u}");
    }

    #[test]
    fn serp_url_id_rides_in_the_query_and_round_trips() {
        // The correlation scheme: the id must be recoverable from the URL by the
        // same logic the init-script uses.
        for id in ["1", "1700000000000000001", "42"] {
            let u = serp_url(SearchEngine::Bing, "any query", id);
            assert!(u.contains(&format!("&{ID_KEY}={id}")), "id not in query: {u}");
            assert!(!u.contains('#'), "id must not be a fragment: {u}");
            assert_eq!(serpid_from_url(&u).as_deref(), Some(id), "url was {u}");
        }
    }

    #[test]
    fn serpid_survives_a_challenge_redirect() {
        // Google bounces `/search?…&__serpid=123` to `/sorry/index?continue=<enc>`;
        // the id survives, percent-encoded, inside `continue` and must still be
        // recoverable (so the challenge is detected, not silently timed out).
        let original = serp_url(SearchEngine::Google, "test", "123456789");
        let continue_enc = original.replace('&', "%26").replace('=', "%3D");
        let redirected = format!("https://www.google.com/sorry/index?continue={continue_enc}&hl=en");
        assert_eq!(serpid_from_url(&redirected).as_deref(), Some("123456789"), "redirected url was {redirected}");
    }

    #[test]
    fn homepage_urls_are_distinct_and_parseable() {
        for e in [
            SearchEngine::BingCn,
            SearchEngine::Bing,
            SearchEngine::Google,
            SearchEngine::Baidu,
            SearchEngine::DuckDuckGo,
        ] {
            assert!(homepage_url(e).parse::<tauri::Url>().is_ok(), "bad homepage for {e:?}");
        }
    }

    #[test]
    fn resolve_pending_prefers_exact_id_then_falls_back_to_sole_entry() {
        // Exact match wins.
        assert_eq!(
            resolve_pending_key(&["a".into(), "b".into()], "b").as_deref(),
            Some("b")
        );
        // No id match but a single in-flight request → routed to it (redirect case).
        assert_eq!(resolve_pending_key(&["only".into()], "").as_deref(), Some("only"));
        // Ambiguous (>1 pending, no match) → give up rather than misattribute.
        assert_eq!(resolve_pending_key(&["a".into(), "b".into()], "z"), None);
        // Nothing pending → nothing to route.
        assert_eq!(resolve_pending_key(&[], "x"), None);
    }

    #[test]
    fn init_script_carries_the_event_name_key_and_readiness() {
        let js = init_script();
        assert!(js.contains("serp-html"), "event name missing");
        assert!(js.contains("__serpid"), "id key missing");
        assert!(js.contains("decodeURIComponent(href)"), "redirect-surviving recovery missing");
        assert!(js.contains("4000"), "readiness cap missing");
        assert!(js.contains("#b_results"), "readiness selector missing");
        // Reports through the captured, bound invoke via the permissioned event
        // plugin, top-frame only.
        assert!(js.contains(".invoke.bind("), "must capture a bound invoke");
        assert!(js.contains("if (invoke) invoke("), "grab must use the captured invoke");
        assert!(js.contains("plugin:event|emit"));
        assert!(js.contains("window.top !== window.self"));
    }

    // ── outcome classifier (benchmark instrumentation) ─────────────────────

    #[test]
    fn classify_outcome_covers_every_arm() {
        let bing = r#"<div id="b_results"><ol><li class="b_algo">
            <h2><a href="https://example.com/a">A</a></h2><div class="b_caption"><p>s</p></div>
          </li></ol></div>"#;
        assert!(matches!(
            classify_outcome(SearchEngine::BingCn, &Ok(bing.to_string())),
            Outcome::Results(n) if n >= 1
        ));

        let cf = r#"<title>Just a moment...</title><div class="cf-browser-verification"></div>"#;
        assert_eq!(classify_outcome(SearchEngine::Google, &Ok(cf.to_string())), Outcome::Challenge);

        let empty = "<html><body><main>nothing recognizable</main></body></html>";
        assert_eq!(classify_outcome(SearchEngine::BingCn, &Ok(empty.to_string())), Outcome::Empty);

        assert_eq!(
            classify_outcome(SearchEngine::BingCn, &Err("timed out".to_string())),
            Outcome::Failed
        );
    }

    // ── challenge detection + retry policy (objective 3, user Test 4A/4B) ──

    #[test]
    fn is_challenge_page_detects_cloudflare_and_captcha() {
        assert!(is_challenge_page(
            r#"<html><head><title>Just a moment...</title></head>
               <body><div class="cf-browser-verification"></div></body></html>"#
        ));
        assert!(is_challenge_page(r#"<div class="g-recaptcha" data-sitekey="x"></div>"#));
        assert!(is_challenge_page("<html><body>百度安全验证</body></html>"));
        assert!(is_challenge_page("<html><body>检测到异常流量</body></html>"));
    }

    #[test]
    fn is_challenge_page_false_on_a_real_serp() {
        let serp = r#"<ol><li class="b_algo"><h2><a href="https://example.com/a">Title A</a></h2>
            <div class="b_caption"><p>An ordinary snippet about a CAPTCHA tutorial.</p></div></li></ol>"#;
        assert!(!is_challenge_page(serp), "a result that mentions captcha must not count as a challenge");
    }

    #[test]
    fn challenge_markers_reports_each_hit_for_diagnosis() {
        assert!(challenge_markers("<html><body>ordinary results</body></html>").is_empty());
        let m = challenge_markers(r#"<title>Just a moment...</title><div class="g-recaptcha"></div>"#);
        assert!(m.contains(&"just a moment") && m.contains(&"g-recaptcha"), "got {m:?}");
        // The discriminating marker: a real Google block says /sorry/index, not just g-recaptcha.
        assert!(challenge_markers("redirected to /sorry/index?continue=x").contains(&"/sorry/index"));
        assert!(challenge_markers("<p>百度安全验证</p>").contains(&"百度安全验证"));
    }

    #[test]
    fn should_retry_exactly_once_on_a_blocking_challenge() {
        // Blocking challenge → retry once, then stop at the attempt cap.
        assert!(should_retry(1, true, MAX_ATTEMPTS));
        assert!(!should_retry(MAX_ATTEMPTS, true, MAX_ATTEMPTS));
        // Not a block (real SERP, or merely-empty page) → never retry.
        assert!(!should_retry(1, false, MAX_ATTEMPTS));
    }

    #[test]
    fn is_blocking_challenge_requires_no_results() {
        let cf = r#"<title>Just a moment...</title><div class="cf-browser-verification"></div>"#;
        // Challenge markers but results WERE parsed → not a block. This is the bug
        // the screenshot exposed: a working Google SERP embeds a challenge-y token,
        // and we must serve its results instead of popping a solve window.
        assert!(!is_blocking_challenge(true, cf));
        // Challenge markers and no results → a genuine block.
        assert!(is_blocking_challenge(false, cf));
        // No markers, no results → just empty, not a challenge.
        assert!(!is_blocking_challenge(false, "<html><body>nothing</body></html>"));
    }

    #[test]
    fn classify_prefers_results_over_a_stray_challenge_token() {
        // A real Bing SERP whose HTML also loads a reCAPTCHA asset must classify as
        // Results, not Challenge (checking the marker first would misfire).
        let serp = r#"<div id="b_results"><ol><li class="b_algo">
            <h2><a href="https://example.com/a">A</a></h2><div class="b_caption"><p>s</p></div>
          </li></ol></div><script src="https://www.google.com/recaptcha/api.js"></script>"#;
        assert!(matches!(
            classify_outcome(SearchEngine::BingCn, &Ok(serp.to_string())),
            Outcome::Results(n) if n >= 1
        ));
    }

    // ── manual-solve backstop (1b) ─────────────────────────────────────────

    #[test]
    fn should_prompt_manual_only_on_challenge_and_off_cooldown() {
        assert!(should_prompt_manual(true, false), "challenge + no cooldown → prompt");
        assert!(!should_prompt_manual(true, true), "cooldown active → don't nag");
        assert!(!should_prompt_manual(false, false), "not a challenge → no prompt");
        assert!(!should_prompt_manual(false, true));
    }

    #[test]
    fn manual_solve_policy_defaults_on_and_yields_to_bench_and_off_values() {
        // Default (env unset) → on: a challenge surfaces the window.
        assert!(manual_solve_policy(false, None));
        // Explicit off-ish values disable it.
        for v in ["0", "false", "off", "OFF", "False"] {
            assert!(!manual_solve_policy(false, Some(v)), "value {v:?} should disable");
        }
        // Any other value keeps it on.
        for v in ["1", "true", "on", "yes"] {
            assert!(manual_solve_policy(false, Some(v)), "value {v:?} should enable");
        }
        // A benchmark run forces it off regardless of the manual value (unattended —
        // must never pop a window mid-bench).
        assert!(!manual_solve_policy(true, None));
        assert!(!manual_solve_policy(true, Some("1")));
        assert!(!manual_solve_policy(true, Some("on")));
    }

    #[test]
    fn solve_strings_are_localized_and_fall_back_to_english() {
        for loc in ["zh", "ja", "en", "", "fr"] {
            let (title, hint) = solve_strings(loc);
            assert!(!title.is_empty(), "empty title for {loc:?}");
            assert!(!hint.is_empty(), "empty hint for {loc:?}");
        }
        // Distinct per supported locale.
        assert_ne!(solve_strings("zh").0, solve_strings("en").0);
        assert_ne!(solve_strings("ja").0, solve_strings("en").0);
        assert_ne!(solve_strings("zh").1, solve_strings("ja").1);
        // Unknown locale → English fallback.
        assert_eq!(solve_strings("fr"), solve_strings("en"));
    }

    #[test]
    fn solve_banner_js_json_encodes_the_instruction() {
        let js = solve_banner_js("Please verify");
        assert!(js.contains("\"Please verify\""), "instruction not embedded: {js}");
        assert!(js.contains("__mutsumi_hint"), "sentinel id missing");
        assert!(js.contains("position"), "banner not pinned");
        // Special chars are escaped by serde so they can't break out of the JS string.
        let js2 = solve_banner_js("a\"b");
        assert!(js2.contains(r#"a\"b"#), "quote not escaped: {js2}");
    }
}
