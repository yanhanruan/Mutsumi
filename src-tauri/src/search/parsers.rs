//! Regex-based SERP parsers — a complement to the CSS-selector parsers in
//! `engines.rs`.  Regex tolerates class-name churn better on well-structured
//! layout landmarks (e.g. `<li class="b_algo">`); CSS selectors remain the
//! fallback when the regex pass returns nothing.
//!
//! URL cleaning delegates to `tracking` so both parser paths produce identical,
//! redirect-resolved URLs.

use std::sync::OnceLock;

use regex::Regex;

use super::SearchEngine;
use super::engines::RawResult;
use super::tracking;

/// Parse a SERP HTML document into raw results using regex.
/// Returns an empty vec if no results matched — caller should fall back to
/// `engines::parse_serp`.
pub fn parse_serp_regex(engine: SearchEngine, html: &str, max: usize) -> Vec<RawResult> {
    match engine {
        SearchEngine::Bing | SearchEngine::BingCn => parse_bing(html, max),
        SearchEngine::Google => parse_google(html, max),
        SearchEngine::DuckDuckGo => parse_ddg(html, max),
        // Baidu's modern result cards are deeply nested with hashed class names
        // that defeat block-level regex — go straight to the CSS parser
        // (`engines::parse_baidu`), which reads the real URL from the container's
        // `mu` attribute and the snippet from its `data-module="abstract"` block.
        SearchEngine::Baidu => Vec::new(),
    }
}

// ── Bing (CN + international share the `b_algo` layout) ──────────────────────

fn parse_bing(html: &str, max: usize) -> Vec<RawResult> {
    static BLOCK: OnceLock<Regex> = OnceLock::new();
    static TITLE: OnceLock<Regex> = OnceLock::new();
    static SNIP: OnceLock<Regex> = OnceLock::new();

    let block_re =
        BLOCK.get_or_init(|| Regex::new(r"(?si)<li\s+class=.b_algo.[^>]*>.*?</li>").unwrap());
    let title_re = TITLE.get_or_init(|| {
        Regex::new(r#"(?si)<h2[^>]*>\s*<a[^>]+href="([^"]+)"[^>]*>(.*?)</a>"#).unwrap()
    });
    let snip_re =
        SNIP.get_or_init(|| Regex::new(r"(?si)<p[^>]*>(.*?)</p>").unwrap());

    let mut out = Vec::new();
    for block in block_re.find_iter(html) {
        let b = block.as_str();
        let Some(caps) = title_re.captures(b) else { continue };
        let url = tracking::clean_bing_url(caps.get(1).map_or("", |m| m.as_str()));
        let title = clean_html(caps.get(2).map_or("", |m| m.as_str()));
        if title.is_empty() || url.is_empty() {
            continue;
        }
        let snippet = snip_re
            .captures(b)
            .and_then(|c| c.get(1))
            .map(|m| clean_html(m.as_str()))
            .unwrap_or_default();
        out.push(RawResult { title, url, snippet });
        if out.len() >= max {
            break;
        }
    }
    out
}

// ── Google ────────────────────────────────────────────────────────────────────

fn parse_google(html: &str, max: usize) -> Vec<RawResult> {
    static BLOCK: OnceLock<Regex> = OnceLock::new();
    static TITLE: OnceLock<Regex> = OnceLock::new();
    static SNIP: OnceLock<Regex> = OnceLock::new();

    let block_re = BLOCK.get_or_init(|| {
        Regex::new(r#"(?si)<div\s+class="g"[^>]*>.*?</div>\s*</div>\s*</div>"#).unwrap()
    });
    let title_re = TITLE.get_or_init(|| {
        Regex::new(r#"(?si)<a[^>]+href="([^"]+)"[^>]*>.*?<h3[^>]*>(.*?)</h3>"#).unwrap()
    });
    let snip_re = SNIP.get_or_init(|| {
        Regex::new(
            r#"(?si)<div[^>]+(?:class="[^"]*VwiC3b[^"]*"|data-sncf="[^"]*")[^>]*>(.*?)</div>"#,
        )
        .unwrap()
    });

    let mut out = Vec::new();
    for block in block_re.find_iter(html) {
        let b = block.as_str();
        let Some(caps) = title_re.captures(b) else { continue };
        let url = tracking::clean_google_url(caps.get(1).map_or("", |m| m.as_str()));
        let title = clean_html(caps.get(2).map_or("", |m| m.as_str()));
        if title.is_empty() || url.is_empty() || tracking::is_google_internal(&url) {
            continue;
        }
        let snippet = snip_re
            .captures(b)
            .and_then(|c| c.get(1))
            .map(|m| clean_html(m.as_str()))
            .unwrap_or_default();
        out.push(RawResult { title, url, snippet });
        if out.len() >= max {
            break;
        }
    }
    out
}

// ── DuckDuckGo (html.duckduckgo.com) ─────────────────────────────────────────

fn parse_ddg(html: &str, max: usize) -> Vec<RawResult> {
    static LINK: OnceLock<Regex> = OnceLock::new();
    static SNIP: OnceLock<Regex> = OnceLock::new();

    let link_re = LINK.get_or_init(|| {
        Regex::new(r#"(?si)<a[^>]+class="result__a"[^>]+href="([^"]+)"[^>]*>(.*?)</a>"#).unwrap()
    });
    let snip_re = SNIP.get_or_init(|| {
        Regex::new(
            r#"(?si)<(?:a|div)[^>]+class="result__snippet"[^>]*>(.*?)</(?:a|div)>"#,
        )
        .unwrap()
    });

    let links: Vec<_> = link_re.captures_iter(html).collect();
    let snippets: Vec<_> = snip_re.captures_iter(html).collect();
    let mut out = Vec::new();
    for (idx, caps) in links.iter().enumerate() {
        let url = tracking::clean_ddg_url(caps.get(1).map_or("", |m| m.as_str()));
        let title = clean_html(caps.get(2).map_or("", |m| m.as_str()));
        if title.is_empty() || url.is_empty() {
            continue;
        }
        let snippet = snippets
            .get(idx)
            .and_then(|c| c.get(1))
            .map(|m| clean_html(m.as_str()))
            .unwrap_or_default();
        out.push(RawResult { title, url, snippet });
        if out.len() >= max {
            break;
        }
    }
    out
}

// ── Shared helpers ────────────────────────────────────────────────────────────

/// Strip HTML tags, decode entities, collapse whitespace.
fn clean_html(s: &str) -> String {
    static TAGS: OnceLock<Regex> = OnceLock::new();
    static WS: OnceLock<Regex> = OnceLock::new();
    let stripped = TAGS.get_or_init(|| Regex::new(r"<[^>]+>").unwrap()).replace_all(s, "");
    let unescaped = tracking::html_unescape(&stripped);
    WS.get_or_init(|| Regex::new(r"\s+").unwrap())
        .replace_all(&unescaped, " ")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bing_regex_basic_layout() {
        let html = r#"
            <ol>
              <li class="b_algo"><h2><a href="https://example.com/a">Title A</a></h2>
                <div class="b_caption"><p>Snippet A here.</p></div></li>
              <li class="b_algo"><h2><a href="https://example.com/b">Title B</a></h2>
                <div class="b_caption"><p>Snippet B here.</p></div></li>
            </ol>"#;
        let r = parse_serp_regex(SearchEngine::BingCn, html, 5);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].title, "Title A");
        assert_eq!(r[0].url, "https://example.com/a");
        assert_eq!(r[0].snippet, "Snippet A here.");
    }

    #[test]
    fn ddg_regex_decodes_redirect() {
        let html = r#"
            <div class="result">
              <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.org%2Fpage&rut=x">
                Example Page</a>
              <div class="result__snippet">A snippet.</div>
            </div>"#;
        let r = parse_serp_regex(SearchEngine::DuckDuckGo, html, 5);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].url, "https://example.org/page");
        assert_eq!(r[0].title, "Example Page");
    }

    #[test]
    fn empty_html_yields_empty() {
        assert!(parse_serp_regex(SearchEngine::BingCn, "<html><body>nope</body></html>", 5).is_empty());
        assert!(parse_serp_regex(SearchEngine::Google, "<html><body>nope</body></html>", 5).is_empty());
    }

    #[test]
    fn clean_html_strips_tags_and_entities() {
        assert_eq!(clean_html("<b>Hello &amp; World</b>"), "Hello & World");
        assert_eq!(clean_html("<span>  foo  <br/>  bar  </span>"), "foo bar");
    }

    #[test]
    fn max_results_respected() {
        let block = r#"<li class="b_algo"><h2><a href="https://example.com/N">Title N</a></h2><p>Snip</p></li>"#;
        let html: String = (1..=6).map(|i| block.replace('N', &i.to_string())).collect();
        let r = parse_serp_regex(SearchEngine::BingCn, &html, 3);
        assert_eq!(r.len(), 3);
    }
}
