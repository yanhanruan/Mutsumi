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
//!   needs_search? → webview::fetch_serp (challenge → window surfaces, waits
//!   for the user's solve) → parse_rendered (regex → CSS fallback) → format as
//!   a labeled real-time-context block for injection into the chat prompt.

pub mod bench;
pub mod trigger;
pub mod webview;

mod engines;
mod parsers;
mod quality;
mod tracking;

use std::sync::{Mutex, OnceLock};

use regex::Regex;
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
    maybe_dump_html(app, engine, &html);

    let raw = parse_rendered(engine, &html);
    // A challenge page can itself parse to a stray "result" (Baidu's 百度安全验证
    // captcha yields one). Never feed that to the model. Reaching this with a
    // challenge means the user didn't clear the surfaced window in time (or the
    // solve is disabled) — `fetch_serp` already waited for them.
    if webview::is_blocking_challenge(engine, !raw.is_empty(), &html) {
        log::info!("search: {engine:?} challenge not cleared — no web context");
        return Vec::new();
    }
    if raw.is_empty() {
        log::info!("search: {engine:?} returned no parseable results");
        return Vec::new();
    }

    let results = curate(query, raw);
    // log::info!("search: webview {engine:?} → {} result(s)", results.len());
    log::info!("search: webview {engine:?} → {:?}", results);
    results
}

/// Debug aid: when `MUTSUMI_SERP_DUMP` is set, write the raw fetched SERP HTML to
/// `<app-log-dir>/serp-dump-<engine>.html` so the real DOM can be inspected and
/// turned into a parser fixture — instead of guessing at (hashed) class names.
/// No-op otherwise, so it's safe to call unconditionally.
fn maybe_dump_html(app: &AppHandle, engine: SearchEngine, html: &str) {
    if std::env::var("MUTSUMI_SERP_DUMP").is_err() {
        return;
    }
    use tauri::Manager;
    let Ok(dir) = app.path().app_log_dir() else { return };
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("serp-dump-{engine:?}.html"));
    match std::fs::write(&path, html) {
        Ok(()) => log::info!("search: dumped {engine:?} SERP HTML ({} bytes) to {}", html.len(), path.display()),
        Err(e) => log::warn!("search: could not dump {engine:?} HTML: {e}"),
    }
}

/// Turn raw hits into the ≤[`MAX_RESULTS`] results injected into chat. Pure —
/// testable. Beyond cleaning + host de-duplication, this is where "useless /
/// unrelated" SERP noise is kept out of the prompt (see [`quality`]):
///
///   * **Relevance** — a hit must be on-topic for `query` (share a term). A hit
///     that is *both* off-topic **and** carries no concrete datum is dropped
///     outright — that's the "unrelated nonsense" the model should never see.
///   * **Low-information snippets** — SEO filler ("X天气网为您提供…查询") names the
///     topic without stating the fact. We keep the topical link but blank its
///     filler snippet, so the model gets a source, not a tagline to parrot.
///   * **Best-first, three tiers** — hits whose snippet states a concrete datum
///     (the number/date the user asked for) fill the slots first, then on-topic
///     prose, then datum-less topical links. SERP order is kept within a tier —
///     we surface the best content without second-guessing the engine's ranking.
fn curate(query: &str, raw: Vec<RawResult>) -> Vec<SearchResult> {
    curate_report(query, raw).0
}

/// What one raw SERP row became in [`curate`], and why — the audit trail behind
/// the quality report (`MUTSUMI_SERP_QUALITY`), so filter behavior is reviewable
/// against real SERPs row by row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowFate {
    /// Sent to the model at rank N (1-based): snippet states a concrete datum.
    SentDatum(usize),
    /// Sent at rank N: on-topic prose, no hard datum.
    SentProse(usize),
    /// Sent at rank N as a bare topical link — its SEO-filler snippet blanked.
    SentLink(usize),
    /// Survived every filter but lost the [`MAX_RESULTS`] cut.
    BeyondCap,
    /// Off-topic for the query and carrying no datum — the "unrelated nonsense".
    DroppedOffTopic,
    /// Same source as an earlier row.
    DroppedDuplicate,
    /// No usable title / non-http URL.
    DroppedUnusable,
}

impl RowFate {
    pub(crate) fn label(&self) -> String {
        match self {
            RowFate::SentDatum(r) => format!("**sent #{r}** · datum"),
            RowFate::SentProse(r) => format!("**sent #{r}** · prose"),
            RowFate::SentLink(r) => format!("**sent #{r}** · link (filler blanked)"),
            RowFate::BeyondCap => "beyond cap".into(),
            RowFate::DroppedOffTopic => "dropped · off-topic".into(),
            RowFate::DroppedDuplicate => "dropped · duplicate source".into(),
            RowFate::DroppedUnusable => "dropped · unusable".into(),
        }
    }
}

/// Per-row audit record: the cleaned fields as parsed (snippet **before** any
/// filler-blanking, so a reviewer sees what the SERP actually said), the quality
/// signals, and the row's fate.
pub(crate) struct RowVerdict {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub relevant: bool,
    pub has_datum: bool,
    pub fate: RowFate,
}

/// [`curate`] with the full audit trail: returns (what is sent to the model,
/// a verdict for **every** raw row in SERP order). The first element is exactly
/// what `curate` returns — `curate` is a thin wrapper.
pub(crate) fn curate_report(query: &str, raw: Vec<RawResult>) -> (Vec<SearchResult>, Vec<RowVerdict>) {
    let terms = quality::query_terms(query);
    // "Datum" is query-aware: for a 天气 query only weather values count, for a
    // 股价/汇率 query only price-like numbers, for a 发售 query dates — so an
    // encyclopedia row's population figure can't win the datum slot for a
    // weather question (the exact failure the quality report exposed).
    let kind = quality::wanted_datum(query);
    let mut verdicts: Vec<RowVerdict> = Vec::with_capacity(raw.len());
    // Surviving rows per tier: (verdict index, has_info), SERP order kept.
    let mut datum_i: Vec<(usize, bool)> = Vec::new();
    let mut prose_i: Vec<(usize, bool)> = Vec::new();
    let mut link_i: Vec<(usize, bool)> = Vec::new();
    let mut seen: Vec<String> = Vec::new();

    for r in raw {
        let title = clean_snippet(&r.title);
        let snippet = clean_snippet(&r.snippet);
        let relevant = quality::is_relevant(&terms, &title) || quality::is_relevant(&terms, &snippet);
        // Titles count too: "英伟达公司 194.83 (-1.39%)_新浪财经" states the price
        // in the title above a filler snippet.
        let has_data =
            quality::has_wanted_datum(kind, &snippet) || quality::has_wanted_datum(kind, &title);
        let has_info = !snippet.is_empty() && !quality::is_low_information(&snippet);
        let mut v = RowVerdict {
            title,
            url: r.url,
            snippet,
            relevant,
            has_datum: has_data,
            fate: RowFate::DroppedUnusable,
        };

        // A row with neither a title nor a usable link is noise.
        if v.title.is_empty() || !v.url.starts_with("http") {
            verdicts.push(v);
            continue;
        }
        // Drop only clear noise: off-topic *and* carrying nothing quotable. A hit
        // that is on-topic is kept; so is one that states the wanted datum even if
        // the topic-match missed (e.g. an English "Tokyo 18°C" for a "东京天气"
        // query) — but off-topic prose with no datum is the "unrelated nonsense".
        if !relevant && !has_data {
            v.fate = RowFate::DroppedOffTopic;
            verdicts.push(v);
            continue;
        }
        let key = dedup_key(&v.url);
        if !key.is_empty() && seen.iter().any(|h| h == &key) {
            v.fate = RowFate::DroppedDuplicate;
            verdicts.push(v);
            continue;
        }
        if !key.is_empty() {
            seen.push(key);
        }
        // Survivor: provisional fate until ranks are assigned below. Only rows
        // that are BOTH on-topic and datum-bearing reach the datum tier — an
        // off-topic row with a datum (cross-language hit, or a stale/mislabeled
        // page the relevance check couldn't place) rides in the prose tier so it
        // can never outrank a relevant answer.
        v.fate = RowFate::BeyondCap;
        let idx = verdicts.len();
        if relevant && has_data {
            datum_i.push((idx, has_info));
        } else if has_info || has_data {
            prose_i.push((idx, has_info));
        } else {
            link_i.push((idx, has_info));
        }
        verdicts.push(v);
    }

    // Fill the slots best-first: datum > prose > bare links. A filler snippet is
    // never fed to the model regardless of tier (a title-datum row keeps its
    // rank but contributes its title + URL only).
    let ordered: Vec<(usize, u8, bool)> = datum_i
        .iter()
        .map(|&(i, info)| (i, 0u8, info))
        .chain(prose_i.iter().map(|&(i, info)| (i, 1, info)))
        .chain(link_i.iter().map(|&(i, info)| (i, 2, info)))
        .collect();
    let mut results = Vec::new();
    for (pos, &(vi, tier, has_info)) in ordered.iter().enumerate().take(MAX_RESULTS) {
        let rank = pos + 1;
        let v = &mut verdicts[vi];
        v.fate = match tier {
            0 => RowFate::SentDatum(rank),
            1 => RowFate::SentProse(rank),
            _ => RowFate::SentLink(rank),
        };
        results.push(SearchResult {
            title: v.title.clone(),
            url: v.url.clone(),
            snippet: if has_info { v.snippet.clone() } else { String::new() },
        });
    }
    (results, verdicts)
}

/// De-duplication key for a result URL. Normally the registrable host, so the
/// same source isn't repeated. **But** Baidu proxies every hit through
/// `www.baidu.com/link?url=<token>` — an identical host for every row — so plain
/// host-dedup collapses a whole Baidu SERP to a single result. For such redirect
/// wrappers, key on the unique redirect token instead, so distinct destinations
/// survive. Pure — unit-tested.
fn dedup_key(url: &str) -> String {
    if let Some(pos) = url.find("/link?url=") {
        return url[pos..].to_string();
    }
    host_of(url)
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
    // Bing prefixes captions with a crawl-freshness date ("3 天之前 · …",
    // "2025年5月23日 · …"). It's SERP chrome, not the result's own datum — strip
    // it so the quality tiering isn't fooled into treating a crawl date as
    // concrete data on an otherwise information-free snippet. Only strip when
    // followed by the "·" separator, so a date that IS the answer (e.g. a
    // release date leading the sentence) survives.
    t = freshness_prefix_re().replace(&t, "").into_owned();
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

/// Bing's `news_dt` freshness chip rendered into the caption text: an absolute
/// or relative date followed by a "·" separator, at the very start.
fn freshness_prefix_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^\s*(?:\d+\s*天之前|\d{4}\s*年\s*\d{1,2}\s*月\s*\d{1,2}\s*日?|昨天|今天)\s*[·•]\s*",
        )
        .expect("valid freshness regex")
    })
}

/// Registrable host of a URL (for de-duplication). Best-effort, no allocation of
/// a full URL parser: strips scheme, path, and a leading `www.`.
pub(crate) fn host_of(url: &str) -> String {
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
        let out = curate("上海天气", raw);
        assert_eq!(out.len(), MAX_RESULTS, "must cap at MAX_RESULTS");
        assert_eq!(out[0].url, "https://www.weather.com.cn/a");
        assert_eq!(out[1].url, "https://tianqi.com/x", "second weather.com.cn host deduped away");
        assert!(!out[0].snippet.contains("网页快照"), "boilerplate stripped");
    }

    #[test]
    fn clean_snippet_strips_bing_freshness_chrome_only() {
        assert_eq!(clean_snippet("3 天之前 · 福安天气预报详情"), "福安天气预报详情");
        assert_eq!(clean_snippet("2025年5月23日 · 福安市 今日天气 33°"), "福安市 今日天气 33°");
        assert_eq!(clean_snippet("今天 · 有雨"), "有雨");
        // A leading date **without** the "·" separator is the datum, not chrome.
        assert_eq!(
            clean_snippet("2025年3月14日 演唱会在东京巨蛋举行"),
            "2025年3月14日 演唱会在东京巨蛋举行"
        );
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

    #[test]
    fn dedup_key_uses_redirect_token_else_host() {
        // Baidu redirect → key on the unique token, not the shared host.
        assert_eq!(dedup_key("http://www.baidu.com/link?url=XYZ"), "/link?url=XYZ");
        // Ordinary URL → registrable host.
        assert_eq!(dedup_key("https://www.example.com/page"), "example.com");
    }

    #[test]
    fn curate_keeps_distinct_baidu_redirects_not_collapsed_by_host() {
        // Baidu wraps every hit in www.baidu.com/link?url=<token>; host-dedup would
        // collapse them all to one (the "only 1 Baidu result" bug). Distinct tokens
        // must survive.
        let raw = vec![
            RawResult { title: "汇率卡片".into(), url: "http://www.baidu.com/link?url=AAA".into(), snippet: "".into() },
            RawResult { title: "东方财富 实时汇率".into(), url: "http://www.baidu.com/link?url=BBB".into(), snippet: "1 CNY ≈ 21 JPY".into() },
        ];
        assert_eq!(curate("美元汇率", raw).len(), 2, "distinct baidu redirects must not be host-collapsed");

        // Same redirect token → still deduped to one.
        let dup = vec![
            RawResult { title: "美元汇率 A".into(), url: "http://www.baidu.com/link?url=SAME".into(), snippet: "".into() },
            RawResult { title: "美元汇率 B".into(), url: "http://www.baidu.com/link?url=SAME".into(), snippet: "".into() },
        ];
        assert_eq!(curate("美元汇率", dup).len(), 1, "identical redirect token deduped");
    }

    // ── quality/relevance curation (Task: "results are always useless") ─────
    //
    // These use the *real* boilerplate strings from the "福安今天天气如何" Bing dump
    // that prompted this work, so the test breaks if the filler ever leaks back in.

    #[test]
    fn curate_strips_seo_filler_and_drops_offtopic() {
        let raw = vec![
            // On-topic but pure SEO filler — names the topic, states no fact.
            RawResult {
                title: "福安天气预报,福安7天天气预报".into(),
                url: "https://www.weather.com.cn/weather/101230306.shtml".into(),
                snippet: "3 天之前 · 福安天气网为您提供福安天气预报24小时详情、福安今日天气预报，包括今日实时温度、24小时降水概率、湿度、pm2.5、风向、紫外线强度等，助您放心出行。".into(),
            },
            RawResult {
                title: "【福安今天天气预报】福安天气预报24小时详情_福安天气网".into(),
                url: "https://www.tianqi.com/fuan/today/".into(),
                snippet: "2 天之前 · 天气网提供福安天气预报15天,30天,今日天气,明天天气，旅游出行,从天气网开始！".into(),
            },
            // Outright off-topic, and carrying no quotable datum → dropped entirely.
            RawResult {
                title: "英伟达股价创新高 分析师看好".into(),
                url: "https://finance.example.com/nvda".into(),
                snippet: "英伟达盘中大涨，分析师纷纷上调目标价，市场情绪高涨。".into(),
            },
        ];
        let out = curate("福安今天天气如何", raw);
        assert!(
            out.iter().all(|r| !r.url.contains("finance.example.com")),
            "off-topic row must be dropped, got {out:?}"
        );
        assert!(!out.is_empty(), "on-topic weather links must survive");
        for r in &out {
            // The SEO tagline must never reach the prompt as if it were the answer.
            assert!(!r.snippet.contains("为您提供"), "filler leaked: {r:?}");
            assert!(!r.snippet.contains("旅游出行"), "filler leaked: {r:?}");
        }
    }

    #[test]
    fn curate_puts_datum_bearing_results_first() {
        // A datum-less topical link appears *before* a datum-bearing one in SERP
        // order; curation must still surface the one that carries the fact.
        let raw = vec![
            RawResult {
                title: "东京天气 - 天气网".into(),
                url: "https://a.example.com/tokyo".into(),
                snippet: "天气网为您提供东京天气预报，便捷查询。".into(), // filler → blanked
            },
            RawResult {
                title: "Tokyo Weather Today".into(),
                url: "https://b.example.com/tokyo".into(),
                snippet: "东京今天多云，气温 18~25℃，降水概率 20%。".into(), // real datum
            },
        ];
        let out = curate("东京天气", raw);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].url, "https://b.example.com/tokyo", "datum-bearing result first");
        assert!(out[0].snippet.contains("18~25℃"));
        assert!(out[1].snippet.is_empty(), "the filler snippet was blanked, link kept");
    }

    #[test]
    fn curate_keeps_crosslanguage_hit_that_carries_a_datum() {
        // Chinese query, English result whose text shares no query term but states
        // the fact — the concrete datum keeps it (no false-drop).
        let raw = vec![RawResult {
            title: "Tokyo Weather - AccuWeather".into(),
            url: "https://accuweather.example.com/tokyo".into(),
            snippet: "Cloudy, 18°C in Tokyo now.".into(),
        }];
        let out = curate("东京天气", raw);
        assert_eq!(out.len(), 1, "datum-bearing cross-language hit kept: {out:?}");
    }

    // ── query-aware datum fixes (from the live quality-report failure modes) ─

    #[test]
    fn curate_weather_query_rejects_encyclopedia_datum() {
        // BingCn·天气 failure: the SERP led with 百科 rows about the CITY, whose
        // population/date figures counted as "datum" and were sent as if they
        // answered a weather question. Wrong-type data must not win the slot.
        let raw = vec![
            RawResult {
                title: "福安市".into(),
                url: "https://baike.example.com/fuan".into(),
                snippet: "截至2025年7月，福安市辖4个街道、13个镇、5个乡。截至2024年末，福安市总人口668179人。".into(),
            },
            RawResult {
                title: "福安实时天气".into(),
                url: "https://weather.example.com/fuan".into(),
                snippet: "福安今天多云，气温 28~37℃，南风3级。".into(),
            },
        ];
        let (out, verdicts) = curate_report("福安今天天气如何", raw);
        assert_eq!(out[0].url, "https://weather.example.com/fuan", "weather datum first: {out:?}");
        assert!(!verdicts[0].has_datum, "population figures are not weather data");
        assert_eq!(verdicts[0].fate, RowFate::SentProse(2), "encyclopedia demoted to prose");
        assert_eq!(verdicts[1].fate, RowFate::SentDatum(1));
    }

    #[test]
    fn curate_counts_title_datum_and_blanks_its_filler_snippet() {
        // Bing·股价 failure: the price lived in the TITLE ("194.83 (-1.39%)")
        // above an SEO snippet; snippet-only datum detection sent priceless
        // portal blurbs instead. The title must promote the row — without its
        // filler snippet riding into the prompt.
        let raw = vec![
            RawResult {
                title: "英伟达 (NVDA)股票股价_行情 - 雪球".into(),
                url: "https://xueqiu.example.com/NVDA".into(),
                snippet: "雪球为您提供英伟达行情走势、财报及实时数据分析，便捷查询。".into(),
            },
            RawResult {
                title: "英伟达公司 194.83 (-1.39%)_美股_新浪财经".into(),
                url: "https://sina.example.com/NVDA".into(),
                snippet: "新浪财经-美股频道为您提供英伟达股票股价,股票实时行情,新闻,财报,美股实时交易数据,便捷查询。".into(),
            },
        ];
        let (out, verdicts) = curate_report("英伟达股价", raw);
        assert_eq!(out[0].url, "https://sina.example.com/NVDA", "title price promotes: {out:?}");
        assert!(out[0].title.contains("194.83"), "the datum reaches the model via the title");
        assert!(out[0].snippet.is_empty(), "its filler snippet must be blanked: {out:?}");
        assert_eq!(verdicts[1].fate, RowFate::SentDatum(1));
    }

    #[test]
    fn curate_offtopic_datum_ranks_below_relevant_datum() {
        // Google·GTA6 failure: Rockstar's SUPERSEDED May-26 post (which shares no
        // query term) outranked the current Nov-19 answer purely because both had
        // dates. Off-topic datum rows ride in the prose tier now.
        let raw = vec![
            RawResult {
                title: "Grand Theft Auto VI 推出时间现为2026 年5 月26 日".into(),
                url: "https://rockstar.example.com/may".into(),
                snippet: "大家好，Grand Theft Auto VI 现已定在2026 年5 月26 日推出。".into(),
            },
            RawResult {
                title: "R 星宣布《GTA 6》再次跳票至2026 年11月19日发售".into(),
                url: "https://zhihu.example.com/nov".into(),
                snippet: "《侠盗猎车手VI》现定于2026年11月19日发售，预购6月25日开启。".into(),
            },
        ];
        let (out, verdicts) = curate_report("GTA6 发售日期", raw);
        assert_eq!(out[0].url, "https://zhihu.example.com/nov", "relevant datum first: {out:?}");
        assert_eq!(verdicts[1].fate, RowFate::SentDatum(1));
        assert_eq!(verdicts[0].fate, RowFate::SentProse(2), "off-topic datum kept, but never #1");
    }

    // ── per-engine SERP-quality suite ────────────────────────────────────────
    //
    // End-to-end (parse_rendered → curate) for every engine. Each fixture mixes,
    // like a real SERP: a sponsored/ad row, SEO-filler rows, ONE row whose
    // snippet states the real-time datum, and an off-topic row. The pipeline
    // must, for all 5 engines:
    //   1. relevance — every surfaced hit is on-topic for the query;
    //   2. real-time data — the datum-bearing hit comes FIRST, with its fields
    //      (temperature / humidity / wind …) intact and its REAL destination URL
    //      (tracking redirect unwrapped);
    //   3. no junk/ads — sponsored rows and off-topic rows never appear, and SEO
    //      filler is never fed to the model as a snippet.

    struct QualityCase {
        engine: SearchEngine,
        query: &'static str,
        html: &'static str,
        /// The datum-bearing hit that must surface first: (real URL, title).
        want_url: &'static str,
        want_title: &'static str,
        /// Real-time fields that must survive into the first snippet.
        want_fields: &'static [&'static str],
        /// Fragments that must appear in NO surfaced result (ads / off-topic /
        /// filler / SERP chrome).
        banned: &'static [&'static str],
    }

    fn assert_serp_quality(c: &QualityCase) {
        let raw = parse_rendered(c.engine, c.html);
        let out = curate(c.query, raw);
        assert!(!out.is_empty(), "{:?}: pipeline produced no results", c.engine);
        assert_eq!(out[0].url, c.want_url, "{:?}: datum hit must be first: {out:#?}", c.engine);
        assert_eq!(out[0].title, c.want_title, "{:?}: title field", c.engine);
        for f in c.want_fields {
            assert!(
                out[0].snippet.contains(f),
                "{:?}: real-time field {f:?} missing from snippet {:?}",
                c.engine,
                out[0].snippet
            );
        }
        for r in &out {
            assert!(r.url.starts_with("http"), "{:?}: unusable URL {:?}", c.engine, r.url);
            for b in c.banned {
                assert!(
                    !r.title.contains(b) && !r.url.contains(b) && !r.snippet.contains(b),
                    "{:?}: banned fragment {b:?} leaked into {r:?}",
                    c.engine
                );
            }
        }
    }

    #[test]
    fn serp_quality_bing_from_real_dump_rows() {
        // The b_algo rows are VERBATIM from a real cn.bing dump of the query
        // "福安今天天气如何" (the exact case the user reported as useless): two
        // SEO-filler rows, then The Weather Channel row that actually carries the
        // conditions. An ad block and an off-topic row are added around them.
        let html = r##"<div id="b_results">
          <li class="b_ad"><ul><li class="sb_add sb_adTA"><h2><a href="https://www.bing.com/aclick?ld=e8xy&u=https%3a%2f%2fad.example.com%2fbooking">福安酒店特惠预订</a></h2><div class="b_caption"><p>超值优惠，立即预订福安酒店。</p></div></li></ul></li>
          <li class="b_algo"><h2 class=""><a target="_blank" href="https://www.bing.com/ck/a?!&amp;&amp;p=aa857730e6d4cdf03fc4e6918bf9c7ccd9c7e29c41b851b5e54b6dcc36e53755JmltdHM9MTc4Mjk1MDQwMA&amp;ptn=3&amp;ver=2&amp;hsh=4&amp;fclid=328499a2-8bf8-6a7a-1859-8e298ac36b44&amp;psq=%e7%a6%8f%e5%ae%89%e4%bb%8a%e5%a4%a9%e5%a4%a9%e6%b0%94%e5%a6%82%e4%bd%95&amp;u=a1aHR0cHM6Ly93d3cudGlhbnFpLmNvbS9mdWFuL3RvZGF5Lw&amp;ntb=1" h="ID=SERP,5179.2">【福安今天天气预报】福安天气预报24小时详情_福安天气网</a></h2><div class="b_caption"><p class="b_lineclamp2"><span class="news_dt">3 天之前</span>&nbsp;· 福安天气网为您提供福安天气预报24小时详情、福安今日天气预报，包括今日实时温度、24小时降水概率、湿度、pm2.5、风向、紫外线强度等，助您放心出行。</p></div></li>
          <li class="b_algo"><h2 class=""><a target="_blank" href="https://www.bing.com/ck/a?!&amp;&amp;p=928fd800943fc7e0ef3ae3179727c8cbe64ff5d59e11c2899544657d1554f4a0JmltdHM9MTc4Mjk1MDQwMA&amp;ptn=3&amp;ver=2&amp;hsh=4&amp;fclid=328499a2-8bf8-6a7a-1859-8e298ac36b44&amp;psq=%e7%a6%8f%e5%ae%89%e4%bb%8a%e5%a4%a9%e5%a4%a9%e6%b0%94%e5%a6%82%e4%bd%95&amp;u=a1aHR0cHM6Ly93d3cud2VhdGhlci5jb20uY24vd2VhdGhlcjQwZG4vMTAxMjMwMzA2LnNodG1s&amp;ntb=1" h="ID=SERP,5220.2">【福安天气】福安今天天气预报,今天,今天天气,7天,15天天气 ...</a></h2><div class="b_caption"><p class="b_lineclamp2">福安天气预报，及时准确发布中央气象台天气信息，便捷查询北京今日天气，福安周末天气，福安一周天气预报，福安15日天气预报，福安40日天气预报，福安天气预报还提供福安各区县的生活指数、健康 …</p></div></li>
          <li class="b_algo"><h2 class=""><a target="_blank" href="https://www.bing.com/ck/a?!&amp;&amp;p=9f79dab656e5c9fae261980786c630d07e47374b7237a358fdd6f7d289172c56JmltdHM9MTc4Mjk1MDQwMA&amp;ptn=3&amp;ver=2&amp;hsh=4&amp;fclid=328499a2-8bf8-6a7a-1859-8e298ac36b44&amp;psq=%e7%a6%8f%e5%ae%89%e4%bb%8a%e5%a4%a9%e5%a4%a9%e6%b0%94%e5%a6%82%e4%bd%95&amp;u=a1aHR0cHM6Ly93ZWF0aGVyLmNvbS96aC1DTi93ZWF0aGVyL3RvZGF5L2wvOWU3YmQzZDE5NzYxYTA0OTM3YzYwNTIzODZmNjczZTk4M2M0OTJhZDYxOTUyYmI5Mjg1Mjk0NDM2NjkzYjQ2OQ&amp;ntb=1" h="ID=SERP,5240.2">福安市, 福建省天气预报和情况 - The Weather Channel</a></h2><div class="b_caption"><p class="b_lineclamp2"><span class="news_dt">2025年5月23日</span>&nbsp;· 福安市, 福建省 今日天气 体感温度33° 5:09 18:46 高 / 低 -- / 23° 大风 5公里/小时 湿度 90% 露点 26° 气压 1002.7毫巴 紫外线指数 2（最大值11） 能见度 8.05 ...</p></div></li>
          <li class="b_algo"><h2 class=""><a target="_blank" href="https://finance.example.com/nvda">英伟达股价创新高，分析师上调目标价</a></h2><div class="b_caption"><p class="b_lineclamp2">英伟达盘中大涨，市场情绪高涨，机构纷纷看多。</p></div></li>
        </div>"##;
        for engine in [SearchEngine::BingCn, SearchEngine::Bing] {
            let case = QualityCase {
                engine,
                query: "福安今天天气如何",
                html,
                // Base64 `u=` param unwrapped to the real destination.
                want_url: "https://weather.com/zh-CN/weather/today/l/9e7bd3d19761a04937c6052386f673e983c492ad61952bb9285294436693b469",
                want_title: "福安市, 福建省天气预报和情况 - The Weather Channel",
                want_fields: &["体感温度33°", "湿度 90%", "露点 26°", "5公里/小时"],
                banned: &[
                    "aclick", "ad.example.com", "finance.example.com", // ad + off-topic
                    "为您提供", "便捷查询", // SEO filler must not be fed
                    "2025年5月23日", // crawl-date chrome must be stripped
                ],
            };
            assert_serp_quality(&case);
            // The junk rows may survive as bare topical links — but only with
            // their filler snippet blanked.
            let out = curate(case.query, parse_rendered(engine, html));
            for r in out.iter().skip(1) {
                assert!(r.snippet.is_empty(), "{engine:?}: junk snippet fed: {r:?}");
            }
        }
    }

    #[test]
    fn serp_quality_google() {
        // Layout-faithful fixture: ads live under #tads (never div.g), organics
        // under #rso with /url?q= redirects and VwiC3b snippets.
        let html = r#"
          <div id="tads"><div data-text-ad="1"><a href="https://www.googleadservices.com/pagead/aclk?sa=L&adurl=https://ad.example.com/tokyo-hotel"><div role="heading">东京酒店 5折特惠</div></a><div>广告 · 立即预订东京酒店。</div></div></div>
          <div id="rso">
            <div class="g"><div class="wrap"><a href="/url?q=https://tianqi-portal.example.cn/tokyo&sa=U"><h3>东京天气预报查询 - 天气门户</h3></a><div class="VwiC3b">天气门户为您提供东京天气预报查询服务，便捷出行，欢迎访问官方网站。</div></div></div>
            <div class="g"><div class="wrap"><a href="/url?q=https://tenki.example.jp/forecast/3/16/&sa=U"><h3>東京の天気 - tenki.jp</h3></a><div class="VwiC3b">东京 今天多云转晴，气温 18~25℃，湿度 60%，北东风 3级，降水概率 20%。</div></div></div>
            <div class="g"><div class="wrap"><a href="/url?q=https://finance.example.com/nvda&sa=U"><h3>英伟达股价创新高</h3></a><div class="VwiC3b">盘中大涨，机构纷纷看多。</div></div></div>
          </div>"#;
        assert_serp_quality(&QualityCase {
            engine: SearchEngine::Google,
            query: "东京天气",
            html,
            want_url: "https://tenki.example.jp/forecast/3/16/",
            want_title: "東京の天気 - tenki.jp",
            want_fields: &["气温 18~25℃", "湿度 60%", "3级"],
            banned: &["adurl", "ad.example.com", "finance.example.com", "为您提供", "立即预订"],
        });
    }

    #[test]
    fn serp_quality_baidu() {
        // Modern Baidu markup: real URL in `mu`, snippet in data-module=abstract,
        // sponsored row marked data-tuiguang + 广告 badge.
        let html = r#"<div id="content_left">
            <div class="result c-container" data-tuiguang="1" mu="https://ad.example.com/tokyo">
              <h3><a href="http://www.baidu.com/link?url=AD1">东京旅游特价套餐</a></h3>
              <span class="ec-tuiguang-label">广告</span>
              <div data-module="abstract"><span class="summary-text_9">低价预订，立即抢购。</span></div></div>
            <div class="result c-container" mu="https://tianqi-portal.example.cn/tokyo">
              <h3><a href="http://www.baidu.com/link?url=J1">东京天气预报一周_天气门户</a></h3>
              <div data-module="abstract"><span class="summary-text_a">天气门户为您提供东京天气预报查询，便捷出行，助您放心安排行程。</span></div></div>
            <div class="result c-container" mu="https://weathernews.example.jp/tokyo">
              <h3><a href="http://www.baidu.com/link?url=D1">东京实时天气</a></h3>
              <div data-module="abstract"><span class="summary-text_b">东京 今天多云转晴，气温 18~25℃，湿度 60%，北东风 3级。</span></div></div>
            <div class="result c-container" mu="https://finance.example.com/nvda">
              <h3><a href="http://www.baidu.com/link?url=O1">英伟达股价创新高</a></h3>
              <div data-module="abstract"><span class="summary-text_c">盘中大涨，机构纷纷看多。</span></div></div>
          </div>"#;
        assert_serp_quality(&QualityCase {
            engine: SearchEngine::Baidu,
            query: "东京天气",
            html,
            // `mu` (real destination), not the baidu.com/link redirect.
            want_url: "https://weathernews.example.jp/tokyo",
            want_title: "东京实时天气",
            want_fields: &["气温 18~25℃", "湿度 60%", "3级"],
            banned: &["ad.example.com", "finance.example.com", "为您提供", "立即抢购", "广告"],
        });
    }

    #[test]
    fn serp_quality_duckduckgo() {
        // html.duckduckgo.com layout: sponsored row = result--ad + y.js redirect;
        // organics carry uddg-wrapped destinations.
        let html = r#"<div id="links">
            <div class="result results_links result--ad">
              <a class="result__a" href="https://duckduckgo.com/y.js?ad_provider=bingv7aa&u3=https%3A%2F%2Fad.example.com%2Fhotel">东京酒店特惠</a>
              <div class="result__snippet">立即预订东京酒店，超值优惠。</div></div>
            <div class="result results_links">
              <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Ftianqi-portal.example.cn%2Ftokyo&rut=a">东京天气预报查询 - 天气门户</a>
              <div class="result__snippet">天气门户为您提供东京天气预报查询，便捷出行，欢迎访问。</div></div>
            <div class="result results_links">
              <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Ftenki.example.jp%2Ftokyo&rut=b">Tokyo Weather Now</a>
              <div class="result__snippet">东京 今天多云，气温 18~25℃，湿度 60%，北东风 3级。</div></div>
            <div class="result results_links">
              <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Ffinance.example.com%2Fnvda&rut=c">NVIDIA stock hits record</a>
              <div class="result__snippet">Shares rally as analysts cheer the results.</div></div>
          </div>"#;
        assert_serp_quality(&QualityCase {
            engine: SearchEngine::DuckDuckGo,
            query: "东京天气",
            html,
            // uddg param unwrapped to the real destination.
            want_url: "https://tenki.example.jp/tokyo",
            want_title: "Tokyo Weather Now",
            want_fields: &["气温 18~25℃", "湿度 60%", "3级"],
            banned: &["y.js", "ad.example.com", "finance.example.com", "为您提供", "立即预订"],
        });
    }

    /// Diagnostic, not CI: runs the real pipeline over the SERP dumps the app
    /// wrote via `MUTSUMI_SERP_DUMP` and prints what the model would receive.
    ///   cargo test --lib real_dump_report -- --ignored --nocapture
    /// The query must match the one used when the dump was captured (it's in the
    /// dump's <title>).
    #[test]
    #[ignore = "reads real serp-dump-*.html from the app log dir"]
    fn real_dump_report() {
        let dir = std::path::Path::new(&std::env::var("LOCALAPPDATA").unwrap_or_default())
            .join("com.mutsumi.app")
            .join("logs");
        for (file, engine, query) in [
            ("serp-dump-Bing.html", SearchEngine::Bing, "福安今天天气如何"),
            ("serp-dump-BingCn.html", SearchEngine::BingCn, "福安今天天气如何"),
            ("serp-dump-Google.html", SearchEngine::Google, "东京天气"),
            ("serp-dump-Baidu.html", SearchEngine::Baidu, "查询一下宁德今天气温范围"),
            ("serp-dump-DuckDuckGo.html", SearchEngine::DuckDuckGo, "东京天气"),
        ] {
            let Ok(html) = std::fs::read_to_string(dir.join(file)) else {
                println!("== {engine:?}: no dump ({file})");
                continue;
            };
            let raw = parse_rendered(engine, &html);
            println!("\n== {engine:?} «{query}» — {} raw rows", raw.len());
            for r in &raw {
                let t: String = r.title.chars().take(34).collect();
                let s: String = r.snippet.chars().take(44).collect();
                println!("   raw | {t} | {} | {s}", host_of(&r.url));
            }
            for (i, r) in curate(query, raw).iter().enumerate() {
                println!("   OUT #{} | {} | {} | {}", i + 1, r.title, r.url, r.snippet);
            }
        }
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
