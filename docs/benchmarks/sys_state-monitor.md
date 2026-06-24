# System-state monitor — resource benchmark

The `sys_state` module ([`src-tauri/src/sys_state.rs`](../../src-tauri/src/sys_state.rs))
runs a background thread that emits a `system-state` event once per second for
the System State overlay. It is spawned at startup
([`lib.rs`](../../src-tauri/src/lib.rs)) with a stop flag that is never stored,
so it runs for the **whole app lifetime** — its per-tick cost is a 24/7
background load, whether or not the overlay is open.

This note records the measurements that motivated dropping
`System::refresh_all()` in favour of refreshing only CPU usage + memory.

Date: 2026-06-25 · Machine: Intel Core i7-8550U (4C/8T laptop), Windows 10,
~340 live processes.

## Method

- Per-tick cost = wall-clock time of one collection pass (the `refresh` calls +
  `detect_network` + battery probe), averaged over 30 iterations, measured with
  a temporary `#[ignore]` test in the module (`bench_tick_cost`, **not
  committed** — added, run, then reverted).
- **Current** = what `spawn()` did before: `sys.refresh_all()` +
  `networks.refresh(true)`.
- **Minimal** = what the loop actually needs: `sys.refresh_cpu_usage()` +
  `sys.refresh_memory()`.
- `% of one core @1 Hz` = `ms-per-tick / 1000` × 100 (the loop sleeps 1 s
  between ticks). `% of total CPU` divides by the 8 logical cores.

## Results

### Run 1 — debug build (`cargo test`)

| Path | ms / tick | % of one core @1 Hz |
| --- | --- | --- |
| Current (`refresh_all` + `networks.refresh`) | 54.447 | 5.45% |
| Minimal (`cpu%` + `memory`) | 16.823 | 1.68% |

Processes tracked: 340.

### Run 2 — optimized build (`cargo test --release`, opt-level 3, no LTO)

| Path | ms / tick | % of one core @1 Hz | % of total CPU |
| --- | --- | --- | --- |
| Current (`refresh_all` + `networks.refresh`) | 57.463 | 5.75% | ~0.72% |
| Minimal (`cpu%` + `memory`) | 12.746 | 1.27% | ~0.16% |

Processes tracked: 338.

Note: `refresh_all` costs the **same** optimized as in debug (57 vs 54 ms) — it
is dominated by the OS process-enumeration syscall, which the optimizer cannot
speed up. Only the minimal path got faster (it is Rust-bound work).

### Real-world steady state (running release app, overlay closed)

Sampled the live `Mutsumi` process over 8 s
(`TotalProcessorTime` delta ÷ window):

- CPU: **1.07% of total** (8.59% of one core, on 8 logical cores).
- Working set: **43.4 MB**, stable — no growth across the sample (no leak).

## Verdict

**Within a safe threshold** — memory is comfortably fine and the CPU load won't
cause problems — **but ~78% of the monitor's CPU was waste**: it scanned all
~340 processes every second to read only `global_cpu_usage()`,
`total_memory()`, `used_memory()` and `uptime()`, none of which need process
data. On a laptop, waking the CPU each second for that also works against deep
idle states / battery.

## Optimization applied

In `sys_state::spawn()`:

- `System::new_all()` → `System::new()` (no retained process list → lower
  baseline memory).
- `sys.refresh_all()` → `sys.refresh_cpu_usage()` + `sys.refresh_memory()`.
- `networks.refresh(true)` gated to `#[cfg(not(windows))]`; the Windows path
  classifies the link via `GetAdaptersAddresses` and never reads sysinfo's
  `Networks`, so `detect_network()` no longer takes that argument on Windows.

**Expected effect:** per-tick cost ~57 ms → ~13 ms (≈ 78% reduction), i.e. from
~0.72% to ~0.16% of total CPU, sustained 24/7. Reproduce by re-adding the
`bench_tick_cost` test described under *Method* and running
`cargo test --release bench_tick_cost -- --ignored --nocapture`.
