//! Env-gated, in-app **bot-detection benchmark** for the WebView search path.
//!
//! The live path needs a real WebView (an `AppHandle`), so it can't run in the
//! `cargo test` harness — this runs inside the app instead. Set
//! `MUTSUMI_SERP_BENCH=<engine|all>` before launch and it drives a fixed query
//! set (spaced like real usage) through [`super::webview::fetch_serp`], classifies
//! each outcome with [`super::webview::classify_outcome`], and writes a Markdown
//! results table to the app log dir.
//!
//!   MUTSUMI_SERP_BENCH=google npm run tauri dev
//!
//! See `docs/benchmarks/search-bot-detection.md` for method + results.

use std::time::Duration;

use tauri::{AppHandle, Manager};

use super::webview::{self, classify_outcome, Outcome};
use super::SearchEngine;

/// Fixed query set — all *should* return results, so a `challenge`/`empty` count
/// clearly signals bot-detection rather than a bad query. Mix of entity / weather
/// / price / time-sensitive so the engines' answer paths are exercised.
const QUERIES: &[&str] = &[
    "MyGO 主唱是谁",
    "今天东京天气",
    "比特币价格",
    "Ave Mujica 演唱会",
    "iPhone 16 参数",
    "上海天气",
    "英伟达股价",
    "2024 巴黎奥运会 金牌榜",
    "Rust 最新版本",
    "东京到大阪 新干线 时间",
    "祥子 生日 BanG Dream",
    "北京今天温度",
];

/// Real-usage spacing between searches (matches the app's ≤1 / 5 s pattern and
/// avoids the burst that would itself provoke challenges).
const SPACING: Duration = Duration::from_secs(5);

/// Spawn the benchmark in the background if `MUTSUMI_SERP_BENCH` is set. No-op
/// otherwise, so this is safe to call unconditionally at startup.
pub fn maybe_spawn(app: &AppHandle) {
    let Ok(spec) = std::env::var("MUTSUMI_SERP_BENCH") else { return };
    let engines = parse_engines(&spec);
    if engines.is_empty() {
        log::warn!("serp-bench: MUTSUMI_SERP_BENCH={spec:?} matched no engines");
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move { run(app, engines).await });
}

/// Parse the env spec into engines: `all`, or a comma-separated list of keys.
fn parse_engines(spec: &str) -> Vec<SearchEngine> {
    let spec = spec.trim();
    if spec.eq_ignore_ascii_case("all") {
        return vec![
            SearchEngine::BingCn,
            SearchEngine::Bing,
            SearchEngine::Google,
            SearchEngine::Baidu,
            SearchEngine::DuckDuckGo,
        ];
    }
    spec.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(SearchEngine::from_str)
        .collect()
}

/// Tally of outcomes for one engine.
#[derive(Default)]
struct Tally {
    results: usize,
    challenge: usize,
    empty: usize,
    failed: usize,
}

impl Tally {
    fn record(&mut self, o: Outcome) {
        match o {
            Outcome::Results(_) => self.results += 1,
            Outcome::Challenge => self.challenge += 1,
            Outcome::Empty => self.empty += 1,
            Outcome::Failed => self.failed += 1,
        }
    }
}

async fn run(app: AppHandle, engines: Vec<SearchEngine>) {
    let n = QUERIES.len();
    log::info!("serp-bench: START engines={engines:?} queries={n}");

    let mut rows: Vec<(SearchEngine, Tally)> = Vec::new();
    for engine in engines {
        let mut tally = Tally::default();
        for (i, q) in QUERIES.iter().enumerate() {
            let fetch = webview::fetch_serp(&app, engine, q).await;
            let outcome = classify_outcome(engine, &fetch);
            tally.record(outcome);
            log::info!("serp-bench {engine:?} [{}/{n}] {q} -> {}", i + 1, outcome.label());
            if i + 1 < n {
                tokio::time::sleep(SPACING).await;
            }
        }
        log::info!(
            "serp-bench {engine:?} DONE results={} challenge={} empty={} failed={}",
            tally.results, tally.challenge, tally.empty, tally.failed
        );
        rows.push((engine, tally));
    }

    let report = render_report(n, &rows);
    log::info!("serp-bench: RESULTS\n{report}");
    write_report(&app, &report);
}

fn render_report(n: usize, rows: &[(SearchEngine, Tally)]) -> String {
    let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    let mut s = format!(
        "## Run {ts} ({n} queries/engine)\n\n\
         | engine | results | challenge | empty | failed |\n\
         |---|---:|---:|---:|---:|\n"
    );
    for (engine, t) in rows {
        s.push_str(&format!(
            "| {engine:?} | {} | {} | {} | {} |\n",
            t.results, t.challenge, t.empty, t.failed
        ));
    }
    s.push('\n');
    s
}

/// Append the report to `<app-log-dir>/serp-bench.md` (best-effort).
fn write_report(app: &AppHandle, report: &str) {
    let Ok(dir) = app.path().app_log_dir() else {
        log::warn!("serp-bench: no app_log_dir; results only in the log above");
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("serp-bench.md");
    // Append so successive runs accumulate in one file.
    use std::io::Write;
    match std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        Ok(mut f) => {
            if f.write_all(report.as_bytes()).is_ok() {
                log::info!("serp-bench: appended results to {}", path.display());
            }
        }
        Err(e) => log::warn!("serp-bench: could not write {}: {e}", path.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_engines_handles_all_and_lists() {
        assert_eq!(parse_engines("all").len(), 5);
        assert_eq!(parse_engines("google"), vec![SearchEngine::Google]);
        assert_eq!(
            parse_engines("bing-cn, google , baidu"),
            vec![SearchEngine::BingCn, SearchEngine::Google, SearchEngine::Baidu]
        );
        assert!(parse_engines("").is_empty());
    }

    #[test]
    fn tally_records_each_outcome() {
        let mut t = Tally::default();
        t.record(Outcome::Results(3));
        t.record(Outcome::Challenge);
        t.record(Outcome::Empty);
        t.record(Outcome::Failed);
        t.record(Outcome::Results(1));
        assert_eq!((t.results, t.challenge, t.empty, t.failed), (2, 1, 1, 1));
    }

    #[test]
    fn render_report_has_a_row_per_engine() {
        let rows = vec![
            (SearchEngine::Google, Tally { results: 8, challenge: 4, empty: 0, failed: 0 }),
            (SearchEngine::BingCn, Tally { results: 12, challenge: 0, empty: 0, failed: 0 }),
        ];
        let r = render_report(12, &rows);
        assert!(r.contains("| Google | 8 | 4 | 0 | 0 |"));
        assert!(r.contains("| BingCn | 12 | 0 | 0 | 0 |"));
    }
}
