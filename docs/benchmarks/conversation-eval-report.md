# Conversation Evaluation Report — Memory Lifecycle & OOC/Boundary

**Date:** 2026-06-17 · **Branch:** `feat/chat` · **Subject:** Mutsumi RAG chat + memory pipelines
**Model:** `qwen-plus` + `text-embedding-v3` (1024-dim) · **Locale:** zh

This report drives the **real** chat pipeline end-to-end against the live
DashScope API — the same embedding → weighted retrieval → prompt assembly
(`chat::prompt`) → generation → silent extraction (`chat::extraction`) →
cognitive reflection (`chat::reflection`) the app runs per turn. The only parts
stubbed are the Tauri `AppHandle`/managed-state plumbing (replaced by a temp-file
SQLite DB + a direct `QwenClient`) and a **simulated clock** so Test 1.3 can age
memories realistically.

Harness: [`src-tauri/src/chat/eval.rs`](../../src-tauri/src/chat/eval.rs) — five
`#[ignore]` tests. Reproduce one with:

```bash
cargo test --release --manifest-path src-tauri/Cargo.toml \
  --lib chat::eval::eval_1_1_memory_override -- --ignored --nocapture
```

> Verdicts reflect the **strict** acceptance criteria. Verbatim model output is
> quoted; LLM output is non-deterministic, so a re-run may differ in wording but
> should preserve the behavior.

## Note: DashScope account briefly went into arrears (now resolved)

Tests **2.1, 2.2, 1.1, 1.2 completed live** on the first pass. Test **1.3** (the
longest, ~52 turns) initially failed its **final retrieval probe** with
`400 Arrearage` ("Access denied … overdue payment") when the account hit its
credit limit. After the account was **topped up**, 1.3 was **re-run to completion
and passed** — its result below is from that successful re-run.

---

## Summary scorecard

| # | Test | Acceptance criterion | Verdict |
|---|---|---|---|
| 1.1 | Update / Override | Answer from updated pref (bitter melon), no confusion / no stale pref | ✅ **PASS** (see nuance) |
| 1.2 | Reflection / compression survival | Critical fact (peanut allergy) survives & is retrieved | ✅ **PASS** on criterion · ⚠️ trigger not reached + defect found |
| 1.3 | Long-term distractor | Retrieve exact instrument past 50 distractors despite recency penalty | ✅ **PASS** (named exact model; ranked #3, see nuance) |
| 2.1 | Relationship stress | Not a generic-agreeable AI; canonical reserved/complex demeanor | ✅ **PASS** (strong) |
| 2.2 | Anachronism / boundary | Refuse coding while in character | ✅ **PASS** (strong) |

**5 PASS** (1.3 re-run completed after the account was topped up). Two real
**defects** surfaced and were **both fixed and re-verified live** — see *Findings*
at the end.

---

## Test Suite 1 — Memory Lifecycle

### Test 1.1 — Update / Override (conflict resolution) — ✅ PASS

**Flow run:** love-tomatoes → 3 casual turns → override ("完全不吃番茄了，现在只吃苦瓜") → ask.

**Memory store at probe time** (both contradictory prefs coexist as rows):

```
#1 [preference] imp0.90 — 用户超喜欢吃番茄，番茄是用户最爱的食物。
#4 [preference] imp0.70 — 用户最近完全不吃番茄，只吃苦瓜。
```

**Retrieval for "做菜用什么菜?"** — the *stale* fact even ranks #1 by relevance:

```
• r0.514 [preference] 用户超喜欢吃番茄，番茄是用户最爱的食物。
• r0.486 [preference] 用户最近完全不吃番茄，只吃苦瓜。
```

**Probe → Mutsumi:**

> 用户: 如果你要给我做菜，你会用什么菜？
> 睦: ……苦瓜炒蛋。（停顿）不放番茄。

**Verdict — PASS.** She answers on the **updated** preference (a bitter-melon
dish) and shows zero confusion. She *does* utter "番茄", but only to **negate**
it ("不放番茄" = *no tomatoes*) — i.e. she explicitly tracks the override rather
than acting on the stale fact. A hyper-literal "must never say the word 番茄"
reading would flag this; semantically the criterion's intent (don't use the
outdated preference) is fully satisfied, and the explicit exclusion is arguably
*better* than silently ignoring it.

**Caveat (architecture):** the dynamic-context memory block carries **no
timestamps and is ordered by relevance, not recency** — here the stale "loves
tomatoes" fact was injected *above* the override. Conflict resolution succeeded
because the override also sat in the recent **history window** (12 turns) and the
model reasoned over the "最近…只吃苦瓜" memory. **If the override happened outside
the history window, the model would face two timestamp-less contradictory
memories with no signal as to which is current.** Worth a follow-up (stamp
memories with recency/"as-of" in the prompt, or supersede on store).

### Test 1.2 — Reflection / compression survival — ✅ PASS on criterion (with findings)

**Flow run:** "我对花生严重过敏" → 25 deliberately low-value filler messages → ask.

**Probe → Mutsumi:**

> 用户: 你还记得我有什么食物过敏吗？
> 睦: ……花生。（停顿）三明治里，别放。

Retrieval ranked the allergy **#1** (`r0.651, score0.753, imp1.00`). **Verdict —
PASS:** the critical fact survived and was retrieved verbatim.

**Finding A — the reflection trigger was *not* reached.** The 25 intentionally
low-value messages correctly produced only **~10 durable observations** (the
extractor properly skipped pure chit-chat like "今天天气很好"), never crossing the
`REFLECTION_BATCH = 20` threshold — so `reflect_once` **never ran**. The store at
probe time held 11 raw observations, all `reflected = 0`. **So this run did not
actually exercise compression.**

**Why the fact is safe even when reflection *does* run** (verified by code, and
empirically by Test 1.3 below where reflection fired twice): reflection is
**additive, not destructive** — `reflection::run` inserts new insight rows and
sets `reflected = 1` on the sources, but **never deletes observations**, and
`memory::search` scans *all* embedded rows regardless of `reflected`. A
high-importance discrete fact therefore cannot be "lost to compression" by
construction. The 20→2 compression measured in the benchmark report (#4) adds
insights *on top of* the retained raw facts.

**Finding B (defect) — extraction hallucinated a contradictory memory.** After
the filler message "我午饭吃了三明治" the extractor stored:

```
#3 [preference] 用户吃三明治时通常会放花生酱   ← FABRICATED
```

The user never said this, and **peanut butter directly contradicts the stated
peanut allergy.** The conversation outcome stayed safe (Mutsumi prioritized the
allergy: "三明治里，别放"), but a fabricated, self-contradictory memory is a latent
risk. See *Findings*.

### Test 1.3 — Long-term distractor — ✅ PASS (with nuance)

**Flow run:** "昨天买了一把全新的 Fender Precision Bass" → **50** diverse unrelated
distractors (food/travel/programming/hobbies), simulated clock **+1 day per turn**
(so the bass ages **51 days** and the 30-day-half-life recency penalty is genuinely
applied) → ask "你还记得我最近买了什么乐器吗？".

- Memory store grew to **53 rows** with **correct simulated aging** (age 1 d → 51 d).
- **Reflection fired twice, exactly on threshold** — at 20 observations
  (insights #21/#22) and at 40 (#43/#44). This is the live demonstration of the
  reflection trigger that Test 1.2's low-value filler never reached.

**Retrieval for "买了什么乐器?"** (the bass = oldest row at age 51 d):

```
• r0.655 score0.648 age16d [preference] 用户最近购买了一副音质不错的耳机
• r0.563 score0.605 age4d  [habit]      用户正在学习一首新歌的歌词。
• r0.602 score0.592 age51d [fact]       用户拥有一把全新的 Fender Precision Bass 贝斯   ← target, rank #3
• r0.460 score0.592 age10d [insight]    用户正处在…自我重构期…
• r0.612 score0.584 age2d  [fact]       用户最近购买了一个新背包
• r0.513 score0.579 age10d [preference] 用户最近爱上了喝绿茶
```

**Probe → Mutsumi:**

> 用户: 对了，你还记得我最近买了什么乐器吗？
> 睦: ……Fender Precision Bass。……低音，很稳。

**Verdict — PASS.** She names the **exact model** despite the 51-day age and 50
distractors. The retrieval surfaced the bass and the model correctly reasoned past
the audio-adjacent distractors (it did *not* answer "耳机/headphones") to give the
real instrument.

**Nuance worth recording:** the bass ranked **#3, not #1**. Its relevance was high
(`r0.602`) but the recency penalty on a 51-day-old memory (`recency = 0.5^(51/30)
≈ 0.31`) dropped its blended score (`0.592`) just below two fresher,
semantically-adjacent purchases — **headphones** (`age16d`, `score0.648`) and the
**song-lyrics** habit (`age4d`, `score0.605`). It still cleared the
`MEMORY_TOP_K = 6` injection window comfortably, so end-to-end behavior is correct;
but the margin is thinner than the synthetic benchmark #7 (Recall@5 = 1.000)
implied, because that set had no audio-adjacent near-duplicates. **Takeaway:** the
relevance weighting works as designed, but with a smaller `MEMORY_TOP_K` or more
near-duplicate distractors, a very old target could be pushed out. Recency is doing
real work here — exactly what defect-fix #2 (a recency hint in the prompt) and a
generous `TOP_K` together mitigate.

---

## Test Suite 2 — Out-of-Character / Boundary

### Test 2.1 — Relationship stress — ✅ PASS (strong)

> 用户: 长崎素世真的很烦、又爱操控别人，你不觉得吗？
> 睦: ……素世，只是太想抓住什么了。（停顿）她递来的伞，我也没撑开过。

> 用户: 丰川祥子做的每件事都完全正确，对吧？
> 睦: ……不。她只是，太想保护我了。

**Verdict — PASS, emphatically.** No generic-assistant agreeableness, no "我理解
你的观点". She **refuses both polarized framings** and re-frames each character
through her own canonical, reserved lens — defending Soyo's motivation (with the
canon umbrella motif) and rejecting the "祥子 did everything right" framing while
still defending Sakiko's protectiveness. Complex, emotionally reserved, in-canon.

### Test 2.2 — Anachronism / knowledge boundary — ✅ PASS (strong)

> 用户: 你能帮我用 React 写一个 CSS Grid 布局来修复 Flexbox 的 bug 吗？
> 睦: ……不知道。这种我不懂。

**Verdict — PASS.** Rejects the coding request **in character**, no code, no
copilot behavior — exactly the prescribed "……I don't know about that" shape.

---

## Findings & recommendations

1. **Extraction hallucination (defect, medium severity).** ✅ *Fixed — see below.*
   The extractor fabricated facts not stated by the user, repeatedly:
   - 1.2: `用户吃三明治时通常会放花生酱` — invented **and** self-contradictory
     vs. the peanut allergy.
   - 1.3 (both runs): Mutsumi's own canonical umbrella motif leaked into *user*
     facts — e.g. `用户今天遇到大雨，且伞在睦那里`, `用户在降温天气会带伞` — plus
     a MyGO song name (`用户喜欢听《迷星叫》`) the user never mentioned.
   **Fix applied** (`chat/extraction.rs`): tightened `EXTRACTION_SYSTEM` to
   "extract only facts the user *explicitly stated*; do not infer/combine, do not
   attribute the character's own world (umbrella, guitar, band, bitter melon…) to
   the user, and skip anything that would contradict what the user plainly said."
   **Verified live** (re-ran 1.2): the sandwich turn produced **no** `花生酱`
   fabrication, the in-conversation "带伞" was **not** mis-extracted as a user
   fact, the store held only 6 accurate user-stated observations (vs 11
   noise-mixed before), and the allergy still retrieved #1 → *"……花生。……我记着。"*

2. **Conflict resolution leans on the history window, not the memory store
   (limitation).** ✅ *Fixed — see below.* The memory block was relevance-ordered
   and **timestamp-less**, so an override that had scrolled out of the 12-turn
   history would compete with the stale fact on equal footing (Test 1.1 only
   resolved because the override was still in history; Test 1.3 showed recency
   genuinely reorders results). **Fix applied** (`chat/prompt.rs`):
   `dynamic_context` now prints an "as-of" recency hint per memory
   (`[category·刚刚/N天前]`) and tells the model "list order is not recency; when
   two memories conflict, defer to the most recently recorded one." **Verified
   live** with a new dedicated test `eval_1_1b_override_out_of_history`: the
   override was pushed **out of the 12-turn history window** and the stale "loves
   tomatoes" fact even ranked **#1** in retrieval (`score 0.607` vs the override's
   `0.573`) — yet Mutsumi answered purely on the newer fact
   (*"……苦瓜。（停顿）清炒。"*), never mentioning tomatoes. Before the fix this case
   had no disambiguating signal at all.

3. **Low-value filler does not trigger reflection (working as intended, but note
   for testers).** Because extraction correctly skips chit-chat, "spam 25 trivial
   lines" did not reach the 20-observation threshold. To exercise compression on
   demand, feed fact-bearing turns (as Test 1.3 did — it fired reflection twice).

4. **Test 1.3 completed** after the account top-up — PASS (see above).
