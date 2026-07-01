//! Search activation — decide whether a user message needs a live web search.
//!
//! Proactive, LLM-free matching so the search fires without a model round-trip.
//! Pure and DOM-/network-free → unit-tested. The challenge is **recall vs false
//! positives**: factual question words (是谁 / 什么时候 / 多少钱 …) also appear in
//! ordinary chat directed at us, so a flat keyword allow-list either misses real
//! questions or fires on small talk. Instead [`needs_search`] is **layered with
//! precedence**:
//!
//! 1. Explicit search verbs (查一下 / 帮我查 …)           → search (wins over all)
//! 2. Self-reference about the user (我叫什么 / 我的生日)   → no search (local memory)
//! 3. Personal chit-chat (你/我 + daily-life verb, no entity) → no search
//! 4. Time / info / English freshness cues                → search
//! 5. Factual interrogative **about an external entity**   → search
//! 5b. Factual interrogative with no 1st/2nd-person subject → search
//! 6. Bare verb (short) / explicit date                   → search
//! 7. otherwise                                           → no search
//!
//! The entity test ([`mentions_world_entity`]) is what lets "你知道 MyGO 的主唱是
//! 谁吗" search while "你是谁" / "你今天什么时候吃饭" do not.

use std::sync::OnceLock;

use regex::Regex;

/// Explicit "please search" verbs — always trigger.
const PROACTIVE: &[&str] = &["搜索", "查一下", "帮我搜", "帮我查", "查查", "搜一下", "搜搜", "搜下"];
/// Time-sensitivity cues.
const TIME: &[&str] = &["最新", "实时", "今天", "今日", "昨天", "明天", "近期", "目前", "现在"];
/// Information categories that usually need fresh data.
const INFO: &[&str] = &["新闻", "价格", "票价", "天气", "版本", "发布", "公告", "汇率", "股价", "演唱会", "上映", "比分", "赛程"];
/// English cues (matched case-insensitively).
const ENGLISH: &[&str] = &[
    "latest", "current", "today", "news", "price", "weather", "release", "schedule", "score",
];
/// The bare verb "查" / "搜索" — kept narrow (whole-message short queries) to limit
/// over-triggering on words like 检查 / 调查.
const BARE_VERBS: &[&str] = &["查", "搜索"];

/// Factual interrogative markers (CJK) — identity / definition / date / place /
/// quantity questions. They trigger only with an external entity (step 5) or no
/// personal subject (step 5b), so they don't fire on "你是谁" / "我累了".
const FACTUAL_CJK: &[&str] = &[
    "是谁", "谁是", "是什么", "什么是", "什么意思", "啥意思", "干什么的",
    "几号", "几月", "什么时候", "何时", "哪一年", "哪年", "哪一天", "哪天",
    "在哪", "哪里", "哪儿", "怎么去", "怎么走", "怎么到", "哪个",
    "多少钱", "多高", "多大", "多远", "多重", "几岁",
];
/// Factual interrogative markers (English, case-insensitive).
const FACTUAL_EN: &[&str] = &[
    "who is", "who are", "what is", "what are", "where is", "where are",
    "how to", "how much", "how many", "when is", "when did", "when does",
];

/// First/second-person conversational pronouns. Their presence as a subject is
/// what distinguishes "about us" chat from "about the world" questions.
const SELF_USER_PRONOUNS: &[&str] = &["你", "您", "我", "咱"];

/// Daily-life verbs/states — when bound to a personal pronoun (and no external
/// entity), the message is small talk, not a factual query.
const PERSONAL_VERBS: &[&str] = &[
    "吃饭", "吃", "喝", "睡觉", "睡", "起床", "醒了", "睡了", "休息", "上班", "下班",
    "放学", "上学", "玩", "做什么", "干嘛", "干什么", "在干", "在吗", "在不在",
    "有空", "累", "困", "无聊", "开心", "心情", "想我", "陪我", "喜欢我",
];

/// Self-identity phrases about the *user* — answered from local memory, never the
/// web. These are bound forms ("我叫什么", not bare "我" + "什么") to avoid catching
/// "我想知道 X 是谁".
const SELF_PATTERNS: &[&str] = &[
    "我叫什么", "我的名字", "我是谁", "我叫啥", "我的生日", "我多大", "我几岁",
    "我住", "我的电话", "我的邮箱", "我多高", "我的星座", "我属什么", "我的工作",
];

/// Possessive attributes ("X的<attr>") that mark a factual world query when the
/// owner X is **not** a pronoun (so 祥子的生日 counts, 我的生日 / 你的生日 don't).
const ENTITY_ATTRS: &[&str] = &[
    "生日", "主唱", "队长", "成员", "身高", "年龄", "价格", "票价", "销量", "票房",
    "排名", "作者", "导演", "国籍", "成立", "出道", "首张", "专辑", "声优", "配音",
    "本名", "原名",
];

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

    // 1. Explicit search verbs — the user asked; always search.
    if PROACTIVE.iter().any(|k| text.contains(k)) {
        return true;
    }

    // 2. Self-reference about the user — served from memory, not the web.
    if is_self_reference(text) {
        return false;
    }

    // 3. Personal chit-chat about us — no web context needed.
    if is_personal_chitchat(text) {
        return false;
    }

    // 4. Freshness cues (time / info category / English).
    if TIME.iter().any(|k| text.contains(k))
        || INFO.iter().any(|k| text.contains(k))
        || ENGLISH.iter().any(|k| lower.contains(k))
    {
        return true;
    }

    // 5. Factual interrogative about an external named entity.
    let has_factual: bool =
        FACTUAL_CJK.iter().any(|k| text.contains(k)) || FACTUAL_EN.iter().any(|k| lower.contains(k));
    if has_factual && mentions_world_entity(text) {
        return true;
    }
    // 5b. A factual question with no 1st/2nd-person subject is about the world
    // even without an explicit entity (e.g. 怎么去东京迪士尼); kept short to avoid
    // firing inside long personal sentences.
    if has_factual
        && !SELF_USER_PRONOUNS.iter().any(|p| text.contains(p))
        && text.chars().count() <= 20
    {
        return true;
    }

    // 6. Bare "查"/"搜索" only for short, query-shaped messages, then dates.
    if text.chars().count() <= 12 && BARE_VERBS.iter().any(|k| text.contains(k)) {
        return true;
    }
    date_re().is_match(text)
}

/// Whether the message asks about the user themselves (→ local memory).
fn is_self_reference(text: &str) -> bool {
    SELF_PATTERNS.iter().any(|p| text.contains(p))
}

/// Whether the message is small talk about us (pronoun + daily-life verb) with no
/// external entity to look up.
fn is_personal_chitchat(text: &str) -> bool {
    let has_pronoun = ["你", "我", "咱"].iter().any(|p| text.contains(p));
    has_pronoun
        && PERSONAL_VERBS.iter().any(|v| text.contains(v))
        && !mentions_world_entity(text)
}

/// High-precision heuristic: does the text name an external subject worth a web
/// lookup? Latin proper noun, a quoted title, or a non-pronoun possessive.
fn mentions_world_entity(text: &str) -> bool {
    has_latin_run(text) || has_quoted_title(text) || has_entity_possessive(text)
}

/// A run of ≥2 ASCII letters — brand/song/person names like `MyGO`, `Soyo`.
fn has_latin_run(text: &str) -> bool {
    let mut run = 0u32;
    for c in text.chars() {
        if c.is_ascii_alphabetic() {
            run += 1;
            if run >= 2 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

/// A CJK title/quote wrapper — 《…》/「…」/『…』/"…".
fn has_quoted_title(text: &str) -> bool {
    ['《', '「', '『', '“', '〈'].iter().any(|q| text.contains(*q))
}

/// A possessive "X的<attr>" where X is a non-pronoun owner (祥子的生日) — excludes
/// 我的生日 / 你的生日.
fn has_entity_possessive(text: &str) -> bool {
    const PRONOUN_CHARS: &str = "你我咱他她它您们";
    let chars: Vec<char> = text.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if *c != '的' || i == 0 {
            continue;
        }
        if PRONOUN_CHARS.contains(chars[i - 1]) {
            continue; // 我的 / 你的 / 他们的 …
        }
        let after: String = chars[i + 1..].iter().collect();
        if ENTITY_ATTRS.iter().any(|a| after.starts_with(a)) {
            return true;
        }
    }
    false
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
        // bare-verb rule (and contains no multi-char keyword / entity).
        assert!(!needs_search("我想仔细地检查我写的这段代码到底哪里有问题啊"));
    }

    // ── factual-question recall vs false positives (objective 5) ────────────

    #[test]
    fn factual_keyword_beats_pronoun() {
        // Strong factual marker + a world entity overrides the leading 你/我.
        assert!(needs_search("你知道 MyGO 的主唱是谁吗"));
        assert!(needs_search("MyGO 的主唱是谁"));
        assert!(needs_search("祥子的生日是几号")); // pure-CJK name via possessive
        assert!(needs_search("怎么去东京迪士尼")); // no pronoun → step 5b
    }

    #[test]
    fn explicit_search_verb_always_fires() {
        assert!(needs_search("帮我查一下 Soyo 的生日是什么时候"));
    }

    #[test]
    fn personal_chitchat_not_triggered() {
        // Daily-life activity directed at us — even with 今天 / 什么时候.
        assert!(!needs_search("你今天什么时候吃饭"));
        assert!(!needs_search("你在做什么呢"));
    }

    #[test]
    fn self_reference_not_triggered() {
        // Questions about the user → answered from local memory.
        assert!(!needs_search("我叫什么名字"));
        assert!(!needs_search("我的生日是哪天"));
    }

    #[test]
    fn mutsumi_identity_not_triggered() {
        // "Who are you" is about the persona, not the web.
        assert!(!needs_search("你是谁"));
        assert!(!needs_search("你叫什么"));
    }

    #[test]
    fn entity_detection_excludes_pronoun_possessives() {
        assert!(mentions_world_entity("祥子的生日"));
        assert!(mentions_world_entity("MyGO 真好听"));
        assert!(mentions_world_entity("《孤独摇滚》好看吗"));
        assert!(!mentions_world_entity("我的生日"));
        assert!(!mentions_world_entity("你的票价")); // pronoun owner
        assert!(!mentions_world_entity("今天天气怎么样"));
    }
}
