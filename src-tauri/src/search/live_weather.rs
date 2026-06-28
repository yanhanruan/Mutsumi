//! Structured live-weather lookup — the root-cause fix for "weather for some
//! cities (福安, a small county-level city) isn't supported".
//!
//! Weather questions used to be answered by scraping a search engine's result
//! page. That is fundamentally unreliable for this: modern SERPs are
//! JavaScript-rendered SPAs behind anti-bot defences (Cloudflare challenges,
//! CAPTCHA, TLS/JA3 fingerprinting), so the static `reqwest` client only ever
//! receives a blank or HTTP-202 challenge page. Coverage was therefore random
//! and small cities appeared "unsupported" — it was never about the city.
//!
//! Instead we hit a dedicated weather service (`wttr.in`) that answers a single
//! stateless HTTPS GET with **structured JSON** (`format=j1`): no JavaScript, no
//! anti-bot challenge, its own geocoder (resolves county-level Chinese cities),
//! no API key. The result is formatted into the same labeled real-time-context
//! block the chat prompt already understands. On any failure the caller falls
//! back to the generic search path, so chat is never blocked.

use std::time::Duration;

use serde::Deserialize;

/// wttr.in per-request timeout. The service answers in ~1–4s; keep this tight so
/// a slow lookup degrades to the search fallback instead of stalling the turn.
const WTTR_TIMEOUT: Duration = Duration::from_secs(6);

/// Weather-intent markers; if none is present the message isn't a weather query.
const WEATHER_MARKERS: &[&str] = &["天气", "气温", "weather"];

/// CJK tokens stripped to isolate the location (Chinese has no word boundaries,
/// so these are removed as substrings — longest first).
const NOISE_CJK: &[&str] = &[
    "今天", "今日", "明天", "后天", "昨天", "现在", "目前", "近期", "未来",
    "查询", "查一下", "查查", "搜索", "搜一下", "帮我", "告诉我", "看看", "一下", "请问", "想知道",
    "怎么样", "怎样", "如何", "几度", "多少度", "多少", "温度", "预报", "情况", "状况",
    "天气", "气温", "的", "了", "吗", "呢", "啊", "是", "查", "搜",
];
/// ASCII tokens stripped to isolate the location (matched whole-word,
/// case-insensitively).
const NOISE_ASCII: &[&str] = &[
    "weather", "today", "tomorrow", "yesterday", "tonight", "now", "current", "currently",
    "what", "whats", "what's", "is", "the", "in", "at", "of", "for", "please", "tell", "me",
    "how", "about", "s",
];

/// A compact, structured current-weather summary.
#[derive(Debug, Clone, PartialEq)]
pub struct WeatherBrief {
    pub location: String,
    pub temp_c: String,
    pub feels_like_c: String,
    pub description: String,
    pub humidity: String,
}

/// Detect a weather question and extract its location, or `None` if the message
/// isn't a weather query / no location can be isolated. Pure — unit-tested.
pub fn parse_weather_query(msg: &str) -> Option<String> {
    let lower = msg.to_lowercase();
    let is_weather = WEATHER_MARKERS
        .iter()
        .any(|m| if m.is_ascii() { lower.contains(m) } else { msg.contains(m) });
    if !is_weather {
        return None;
    }

    // Punctuation → spaces (keep alphanumerics — including CJK — and the
    // apostrophe in contractions like "what's").
    let spaced: String = msg
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '\'' { c } else { ' ' })
        .collect();

    // Strip CJK noise as substrings, longest first so multi-char verbs are
    // removed before any single-char piece.
    let mut cjk = NOISE_CJK.to_vec();
    cjk.sort_by_key(|w| std::cmp::Reverse(w.chars().count()));
    let mut s = spaced;
    for w in cjk {
        s = s.replace(w, " ");
    }

    // Drop ASCII noise tokens (whole-word, case-insensitive); keep original case.
    let city = s
        .split_whitespace()
        .filter(|tok| !NOISE_ASCII.contains(&tok.to_lowercase().as_str()))
        .collect::<Vec<_>>()
        .join(" ");
    let city = city.trim();

    // Reject empties, over-long noise, and anything a marker survived in (which
    // means extraction failed) — the caller then falls back to generic search.
    if city.is_empty()
        || city.chars().count() > 20
        || WEATHER_MARKERS.iter().any(|m| city.to_lowercase().contains(m))
    {
        None
    } else {
        Some(city.to_string())
    }
}

// ── wttr.in `format=j1` deserialisers ───────────────────────────────

#[derive(Deserialize)]
struct WttrRoot {
    current_condition: Vec<WttrCurrent>,
    #[serde(default)]
    nearest_area: Vec<WttrArea>,
}

#[derive(Deserialize)]
struct WttrCurrent {
    #[serde(rename = "temp_C")]
    temp_c: String,
    #[serde(rename = "FeelsLikeC", default)]
    feels_like_c: String,
    #[serde(rename = "weatherDesc", default)]
    weather_desc: Vec<WttrValue>,
    #[serde(rename = "lang_zh", default)]
    lang_zh: Vec<WttrValue>,
    #[serde(default)]
    humidity: String,
}

#[derive(Deserialize)]
struct WttrArea {
    #[serde(rename = "areaName", default)]
    area_name: Vec<WttrValue>,
    #[serde(default)]
    region: Vec<WttrValue>,
    #[serde(default)]
    country: Vec<WttrValue>,
}

#[derive(Deserialize)]
struct WttrValue {
    value: String,
}

fn first(v: &[WttrValue]) -> &str {
    v.first().map(|x| x.value.as_str()).unwrap_or("")
}

/// Parse wttr.in `format=j1` JSON into a [`WeatherBrief`], or `None` if the
/// payload carries no current conditions. Pure — unit-tested against a fixture.
pub fn parse_wttr(json: &str) -> Option<WeatherBrief> {
    let root: WttrRoot = serde_json::from_str(json).ok()?;
    let cur = root.current_condition.first()?;

    // Prefer the localized description when present and non-empty.
    let zh = first(&cur.lang_zh);
    let en = first(&cur.weather_desc);
    let description = if !zh.is_empty() { zh } else { en }.trim().to_string();

    let location = root
        .nearest_area
        .first()
        .map(|a| {
            [first(&a.area_name), first(&a.region), first(&a.country)]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("·")
        })
        .unwrap_or_default();

    Some(WeatherBrief {
        location,
        temp_c: cur.temp_c.trim().to_string(),
        feels_like_c: cur.feels_like_c.trim().to_string(),
        description,
        humidity: cur.humidity.trim().to_string(),
    })
}

/// Format a [`WeatherBrief`] into the labeled real-time-context block injected
/// into the chat prompt (same shape as `search::format_context`).
pub fn format_weather_context(query: &str, w: &WeatherBrief) -> String {
    let mut s = String::from("以下是检索到的实时天气数据，仅供参考，与问题无关请忽略。\n\n");
    s.push_str(&format!("【天气：{query}】\n"));
    if !w.location.is_empty() {
        s.push_str(&format!("地点：{}\n", w.location));
    }
    s.push_str(&format!("当前气温：{}°C", w.temp_c));
    if !w.feels_like_c.is_empty() {
        s.push_str(&format!("（体感 {}°C）", w.feels_like_c));
    }
    s.push('\n');
    if !w.description.is_empty() {
        s.push_str(&format!("天气状况：{}\n", w.description));
    }
    if !w.humidity.is_empty() {
        s.push_str(&format!("相对湿度：{}%\n", w.humidity));
    }
    s.push_str("数据来源：wttr.in\n");
    s
}

/// Percent-encode a location for a wttr.in path segment (spaces → `+`).
fn encode_path(city: &str) -> String {
    let mut out = String::new();
    for b in city.trim().bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Look up `city` from wttr.in and format it as a chat-context block, or `None`
/// on any failure (caller falls back to generic search). The reqwest client is
/// shared with the search subsystem.
pub async fn fetch_weather_context(
    http: &reqwest::Client,
    query: &str,
    city: &str,
) -> Option<String> {
    let url = format!("https://wttr.in/{}?format=j1&lang=zh", encode_path(city));
    let resp = http.get(&url).timeout(WTTR_TIMEOUT).send().await.ok()?;
    if !resp.status().is_success() {
        log::warn!("live_weather: wttr.in status {} for {city:?}", resp.status());
        return None;
    }
    let json = resp.text().await.ok()?;
    let brief = parse_wttr(&json)?;
    // A bare geocode resolve with no temperature is useless — treat as a miss.
    if brief.temp_c.is_empty() {
        return None;
    }
    Some(format_weather_context(query, &brief))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── city extraction ──────────────────────────────────────────

    #[test]
    fn extracts_city_from_chinese_weather_questions() {
        // The exact phrasings from the v1.3.0 bug report screenshot.
        assert_eq!(parse_weather_query("今天福安天气几度？").as_deref(), Some("福安"));
        assert_eq!(parse_weather_query("查询一下福安今天的天气").as_deref(), Some("福安"));
        assert_eq!(parse_weather_query("上海今天天气怎么样").as_deref(), Some("上海"));
        assert_eq!(parse_weather_query("宁德天气今天怎么样").as_deref(), Some("宁德"));
        assert_eq!(parse_weather_query("嵊州天气").as_deref(), Some("嵊州"));
    }

    #[test]
    fn extracts_city_from_english_weather_questions() {
        assert_eq!(parse_weather_query("London weather today").as_deref(), Some("London"));
        assert_eq!(parse_weather_query("what's the weather in Tokyo").as_deref(), Some("Tokyo"));
    }

    #[test]
    fn non_weather_messages_are_ignored() {
        assert_eq!(parse_weather_query("你在做什么呢"), None);
        assert_eq!(parse_weather_query("帮我查东京到大阪的票价"), None);
        assert_eq!(parse_weather_query("hello how are you"), None);
    }

    #[test]
    fn weather_question_without_a_city_yields_none() {
        assert_eq!(parse_weather_query("今天天气怎么样"), None);
    }

    // ── wttr.in JSON parsing ─────────────────────────────────────

    const FUAN_J1: &str = r#"{
        "current_condition":[{"temp_C":"21","FeelsLikeC":"16","humidity":"100",
            "weatherDesc":[{"value":"Light rain shower"}],"lang_zh":[{"value":"小阵雨"}]}],
        "nearest_area":[{"areaName":[{"value":"Hanyang"}],"region":[{"value":"Fujian"}],
            "country":[{"value":"China"}]}]
    }"#;

    #[test]
    fn parses_wttr_j1_current_conditions() {
        let w = parse_wttr(FUAN_J1).expect("fixture should parse");
        assert_eq!(w.temp_c, "21");
        assert_eq!(w.feels_like_c, "16");
        assert_eq!(w.humidity, "100");
        assert_eq!(w.description, "小阵雨", "localized description preferred");
        assert!(w.location.contains("Fujian"), "location was {:?}", w.location);
    }

    #[test]
    fn parse_wttr_rejects_garbage() {
        assert!(parse_wttr("not json").is_none());
        assert!(parse_wttr(r#"{"current_condition":[]}"#).is_none());
    }

    #[test]
    fn formats_weather_context_block() {
        let w = parse_wttr(FUAN_J1).unwrap();
        let ctx = format_weather_context("今天福安天气几度？", &w);
        assert!(ctx.contains("【天气：今天福安天气几度？】"));
        assert!(ctx.contains("21°C"));
        assert!(ctx.contains("小阵雨"));
        assert!(ctx.contains("wttr.in"));
    }

    //   cargo test --lib search::live_weather::tests::live_wttr_smoke -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "real network call to wttr.in (no anti-bot); manual sanity check"]
    async fn live_wttr_smoke() {
        // The cities that previously appeared "unsupported" via search scraping —
        // county-level CN cities + an international one — must all resolve from
        // the structured source. wttr.in rate-limits bursts (unlike real usage,
        // which is one request per turn), so space requests out and retry a miss
        // once before failing.
        let http = reqwest::Client::builder().build().unwrap();
        let cities = ["今天福安天气几度", "宁德天气", "嵊州天气", "邛崃天气", "London weather"];
        let mut misses = Vec::new();
        for q in cities {
            let city = parse_weather_query(q).expect("should be a weather query");
            let mut ctx = fetch_weather_context(&http, q, &city).await;
            if ctx.is_none() {
                tokio::time::sleep(Duration::from_secs(3)).await;
                ctx = fetch_weather_context(&http, q, &city).await;
            }
            println!("{q} -> [{city}] -> {}", if ctx.is_some() { "ok" } else { "MISS" });
            if ctx.is_none() {
                misses.push(q);
            }
            tokio::time::sleep(Duration::from_millis(1500)).await;
        }
        assert!(misses.is_empty(), "no structured weather for: {misses:?}");
    }
}
