//! Result-quality scoring — turn a page of raw SERP hits into the few that
//! actually answer the query. Pure + synchronous → unit-tested.
//!
//! The SERP hands us titles + snippets, but a lot of what it hands us is noise:
//!   * **SEO boilerplate** — "X天气网为您提供…24小时详情…助您放心出行", "…15天,30天,
//!     40天…旅游出行,从…开始！". It names the topic without ever stating the fact.
//!   * **Off-topic hits** — rows that share nothing with the query.
//! Feeding either to the model is worse than nothing: it invites a confident
//! non-answer ("福安天气网为您提供…" → the model parrots the site's tagline).
//!
//! So [`super::curate`] uses this module to (1) require a hit to be *on-topic*
//! for the query, and (2) reject *low-information* snippets so only a snippet
//! that carries a concrete datum survives — a topical link with no datum is
//! kept, but its filler snippet is dropped rather than fed as an answer.

use std::sync::OnceLock;

use regex::Regex;

/// Fold common traditional-Chinese characters to simplified so relevance and
/// datum matching work across scripts (Yahoo HK's 英偉達/股價 must match a query's
/// 英伟达/股价). Also folds Japanese **shinjitai** variants (気/発/売…) so a
/// ja-locale query like「東京の天気」classifies and matches like 东京天气 — the
/// app ships a ja UI, so ja queries are first-class. A curated table of
/// high-frequency chars, not a full converter — misses degrade to the old
/// behavior, never break it.
pub fn fold_zh(s: &str) -> String {
    fn fold(c: char) -> char {
        match c {
            // Japanese shinjitai → simplified Chinese (distinct code points from
            // the traditional forms below: 気≠氣, 発≠發, 売≠賣).
            '気' => '气', '発' => '发', '売' => '售', '対' => '对', '楽' => '乐',
            '駅' => '站', '図' => '图', '広' => '广',
            // More shinjitai that unlock search vocab: 価格/株価, 検索, 経済…
            '価' => '价', '検' => '检', '経' => '经', '実' => '实', '変' => '变',
            '転' => '转', '覚' => '觉', '応' => '应', '単' => '单', '続' => '续',
            '覧' => '览', '圧' => '压', '険' => '险', '営' => '营', '県' => '县',
            '東' => '东', '氣' => '气', '溫' => '温', '濕' => '湿', '雲' => '云',
            '陰' => '阴', '風' => '风', '預' => '预', '報' => '报', '週' => '周',
            '幾' => '几', '號' => '号', '誰' => '谁', '燈' => '灯', '樂' => '乐',
            '隊' => '队', '專' => '专', '輯' => '辑', '聲' => '声', '優' => '优',
            '價' => '价', '匯' => '汇', '幣' => '币', '圓' => '圆', '錢' => '钱',
            '銀' => '银', '買' => '买', '賣' => '卖', '產' => '产', '業' => '业',
            '資' => '资', '訊' => '讯', '聞' => '闻', '網' => '网', '頁' => '页',
            '電' => '电', '腦' => '脑', '機' => '机', '遊' => '游', '戲' => '戏',
            '點' => '点', '線' => '线', '車' => '车', '幹' => '干', '時' => '时',
            '間' => '间', '發' => '发', '開' => '开', '關' => '关', '門' => '门',
            '問' => '问', '題' => '题', '學' => '学', '國' => '国', '會' => '会',
            '舉' => '举', '辦' => '办', '應' => '应', '該' => '该', '說' => '说',
            '話' => '话', '語' => '语', '讀' => '读', '寫' => '写', '聽' => '听',
            '見' => '见', '覺' => '觉', '觀' => '观', '體' => '体', '麼' => '么',
            '們' => '们', '對' => '对', '臺' => '台', '灣' => '湾', '錄' => '录',
            '記' => '记', '偉' => '伟', '達' => '达', '華' => '华', '為' => '为',
            '與' => '与', '於' => '于', '後' => '后', '級' => '级', '陣' => '阵',
            other => other,
        }
    }
    s.chars().map(fold).collect()
}

/// Query terms for relevance matching: lowercased latin tokens (≥2 chars) plus
/// CJK bigrams (adjacent CJK-term-char pairs within a run; a lone char is kept
/// as-is). Bigrams beat single chars — "天气" is a term, "天"/"气" alone over-match.
/// "CJK term char" is kanji/hanzi **and katakana** (Japanese foreign proper nouns
/// like ブルーアーカイブ) — see [`is_cjk_term_char`]. Terms are built from the
/// trad→simp folded query (see [`fold_zh`]); topic-free function words are dropped
/// (see [`STOP_BIGRAMS`] for CJK, [`STOP_LATIN`] for English).
#[derive(Debug, Default)]
pub struct QueryTerms {
    latin: Vec<String>,
    cjk: Vec<String>,
}

impl QueryTerms {
    fn is_empty(&self) -> bool {
        self.latin.is_empty() && self.cjk.is_empty()
    }
}

/// A char that can anchor a CJK relevance term: Han (kanji/hanzi) or katakana.
/// Katakana is included because Japanese renders foreign proper nouns —
/// ブルーアーカイブ, ポケモン — in katakana, and those are strong topic anchors.
/// Hiragana is deliberately **excluded**: it is mostly particles and inflection
/// (の/は/を…), which would spawn noise bigrams (京の, の天); Japanese content
/// words are overwhelmingly kanji or katakana.
fn is_cjk_term_char(c: char) -> bool {
    matches!(c as u32,
        0x3400..=0x9FFF        // CJK unified ideographs (kanji / hanzi)
      | 0x30A0..=0x30FF        // katakana (incl. the ー prolonged-sound mark)
      | 0x31F0..=0x31FF        // katakana phonetic extensions
      | 0xFF66..=0xFF9D        // halfwidth katakana
    )
}

/// Function-word bigrams that carry no topic: time deixis (今天/现在), question
/// chrome (如何/什么/多少), and bigram artifacts of them (天天 from 今天+天气).
/// Dropped from [`query_terms`] — a junk row that merely shares "今天" with a
/// weather query is NOT on-topic, and counting it as relevant let such rows
/// slip past the off-topic gate into the prose tier. If a query is *entirely*
/// stopwords the term set goes empty and [`is_relevant`] fails open (matches
/// everything), so nothing is ever dropped for lacking a topic anchor.
const STOP_BIGRAMS: &[&str] = &[
    "今天", "今日", "明天", "昨天", "现在", "目前", "最近", "天天",
    "如何", "什么", "怎么", "怎样", "时候", "哪里", "哪儿", "哪个",
    "多少", "是谁", "谁是", "知道", "请问", "帮我", "告诉", "一下",
];

/// English/romaji function words that carry no topic — dropped from latin terms
/// (mirror of [`STOP_BIGRAMS`]). Without this every English row shares "the"/"is"
/// with the query, so [`is_relevant`] would fire for all of them and the
/// off-topic gate would be effectively disabled for English. Fail-open still
/// holds: an all-stopword query yields empty terms and matches everything.
const STOP_LATIN: &[&str] = &[
    "the", "a", "an", "of", "to", "in", "on", "at", "for", "and", "or", "is",
    "are", "was", "were", "be", "do", "does", "did", "what", "who", "when",
    "where", "why", "how", "which", "whose", "whom", "my", "your", "you", "me",
    "it", "its", "this", "that", "these", "those", "with", "about", "from",
    "into", "latest", "current", "today", "now", "please", "tell", "know",
    "there", "their", "have", "has", "will", "can", "get",
];

/// Extract matchable terms from the user's query.
pub fn query_terms(query: &str) -> QueryTerms {
    let query = fold_zh(query);
    let lower = query.to_lowercase();

    // Latin/alphanumeric runs of ≥2 chars (brand/song/person names: MyGO, Soyo).
    let mut latin = Vec::new();
    let mut cur = String::new();
    for c in lower.chars() {
        if c.is_ascii_alphanumeric() {
            cur.push(c);
        } else {
            if cur.len() >= 2 {
                latin.push(std::mem::take(&mut cur));
            } else {
                cur.clear();
            }
        }
    }
    if cur.len() >= 2 {
        latin.push(cur);
    }

    // CJK bigrams within each contiguous Han run.
    let mut cjk = Vec::new();
    let mut run: Vec<char> = Vec::new();
    let flush = |run: &mut Vec<char>, cjk: &mut Vec<String>| {
        match run.len() {
            0 => {}
            1 => cjk.push(run[0].to_string()),
            _ => {
                for w in run.windows(2) {
                    cjk.push(w.iter().collect());
                }
            }
        }
        run.clear();
    };
    for c in query.chars() {
        if is_cjk_term_char(c) {
            run.push(c);
        } else {
            flush(&mut run, &mut cjk);
        }
    }
    flush(&mut run, &mut cjk);

    cjk.retain(|t| !STOP_BIGRAMS.contains(&t.as_str()));
    latin.retain(|t| !STOP_LATIN.contains(&t.as_str()));

    latin.sort();
    latin.dedup();
    cjk.sort();
    cjk.dedup();
    QueryTerms { latin, cjk }
}

/// Whether `text` is on-topic for the query — shares any latin token or CJK
/// bigram. An **empty** query (no extractable terms) matches everything, so an
/// odd query never silently drops every result. Deliberately lenient: relevance
/// only *gates* a drop in combination with low-information (see [`super::curate`]),
/// so over-matching here is safe, false-dropping a good hit is not.
pub fn is_relevant(terms: &QueryTerms, text: &str) -> bool {
    if terms.is_empty() {
        return true;
    }
    let text = fold_zh(text);
    let lower = text.to_lowercase();
    terms.latin.iter().any(|t| lower.contains(t.as_str()))
        || terms.cjk.iter().any(|t| text.contains(t.as_str()))
}

/// Whether a snippet is SEO/marketing filler that names the topic without stating
/// a fact — so it should not be fed to the model as an answer. True when the
/// snippet leans on service/marketing language (or is a multi-range forecast SEO
/// list) **and** carries no concrete datum. Conservative: a snippet with any hard
/// datum (a temperature, a rate, a date…) is never treated as filler.
pub fn is_low_information(snippet: &str) -> bool {
    if snippet.is_empty() {
        return false; // "empty" is handled by the caller, not "low-info"
    }
    if has_concrete_data(snippet) {
        return false;
    }
    // Marketing / service language typical of SEO landing pages. English is
    // matched case-insensitively (`hay` is lowercased; CJK is unaffected by it).
    let hay = snippet.to_lowercase();
    const SERVICE: &[&str] = &[
        // zh
        "为您提供", "为你提供", "提供", "查询", "助您", "便捷", "欢迎", "访问",
        "官方网站", "官网", "一站式", "尽在", "服务平台", "旅游出行", "放心出行",
        "轻松掌握", "尽收眼底",
        // ja
        "を提供", "をお届け", "公式サイト", "ご案内", "お任せ", "詳しくは", "検索",
        // en (lowercase)
        "official site", "official website", "welcome to", "find the latest",
        "your source for", "everything you need", "learn more", "we provide",
        "provides the latest",
    ];
    let service_hits = SERVICE.iter().filter(|w| hay.contains(**w)).count();
    service_hits >= 2 || day_range_count(snippet) >= 2
}

/// Count of "N天" forecast-range tokens (15天, 30天, 40天…) — a dead giveaway of a
/// weather-aggregator SEO description ("…15天,30天,40天的天气…").
fn day_range_count(s: &str) -> usize {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\d{1,3}\s*天").unwrap())
        .find_iter(s)
        .count()
}

/// Whether the snippet carries a concrete, quotable datum: a number tied to a
/// unit/symbol (28℃, 6.7797 人民币, 20%, 3级), an approx-rate arrow (≈ 23.9), an
/// explicit date (1月14日 / 2025年), or a clock time (10:40). Bare numbers that
/// are *not* data — pm2.5, "24小时", "15天" — are intentionally excluded.
pub fn has_concrete_data(s: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?x)
              \d+\s*(?:℃|℉|°|%|％|元|¥|\$|€|£|km/h|km|公里|千米|级|hpa|mm|万|亿|人民币|美元|日元|欧元|英镑|dollars?|yen|euros?|pounds?)
            | [≈=]\s*\d
            | \d{1,2}\s*月\s*\d{1,2}
            | \d{4}\s*年
            | \d{1,2}:\d{2}
            | \d+\s*degrees?
            ",
        )
        .unwrap()
    })
    // Lowercased so English units (°F, DOLLARS, hPa) match regardless of case;
    // CJK is unaffected.
    .is_match(&s.to_lowercase())
}

// ── query-aware datum matching ────────────────────────────────────────────────
//
// "Has a datum" is not enough: for «福安今天天气如何» an encyclopedia row stating
// the city's population ("总人口668179人…截至2025年7月") is a datum — just not the
// one asked for. The quality-report run showed exactly that failure. So the
// datum *tier* requires the kind of fact the query is asking about.

/// The kind of concrete data a query asks for, inferred from its wording (the
/// same keyword families the search trigger fires on).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatumKind {
    /// 天气/气温… → temperatures, condition states, wind levels.
    Weather,
    /// 汇率/股价/价格… → price-like numbers (decimals, %, currency amounts).
    Money,
    /// 发售/上映/什么时候… → explicit dates.
    Date,
    /// Everything else → any concrete datum (with boilerplate dates discounted).
    Any,
}

/// Classify what the query wants. Checked most-specific-first; folds trad→simp
/// so 天氣/股價 classify like 天气/股价.
pub fn wanted_datum(query: &str) -> DatumKind {
    // `q` is folded (気→气, 発→发, 価→价…) AND lowercased, so CJK keywords are
    // written in post-fold simplified form and English keywords in lowercase.
    let q = fold_zh(query).to_lowercase();
    const WEATHER: &[&str] = &[
        // zh
        "天气", "气温", "温度", "下雨", "下雪", "降温", "几度", "多少度",
        // ja (post-fold; 天気→天气・気温→气温 fold into the zh row)
        "气候", "气象", "天候", "何度", "晴れ", "曇り", "くもり",
        // en
        "weather", "temperature", "forecast", "humidity", "celsius", "fahrenheit",
    ];
    const MONEY: &[&str] = &[
        // zh
        "汇率", "股价", "价格", "多少钱", "币价", "市值", "房价", "票价", "股票",
        // ja (post-fold; 為替→为替, 株価→株价, 価格→价格 folds into zh 价格)
        "为替", "株价", "値段", "相場", "いくら",
        // en
        "exchange rate", "stock price", "price", "cost", "how much", "currency",
        "dollar", "yen", "euro", "usd", "cny", "jpy", "eur", "gbp",
    ];
    const DATE: &[&str] = &[
        // zh
        "发售", "上映", "发布日期", "上线时间", "什么时候", "几号", "哪天",
        "哪一天", "何时", "日期", "生日",
        // ja (post-fold; 発売→发售・公開→公开 fold; 誕生日 covered by zh 生日)
        "公开", "リリース", "いつ",
        // en
        "release date", "release", "launch", "when is", "when does", "when did",
        "what date", "premiere", "coming out",
    ];
    if WEATHER.iter().any(|k| q.contains(k)) {
        return DatumKind::Weather;
    }
    if MONEY.iter().any(|k| q.contains(k)) {
        return DatumKind::Money;
    }
    if DATE.iter().any(|k| q.contains(k)) {
        return DatumKind::Date;
    }
    DatumKind::Any
}

/// Whether `text` states the kind of concrete fact the query asks for. This is
/// what promotes a row into the datum tier — see [`super::curate`].
pub fn has_wanted_datum(kind: DatumKind, text: &str) -> bool {
    // Folded (trad/shinjitai → simp) AND lowercased, so the datum regexes can
    // carry lowercase English branches (°f, dollars, november) alongside CJK.
    let text = fold_zh(text).to_lowercase();
    match kind {
        DatumKind::Weather => weather_datum(&text),
        DatumKind::Money => money_datum(&text),
        DatumKind::Date => date_datum(&text),
        DatumKind::Any => any_datum(&text),
    }
}

/// Weather facts are strictly digit-anchored: a temperature, range, wind level,
/// or weather vocabulary tied to a value. Deliberately NOT bare vocabulary or
/// condition words — SEO filler lists "温度、降水概率、湿度、风向" without values,
/// and a national headline saying 暴雨来袭 states no local conditions; both must
/// ride the prose tier, not outrank a row with the actual numbers.
fn weather_datum(s: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?x)
              \d+\s*[°℃℉]
            | \d+\s*~\s*\d+                                        # 28~37 range
            | (?:气温|温度|体感|湿度|降水概率|降雨概率|能见度|紫外线指数)\s*[:：]?\s*\d
            | [东南西北]?风\s*\d                                    # 西风 3-4级
            | \d+\s*级
            | \d+\s*degrees?                                       # en: 20 degrees
            | (?:humidity|wind|temp(?:erature)?)\s*[:：]?\s*\d      # en: humidity: 90
            | \d+\s*(?:mph|km/?h)                                  # en/units: wind speed
            ",
        )
        .unwrap()
    })
    .is_match(s)
}

/// Price-like numbers: decimals, percents, ≈/= rates, currency symbols, or a
/// ≥2-digit amount tied to a currency word ("1 日元" is converter boilerplate,
/// "13,620日元" is a fare).
fn money_datum(s: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?x)
              \d+\.\d+
            | \d+(?:\.\d+)?\s*[%％]
            | [≈=]\s*\d
            | [¥￥$＄€£]\s*\d
            | \d[\d,]+\s*(?:日元|日圆|円|人民币|美元|美金|港元|港币|欧元|英镑|韩元|元|圆)
            | \d+(?:\.\d+)?\s*(?:dollars?|yen|euros?|pounds?|usd|cny|jpy|eur|gbp|rmb)  # en/romaji words
            ",
        )
        .unwrap()
    })
    .is_match(s)
}

/// Explicit dates. No boilerplate discount here: when the query itself asks for
/// a date (发售/上线时间…), even "上线" phrasing can be the wanted answer.
fn date_datum(s: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?x)
              \d{4}\s*年
            | \d{1,2}\s*月\s*\d{1,2}\s*[日号]?
            | \d{4}[./-]\d{1,2}[./-]\d{1,2}
            | (?:jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec)[a-z]*\.?\s+\d{1,2}   # en: november 19
            | \d{1,2}\s+(?:jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec)[a-z]*      # en: 19 november
            ",
        )
        .unwrap()
    })
    .is_match(s)
}

/// General datum for [`DatumKind::Any`]: any non-date concrete signal (units,
/// rates, clock times), a duration, or a date that is NOT site-history
/// boilerplate. The discount kills "创立于1993年1月" / "2011 年正式上线" — founding
/// dates that made junk rows outrank real answers in the quality report.
fn any_datum(s: &str) -> bool {
    static NONDATE: OnceLock<Regex> = OnceLock::new();
    static DATE: OnceLock<Regex> = OnceLock::new();
    let nondate = NONDATE.get_or_init(|| {
        Regex::new(
            r"(?x)
              \d+\s*(?:℃|℉|°|%|％|元|¥|\$|€|£|km/h|km|公里|千米|米|级|hpa|mm|万|亿|人民币|美元|日元|欧元|英镑|dollars?|yen|euros?|pounds?)
            | ≈\s*\d
            | \d{1,2}:\d{2}
            | \d+\s*小时\s*\d+\s*分                                # 2小时30分(钟)
            | \d+\s*时间\s*\d+\s*分                                # ja: 2時間30分 → 时间
            | \d+\s*分钟                                           # 130分钟 — bare 24小时 spans stay excluded
            | \d+\s*(?:hours?|hrs?)\s*\d+\s*(?:minutes?|mins?)     # en: 2 hours 30 minutes
            | \d+\s*(?:minutes?|mins?)                             # en: 130 minutes
            ",
        )
        .unwrap()
    });
    if nondate.is_match(s) {
        return true;
    }
    let date = DATE.get_or_init(|| {
        Regex::new(r"\d{4}\s*年|\d{1,2}\s*月\s*\d{1,2}\s*[日号]?").unwrap()
    });
    date.find_iter(s).any(|m| !is_founding_date(s, m.start(), m.end()))
}

/// True when a date match sits next to site-history phrasing (创立/成立/上线/…) —
/// a fact about the *website*, not an answer to the query.
fn is_founding_date(s: &str, start: usize, end: usize) -> bool {
    const MARKERS: &[&str] = &["创立", "成立", "上线", "创办", "核准", "注册", "创建"];
    // ±24 bytes (≈8 CJK chars), snapped to char boundaries.
    let mut lo = start.saturating_sub(24);
    while !s.is_char_boundary(lo) {
        lo -= 1;
    }
    let mut hi = (end + 24).min(s.len());
    while !s.is_char_boundary(hi) {
        hi += 1;
    }
    let window = &s[lo..hi];
    MARKERS.iter().any(|m| window.contains(m))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_terms_extracts_latin_and_cjk_bigrams() {
        let t = query_terms("MyGO 的主唱是谁");
        assert!(t.latin.contains(&"mygo".to_string()));
        assert!(t.cjk.contains(&"主唱".to_string()));
        // Question chrome ("是谁") is a stopword bigram, not a topic anchor.
        assert!(!t.cjk.contains(&"是谁".to_string()));
    }

    #[test]
    fn query_terms_drops_function_word_bigrams_but_fails_open_when_all_stop() {
        // «福安今天天气如何»: 今天/天天/如何 are chrome; 福安/天气 anchor the topic.
        let t = query_terms("福安今天天气如何");
        assert!(t.cjk.contains(&"福安".to_string()));
        assert!(t.cjk.contains(&"天气".to_string()));
        for stop in ["今天", "天天", "如何"] {
            assert!(!t.cjk.contains(&stop.to_string()), "{stop} should be dropped");
        }
        // A junk row sharing only "今天" with the query is NOT on-topic now.
        assert!(!is_relevant(&t, "今天A股大跌，沪指下挫2%"));
        // All-stopword query → empty terms → fail-open (never drop everything).
        assert!(is_relevant(&query_terms("今天"), "anything at all"));
    }

    #[test]
    fn folding_covers_japanese_shinjitai() {
        // ja-locale queries are first-class: 気 (U+6C17) ≠ 氣 (U+6C23).
        assert_eq!(wanted_datum("東京の天気"), DatumKind::Weather);
        assert_eq!(wanted_datum("ブルーアーカイブの発売日"), DatumKind::Date);
        let t = query_terms("東京の天気");
        assert!(is_relevant(&t, "东京天气预报：多云 18~25℃"));
        assert!(has_wanted_datum(DatumKind::Weather, "東京の天気は晴れ、気温 18~25℃"));
    }

    #[test]
    fn relevance_matches_topic_and_ignores_offtopic() {
        let t = query_terms("福安今天天气如何");
        // On-topic: the weather SEO page shares 福安 / 天气.
        assert!(is_relevant(&t, "福安天气网为您提供福安天气预报24小时详情"));
        // Off-topic: shares nothing.
        assert!(!is_relevant(&t, "英伟达股价创新高 分析师看好"));
        // Empty query matches everything (never silently drop all).
        assert!(is_relevant(&query_terms(""), "anything at all"));
    }

    #[test]
    fn relevance_matches_latin_case_insensitively() {
        let t = query_terms("mygo 主唱");
        assert!(is_relevant(&t, "MyGO!!!!! Official Site — Wikipedia"));
    }

    #[test]
    fn low_information_flags_seo_boilerplate() {
        // The exact junk the user saw (from the real Bing dump).
        assert!(is_low_information(
            "福安天气网为您提供福安天气预报24小时详情、福安今日天气预报，包括今日实时温度、24小时降水概率、湿度、pm2.5、风向、紫外线强度等，助您放心出行。"
        ));
        assert!(is_low_information(
            "天气网提供福安天气预报15天,30天,今日天气,明天天气,福安未来一周的天气预报，旅游出行,从天气网开始！"
        ));
        assert!(is_low_information(
            "福安天气预报，及时准确发布中央气象台天气信息，便捷查询福安一周天气预报，福安15日天气预报。"
        ));
    }

    #[test]
    fn low_information_keeps_snippets_with_a_real_datum() {
        // A snippet that actually states the fact must NOT be flagged as filler,
        // even if it also contains service words.
        assert!(!is_low_information("福安今天多云，气温24~33℃，降水概率20%。"));
        assert!(!is_low_information("1 美元 ≈ 6.7797 人民币；1 人民币 ≈ 0.1475 美元"));
        assert!(!is_low_information("演唱会将于 2025年3月14日 在东京巨蛋举行"));
        // A plain factual sentence with no service language is never filler.
        assert!(!is_low_information("MyGO!!!!! 的主唱是高松灯。"));
    }

    #[test]
    fn concrete_data_excludes_non_data_numbers() {
        assert!(has_concrete_data("28℃"));
        assert!(has_concrete_data("湿度 65%"));
        assert!(has_concrete_data("10:40 更新"));
        assert!(has_concrete_data("≈ 23.92"));
        // Not data: version/pm/counter numbers, forecast-range days, hour spans.
        assert!(!has_concrete_data("pm2.5 空气质量"));
        assert!(!has_concrete_data("15天,30天,40天的天气预报"));
        assert!(!has_concrete_data("24小时详情"));
    }

    // ── query-aware datum classes (from the quality-report failure modes) ────

    #[test]
    fn wanted_datum_classifies_query_themes() {
        assert_eq!(wanted_datum("福安今天天气如何"), DatumKind::Weather);
        assert_eq!(wanted_datum("東京天氣"), DatumKind::Weather); // trad folds
        assert_eq!(wanted_datum("人民币对日元汇率"), DatumKind::Money);
        assert_eq!(wanted_datum("英伟达股价"), DatumKind::Money);
        assert_eq!(wanted_datum("GTA6 发售日期"), DatumKind::Date);
        assert_eq!(wanted_datum("鬼灭之刃什么时候上映"), DatumKind::Date);
        assert_eq!(wanted_datum("MyGO 的主唱是谁"), DatumKind::Any);
        assert_eq!(wanted_datum("今天有什么科技新闻"), DatumKind::Any);
    }

    #[test]
    fn weather_datum_requires_values_not_vocabulary() {
        let w = DatumKind::Weather;
        // Real conditions: temps, ranges, wind levels, condition states.
        assert!(has_wanted_datum(w, "体感温度33° 湿度 90% 露点 26°"));
        assert!(has_wanted_datum(w, "福安今日天气为雨 西风 3-4级"));
        assert!(has_wanted_datum(w, "多云 25 ~ 37℃ 南风2级"));
        assert!(has_wanted_datum(w, "明天雨转多云，气温 18~25℃"));
        // The exact failure: encyclopedia numbers are data, but not WEATHER data.
        assert!(!has_wanted_datum(w, "截至2025年7月，福安市辖4个街道，总人口668179人。"));
        // SEO filler naming weather vocabulary without values.
        assert!(!has_wanted_datum(w, "包括今日实时温度、24小时降水概率、湿度、pm2.5、风向、紫外线强度等"));
        assert!(!has_wanted_datum(w, "使用 MSN 天气 获取每小时准确预测，以及 10 天每日预测和天气雷达"));
        // Condition words without values (a national headline, from the real
        // dump) state no local conditions — prose, not datum.
        assert!(!has_wanted_datum(
            w,
            "南方新一轮强降雨过程登场 北方多地高温天气发展增多 今天起，南方新一轮强降雨来袭，需警惕暴雨致灾"
        ));
        assert!(!has_wanted_datum(w, "今天有小雨，出门记得带伞"));
    }

    #[test]
    fn money_datum_requires_price_like_numbers() {
        let m = DatumKind::Money;
        assert!(has_wanted_datum(m, "英伟达公司 194.83 (-1.39%)")); // price in a title
        assert!(has_wanted_datum(m, "1 人民币 ≈ 23.7912 日元"));
        assert!(has_wanted_datum(m, "当前汇率:1日元=0.042141人民币"));
        assert!(has_wanted_datum(m, "自由席13,620日元"));
        // Converter boilerplate ("1 日元 到 人民币") and site history are not prices.
        assert!(!has_wanted_datum(m, "免费获取最新的 1 日元 到 中国人民币 汇率"));
        assert!(!has_wanted_datum(m, "英伟达，创立于1993年1月，是美国一家半导体公司"));
        assert!(!has_wanted_datum(m, "新浪财经为您提供行情走势、五档盘口、逐笔交易等实时行情数据"));
    }

    #[test]
    fn date_datum_matches_dates_for_date_queries() {
        let d = DatumKind::Date;
        assert!(has_wanted_datum(d, "游戏将于2026年11月19日正式发售"));
        assert!(has_wanted_datum(d, "预购将于 6 月 25 日零点开启"));
        assert!(!has_wanted_datum(d, "开放世界玩法再升级，每一处都藏着惊喜"));
    }

    #[test]
    fn any_datum_discounts_founding_dates_and_counts_durations() {
        let a = DatumKind::Any;
        // Site-history boilerplate: the ONLY datum is a founding-style date.
        assert!(!has_wanted_datum(a, "知乎，于 2011 年 1 月正式上线的问答社区"));
        assert!(!has_wanted_datum(a, "2010年,中央外宣办核准网站名称为中国科技网"));
        // A birthday/founding of the QUERIED entity is still the answer (诞生 is
        // not discounted), and any non-date signal always counts.
        assert!(has_wanted_datum(a, "乐队诞生于2022年4月29日"));
        assert!(has_wanted_datum(a, "成立于1998年，注册资本 5000万"));
        // Durations are data now; bare "24小时" service spans still are not.
        assert!(has_wanted_datum(a, "最短行程时间仅2小时 14分钟"));
        assert!(has_wanted_datum(a, "全程约130分钟"));
        assert!(!has_wanted_datum(a, "24小时详情尽在天气网"));
    }

    #[test]
    fn folding_matches_traditional_text_against_simplified_query() {
        let t = query_terms("英伟达股价");
        assert!(is_relevant(&t, "英偉達(NVDA) 股價、新聞、報價和記錄"));
        let t = query_terms("東京天氣"); // trad query ↔ simp text too
        assert!(is_relevant(&t, "东京天气预报一周"));
        // Trad weather text carries the datum.
        assert!(has_wanted_datum(DatumKind::Weather, "多雲轉晴 氣溫 18~25℃ 東風3級"));
    }

    // ── multilingual coverage: English + Japanese ───────────────────────────

    #[test]
    fn wanted_datum_classifies_english_and_japanese() {
        // English (matched case-insensitively).
        assert_eq!(wanted_datum("weather in Tokyo tomorrow"), DatumKind::Weather);
        assert_eq!(wanted_datum("USD to JPY exchange rate"), DatumKind::Money);
        assert_eq!(wanted_datum("GTA6 release date"), DatumKind::Date);
        assert_eq!(wanted_datum("who is the singer of MyGO"), DatumKind::Any);
        // Japanese: kanji that fold into the zh families…
        assert_eq!(wanted_datum("東京の天気"), DatumKind::Weather); // 天気→天气
        assert_eq!(wanted_datum("ブルアカの発売日"), DatumKind::Date); // 発売→发售
        assert_eq!(wanted_datum("ドル円の為替"), DatumKind::Money); // 為替→为替
        assert_eq!(wanted_datum("新幹線の値段"), DatumKind::Money); // ja-distinct 値段
        assert_eq!(wanted_datum("株価はいくら"), DatumKind::Money); // 株価→株价 (価→价)
        assert_eq!(wanted_datum("いつリリースされる"), DatumKind::Date); // kana リリース/いつ
    }

    #[test]
    fn weather_datum_matches_english_values() {
        let w = DatumKind::Weather;
        assert!(has_wanted_datum(w, "Tokyo: 18°C, partly cloudy"));
        assert!(has_wanted_datum(w, "High of 72 degrees, wind 12 mph"));
        assert!(has_wanted_datum(w, "Humidity: 90%, feels like 33°"));
        // Vocabulary without a value is still not a weather datum.
        assert!(!has_wanted_datum(w, "Get the latest weather forecast and radar"));
    }

    #[test]
    fn money_datum_matches_symbols_and_english_words() {
        let m = DatumKind::Money;
        assert!(has_wanted_datum(m, "€1,299.00 for the base model"));
        assert!(has_wanted_datum(m, "around £25 per ticket"));
        assert!(has_wanted_datum(m, "1 USD = 157.3 JPY"));
        assert!(has_wanted_datum(m, "costs 20 dollars")); // single-digit worded ok
        // A currency word with no amount is not a price.
        assert!(!has_wanted_datum(m, "convert dollars to yen easily"));
    }

    #[test]
    fn date_datum_matches_english_month_names() {
        let d = DatumKind::Date;
        assert!(has_wanted_datum(d, "Releases on November 19, 2026"));
        assert!(has_wanted_datum(d, "coming 19 Nov 2026"));
        assert!(has_wanted_datum(d, "out on 2026-11-19"));
        assert!(!has_wanted_datum(d, "coming soon to all platforms"));
    }

    #[test]
    fn any_datum_counts_english_and_japanese_durations() {
        let a = DatumKind::Any;
        assert!(has_wanted_datum(a, "The trip takes about 130 minutes"));
        assert!(has_wanted_datum(a, "2 hours 30 minutes end to end"));
        assert!(has_wanted_datum(a, "最短2時間14分")); // ja 時間→时间
    }

    #[test]
    fn query_terms_and_relevance_cover_katakana_and_english() {
        // Katakana proper noun becomes matchable CJK terms (foreign-name anchor).
        let t = query_terms("ブルーアーカイブの声優");
        assert!(t.cjk.iter().any(|term| "ブルーアーカイブ".contains(term.as_str())));
        assert!(is_relevant(&t, "ブルーアーカイブ 公式サイト｜声優一覧"));
        // English relevance ignores stopwords, so a shared "the"/"is" alone
        // does NOT make an off-topic row relevant.
        let t = query_terms("who is the singer of MyGO");
        assert!(is_relevant(&t, "MyGO!!!!! members and their roles"));
        assert!(!is_relevant(&t, "This is the best of everything today"));
    }

    #[test]
    fn low_information_flags_english_and_japanese_seo() {
        // Two service phrases + no datum → filler.
        assert!(is_low_information(
            "Welcome to the official site. Your source for the latest news and more."
        ));
        assert!(is_low_information("公式サイトへようこそ。最新情報をお届けする検索ポータル。"));
        // English snippet stating a real fact is never filler.
        assert!(!is_low_information("The official site lists the price at $59.99."));
    }
}
