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
//
// Modern Baidu results are `div.result.c-container` cards. Two quirks vs. the
// other engines (verified against a real dump):
//   * The visible `<a href>` is an opaque `baidu.com/link?url=…` redirect, but
//     the container carries the **real** destination in its `mu` attribute — so
//     we read `mu` (which also gives distinct hosts for de-dup). Fall back to the
//     redirect href only if `mu` is missing.
//   * The snippet lives in a `data-module="abstract"` block with hashed class
//     names (`summary-text_xxx`), not the old `.c-abstract` — so target the
//     stable `data-module` hook first, then older class fallbacks.
fn parse_baidu(doc: &Html) -> Vec<RawResult> {
    let item = sel("div.result.c-container, div.result");
    let title_a = sel("h3 a");
    let snippet = sel(
        "[data-module=abstract], [class*=summary-text], .c-abstract, [class*=content-right], .c-span-last",
    );
    // 商业推广 (sponsored) rows: Baidu marks them with `tuiguang` classes/attrs
    // or an ad-badge span whose entire text is 广告/推广. Never surface ads.
    let ad_marker = sel("[data-tuiguang], [class*=tuiguang]");
    let span = sel("span");
    let mut out = Vec::new();
    // Baidu answer cards are `result-op` aladdin cards the organic loop below
    // skips, but they hold the actual datum — put one first so "…汇率" / "…天气"
    // queries feed the model the number, not just links. (A query shows at most
    // one such card.)
    if let Some(card) = parse_baidu_exrate_card(doc).or_else(|| parse_baidu_weather_card(doc)) {
        out.push(card);
    }
    for el in doc.select(&item) {
        if el.value().attr("data-tuiguang").is_some()
            || el.value().classes().any(|c| c.contains("tuiguang"))
            || el.select(&ad_marker).next().is_some()
            || el.select(&span).any(|s| matches!(text_of(&s).as_str(), "广告" | "推广"))
        {
            continue; // sponsored row
        }
        let Some(a) = el.select(&title_a).next() else { continue };
        let url = el
            .value()
            .attr("mu")
            .filter(|u| u.starts_with("http"))
            .map(str::to_string)
            .unwrap_or_else(|| a.value().attr("href").unwrap_or_default().to_string());
        let title = text_of(&a);
        if url.is_empty() || title.is_empty() {
            continue;
        }
        out.push(RawResult {
            title,
            url,
            snippet: el.select(&snippet).next().map(|s| text_of(&s)).unwrap_or_default(),
        });
    }
    out
}

/// Baidu's exchange-rate answer card (百度股市通 汇率换算): a `result-op`
/// aladdin card with `tpl="jr_exrate_san"`. It's not a normal `result` card, so
/// the organic parser skips it — but it holds the rate in `money-style` divs
/// (e.g. "1 美元 ≈ 6.7797 人民币" / "1 人民币 ≈ 0.1475 美元"), which is exactly the
/// answer for "…汇率" queries. Extract those lines as a synthetic result, using
/// the card's `mu` as the source URL.
fn parse_baidu_exrate_card(doc: &Html) -> Option<RawResult> {
    let card = sel("div.result-op[tpl=jr_exrate_san], div[tpl=jr_exrate_san]");
    let rate = sel("[class*=money-style]");
    let el = doc.select(&card).next()?;
    let lines: Vec<String> = el
        .select(&rate)
        .map(|r| text_of(&r))
        .filter(|s| !s.is_empty())
        .collect();
    if lines.is_empty() {
        return None;
    }
    let url = el
        .value()
        .attr("mu")
        .filter(|u| u.starts_with("http"))
        .unwrap_or("https://www.baidu.com/")
        .to_string();
    Some(RawResult { title: "汇率换算".to_string(), url, snippet: lines.join("；") })
}

/// Baidu's weather answer card (`tpl="weather_forecast_new_san"`, a `result-op`
/// aladdin card). Its rendered summary — city, current temp, condition, day
/// range, air quality, feels-like, wind — is a contiguous run that precedes the
/// hourly chart, whose axis tabs begin at "气温". Take the card text up to there
/// so the model gets today's conditions without the hourly/7-day noise.
fn parse_baidu_weather_card(doc: &Html) -> Option<RawResult> {
    let card = sel(
        "div.result-op[tpl=weather_forecast_new_san], div[tpl=weather_forecast_new_san]",
    );
    let el = doc.select(&card).next()?;
    let text = text_of(&el);
    let summary = text.split("气温").next().unwrap_or(&text).trim();
    if summary.is_empty() {
        return None;
    }
    let url = el
        .value()
        .attr("mu")
        .filter(|u| u.starts_with("http"))
        .unwrap_or("https://www.baidu.com/")
        .to_string();
    Some(RawResult { title: "天气".to_string(), url, snippet: summary.to_string() })
}

// ── DuckDuckGo (html.duckduckgo.com) ────────────────────────────────
fn parse_ddg(doc: &Html) -> Vec<RawResult> {
    let item = sel("div.result, div.web-result");
    let title_a = sel("a.result__a");
    let snippet = sel(".result__snippet");
    let mut out = Vec::new();
    for el in doc.select(&item) {
        // Sponsored rows carry a `result--ad` class (their y.js click-through URL
        // is also rejected in `clean_ddg_url`, which covers the regex path too).
        if el.value().classes().any(|c| c.starts_with("result--ad")) {
            continue;
        }
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
        // Older layout: no `mu`, snippet in `.c-abstract` → falls back correctly.
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
    fn parses_baidu_modern_card_mu_url_and_abstract_snippet() {
        // Trimmed from a real dump: the real URL is in `mu`, the visible href is
        // an opaque baidu.com/link redirect, and the snippet (with the datum) is
        // in a data-module="abstract" block with hashed classes.
        let html = r#"
            <div class="result c-container xpath-log new-pmd" tpl="www_index"
                 mu="https://cn.investing.com/currencies/cny-jpy">
              <div class="cosc-card"><div class="title-wrapper_4oy6O">
                <h3 class="cosc-title t"><a class="cosc-title-a"
                    href="http://www.baidu.com/link?url=v8x9MKHcyw5"><span class="cosc-title-slot">
                    <span class="tts-b-hl"><!--s-text-->今日<em>人民币对日元汇率</em>(CNY JPY)<!--/s-text--></span>
                    </span></a></h3></div>
                <div data-module="abstract" class="content-gap_3jlQr"><div class="cos-color-text-tiny">
                  <span class="summary-text_15QGa"><em>查询</em>今日人民币对日元汇率(CNY JPY)最新行情。 CNY/JPY最新汇率为23.9200。 实时外汇 货币 JPY 23.9203 -0.0114 (-0.05%)</span>
                </div></div>
              </div></div>"#;
        let r = parse_serp(SearchEngine::Baidu, html);
        assert_eq!(r.len(), 1);
        // Real destination host, not the baidu.com/link redirect.
        assert_eq!(r[0].url, "https://cn.investing.com/currencies/cny-jpy");
        assert_eq!(r[0].title, "今日人民币对日元汇率(CNY JPY)");
        // The actual datum survives into the snippet.
        assert!(r[0].snippet.contains("CNY/JPY最新汇率为23.9200"), "snippet was {:?}", r[0].snippet);
        assert!(r[0].snippet.contains("23.9203"), "rate missing from {:?}", r[0].snippet);
    }

    #[test]
    fn parses_baidu_exchange_rate_card_first() {
        // The 汇率换算 answer card is a `result-op` aladdin card (tpl=jr_exrate_san)
        // with the rate in money-style divs and the real URL in `mu`. It must come
        // out first, ahead of the organic result below it.
        let html = r#"
            <div id="content_left">
              <div class="result-op c-container new-pmd" tpl="jr_exrate_san"
                   mu="http://forex.hexun.com/USDCNY/">
                <a href="http://www.baidu.com/link?url=WeOV">汇率换算</a>
                <div><div class="money-style_59F57">1 美元 ≈ 6.7797 人民币</div>
                     <div class="money-style_59F57">1 人民币 ≈ 0.1475 美元</div></div>
              </div>
              <div class="result c-container" mu="https://www.chinamoney.com.cn/x">
                <h3><a href="http://www.baidu.com/link?url=abc">人民币汇率中间价</a></h3>
                <div data-module="abstract"><span class="summary-text_1">中间价说明</span></div>
              </div>
            </div>"#;
        let r = parse_serp(SearchEngine::Baidu, html);
        assert_eq!(r.len(), 2, "card + one organic");
        assert_eq!(r[0].title, "汇率换算");
        assert_eq!(r[0].url, "http://forex.hexun.com/USDCNY/", "card URL from mu");
        assert!(r[0].snippet.contains("6.7797") && r[0].snippet.contains("0.1475"), "got {:?}", r[0].snippet);
        assert_eq!(r[1].url, "https://www.chinamoney.com.cn/x", "organic still parsed");
    }

    #[test]
    fn parses_baidu_weather_card_summary_without_hourly() {
        // Weather answer card (tpl=weather_forecast_new_san): take the summary up
        // to the hourly chart ("气温" tab), URL from `mu`.
        let html = r#"
            <div id="content_left">
              <div class="result-op c-container new-pmd" tpl="weather_forecast_new_san"
                   mu="https://weathernew.pae.baidu.com/weathernew/pc?query=x">
                <div class="main-info_7ImSO">
                  <div class="loc_4ZDVN">宁德</div><div>10:40更新</div>
                  <div class="weather-main_lecpj">33<span>°</span> 28~37°C 晴 44 优 体感38° 东风3级
                    <div class="tab">气温 风力 降水量 紫外线 现在 11:00 12:00</div>
                  </div>
                </div>
              </div>
            </div>"#;
        let r = parse_serp(SearchEngine::Baidu, html);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].title, "天气");
        assert_eq!(r[0].url, "https://weathernew.pae.baidu.com/weathernew/pc?query=x");
        assert!(r[0].snippet.contains("宁德") && r[0].snippet.contains("晴") && r[0].snippet.contains("28~37°C"), "got {:?}", r[0].snippet);
        assert!(!r[0].snippet.contains("11:00"), "hourly chart must be cut: {:?}", r[0].snippet);
    }

    #[test]
    fn baidu_skips_sponsored_rows() {
        // Ad containers carry data-tuiguang / tuiguang classes / a 广告 badge span;
        // all three markers must exclude the row while organics still parse.
        let html = r#"
            <div id="content_left">
              <div class="result c-container" data-tuiguang="1" mu="https://ad.example.com/a">
                <h3><a href="http://www.baidu.com/link?url=AD1">特价旅游套餐</a></h3></div>
              <div class="result c-container" mu="https://ad.example.com/b">
                <h3><a href="http://www.baidu.com/link?url=AD2">特价酒店</a></h3>
                <span class="ec-tuiguang-label">优惠</span></div>
              <div class="result c-container" mu="https://ad.example.com/c">
                <h3><a href="http://www.baidu.com/link?url=AD3">贷款服务</a></h3>
                <span class="badge">广告</span></div>
              <div class="result c-container" mu="https://real.example.com/x">
                <h3><a href="http://www.baidu.com/link?url=OK">正经结果</a></h3>
                <div data-module="abstract"><span class="summary-text_1">真实摘要。</span></div></div>
            </div>"#;
        let r = parse_serp(SearchEngine::Baidu, html);
        assert_eq!(r.len(), 1, "only the organic row survives: {r:?}");
        assert_eq!(r[0].url, "https://real.example.com/x");
    }

    #[test]
    fn ddg_skips_ad_rows() {
        // Sponsored rows have a result--ad class and a y.js click-through URL —
        // either alone must exclude them.
        let html = r#"
            <div class="result results_links result--ad">
              <a class="result__a" href="https://duckduckgo.com/y.js?ad_provider=bingv7aa&u3=https%3A%2F%2Fad.example.com">Ad title</a>
              <div class="result__snippet">Buy now.</div></div>
            <div class="result results_links">
              <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Freal.example.org%2Fpage&rut=x">Real result</a>
              <div class="result__snippet">Real snippet.</div></div>"#;
        let r = parse_serp(SearchEngine::DuckDuckGo, html);
        assert_eq!(r.len(), 1, "only the organic row survives: {r:?}");
        assert_eq!(r[0].url, "https://real.example.org/page");
    }

    #[test]
    fn missing_selectors_yield_empty() {
        assert!(parse_serp(SearchEngine::BingCn, "<html><body>nope</body></html>").is_empty());
        assert!(parse_serp(SearchEngine::Google, "<html><body>nope</body></html>").is_empty());
        assert!(parse_serp(SearchEngine::Baidu, "<html><body>nope</body></html>").is_empty());
    }


}
