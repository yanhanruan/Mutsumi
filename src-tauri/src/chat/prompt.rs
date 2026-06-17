//! Prompt assembly for Pipeline A (RAG-enhanced chat).
//!
//! Pure and DOM-/IO-free: given the stable persona, the current relationship +
//! profile, the retrieved memories, and the recent history, produce the message
//! list sent to the LLM. Structured as:
//!
//!   1. system — stable persona (cacheable prefix; from [`crate::persona`])
//!   2. system — per-turn dynamic context (locale, relationship, profile, memories)
//!   3. …recent conversation history…
//!   4. user — the new input
//!
//! Keeping (1) separate from (2) means the large persona block stays byte-stable
//! across turns, so the provider can cache it.

use crate::db::memory::ScoredMemory;
use crate::db::state::RelationshipState;
use crate::services::qwen::ChatMessage;

/// Everything needed to build a turn's message list.
pub struct PromptContext<'a> {
    /// Stable base system prompt (persona rules + character docs).
    pub base_persona: &'a str,
    /// Output-language hint: "en" | "zh" | "ja" (anything else → zh).
    pub locale: &'a str,
    pub profile: &'a [(String, String)],
    pub relationship: &'a RelationshipState,
    /// Retrieved facts about the **user**.
    pub memories: &'a [ScoredMemory],
    /// Retrieved memories of **Mutsumi's own** (her feelings toward / promises to
    /// the user). Kept in a separate, labeled block so the two never blur.
    pub self_memories: &'a [ScoredMemory],
    /// Optional real-time web-search context (labeled block) for this turn.
    pub search_context: Option<&'a str>,
    pub history: &'a [ChatMessage],
    pub user_input: &'a str,
    /// Current unix timestamp (seconds), used to label how long ago each memory
    /// was recorded so the model can prefer recent facts when they conflict.
    pub now: i64,
}

/// Assemble the full ordered message list for a chat turn.
pub fn assemble(ctx: &PromptContext) -> Vec<ChatMessage> {
    let mut messages = Vec::with_capacity(ctx.history.len() + 4);
    messages.push(ChatMessage::system(ctx.base_persona.to_string()));
    messages.push(ChatMessage::system(dynamic_context(ctx)));
    // Real-time search results, if any, as their own labeled context message.
    if let Some(search) = ctx.search_context {
        messages.push(ChatMessage::system(search.to_string()));
    }
    messages.extend(ctx.history.iter().cloned());
    messages.push(ChatMessage::user(ctx.user_input.to_string()));
    messages
}

/// Build the per-turn dynamic-context system message.
fn dynamic_context(ctx: &PromptContext) -> String {
    let mut s = String::from("# 当前情境（仅供你参考，不要直接复述给用户）\n");

    s.push_str(&format!("- 回复语言：请用{}回复。\n", lang_name(ctx.locale)));

    s.push_str(&format!(
        "- 与该用户的关系：好感度 {:.0}，信任 {:.0}，当前心情「{}」。\
         请让语气与亲疏相称——好感低则更疏离、更短；好感高则透出一丝暖意，但始终克制。\n",
        ctx.relationship.affection, ctx.relationship.trust, ctx.relationship.mood
    ));

    if !ctx.profile.is_empty() {
        s.push_str("- 你已知的关于用户的事：\n");
        for (k, v) in ctx.profile {
            s.push_str(&format!("  - {k}：{v}\n"));
        }
    }

    if ctx.memories.is_empty() {
        s.push_str("- 关于用户的记忆：暂无。\n");
    } else {
        s.push_str(
            "- 关于用户的记忆（按相关度排序，方括号内为类别与记下的时间。\
             注意：列表顺序不代表新旧；若两条记忆相互矛盾，一律以最近记下的那条为准）：\n",
        );
        for m in ctx.memories {
            s.push_str(&memory_line(m, ctx.now));
        }
    }

    // Mutsumi's own memory — clearly separated so it is never read as a user fact.
    if !ctx.self_memories.is_empty() {
        s.push_str(
            "- 你（睦）自己的记忆——你对这位用户的情感，以及你与对方之间的约定/相处片段（这些是你自己的，不是用户的事实）：\n",
        );
        for m in ctx.self_memories {
            s.push_str(&memory_line(m, ctx.now));
        }
    }

    s
}

/// Render one memory as a `  - [category·when] content` line. Age is from
/// `updated_at` (last written/refreshed), not `created_at`, so a re-stated/merged
/// memory reads as recent and wins the conflict directive.
fn memory_line(m: &ScoredMemory, now: i64) -> String {
    let category = m.memory.category.as_deref().unwrap_or("记忆");
    let when = recency_label(now - m.memory.updated_at);
    format!("  - [{}·{}] {}\n", category, when, m.memory.content)
}

/// A coarse, human-readable "recorded N ago" label for a memory's age (seconds).
/// Deliberately fuzzy — the goal is only to let the model order conflicting
/// memories by recency, not to give exact timestamps.
fn recency_label(age_secs: i64) -> String {
    let secs = age_secs.max(0);
    let mins = secs / 60;
    let hours = secs / 3600;
    let days = secs / 86400;
    if days >= 1 {
        format!("{days}天前")
    } else if hours >= 1 {
        format!("{hours}小时前")
    } else if mins >= 1 {
        format!("{mins}分钟前")
    } else {
        "刚刚".to_string()
    }
}

/// Map a locale code to the Chinese name used in the language directive.
fn lang_name(locale: &str) -> &'static str {
    match locale {
        "en" => "英文",
        "ja" => "日文",
        _ => "中文",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::memory::{Memory, MemoryKind, MemorySubject, ScoredMemory};

    fn scored(content: &str, category: &str) -> ScoredMemory {
        scored_subj(content, category, MemorySubject::User)
    }

    fn scored_subj(content: &str, category: &str, subject: MemorySubject) -> ScoredMemory {
        ScoredMemory {
            memory: Memory {
                id: 1,
                kind: MemoryKind::Observation,
                category: Some(category.into()),
                content: content.into(),
                importance: 0.5,
                embedding: None,
                created_at: 0,
                last_access: 0,
                updated_at: 0,
                superseded: false,
                reflected: false,
                subject,
            },
            score: 0.9,
            relevance: 0.8,
        }
    }

    fn ctx_with<'a>(
        rel: &'a RelationshipState,
        mems: &'a [ScoredMemory],
        hist: &'a [ChatMessage],
    ) -> PromptContext<'a> {
        PromptContext {
            base_persona: "PERSONA",
            locale: "zh",
            profile: &[],
            relationship: rel,
            memories: mems,
            self_memories: &[],
            search_context: None,
            history: hist,
            user_input: "在吗",
            now: 0,
        }
    }

    #[test]
    fn structure_is_systems_history_then_user() {
        let rel = RelationshipState::default();
        let hist = vec![ChatMessage::user("你好"), ChatMessage::assistant("……嗯")];
        let msgs = assemble(&ctx_with(&rel, &[], &hist));

        assert_eq!(msgs.len(), 5); // 2 system + 2 history + 1 user
        assert_eq!(msgs[0].role, "system");
        assert_eq!(msgs[0].content.as_deref(), Some("PERSONA"));
        assert_eq!(msgs[1].role, "system");
        assert_eq!(msgs[2].role, "user"); // history begins
        assert_eq!(msgs[4].role, "user");
        assert_eq!(msgs[4].content.as_deref(), Some("在吗"));
    }

    #[test]
    fn dynamic_context_includes_memories_and_relationship() {
        let rel = RelationshipState {
            affection: 30.0,
            trust: 10.0,
            mood: "平静".into(),
            updated_at: 0,
        };
        let mems = vec![scored("用户喜欢猫", "preference")];
        let msgs = assemble(&ctx_with(&rel, &mems, &[]));
        let dynamic = msgs[1].content.as_deref().unwrap();

        assert!(dynamic.contains("用户喜欢猫"));
        assert!(dynamic.contains("preference"));
        assert!(dynamic.contains("好感度 30"));
        assert!(dynamic.contains("平静"));
    }

    #[test]
    fn recency_label_buckets() {
        assert_eq!(recency_label(-5), "刚刚");
        assert_eq!(recency_label(0), "刚刚");
        assert_eq!(recency_label(30), "刚刚");
        assert_eq!(recency_label(120), "2分钟前");
        assert_eq!(recency_label(3 * 3600), "3小时前");
        assert_eq!(recency_label(5 * 86400), "5天前");
    }

    #[test]
    fn memories_get_recency_label_and_conflict_directive() {
        let rel = RelationshipState::default();
        // A memory last written 5 days before "now" (label reads `updated_at`).
        let mut m = scored("用户只吃苦瓜", "preference");
        m.memory.updated_at = 0;
        let mems = [m];
        let mut ctx = ctx_with(&rel, &mems, &[]);
        ctx.now = 5 * 86400; // 5 days later
        let dynamic = assemble(&ctx)[1].content.clone().unwrap();

        assert!(dynamic.contains("[preference·5天前] 用户只吃苦瓜"));
        // The conflict-resolution directive must be present.
        assert!(dynamic.contains("以最近记下的那条为准"));
    }

    #[test]
    fn search_context_injected_as_system_message() {
        let rel = RelationshipState::default();
        let mut ctx = ctx_with(&rel, &[], &[]);
        ctx.search_context = Some("以下是检索到的实时数据……\n【搜索：天气】东京 晴 20C");
        let msgs = assemble(&ctx);
        // persona, dynamic, search, user
        assert_eq!(msgs.len(), 4);
        assert_eq!(msgs[2].role, "system");
        assert!(msgs[2].content.as_deref().unwrap().contains("东京 晴"));
        assert_eq!(msgs[3].role, "user");
    }

    #[test]
    fn no_search_context_means_no_extra_message() {
        let rel = RelationshipState::default();
        let msgs = assemble(&ctx_with(&rel, &[], &[]));
        assert_eq!(msgs.len(), 3); // persona, dynamic, user — no search block
    }

    #[test]
    fn empty_memories_noted() {
        let rel = RelationshipState::default();
        let msgs = assemble(&ctx_with(&rel, &[], &[]));
        assert!(msgs[1].content.as_deref().unwrap().contains("关于用户的记忆：暂无"));
    }

    #[test]
    fn self_memories_render_in_a_separate_labeled_block() {
        let rel = RelationshipState::default();
        let user_mems = [scored("用户喜欢猫", "preference")];
        let self_mems = [scored_subj("睦答应陪用户调音", "promise", MemorySubject::Mutsumi)];
        let mut ctx = ctx_with(&rel, &user_mems, &[]);
        ctx.self_memories = &self_mems;
        let dynamic = assemble(&ctx)[1].content.clone().unwrap();

        assert!(dynamic.contains("关于用户的记忆")); // user block
        assert!(dynamic.contains("用户喜欢猫"));
        assert!(dynamic.contains("你（睦）自己的记忆")); // self block, distinct
        assert!(dynamic.contains("睦答应陪用户调音"));
    }

    #[test]
    fn locale_maps_to_language_name() {
        assert_eq!(lang_name("en"), "英文");
        assert_eq!(lang_name("ja"), "日文");
        assert_eq!(lang_name("zh"), "中文");
        assert_eq!(lang_name("fr"), "中文"); // fallback
    }
}
