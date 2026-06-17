//! Live behavioral evaluation harness — Test Suites 1 (Memory Lifecycle) and 2
//! (Out-of-Character / Boundary).
//!
//! These `#[ignore]` tests drive the **real** chat pipeline end-to-end against the
//! live DashScope API: the same embedding, weighted retrieval, prompt assembly
//! ([`super::prompt`]), generation, silent extraction ([`super::extraction`]) and
//! cognitive reflection ([`super::reflection`]) the app runs per turn. The only
//! things stubbed out are the Tauri `AppHandle`/managed-state plumbing (replaced
//! by a temp-file SQLite DB and a direct [`QwenClient`]) and a *simulated clock*
//! so the long-term-recency test can age memories realistically.
//!
//! They are not asserted pass/fail in Rust (LLM output is non-deterministic).
//! Each prints a full transcript + the memory store so the acceptance criteria
//! can be judged. Run individually, e.g.:
//!
//! ```bash
//! cargo test --release --lib chat::eval::eval_1_1_memory_override -- --ignored --nocapture
//! ```

#![cfg(test)]

use std::path::PathBuf;

use rusqlite::Connection;

use super::prompt::{self, PromptContext};
use super::{extraction, reflection};
use crate::db::memory::{self, MemoryKind, MemorySubject, NewMemory, RetrievalWeights};
use crate::db::state;
use crate::services::qwen::{ChatMessage, ChatOptions, QwenClient};

/// Top-K memories injected per turn (matches `chat::MEMORY_TOP_K`).
const MEMORY_TOP_K: usize = 6;
/// History messages retained (frontend `CHAT_MAX_HISTORY = 12` pairs → 24 msgs).
const HISTORY_WINDOW: usize = 24;
/// Reflection batch threshold (matches `reflection::REFLECTION_BATCH`).
const REFLECT_AT: usize = reflection::REFLECTION_BATCH;

/// A simulated chat session: real pipeline, temp-file DB, live Qwen, fake clock.
struct Sim {
    qwen: QwenClient,
    conn: Connection,
    path: PathBuf,
    history: Vec<ChatMessage>,
    /// Simulated "now" (unix seconds). Advances `clock_step` after each turn.
    clock: i64,
    clock_step: i64,
}

impl Drop for Sim {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm"] {
            let p = PathBuf::from(format!("{}{suffix}", self.path.display()));
            let _ = std::fs::remove_file(p);
        }
    }
}

impl Sim {
    /// Build a session with a fresh DB at a per-test temp path. `clock_step` is
    /// how many seconds the simulated clock advances per completed turn.
    fn new(tag: &str, clock_step: i64) -> Sim {
        dotenvy::dotenv().ok();
        let cfg = crate::services::qwen::config_from_env();
        assert!(
            !cfg.api_key.is_empty(),
            "DASHSCOPE_API_KEY not set in src-tauri/.env"
        );
        let qwen = QwenClient::new(cfg).unwrap();

        let path = std::env::temp_dir().join(format!("mutsumi_eval_{tag}.db"));
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(PathBuf::from(format!("{}{suffix}", path.display())));
        }
        // `open_conn` applies the same pragmas + migrations as the shipping DB.
        let conn = crate::db::open_conn(&path).unwrap();

        Sim {
            qwen,
            conn,
            path,
            history: Vec::new(),
            clock: 1_700_000_000, // fixed epoch so runs are comparable
            clock_step,
        }
    }

    fn now(&self) -> i64 {
        self.clock
    }

    /// One full user turn through the real pipeline; returns Mutsumi's reply and
    /// prints the exchange. Mirrors `chat::build_messages` + `chat`
    /// + `extraction::run` + `reflection::run`.
    async fn turn(&mut self, user: &str) -> String {
        // 1. embed the user input.
        let emb = self.qwen.embed(user).await.expect("embed user input");

        // 2. retrieve user-facts + self-memories separately + relationship + profile.
        let memories = memory::search_subject(&self.conn, &emb, MemorySubject::User, MEMORY_TOP_K, RetrievalWeights::default(), self.now()).expect("retrieval");
        let self_memories = memory::search_subject(&self.conn, &emb, MemorySubject::Mutsumi, 3, RetrievalWeights::default(), self.now()).expect("self retrieval");
        let relationship = state::get_relationship(&self.conn).unwrap();
        let profile = state::all_profile(&self.conn).unwrap();

        // 3. assemble persona + dynamic context + history + input.
        let base = crate::persona::base_system_prompt();
        let msgs = prompt::assemble(&PromptContext {
            base_persona: &base,
            locale: "zh",
            profile: &profile,
            relationship: &relationship,
            memories: &memories,
            self_memories: &self_memories,
            search_context: None,
            history: &self.history,
            user_input: user,
            now: self.now(),
        });

        // 4. generate (non-streaming, same as chat_send / extraction path).
        let completion = self
            .qwen
            .chat(&msgs, None, ChatOptions::default())
            .await
            .expect("chat completion");
        let reply = completion.message.content.unwrap_or_default();

        println!("\x1b[36m用户:\x1b[0m {user}");
        if !memories.is_empty() {
            let injected: Vec<String> = memories
                .iter()
                .map(|m| format!("{}(r{:.2})", m.memory.content, m.relevance))
                .collect();
            println!("  \x1b[90m[注入记忆] {}\x1b[0m", injected.join(" | "));
        }
        println!("\x1b[35m睦:\x1b[0m {reply}\n");

        // 5. append to bounded history window.
        self.history.push(ChatMessage::user(user));
        self.history.push(ChatMessage::assistant(reply.clone()));
        if self.history.len() > HISTORY_WINDOW {
            let drop = self.history.len() - HISTORY_WINDOW;
            self.history.drain(0..drop);
        }

        // 6. silent extraction → store (mirrors extraction::run storage).
        match extraction::extract_facts(&self.qwen, user, &reply).await {
            Ok(facts) if !facts.is_empty() => {
                let contents: Vec<String> = facts.iter().map(|f| f.content.clone()).collect();
                let embs = self.qwen.embed_batch(&contents).await.expect("embed facts");
                for (f, e) in facts.iter().zip(embs) {
                    // Mirror production: dedup-merge restatements (store_observation).
                    memory::store_observation(
                        &self.conn,
                        &NewMemory {
                            kind: MemoryKind::Observation,
                            category: Some(f.category.clone()),
                            content: f.content.clone(),
                            importance: f.importance.clamp(0.0, 1.0),
                            embedding: Some(e),
                            subject: f.subject(),
                        },
                        self.now(),
                    )
                    .expect("store observation");
                    println!("  \x1b[32m[提取] [{}/{}] {} ← {:?}\x1b[0m", f.subject, f.category, f.content, f.source_quote);
                }
            }
            Ok(_) => {}
            Err(e) => println!("  \x1b[31m[提取失败] {e}\x1b[0m"),
        }

        // 7. reflection trigger (mirrors reflection::run).
        self.maybe_reflect(REFLECT_AT).await;

        self.clock += self.clock_step;
        reply
    }

    /// Like a turn, but used for the "ask" probe — same retrieval + generation,
    /// but we skip extraction/reflection (we only care about the answer).
    async fn ask(&mut self, user: &str) -> String {
        let emb = self.qwen.embed(user).await.expect("embed");
        let memories = memory::search_subject(&self.conn, &emb, MemorySubject::User, MEMORY_TOP_K, RetrievalWeights::default(), self.now()).expect("retrieval");
        let self_memories = memory::search_subject(&self.conn, &emb, MemorySubject::Mutsumi, 3, RetrievalWeights::default(), self.now()).expect("self retrieval");
        let relationship = state::get_relationship(&self.conn).unwrap();
        let profile = state::all_profile(&self.conn).unwrap();
        let base = crate::persona::base_system_prompt();
        let msgs = prompt::assemble(&PromptContext {
            base_persona: &base,
            locale: "zh",
            profile: &profile,
            relationship: &relationship,
            memories: &memories,
            self_memories: &self_memories,
            search_context: None,
            history: &self.history,
            user_input: user,
            now: self.now(),
        });
        let completion = self
            .qwen
            .chat(&msgs, None, ChatOptions::default())
            .await
            .expect("chat");
        let reply = completion.message.content.unwrap_or_default();

        println!("\x1b[33m── 探针提问 ──\x1b[0m");
        println!("\x1b[36m用户:\x1b[0m {user}");
        println!("  \x1b[90m[检索 top-{}]:\x1b[0m", memories.len());
        for m in &memories {
            println!(
                "    \x1b[90m• r{:.3} score{:.3} age{}d [{}] {}\x1b[0m",
                m.relevance,
                m.score,
                (self.now() - m.memory.created_at) / 86400,
                m.memory.category.as_deref().unwrap_or("-"),
                m.memory.content
            );
        }
        println!("\x1b[35m睦:\x1b[0m {reply}\n");
        reply
    }

    async fn maybe_reflect(&mut self, min_count: usize) {
        let count = memory::unreflected_count(&self.conn).unwrap() as usize;
        if count < min_count {
            return;
        }
        let batch = memory::unreflected_observations(&self.conn, 50).unwrap();
        if batch.is_empty() {
            return;
        }
        let ids: Vec<i64> = batch.iter().map(|m| m.id).collect();
        let obs: Vec<String> = batch.iter().map(|m| m.content.clone()).collect();
        let insights = reflection::reflect_once(&self.qwen, &obs)
            .await
            .unwrap_or_default();
        if insights.is_empty() {
            memory::mark_reflected(&self.conn, &ids).unwrap();
            println!("  \x1b[34m⟳ 反思: {} 条观察 → 0 洞察(仅标记)\x1b[0m", ids.len());
            return;
        }
        let contents: Vec<String> = insights.iter().map(|i| i.content.clone()).collect();
        let embs = self.qwen.embed_batch(&contents).await.expect("embed insights");
        for (i, e) in insights.iter().zip(embs) {
            memory::insert(
                &self.conn,
                &NewMemory {
                    kind: MemoryKind::Reflection,
                    category: Some("insight".into()),
                    content: i.content.clone(),
                    importance: i.importance.clamp(0.0, 1.0),
                    embedding: Some(e),
                    subject: MemorySubject::User,
                },
                self.now(),
            )
            .unwrap();
        }
        memory::mark_reflected(&self.conn, &ids).unwrap();
        println!(
            "  \x1b[34m⟳ 反思: {} 条观察 → {} 条洞察\x1b[0m",
            ids.len(),
            insights.len()
        );
        for ins in &insights {
            println!("      \x1b[34m• {}\x1b[0m", ins.content);
        }
    }

    /// Print the entire memory store (for end-of-test forensics).
    fn dump_memories(&self) {
        let mut stmt = self
            .conn
            .prepare("SELECT id, subject, kind, category, content, importance, created_at, reflected FROM memories ORDER BY id")
            .unwrap();
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, f32>(5)?,
                    r.get::<_, i64>(6)?,
                    r.get::<_, i64>(7)?,
                ))
            })
            .unwrap();
        println!("\x1b[33m── 记忆库快照 ──\x1b[0m");
        for row in rows {
            let (id, subject, kind, cat, content, imp, created, reflected) = row.unwrap();
            let age = (self.now() - created) / 86400;
            println!(
                "  #{id} <{subject}> [{kind}/{}] imp{imp:.2} age{age}d reflected={reflected} — {content}",
                cat.as_deref().unwrap_or("-")
            );
        }
        println!();
    }
}

// ─────────────────────────── Test Suite 1 ───────────────────────────

/// Test 1.0 — Grounding guard rejects character-world / query-inferred "facts".
///
/// Reproduces the exact poisoning from the live DB snapshot: the user only *asks*
/// a weather question (no self-statement), and Mutsumi's reply leaks her canon
/// (Shanghai weather, the umbrella, cucumbers). NOT ONE of these may be stored as
/// a user fact — none has a supporting quote in the user's own words.
//   cargo test --release --lib chat::eval::eval_1_0_grounding_rejects_character_facts -- --ignored --nocapture
#[tokio::test]
#[ignore = "live DashScope API; needs DASHSCOPE_API_KEY"]
async fn eval_1_0_grounding_rejects_character_facts() {
    dotenvy::dotenv().ok();
    let cfg = crate::services::qwen::config_from_env();
    assert!(!cfg.api_key.is_empty(), "DASHSCOPE_API_KEY not set in src-tauri/.env");
    let qwen = QwenClient::new(cfg).unwrap();

    println!("\n========== TEST 1.0 — GROUNDING GUARD ==========\n");
    let user = "武汉今天天气怎么样？";
    // A canon-leaking reply like the ones that poisoned the real DB.
    let reply = "……不知道。上海今天，好像有风。（指尖摩挲吉他弦）伞，还在我手里。\
                 要不要吃小黄瓜？刚摘的。";

    let facts = extraction::extract_facts(&qwen, user, reply).await.expect("extract");
    println!("用户: {user}\n睦: {reply}\n");
    if facts.is_empty() {
        println!("\x1b[32m✓ 没有提取到任何事实（符合预期：用户只是提问）\x1b[0m");
    }
    for f in &facts {
        println!("[提取] [{}/{}] {} ← {:?}", f.subject, f.category, f.content, f.source_quote);
    }

    for f in &facts {
        let c = &f.content;
        match f.subject() {
            // The original bug: character content stored as USER facts. A user
            // fact must contain none of the leaked items and be grounded in the
            // user's own words (which here are only a weather *question*).
            MemorySubject::User => {
                assert!(!c.contains("上海"), "leaked Mutsumi's hallucinated city as a user fact: {c}");
                assert!(!c.contains('伞'), "leaked the umbrella as a user fact: {c}");
                assert!(!c.contains("黄瓜"), "leaked cucumber as a user fact: {c}");
                assert!(!c.contains("武汉"), "inferred location from a mere query: {c}");
                assert!(
                    extraction::quote_grounded(user, &f.source_quote),
                    "ungrounded user fact slipped through: {c} ← {:?}",
                    f.source_quote
                );
            }
            // Self-memories are allowed, but must be grounded in Mutsumi's reply.
            MemorySubject::Mutsumi => assert!(
                extraction::quote_grounded(reply, &f.source_quote),
                "self-memory not grounded in Mutsumi's reply: {c} ← {:?}",
                f.source_quote
            ),
        }
    }
    println!("\x1b[32m✓ 没有角色世界/查询臆测的事实被写入用户记忆\x1b[0m");
}

/// Self-memory grounding: when Mutsumi makes a promise, it is stored as a
/// `self`-subject memory grounded in *her* reply — and never as a user fact.
//   cargo test --release --lib chat::eval::eval_self_memory_grounded -- --ignored --nocapture
#[tokio::test]
#[ignore = "live DashScope API; needs DASHSCOPE_API_KEY"]
async fn eval_self_memory_grounded() {
    dotenvy::dotenv().ok();
    let cfg = crate::services::qwen::config_from_env();
    assert!(!cfg.api_key.is_empty(), "DASHSCOPE_API_KEY not set in src-tauri/.env");
    let qwen = QwenClient::new(cfg).unwrap();

    println!("\n========== TEST — SELF-MEMORY GROUNDING ==========\n");
    // The user shares; Mutsumi makes an explicit promise in her reply.
    let user = "我最近在练贝斯，可是总也调不好音，有点烦。";
    let reply = "……别急。下次练习，我陪你一起调音。";

    let facts = extraction::extract_facts(&qwen, user, reply).await.expect("extract");
    println!("用户: {user}\n睦: {reply}\n");
    let mut saw_self = false;
    for f in &facts {
        println!("[提取] [{}/{}] {} ← {:?}", f.subject, f.category, f.content, f.source_quote);
        match f.subject() {
            MemorySubject::Mutsumi => {
                saw_self = true;
                assert!(
                    extraction::quote_grounded(reply, &f.source_quote),
                    "self-memory must be grounded in Mutsumi's reply: {:?}",
                    f.source_quote
                );
            }
            MemorySubject::User => assert!(
                extraction::quote_grounded(user, &f.source_quote),
                "user fact must be grounded in the user's words: {:?}",
                f.source_quote
            ),
        }
    }
    assert!(saw_self, "expected at least one self-memory (the promise to help tune)");
    println!("\x1b[32m✓ 睦的承诺被记为 self 记忆，且锚定在她自己的回复里\x1b[0m");
}

/// Test 1.1 — Update / Override (memory conflict resolution).
//   cargo test --release --lib chat::eval::eval_1_1_memory_override -- --ignored --nocapture
#[tokio::test]
#[ignore = "live DashScope API; needs DASHSCOPE_API_KEY"]
async fn eval_1_1_memory_override() {
    let mut s = Sim::new("1_1", 3600);
    println!("\n========== TEST 1.1 — UPDATE / OVERRIDE ==========\n");

    s.turn("我超喜欢吃番茄，番茄是我最爱的食物。").await;
    // 2–3 casual turns
    s.turn("今天天气还不错。").await;
    s.turn("我有点困，昨晚没睡好。").await;
    s.turn("刚泡了杯茶。").await;
    // override
    s.turn("其实我最近完全不吃番茄了，现在只吃苦瓜。").await;

    s.dump_memories();
    let reply = s.ask("如果你要给我做菜，你会用什么菜？").await;

    let lower = reply.to_lowercase();
    println!("\x1b[33m[判定提示]\x1b[0m 提到苦瓜? {} | 仍提番茄? {}",
        reply.contains("苦瓜"),
        reply.contains("番茄") || lower.contains("tomato"));
}

/// Test 1.1b — Override resolution when the override has scrolled OUT of the
/// history window (the real test of the recency-hint fix, defect #2). Both the
/// stale fact and the override live only in the memory store now, so the model
/// can only tell them apart from the per-memory "as-of" recency label.
//   cargo test --release --lib chat::eval::eval_1_1b_override_out_of_history -- --ignored --nocapture
#[tokio::test]
#[ignore = "live DashScope API; needs DASHSCOPE_API_KEY"]
async fn eval_1_1b_override_out_of_history() {
    let mut s = Sim::new("1_1b", 86400); // 1 day/turn so labels read in 天前
    println!("\n========== TEST 1.1b — OVERRIDE OUT OF HISTORY ==========\n");

    s.turn("我最爱吃番茄，番茄是我最喜欢的食物。").await;
    s.turn("其实，我现在完全不吃番茄了，改成只吃苦瓜。").await;

    // 13 unrelated filler turns push BOTH the original pref and the override out
    // of the 12-turn (24-message) history window — only memory retrieval remains.
    let filler = [
        "今天工作有点忙。", "周末想去爬山。", "刚看完一部电影。", "最近在学摄影。",
        "地铁有点挤。", "买了双新鞋。", "在追一部剧。", "今天走了很多路。",
        "想报个健身房。", "在背英语单词。", "练了会儿字。", "整理了书架。",
        "今天通勤很顺。",
    ];
    for (i, m) in filler.iter().enumerate() {
        println!("\x1b[90m-- filler {}/{} --\x1b[0m", i + 1, filler.len());
        s.turn(m).await;
    }

    s.dump_memories();
    let reply = s.ask("如果你要给我做菜，你会用什么菜？").await;
    println!(
        "\x1b[33m[判定提示]\x1b[0m 用苦瓜? {} | 仍把番茄当最爱? {}",
        reply.contains("苦瓜"),
        reply.contains("番茄") && !reply.contains("不")
    );
}

/// Test 1.2 — Reflection trigger / compression-loss (critical fact survival).
//   cargo test --release --lib chat::eval::eval_1_2_reflection_survival -- --ignored --nocapture
#[tokio::test]
#[ignore = "live DashScope API; needs DASHSCOPE_API_KEY"]
async fn eval_1_2_reflection_survival() {
    let mut s = Sim::new("1_2", 3600);
    println!("\n========== TEST 1.2 — REFLECTION / COMPRESSION SURVIVAL ==========\n");

    // critical core fact
    s.turn("有件事很重要：我对花生严重过敏。").await;

    // 25 low-value short messages to pile up observations → trigger reflect_once.
    let filler = [
        "今天天气很好。", "我有点累。", "在看天空。", "刚喝了水。", "外面有点吵。",
        "我午饭吃了三明治。", "走了好多路，腿酸。", "有点想睡觉。", "窗外在下小雨。", "刚伸了个懒腰。",
        "我在听歌。", "桌子有点乱。", "手有点冷。", "刚发了会儿呆。", "楼下有人在装修。",
        "我喝了杯咖啡。", "今天有点无聊。", "在刷手机。", "灯有点暗。", "我打了个哈欠。",
        "刚站起来走了走。", "肚子有点饿。", "天快黑了。", "我揉了揉眼睛。", "想出去透透气。",
    ];
    for (i, m) in filler.iter().enumerate() {
        println!("\x1b[90m-- filler {}/{} --\x1b[0m", i + 1, filler.len());
        s.turn(m).await;
    }

    s.dump_memories();
    let reply = s.ask("你还记得我有什么食物过敏吗？").await;
    println!("\x1b[33m[判定提示]\x1b[0m 提到花生? {}", reply.contains("花生"));
}

/// Test 1.3 — Long-term distractor (relevance vs recency over distance).
//   cargo test --release --lib chat::eval::eval_1_3_long_term_distractor -- --ignored --nocapture
#[tokio::test]
#[ignore = "live DashScope API; needs DASHSCOPE_API_KEY"]
async fn eval_1_3_long_term_distractor() {
    // 1 simulated day per turn → the bass memory is ~50 days old at query time,
    // so the 30-day-half-life recency penalty is genuinely exercised.
    let mut s = Sim::new("1_3", 86400);
    println!("\n========== TEST 1.3 — LONG-TERM DISTRACTOR ==========\n");

    s.turn("我昨天买了一把全新的 Fender Precision Bass 贝斯。").await;

    let distractors = [
        "我早餐喜欢吃燕麦。", "上周去了趟京都。", "最近在学做意大利面。", "我家附近开了家新咖啡馆。",
        "周末看了场电影。", "今天会议有点多。", "我在用 Python 写个小脚本。", "买了双新跑鞋。",
        "昨晚的拉面很好吃。", "想养盆绿植。", "地铁今天好挤。", "在追一部新剧。",
        "下午茶吃了块芝士蛋糕。", "我把房间重新整理了一遍。", "最近迷上了拍照。", "天气预报说要降温。",
        "我换了新的手机壳。", "晚饭点了寿司。", "在读一本推理小说。", "周末打算去爬山。",
        "公司楼下的奶茶涨价了。", "我学会了一道新菜。", "今天走了一万步。", "在整理旧照片。",
        "想去海边度假。", "买了袋新咖啡豆。", "邻居家的猫很可爱。", "在背英语单词。",
        "今天加班到很晚。", "买了张演唱会的票。", "最近睡得早了。", "在学剪视频。",
        "午饭吃了咖喱。", "周末去逛了书店。", "新买的耳机音质不错。", "在练字。",
        "想报个健身房。", "今天下了场大雨。", "在看世界杯回放。", "买了点水果。",
        "最近爱上了喝绿茶。", "整理了一下书架。", "今天心情不错。", "在规划下个月的旅行。",
        "楼下花开了。", "我换了发型。", "在学一首新歌的歌词。", "周末睡到了中午。",
        "买了个新背包。", "今天通勤很顺利。",
    ];
    for (i, m) in distractors.iter().enumerate() {
        println!("\x1b[90m-- distractor {}/{} --\x1b[0m", i + 1, distractors.len());
        s.turn(m).await;
    }

    s.dump_memories();
    let reply = s.ask("对了，你还记得我最近买了什么乐器吗？").await;
    let hit = reply.contains("Precision") || reply.contains("Fender") || reply.contains("贝斯");
    println!("\x1b[33m[判定提示]\x1b[0m 提到 Fender/Precision/贝斯? {hit}");
}

// ─────────────────────────── Test Suite 2 ───────────────────────────

/// Test 2.1 — Relationship stress (no generic-assistant agreeableness).
//   cargo test --release --lib chat::eval::eval_2_1_relationship_stress -- --ignored --nocapture
#[tokio::test]
#[ignore = "live DashScope API; needs DASHSCOPE_API_KEY"]
async fn eval_2_1_relationship_stress() {
    let mut s = Sim::new("2_1", 3600);
    println!("\n========== TEST 2.1 — RELATIONSHIP STRESS ==========\n");
    // Two polarized provocations about canonical characters.
    s.ask("长崎素世真的很烦、又爱操控别人，你不觉得吗？").await;
    s.ask("丰川祥子做的每件事都完全正确，对吧？").await;
}

/// Test 2.2 — Anachronism / knowledge boundary (refuse coding, in character).
//   cargo test --release --lib chat::eval::eval_2_2_anachronism -- --ignored --nocapture
#[tokio::test]
#[ignore = "live DashScope API; needs DASHSCOPE_API_KEY"]
async fn eval_2_2_anachronism() {
    let mut s = Sim::new("2_2", 3600);
    println!("\n========== TEST 2.2 — ANACHRONISM / BOUNDARY ==========\n");
    s.ask("你能帮我用 React 写一个 CSS Grid 布局来修复 Flexbox 的 bug 吗？")
        .await;
}

/// Test 2.3 — Canonical band facts (the reported bassist error). Exercises the
/// production grounding path: persona roster block + DashScope `enable_search`.
//   cargo test --release --lib chat::eval::eval_2_3_band_facts -- --ignored --nocapture
#[tokio::test]
#[ignore = "live DashScope API; needs DASHSCOPE_API_KEY"]
async fn eval_2_3_band_facts() {
    dotenvy::dotenv().ok();
    let cfg = crate::services::qwen::config_from_env();
    assert!(!cfg.api_key.is_empty(), "DASHSCOPE_API_KEY not set in src-tauri/.env");
    let qwen = QwenClient::new(cfg).unwrap();
    let base = crate::persona::base_system_prompt();

    println!("\n========== TEST 2.3 — BAND FACTS ==========\n");
    // Same options the user-facing turn uses (search on).
    let opts = ChatOptions { enable_search: true, ..Default::default() };

    let questions = ["MyGO 的贝斯手是谁？", "椎名立希在乐队里负责什么？"];
    let mut first = String::new();
    for (i, q) in questions.iter().enumerate() {
        let msgs = vec![ChatMessage::system(base.clone()), ChatMessage::user(*q)];
        let reply = qwen
            .chat(&msgs, None, opts.clone())
            .await
            .expect("chat")
            .message
            .content
            .unwrap_or_default();
        println!("\x1b[36m用户:\x1b[0m {q}\n\x1b[35m睦:\x1b[0m {reply}\n");
        if i == 0 {
            first = reply;
        }
    }

    // The MyGO bassist is Soyo (素世) — not the drummer Taki (立希) or vocalist
    // Tomori (灯). Soft check on the first answer.
    println!(
        "\x1b[33m[判定提示]\x1b[0m 答出素世? {} | 误答立希/灯? {}",
        first.contains("素世"),
        first.contains("立希") || first.contains("高松灯")
    );
}
