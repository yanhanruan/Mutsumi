# Answer Correctness — Without vs With Search (S2)

**Date:** 2026-06-25 · **Branch:** `experiment/misc-tests` · **Subject:** the
search-enhancement pipeline (`search::search` → `format_context` → injected prompt)

Companion to [S1](search-intent-gate.md): S1 showed the gate fires on the right
messages; S2 asks whether firing it actually **improves the answer** on time-sensitive
questions.

## Method

15 questions ([`benchmarks.rs::TIME_SENSITIVE_Q`](../../src-tauri/src/benchmarks.rs)) —
13 time-sensitive (weather / fx / prices / "latest X" / current officials / news) plus
2 stable past-fact controls. Each is answered twice: **OFF** (model only) and **ON**
(model + the real scraped search context, exactly as the app injects it). A **neutral
assistant** system prompt is used (not Mutsumi's persona) so the result isolates the
*search subsystem's* contribution rather than her in-character "that's outside my world"
deflection. DuckDuckGo engine, `temperature = 0`, 3 s between queries to stay under
anti-bot thresholds. Correctness is graded by hand from the transcripts (see caveat on
the automated counter below).

```bash
cd src-tauri
cargo test --lib bench_search_correctness -- --ignored --nocapture
```

## Headline

**Search returned context for 15/15 questions** (the gate + scraper both worked). Its
*value*, graded by hand, is concentrated and honest:

| Category | Questions | Search improved the answer? |
|---|---|---|
| **Quote / price** (USD-CNY, BTC, gold, oil) | 4 | **✅ 4/4** — OFF refused or gave a stale figure; ON returned the **retrieved current number** |
| Weather (Tokyo, Shanghai) | 2 | ❌ 0/2 — scraping returns the *site*, not the temperature |
| "Latest version / news" (Node, Python, news) | 3 | ❌ 0/3 — snippets are headlines/links with no clean extractable datum |
| Already-known (UK PM, JP PM, iPhone, EUR-JPY) | 4 | ❌ ~0/4 — model answered from training; context didn't change it (and once *confused* it) |
| Stable controls (2022 WC, Everest height) | 2 | ✅ both correct OFF **and** ON — search doesn't break known-fact answering |

So on genuinely current questions, useful grounding went from **~0/13 → 4/13**, and
**every win was a quote-style question** where the SERP snippet directly contained the
number.

### The wins (concrete)

| Question | OFF (model only) | ON (with search) |
|---|---|---|
| 1 美元 ≈ ? 人民币 | "无法获取… 大约 **7.1–7.3**"（stale guess） | "约 **6.80** 元"（from retrieved data） |
| 比特币价格？ | "无法提供…准确价格"（refused） | "约 **6.1 万美元**" |
| 黄金每克？ | "无法提供…准确价格"（refused） | "约 **871.90 元/克**" |
| 布伦特原油？ | "无法获取…市场数据"（refused） | "**65–74 美元/桶**" |

## Why the misses (the useful findings)

1. **Body extraction is the bottleneck, not retrieval.** The gate fired and the scraper
   returned results every time, but for weather the top hits are JS-rendered portals
   (`weather.com.cn`, AccuWeather) whose live numbers aren't in the static HTML — one
   scraped "body" was even an Akamai error page. The model then *correctly* declined,
   because the injected context had links but no temperature. Same for "latest Node/Python
   version" and "recent news": the snippet is a heading, not the fact. **Targeted answer
   extraction (weather/FX/quote boxes) would convert most of these misses into wins** —
   the value ceiling here is parsing, not search coverage.

2. **Stray future-dated context can confuse the model.** For "现任日本首相", ON answered
   worse than OFF: it flagged that the retrieved data "包含 2025 年的未来时间，无法作为参考"
   and hedged between 岸田文雄 / 石破茂. The neutral harness deliberately omits a current-date
   anchor; the **production prompt does inject `now` (the real date)** in
   `chat::build_messages`, which would mitigate this — so this specific confusion is partly
   an artifact of the isolation, but it shows context cleanliness (dropping/relabeling
   dated lines) matters.

3. **Caveat on the automated counter.** The harness also prints a "no explicit-uncertainty
   marker" count (OFF 2/15, ON 4/15). **Do not read that as correctness** — the model
   appends "请以实时数据为准" disclaimers even to its correct grounded answers (e.g. the BTC
   and gold wins above), so the marker fires on good answers too. It's reported only as a
   crude lower bound; the hand-graded table above is the real result.

## Verdict

✅ Search delivers real, grounded improvements on **quote-style time-sensitive questions**
(currency, crypto, commodities): OFF refuses or guesses stale, ON answers with the
retrieved current figure (4/4). ⚠️ It does **not** yet help weather / "latest version" /
news, and the eval pinpoints why — **body extraction**, not the gate or retrieval, is the
limiter (context was returned 15/15). The two stable controls confirm search doesn't
degrade answers the model already knows. Net: a precise, honest picture of where the
feature pays off and the single highest-leverage place to improve it next.

> Live numbers vary with the web and the scraped layouts; figures above are from the
> 2026-06-25 run captured in the test output. The "current" values (6.80, 6.1万, 871.90,
> 65–74) are what the SERP snippets carried at run time, not independently re-verified.
