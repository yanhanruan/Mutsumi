//! Web-search subsystem (Pipeline: search-enhanced awareness).
//!
//! Keyword-triggered ([`trigger`]), synchronous, scraping-based search with no
//! official APIs. Flow:
//!
//!   needs_search? → fetch SERP (primary engine) → parse results →
//!   concurrently scrape the top pages for body text → format as a labeled
//!   real-time-context block for injection into the chat prompt.
//!
//! Fallback chain on empty/failed primary: **DuckDuckGo HTML → DuckDuckGo
//! Instant Answer → nothing**. All network work goes through Rust/`reqwest`
//! (the webview's `fetch` would hit CORS), with a real desktop User-Agent and
//! redirect following. Failures degrade gracefully — search never breaks chat.

pub mod trigger;

mod engines;
mod parsers;
mod tracking;

use std::sync::Mutex;
use std::time::Duration;

use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};

use engines::RawResult;

/// Desktop Chrome UA — Bing/Google return a degraded layout without a real one.
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
    (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
/// SERP fetch timeout. Bing CN responds in ~4–5s from some networks, so this
/// must stay above that or the primary engine always times out into fallback.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(6);
/// Tighter per-page timeout for deep body scraping — heavy result pages
/// (weather portals etc.) must not stall the turn; we fall back to the snippet.
const BODY_TIMEOUT: Duration = Duration::from_secs(3);
/// Max SERP results carried forward.
const MAX_RESULTS: usize = 3;
/// How many top results to deep-scrape for body text (concurrently).
const PAGES_TO_SCRAPE: usize = 2;
/// Hard cap on extracted body text per page.
const MAX_BODY_CHARS: usize = 700;

/// The user-selectable primary search engine.
///
/// Default is **DuckDuckGo**: its HTML endpoint is fast (~0.3–1.5s) and
/// scrape-friendly, whereas Bing CN measured ~6s + flaky empty results from a
/// mainland network, dominating chat latency. Bing CN and the rest remain
/// selectable (Phase 5b settings picker).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SearchEngine {
    BingCn,
    Bing,
    Google,
    Baidu,
    #[default]
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

/// A finished search hit (with optional deep-scraped body).
#[derive(Debug, Clone, PartialEq)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub body: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("search http error: {0}")]
    Http(#[from] reqwest::Error),
}

/// Reusable scraping HTTP client (real UA, redirect-following, timed out).
pub struct SearchClient {
    http: reqwest::Client,
}

impl SearchClient {
    pub fn new() -> Result<Self, SearchError> {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(REQUEST_TIMEOUT)
            .build()?;
        Ok(Self { http })
    }

    async fn fetch_serp(&self, engine: SearchEngine, query: &str) -> Option<Vec<RawResult>> {
        let (url, param) = engines::endpoint(engine);
        let response = self
            .http
            .get(url)
            .query(&[(param, query)])
            .send()
            .await
            .ok()?;

        // 插入 1：打印 HTTP 状态码（比如 200 或 403拦截）
        println!("status={}", response.status());

        // 再获取 HTML 文本
        let html = response.text().await.ok()?;
        
        // 插入 2：打印 HTML 长度（长度过短通常意味着被反爬虫拦截了）
        println!("html len={}", html.len());

        // 解析并整合结果
        let mut results = parsers::parse_serp_regex(engine, &html, MAX_RESULTS * 2);
        if results.is_empty() {
            results = engines::parse_serp(engine, &html);
        }
        
        // 插入 3：打印最终解析出的结果数量
        println!("results={}", results.len());

        if !results.is_empty() {
            Some(results)
        } else {
            None // 如果你希望即使是空数组也返回 Some，可以改成 Some(results)
        }
    }
}

/// Managed state: the search client + the currently-selected engine.
pub struct SearchState {
    pub client: SearchClient,
    engine: Mutex<SearchEngine>,
}

impl SearchState {
    pub fn new(engine: SearchEngine) -> Result<Self, SearchError> {
        Ok(Self {
            client: SearchClient::new()?,
            engine: Mutex::new(engine),
        })
    }

    pub fn engine(&self) -> SearchEngine {
        *self.engine.lock().expect("search engine mutex")
    }

    pub fn set_engine(&self, engine: SearchEngine) {
        *self.engine.lock().expect("search engine mutex") = engine;
    }
}

/// Tauri command: switch the active search engine at runtime (settings UI).
/// `engine` is the kebab-case key from the frontend config.
#[tauri::command]
pub fn set_search_engine(engine: String, search: tauri::State<'_, SearchState>) {
    search.set_engine(SearchEngine::from_str(&engine));
}

/// Run a search and return up to [`MAX_RESULTS`] hits, deep-scraping the top
/// [`PAGES_TO_SCRAPE`] for body text. Best-effort: returns empty on total
/// failure rather than erroring, so chat is never blocked by search problems.
pub async fn search(client: &SearchClient, engine: SearchEngine, query: &str) -> Vec<SearchResult> {
    let raw = resolve_with_fallback(client, engine, query).await;
    if raw.is_empty() {
        return Vec::new();
    }

    let top: Vec<RawResult> = raw.into_iter().take(MAX_RESULTS).collect();

    // Concurrently deep-scrape the first few pages for body text.
    let mut handles = Vec::new();
    for r in top.iter().take(PAGES_TO_SCRAPE) {
        let http = client.http.clone();
        let url = r.url.clone();
        handles.push(tauri::async_runtime::spawn(fetch_body(http, url)));
    }
    let mut bodies: Vec<Option<String>> = Vec::with_capacity(handles.len());
    for h in handles {
        bodies.push(h.await.ok().flatten());
    }

    top.into_iter()
        .enumerate()
        .map(|(i, r)| SearchResult {
            title: r.title,
            url: r.url,
            snippet: r.snippet,
            body: bodies.get(i).cloned().flatten(),
        })
        .collect()
}

/// Primary engine → DuckDuckGo HTML → DuckDuckGo Instant Answer.
async fn resolve_with_fallback(
    client: &SearchClient,
    engine: SearchEngine,
    query: &str,
) -> Vec<RawResult> {
    if let Some(r) = client.fetch_serp(engine, query).await {
        if !r.is_empty() {
            return r;
        }
        log::info!("search: primary engine {engine:?} returned no results, falling back");
    }
    if engine != SearchEngine::DuckDuckGo {
        if let Some(r) = client.fetch_serp(SearchEngine::DuckDuckGo, query).await {
            if !r.is_empty() {
                return r;
            }
        }
    }
    ddg_instant_answer(client, query).await
}

/// Final fallback: DuckDuckGo Instant Answer JSON (one synthesized result).
async fn ddg_instant_answer(client: &SearchClient, query: &str) -> Vec<RawResult> {
    let json = match client
        .http
        .get("https://api.duckduckgo.com/")
        .query(&[("q", query), ("format", "json"), ("no_html", "1")])
        .send()
        .await
    {
        Ok(resp) => match resp.text().await {
            Ok(t) => t,
            Err(_) => return Vec::new(),
        },
        Err(_) => return Vec::new(),
    };
    let v: serde_json::Value = match serde_json::from_str(&json) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let abstract_text = v["AbstractText"].as_str().unwrap_or("");
    if abstract_text.is_empty() {
        return Vec::new();
    }
    vec![RawResult {
        title: v["Heading"].as_str().unwrap_or("DuckDuckGo").to_string(),
        url: v["AbstractURL"].as_str().unwrap_or("").to_string(),
        snippet: abstract_text.to_string(),
    }]
}

/// Fetch a result page and extract readable body text (or `None`).
async fn fetch_body(http: reqwest::Client, url: String) -> Option<String> {
    if !url.starts_with("http") {
        return None;
    }
    // Tight per-request timeout so a heavy/slow page degrades to "snippet only".
    let html = http
        .get(&url)
        .timeout(BODY_TIMEOUT)
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    let body = extract_body(&html);
    (!body.is_empty()).then_some(body)
}

/// Extract readable body text: concatenate substantial block elements, skipping
/// scripts/styles/nav noise, collapsed and capped at [`MAX_BODY_CHARS`].
pub fn extract_body(html: &str) -> String {
    let doc = Html::parse_document(html);
    let selector = Selector::parse("article p, main p, p, h1, h2, h3, li").unwrap();
    let mut out = String::new();
    for el in doc.select(&selector) {
        let t: String = el.text().collect::<String>();
        let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
        if t.chars().count() < 20 {
            continue; // skip tiny nav/label fragments
        }
        out.push_str(&t);
        out.push('\n');
        if out.chars().count() >= MAX_BODY_CHARS {
            break;
        }
    }
    truncate_chars(&out, MAX_BODY_CHARS)
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.trim().to_string();
    }
    s.chars().take(max).collect::<String>().trim().to_string()
}

/// Format results as the labeled real-time-context block injected into chat.
pub fn format_context(query: &str, results: &[SearchResult]) -> String {
    let mut s = String::from("以下是检索到的实时数据，仅供参考，与问题无关请忽略。\n\n");
    s.push_str(&format!("【搜索：{query}】\n"));
    for (i, r) in results.iter().enumerate() {
        s.push_str(&format!("[{}] {}\n{}\n", i + 1, r.title, r.url));
        if !r.snippet.is_empty() {
            s.push_str(&format!("{}\n", r.snippet));
        }
        if let Some(body) = &r.body {
            if !body.is_empty() {
                s.push_str(body);
                s.push('\n');
            }
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
        // Unknown → the app default (DuckDuckGo).
        assert_eq!(SearchEngine::from_str("anything"), SearchEngine::DuckDuckGo);
        assert_eq!(SearchEngine::default(), SearchEngine::DuckDuckGo);
    }

    #[test]
    fn extract_body_skips_scripts_and_caps_length() {
        let html = r#"<html><head><style>.x{color:red}</style></head><body>
            <script>var a = "this should never appear in the output text";</script>
            <nav>Home</nav>
            <p>This is a substantial paragraph of real content worth keeping.</p>
            <p>Another meaningful sentence that should also be captured here.</p>
        </body></html>"#;
        let body = extract_body(html);
        assert!(body.contains("substantial paragraph"));
        assert!(!body.contains("should never appear"));
        assert!(body.chars().count() <= MAX_BODY_CHARS);
    }

    #[test]
    fn format_context_has_label_and_entries() {
        let results = vec![SearchResult {
            title: "Tokyo Weather".into(),
            url: "https://example.com".into(),
            snippet: "Sunny, 20C".into(),
            body: Some("Detailed forecast text.".into()),
        }];
        let ctx = format_context("东京天气", &results);
        assert!(ctx.contains("以下是检索到的实时数据"));
        assert!(ctx.contains("【搜索：东京天气】"));
        assert!(ctx.contains("Tokyo Weather"));
        assert!(ctx.contains("Detailed forecast text."));
    }

    //   cargo test --lib live_search -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "performs real web scraping (network); layout-dependent"]
    async fn live_search() {
        let client = SearchClient::new().unwrap();
        let results = search(&client, SearchEngine::default(), "今天东京天气").await;
        println!("got {} result(s)", results.len());
        for r in &results {
            println!("- {}\n  {}\n  snippet: {}", r.title, r.url, r.snippet);
            if let Some(b) = &r.body {
                println!("  body[{}]: {}…", b.chars().count(), b.chars().take(80).collect::<String>());
            }
        }
        assert!(!results.is_empty(), "expected at least one search result");
    }

    //   cargo test --lib time_search_phases -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "network timing probe"]
    async fn time_search_phases() {
        use std::time::Instant;
        let client = SearchClient::new().unwrap();
        let q = "今天东京天气";

        for engine in [SearchEngine::BingCn, SearchEngine::DuckDuckGo] {
            let t = Instant::now();
            let serp = client.fetch_serp(engine, q).await.unwrap_or_default();
            println!("[{engine:?}] SERP: {:.2}s, {} results", t.elapsed().as_secs_f32(), serp.len());

            let t = Instant::now();
            let results = search(&client, engine, q).await;
            println!("[{engine:?}] FULL search(): {:.2}s, {} results", t.elapsed().as_secs_f32(), results.len());
        }
    }
}
