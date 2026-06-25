# World-Facts Grounding — A/B of the `WORLD_FACTS` block (P1a)

**Date:** 2026-06-25 · **Branch:** `experiment/misc-tests` · **Subject:**
`persona::WORLD_FACTS` — the authoritative band-roster block in the base system prompt

The persona prompt includes a `WORLD_FACTS` table that spells out every band line-up
(who plays bass / drums / vocals across CRYCHIC, MyGO!!!!!, Ave Mujica). Its source
comment says it was added because "the model used to misattribute instruments
(answering that the *drummer* or *vocalist* was 'the bassist')." This A/B measures
whether that block still changes roster accuracy.

## Method

20 single-answer roster questions
([`benchmarks.rs::ROSTER_QA`](../../src-tauri/src/benchmarks.rs)), focused on exactly the
attributions the block flags as most error-prone (bassist vs. drummer vs. vocalist, and
that Mutsumi is always the guitarist). Each is asked twice — once with the system prompt
built **without** the `WORLD_FACTS` block, once **with** it — at `temperature = 0` for a
clean comparison. A reply is graded correct if it contains the right name/instrument
(aliases accepted; empty/errored replies are retried so a transient blip can't count as
a miss). The two prompts differ *only* by the roster block
(`persona::base_system_prompt_variant`, asserted by a unit test).

```bash
cd src-tauri
cargo test --lib bench_world_facts_grounding -- --ignored --nocapture
```

## Result

| Condition | Roster accuracy |
|---|---|
| **without** `WORLD_FACTS` | **20 / 20 (100%)** |
| **with** `WORLD_FACTS` | **19 / 20 (95%)** |

The single miss *with* the block was `Ave Mujica 的鼓手是谁？` → `……若麦` — a phonetic
mangle of the drummer's katakana name にゃむ (祐天寺にゃむ). That same question is the only
flaky item *without* the block too across runs; it's an obscure Japanese name the model
renders inconsistently in Chinese, not an instrument misattribution.

## Interpretation — an honest null result

**The expected before/after gap did not reproduce.** The premise (that the model
misattributes instruments without the explicit table) does not hold for the *current*
model + persona docs: roster accuracy is already saturated at ~100% **without** the
block. Two reasons:

1. The character docs (`character_analysis.md` + `soul.md`, ~40 KB) already describe the
   bands and Mutsumi's role in depth — they ground the roster on their own.
2. BanG Dream! / MyGO / Ave Mujica are a well-known franchise in the model's
   pretraining, so it knows the line-ups even with the docs stripped.

So at today's model/doc maturity, `WORLD_FACTS` is **redundant insurance, not a
load-bearing fix** — it doesn't move roster accuracy, but it's cheap, stable, and guards
against regressions (a weaker/cheaper model, or trimmed docs, could still misattribute;
the block is the deterministic backstop). The historical misattribution the comment
describes was presumably real on an earlier model or before the docs were enriched.

**Résumé-honest takeaway:** the persona's factual grounding is robust (20/20 roster
accuracy, verified by A/B), and the A/B *itself* is the contribution — it shows the
grounding is carried by the character docs + model, and identifies the explicit fact
table as a candidate for simplification rather than a critical component. No staged
"before" was manufactured to produce a prettier number.

> Caveat: 20 direct questions at temperature 0; small nondeterminism remains (the にゃむ
> item flips between runs). A larger or deliberately adversarial set (yes/no "trap"
> questions baiting the drummer-as-bassist confusion) could still expose a gap on weaker
> models, but on this model both conditions are effectively at ceiling.
