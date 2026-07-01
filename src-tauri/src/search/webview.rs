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
/// request id in the fragment (`#__serpid=<id>`). The fragment is never sent to
/// the server and is ignored by the SERP page, but the init-script reads it back
/// so Rust can correlate the emitted HTML with this exact request.
fn serp_url(engine: SearchEngine, query: &str, id: &str) -> String {
    let (base, param) = engines::endpoint(engine);
    format!("{base}?{param}={}#{ID_KEY}={id}", enc(query))
}

/// Init-script injected at document-start (WebView2 runs embedder scripts before
/// page scripts and outside the page CSP). Top frame only; reads its request id
/// from the URL fragment *before* the page can rewrite it, then polls for a
/// known results container (up to [`READY_MAX_MS`]) and emits the rendered
/// `outerHTML` via the core event plugin (works without `withGlobalTauri` — uses
/// `__TAURI_INTERNALS__`).
fn init_script() -> String {
    format!(
        r#"(function() {{
  if (window.top !== window.self) return;          // ignore ad/iframe sub-frames
  var m = (location.hash || '').match(/{key}=([^&]+)/);
  if (!m) return;                                  // not one of our navigations
  var ID = decodeURIComponent(m[1]);
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
      window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {{ event: {event:?}, payload: {{ id: ID, html: html }} }});
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
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use tauri::{AppHandle, Listener, Manager, WebviewUrl, WebviewWindowBuilder};
    use tokio::sync::oneshot;

    use super::{
        init_script, serp_url, should_retry, WebviewSerp, MAX_ATTEMPTS, SERP_EVENT,
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
                if let Some(tx) =
                    app_l.state::<WebviewSerp>().pending.lock().unwrap().remove(&msg.id)
                {
                    let _ = tx.send(msg.html);
                }
            }
        });
    }

    /// Create the hidden window. Must run on the main (UI) thread.
    fn build_window(app: &AppHandle, url: tauri::Url) -> Result<(), String> {
        let visible = std::env::var("MUTSUMI_SERP_VISIBLE").is_ok(); // debugging aid
        let built = WebviewWindowBuilder::new(app, SERP_WINDOW, WebviewUrl::External(url))
            .title("search")
            .visible(visible)
            .skip_taskbar(true)
            .inner_size(1000.0, 800.0)
            .initialization_script(init_script().as_str())
            .build()
            .map_err(|e| format!("build: {e}"))?;
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

        let mut last = navigate_once(app, &state, engine, query).await?;
        for attempt in 1..MAX_ATTEMPTS {
            if !should_retry(attempt, &last, MAX_ATTEMPTS) {
                break;
            }
            log::info!("search: webview {engine:?} hit a challenge page, retrying once");
            tokio::time::sleep(RETRY_BACKOFF).await;
            last = navigate_once(app, &state, engine, query).await?;
        }
        Ok(last)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    /// Mirror of the init-script's id regex, so the test proves `serp_url`
    /// produces a fragment the browser-side script can read back.
    fn extract_serpid(url: &str) -> Option<String> {
        let hash = url.split_once('#').map(|(_, h)| h)?;
        let re = Regex::new(&format!(r"{ID_KEY}=([^&]+)")).unwrap();
        re.captures(hash).map(|c| c[1].to_string())
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
    fn serp_url_id_round_trips_through_the_fragment() {
        // The whole correlation scheme: the id we embed must be recoverable by
        // the init-script's `/__serpid=([^&]+)/` regex.
        for id in ["1", "1700000000000000001", "42"] {
            let u = serp_url(SearchEngine::Bing, "any query", id);
            assert_eq!(extract_serpid(&u).as_deref(), Some(id), "url was {u}");
        }
    }

    #[test]
    fn serp_url_fragment_is_not_part_of_the_query_string() {
        let u = serp_url(SearchEngine::Google, "x", "7");
        let (before, after) = u.split_once('#').expect("has a fragment");
        assert!(!before.contains('#'));
        assert!(after.starts_with(&format!("{ID_KEY}=")));
    }

    #[test]
    fn init_script_carries_the_event_name_key_and_readiness() {
        let js = init_script();
        assert!(js.contains("serp-html"), "event name missing");
        assert!(js.contains("__serpid"), "fragment key missing");
        assert!(js.contains("4000"), "readiness cap missing");
        assert!(js.contains("#b_results"), "readiness selector missing");
        // Must report through the permissioned event plugin, top-frame only.
        assert!(js.contains("plugin:event|emit"));
        assert!(js.contains("window.top !== window.self"));
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
}
