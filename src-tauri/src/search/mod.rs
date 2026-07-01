//! Web-search subsystem (Pipeline: search-enhanced awareness).
//!
//! Keyword-triggered ([`trigger`]) live search with a **single** network path:
//! a hidden, reused WebView window ([`webview`]) renders the real SERP — running
//! JS and clearing anti-bot challenges with the browser's genuine TLS/JA3
//! fingerprint and warm cookie jar — and we parse the rendered HTML. There is no
//! `reqwest` fallback and no engine fallback: the static HTTP client only ever
//! received blank / HTTP-202 / challenge pages from modern SERPs, so the WebView
//! is strictly more capable. Any failure degrades to "no web context"; search
//! never breaks chat.
//!
//!   needs_search? → webview::fetch_serp (primary engine, challenge-gated retry)
//!   → parse_rendered (regex → CSS fallback) → format as a labeled
//!   real-time-context block for injection into the chat prompt.

pub mod trigger;
pub mod webview;

mod engines;
mod parsers;
mod tracking;

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use engines::RawResult;

/// Max SERP results carried forward into the prompt.
const MAX_RESULTS: usize = 3;

/// The user-selectable search engine.
///
/// Default is **Bing CN**: it renders fully in the WebView and is reliably
/// reachable from a mainland network (Google often isn't). All engines remain
/// selectable from Settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SearchEngine {
    #[default]
    BingCn,
    Bing,
    Google,
    Baidu,
    DuckDuckGo,
}

impl SearchEngine {
    /// Parse a persisted setting value (kebab-case) into an engine.
    pub fn from_str(s: &str) -> SearchEngine {
        match s.to_lowercase().replace(['_', ' '], "-").as_str() {
            "bing-cn" => SearchEngine::BingCn,
            "bing" => SearchEngine::Bing,
            "google" => SearchEngine::Google,
            "baidu" => SearchEngine::Baidu,
            "duckduckgo" | "ddg" => SearchEngine::DuckDuckGo,
            _ => SearchEngine::default(),
        }
    }
}

/// A finished search hit (title + URL + snippet from the rendered SERP).
#[derive(Debug, Clone, PartialEq)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Managed state: the currently-selected engine + whether search is enabled at
/// all (user toggle in Settings). The WebView fetch window is managed
/// separately as [`webview::WebviewSerp`].
pub struct SearchState {
    engine: Mutex<SearchEngine>,
    /// Master on/off for web search. When off, no SERP fetch happens regardless
    /// of the keyword trigger — chat replies purely from the model + memory.
    enabled: std::sync::atomic::AtomicBool,
}

impl SearchState {
    pub fn new(engine: SearchEngine) -> Self {
        Self {
            engine: Mutex::new(engine),
            enabled: std::sync::atomic::AtomicBool::new(true), // on by default
        }
    }

    pub fn engine(&self) -> SearchEngine {
        *self.engine.lock().expect("search engine mutex")
    }

    pub fn set_engine(&self, engine: SearchEngine) {
        *self.engine.lock().expect("search engine mutex") = engine;
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Tauri command: switch the active search engine at runtime (settings UI).
/// `engine` is the kebab-case key from the frontend config.
#[tauri::command]
pub fn set_search_engine(engine: String, search: tauri::State<'_, SearchState>) {
    search.set_engine(SearchEngine::from_str(&engine));
}

/// Tauri command: enable/disable web search entirely (settings UI).
#[tauri::command]
pub fn set_search_enabled(enabled: bool, search: tauri::State<'_, SearchState>) {
    search.set_enabled(enabled);
}

/// Run a search and return up to [`MAX_RESULTS`] hits. Best-effort: returns
/// empty on any failure (fetch error, challenge page, unrecognized layout)
/// rather than erroring, so chat is never blocked by search problems.
pub async fn search(app: &AppHandle, engine: SearchEngine, query: &str) -> Vec<SearchResult> {
    let html = match webview::fetch_serp(app, engine, query).await {
        Ok(html) => html,
        Err(e) => {
            log::info!("search: webview {engine:?} fetch failed ({e})");
            return Vec::new();
        }
    };

    let raw = parse_rendered(engine, &html);
    if raw.is_empty() {
        log::info!("search: {engine:?} returned no parseable results");
        return Vec::new();
    }
    log::info!("search: webview {engine:?} → {} result(s)", raw.len());

    raw.into_iter()
        .take(MAX_RESULTS)
        .map(|r| SearchResult {
            title: r.title,
            url: r.url,
            snippet: r.snippet,
        })
        .collect()
}

/// Parse a rendered SERP document: the regex pass first (tolerant of class-name
/// churn on layout landmarks), then the CSS-selector pass as a fallback. Pure
/// and synchronous; an unrecognized / challenge page yields an empty vec.
pub(crate) fn parse_rendered(engine: SearchEngine, html: &str) -> Vec<RawResult> {
    let mut results = parsers::parse_serp_regex(engine, html, MAX_RESULTS * 2);
    if results.is_empty() {
        results = engines::parse_serp(engine, html);
    }
    results
}

/// Format results as the labeled real-time-context block injected into chat.
/// The preamble nudges Mutsumi (in character) to actually use relevant results
/// instead of reflexively answering "不知道", while keeping her terse tone.
pub fn format_context(query: &str, results: &[SearchResult]) -> String {
    let mut s = String::from(
        "以下是检索到的实时数据。若与用户的问题相关，请据此作答（保持你一贯简短、克制的语气即可），\
         不要明明有结果却回一句「不知道」；只有确实无关时才忽略。\n\n",
    );
    s.push_str(&format!("【搜索：{query}】\n"));
    for (i, r) in results.iter().enumerate() {
        s.push_str(&format!("[{}] {}\n{}\n", i + 1, r.title, r.url));
        if !r.snippet.is_empty() {
            s.push_str(&format!("{}\n", r.snippet));
        }
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_from_str() {
        assert_eq!(SearchEngine::from_str("bing-cn"), SearchEngine::BingCn);
        assert_eq!(SearchEngine::from_str("bing_cn"), SearchEngine::BingCn);
        assert_eq!(SearchEngine::from_str("google"), SearchEngine::Google);
        assert_eq!(SearchEngine::from_str("DuckDuckGo"), SearchEngine::DuckDuckGo);
        assert_eq!(SearchEngine::from_str("ddg"), SearchEngine::DuckDuckGo);
        // Unknown → the app default (Bing CN).
        assert_eq!(SearchEngine::from_str("anything"), SearchEngine::BingCn);
        assert_eq!(SearchEngine::default(), SearchEngine::BingCn);
    }

    #[test]
    fn format_context_has_label_and_entries() {
        let results = vec![SearchResult {
            title: "Tokyo Weather".into(),
            url: "https://example.com".into(),
            snippet: "Sunny, 20C".into(),
        }];
        let ctx = format_context("东京天气", &results);
        assert!(ctx.contains("以下是检索到的实时数据"));
        assert!(ctx.contains("【搜索：东京天气】"));
        assert!(ctx.contains("Tokyo Weather"));
        assert!(ctx.contains("Sunny, 20C"));
    }

    // ── parse_rendered: per-engine coverage on rendered-style HTML ──────────

    #[test]
    fn parse_rendered_extracts_bing_results() {
        let html = r#"<div id="b_results"><ol>
            <li class="b_algo"><h2><a href="https://example.com/a">Bing Title A</a></h2>
              <div class="b_caption"><p>Bing snippet A.</p></div></li>
            <li class="b_algo"><h2><a href="https://example.com/b">Bing Title B</a></h2>
              <div class="b_caption"><p>Bing snippet B.</p></div></li>
          </ol></div>"#;
        for engine in [SearchEngine::BingCn, SearchEngine::Bing] {
            let r = parse_rendered(engine, html);
            assert!(r.len() >= 2, "{engine:?} parsed {} results", r.len());
            assert_eq!(r[0].title, "Bing Title A");
            assert_eq!(r[0].url, "https://example.com/a");
        }
    }

    #[test]
    fn parse_rendered_extracts_google_results() {
        // CSS-path fixture (div.g + h3 + .VwiC3b); parse_rendered falls back to
        // the CSS parser when the regex pass misses.
        let html = r#"<div id="search"><div id="rso">
            <div class="g"><a href="/url?q=https://example.com/g1&sa=t"><h3>Google Title</h3></a>
              <div class="VwiC3b">Google snippet.</div></div>
          </div></div>"#;
        let r = parse_rendered(SearchEngine::Google, html);
        assert_eq!(r.len(), 1, "google parsed {} results", r.len());
        assert_eq!(r[0].title, "Google Title");
        assert_eq!(r[0].url, "https://example.com/g1");
    }

    #[test]
    fn parse_rendered_extracts_baidu_results() {
        let html = r#"<div id="content_left">
            <div class="result c-container"><h3><a href="http://www.baidu.com/link?url=abc">Baidu Title</a></h3>
              <div class="c-abstract">Baidu snippet.</div></div>
          </div>"#;
        let r = parse_rendered(SearchEngine::Baidu, html);
        assert_eq!(r.len(), 1, "baidu parsed {} results", r.len());
        assert_eq!(r[0].title, "Baidu Title");
    }

    #[test]
    fn parse_rendered_extracts_ddg_results_and_decodes_redirect() {
        let html = r#"<div id="links">
            <div class="result"><a class="result__a"
                href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.org%2Fpage&rut=x">DDG Title</a>
              <div class="result__snippet">DDG snippet.</div></div>
          </div>"#;
        let r = parse_rendered(SearchEngine::DuckDuckGo, html);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].url, "https://example.org/page");
        assert_eq!(r[0].title, "DDG Title");
    }

    // ── graceful degradation (objective 3, user Test 4A/4B) ────────────────

    #[test]
    fn parse_rendered_yields_empty_on_a_challenge_page() {
        let cf = r#"<html><head><title>Just a moment...</title></head>
            <body><div class="cf-browser-verification"></div></body></html>"#;
        for engine in [
            SearchEngine::BingCn,
            SearchEngine::Bing,
            SearchEngine::Google,
            SearchEngine::Baidu,
            SearchEngine::DuckDuckGo,
        ] {
            assert!(parse_rendered(engine, cf).is_empty(), "{engine:?} should parse a challenge page to empty");
        }
    }

    #[test]
    fn parse_rendered_yields_empty_on_unknown_dom() {
        let unknown = "<html><body><main><section>nothing we recognize</section></main></body></html>";
        for engine in [
            SearchEngine::BingCn,
            SearchEngine::Google,
            SearchEngine::Baidu,
            SearchEngine::DuckDuckGo,
        ] {
            assert!(parse_rendered(engine, unknown).is_empty(), "{engine:?} should degrade to empty, not panic");
        }
    }

    //   cargo test --lib search::tests::live_search -- --ignored --nocapture
    // Requires the running app (a real WebView + AppHandle), so it cannot run in
    // the unit-test harness — exercise it via the manual runtime checklist
    // (npm run tauri dev, then send a factual query and watch the
    // `search: webview … → N result(s)` log). Kept here as documentation.
}
