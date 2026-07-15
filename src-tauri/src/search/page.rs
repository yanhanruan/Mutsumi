//! Result-page content extraction — the "go deeper" step.
//!
//! A SERP snippet is a *bad carrier* of answers: engines often truncate the
//! datum out of it or replace it with the site's SEO tagline, which is why
//! [`super::quality`] has to work so hard on surface text. When the curated
//! snippet does **not** already state the kind of fact the query asks for
//! ([`should_deepen`]), the pipeline navigates the same hidden WebView to the
//! top result's real page ([`super::webview::fetch_page`]) and extracts the
//! relevant content from it — the page, unlike the snippet, actually contains
//! the answer.
//!
//! Extraction is pure and unit-tested: strip non-content subtrees, collect
//! leaf-block text segments, keep the segments that state the wanted datum
//! and/or are on-topic for the query, in document order, under a hard cap.
//! Anything ambiguous degrades to an empty extract — the caller then keeps the
//! surface snippet, so deepening can only ever *add* signal, never lose it.

use scraper::{Html, Selector};

use super::quality::{self, DatumKind};

/// Hard cap on the assembled extract (chars) — the page replaces a snippet in
/// the prompt, so it may be richer than one, but must not crowd the context.
const EXTRACT_MAX_CHARS: usize = 400;
/// Cap on any single segment, so one verbose paragraph can't eat the budget.
const SEGMENT_MAX_CHARS: usize = 160;
/// Segments shorter than this are UI crumbs (登录 / 更多), never content.
const SEGMENT_MIN_CHARS: usize = 4;

/// Whether the curated results warrant fetching the top result's page.
///
/// * Rank-1 already states the wanted datum in its surface text → surface data
///   is enough; deepening would only add latency.
/// * A specific-datum query (weather / money / date) whose rank-1 does **not**
///   state it → the snippet failed to carry the answer; go get the page.
/// * A general query ([`DatumKind::Any`]) → deepen only when rank-1 has no
///   usable snippet at all (filler blanked to a bare link): on-topic prose
///   often *is* the answer for these, and a page fetch costs seconds.
///
/// Pure — unit-tested. The caller additionally requires a non-empty result set.
pub(crate) fn should_deepen(kind: DatumKind, rank1_datum: bool, rank1_has_snippet: bool) -> bool {
    if rank1_datum {
        return false;
    }
    match kind {
        DatumKind::Any => !rank1_has_snippet,
        _ => true,
    }
}

/// Extract the query-relevant content from a fetched result page.
///
/// Returns an empty string when the page is an anti-bot interstitial or when
/// nothing on it relates to the query — the caller treats empty as "deepening
/// failed" and keeps the surface snippet (fail-open, like every quality gate).
pub(crate) fn extract_relevant(query: &str, html: &str) -> String {
    if super::webview::is_challenge_page(html) {
        return String::new();
    }
    let segments = text_segments(html);
    let terms = quality::query_terms(query);
    let kind = quality::wanted_datum(query);

    // Score: the wanted datum outranks topical prose; both beat neither.
    let mut scored: Vec<(usize, u8, &String)> = Vec::new();
    for (i, seg) in segments.iter().enumerate() {
        let datum = quality::has_wanted_datum(kind, seg);
        let relevant = quality::is_relevant(&terms, seg);
        let score = match (datum, relevant) {
            (true, true) => 3,
            (true, false) => 2,
            (false, true) => 1,
            (false, false) => continue,
        };
        scored.push((i, score, seg));
    }

    // Fill the budget best-score-first, then re-assemble in document order so
    // the extract reads like the page, not like a shuffle.
    scored.sort_by_key(|&(i, score, _)| (std::cmp::Reverse(score), i));
    let mut picked: Vec<(usize, String)> = Vec::new();
    let mut total = 0usize;
    for (i, _, seg) in scored {
        let clipped = clip(seg, SEGMENT_MAX_CHARS);
        let len = clipped.chars().count();
        if total + len > EXTRACT_MAX_CHARS {
            continue;
        }
        total += len;
        picked.push((i, clipped));
    }
    picked.sort_by_key(|&(i, _)| i);
    picked.into_iter().map(|(_, s)| s).collect::<Vec<_>>().join("\n")
}

/// Truncate to `max` chars with an ellipsis, on a char boundary.
fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect::<String>().trim_end().to_string() + "…"
}

/// Subtrees that never carry content: code, chrome, and controls.
const SKIP_TAGS: &[&str] = &[
    "script", "style", "noscript", "svg", "nav", "header", "footer", "aside",
    "form", "button", "select", "template", "head", "iframe",
];

/// Block-level elements whose text forms one segment. `div` is included because
/// data widgets (weather cards, rate tables) are div/span soup with no `p` at
/// all — the leaf-block rule below keeps nested divs from double-counting.
const BLOCK_SELECTOR: &str =
    "p,div,li,h1,h2,h3,h4,h5,h6,td,th,dd,dt,blockquote,figcaption";

/// Collect the page's text as leaf-block segments, in document order:
/// each block element that contains **no other block element** contributes its
/// full descendant text (so inline markup — `气温<b>33</b>度` — stays joined),
/// and anything under a [`SKIP_TAGS`] subtree is dropped. Whitespace-collapsed,
/// de-duplicated (pages repeat their own headings), crumbs dropped.
fn text_segments(html: &str) -> Vec<String> {
    let doc = Html::parse_document(html);
    let blocks = Selector::parse(BLOCK_SELECTOR).expect("valid block selector");
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();

    for el in doc.select(&blocks) {
        // Leaf blocks only — an element wrapping other blocks would repeat them.
        if el.select(&blocks).next().is_some() {
            continue;
        }
        // Skip anything inside non-content chrome.
        let in_chrome = el.ancestors().any(|a| {
            a.value()
                .as_element()
                .is_some_and(|e| SKIP_TAGS.contains(&e.name()))
        });
        if in_chrome {
            continue;
        }
        let text: String = el.text().collect::<Vec<_>>().join(" ");
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if text.chars().count() < SEGMENT_MIN_CHARS {
            continue;
        }
        if seen.insert(text.clone()) {
            out.push(text);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deepen_policy_skips_when_surface_data_is_enough() {
        // Rank-1 already states the wanted datum → never deepen.
        assert!(!should_deepen(DatumKind::Weather, true, true));
        assert!(!should_deepen(DatumKind::Any, true, true));
        // Specific-datum query whose snippet failed to carry it → deepen.
        assert!(should_deepen(DatumKind::Weather, false, true));
        assert!(should_deepen(DatumKind::Money, false, false));
        assert!(should_deepen(DatumKind::Date, false, true));
        // General query: on-topic prose is often the answer — deepen only when
        // there is no usable snippet at all.
        assert!(!should_deepen(DatumKind::Any, false, true));
        assert!(should_deepen(DatumKind::Any, false, false));
    }

    /// A realistic weather page: chrome, scripts, a div/span data widget (no
    /// <p> anywhere near the values), and unrelated cross-links.
    const WEATHER_PAGE: &str = r#"<html><head><title>福安天气</title>
        <script>var config = { temp: 999, api: "https://t.example/api" };</script>
        <style>.card { color: red }</style></head>
        <body>
        <nav><ul><li>首页</li><li>国际天气</li><li>历史天气</li></ul></nav>
        <div class="card">
          <div class="now"><span>福安市 今日天气</span></div>
          <div class="temp"><span>体感温度</span><span>33°</span></div>
          <div class="range">多云 25 ~ 37℃ 南风2级</div>
          <div class="hum"><span>湿度</span><span>90%</span></div>
        </div>
        <div class="links">
          <div>北京旅游攻略大全，吃住行一站式服务平台</div>
        </div>
        <footer><div>关于我们 © 2011 年正式上线</div></footer>
        </body></html>"#;

    #[test]
    fn extract_keeps_datum_segments_and_drops_chrome() {
        let out = extract_relevant("福安今天天气如何", WEATHER_PAGE);
        assert!(out.contains("25 ~ 37℃"), "range missing: {out}");
        assert!(out.contains("体感温度 33°"), "widget spans not joined: {out}");
        assert!(out.contains("湿度 90%"), "humidity missing: {out}");
        // Chrome and code never leak.
        assert!(!out.contains("temp: 999"), "script leaked: {out}");
        assert!(!out.contains("国际天气"), "nav leaked: {out}");
        assert!(!out.contains("正式上线"), "footer leaked: {out}");
        // Off-topic sidebar (no weather datum, shares no term) is dropped.
        assert!(!out.contains("北京旅游"), "off-topic sidebar leaked: {out}");
    }

    #[test]
    fn extract_reads_in_document_order_and_respects_the_cap() {
        let out = extract_relevant("福安今天天气如何", WEATHER_PAGE);
        let temp = out.find("体感温度").expect("temp present");
        let hum = out.find("湿度").expect("humidity present");
        assert!(temp < hum, "document order not preserved: {out}");
        assert!(out.chars().count() <= EXTRACT_MAX_CHARS + 8, "over cap: {}", out.chars().count());
    }

    #[test]
    fn extract_money_page_keeps_rates() {
        let html = r#"<body>
            <h1>人民币兑日元汇率</h1>
            <table><tr><td>1 CNY</td><td>≈ 21.44 JPY</td></tr>
            <tr><td>更新时间</td><td>10:40</td></tr></table>
            <div>本站创立于1993年1月，专注外汇资讯。</div>
          </body>"#;
        let out = extract_relevant("人民币对日元汇率", html);
        assert!(out.contains("21.44"), "rate missing: {out}");
        assert!(out.contains("人民币兑日元汇率"), "topical heading missing: {out}");
    }

    #[test]
    fn extract_fails_open_to_empty_on_challenge_or_irrelevant_pages() {
        // Anti-bot interstitial → empty (caller keeps the surface snippet).
        let cf = r#"<html><head><title>Just a moment...</title></head>
            <body><div class="cf-browser-verification"></div></body></html>"#;
        assert_eq!(extract_relevant("东京天气", cf), "");
        // A page that shares nothing with the query and states no wanted datum.
        let off = "<body><p>英伟达盘中大涨，分析师纷纷上调目标价。</p></body>";
        assert_eq!(extract_relevant("东京天气", off), "");
    }

    #[test]
    fn extract_answers_prose_questions_from_topical_paragraphs() {
        // DatumKind::Any + no number anywhere: on-topic prose still extracts.
        let html = r#"<body>
            <nav><div>登录 注册 更多</div></nav>
            <p>MyGO!!!!! 的主唱是高松灯，由羊宫妃那配音。</p>
            <p>广告：一站式服务平台，欢迎访问官方网站。</p>
          </body>"#;
        let out = extract_relevant("MyGO 的主唱是谁", html);
        assert!(out.contains("高松灯"), "answer missing: {out}");
        assert!(!out.contains("一站式"), "ad paragraph leaked: {out}");
    }

    #[test]
    fn segments_join_inline_markup_and_dedup_repeats() {
        let html = r#"<body>
            <p>气温<b>33</b>度</p>
            <div>重复的标题</div><div>重复的标题</div>
          </body>"#;
        let segs = text_segments(html);
        assert!(segs.iter().any(|s| s == "气温 33 度"), "inline text split: {segs:?}");
        assert_eq!(segs.iter().filter(|s| s.contains("重复的标题")).count(), 1, "{segs:?}");
    }
}
