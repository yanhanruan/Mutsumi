# Search Bot-Detection — post-mortem

**Subject:** the WebView search path (`search::webview::fetch_serp`) and whether it gets
stopped by anti-bot challenges. Companion to [S2](search-correctness.md) (which measured whether
search *improves answers*).

## TL;DR

We thought Google served a reCAPTCHA on every query and built fingerprint/profile **hardening**
to beat it. It was a **false alarm caused by our own classifier**, not a real block. The
hardening was removed as dead weight. The one real bug — misclassifying working result pages as
challenges — is fixed.

## What we believed (wrong)

After the WebView-only refactor, runtime checks + an in-app benchmark reported Google returning a
challenge on **12/12** queries, both with and without hardening. We concluded Google was 100%
blocked, that no client-side lever moved it, and that IP reputation dominated. On that basis we
added: `--disable-blink-features=AutomationControlled`, a persistent profile, and a document-start
delete of `window.__TAURI_INTERNALS__` (all behind `MUTSUMI_SERP_HARDEN`).

## The actual bug

`classify_outcome` (and the retry loop, and the manual-solve trigger) checked
`is_challenge_page(html)` **before** checking whether the page produced results. A normal Google
SERP embeds reCAPTCHA assets (`/recaptcha/api.js`), so its HTML contains a challenge-y token — and
the classifier scored those **working results pages as challenges**. A runtime screenshot settled
it: a full Tokyo-weather SERP (weather card + forecast + news) rendered fine while the code popped
a manual-solve window over it. Google had been returning results the whole time; the "12/12
challenge" counts were the classifier over-reporting. Every conclusion drawn from that benchmark
is retracted.

## The fix

Results win. One predicate gates classification, retry, and the solve popup:

```
is_blocking_challenge(has_results, html) = !has_results && is_challenge_page(html)
```

A page we can parse is served as results even if it carries a stray token; only a **resultless**
page with a challenge marker is a real block.

## Hardening: removed

With the classifier fixed, the hardening had no job — the engines return results to a plain
incognito WebView. Removed: the `MUTSUMI_SERP_HARDEN` toggle, the `AutomationControlled`/browser
args, the persistent `serp-profile` data directory, and the `__TAURI_INTERNALS__`-hiding
init-script. The fetch window is now a plain incognito tab with the browser's genuine fingerprint.
(The one thing that survived independently: deleting the IPC global never broke `invoke`/emit —
but that only ever mattered *because* of the hardening, which is gone.)

**Kept:** the manual-solve backstop (`MUTSUMI_SERP_MANUAL`, default on). Post-fix it fires only on
a genuine block (zero results **and** a challenge marker), so it won't misfire on working Google —
if a real challenge ever appears, the fetch window surfaces with a localized banner for the user to
clear it, and the cleared cookie persists for the window's session.

## Running the benchmark now

Still available (`search::bench`, env-gated), now without the HARDEN split:

```bash
MUTSUMI_SERP_BENCH=google npm run tauri dev   # one engine
MUTSUMI_SERP_BENCH=all    npm run tauri dev   # all five
```

12 fixed queries per engine, spaced 5 s, classified into results / challenge / empty / failed and
appended to `<app-log-dir>/serp-bench.md`. With the fixed classifier, `challenge` should be ~0 for
queries that actually work. Any non-`results` outcome also gets an **"Anomalies (per query)"**
detail line — the fetch error for `failed`, and the matched challenge markers + page size for
`challenge` (`/sorry/index` or "unusual traffic" = a real Google block; a lone `g-recaptcha` on a
large page = a stray asset on a SERP that just failed to parse).

## Lesson

Validate the measurement against a real page before building on it. A whole hardening effort was
spent on a number the instrument invented; one screenshot of a real SERP falsified it.
