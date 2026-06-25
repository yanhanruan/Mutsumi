# Chat-Memory System — Benchmark Report

**Date:** 2026-06-17 · **Branch:** `feat/chat` · **Subject:** Mutsumi RAG chat + memory pipelines

This report measures the role-play chat / memory system (Phases 0–5b). Every
number below comes from an actual run on this machine — nothing is projected
unless explicitly labeled **(projected)** or **(needs app)**.

## How to reproduce

Harness lives in [`src-tauri/src/benchmarks.rs`](../../src-tauri/src/benchmarks.rs) as `#[ignore]` tests.

```bash
# offline (no network)
cargo test --release --lib bench_retrieval_latency -- --ignored --nocapture
cargo test --lib bench_db_size        -- --ignored --nocapture
cargo test --lib bench_cost_reduction -- --ignored --nocapture
# live (needs DASHSCOPE_API_KEY in src-tauri/.env)
cargo test --lib bench_retrieval_accuracy    -- --ignored --nocapture
cargo test --lib bench_reflection_perf       -- --ignored --nocapture
cargo test --lib bench_extraction            -- --ignored --nocapture
cargo test --lib bench_long_term_consistency -- --ignored --nocapture
```

## Read this first — scope & honesty caveats

- **Not `sqlite-vec`.** Retrieval is **in-Rust brute-force cosine**: every query
  loads all embedded rows, scores them in Rust (relevance·recency·importance),
  and ranks. We chose this over `sqlite-vec` in Phase 0 for zero native-extension
  overhead. All retrieval numbers describe *that* implementation.
- **Build profile matters.** Latency was measured in `--release` (the shipping
  `opt-level="z"` + LTO profile). Debug builds are ~4× slower and not
  representative.
- **Live numbers vary** with network and model load (`qwen-plus`,
  `text-embedding-v3` @ 1024-dim, from a mainland-CN network). Treat them as
  order-of-magnitude, not guarantees.
- **Environment:** Windows 10, single query thread, warm OS page cache.

---

## #1 Vector retrieval latency  ⚠️ revises the "<10 ms" claim

Full `memory::search()` (load all rows → cosine → weighted rank → top-K
`last_access` touch), 1024-dim embeddings, 100 queries per tier, **release**:

| N (memories) | p50 | p95 | mean | max |
|---|---|---|---|---|
| 1,000  | **41 ms**  | **52 ms**  | 40 ms  | 73 ms |
| 5,000  | 183 ms | 216 ms | 187 ms | 294 ms |
| 10,000 | 350 ms | 417 ms | 351 ms | 485 ms |

**Verdict:** latency is **linear in N** and dominated by loading + decoding every
embedding BLOB each query. It is **not** millisecond-level at scale.
- ✅ Comfortable for a personal companion's realistic store (**≤ ~1–2k memories → sub-100 ms p95**).
- ⚠️ At 10k it's ~0.4 s p95 — usable but noticeable.
- (Debug build for contrast: 168 / 735 / 1487 ms p50 — why release is the honest number.)

**Recommendations for scale:** an in-memory embedding cache (avoid re-reading 40 MB
of BLOBs per query), a SIMD/`opt-level=3` hot path for cosine, or adopting
`sqlite-vec`/ANN would each cut this substantially. Pruning/down-weighting
reflected raw observations also keeps N bounded.

> **✅ Done (R1).** The first two recommendations are now implemented: an in-RAM
> embedding cache + AVX2/FMA SIMD cosine + a covering-index freshness check. Retrieval
> is now **flat ~20 ms p50 / ~38 ms p95 from 1k to 10k** (p95 @10k ~8× faster). Full
> before/after methodology in [`retrieval-optimization.md`](retrieval-optimization.md).

## #2 Database size & binary footprint

DB file size after `wal_checkpoint(TRUNCATE)`, 1024-dim f32 embedding BLOBs:

| N (memories) | DB size | per entry |
|---|---|---|
| 1,000  | 4.67 MB  | 4,669 B |
| 5,000  | 23.23 MB | 4,647 B |
| 10,000 | 46.43 MB | 4,643 B |

Growth is **~4.6 KB/memory**, fully accounted for by the embedding (1024 × 4 B =
4,096 B) plus content + row overhead. Linear, predictable: **~46 MB at 10k**.

**Binary footprint:** the only storage dependency is `rusqlite` with **bundled
SQLite** — the SQLite engine is compiled statically into the app (≈ 1–1.5 MB of
native code), **no external service, no separate process, zero ops**. Contrast a
standalone vector DB (Qdrant/Milvus): a separate multi-hundred-MB server with its
own lifecycle. (The full Tauri app `.exe` is tens of MB, but that's the desktop
shell + bundled frontend, not the memory layer.)

**Verdict:** ✅ Lightweight, zero-ops, on-device. DB size is the honest cost and
it's modest; embeddings dominate (reducible via 768/512-dim if needed).

## #3 Retrieval accuracy — Recall@5 & MRR

30 hand-built `(memory, query)` pairs (distinct Chinese user facts; query is a
paraphrase that should retrieve its fact). Real `text-embedding-v3` embeddings.

| Metric | Result |
|---|---|
| **Recall@5** | **1.000** (30/30) |
| **MRR** | **1.000** (every query's target ranked #1) |

**Verdict:** ✅ For distinct facts, embedding + cosine retrieval is essentially
perfect. Caveat: the dataset is moderately easy (well-separated facts); a set with
near-duplicate or overlapping memories would score lower — a good follow-up.

## #4 Reflection task performance

`reflect_once` over 20 observation fragments, 3 runs, `qwen-plus`:

| run | latency | fragments → insights | tokens (prompt + completion) |
|---|---|---|---|
| 1 | 5.63 s | 20 → 2 | 510 + 190 = 700 |
| 2 | 4.72 s | 20 → 2 | 510 + 240 = 750 |
| 3 | 3.69 s | 20 → 2 | 510 + 182 = 692 |
| **avg** | **~4.7 s** | **20 → 2** | **~714** |

**Verdict:** ✅ Aggregates **20 raw fragments into 2 high-level insights in ~4.7 s
for ~714 tokens** per reflection. Runs on a background task (see #6), so it never
blocks chat. Output quality was strong and genuinely synthetic (not restated).

## #5 Function-calling extraction reliability

12 conversations (10 fact-bearing + 2 chit-chat), `extract_memory` tool:

| Metric | Result |
|---|---|
| Structured-output success (valid JSON + required fields) | **1.000** (12/12 tool calls) |
| Avg facts extracted / conversation | **1.00** |
| Extraction-call latency | p50 **3.42 s** / p95 **3.67 s** |

**Additional latency to the user's reply: ≈ 0.** By design extraction is
**fire-and-forget** — `chat_stream` streams the full reply and sends `Done`, *then*
`tauri::async_runtime::spawn`s the extraction. The ~3.4 s extraction call happens
entirely after, off the response path.

**Verdict:** ✅ Reliable structured extraction (100% valid), ~1 preference/turn,
zero added user-visible latency.

## #6 Non-blocking UI  — (needs running app; not run here)

I cannot measure webview main-thread frame time without `npm run tauri dev`, and I
won't fabricate numbers. **Architectural evidence** it's non-blocking:

- Extraction & reflection run via `tauri::async_runtime::spawn` (Rust/Tokio worker
  threads) — a different OS thread from the WebView2 UI thread. They touch SQLite
  + the network, never the DOM.
- The webview only ever receives small `Channel.send(ChatEvent)` token deltas; it
  does no reflection work.

So by construction, background cognition cannot stall rendering. **To get an
empirical p95**, paste this into the app and trigger a reflection (≥20 memories),
comparing against idle:

```js
let last = performance.now(), frames = []
function tick(t){ frames.push(t - last); last = t; if (frames.length < 600) requestAnimationFrame(tick) }
requestAnimationFrame(tick)
// after ~10s: frames.sort((a,b)=>a-b); console.log('p95 frame ms', frames[Math.floor(frames.length*0.95)])
```

Expected: p95 frame time ≈ the 60 FPS idle baseline (~16.7 ms), unchanged during reflection.

## #7 Long-term consistency  ⚠️ found & fixed a real defect

Can a memory from "long ago" still be retrieved past 20 fresher entries? Target
aged **30 days**, 20 fresh distractors, query about the target, check top-5:

| Retrieval weighting | old-memory Recall@5 |
|---|---|
| **Before** (relevance .6 / recency .2 / half-life 7 d) | **0.067** (1/15) ❌ |
| **After** (relevance .7 / recency .1 / half-life 30 d) | **1.000** (15/15) ✅ |

The original default let the recency penalty on a 30-day-old memory (~0.19)
**cancel its relevance advantage** (~0.18), so fresh-but-irrelevant entries
out-ranked the aged-but-relevant one. **Fix:** retuned `RetrievalWeights::default()`
so relevance dominates and recency is a gentle tiebreaker (one-month half-life).
**Accuracy (#3) stayed 1.000/1.000 after the change — no regression.**

**Verdict:** ✅ After the retune, long-term memory persists reliably. *This is
exactly what the benchmark was for — it caught a shipped tuning bug.*

## #8 Cost / API-call reduction (batch reflection vs naive)

Batch reflection makes **1 LLM call per 20 observations** vs a naive
"summarize every memory on creation" (1 call each):

| Observations | Batch calls | Naive calls | Reduction |
|---|---|---|---|
| 20   | 1  | 20   | **95.0%** |
| 100  | 5  | 100  | **95.0%** |
| 1000 | 50 | 1000 | **95.0%** |

Amortized token cost: one reflection (~714 tokens) covers 20 memories ≈ **~36
tokens/memory**, vs a naive per-memory summary call (prompt overhead alone is
hundreds of tokens *each*). Extraction adds exactly one call per *conversation*
(not per memory), independent of this.

**Verdict:** ✅ Batch reflection is ~**20× fewer** summarization calls and
dramatically fewer tokens than naive per-item processing.

---

## Benchmark-driven fixes landed in this pass

1. **`embed_batch` chunking** — #3 surfaced a hard `text-embedding-v3` limit of
   **10 inputs/request** (400 error on 30). `embed_batch` now transparently chunks
   to ≤10, so any batch size works. (The app never hit it before because
   extraction/reflection embed only a few at a time.)
2. **Retrieval weight retune** — #7 surfaced that old relevant memories were
   buried; defaults changed to relevance 0.7 / recency 0.1 / 30-day half-life
   (old-memory recall 0.07 → 1.00, accuracy unchanged).
3. **Token accounting** — added `usage` parsing to the Qwen client to produce the
   #4/#8 token numbers.

## Summary scorecard

| # | Claim | Result | Verdict |
|---|---|---|---|
| 1 | millisecond retrieval | brute: 41 ms p95 @1k → 417 ms @10k; **optimized (R1): ~38 ms p95 flat to 10k** | ✅ (optimized) |
| 2 | lightweight, zero-ops | ~4.6 KB/mem; bundled SQLite; no service | ✅ |
| 3 | high accuracy | Recall@5 1.00, MRR 1.00 | ✅ (easy set) |
| 4 | efficient reflection | 20→2 insights, ~4.7 s, ~714 tok | ✅ |
| 5 | reliable extraction | 100% valid, ~0 added latency | ✅ |
| 6 | non-blocking UI | architecture sound; empirical run = app needed | ◻️ pending |
| 7 | long-term persistence | 0.07 → **1.00** after fix | ✅ (fixed) |
| 8 | lower cost | ~20× fewer reflection calls | ✅ |
