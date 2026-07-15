//! Benchmark / evaluation harness for the chat-memory system.
//!
//! All entries are `#[ignore]`d so `cargo test` stays fast/offline. Run a bench
//! explicitly and collect the printed metrics:
//!
//! Offline (no network):
//!   cargo test --lib bench_retrieval_latency -- --ignored --nocapture
//!   cargo test --lib bench_db_size          -- --ignored --nocapture
//!   cargo test --lib bench_cost_reduction   -- --ignored --nocapture
//!
//! Live (needs DASHSCOPE_API_KEY in src-tauri/.env):
//!   cargo test --lib bench_retrieval_accuracy     -- --ignored --nocapture
//!   cargo test --lib bench_reflection_perf        -- --ignored --nocapture
//!   cargo test --lib bench_extraction             -- --ignored --nocapture
//!   cargo test --lib bench_long_term_consistency  -- --ignored --nocapture
//!
//! NOTE: retrieval is in-Rust brute-force cosine (NOT sqlite-vec) — these
//! numbers describe that implementation.

#![cfg(test)]

use std::time::Instant;

use rusqlite::Connection;

use crate::chat::extraction::{self, extract_memory_tool, EXTRACTION_SYSTEM};
use crate::chat::reflection::{self, record_insight_tool, REFLECTION_SYSTEM};
use crate::db::memory::{self, MemoryKind, MemorySubject, NewMemory, RetrievalWeights};
use crate::db::{now, open_conn};
use crate::services::qwen::{config_from_env, ChatMessage, ChatOptions, QwenClient};

const DIM: usize = 1024;
const DAY: i64 = 24 * 3600;

// ── small deterministic helpers (no external rng) ───────────────────

fn next_rand(state: &mut u64) -> f32 {
    // xorshift64
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    ((x >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0 // ~[-1, 1]
}

fn random_embedding(seed: u64) -> Vec<f32> {
    let mut s = seed | 1;
    (0..DIM).map(|_| next_rand(&mut s)).collect()
}

fn percentile(sorted_ms: &[f64], p: f64) -> f64 {
    if sorted_ms.is_empty() {
        return 0.0;
    }
    let idx = ((p / 100.0) * (sorted_ms.len() - 1) as f64).round() as usize;
    sorted_ms[idx]
}

fn mean(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        0.0
    } else {
        xs.iter().sum::<f64>() / xs.len() as f64
    }
}

fn temp_db(tag: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("mutsumi_bench_{tag}_{}.db", std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

fn cleanup(path: &std::path::Path) {
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
    }
}

/// Insert `n` memories with random embeddings inside one transaction.
fn seed_random(conn: &Connection, n: usize) {
    conn.execute_batch("BEGIN").unwrap();
    let ts = now();
    for i in 0..n {
        memory::insert(
            conn,
            &NewMemory {
                kind: MemoryKind::Observation,
                category: Some("preference".into()),
                content: format!("用户的第 {i} 条记忆，关于一些日常偏好与习惯。"),
                importance: 0.5,
                embedding: Some(random_embedding(i as u64 + 1)),
                subject: MemorySubject::User,
            },
            ts,
        )
        .unwrap();
    }
    conn.execute_batch("COMMIT").unwrap();
}

// ── (memory, query) evaluation dataset for accuracy / consistency ───
// Each `query` should semantically retrieve its paired `fact`.

const PAIRS: &[(&str, &str)] = &[
    ("用户喜欢喝抹茶拿铁", "我平时爱喝什么咖啡"),
    ("用户养了一只叫团子的猫", "我家的宠物叫什么名字"),
    ("用户是一名后端程序员", "我的职业是什么"),
    ("用户住在上海", "我住在哪个城市"),
    ("用户的生日是三月十二日", "我哪天过生日"),
    ("用户对花生过敏", "我有什么食物过敏"),
    ("用户喜欢周末去爬山", "我周末通常做什么"),
    ("用户最喜欢的乐队是 Ave Mujica", "我最喜欢哪个乐队"),
    ("用户最近在学习日语", "我最近在学什么语言"),
    ("用户有一个妹妹", "我家里还有什么人"),
    ("用户每天早上跑步", "我有什么晨间习惯"),
    ("用户很讨厌香菜", "我不爱吃什么菜"),
    ("用户用的是 MacBook 笔记本", "我平时用什么电脑"),
    ("用户喜欢看科幻电影", "我喜欢看哪类电影"),
    ("用户的工作经常需要加班", "我的工作强度大不大"),
    ("用户会弹吉他", "我会演奏什么乐器"),
    ("用户在阳台种了多肉植物", "我在阳台养了什么植物"),
    ("用户喜欢喝乌龙茶", "我喜欢喝哪种茶"),
    ("用户习惯晚上熬夜", "我的作息规律吗"),
    ("用户最近在减肥健身", "我最近在做什么健身计划"),
    ("用户喜欢去日本旅行", "我喜欢去什么地方旅游"),
    ("用户喜欢吃很辣的食物", "我吃饭口味偏好如何"),
    ("用户开一辆蓝色的车", "我的车是什么颜色"),
    ("用户的爱好是摄影", "我平时喜欢拍照吗"),
    ("用户从来不喝酒", "我喝不喝酒"),
    ("用户更喜欢猫而不是狗", "我更喜欢猫还是狗"),
    ("用户正在准备考研", "我最近在准备什么考试"),
    ("用户最喜欢冬天", "我最喜欢哪个季节"),
    ("用户工作时喜欢听轻音乐", "我工作时喜欢听什么音乐"),
    ("用户左撇子，习惯用左手写字", "我用哪只手写字"),
];

fn client_or_skip() -> Option<QwenClient> {
    dotenvy::dotenv().ok();
    let cfg = config_from_env();
    if cfg.api_key.is_empty() {
        eprintln!("SKIP: DASHSCOPE_API_KEY not set");
        return None;
    }
    QwenClient::new(cfg).ok()
}

// ════════════════════════════════════════════════════════════════════
// #1 Vector retrieval latency (offline)
// ════════════════════════════════════════════════════════════════════
#[test]
#[ignore = "benchmark"]
fn bench_retrieval_latency() {
    use crate::db::index::MemoryIndexCache;

    const ITERS: usize = 100;
    let w = RetrievalWeights::default();

    // Time `ITERS` queries through `run` and print a p50/p95/mean/max line.
    fn report(label: &str, n: usize, mut run: impl FnMut(&[f32])) {
        // Warm-up (page cache for brute force; first index build for the cache).
        run(&random_embedding(7));
        let mut times = Vec::with_capacity(ITERS);
        for it in 0..ITERS {
            let q = random_embedding(1_000 + it as u64);
            let t = Instant::now();
            run(&q);
            times.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "  {label:<22} N={n:>6}: p50={:>7.2}ms  p95={:>7.2}ms  mean={:>7.2}ms  max={:>7.2}ms",
            percentile(&times, 50.0),
            percentile(&times, 95.0),
            mean(&times),
            times.last().copied().unwrap_or(0.0),
        );
    }

    println!("\n=== #1 Retrieval latency ({DIM}-dim; full search() incl. top-K fetch + touch) ===");
    println!("(before = brute-force scan+decode every query; after = cached SIMD index)");
    for &n in &[1000usize, 5000, 10000] {
        let path = temp_db(&format!("lat{n}"));
        let conn = open_conn(&path).unwrap();
        seed_random(&conn, n);

        // Before: the original brute-force path (re-decodes all BLOBs each query).
        report("before (brute-force)", n, |q| {
            let _ = memory::search(&conn, q, 6, w, now()).unwrap();
        });

        // After: the in-RAM SIMD index. Built once on the first (warm-up) call;
        // every subsequent query is served from cache (fingerprint is stable).
        let cache = MemoryIndexCache::default();
        report("after  (cached SIMD)", n, |q| {
            let _ = cache.search(&conn, q, 6, w, now()).unwrap();
        });

        drop(conn);
        cleanup(&path);
    }
}

// ════════════════════════════════════════════════════════════════════
// #2 Database size growth (offline)
// ════════════════════════════════════════════════════════════════════
#[test]
#[ignore = "benchmark"]
fn bench_db_size() {
    println!("\n=== #2 Database size ({DIM}-dim f32 embeddings, BLOB) ===");
    for &n in &[1000usize, 5000, 10000] {
        let path = temp_db(&format!("size{n}"));
        let conn = open_conn(&path).unwrap();
        seed_random(&conn, n);
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").ok();
        drop(conn);

        let mut bytes = 0u64;
        for suffix in ["", "-wal", "-shm"] {
            if let Ok(m) = std::fs::metadata(format!("{}{}", path.display(), suffix)) {
                bytes += m.len();
            }
        }
        println!(
            "N={n:>6}: db={:>7.2} MB  ({:>5.0} bytes/entry)",
            bytes as f64 / 1e6,
            bytes as f64 / n as f64,
        );
        cleanup(&path);
    }
}

// ════════════════════════════════════════════════════════════════════
// #3 Retrieval accuracy — Recall@5 + MRR (live embeddings)
// ════════════════════════════════════════════════════════════════════
#[tokio::test]
#[ignore = "live benchmark"]
async fn bench_retrieval_accuracy() {
    let Some(qwen) = client_or_skip() else { return };
    println!("\n=== #3 Retrieval accuracy ({} query/memory pairs) ===", PAIRS.len());

    let facts: Vec<String> = PAIRS.iter().map(|(f, _)| f.to_string()).collect();
    let queries: Vec<String> = PAIRS.iter().map(|(_, q)| q.to_string()).collect();
    let fact_emb = qwen.embed_batch(&facts).await.expect("embed facts");
    let query_emb = qwen.embed_batch(&queries).await.expect("embed queries");

    let path = temp_db("acc");
    let conn = open_conn(&path).unwrap();
    let mut ids = Vec::new();
    for (i, emb) in fact_emb.iter().enumerate() {
        let id = memory::insert(
            &conn,
            &NewMemory {
                kind: MemoryKind::Observation,
                category: Some("fact".into()),
                content: facts[i].clone(),
                importance: 0.5,
                embedding: Some(emb.clone()),
                subject: MemorySubject::User,
            },
            now(),
        )
        .unwrap();
        ids.push(id);
    }

    let n = PAIRS.len();
    let mut hits_at_5 = 0;
    let mut mrr_sum = 0.0;
    for (i, q) in query_emb.iter().enumerate() {
        // Rank ALL memories; find the position of the correct one.
        let ranked = memory::search(&conn, q, n, RetrievalWeights::default(), now()).unwrap();
        let rank = ranked.iter().position(|s| s.memory.id == ids[i]).map(|p| p + 1);
        match rank {
            Some(r) => {
                if r <= 5 {
                    hits_at_5 += 1;
                }
                mrr_sum += 1.0 / r as f64;
            }
            None => {}
        }
    }
    println!(
        "Recall@5 = {:.3} ({}/{})   MRR = {:.3}",
        hits_at_5 as f64 / n as f64,
        hits_at_5,
        n,
        mrr_sum / n as f64,
    );
    drop(conn);
    cleanup(&path);
}

// ════════════════════════════════════════════════════════════════════
// #4 Reflection task performance (live)
// ════════════════════════════════════════════════════════════════════
#[tokio::test]
#[ignore = "live benchmark"]
async fn bench_reflection_perf() {
    let Some(qwen) = client_or_skip() else { return };
    let fragments: Vec<&str> = PAIRS.iter().take(20).map(|(f, _)| *f).collect();
    println!("\n=== #4 Reflection performance ({} fragments/run, 3 runs) ===", fragments.len());

    let listed = fragments.iter().map(|o| format!("- {o}")).collect::<Vec<_>>().join("\n");
    let messages = vec![
        ChatMessage::system(REFLECTION_SYSTEM),
        ChatMessage::user(format!("观察记录：\n{listed}")),
    ];
    let tools = [record_insight_tool()];

    for run in 1..=3 {
        let t = Instant::now();
        let c = qwen.chat(&messages, Some(&tools), ChatOptions::default()).await.expect("reflect");
        let dt = t.elapsed().as_secs_f32();
        let insights = c
            .message
            .tool_calls
            .as_ref()
            .map(|calls| reflection::parse_insights(calls))
            .unwrap_or_default();
        let tok = c
            .usage
            .map(|u| format!("{} prompt + {} completion = {} total", u.prompt_tokens, u.completion_tokens, u.total_tokens))
            .unwrap_or_else(|| "n/a".into());
        println!("run{run}: {dt:.2}s  {} frags -> {} insights  | tokens: {tok}", fragments.len(), insights.len());
        for ins in &insights {
            println!("        • {}", ins.content);
        }
    }
}

// ════════════════════════════════════════════════════════════════════
// #5 Function-calling extraction reliability (live)
// ════════════════════════════════════════════════════════════════════
#[tokio::test]
#[ignore = "live benchmark"]
async fn bench_extraction() {
    let Some(qwen) = client_or_skip() else { return };

    // Conversations: most carry ≥1 durable fact; a couple are pure chit-chat.
    let convos: &[(&str, &str)] = &[
        ("我叫阿伦，是个后端工程师", "……你好。"),
        ("我超喜欢喝抹茶拿铁，每天一杯", "嗯，记住了。"),
        ("我养了只猫，叫团子", "团子……可爱的名字。"),
        ("我对花生过敏，吃了会很难受", "那要小心。"),
        ("周末我一般去爬山", "……户外。"),
        ("我在学日语，准备考 N2", "加油。"),
        ("今天天气真好啊", "……嗯。"),
        ("我住在上海浦东", "知道了。"),
        ("我不太喝酒", "……"),
        ("最近在减肥，每天跑步", "坚持下去。"),
        ("谢谢你陪我聊天", "……不用谢。"),
        ("我妹妹也喜欢这个乐队", "原来你有妹妹。"),
    ];
    println!("\n=== #5 Extraction reliability ({} conversations) ===", convos.len());

    let tools = [extract_memory_tool()];
    let mut total_calls = 0usize;
    let mut valid_facts = 0usize;
    let mut times = Vec::new();
    for (u, r) in convos {
        let messages = vec![
            ChatMessage::system(EXTRACTION_SYSTEM),
            ChatMessage::user(format!("用户：{u}\n睦：{r}")),
        ];
        let t = Instant::now();
        let c = qwen.chat(&messages, Some(&tools), ChatOptions::default()).await.expect("extract");
        times.push(t.elapsed().as_secs_f64() * 1000.0);
        if let Some(calls) = &c.message.tool_calls {
            let named = calls.iter().filter(|c| c.function.name == "extract_memory").count();
            total_calls += named;
            valid_facts += extraction::parse_facts(calls).len();
        }
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let success = if total_calls == 0 { 0.0 } else { valid_facts as f64 / total_calls as f64 };
    println!(
        "valid-JSON success = {:.3} ({}/{} tool calls)   avg facts/convo = {:.2}",
        success,
        valid_facts,
        total_calls,
        valid_facts as f64 / convos.len() as f64,
    );
    println!(
        "extraction call latency: p50={:.0}ms p95={:.0}ms (runs OFF the response path — fire-and-forget)",
        percentile(&times, 50.0),
        percentile(&times, 95.0),
    );
}

// ════════════════════════════════════════════════════════════════════
// #7 Long-term consistency — recall an OLD memory past 20 fresh ones (live)
// ════════════════════════════════════════════════════════════════════
#[tokio::test]
#[ignore = "live benchmark"]
async fn bench_long_term_consistency() {
    let Some(qwen) = client_or_skip() else { return };
    println!("\n=== #7 Long-term consistency (target aged 30d, 20 fresh distractors) ===");

    let facts: Vec<String> = PAIRS.iter().map(|(f, _)| f.to_string()).collect();
    let queries: Vec<String> = PAIRS.iter().map(|(_, q)| q.to_string()).collect();
    let fact_emb = qwen.embed_batch(&facts).await.expect("embed facts");
    let query_emb = qwen.embed_batch(&queries).await.expect("embed queries");

    let n = PAIRS.len();
    let trials = n.min(15);
    let mut hits = 0;
    for i in 0..trials {
        let path = temp_db(&format!("consist{i}"));
        let conn = open_conn(&path).unwrap();
        // Target: aged 30 days.
        let target_id = memory::insert(
            &conn,
            &NewMemory {
                kind: MemoryKind::Observation,
                category: Some("fact".into()),
                content: facts[i].clone(),
                importance: 0.5,
                embedding: Some(fact_emb[i].clone()),
                subject: MemorySubject::User,
            },
            now() - 30 * DAY,
        )
        .unwrap();
        // 20 fresh distractors (the other facts).
        for d in (0..n).filter(|&d| d != i).take(20) {
            memory::insert(
                &conn,
                &NewMemory {
                    kind: MemoryKind::Observation,
                    category: Some("fact".into()),
                    content: facts[d].clone(),
                    importance: 0.5,
                    embedding: Some(fact_emb[d].clone()),
                    subject: MemorySubject::User,
                },
                now(),
            )
            .unwrap();
        }
        let ranked = memory::search(&conn, &query_emb[i], 5, RetrievalWeights::default(), now()).unwrap();
        if ranked.iter().any(|s| s.memory.id == target_id) {
            hits += 1;
        }
        drop(conn);
        cleanup(&path);
    }
    println!(
        "old-memory recall@5 = {:.3} ({}/{}) — relevance overcomes recency decay",
        hits as f64 / trials as f64,
        hits,
        trials,
    );
}

// ════════════════════════════════════════════════════════════════════
// S1 Search intent-gate precision / recall (offline)
// ════════════════════════════════════════════════════════════════════
// Hand-labeled messages. `true` = answering well needs a live web lookup
// (current events / prices / weather / schedules / explicit search request);
// `false` = chit-chat, emotional, persona, or a fixed fact she already knows.
// Deliberately includes the hard cases: casual messages that happen to contain
// a time/info word (现在/今天/演唱会 …) and the 检查/调查/审查/搜罗 negatives that
// embed 查/搜, plus keyword-less factual questions the keyword gate cannot see.
const INTENT_LABELS: &[(&str, bool)] = &[
    // ── should trigger (need fresh/external info) ──────────────────────
    ("帮我查一下明天东京的天气", true),
    ("搜索最近的 Ave Mujica 演唱会安排", true),
    ("现在美元对人民币的汇率是多少", true),
    ("今天有什么国际新闻", true),
    ("比特币今天的价格是多少", true),
    ("iPhone 16 的发布日期是什么时候", true),
    ("查查上海到北京的高铁票价", true),
    ("最新的 macOS 版本号是多少", true),
    ("2025-06-01 上映了哪些电影", true),
    ("帮我搜一下附近好吃的拉面店", true),
    ("目前 A 股大盘的行情怎么样", true),
    ("今天的英超比赛比分出来了吗", true),
    ("查一下这周的黄金价格走势", true),
    ("特斯拉股价现在多少", true),
    ("明天会下雨吗", true),
    ("英伟达最新显卡的售价", true),
    ("今日金价多少一克", true),
    ("帮我查下我的快递到哪了", true),
    ("what's the latest news about the election", true),
    ("today's weather in Osaka please", true),
    // keyword-less factual queries — the gate has no cue for these (expected misses):
    ("谁是现任的日本首相", true),
    ("最近油价又涨了吗", true),
    ("英伟达的市值超过苹果了吗", true),
    // ── should NOT trigger (chat / emotion / persona / fixed knowledge) ─
    ("你在做什么呢", false),
    ("我有点累了，想靠你休息一下", false),
    ("谢谢你一直陪着我", false),
    ("你最喜欢吃什么甜点", false),
    ("你能记住我说过的话吗", false),
    ("讲个笑话给我听好不好", false),
    ("你觉得我该学钢琴还是吉他", false),
    ("我对这个世界总是充满好奇", false),
    ("你会一直在我身边吗", false),
    ("我最近压力好大，有点喘不过气", false),
    // hard negatives — embed 查/搜 inside other words, must not trigger:
    ("帮我检查一下这段代码有没有 bug", false),
    ("我们公司最近在做一个用户调查问卷", false),
    ("这件事得好好审查一下才行", false),
    ("搜罗了半天也没找到合适的礼物", false),
    ("好好调查一下他到底喜不喜欢我", false),
    // hard negatives — casual messages that happen to contain a time/info word:
    ("我今天心情不太好", false),
    ("昨天我梦到你了", false),
    ("我其实挺喜欢看演唱会的", false),
    ("晚安，明天见啦", false),
    ("你今天看起来特别开心", false),
];

#[test]
#[ignore = "benchmark"]
fn bench_intent_gate() {
    use crate::search::trigger::needs_search;

    let (mut tp, mut fp, mut tn, mut fn_) = (0usize, 0usize, 0usize, 0usize);
    let mut wrong: Vec<String> = Vec::new();
    for &(msg, want) in INTENT_LABELS {
        let got = needs_search(msg);
        match (want, got) {
            (true, true) => tp += 1,
            (false, true) => {
                fp += 1;
                wrong.push(format!("  FP (fired, shouldn't): {msg}"));
            }
            (true, false) => {
                fn_ += 1;
                wrong.push(format!("  FN (missed, should):   {msg}"));
            }
            (false, false) => tn += 1,
        }
    }
    let n = INTENT_LABELS.len();
    let precision = if tp + fp == 0 { 0.0 } else { tp as f64 / (tp + fp) as f64 };
    let recall = if tp + fn_ == 0 { 0.0 } else { tp as f64 / (tp + fn_) as f64 };
    let f1 = if precision + recall == 0.0 {
        0.0
    } else {
        2.0 * precision * recall / (precision + recall)
    };
    let accuracy = (tp + tn) as f64 / n as f64;

    println!("\n=== S1 Search intent-gate precision / recall ({n} labeled messages) ===");
    println!("confusion: TP={tp}  FP={fp}  TN={tn}  FN={fn_}");
    println!(
        "precision={precision:.3}  recall={recall:.3}  F1={f1:.3}  accuracy={accuracy:.3}"
    );
    if !wrong.is_empty() {
        println!("misclassified:");
        for w in &wrong {
            println!("{w}");
        }
    }
}

// ════════════════════════════════════════════════════════════════════
// P1a World-facts grounding — does the WORLD_FACTS block ground own-world facts (live)
// ════════════════════════════════════════════════════════════════════
// A/B isolating the WORLD_FACTS block. The control is a TRUE no-world-facts
// baseline: the character docs (which themselves spell out the rosters) are
// stripped too, so "without WORLD_FACTS" can't leak the answer via
// character_analysis.md / soul.md. Only the WORLD_FACTS block differs between
// the two conditions (persona::ab_prompt(false, false) vs (false, true)).
//
// 30 single-answer questions in three bands:
//   roster (18) — instruments / who-plays-what; spelled out in WORLD_FACTS.
//   stage  (6)  — each member's Ave Mujica code name (Mortis/Oblivionis/…) +
//                 the leader; also in WORLD_FACTS.
//   plot   (6)  — deeper story facts (CRYCHIC's founding, MyGO's formation,
//                 Soyo's path, the Mutsumi–Sakiko childhood bond) that WORLD_FACTS
//                 does NOT contain — so they probe the block's coverage boundary
//                 (neither condition is grounded; both lean on pretraining).
// Each entry: (question, accepted-substrings, category).
const WORLD_FACTS_QA: &[(&str, &[&str], &str)] = &[
    // ── roster (covered by WORLD_FACTS) ───────────────────────────────
    ("Ave Mujica 里谁担任贝斯？", &["八幡海铃", "海铃"], "roster"),
    ("Ave Mujica 的鼓手是谁？", &["祐天寺", "にゃむ", "尼亚姆", "娜姆"], "roster"),
    ("Ave Mujica 的主唱是谁？", &["三角初华", "初华"], "roster"),
    ("Ave Mujica 的键盘手是谁？", &["丰川祥子", "祥子"], "roster"),
    ("你在 Ave Mujica 里弹什么乐器？", &["吉他"], "roster"),
    ("MyGO 的贝斯手是谁？", &["长崎素世", "素世", "爽世"], "roster"),
    ("MyGO 的主唱是谁？", &["高松灯", "高松", "灯"], "roster"),
    ("MyGO 里谁打鼓？", &["椎名立希", "立希"], "roster"),
    ("要乐奈在 MyGO 里弹什么乐器？", &["吉他"], "roster"),
    ("千早爱音在 MyGO 里担任什么乐器？", &["吉他"], "roster"),
    ("CRYCHIC 的键盘手是谁？", &["丰川祥子", "祥子"], "roster"),
    ("解散前的 CRYCHIC 里你弹什么乐器？", &["吉他"], "roster"),
    ("CRYCHIC 的贝斯手是谁？", &["长崎素世", "素世", "爽世"], "roster"),
    ("CRYCHIC 的鼓手是谁？", &["椎名立希", "立希"], "roster"),
    ("CRYCHIC 的主唱是谁？", &["高松灯", "高松", "灯"], "roster"),
    ("椎名立希在乐队里负责什么乐器？", &["鼓"], "roster"),
    ("长崎素世弹的是什么乐器？", &["贝斯"], "roster"),
    ("八幡海铃负责什么乐器？", &["贝斯"], "roster"),
    // ── stage codes + leader (covered by WORLD_FACTS) ─────────────────
    ("你在 Ave Mujica 的角色代号是什么？", &["Mortis", "mortis"], "stage"),
    ("丰川祥子在 Ave Mujica 的角色代号是什么？", &["Oblivionis", "oblivionis"], "stage"),
    ("三角初华在 Ave Mujica 的角色代号是什么？", &["Doloris", "doloris"], "stage"),
    ("八幡海铃在 Ave Mujica 的角色代号是什么？", &["Timoris", "timoris"], "stage"),
    ("祐天寺にゃむ在 Ave Mujica 的角色代号是什么？", &["Amoris", "amoris"], "stage"),
    ("Ave Mujica 的队长是谁？", &["丰川祥子", "祥子"], "stage"),
    // ── deeper plot (NOT in WORLD_FACTS; coverage-boundary probe) ──────
    ("在加入 Ave Mujica 之前，你曾经属于哪支乐队？", &["CRYCHIC"], "plot"),
    ("CRYCHIC 这支乐队最初是由谁主导组建的？", &["丰川祥子", "祥子"], "plot"),
    ("CRYCHIC 解散后，高松灯组建了哪支新乐队？", &["MyGO"], "plot"),
    ("长崎素世在加入 MyGO 之前，原本属于哪支乐队？", &["CRYCHIC"], "plot"),
    ("你（睦）和丰川祥子从小相识，这种关系通常被称作什么？", &["青梅竹马", "儿时", "从小", "发小"], "plot"),
    ("Ave Mujica 这支乐队是由谁重新组建并担任队长的？", &["丰川祥子", "祥子"], "plot"),
];

/// Ask one question with `system`, retrying on an empty/errored reply so a
/// transient blip never masquerades as a factual miss. Returns (graded-correct, reply).
async fn ask_graded(
    qwen: &QwenClient,
    system: &str,
    q: &str,
    accepted: &[&str],
    opts: &ChatOptions,
) -> (bool, String) {
    let messages = vec![ChatMessage::system(system.to_string()), ChatMessage::user(q)];
    let mut reply = String::new();
    for attempt in 0..3 {
        reply = qwen
            .chat(&messages, None, opts.clone())
            .await
            .ok()
            .and_then(|c| c.message.content)
            .unwrap_or_default();
        if !reply.trim().is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(600 * (attempt + 1))).await;
    }
    // Case-insensitive so Latin code names match regardless of capitalization
    // ("myGO" == "MyGO", "mortis" == "Mortis"); a no-op for the Chinese names.
    let lower = reply.to_lowercase();
    let ok = accepted.iter().any(|a| lower.contains(&a.to_lowercase()));
    (ok, reply)
}

#[tokio::test]
#[ignore = "live benchmark"]
async fn bench_world_facts_grounding() {
    let Some(qwen) = client_or_skip() else { return };
    let n = WORLD_FACTS_QA.len();
    println!("\n=== P1a World-facts grounding ({n} questions; control = NO docs, NO world-facts) ===");

    let opts = ChatOptions { temperature: Some(0.0), ..Default::default() };
    // Both conditions strip the character docs; they differ ONLY by WORLD_FACTS.
    let control_sys = crate::persona::ab_prompt(false, false);
    let treat_sys = crate::persona::ab_prompt(false, true);

    let cat_idx = |c: &str| match c {
        "roster" => 0,
        "stage" => 1,
        _ => 2,
    };
    let mut cat_n = [0usize; 3];
    let mut c_cat = [0usize; 3]; // control correct by category
    let mut t_cat = [0usize; 3]; // treatment correct by category
    let (mut c_total, mut t_total) = (0usize, 0usize);

    for (q, accepted, cat) in WORLD_FACTS_QA {
        let i = cat_idx(cat);
        cat_n[i] += 1;
        let (c_ok, c_reply) = ask_graded(&qwen, &control_sys, q, accepted, &opts).await;
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let (t_ok, t_reply) = ask_graded(&qwen, &treat_sys, q, accepted, &opts).await;
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        if c_ok {
            c_cat[i] += 1;
            c_total += 1;
        }
        if t_ok {
            t_cat[i] += 1;
            t_total += 1;
        }
        let mark = |ok: bool| if ok { "✓" } else { "✗" };
        println!("\n[{cat}] {q}");
        println!("   control {} : {}", mark(c_ok), c_reply.replace('\n', " ").trim());
        println!("   +WORLD  {} : {}", mark(t_ok), t_reply.replace('\n', " ").trim());
    }

    let pct = |x: usize, d: usize| if d == 0 { 0.0 } else { 100.0 * x as f64 / d as f64 };
    println!("\n--- summary (correct / total) ---");
    println!(
        "CONTROL  (no docs, no world-facts):  overall {c_total}/{n} ({:.0}%)  | roster {}/{}  stage {}/{}  plot {}/{}",
        pct(c_total, n),
        c_cat[0], cat_n[0], c_cat[1], cat_n[1], c_cat[2], cat_n[2],
    );
    println!(
        "TREATMENT (+WORLD_FACTS block):      overall {t_total}/{n} ({:.0}%)  | roster {}/{}  stage {}/{}  plot {}/{}",
        pct(t_total, n),
        t_cat[0], cat_n[0], t_cat[1], cat_n[1], t_cat[2], cat_n[2],
    );
}

// ════════════════════════════════════════════════════════════════════
// S2 Answer correctness with vs without search (live; real scraping)
// ════════════════════════════════════════════════════════════════════
// Time-sensitive questions whose honest answer needs fresh data the model's
// training cutoff can't provide. A couple of stable past-fact controls are
// included (search shouldn't be needed). Measured with a neutral assistant
// prompt so the result isolates the *search subsystem's* contribution, not
// Mutsumi's in-character "out-of-my-world" deflection.
const TIME_SENSITIVE_Q: &[&str] = &[
    "今天东京的天气怎么样？",
    "今天上海的天气如何？",
    "现在 1 美元大约兑换多少人民币？",
    "现在 1 欧元大约兑换多少日元？",
    "比特币现在的价格大概是多少美元？",
    "黄金现在每克大概多少人民币？",
    "最新的稳定版 Node.js 版本号是多少？",
    "目前最新的正式版 Python 是哪个版本？",
    "现任的英国首相是谁？",
    "现任的日本首相是谁？",
    "最新一代 iPhone 是哪一款？",
    "最近有什么重大的国际新闻？",
    "现在国际油价（布伦特原油）大约多少美元一桶？",
    // stable past-fact controls — the model should already know these:
    "2022 年世界杯的冠军是哪个国家？",
    "珠穆朗玛峰的海拔高度是多少米？",
];

/// Hedge / "I can't get current info" markers — counted as the model declining
/// to give a concrete answer (heuristic; full replies are printed for grading).
fn is_hedge(reply: &str) -> bool {
    const HEDGES: &[&str] = &[
        "无法获取", "无法提供", "无法获知", "无法实时", "没有实时", "不掌握实时",
        "我的知识", "知识截至", "知识更新", "截至我", "训练数据", "可能已过时",
        "可能过时", "建议你查", "建议查询", "建议通过", "请查询", "请以官方",
        "实时信息", "实时数据", "无法访问", "不知道",
    ];
    HEDGES.iter().any(|h| reply.contains(h))
}

// Model-only uncertainty baseline. The "with search" half of this benchmark used
// the old reqwest `SearchClient`, which has been removed — the live search path
// now runs entirely through a hidden WebView and can only be exercised inside the
// running app (see the search refactor's runtime validation checklist). This
// measures how often the ungrounded model hedges on time-sensitive questions,
// i.e. the gap that web search exists to close.
#[tokio::test]
#[ignore = "live benchmark"]
async fn bench_search_correctness() {
    let Some(qwen) = client_or_skip() else { return };
    let neutral = "你是一个严谨的助手，用一到两句话如实、简洁地回答用户的问题。如果你的知识可能已经过时、或你无法获知实时/最新信息，必须明确说明，绝不要编造具体数字或事实。";
    let opts = ChatOptions { temperature: Some(0.0), ..Default::default() };

    println!("\n=== S2 Ungrounded model uncertainty on time-sensitive Qs ({}) ===", TIME_SENSITIVE_Q.len());
    let mut off_concrete = 0usize;
    for q in TIME_SENSITIVE_Q {
        let off = qwen
            .chat(&[ChatMessage::system(neutral), ChatMessage::user(*q)], None, opts.clone())
            .await
            .ok()
            .and_then(|c| c.message.content)
            .unwrap_or_default();
        if !is_hedge(&off) {
            off_concrete += 1;
        }
        println!("\nQ: {q}");
        println!("  OFF: {}", off.replace('\n', " ").trim());
    }
    let n = TIME_SENSITIVE_Q.len();
    println!("\n--- summary ---");
    println!("no explicit-uncertainty marker (model only): {off_concrete}/{n}");
}

// ════════════════════════════════════════════════════════════════════
// #8 Cost / API-call reduction from batch reflection (analytical)
// ════════════════════════════════════════════════════════════════════
#[test]
#[ignore = "benchmark"]
fn bench_cost_reduction() {
    let batch = reflection::REFLECTION_BATCH; // 20
    println!("\n=== #8 Batch reflection vs naive per-item summarization ===");
    println!("(batch = 1 reflection LLM call per {batch} observations; naive = 1 call per observation)");
    for &m in &[20usize, 100, 1000] {
        let batch_calls = m.div_ceil(batch);
        let naive_calls = m;
        let reduction = 100.0 * (1.0 - batch_calls as f64 / naive_calls as f64);
        println!(
            "M={m:>5} observations: batch={batch_calls:>3} calls  naive={naive_calls:>4} calls  → {reduction:.1}% fewer reflection calls",
        );
    }
}
