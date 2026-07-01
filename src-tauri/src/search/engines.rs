//! Per-engine search-result-page (SERP) scraping.
//!
//! Each engine maps to a GET endpoint + query-param name and a CSS-selector
//! based parser that turns the result HTML into `RawResult`s. Parsing is pure +
//! synchronous (no `await`), so the non-`Send` `scraper::Html` never crosses an
//! await point. Missing selectors fail gracefully (empty vec), never panic.
//!
//! Selectors target the current desktop layouts; engines change markup over
//! time, so callers must tolerate an empty result and fall back.

use scraper::{Html, Selector};

use super::tracking;
use super::SearchEngine;

/// A single parsed search hit (pre body-extraction).
#[derive(Debug, Clone, PartialEq)]
pub struct RawResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// GET endpoint + query-parameter name for an engine.
pub fn endpoint(engine: SearchEngine) -> (&'static str, &'static str) {
    match engine {
        SearchEngine::BingCn => ("https://cn.bing.com/search", "q"),
        SearchEngine::Bing => ("https://www.bing.com/search", "q"),
        SearchEngine::Google => ("https://www.google.com/search", "q"),
        SearchEngine::Baidu => ("https://www.baidu.com/s", "wd"),
        SearchEngine::DuckDuckGo => ("https://html.duckduckgo.com/html/", "q"),
    }
}

/// Parse a SERP HTML document into results for the given engine.
pub fn parse_serp(engine: SearchEngine, html: &str) -> Vec<RawResult> {
    let doc = Html::parse_document(html);
    match engine {
        SearchEngine::BingCn | SearchEngine::Bing => parse_bing(&doc),
        SearchEngine::Google => parse_google(&doc),
        SearchEngine::Baidu => parse_baidu(&doc),
        SearchEngine::DuckDuckGo => parse_ddg(&doc),
    }
}

/// Compile a selector, treating an invalid one as "matches nothing".
fn sel(s: &str) -> Selector {
    Selector::parse(s).unwrap_or_else(|_| Selector::parse("nonexistent-xyz").unwrap())
}

fn text_of(el: &scraper::ElementRef) -> String {
    collapse_ws(&el.text().collect::<String>())
}

fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ── Bing (CN + international share the `b_algo` layout) ──────────────
fn parse_bing(doc: &Html) -> Vec<RawResult> {
    let item = sel("li.b_algo");
    let title_a = sel("h2 a");
    let caption = sel(".b_caption p, p");
    let mut out = Vec::new();
    for el in doc.select(&item) {
        let Some(a) = el.select(&title_a).next() else { continue };
        let url = tracking::clean_bing_url(a.value().attr("href").unwrap_or_default());
        if url.is_empty() {
            continue;
        }
        out.push(RawResult {
            title: text_of(&a),
            url,
            snippet: el.select(&caption).next().map(|p| text_of(&p)).unwrap_or_default(),
        });
    }
    out
}

// ── Google ──────────────────────────────────────────────────────────
fn parse_google(doc: &Html) -> Vec<RawResult> {
    let item = sel("div.g, div.tF2Cxc");
    let title_h = sel("h3");
    let link = sel("a");
    let snippet = sel(".VwiC3b, .IsZvec, div[data-sncf] span");
    let mut out = Vec::new();
    for el in doc.select(&item) {
        let Some(h) = el.select(&title_h).next() else { continue };
        let Some(a) = el.select(&link).next() else { continue };
        let url = tracking::clean_google_url(a.value().attr("href").unwrap_or_default());
        if url.is_empty() || tracking::is_google_internal(&url) {
            continue;
        }
        out.push(RawResult {
            title: text_of(&h),
            url,
            snippet: el.select(&snippet).next().map(|s| text_of(&s)).unwrap_or_default(),
        });
    }
    out
}

// ── Baidu ───────────────────────────────────────────────────────────
fn parse_baidu(doc: &Html) -> Vec<RawResult> {
    let item = sel("div.result, div.c-container");
    let title_a = sel("h3 a");
    let snippet = sel(".c-abstract, [class*=content-right], .c-span-last");
    let mut out = Vec::new();
    for el in doc.select(&item) {
        let Some(a) = el.select(&title_a).next() else { continue };
        let url = a.value().attr("href").unwrap_or_default().to_string();
        if url.is_empty() {
            continue;
        }
        out.push(RawResult {
            title: text_of(&a),
            // Baidu links are redirectors (baidu.com/link?url=…); kept as-is —
            // body extraction follows the redirect, and the URL is still shown.
            url,
            snippet: el.select(&snippet).next().map(|s| text_of(&s)).unwrap_or_default(),
        });
    }
    out
}

// ── DuckDuckGo (html.duckduckgo.com) ────────────────────────────────
fn parse_ddg(doc: &Html) -> Vec<RawResult> {
    let item = sel("div.result, div.web-result");
    let title_a = sel("a.result__a");
    let snippet = sel(".result__snippet");
    let mut out = Vec::new();
    for el in doc.select(&item) {
        let Some(a) = el.select(&title_a).next() else { continue };
        let raw = a.value().attr("href").unwrap_or_default();
        let url = tracking::clean_ddg_url(raw);
        if url.is_empty() {
            continue;
        }
        out.push(RawResult {
            title: text_of(&a),
            url,
            snippet: el.select(&snippet).next().map(|s| text_of(&s)).unwrap_or_default(),
        });
    }
    out
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bing_layout() {
        let html = r#"
            <ol>
              <li class="b_algo"><h2><a href="https://example.com/a">Title A</a></h2>
                <div class="b_caption"><p>Snippet A here.</p></div></li>
              <li class="b_algo"><h2><a href="https://example.com/b">Title B</a></h2>
                <div class="b_caption"><p>Snippet B here.</p></div></li>
            </ol>"#;
        let r = parse_serp(SearchEngine::BingCn, html);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].title, "Title A");
        assert_eq!(r[0].url, "https://example.com/a");
        assert_eq!(r[0].snippet, "Snippet A here.");
    }

    #[test]
    fn parses_ddg_and_decodes_redirect() {
        let html = r#"
            <div class="result">
              <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.org%2Fpage&rut=x">
                Example Page</a>
              <div class="result__snippet">A snippet.</div>
            </div>"#;
        let r = parse_serp(SearchEngine::DuckDuckGo, html);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].url, "https://example.org/page");
        assert_eq!(r[0].title, "Example Page");
    }

    #[test]
    fn parses_google_layout_and_unwraps_redirect() {
        let html = r#"
            <div id="rso">
              <div class="g"><a href="/url?q=https%3A%2F%2Fexample.com%2Fg1&sa=t">
                <h3>Google Title</h3></a>
                <div class="VwiC3b">Google snippet here.</div></div>
            </div>"#;
        let r = parse_serp(SearchEngine::Google, html);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].title, "Google Title");
        assert_eq!(r[0].url, "https://example.com/g1");
        assert_eq!(r[0].snippet, "Google snippet here.");
    }

    #[test]
    fn parses_baidu_layout() {
        let html = r#"
            <div id="content_left">
              <div class="result c-container">
                <h3><a href="http://www.baidu.com/link?url=abc">Baidu Title</a></h3>
                <div class="c-abstract">Baidu snippet here.</div></div>
            </div>"#;
        let r = parse_serp(SearchEngine::Baidu, html);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].title, "Baidu Title");
        assert_eq!(r[0].url, "http://www.baidu.com/link?url=abc");
        assert_eq!(r[0].snippet, "Baidu snippet here.");
    }

    #[test]
    fn missing_selectors_yield_empty() {
        assert!(parse_serp(SearchEngine::BingCn, "<html><body>nope</body></html>").is_empty());
        assert!(parse_serp(SearchEngine::Google, "<html><body>nope</body></html>").is_empty());
        assert!(parse_serp(SearchEngine::Baidu, "<html><body>nope</body></html>").is_empty());
    }


}
