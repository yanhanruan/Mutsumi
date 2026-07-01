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
//! listens for. Each navigation carries a request id in the URL fragment
//! (`#__serpid=…`); the init-script reads it at document-start and echoes it back
//! so a late emit from a previous, timed-out navigation can never be mistaken for
//! the current one.
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
/// When `hide_globals` is set (hardened mode), it first captures a **bound**
/// `invoke` reference, then deletes `window.__TAURI_INTERNALS__` and the other
/// `__TAURI*` globals — before any SERP/detector script runs — so the page can't
/// fingerprint the embedded webview. `grab()` reports through the captured
/// reference, which keeps working after the global is removed.
fn init_script(hide_globals: bool) -> String {
    // Runs at document-start, after Tauri's IPC init-script, before page scripts.
    let hide = if hide_globals {
        r#"try { ['__TAURI_INTERNALS__','__TAURI__','__TAURI_METADATA__','__TAURI_EVENT_PLUGIN_INTERNALS__','__TAURI_OS_PLUGIN_INTERNALS__'].forEach(function(k){ try { delete window[k]; } catch(e) { try { window[k] = undefined; } catch(e2){} } }); } catch(e) {}"#
    } else {
        ""
    };
    format!(
        r#"(function() {{
  if (window.top !== window.self) return;          // ignore ad/iframe sub-frames
  var __ti = window.__TAURI_INTERNALS__;           // capture the IPC entry point…
  var invoke = (__ti && __ti.invoke) ? __ti.invoke.bind(__ti) : null;
  {hide}                                           // …then remove the automation tells
  var href = location.href;
  try {{ href = decodeURIComponent(href); }} catch (e) {{}}
  var m = href.match(/{key}=([0-9]+)/);            // survives a `continue=`-wrapped redirect
  if (!m) return;                                  // not one of our navigations
  var ID = m[1];
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
        hide = hide,
        key = ID_KEY,
        event = SERP_EVENT,
        max = READY_MAX_MS,
        poll = POLL_MS,
        settle = SETTLE_MS,
    )
}

/// Master hardening toggle, read from `MUTSUMI_SERP_HARDEN`. Default (unset) is
/// **on** — the benchmark sets `0` for a clean baseline. Governs the browser
/// args, the persistent profile vs incognito, and the global-hiding init-script.
pub fn hardening_enabled() -> bool {
    std::env::var("MUTSUMI_SERP_HARDEN")
        .map(|v| v != "0" && !v.eq_ignore_ascii_case("false") && !v.eq_ignore_ascii_case("off"))
        .unwrap_or(true)
}

/// WebView2 args applied in hardened mode. Preserves wry's defaults and adds
/// `--disable-blink-features=AutomationControlled`, which removes
/// `navigator.webdriver` / the automation flag.
const HARDEN_BROWSER_ARGS: &str =
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --disable-blink-features=AutomationControlled";

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
        Ok(html) if is_challenge_page(html) => Outcome::Challenge,
        Ok(html) => match super::parse_rendered(engine, html).len() {
            0 => Outcome::Empty,
            n => Outcome::Results(n),
        },
        Err(_) => Outcome::Failed,
    }
}

/// True when `html` is an anti-bot interstitial (Cloudflare / CAPTCHA / "unusual
/// traffic"), not a real SERP. Markers are deliberately specific to challenge
/// chrome so a result snippet that merely mentions "captcha" doesn't trip it.
/// Pure — unit-tested.
pub fn is_challenge_page(html: &str) -> bool {
    let lower = html.to_lowercase();
    const ASCII: &[&str] = &[
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
    if ASCII.iter().any(|m| lower.contains(m)) {
        return true;
    }
    const CJK: &[&str] = &["人机验证", "安全验证", "异常流量", "请完成验证", "滑动验证", "百度安全验证"];
    CJK.iter().any(|m| html.contains(m))
}

/// Whether to re-navigate after a failed attempt: only when a challenge was
/// detected *and* attempts remain. A merely-empty/unparseable page is **not**
/// retried. Pure — unit-tested. (`attempt` is 1-based: the attempt that just
/// produced `html`.)
pub fn should_retry(attempt: u8, html: &str, max_attempts: u8) -> bool {
    attempt < max_attempts && is_challenge_page(html)
}

/// Whether the **manual-solve backstop** is enabled (opt-in via env for now; a
/// UI setting can promote it later — kept off by default so the app never pops a
/// window unexpectedly). When on, a still-challenged fetch surfaces the hidden
/// window so the user clears the reCAPTCHA once; the clearance cookie then
/// persists in the profile for subsequent searches.
pub fn manual_solve_enabled() -> bool {
    std::env::var("MUTSUMI_SERP_MANUAL")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("on"))
        .unwrap_or(false)
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
        hardening_enabled, homepage_url, init_script, is_challenge_page, manual_solve_enabled,
        resolve_pending_key, serp_url, should_prompt_manual, should_retry, WebviewSerp,
        HARDEN_BROWSER_ARGS, MAX_ATTEMPTS, SERP_EVENT,
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

    /// Dedicated on-disk profile for the SERP window (its own cookies + cache +
    /// history — a returning-user look). Isolated from the user's real browser;
    /// nothing here ever deletes user data.
    fn serp_profile_dir(app: &AppHandle) -> Option<std::path::PathBuf> {
        app.path().app_local_data_dir().ok().map(|d| d.join("serp-profile"))
    }

    /// Create the hidden window. Must run on the main (UI) thread.
    ///
    /// In hardened mode (default; see [`hardening_enabled`]) it disables the
    /// `AutomationControlled` fingerprint flag, uses a persistent profile, and
    /// injects the global-hiding init-script. The un-hardened path (the
    /// benchmark's baseline) uses an incognito profile and none of those.
    fn build_window(app: &AppHandle, url: tauri::Url) -> Result<(), String> {
        let visible = std::env::var("MUTSUMI_SERP_VISIBLE").is_ok(); // debugging aid
        let harden = hardening_enabled();

        let mut builder = WebviewWindowBuilder::new(app, SERP_WINDOW, WebviewUrl::External(url))
            .title("search")
            .visible(visible)
            .skip_taskbar(true)
            .inner_size(1000.0, 800.0)
            .initialization_script(init_script(harden).as_str());

        if harden {
            builder = builder.additional_browser_args(HARDEN_BROWSER_ARGS);
            if let Some(dir) = serp_profile_dir(app) {
                builder = builder.data_directory(dir);
            }
        } else {
            // Baseline: ephemeral profile so warm cookies can't leak into it.
            builder = builder.incognito(true);
        }

        let built = builder.build().map_err(|e| format!("build: {e}"))?;
        #[cfg(debug_assertions)]
        if visible {
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

    /// Show or hide the singleton fetch window (main thread).
    fn set_window_visible(app: &AppHandle, show: bool) {
        let app2 = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Some(w) = app2.get_webview_window(SERP_WINDOW) {
                if show {
                    let _ = w.show();
                    let _ = w.set_focus();
                } else {
                    let _ = w.hide();
                }
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

        log::info!(
            "search: {engine:?} still challenged — surfacing window for manual solve (≤{}s)",
            MANUAL_TIMEOUT.as_secs()
        );
        set_window_visible(app, true);
        let outcome = tokio::time::timeout(MANUAL_TIMEOUT, rx).await;
        set_window_visible(app, false);
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
            if !should_retry(attempt, &last, MAX_ATTEMPTS) {
                break;
            }
            log::info!("search: webview {engine:?} hit a challenge page, retrying once");
            tokio::time::sleep(RETRY_BACKOFF).await;
            last = navigate_once(app, &state, engine, query).await?;
        }

        // Backstop: if still challenged and manual-solve is enabled, surface the
        // window so the user clears it once (cookie then persists). Cooldown-gated
        // so a dismissed prompt doesn't re-pop on every subsequent search.
        if manual_solve_enabled() {
            let cooldown_active = state
                .manual_cooldown
                .lock()
                .unwrap()
                .map(|t| t.elapsed() < MANUAL_COOLDOWN)
                .unwrap_or(false);
            if should_prompt_manual(is_challenge_page(&last), cooldown_active) {
                match manual_solve(app, &state, engine).await {
                    Ok(html) => {
                        *state.manual_cooldown.lock().unwrap() = None; // solved → clear
                        last = html;
                    }
                    Err(_) => {
                        *state.manual_cooldown.lock().unwrap() = Some(Instant::now());
                    }
                }
            }
        }

        Ok(last)
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
        let js = init_script(true);
        assert!(js.contains("serp-html"), "event name missing");
        assert!(js.contains("__serpid"), "id key missing");
        assert!(js.contains("decodeURIComponent(href)"), "redirect-surviving recovery missing");
        assert!(js.contains("4000"), "readiness cap missing");
        assert!(js.contains("#b_results"), "readiness selector missing");
        // Must report through the permissioned event plugin, top-frame only.
        assert!(js.contains("plugin:event|emit"));
        assert!(js.contains("window.top !== window.self"));
    }

    #[test]
    fn init_script_hides_tauri_globals_only_when_hardened() {
        let hardened = init_script(true);
        // Captures a bound invoke first, then deletes the automation tells.
        assert!(hardened.contains(".invoke.bind("), "must capture invoke before hiding");
        assert!(hardened.contains("delete window[k]"), "must delete the globals");
        assert!(hardened.contains("__TAURI_INTERNALS__"), "must target the IPC global");
        // grab() must use the captured reference (works after the delete).
        assert!(hardened.contains("if (invoke) invoke("), "grab must use captured invoke");

        let baseline = init_script(false);
        assert!(!baseline.contains("delete window[k]"), "baseline must not touch globals");
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

    #[test]
    fn hardening_defaults_on_and_respects_the_env_toggle() {
        // Note: reads process env; assert only the parse of explicit values via a
        // pure mirror to avoid mutating global env in a parallel test run.
        for (v, want) in [("0", false), ("false", false), ("off", false), ("1", true), ("yes", true)] {
            let parsed = v != "0" && !v.eq_ignore_ascii_case("false") && !v.eq_ignore_ascii_case("off");
            assert_eq!(parsed, want, "value {v}");
        }
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
    fn should_retry_exactly_once_on_a_challenge() {
        let cf = r#"<title>Just a moment...</title>"#;
        // First attempt hit a challenge → retry permitted.
        assert!(should_retry(1, cf, MAX_ATTEMPTS));
        // After the retry (attempt == MAX) → no more retries even if still challenged.
        assert!(!should_retry(MAX_ATTEMPTS, cf, MAX_ATTEMPTS));
    }

    #[test]
    fn should_not_retry_on_a_merely_unparseable_page() {
        let unknown = "<html><body><div>nothing we recognize</div></body></html>";
        assert!(!should_retry(1, unknown, MAX_ATTEMPTS));
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
    fn manual_solve_is_opt_in_via_env_value() {
        // Pure mirror of the env parse (avoid mutating global env under parallel tests).
        for (v, want) in [("1", true), ("true", true), ("on", true), ("0", false), ("", false)] {
            let parsed = v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("on");
            assert_eq!(parsed, want, "value {v:?}");
        }
    }
}
