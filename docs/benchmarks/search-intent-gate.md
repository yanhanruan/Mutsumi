# Search Intent-Gate — Precision / Recall (S1)

**Date:** 2026-06-25 · **Branch:** `experiment/misc-tests` · **Subject:**
`search::trigger::needs_search` — the keyword gate that decides whether a chat turn
triggers a live web search

The gate is the cheap, LLM-free pre-filter in front of web search: a hit means the
turn pays for a network round-trip, a miss means Mutsumi answers from model + memory
only. This measures how well that keyword heuristic separates "needs fresh info" from
ordinary conversation, on a hand-labeled set.

## Method

43 hand-labeled messages
([`benchmarks.rs::INTENT_LABELS`](../../src-tauri/src/benchmarks.rs)), where the label is
**"does answering well require a live web lookup?"** — current events, prices, weather,
schedules, or an explicit search request → `true`; chit-chat, emotion, persona, or a
fixed fact → `false`. The set deliberately includes the adversarial cases: casual
messages that happen to contain a time/info word (`现在 / 今天 / 演唱会 …`), the
`检查 / 调查 / 审查 / 搜罗` negatives that embed `查 / 搜`, and keyword-less factual
questions the gate has no cue for.

```bash
cd src-tauri
cargo test --lib bench_intent_gate -- --ignored --nocapture
```

## Result

```
confusion: TP=20  FP=8  TN=12  FN=3   (n=43)
precision = 0.714   recall = 0.870   F1 = 0.784   accuracy = 0.744
```

| Metric | Value |
|---|---|
| **Precision** | **0.714** |
| **Recall** | **0.870** |
| **F1** | **0.784** |
| Accuracy | 0.744 |

The gate is **recall-biased** — and that is by design. Its own source comment: *"broad
coverage is preferred; the occasional false trigger is harmless (the retrieved context
is labeled 'ignore if irrelevant')."* A missed search hurts (no fresh data); a spurious
search just adds ignorable context. So high recall at the cost of precision is the
intended operating point, and 0.87 recall / 0.71 precision matches it.

## Where it fails (all 11 misclassifications)

**False negatives (3)** — keyword-less factual questions. The gate keys on surface
words, so a current-affairs question phrased without any cue slips through:

- `谁是现任的日本首相` (who is the current PM)
- `最近油价又涨了吗` — note `最近` ("recently") is **not** in the time-cue list (`近期`
  is, `最近` isn't), so nothing fires
- `英伟达的市值超过苹果了吗`

**False positives (8)** — two distinct causes:

1. *Intentional recall-bias* (5): casual messages containing a time/info word —
   `我今天心情不太好`, `昨天我梦到你了`, `我其实挺喜欢看演唱会的`, `晚安，明天见啦`,
   `你今天看起来特别开心`. These are the "harmless" false triggers the design accepts.

2. *A real substring bug* (3): `查一下` matches **inside** `检查一下 / 审查一下 /
   调查一下`. The bare-verb rule already guards the lone `查` against `检查 / 调查`, but the
   proactive phrase `查一下` is matched as a raw substring, so `检查一下` →
   `…查一下` still fires:
   - `帮我检查一下这段代码有没有 bug`
   - `这件事得好好审查一下才行`
   - `好好调查一下他到底喜不喜欢我`

   This one is *not* the intended tradeoff — it's an unintended match. Fixing it (anchor
   the `查` in `查一下 / 查查` so it doesn't count when preceded by `检 / 调 / 审 / 抽 /
   排 / 普 …`) would lift precision toward ~0.81 with no recall cost. Left as a
   follow-up so this commit stays a pure measurement.

## Verdict

✅ The gate behaves as designed: **recall 0.87** (it rarely misses a genuine search
need, and when it does it's a keyword-less phrasing), with precision intentionally
traded away for coverage. The eval also surfaced one concrete, fixable defect (the
`查一下`-inside-`检查一下` substring match) distinct from the deliberate recall-bias —
exactly the kind of thing a labeled eval is for.
