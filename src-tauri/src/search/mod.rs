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

pub mod bench;
pub mod trigger;
pub mod webview;

mod engines;
mod parsers;
mod tracking;

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use engines::RawResult;

/// Max SERP results carried forward into the prompt. Kept small on purpose: the
/// top two organic hits carry the answer, and extra rows are mostly noise that
/// dilutes the model's focus (a wrong-answer symptom on e.g. weather queries).
const MAX_RESULTS: usize = 2;
/// Hard cap on a single snippet, so one verbose result can't crowd out the other
/// or bury the actual datum in boilerplate.
const MAX_SNIPPET_CHARS: usize = 220;

/// The user-selectable search engine.
///
/// Default is **Bing CN**: it renders fully in the WebView and is reliably
/// reachable from a mainland network (Google often isn't). All engines remain
/// selectable from Settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
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
    // A challenge page can itself parse to a stray "result" (Baidu's 百度安全验证
    // captcha yields one). Never feed that to the model — treat a blocking
    // challenge as no web context. `fetch_serp` already surfaces the solve popup.
    if webview::is_blocking_challenge(engine, !raw.is_empty(), &html) {
        log::info!("search: {engine:?} hit a challenge page — no web context");
        return Vec::new();
    }
    if raw.is_empty() {
        log::info!("search: {engine:?} returned no parseable results");
        return Vec::new();
    }

    let results = curate(raw);
    log::info!("search: webview {engine:?} → {} result(s)", results.len());
    results
}

/// Turn raw hits into the ≤[`MAX_RESULTS`] results injected into chat: drop
/// junk/empty rows, de-duplicate by host (mirror/aggregator spam), and clean each
/// snippet + title so the model sees the datum, not boilerplate. Pure — testable.
fn curate(raw: Vec<RawResult>) -> Vec<SearchResult> {
    let mut out: Vec<SearchResult> = Vec::new();
    let mut seen_hosts: Vec<String> = Vec::new();
    for r in raw {
        let title = clean_snippet(&r.title);
        let snippet = clean_snippet(&r.snippet);
        // A row with neither a title nor a usable link is noise.
        if title.is_empty() || !r.url.starts_with("http") {
            continue;
        }
        let host = host_of(&r.url);
        if !host.is_empty() && seen_hosts.iter().any(|h| h == &host) {
            continue; // one hit per host — avoid the same source repeated
        }
        if !host.is_empty() {
            seen_hosts.push(host);
        }
        out.push(SearchResult { title, url: r.url, snippet });
        if out.len() >= MAX_RESULTS {
            break;
        }
    }
    out
}

/// Collapse whitespace, strip common SERP boilerplate, and cap length so a
/// snippet carries the fact rather than chrome.
fn clean_snippet(s: &str) -> String {
    // Boilerplate fragments that add no information to the model.
    const BOILERPLATE: &[&str] = &[
        "网页快照", "百度快照", "翻译此页", "查看全部", "更多 ›", "更多>>", "详情 >",
        "- 维基百科，自由的百科全书", "_百度百科", "_百度知道", "- 知乎",
    ];
    let mut t: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    for b in BOILERPLATE {
        t = t.replace(b, " ");
    }
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    let t = t.trim_matches(|c: char| c == '·' || c == '|' || c == '-' || c.is_whitespace());
    if t.chars().count() <= MAX_SNIPPET_CHARS {
        return t.to_string();
    }
    t.chars().take(MAX_SNIPPET_CHARS).collect::<String>().trim_end().to_string() + "…"
}

/// Registrable host of a URL (for de-duplication). Best-effort, no allocation of
/// a full URL parser: strips scheme, path, and a leading `www.`.
fn host_of(url: &str) -> String {
    let after_scheme = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let host = after_scheme.split(['/', '?', '#']).next().unwrap_or("");
    host.strip_prefix("www.").unwrap_or(host).to_lowercase()
}

/// Parse a rendered SERP document: the regex pass first (tolerant of class-name
/// churn on layout landmarks), then the CSS-selector pass as a fallback. Pure
/// and synchronous; an unrecognized / challenge page yields an empty vec.
pub(crate) fn parse_rendered(engine: SearchEngine, html: &str) -> Vec<RawResult> {
    // Parse a handful more than we keep, so [`curate`] has headroom to drop
    // junk/duplicate-host rows and still return [`MAX_RESULTS`].
    const RAW_FETCH: usize = 8;
    let mut results = parsers::parse_serp_regex(engine, html, RAW_FETCH);
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

    // ── content curation (Task 2: keep the datum, drop the noise) ──────────

    #[test]
    fn curate_trims_to_max_dedupes_hosts_and_drops_junk() {
        let raw = vec![
            RawResult { title: "上海天气 今天 22°C".into(), url: "https://www.weather.com.cn/a".into(), snippet: "多云 22°C 网页快照".into() },
            // same host as #1 → dropped
            RawResult { title: "上海一周天气".into(), url: "https://weather.com.cn/b".into(), snippet: "...".into() },
            RawResult { title: "上海天气预报".into(), url: "https://tianqi.com/x".into(), snippet: "晴 20°C".into() },
            // no usable link → dropped
            RawResult { title: "广告".into(), url: "".into(), snippet: "buy now".into() },
            RawResult { title: "third host".into(), url: "https://example.org/z".into(), snippet: "s".into() },
        ];
        let out = curate(raw);
        assert_eq!(out.len(), MAX_RESULTS, "must cap at MAX_RESULTS");
        assert_eq!(out[0].url, "https://www.weather.com.cn/a");
        assert_eq!(out[1].url, "https://tianqi.com/x", "second weather.com.cn host deduped away");
        assert!(!out[0].snippet.contains("网页快照"), "boilerplate stripped");
    }

    #[test]
    fn clean_snippet_strips_boilerplate_and_caps_length() {
        assert_eq!(clean_snippet("  多云   转晴  网页快照 "), "多云 转晴");
        let long = "字".repeat(MAX_SNIPPET_CHARS + 50);
        let cleaned = clean_snippet(&long);
        assert!(cleaned.chars().count() <= MAX_SNIPPET_CHARS + 1, "capped (+ ellipsis)");
        assert!(cleaned.ends_with('…'));
    }

    #[test]
    fn host_of_strips_scheme_www_and_path() {
        assert_eq!(host_of("https://www.Weather.com.cn/xyz?q=1"), "weather.com.cn");
        assert_eq!(host_of("http://tianqi.com/"), "tianqi.com");
        assert_eq!(host_of("not a url"), "not a url");
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
