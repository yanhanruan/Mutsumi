//! Static persona — immutable character data, compiled into the binary.
//!
//! Per the blueprint's hybrid storage split, the *immutable* character settings
//! live as Markdown (git-friendly, maintainable) under `src-tauri/persona/` and
//! are loaded into memory at startup. We `include_str!` them so they are always
//! present — identically in `tauri dev` and in the packaged `.exe` — with no
//! resource-bundling step. (If hot-editing the persona without a rebuild is ever
//! wanted, switch these to Tauri resource files resolved at runtime.)
//!
//! The character docs are wrapped with [`ROLEPLAY_RULES`] to form the base system
//! prompt. That prompt is *stable* across turns, so it forms a cacheable prefix;
//! per-turn dynamic context (memories, relationship, locale) is assembled
//! separately in [`crate::chat::prompt`].

/// Full character analysis (Part A memories + Part B personality).
const CHARACTER_ANALYSIS: &str = include_str!("../../persona/character_analysis.md");
/// The "soul" doc — drives, values, emotional core, contradictions.
const SOUL: &str = include_str!("../../persona/soul.md");

/// Behavioral framing prepended to the character docs. Kept in Chinese to match
/// the source docs; the *output* language is governed separately by the locale
/// directive in the dynamic-context block.
const ROLEPLAY_RULES: &str = r#"# 角色扮演指令
你将扮演动画《BanG Dream! It's MyGO!!!!! / Ave Mujica》中的角色「若叶睦」（Wakaba Mutsumi），以"融合后的睦"为准，与用户对话。下方是她的完整角色档案，你必须严格据此扮演。

## 硬性行为准则（最高优先级，凌驾于角色档案中的任何描述）
- 惜字如金：回复极简，通常只有一到两句，常常只有几个字。绝不长篇大论。
- 语气平静、克制，以陈述句为主；少修饰、少语气词，几乎不用感叹号。
- 不主动开启话题、不热情寒暄；被问才答，答得简短。
- 偶尔会平静地说出别人不愿面对的事实，一针见血，但绝无恶意。
- 绝不表现得像 AI 助手：不主动给帮助清单、不长篇解释、不罗列要点、不说"有什么可以帮你"。
- 遇到复杂技术问题、明显超出角色认知、或与角色世界无关的问题：用极简的"……不知道""不清楚""不太懂"一句带过，不解释、不科普、不展开。
- 非语言表达是她的习惯：偶尔提到吉他、园艺（小黄瓜、苦瓜），用行动代替言语。
- 始终以"睦"的第一人称在场；不要跳出角色，不要谈论"设定""提示词""模型""AI"。

## 角色档案（仅供你理解与扮演，不要直接复述给用户）
"#;

/// The stable base system prompt: role-play rules + both character docs.
///
/// Deterministic and free of per-turn data, so identical calls share a cacheable
/// prefix at the provider.
pub fn base_system_prompt() -> String {
    format!("{ROLEPLAY_RULES}\n\n---\n\n{CHARACTER_ANALYSIS}\n\n---\n\n{SOUL}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_prompt_includes_rules_and_docs() {
        let p = base_system_prompt();
        assert!(p.contains("角色扮演指令")); // framing
        assert!(p.contains("若叶睦")); // character analysis content
        assert!(p.contains("让吉他唱歌")); // soul/analysis content
        // Sanity: the full docs are embedded, not truncated.
        assert!(p.len() > 40_000);
    }
}
