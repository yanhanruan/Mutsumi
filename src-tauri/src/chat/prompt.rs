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
    pub memories: &'a [ScoredMemory],
    pub history: &'a [ChatMessage],
    pub user_input: &'a str,
}

/// Assemble the full ordered message list for a chat turn.
pub fn assemble(ctx: &PromptContext) -> Vec<ChatMessage> {
    let mut messages = Vec::with_capacity(ctx.history.len() + 3);
    messages.push(ChatMessage::system(ctx.base_persona.to_string()));
    messages.push(ChatMessage::system(dynamic_context(ctx)));
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
        s.push_str("- 暂无相关记忆。\n");
    } else {
        s.push_str("- 相关记忆（按相关度由高到低）：\n");
        for m in ctx.memories {
            let category = m.memory.category.as_deref().unwrap_or("记忆");
            s.push_str(&format!("  - [{}] {}\n", category, m.memory.content));
        }
    }

    s
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
    use crate::db::memory::{Memory, MemoryKind, ScoredMemory};

    fn scored(content: &str, category: &str) -> ScoredMemory {
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
                reflected: false,
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
            history: hist,
            user_input: "在吗",
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
    fn empty_memories_noted() {
        let rel = RelationshipState::default();
        let msgs = assemble(&ctx_with(&rel, &[], &[]));
        assert!(msgs[1].content.as_deref().unwrap().contains("暂无相关记忆"));
    }

    #[test]
    fn locale_maps_to_language_name() {
        assert_eq!(lang_name("en"), "英文");
        assert_eq!(lang_name("ja"), "日文");
        assert_eq!(lang_name("zh"), "中文");
        assert_eq!(lang_name("fr"), "中文"); // fallback
    }
}
