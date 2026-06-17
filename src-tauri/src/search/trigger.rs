//! Search activation — decide whether a user message needs a live web search.
//!
//! Proactive keyword matching (plus date detection) so the search fires without
//! waiting for an LLM decision. Pure and DOM-/network-free → unit-tested. Per
//! the design, broad coverage is preferred; the occasional false trigger is
//! harmless (the retrieved context is labeled "ignore if irrelevant").

use std::sync::OnceLock;

use regex::Regex;

/// Explicit "please search" verbs.
const PROACTIVE: &[&str] = &["搜索", "查一下", "帮我搜", "帮我查", "查查", "搜一下"];
/// Time-sensitivity cues.
const TIME: &[&str] = &["最新", "实时", "今天", "今日", "昨天", "明天", "近期", "目前", "现在"];
/// Information categories that usually need fresh data.
const INFO: &[&str] = &["新闻", "价格", "票价", "天气", "版本", "发布", "公告", "汇率", "股价", "演唱会", "上映"];
/// English cues (matched case-insensitively).
const ENGLISH: &[&str] = &[
    "latest", "current", "today", "news", "price", "weather", "release", "schedule", "score",
];
/// The bare verb "查" / "搜" — kept narrow (whole-message short queries) to limit
/// over-triggering on words like 检查 / 调查.
const BARE_VERBS: &[&str] = &["查", "搜索"];

/// Compiled date patterns (YYYY-MM-DD / YYYY年M月D日 / M月D日 / M/D variants).
fn date_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?x)
              \d{4}\s*[-/年.]\s*\d{1,2}\s*[-/月.]\s*\d{1,2}   # 2025-01-14 / 2025年1月14日
            | \d{1,2}\s*月\s*\d{1,2}\s*[日号]?               # 1月14日
            | \d{4}\s*年                                      # 2025年
            ",
        )
        .expect("valid date regex")
    })
}

/// True if `text` should trigger a live web search.
pub fn needs_search(text: &str) -> bool {
    let lower = text.to_lowercase();

    if PROACTIVE.iter().any(|k| text.contains(k))
        || TIME.iter().any(|k| text.contains(k))
        || INFO.iter().any(|k| text.contains(k))
        || ENGLISH.iter().any(|k| lower.contains(k))
    {
        return true;
    }

    // Bare "查"/"搜索" only for short, query-shaped messages (reduce false hits
    // inside longer sentences containing 检查/调查/搜罗 etc.).
    if text.chars().count() <= 12 && BARE_VERBS.iter().any(|k| text.contains(k)) {
        return true;
    }

    date_re().is_match(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triggers_on_proactive_verbs() {
        assert!(needs_search("帮我搜一下明天的天气"));
        assert!(needs_search("查一下东京到大阪的票价"));
    }

    #[test]
    fn triggers_on_time_and_info_cues() {
        assert!(needs_search("最新的 Ave Mujica 演唱会消息"));
        assert!(needs_search("今天有什么新闻"));
        assert!(needs_search("现在的汇率是多少"));
    }

    #[test]
    fn triggers_on_english_cues_case_insensitive() {
        assert!(needs_search("what's the LATEST news"));
        assert!(needs_search("today's weather in Tokyo"));
    }

    #[test]
    fn triggers_on_dates() {
        assert!(needs_search("2025-01-14 发生了什么"));
        assert!(needs_search("1月14日是谁的生日"));
        assert!(needs_search("2024年的总结"));
    }

    #[test]
    fn ordinary_chat_without_cues_is_skipped() {
        assert!(!needs_search("你在做什么呢"));
        assert!(!needs_search("我有点累了"));
        assert!(!needs_search("谢谢你陪我"));
        assert!(!needs_search("hello, how are you"));
    }

    #[test]
    fn bare_verb_only_for_short_queries() {
        assert!(needs_search("查 Mutsumi")); // short query
        // Long sentence whose only "查" is inside 检查 must not trigger via the
        // bare-verb rule (and contains no multi-char keyword).
        assert!(!needs_search("我想仔细地检查我写的这段代码到底哪里有问题啊"));
    }
}
