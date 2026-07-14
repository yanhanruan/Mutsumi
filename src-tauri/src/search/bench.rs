//! Env-gated, in-app benchmarks for the WebView search path.
//!
//! The live path needs a real WebView (an `AppHandle`), so neither can run in
//! the `cargo test` harness — they run inside the app instead:
//!
//! * `MUTSUMI_SERP_BENCH=<engine|all>` — **bot-detection** benchmark: drives a
//!   fixed query set through [`super::webview::fetch_serp`], classifies each
//!   outcome, appends a tally table to `<log-dir>/serp-bench.md`.
//! * `MUTSUMI_SERP_QUALITY=<engine|all>` — **result-quality** report: drives a
//!   themed query set through the FULL pipeline (fetch → parse → curate) and
//!   writes `<log-dir>/serp-quality-report.md` with, per engine × theme: every
//!   raw row (before filtering), each row's verdict + reason, and the exact
//!   context block that would be injected into chat (after filtering) — so
//!   filter behavior and result relevance are reviewable side by side.
//!
//!   MUTSUMI_SERP_QUALITY=all npm run tauri dev
//!
//! See `docs/benchmarks/search-bot-detection.md` for the bot-bench method.

use std::time::Duration;

use tauri::{AppHandle, Manager};

use super::webview::{self, classify_outcome, Outcome};
use super::{RowFate, RowVerdict, SearchEngine, SearchResult};

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

/// Themed query set for the quality report — one per chat topic Mutsumi's
/// search trigger actually fires on, so the report shows how each engine serves
/// each *kind* of question, not just one lucky vertical.
const THEMED_QUERIES: &[(&str, &str)] = &[
    ("天气 weather", "福安今天天气如何"),
    ("汇率 exchange-rate", "人民币对日元汇率"),
    ("股价 stock", "英伟达股价"),
    ("新闻 news", "今天有什么科技新闻"),
    ("乐队资讯 band-news", "Ave Mujica 最新消息"),
    ("事实问答 entity-fact", "MyGO 的主唱是谁"),
    ("出行 transport", "东京到大阪新干线要多久"),
    ("发售日期 release-date", "GTA6 发售日期"),
];

/// Spawn whichever benchmarks the environment asks for. No-op without the env
/// vars, so this is safe to call unconditionally at startup.
pub fn maybe_spawn(app: &AppHandle) {
    if let Some(engines) = engines_from_env("MUTSUMI_SERP_BENCH") {
        let app = app.clone();
        tauri::async_runtime::spawn(async move { run(app, engines).await });
    }
    if let Some(engines) = engines_from_env("MUTSUMI_SERP_QUALITY") {
        let app = app.clone();
        tauri::async_runtime::spawn(async move { run_quality(app, engines).await });
    }
}

/// Engines requested via `var`, or `None` when unset/matching nothing.
fn engines_from_env(var: &str) -> Option<Vec<SearchEngine>> {
    let spec = std::env::var(var).ok()?;
    let engines = parse_engines(&spec);
    if engines.is_empty() {
        log::warn!("serp-bench: {var}={spec:?} matched no engines");
        return None;
    }
    Some(engines)
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
    // Per-query notes for anything that wasn't a clean `results` — the detail that
    // turns "1 challenge / 2 failed" into an actual cause.
    let mut notes: Vec<String> = Vec::new();
    for engine in engines {
        let mut tally = Tally::default();
        for (i, q) in QUERIES.iter().enumerate() {
            let fetch = webview::fetch_serp(&app, engine, q).await;
            let outcome = classify_outcome(engine, &fetch);
            tally.record(outcome);
            let detail = query_detail(&fetch, outcome);
            log::info!(
                "serp-bench {engine:?} [{}/{n}] {q} -> {}{}",
                i + 1,
                outcome.label(),
                detail.as_deref().map(|d| format!(" ({d})")).unwrap_or_default()
            );
            if let Some(d) = detail {
                notes.push(format!("- `{engine:?}` — {q} → **{}** — {d}", outcome.label()));
            }
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

    let mut report = render_report(n, &rows);
    if !notes.is_empty() {
        report.push_str("### Anomalies (per query)\n\n");
        for note in &notes {
            report.push_str(note);
            report.push('\n');
        }
        report.push('\n');
    }
    log::info!("serp-bench: RESULTS\n{report}");
    write_report(&app, &report);
}

// ── result-quality report (MUTSUMI_SERP_QUALITY) ─────────────────────────────

/// One engine × theme cell of the quality run.
struct QualityCell {
    /// Summary-matrix symbol (see the report legend).
    summary: String,
    /// Full per-query section (before-table + after-block), already rendered.
    section: String,
}

async fn run_quality(app: AppHandle, engines: Vec<SearchEngine>) {
    let n = THEMED_QUERIES.len();
    log::info!("serp-quality: START engines={engines:?} themes={n}");

    let mut per_engine: Vec<(SearchEngine, Vec<QualityCell>)> = Vec::new();
    for engine in engines {
        let mut cells = Vec::new();
        for (i, &(theme, query)) in THEMED_QUERIES.iter().enumerate() {
            let fetch = webview::fetch_serp(&app, engine, query).await;
            let mut cell = quality_cell(engine, theme, query, &fetch);
            // Exercise the "go deeper" step exactly as search() would, and
            // record its outcome — so the report reviews the full pipeline,
            // not just surface curation. (Re-runs the pure curate; cheap.)
            if let Ok(html) = &fetch {
                let raw = super::parse_rendered(engine, html);
                if !raw.is_empty() && !webview::is_blocking_challenge(engine, true, html) {
                    let (mut sent, verdicts) = super::curate_report(query, raw);
                    if let Some(note) = super::deepen(&app, query, &mut sent, &verdicts).await {
                        cell.section.push_str(&format!("**Deepen:** {note}\n\n"));
                    }
                }
            }
            log::info!("serp-quality {engine:?} [{}/{n}] {query} -> {}", i + 1, cell.summary);
            cells.push(cell);
            if i + 1 < n {
                tokio::time::sleep(SPACING).await;
            }
        }
        per_engine.push((engine, cells));
    }

    let report = render_quality_report(&per_engine);
    write_quality_report(&app, &report);
}

/// Evaluate one fetched SERP through the real pipeline and render its report
/// cell. Pure given the fetch result — unit-tested.
fn quality_cell(
    engine: SearchEngine,
    theme: &'static str,
    query: &'static str,
    fetch: &Result<String, String>,
) -> QualityCell {
    let heading = format!("### {engine:?} · {theme} · «{query}»\n\n");
    match fetch {
        Err(e) => QualityCell {
            summary: "✗ failed".into(),
            section: format!("{heading}fetch failed: `{e}`\n\n"),
        },
        Ok(html) => {
            let raw = super::parse_rendered(engine, html);
            if webview::is_blocking_challenge(engine, !raw.is_empty(), html) {
                return QualityCell {
                    summary: "🛡 challenge".into(),
                    section: format!(
                        "{heading}blocking challenge (markers {:?}) — no context sent\n\n",
                        webview::challenge_markers(html)
                    ),
                };
            }
            let (sent, verdicts) = super::curate_report(query, raw);
            QualityCell {
                summary: summary_symbol(&sent, &verdicts),
                section: format!(
                    "{heading}{}",
                    render_quality_section(query, &verdicts, &sent)
                ),
            }
        }
    }
}

/// Compact summary-matrix symbol: how well this engine served this theme.
fn summary_symbol(sent: &[SearchResult], verdicts: &[RowVerdict]) -> String {
    let raw = verdicts.len();
    if raw == 0 {
        return "∅ no rows".into();
    }
    if sent.is_empty() {
        return format!("∅ 0/{raw}");
    }
    let datum_first = verdicts.iter().any(|v| v.fate == RowFate::SentDatum(1));
    let any_datum = verdicts
        .iter()
        .any(|v| matches!(v.fate, RowFate::SentDatum(_)));
    let sym = if datum_first {
        "✓"
    } else if any_datum {
        "●"
    } else {
        "○"
    };
    format!("{sym} {}/{raw}", sent.len())
}

/// Render one query's detail: the before-table (every raw row + its verdict)
/// and the after-block (the exact context injected into chat).
fn render_quality_section(query: &str, verdicts: &[RowVerdict], sent: &[SearchResult]) -> String {
    let mut s = String::from(
        "**Before — every parsed row and its verdict:**\n\n\
         | # | verdict | rel | datum | title | source | snippet (as parsed) |\n\
         |---|---|:-:|:-:|---|---|---|\n",
    );
    for (i, v) in verdicts.iter().enumerate() {
        s.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} |\n",
            i + 1,
            v.fate.label(),
            tick(v.relevant),
            tick(v.has_datum),
            cell(&v.title, 34),
            super::host_of(&v.url),
            cell(&v.snippet, 60),
        ));
    }
    s.push_str("\n**After — context sent to the LLM:**\n\n");
    if sent.is_empty() {
        s.push_str("*(nothing sent — the model answers without web context)*\n\n");
    } else {
        s.push_str("```\n");
        s.push_str(&super::format_context(query, sent));
        s.push_str("```\n\n");
    }
    s
}

fn tick(b: bool) -> &'static str {
    if b { "✓" } else { "—" }
}

/// Markdown-table-safe, length-capped cell text.
fn cell(s: &str, max: usize) -> String {
    let clean = s.replace('|', "∣");
    if clean.chars().count() <= max {
        return clean;
    }
    clean.chars().take(max).collect::<String>() + "…"
}

/// Assemble the whole report: legend, the engine × theme summary matrix, then
/// the per-query sections grouped by engine.
fn render_quality_report(per_engine: &[(SearchEngine, Vec<QualityCell>)]) -> String {
    let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    let mut s = format!(
        "# SERP quality report — {ts}\n\n\
         Full pipeline (fetch → parse → curate → deepen) per engine × theme. \
         “Before” lists **every** row parsed from the SERP with its verdict; \
         “After” is the **exact** context block injected into chat (max {max} results). \
         A **Deepen** line means the surface snippet lacked the wanted datum, so \
         the top result's page was fetched and its relevant content extracted \
         (that extract replaces the snippet in the real chat context).\n\n\
         Legend: **✓** datum in slot 1 · **●** datum sent, not first · \
         **○** sent, no hard datum · **∅** nothing sent · **🛡** challenge · **✗** fetch failed. \
         `rel` = shares a term with the query (trad/simp folded); `datum` = states \
         the KIND of fact the query asks for (天气→气温/天况, 股价/汇率→价格数字, \
         发售→日期; title or snippet).\n\n\
         > If a challenge window pops (百度安全验证 / reCAPTCHA), solve it — the \
         fetch waits for you, and that same query then records the post-solve \
         result.\n\n",
        max = super::MAX_RESULTS,
    );

    // Summary matrix: themes × engines.
    s.push_str("## Summary\n\n| theme |");
    for (engine, _) in per_engine {
        s.push_str(&format!(" {engine:?} |"));
    }
    s.push_str("\n|---|");
    s.push_str(&"---|".repeat(per_engine.len()));
    s.push('\n');
    for (ti, &(theme, query)) in THEMED_QUERIES.iter().enumerate() {
        s.push_str(&format!("| {theme} «{query}» |"));
        for (_, cells) in per_engine {
            s.push_str(&format!(" {} |", cells.get(ti).map_or("", |c| &c.summary)));
        }
        s.push('\n');
    }
    s.push('\n');

    for (engine, cells) in per_engine {
        s.push_str(&format!("## {engine:?}\n\n"));
        for c in cells {
            s.push_str(&c.section);
        }
    }
    s
}

/// Overwrite `<app-log-dir>/serp-quality-report.md` — each run is one coherent,
/// reviewable document (unlike the bot bench, which accumulates tallies).
fn write_quality_report(app: &AppHandle, report: &str) {
    let Ok(dir) = app.path().app_log_dir() else {
        log::warn!("serp-quality: no app_log_dir; report only in the log above");
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("serp-quality-report.md");
    match std::fs::write(&path, report) {
        Ok(()) => log::info!("serp-quality: report written to {}", path.display()),
        Err(e) => log::warn!("serp-quality: could not write {}: {e}", path.display()),
    }
}

/// Diagnostic detail for a non-`results` outcome: the fetch error for `failed`,
/// the matched challenge markers + page size for `challenge` (a real Google block
/// shows `/sorry/index`; a lone `g-recaptcha` on a large page is a stray asset on
/// a SERP that just failed to parse), the page size for `empty`. `None` for a
/// clean `results` outcome. Pure.
fn query_detail(fetch: &Result<String, String>, outcome: Outcome) -> Option<String> {
    match (fetch, outcome) {
        (Err(e), _) => Some(format!("error: {e}")),
        (Ok(html), Outcome::Challenge) => Some(format!(
            "markers={:?} bytes={}",
            webview::challenge_markers(html),
            html.len()
        )),
        (Ok(html), Outcome::Empty) => Some(format!("no results parsed; bytes={}", html.len())),
        _ => None,
    }
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

    // ── quality report ───────────────────────────────────────────────────────

    /// A Bing-layout SERP mixing junk, a datum row, and an off-topic row.
    const QUALITY_HTML: &str = r#"<div id="b_results"><ol>
        <li class="b_algo"><h2><a href="https://tianqi.example.com/a">东京天气预报 - 天气网</a></h2>
          <div class="b_caption"><p>天气网为您提供东京天气预报查询，便捷出行，助您放心安排。</p></div></li>
        <li class="b_algo"><h2><a href="https://weather.example.jp/b">东京实时天气</a></h2>
          <div class="b_caption"><p>东京今天多云，气温 18~25℃，湿度 60%。</p></div></li>
        <li class="b_algo"><h2><a href="https://finance.example.com/c">英伟达股价创新高</a></h2>
          <div class="b_caption"><p>盘中大涨，机构纷纷看多。</p></div></li>
      </ol></div>"#;

    #[test]
    fn quality_cell_reports_before_verdicts_and_after_context() {
        let cell = quality_cell(
            SearchEngine::BingCn,
            "天气 weather",
            "东京天气",
            &Ok(QUALITY_HTML.to_string()),
        );
        // Datum hit first → ✓, 2 sent (datum + blanked link) of 3 raw rows.
        assert_eq!(cell.summary, "✓ 2/3", "summary was {}", cell.summary);
        let s = &cell.section;
        // Before-table: every row present with verdict + signals.
        assert!(s.contains("sent #1** · datum") && s.contains("东京实时天气"), "{s}");
        assert!(s.contains("· link (filler blanked)") && s.contains("为您提供"), "{s}");
        assert!(s.contains("dropped · off-topic") && s.contains("英伟达股价创新高"), "{s}");
        // After-block: the exact injected context, junk snippet absent.
        assert!(s.contains("【搜索：东京天气】"), "{s}");
        assert!(s.contains("气温 18~25℃"), "{s}");
        let after = s.split("After — context sent").nth(1).unwrap();
        assert!(!after.contains("为您提供"), "junk fed to the model: {after}");
    }

    #[test]
    fn quality_cell_flags_challenge_and_failure() {
        let ch = quality_cell(
            SearchEngine::Baidu,
            "t",
            "q",
            &Ok("<html>百度安全验证</html>".to_string()),
        );
        assert_eq!(ch.summary, "🛡 challenge");
        assert!(ch.section.contains("no context sent"));

        let f = quality_cell(SearchEngine::Bing, "t", "q", &Err("timed out".into()));
        assert_eq!(f.summary, "✗ failed");
        assert!(f.section.contains("timed out"));
    }

    #[test]
    fn quality_report_has_summary_matrix_and_sections() {
        let cells = vec![quality_cell(
            SearchEngine::BingCn,
            THEMED_QUERIES[0].0,
            THEMED_QUERIES[0].1,
            &Ok(QUALITY_HTML.to_string()),
        )];
        let report = render_quality_report(&[(SearchEngine::BingCn, cells)]);
        assert!(report.contains("# SERP quality report"));
        assert!(report.contains("| theme | BingCn |"), "{report}");
        assert!(report.contains("天气 weather"), "{report}");
        assert!(report.contains("### BingCn ·"), "{report}");
    }

    #[test]
    fn table_cell_escapes_pipes_and_caps_length() {
        assert_eq!(cell("a|b", 10), "a∣b");
        let long = "字".repeat(40);
        let c = cell(&long, 10);
        assert_eq!(c.chars().count(), 11, "10 + ellipsis");
        assert!(c.ends_with('…'));
    }

    /// Offline preview of the quality report, built from the real
    /// `serp-dump-*.html` files (written by `MUTSUMI_SERP_DUMP`) instead of live
    /// fetches. Writes `<log-dir>/serp-quality-report-offline.md`.
    ///   cargo test --lib real_dump_quality_report -- --ignored --nocapture
    #[test]
    #[ignore = "reads real serp-dump-*.html from the app log dir and writes a preview report"]
    fn real_dump_quality_report() {
        let dir = std::path::Path::new(&std::env::var("LOCALAPPDATA").unwrap_or_default())
            .join("com.mutsumi.app")
            .join("logs");
        let mut per_engine = Vec::new();
        for (file, engine, theme, query) in [
            ("serp-dump-Bing.html", SearchEngine::Bing, "天气 weather", "福安今天天气如何"),
            ("serp-dump-BingCn.html", SearchEngine::BingCn, "天气 weather", "福安今天天气如何"),
            ("serp-dump-Google.html", SearchEngine::Google, "天气 weather", "东京天气"),
            ("serp-dump-Baidu.html", SearchEngine::Baidu, "天气 weather", "查询一下宁德今天气温范围"),
            ("serp-dump-DuckDuckGo.html", SearchEngine::DuckDuckGo, "天气 weather", "东京天气"),
        ] {
            let Ok(html) = std::fs::read_to_string(dir.join(file)) else { continue };
            per_engine.push((engine, vec![quality_cell(engine, theme, query, &Ok(html))]));
        }
        assert!(!per_engine.is_empty(), "no serp-dump-*.html found in {}", dir.display());
        let report = render_quality_report(&per_engine);
        let out = dir.join("serp-quality-report-offline.md");
        std::fs::write(&out, &report).expect("write preview report");
        println!("wrote {}", out.display());
    }

    #[test]
    fn query_detail_explains_only_anomalies() {
        // Clean results → no note.
        assert!(query_detail(&Ok("<div id=\"b_results\"></div>".into()), Outcome::Results(3)).is_none());
        // Failed → surfaces the fetch error verbatim (timeout vs setup, etc.).
        let d = query_detail(&Err("timed out waiting for rendered HTML".into()), Outcome::Failed).unwrap();
        assert!(d.contains("error:") && d.contains("timed out"), "got {d}");
        // Challenge → matched markers + page size, so a real block is told from a stray token.
        let d = query_detail(&Ok(r#"<div class="g-recaptcha"></div>"#.into()), Outcome::Challenge).unwrap();
        assert!(d.contains("markers=") && d.contains("g-recaptcha") && d.contains("bytes="), "got {d}");
    }
}
