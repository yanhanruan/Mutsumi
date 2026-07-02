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

## Results

Measured 2026-07-02, Google only, direct connection, 12 queries per mode, spaced 5 s.

### Baseline (`HARDEN=0`)

| engine | results | challenge | empty | failed |
|---|---:|---:|---:|---:|
| Google | 0 | 12 | 0 | 0 |

### Hardened (`HARDEN=1`)

| engine | results | challenge | empty | failed |
|---|---:|---:|---:|---:|
| Google | 0 | 12 | 0 | 0 |

### Δ (challenges eliminated)

| engine | baseline challenge | hardened challenge | Δ |
|---|---:|---:|---:|
| Google | 12 | 12 | **0** |

## Interpretation

**Hardening had no effect on Google: 12/12 → 12/12.** Every query was served a reCAPTCHA /
`/sorry/index` challenge in both modes. The three client-side levers
(`AutomationControlled` off, persistent profile, `__TAURI_INTERNALS__` delete) removed zero
challenges here.

Two things this *does* establish:

- **The hardening is safe to keep.** `failed = 0` in both runs is the key control: the page
  HTML was captured and correctly classified as a *challenge*, not lost to a timeout. That
  proves the `delete window.__TAURI_INTERNALS__` leg did **not** break the captured `invoke`
  / emit path — the failure mode we were guarding against (lever 3 silently killing IPC) did
  not occur. So the levers stay on by default; they cost nothing and may still help the four
  engines that already pass. For Google specifically they are a documented no-op.
- **The pre-run ranking was wrong for Google.** We ranked the fixable fingerprint/profile
  tells as the primary lever and IP reputation as secondary. The measurement refutes that:
  with every client-side tell removed, the challenge rate is unchanged. The operative cause
  is the residual we can't touch from the client — IP reputation and/or Google's treatment of
  an embedded WebView2 session at the network level. This is the "honest ceiling" flagged up
  front, now measured rather than assumed.

### Decisions this settles

1. **Bing CN stays the default engine** (unchanged). Google is not viable as a default over
   this WebView path — it challenges 100% of queries.
2. **Keep `MUTSUMI_SERP_HARDEN` on by default.** Free, non-destructive (`failed = 0`), and
   plausibly useful for the non-Google engines; no reason to disable it.
3. **Do _not_ build the 1b audio auto-solver (Vosk / Wit.ai).** The challenge fires on *every*
   Google query, and Google actively hardens the audio challenge against bots — an STT solver
   would be attempting the most-defended path, on every request, for the one engine we already
   keep non-default. That's a poor return for a ~50 MB model dependency plus hosting. The
   opt-in **manual-solve backstop** (`MUTSUMI_SERP_MANUAL=1`) remains the only Google escape
   hatch, for the rare user who insists on Google.

### Optional follow-up

Run `MUTSUMI_SERP_BENCH=all MUTSUMI_SERP_HARDEN=1` once to confirm hardening doesn't *regress*
the four working engines (each should show `results > 0`, `failed = 0`). Not required — those
engines passed runtime validation — but it would close the loop on "hardening breaks nothing."
