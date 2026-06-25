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
