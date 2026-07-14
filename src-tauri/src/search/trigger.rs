//! Search activation — decide whether a user message needs a live web search.
//!
//! Proactive, LLM-free matching so the search fires without a model round-trip.
//! Pure and DOM-/network-free → unit-tested. The challenge is **recall vs false
//! positives**: factual question words (是谁 / 什么时候 / 多少钱 …) also appear in
//! ordinary chat directed at us, so a flat keyword allow-list either misses real
//! questions or fires on small talk. Instead [`needs_search`] is **layered with
//! precedence**. Every layer is **trilingual** (zh + ja + en) — the app ships a
//! ja/en UI, so a query like「東京の天気」or "USD to JPY rate" must trigger just
//! like its Chinese counterpart; matching is done on the lowercased message
//! (harmless to CJK, case-folds English):
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

/// Explicit "please search" verbs — always trigger. (zh + ja + en; matched
/// against the lowercased message so English is case-insensitive.)
const PROACTIVE: &[&str] = &[
    // zh
    "搜索", "查一下", "帮我搜", "帮我查", "查查", "搜一下", "搜搜", "搜下",
    // ja
    "調べて", "検索して", "教えて", "ググって", "調べる",
    // en
    "search for", "look up", "look it up", "google", "find out",
];
/// Time-sensitivity cues (zh + ja). English freshness lives in [`ENGLISH`].
const TIME: &[&str] = &[
    // zh
    "最新", "实时", "今天", "今日", "昨天", "明天", "近期", "目前", "现在",
    // ja
    "最新", "今日", "きょう", "明日", "あした", "昨日", "今", "現在", "最近", "リアルタイム",
];
/// Information categories that usually need fresh data (zh + ja).
const INFO: &[&str] = &[
    // zh
    "新闻", "价格", "票价", "天气", "版本", "发布", "公告", "汇率", "股价", "演唱会", "上映", "比分", "赛程",
    // ja
    "ニュース", "価格", "値段", "天気", "気温", "為替", "株価", "発売", "リリース",
    "ライブ", "公演", "スコア", "試合",
];
/// English cues (matched case-insensitively).
const ENGLISH: &[&str] = &[
    "latest", "current", "today", "tonight", "news", "price", "cost", "weather",
    "forecast", "release", "schedule", "score", "stock", "exchange rate",
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
/// Factual interrogative markers (Japanese). Gated exactly like [`FACTUAL_CJK`]
/// (external entity or no personal subject). Bare 何 is excluded — too broad
/// (何する = chitchat); only specific compounds are listed.
const FACTUAL_JP: &[&str] = &[
    "は誰", "とは", "何ですか", "いつ", "どこ", "いくら", "何時", "何歳",
    "何年", "何月", "何日", "何度", "どうやって", "どちら", "だれ",
];
/// Factual interrogative markers (English, case-insensitive).
const FACTUAL_EN: &[&str] = &[
    "who is", "who are", "what is", "what are", "where is", "where are",
    "how to", "how much", "how many", "when is", "when did", "when does",
    "what time", "what year", "how old is", "how tall", "how far",
];

/// First/second-person conversational pronouns (zh + ja). Their presence as a
/// subject is what distinguishes "about us" chat from "about the world"
/// questions (used by step 5b and personal-chitchat detection).
const SELF_USER_PRONOUNS: &[&str] = &["你", "您", "我", "咱", "私", "わたし", "あなた", "君", "僕", "俺"];

/// Daily-life verbs/states — when bound to a personal pronoun (and no external
/// entity), the message is small talk, not a factual query.
const PERSONAL_VERBS: &[&str] = &[
    // zh
    "吃饭", "吃", "喝", "睡觉", "睡", "起床", "醒了", "睡了", "休息", "上班", "下班",
    "放学", "上学", "玩", "做什么", "干嘛", "干什么", "在干", "在吗", "在不在",
    "有空", "累", "困", "无聊", "开心", "心情", "想我", "陪我", "喜欢我",
    // ja (bound to a pronoun below)
    "食べ", "寝", "遊", "疲れ", "眠", "起き",
];

/// Personal pronouns used to bind [`PERSONAL_VERBS`] into "small talk about us".
const PERSONAL_PRONOUNS: &[&str] = &["你", "我", "咱", "私", "わたし", "あなた", "君", "僕", "俺"];

/// Greeting / affect phrases that are small talk on their own (no pronoun, no
/// entity needed) — English + Japanese. Suppressed only when the same message
/// carries no info/factual cue, so "good morning, what's the weather" still
/// searches (see [`is_personal_chitchat`]).
const GREETING_CHITCHAT: &[&str] = &[
    // en
    "how are you", "good morning", "good night", "good evening", "thank you",
    "thanks", "i love you", "i miss you", "miss you", "i'm tired", "im tired",
    "i'm bored", "nice to meet you", "how's it going",
    // ja
    "おはよう", "こんにちは", "こんばんは", "おやすみ", "ありがとう", "元気",
    "疲れた", "眠い", "会いたい", "ただいま", "おかえり",
];

/// Self-identity phrases about the *user or the persona* — answered from local
/// memory / character, never the web. Bound forms ("我叫什么", not bare "我" +
/// "什么") to avoid catching "我想知道 X 是谁". Matched against the lowercased
/// message so English is case-insensitive.
const SELF_PATTERNS: &[&str] = &[
    // zh — the user
    "我叫什么", "我的名字", "我是谁", "我叫啥", "我的生日", "我多大", "我几岁",
    "我住", "我的电话", "我的邮箱", "我多高", "我的星座", "我属什么", "我的工作",
    // zh — the persona (Mutsumi), about the character not the web
    "你是谁", "你叫什么", "你叫啥", "你的名字", "你几岁", "你多大",
    // ja — user + persona
    "私の名前", "私は誰", "わたしは誰", "私の誕生日", "私は何歳", "わたしの誕生日",
    "あなたは誰", "あなたの名前", "君は誰", "君の名前", "名前は何", "何歳ですか",
    // en — user + persona
    "who am i", "my name", "my birthday", "how old am i", "where do i live",
    "who are you", "what are you", "your name", "how old are you",
];

/// Possessive attributes ("X的<attr>" / "Xの<attr>") that mark a factual world
/// query when the owner X is **not** a pronoun (so 祥子的生日 counts, 我的生日 /
/// 你的生日 don't). Covers both the zh 的 and the ja の particle.
const ENTITY_ATTRS: &[&str] = &[
    // zh
    "生日", "主唱", "队长", "成员", "身高", "年龄", "价格", "票价", "销量", "票房",
    "排名", "作者", "导演", "国籍", "成立", "出道", "首张", "专辑", "声优", "配音",
    "本名", "原名",
    // ja
    "誕生日", "ボーカル", "メンバー", "身長", "年齢", "声優", "発売日", "値段",
    "作者", "監督", "本名",
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

/// True if `text` should trigger a live web search. Language-agnostic: every
/// keyword layer carries zh + ja + en entries and is matched against the
/// lowercased message (lowercasing leaves CJK untouched, so it only helps
/// English). See the module docs for the precedence.
pub fn needs_search(text: &str) -> bool {
    let lower = text.to_lowercase();

    // 1. Explicit search verbs — the user asked; always search.
    if PROACTIVE.iter().any(|k| lower.contains(k)) {
        return true;
    }

    // 2. Self / persona reference — served from memory / character, not the web.
    if is_self_reference(&lower) {
        return false;
    }

    // 3. Personal chit-chat about us — no web context needed.
    if is_personal_chitchat(text, &lower) {
        return false;
    }

    // 4. Freshness cues (time / info category / English).
    if TIME.iter().any(|k| lower.contains(k))
        || INFO.iter().any(|k| lower.contains(k))
        || ENGLISH.iter().any(|k| lower.contains(k))
    {
        return true;
    }

    // 5. Factual interrogative about an external named entity.
    let has_factual: bool = FACTUAL_CJK.iter().any(|k| lower.contains(k))
        || FACTUAL_JP.iter().any(|k| lower.contains(k))
        || FACTUAL_EN.iter().any(|k| lower.contains(k));
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
    if text.chars().count() <= 12 && BARE_VERBS.iter().any(|k| lower.contains(k)) {
        return true;
    }
    date_re().is_match(text)
}

/// Whether the message asks about the user or the persona (→ local memory /
/// character, never the web). `lower` is the lowercased message.
fn is_self_reference(lower: &str) -> bool {
    SELF_PATTERNS.iter().any(|p| lower.contains(p))
}

/// Whether the message is small talk about us. Two shapes:
///   * a **greeting / affect** phrase (EN/JP) that isn't accompanied by an
///     info/factual cue — "good morning" is chat, "good morning, the weather?"
///     is not;
///   * a **personal pronoun bound to a daily-life verb** with no external entity
///     to look up (the original zh path, now with ja pronouns/verbs).
fn is_personal_chitchat(text: &str, lower: &str) -> bool {
    let carries_search_cue = INFO.iter().any(|k| lower.contains(k))
        || ENGLISH.iter().any(|k| lower.contains(k))
        || FACTUAL_EN.iter().any(|k| lower.contains(k))
        || FACTUAL_JP.iter().any(|k| lower.contains(k));
    if GREETING_CHITCHAT.iter().any(|p| lower.contains(p)) && !carries_search_cue {
        return true;
    }
    let bound = PERSONAL_PRONOUNS.iter().any(|p| text.contains(p))
        && PERSONAL_VERBS.iter().any(|v| text.contains(v));
    bound && !mentions_world_entity(text)
}

/// High-precision heuristic: does the text name an external subject worth a web
/// lookup? Latin proper noun, a katakana proper noun (Japanese foreign names),
/// a quoted title, or a non-pronoun possessive.
fn mentions_world_entity(text: &str) -> bool {
    has_latin_run(text)
        || has_katakana_run(text)
        || has_quoted_title(text)
        || has_entity_possessive(text)
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

/// A run of ≥2 katakana — Japanese renders foreign proper nouns (ブルアカ,
/// ポケモン, ドル) in katakana, a strong "external entity" signal.
fn has_katakana_run(text: &str) -> bool {
    let mut run = 0u32;
    for c in text.chars() {
        let is_kata = matches!(c as u32, 0x30A0..=0x30FF | 0x31F0..=0x31FF | 0xFF66..=0xFF9D);
        if is_kata {
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

/// A possessive "X的<attr>" (zh) or "Xの<attr>" (ja) where X is a non-pronoun
/// owner (祥子的生日 / 祥子の誕生日) — excludes 我的生日 / 你的生日 / 私の….
fn has_entity_possessive(text: &str) -> bool {
    const PRONOUN_CHARS: &str = "你我咱他她它您们私僕俺君";
    let chars: Vec<char> = text.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if (*c != '的' && *c != 'の') || i == 0 {
            continue;
        }
        if PRONOUN_CHARS.contains(chars[i - 1]) {
            continue; // 我的 / 你的 / 私の …
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

    // ── multilingual triggering: Japanese + English ─────────────────────────

    #[test]
    fn triggers_on_japanese_info_time_and_search_verbs() {
        assert!(needs_search("東京の天気は？")); // INFO 天気
        assert!(needs_search("最新のニュース教えて")); // TIME/INFO + PROACTIVE 教えて
        assert!(needs_search("ドル円の為替は今いくら")); // INFO 為替
        assert!(needs_search("ブルアカの発売日を調べて")); // PROACTIVE 調べて
    }

    #[test]
    fn triggers_on_japanese_factual_with_entity_or_no_subject() {
        assert!(needs_search("MyGOのボーカルは誰")); // latin entity + は誰
        assert!(needs_search("ブルアカの声優は誰")); // katakana entity + は誰
        assert!(needs_search("鬼滅の刃はいつ公開されますか")); // no personal subject, short
        assert!(needs_search("祥子の誕生日はいつ")); // kanji-name possessive (の)
    }

    #[test]
    fn japanese_persona_and_chitchat_not_triggered() {
        assert!(!needs_search("あなたは誰")); // persona identity
        assert!(!needs_search("君の名前は？")); // persona identity
        assert!(!needs_search("元気？")); // greeting/affect
        assert!(!needs_search("おはよう、ムツミ")); // greeting + name, no question
        assert!(!needs_search("私は疲れた")); // pronoun + daily-life verb
    }

    #[test]
    fn triggers_on_english_factual_and_freshness() {
        assert!(needs_search("who is the CEO of Nvidia"));
        assert!(needs_search("when does GTA6 come out"));
        assert!(needs_search("USD to JPY exchange rate"));
        assert!(needs_search("what's the weather in Tokyo right now"));
    }

    #[test]
    fn english_persona_and_greetings_not_triggered() {
        assert!(!needs_search("who are you"));
        assert!(!needs_search("what are you doing"));
        assert!(!needs_search("good morning!"));
        assert!(!needs_search("thank you so much"));
    }

    #[test]
    fn katakana_and_ja_possessive_entity_detection() {
        assert!(mentions_world_entity("ブルアカ")); // katakana run
        assert!(mentions_world_entity("ポケモンの新作")); // katakana run
        assert!(mentions_world_entity("祥子の誕生日")); // の possessive, kanji owner
        assert!(!mentions_world_entity("私の誕生日")); // pronoun owner (の)
        assert!(!mentions_world_entity("今日の天気")); // no proper-noun entity
    }
}
