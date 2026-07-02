# Search Bot-Detection — Baseline vs Hardened

**Date:** 2026-07-01 · **Branch:** `feat/search-webview-only` · **Subject:** the WebView
search path (`search::webview::fetch_serp`) and its resistance to anti-bot challenges.

Companion to [S2](search-correctness.md): S2 measured whether search *improves answers*; this
measures whether the WebView actually *gets* a SERP, or gets stopped by a reCAPTCHA / Cloudflare
challenge — and how much client-side hardening reduces that.

## Why

After the WebView-only refactor, runtime validation passed for every engine **except Google**,
which serves a reCAPTCHA challenge. This benchmark exists to (a) name the causes, (b) apply
fingerprint/profile hardening, and (c) **quantify** the reduction in challenges rather than
guessing.

## Root-cause analysis — why a challenge fires

The connection to Google here is **direct (no proxy)**, so the fixable levers dominate. Ranked:

1. **Automation fingerprint tells.** `navigator.webdriver` / `AutomationControlled` — wry does
   **not** disable this by default (verified in `wry-0.55.1`). And `window.__TAURI_INTERNALS__`
   (plus other `__TAURI*` globals) is exposed on the page — a dead giveaway of an embedded webview
   to any detection script.
2. **Cold / empty profile.** A fresh WebView with no cookies, cache, or history looks like a bot.
3. **IP reputation.** Still a factor, but with a direct connection it's secondary; the benchmark
   shows how much is left after 1–2 are removed.
4. **Behavioral.** Already fine — ≤ 1 search / 5 s, a real *rendered* window, genuine TLS/JA3.

## Hardening applied (all gated by `MUTSUMI_SERP_HARDEN`)

| # | Lever | Addresses | Where |
|---|---|---|---|
| 1 | `--disable-blink-features=AutomationControlled` (+ wry defaults) | `navigator.webdriver` | `build_window` `additional_browser_args` |
| 2 | Persistent `data_directory` profile (cookies **+ cache + history**); baseline uses `incognito` | cold profile | `build_window` |
| 3 | Capture bound `invoke`, then `delete window.__TAURI_INTERNALS__` (+ `__TAURI*`) at document-start | embedded-webview tell | `init_script(hide_globals=true)` |
| — | Per-session homepage **warm-up** before the first search | cold profile | `warm_up` |

**Deliberately rejected:** UA spoofing. Overriding the UA without matching Client Hints is itself
a mismatch tell; the native WebView2 UA is internally consistent, so we leave it.

The request-id also moved from the URL **fragment** to a **query param** so it survives a
`/sorry/index?continue=…` redirect — without that, redirect challenges time out *invisibly* and
can't even be counted. That fix is the prerequisite for this benchmark.

## Method

12 fixed queries (`bench.rs::QUERIES`) that should all return results, so a `challenge`/`empty`
count is a clean bot-detection signal. Each fetch is classified by
`webview::classify_outcome` into **results / challenge / empty / failed**. Queries are spaced
**5 s** (real-usage pattern; a burst would itself provoke challenges). The runner lives in the app
(`search::bench`) because the live path needs a real WebView — it can't run under `cargo test`.

Two launches per engine, each a fresh process so the toggle is read cleanly at window build:

```bash
# baseline (no hardening; incognito profile)
MUTSUMI_SERP_BENCH=google MUTSUMI_SERP_HARDEN=0 npm run tauri dev
# hardened (default)
MUTSUMI_SERP_BENCH=google MUTSUMI_SERP_HARDEN=1 npm run tauri dev
```

`MUTSUMI_SERP_BENCH` accepts a single engine key, a comma list, or `all`. Results are logged and
appended to `<app-log-dir>/serp-bench.md`. `MUTSUMI_SERP_VISIBLE=1` shows the window so you can
watch a challenge render.

## ⚠️ Correction (2026-07-02): the numbers below are INVALID

**The benchmark's classifier was broken, so every conclusion drawn from it is retracted.**
`classify_outcome` checked `is_challenge_page(html)` *before* checking whether the page
actually yielded results. A real Google results page embeds reCAPTCHA assets (e.g.
`/recaptcha/api.js`), so its HTML contains a challenge-y token — and the classifier counted
those **working results pages as challenges**. A runtime screenshot of a normal Tokyo-weather
SERP (full weather card + forecast + news) confirmed it: Google returned results, the code
called it a challenge and popped a manual-solve window over a perfectly good page.

So "12/12 challenge, both modes" almost certainly means **Google was returning results the
whole time** and the counter mis-scored them. The hardening story ("no client-side lever
moves Google", "IP reputation dominates", "keep hardening because failed = 0") was built on
that mis-scoring and should not be trusted.

**Fix (committed):** results now win. `classify_outcome`, the retry loop, and the manual-solve
trigger all use `is_blocking_challenge(has_results, html) = !has_results && is_challenge_page`,
so a page we can parse is served as results even if it carries a stray token; only a
**resultless** page with a challenge marker counts as a block.

**What still stands / what to redo:**
- Bing CN stays the default (it works; unrelated to this bug).
- The `__TAURI_INTERNALS__`-delete safety check is independent and still holds (`failed = 0`
  means `invoke`/emit was not broken by the delete).
- Everything else — the challenge rate, whether hardening matters, whether Google needs
  manual-solve at all — **must be re-measured** with the fixed classifier before it's believed.

## Results (INVALID — see correction above; kept for the record)

Measured 2026-07-02, Google only, direct connection, 12 queries per mode, spaced 5 s. These
counts came from the broken classifier and over-report `challenge`.

### Baseline (`HARDEN=0`)

| engine | results | challenge | empty | failed |
|---|---:|---:|---:|---:|
| Google | 0 | 12 | 0 | 0 |

### Hardened (`HARDEN=1`)

| engine | results | challenge | empty | failed |
|---|---:|---:|---:|---:|
| Google | 0 | 12 | 0 | 0 |

## Re-run needed

With the fixed classifier, re-run both modes and refill this doc:

```bash
MUTSUMI_SERP_BENCH=google MUTSUMI_SERP_HARDEN=0 npm run tauri dev
MUTSUMI_SERP_BENCH=google MUTSUMI_SERP_HARDEN=1 npm run tauri dev
```

Expect `challenge` to drop sharply (likely to ~0 for the queries that actually work). Only
then decide whether hardening or manual-solve earn their keep for Google.
