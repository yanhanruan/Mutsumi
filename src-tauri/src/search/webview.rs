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
//! * **Inline manual solve, never a retry** — [`is_challenge_page`] detects
//!   Cloudflare/CAPTCHA interstitials. A challenge is **never** re-requested
//!   (without the user clearing it, a second navigation returns the same
//!   interstitial); instead the window is surfaced immediately and the fetch —
//!   and the chat turn awaiting it — waits for the user to solve, so the solved
//!   SERP feeds that same reply. On timeout/failure the challenge page is
//!   returned as-is and chat proceeds without web context.
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
//! [`should_solve_inline`]) stay always-compiled so the URL/correlation/challenge
//! contract *is* unit-tested without a browser.

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

/// Minimum rendered body text (chars) before a result page counts as settled —
/// below this the probe keeps polling (skeleton/loading screens).
const PAGE_TEXT_MIN: usize = 80;

/// Probe script for a **result page** fetch (the "go deeper" step): returns the
/// rendered `outerHTML` once the document looks settled, else an empty string so
/// the Rust side keeps polling. Unlike the SERP path this runs via
/// `ICoreWebView2::ExecuteScript` — *pulled* from Rust, not emitted by the page —
/// so an arbitrary result page needs (and gets) **no** Tauri IPC surface; the
/// capability file stays scoped to the engines' own domains.
///
/// `window.__mutsumi_pre` is the previous-document marker: it is set on the SERP
/// document *before* navigating away, and a navigation wipes JS globals — so if
/// it is still present the probe is racing an uncommitted navigation and must
/// report "not ready" rather than hand back the stale SERP. Redirect-safe (the
/// marker dies on any navigation, wherever it lands).
fn page_probe_js() -> String {
    format!(
        r#"(function() {{
  if (window.__mutsumi_pre) return '';
  if (document.readyState === 'loading') return '';
  var t = (document.body && document.body.innerText) || '';
  return t.length >= {min} ? document.documentElement.outerHTML : '';
}})()"#,
        min = PAGE_TEXT_MIN,
    )
}

/// Decode an `ExecuteScript` completion payload: the executed JS value arrives
/// JSON-encoded, so a string result is a JSON string. Anything else — `null`
/// (threw / undefined), a number, malformed — decodes to empty, which callers
/// treat as "not ready / no HTML". Pure — unit-tested.
fn decode_exec_result(json: &str) -> String {
    serde_json::from_str::<String>(json).unwrap_or_default()
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
        Ok(html) => {
            let n = super::parse_rendered(engine, html).len();
            // Results win (a parseable SERP with a stray token is a SERP) — except
            // where [`is_blocking_challenge`] overrides it (Baidu's parseable
            // captcha page).
            if is_blocking_challenge(engine, n > 0, html) {
                Outcome::Challenge
            } else if n > 0 {
                Outcome::Results(n)
            } else {
                Outcome::Empty
            }
        }
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

/// Whether `html` is a challenge that should **block** — surface the solve popup,
/// retry, and never feed the page to the model.
///
/// For most engines a challenge only blocks when the page *also* produced no
/// results: a real SERP that merely carries a stray challenge token (Google
/// embeds reCAPTCHA assets on ordinary results pages) must serve its results.
///
/// **Baidu is the exception.** Its 百度安全验证 slider-captcha page is itself
/// parseable — our Baidu parser scrapes a stray "result" (the 意见反馈 / 刷新
/// link) off it, so `has_results` is true and results-win would let the captcha
/// junk through as a real result. For Baidu, any challenge page blocks regardless
/// of a spurious parse. Pure — unit-tested.
pub fn is_blocking_challenge(engine: SearchEngine, has_results: bool, html: &str) -> bool {
    if !is_challenge_page(html) {
        return false;
    }
    match engine {
        SearchEngine::Baidu => true,
        _ => !has_results,
    }
}

/// What to do when a fetch lands on a blocking challenge: surface the window and
/// wait for the user, **never** re-request — without the user clearing it, a
/// second navigation returns the exact same interstitial (a lesson from the
/// removed retry: it only added latency). When manual solve is disabled
/// (unattended benchmark, `MUTSUMI_SERP_MANUAL=0`), the challenge page is
/// returned as-is. Pure — unit-tested.
pub fn should_solve_inline(blocking_challenge: bool, manual_enabled: bool) -> bool {
    blocking_challenge && manual_enabled
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

/// Whether the **manual-solve backstop** is enabled. Default on: a challenge
/// surfaces the window immediately and the search (and the chat turn awaiting
/// it) waits for the user to clear it, so the solved SERP answers that same
/// message. Disable with `MUTSUMI_SERP_MANUAL=0` (or `false`/`off`). Suppressed
/// automatically during an unattended benchmark run (`MUTSUMI_SERP_BENCH` set).
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

/// Test stub — see [`fetch_serp`]'s stub note.
#[cfg(test)]
pub async fn fetch_page(_app: &tauri::AppHandle, _url: &str) -> Result<String, String> {
    Err("webview page fetch is disabled under cfg(test)".into())
}

#[cfg(not(test))]
pub use real::{fetch_page, fetch_serp};

#[cfg(not(test))]
mod real {
    use std::sync::atomic::Ordering;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use tauri::{AppHandle, Listener, Manager, WebviewUrl, WebviewWindowBuilder};
    use tokio::sync::oneshot;

    use super::{
        decode_exec_result, homepage_url, init_script, is_blocking_challenge,
        manual_solve_enabled, page_probe_js, resolve_pending_key, serp_url,
        should_solve_inline, solve_banner_js, solve_strings, WebviewSerp, SERP_EVENT,
    };
    use crate::search::SearchEngine;

    /// Fixed label of the singleton hidden fetch window.
    const SERP_WINDOW: &str = "serp-fetcher";
    /// Hard cap on one navigate→render→extract round-trip. Sized to absorb the
    /// cold-start cost of creating the window on the first query; warm
    /// navigations finish far under it.
    const WEBVIEW_TIMEOUT: Duration = Duration::from_secs(7);
    /// How long to let the homepage warm-up navigation settle its cookies before
    /// the first search navigates away.
    const WARMUP_SETTLE: Duration = Duration::from_millis(1500);
    /// How long to wait for the user to clear a surfaced challenge.
    const MANUAL_TIMEOUT: Duration = Duration::from_secs(150);
    /// Overall budget for one result-page fetch (navigate → settle → grab).
    const PAGE_TIMEOUT: Duration = Duration::from_secs(6);
    /// First probe delay after the navigation is scheduled (lets it commit).
    const PAGE_INITIAL_WAIT: Duration = Duration::from_millis(800);
    /// Interval between settle probes.
    const PAGE_POLL: Duration = Duration::from_millis(400);
    /// Cap on one ExecuteScript round-trip (the COM callback is normally fast;
    /// this only guards a wedged renderer).
    const EXEC_TIMEOUT: Duration = Duration::from_secs(4);

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

    /// Cancellation-safe cleanup for a surfaced solve window: hides the window
    /// and drops the pending entry **even if the awaiting future is cancelled**
    /// (e.g. the chat turn's outer budget fires mid-solve). A cancelled solve
    /// must never orphan a visible window — the bug that originally forced the
    /// solve to run detached.
    struct SolveCleanup<'a> {
        app: AppHandle,
        state: &'a WebviewSerp,
        id: String,
    }

    impl Drop for SolveCleanup<'_> {
        fn drop(&mut self) {
            self.state.pending.lock().unwrap().remove(&self.id);
            hide_window(&self.app);
        }
    }

    /// The window is on a challenge page — surface it immediately so the user
    /// clears it in place, and **wait** for the solved SERP to report back (its
    /// `continue=`-preserved id, or the single-pending fallback, routes the emit
    /// here). Returns the solved HTML, or `Err` if the user didn't solve in
    /// time. Assumes the caller holds the nav lock.
    async fn manual_solve(
        app: &AppHandle,
        state: &WebviewSerp,
        engine: SearchEngine,
    ) -> Result<String, String> {
        let id = uid();
        let (tx, rx) = oneshot::channel::<String>();
        state.pending.lock().unwrap().insert(id.clone(), tx);
        let _cleanup = SolveCleanup { app: app.clone(), state, id };

        // Localize the window chrome from the frontend-reported locale (the SERP
        // page is remote, so it can't use the Vue i18n system).
        let locale = app
            .try_state::<crate::app_state::LocaleState>()
            .map(|s| s.get())
            .unwrap_or_default();
        let (title, hint) = solve_strings(&locale);

        log::info!(
            "search: {engine:?} challenged — surfacing window, waiting for the user (≤{}s)",
            MANUAL_TIMEOUT.as_secs()
        );
        surface_for_solve(app, title, &solve_banner_js(hint));
        match tokio::time::timeout(MANUAL_TIMEOUT, rx).await {
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

    /// Run JS in the fetch window's document and return its (JSON-encoded)
    /// result — via `ICoreWebView2::ExecuteScript`, i.e. **pulled from the Rust
    /// side**. This is how result-page fetches read HTML back: unlike the SERP
    /// init-script's `plugin:event|emit`, it needs no Tauri IPC in the page, so
    /// arbitrary result pages get zero app surface and the capability file stays
    /// scoped to the search engines' domains. Windows-only (WebView2), like the
    /// rest of this app's platform integrations.
    async fn exec_script(app: &AppHandle, js: &str) -> Result<String, String> {
        use webview2_com::ExecuteScriptCompletedHandler;
        use windows::core::HSTRING;

        let window = app
            .get_webview_window(SERP_WINDOW)
            .ok_or_else(|| "no fetch window".to_string())?;
        let (tx, rx) = oneshot::channel::<Result<String, String>>();
        let js = js.to_string();
        window
            .with_webview(move |webview| {
                // Runs on the main (UI) thread — required for WebView2 COM calls.
                let core = match unsafe { webview.controller().CoreWebView2() } {
                    Ok(core) => core,
                    Err(e) => {
                        let _ = tx.send(Err(format!("CoreWebView2: {e}")));
                        return;
                    }
                };
                let handler = ExecuteScriptCompletedHandler::create(Box::new(
                    move |hr: windows::core::Result<()>, json: String| {
                        let _ = tx.send(hr.map(|()| json).map_err(|e| format!("ExecuteScript: {e}")));
                        Ok(())
                    },
                ));
                if let Err(e) = unsafe { core.ExecuteScript(&HSTRING::from(js.as_str()), &handler) } {
                    // The handler (owning tx) drops here → rx sees "dropped".
                    log::warn!("search: ExecuteScript submit failed: {e}");
                }
            })
            .map_err(|e| format!("with_webview: {e}"))?;
        match tokio::time::timeout(EXEC_TIMEOUT, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err("exec channel dropped".into()),
            Err(_) => Err("script execution timed out".into()),
        }
    }

    /// Fetch a **result page**'s rendered HTML through the same hidden window —
    /// the "go deeper" step when a SERP snippet doesn't carry the answer.
    ///
    /// Navigate → poll [`page_probe_js`] until the document settles (readyState,
    /// minimum body text, and the previous-document marker cleared) → return the
    /// `outerHTML`. On deadline, grabs whatever is rendered — the extractor
    /// ([`crate::search::page`]) fails open on junk. Never pops a solve window:
    /// a challenge on a content site is simply returned and extracts to nothing.
    pub async fn fetch_page(app: &AppHandle, url: &str) -> Result<String, String> {
        let state = app.state::<WebviewSerp>();
        let parsed: tauri::Url = url.parse().map_err(|e| format!("bad url {url}: {e}"))?;

        // Same serialization as SERP fetches (shared window, warm jar).
        let _guard = state.nav.lock().await;

        // Mark the current (SERP) document so the probe can tell it from the
        // page we're about to load. Best-effort: if there is no window yet
        // there is no stale document to confuse the probe either.
        let _ = exec_script(app, "window.__mutsumi_pre = 1; ''").await;

        let app2 = app.clone();
        let (setup_tx, setup_rx) = oneshot::channel::<Result<(), String>>();
        app.run_on_main_thread(move || {
            let _ = setup_tx.send(build_or_navigate(&app2, parsed));
        })
        .map_err(|e| format!("run_on_main_thread: {e}"))?;
        match setup_rx.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err("setup channel dropped".into()),
        }

        tokio::time::sleep(PAGE_INITIAL_WAIT).await;
        let deadline = tokio::time::Instant::now() + PAGE_TIMEOUT;
        let probe = page_probe_js();
        loop {
            match exec_script(app, &probe).await {
                Ok(json) => {
                    let html = decode_exec_result(&json);
                    if !html.is_empty() {
                        return Ok(html);
                    }
                }
                Err(e) => log::info!("search: page probe failed ({e})"),
            }
            if tokio::time::Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(PAGE_POLL).await;
        }
        // Deadline: take what rendered — slow pages often still carry the fact.
        let json = exec_script(app, "document.documentElement.outerHTML").await?;
        let html = decode_exec_result(&json);
        if html.is_empty() {
            Err("page produced no HTML".into())
        } else {
            Ok(html)
        }
    }

    /// Drive a SERP fetch through the hidden WebView; returns the rendered HTML.
    ///
    /// On a **blocking challenge** the window is surfaced immediately and this
    /// call waits for the user to clear it (≤[`MANUAL_TIMEOUT`]) — no retry is
    /// ever attempted, because without the user solving, a re-request returns
    /// the exact same interstitial. The chat turn awaiting this fetch waits with
    /// it, so a successful solve feeds the solved SERP into that same reply; on
    /// timeout/failure the challenge HTML is returned as-is (parses to no
    /// context, chat proceeds without web data). Best-effort — every error path
    /// is recoverable by the caller degrading to no web context.
    pub async fn fetch_serp(
        app: &AppHandle,
        engine: SearchEngine,
        query: &str,
    ) -> Result<String, String> {
        let state = app.state::<WebviewSerp>();
        ensure_listener(app, &state);

        // One navigation at a time through the shared window (also keeps a
        // concurrent search from navigating away mid-solve).
        let _guard = state.nav.lock().await;

        // Warm this engine's cookie jar once per session (reduces bot challenges).
        if !state.warmed.lock().unwrap().contains(&engine) {
            warm_up(app, engine).await;
            state.warmed.lock().unwrap().insert(engine);
        }

        let html = navigate_once(app, &state, engine, query).await?;
        let has_results = !crate::search::parse_rendered(engine, &html).is_empty();
        let blocked = is_blocking_challenge(engine, has_results, &html);
        if !should_solve_inline(blocked, manual_solve_enabled()) {
            return Ok(html);
        }
        match manual_solve(app, &state, engine).await {
            Ok(solved) => Ok(solved),
            Err(e) => {
                log::info!("search: {engine:?} challenge not cleared ({e}) — no web context");
                Ok(html)
            }
        }
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

    // ── result-page fetch helpers (the "go deeper" step) ───────────────────

    #[test]
    fn page_probe_waits_for_settle_and_guards_the_stale_document() {
        let js = page_probe_js();
        // Not-ready arms all return '' so the Rust side keeps polling.
        assert!(js.contains("window.__mutsumi_pre"), "stale-document guard missing");
        assert!(js.contains("readyState"), "load-state check missing");
        assert!(js.contains(&format!(">= {PAGE_TEXT_MIN}")), "text threshold missing");
        assert!(js.contains("document.documentElement.outerHTML"), "no HTML grab");
        // Pulled via ExecuteScript — the page needs no Tauri IPC surface.
        assert!(!js.contains("__TAURI"), "probe must not touch Tauri IPC");
        assert!(!js.contains("invoke"), "probe must not emit");
    }

    #[test]
    fn decode_exec_result_unwraps_strings_and_maps_everything_else_to_empty() {
        // ExecuteScript returns the JS value JSON-encoded.
        assert_eq!(decode_exec_result(r#""<html>x</html>""#), "<html>x</html>");
        assert_eq!(decode_exec_result(r#""a\nb\"c""#), "a\nb\"c");
        // undefined / thrown → "null"; numbers / junk → not HTML.
        assert_eq!(decode_exec_result("null"), "");
        assert_eq!(decode_exec_result("123"), "");
        assert_eq!(decode_exec_result("not json"), "");
        assert_eq!(decode_exec_result(""), "");
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
    fn challenge_pops_solve_immediately_and_never_when_unattended() {
        // A blocking challenge with manual solve on → surface + wait, right away.
        assert!(should_solve_inline(true, true));
        // Unattended (bench / MUTSUMI_SERP_MANUAL=0) → return the page as-is;
        // and there is NO retry path at all — re-requesting an unsolved
        // challenge just returns the same interstitial.
        assert!(!should_solve_inline(true, false));
        assert!(!should_solve_inline(false, true), "no challenge → nothing to solve");
        assert!(!should_solve_inline(false, false));
    }

    #[test]
    fn is_blocking_challenge_results_win_except_baidu() {
        let cf = r#"<title>Just a moment...</title><div class="cf-browser-verification"></div>"#;
        // Non-Baidu: markers but results WERE parsed → not a block (serve the SERP;
        // the Google reCAPTCHA-asset case).
        assert!(!is_blocking_challenge(SearchEngine::Google, true, cf));
        // Non-Baidu: markers and no results → a genuine block.
        assert!(is_blocking_challenge(SearchEngine::Google, false, cf));
        // No markers → never a block.
        assert!(!is_blocking_challenge(SearchEngine::Google, false, "<html>ok</html>"));
        assert!(!is_blocking_challenge(SearchEngine::Baidu, false, "<html>ok</html>"));

        // Baidu: its captcha page is parseable, so a stray result must NOT mask the
        // challenge — Baidu blocks whenever the challenge chrome is present.
        let baidu_cap = "<html><body>百度安全验证 请完成下方验证后继续操作</body></html>";
        assert!(is_blocking_challenge(SearchEngine::Baidu, true, baidu_cap));
        assert!(is_blocking_challenge(SearchEngine::Baidu, false, baidu_cap));
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

    #[test]
    fn classify_baidu_captcha_as_challenge_despite_a_stray_result() {
        // Baidu's 百度安全验证 page parses to a spurious result (the parser scrapes
        // a link off it); it must still classify as a challenge, not Results.
        let html = r#"<div id="content_left"><div class="result c-container">
            <h3><a href="http://www.baidu.com/link?url=x">意见反馈</a></h3>
            <div class="c-abstract">刷新</div></div></div>
            <div>百度安全验证 请完成下方验证后继续操作</div>"#;
        // Sanity: it really does parse to a (spurious) result.
        assert!(!crate::search::parse_rendered(SearchEngine::Baidu, html).is_empty());
        assert_eq!(
            classify_outcome(SearchEngine::Baidu, &Ok(html.to_string())),
            Outcome::Challenge
        );
    }

    // ── manual-solve backstop (1b) ─────────────────────────────────────────

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
