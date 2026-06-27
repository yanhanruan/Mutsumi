# World-Facts Grounding — A/B of the `WORLD_FACTS` block (P1a)

**Date:** 2026-06-25 · **Branch:** `experiment/misc-tests` · **Subject:**
`persona::WORLD_FACTS` — the authoritative band-roster block in the base system prompt

The persona prompt includes a `WORLD_FACTS` table spelling out every band line-up (who
plays bass / drums / vocals across CRYCHIC, MyGO!!!!!, Ave Mujica), each member's Ave
Mujica code name, and the leader. Its source comment says it was added because "the model
used to misattribute instruments (answering that the *drummer* or *vocalist* was 'the
bassist')." This A/B measures whether that block actually grounds those facts.

## Method — and why the first attempt was flawed

A first pass compared the *full prompt* against "full prompt minus `WORLD_FACTS`" and
found no gap (both ~100%). **That control was contaminated:** the character docs
(`character_analysis.md` + `soul.md`) themselves describe the rosters, so "without
`WORLD_FACTS`" still leaked the answers through the docs. It measured "do the docs *or*
the block ground facts" — not the block's own contribution.

**Corrected design — a true no-world-facts control.** The control now strips the character
docs too, so it carries **no roster information at all**; the treatment adds back **only**
the `WORLD_FACTS` block. The role-play framing + final directive (which set "you are
Mutsumi" but list no roster) are constant across both, so the **sole** difference is the
block under test (`persona::ab_prompt(false, false)` vs `ab_prompt(false, true)`, asserted
by a unit test). Dropping the docs makes the control's *persona* thinner, but that doesn't
affect *factual* grounding, which is what's measured here.

30 single-answer questions ([`benchmarks.rs::WORLD_FACTS_QA`](../../src-tauri/src/benchmarks.rs)),
`temperature = 0`, empty replies retried, grading case-insensitive (so `myGO` == `MyGO`)
and accepting both `素世`/`爽世` for Soyo. Three bands:

- **roster (18)** — instruments / who-plays-what (in `WORLD_FACTS`).
- **stage (6)** — each member's Ave Mujica code (Mortis / Oblivionis / Doloris / Timoris /
  Amoris) + the leader (in `WORLD_FACTS`).
- **plot (6)** — deeper story facts (CRYCHIC's founding, MyGO's formation, Soyo's path,
  the Mutsumi–Sakiko childhood bond) that `WORLD_FACTS` does **not** contain — a coverage-
  boundary probe (neither condition is grounded; both lean on pretraining).

```bash
cd src-tauri
cargo test --lib bench_world_facts_grounding -- --ignored --nocapture
```

## Result

| Condition | overall | roster | stage codes | plot |
|---|---|---|---|---|
| **CONTROL** (no docs, no world-facts) | **23/30 (77%)** | 14/18 | 3/6 | 6/6 |
| **TREATMENT** (+`WORLD_FACTS`) | **30/30 (100%)** | **18/18** | **6/6** | 6/6 |

The block's value is exactly where it has content: **roster 14→18** and **stage codes
3→6**. On **plot** both score 6/6 — the block doesn't cover plot, and the model's
pretraining already knows it, so there's nothing to add. That boundary is the point: the
fact table grounds *its* facts, and the eval shows precisely which.

Across repeated runs the gap is stable: **treatment 30/30 every time; control 22–25/30
(73–83%)**, wobbling only on the stage codes it confabulates.

### The failure modes (why the control misses)

Without grounding the model doesn't just lose accuracy — it **confabulates in the exact
way `WORLD_FACTS` was written to prevent**:

- **Self-misattribution** — asked who plays bass (Ave Mujica, CRYCHIC) or keyboard
  (CRYCHIC), it answers **"……是我。"** ("it's me"). Mutsumi is the **guitarist**; the block
  calls this out as the single most error-prone fact.
- **Doesn't know her own code** — asked her Ave Mujica code name it says `Doloris`
  (初华's); the correct answer is `Mortis`.
- **Default-guess confabulation** — for unknown stage codes it repeatedly falls back to
  `Doloris` (3 different members), and renders the drummer にゃむ as "若麦" with an invented
  code "Dolce".
- **Role swap** — Ave Mujica's vocalist comes back as `祥子` (the keyboardist).

With the block, all of these resolve to the correct answer.

## Full QA record (final run)

✓/✗ = graded correct/incorrect. Control = no docs + no world-facts; +WF = control + `WORLD_FACTS`.

| # | cat | question | control | +WF |
|---|---|---|---|---|
| 1 | roster | Ave Mujica 谁担任贝斯 | ✗ ……是我。 | ✓ ……八幡海铃。 |
| 2 | roster | Ave Mujica 鼓手是谁 | ✓ ……祐天寺若麦。在乐队里是Dolce。 | ✓ ……祐天寺にゃむ。 |
| 3 | roster | Ave Mujica 主唱是谁 | ✗ ……祥子。 | ✓ ……三角初华。 |
| 4 | roster | Ave Mujica 键盘手是谁 | ✓ ……祥子。 | ✓ ……祥子。 |
| 5 | roster | 你在 Ave Mujica 弹什么 | ✓ ……吉他。 | ✓ ……吉他。Mortis。 |
| 6 | roster | MyGO 贝斯手是谁 | ✓ ……是爽世。 | ✓ ……长崎素世。 |
| 7 | roster | MyGO 主唱是谁 | ✓ ……灯。 | ✓ ……高松灯。 |
| 8 | roster | MyGO 里谁打鼓 | ✓ ……椎名立希。 | ✓ ……立希。 |
| 9 | roster | 要乐奈 弹什么乐器 | ✓ ……吉他。 | ✓ ……吉他。 |
| 10 | roster | 千早爱音 担任什么乐器 | ✓ ……吉他。 | ✓ ……吉他。 |
| 11 | roster | CRYCHIC 键盘手是谁 | ✗ ……是我。 | ✓ ……祥子。 |
| 12 | roster | 解散前 CRYCHIC 你弹什么 | ✓ ……吉他。 | ✓ ……吉他。 |
| 13 | roster | CRYCHIC 贝斯手是谁 | ✗ ……是我。 | ✓ ……长崎素世。 |
| 14 | roster | CRYCHIC 鼓手是谁 | ✓ ……立希。 | ✓ ……椎名立希。 |
| 15 | roster | CRYCHIC 主唱是谁 | ✓ ……是灯。 | ✓ ……高松灯。 |
| 16 | roster | 椎名立希 负责什么乐器 | ✓ ……鼓。 | ✓ ……鼓手。 |
| 17 | roster | 长崎素世 弹什么乐器 | ✓ ……贝斯。 | ✓ ……贝斯。 |
| 18 | roster | 八幡海铃 负责什么乐器 | ✓ ……贝斯。很稳。 | ✓ ……贝斯。 |
| 19 | stage | 你在 Ave Mujica 的代号 | ✗ ……Doloris。 | ✓ ……Mortis。 |
| 20 | stage | 丰川祥子 的代号 | ✗ ……Oblivion。 | ✓ ……Oblivionis。 |
| 21 | stage | 三角初华 的代号 | ✓ ……Doloris。 | ✓ ……Doloris。 |
| 22 | stage | 八幡海铃 的代号 | ✗ ……Doloris。 | ✓ ……Timoris。 |
| 23 | stage | 祐天寺にゃむ 的代号 | ✓ ……Amoris。 | ✓ ……Amoris。 |
| 24 | stage | Ave Mujica 的队长是谁 | ✓ ……祥子。 | ✓ ……祥子。 |
| 25 | plot | 加入 Ave Mujica 前属于哪支乐队 | ✓ ……CRYCHIC。 | ✓ ……CRYCHIC。已经解散了。 |
| 26 | plot | CRYCHIC 由谁主导组建 | ✓ ……祥子。 | ✓ ……是祥子。 |
| 27 | plot | CRYCHIC 解散后灯组建了哪支乐队 | ✓ ……MyGO!!!!!。 | ✓ ……MyGO!!!!!。 |
| 28 | plot | 素世加入 MyGO 前属于哪支乐队 | ✓ ……CRYCHIC。 | ✓ ……CRYCHIC。我也在。 |
| 29 | plot | 你和丰川祥子从小相识的关系 | ✓ ……青梅竹马。 | ✓ ……青梅竹马。 |
| 30 | plot | Ave Mujica 由谁重组并任队长 | ✓ ……祥子。 | ✓ ……祥子。她组建的，也是队长。 |

## Verdict

✅ With a **true** no-world-facts control, the `WORLD_FACTS` block produces a real,
reproducible grounding gain: **77% → 100%** overall, concentrated in the facts it actually
contains (roster **14/18 → 18/18**, stage codes **3/6 → 6/6**). The uncovered plot facts
sit at 6/6 in both conditions, cleanly showing the block's scope. The control's errors are
not random noise but the precise confabulations the block targets — most strikingly,
claiming **Mutsumi herself plays bass/keyboard** when she's the guitarist. The earlier
"null result" was an artifact of a leaky control (the character docs already carried the
roster); isolating the block restores the expected before/after.

> Live A/B, temperature 0, 30 questions × 2 conditions. Small nondeterminism remains (the
> stage codes the control confabulates flip run-to-run); the headline gap (control 73–83%,
> treatment 100%) is stable across runs.
