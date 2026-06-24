use serde::Serialize;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

#[derive(Clone, Serialize)]
#[serde(tag = "type")]
pub enum BatteryStatus {
    Charging { percent: u8, time_to_full: Option<u64> },
    Discharging { percent: u8, time_to_empty: Option<u64> },
    PluggedIn { percent: u8 },
}

/// How the machine is currently reaching the network. Serializes to a plain
/// lowercase string (`"ethernet"`, `"wifi"`, `"other"`, `"offline"`).
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NetworkKind {
    Ethernet,
    Wifi,
    /// Connected via something that is neither plain Ethernet nor Wi-Fi
    /// (cellular, a VPN tunnel as the only default route, etc.).
    Other,
    Offline,
}

#[derive(Clone, Serialize)]
pub struct SystemState {
    pub cpu_usage: f32,
    pub mem_usage: f32,
    pub network: NetworkKind,
    pub uptime: u64,
    pub battery: Option<BatteryStatus>,
}

// NOTE: CPU/component temperature is intentionally not exposed. On Windows the
// only OS-level source is the ACPI thermal zone via WMI
// (`MSAcpi_ThermalZoneTemperature`), which `sysinfo` uses under the hood. That
// query requires administrator privileges (access-denied otherwise) and, even
// when elevated, most machines expose no usable CPU sensor — so `Components`
// reliably returns nothing for a normal (non-elevated) desktop app. Reading
// real CPU temperature would require bundling a signed kernel-mode driver,
// which is out of scope for this app, so the field is omitted entirely.

/// Classify the active network connection on Windows.
///
/// Walks the adapter list from `GetAdaptersAddresses` and only considers an
/// adapter a real internet path when it is operationally *up* **and** owns a
/// default gateway — this filters out loopback, disconnected NICs, idle virtual
/// Wi-Fi / Bluetooth PAN adapters, etc. Ethernet wins over Wi-Fi when both are
/// active, matching Windows' usual default-route preference.
#[cfg(windows)]
fn detect_network() -> NetworkKind {
    use windows::Win32::NetworkManagement::IpHelper::{
        GetAdaptersAddresses, GAA_FLAG_INCLUDE_GATEWAYS, GAA_FLAG_SKIP_ANYCAST,
        GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST, IP_ADAPTER_ADDRESSES_LH,
    };
    use windows::Win32::Networking::WinSock::AF_UNSPEC;

    // IANA ifType values (also used by Windows' MIB-II interface table).
    const IF_TYPE_ETHERNET: u32 = 6;
    const IF_TYPE_IEEE80211: u32 = 71; // Wi-Fi
    const IF_TYPE_LOOPBACK: u32 = 24;
    const IF_OPER_STATUS_UP: i32 = 1;

    unsafe {
        let family = AF_UNSPEC.0 as u32;
        let flags = GAA_FLAG_INCLUDE_GATEWAYS
            | GAA_FLAG_SKIP_DNS_SERVER
            | GAA_FLAG_SKIP_MULTICAST
            | GAA_FLAG_SKIP_ANYCAST;

        // First call with a null buffer just reports the size we need.
        let mut size: u32 = 0;
        let _ = GetAdaptersAddresses(family, flags, None, None, &mut size);
        if size == 0 {
            return NetworkKind::Offline;
        }
        let mut buf = vec![0u8; size as usize];
        let head = buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH;
        if GetAdaptersAddresses(family, flags, None, Some(head), &mut size) != 0 {
            return NetworkKind::Offline; // anything but ERROR_SUCCESS (0)
        }

        let (mut eth, mut wifi, mut other) = (false, false, false);
        let mut cur = head;
        while !cur.is_null() {
            let a = &*cur;
            let up = a.OperStatus.0 == IF_OPER_STATUS_UP;
            let has_gateway = !a.FirstGatewayAddress.is_null();
            if up && has_gateway && a.IfType != IF_TYPE_LOOPBACK {
                match a.IfType {
                    IF_TYPE_ETHERNET => eth = true,
                    IF_TYPE_IEEE80211 => wifi = true,
                    _ => other = true,
                }
            }
            cur = a.Next;
        }

        if eth {
            NetworkKind::Ethernet
        } else if wifi {
            NetworkKind::Wifi
        } else if other {
            NetworkKind::Other
        } else {
            NetworkKind::Offline
        }
    }
}

/// Non-Windows fallback: no portable Wi-Fi/Ethernet distinction without extra
/// dependencies, so just report a generic connected/offline state from
/// interface traffic.
#[cfg(not(windows))]
fn detect_network(networks: &sysinfo::Networks) -> NetworkKind {
    for (name, data) in networks {
        let n = name.to_lowercase();
        if n.contains("loopback") || n == "lo" {
            continue;
        }
        if data.total_received() > 0 || data.total_transmitted() > 0 {
            return NetworkKind::Other;
        }
    }
    NetworkKind::Offline
}

/// Estimates battery time-to-full / time-to-empty from the *actual* rate at
/// which the charge level moves, averaged over a multi-minute window.
///
/// The `battery` crate and Windows' `GetSystemPowerStatus` both derive their
/// time estimates from the *instantaneous* power draw, which swings with load
/// and reads very differently from the Windows battery flyout — it badly
/// overestimates while the machine is momentarily idle. Measuring how fast the
/// percentage itself moves over the last few minutes instead gives a
/// load-averaged figure that tracks the system estimate much more closely
/// (the flyout effectively does the same). The reported value is refreshed at
/// most every ~20 s so the UI doesn't churn every tick, and `None` is returned
/// until enough movement has accrued to be meaningful.
struct BatteryEstimator {
    /// `(elapsed_secs, state_of_charge)` where charge is a 0..1 fraction.
    samples: VecDeque<(f64, f64)>,
    charging: bool,
    reported: Option<u64>,
    last_report: f64,
}

impl BatteryEstimator {
    /// Average the rate over up to this much recent history.
    const WINDOW_SECS: f64 = 300.0;
    /// Need at least this much history before projecting…
    const MIN_SPAN_SECS: f64 = 30.0;
    /// …and at least this much charge movement (0.5 %), so the slope is real.
    const MIN_DELTA: f64 = 0.005;
    /// Refresh the displayed value at most this often.
    const REPORT_EVERY_SECS: f64 = 20.0;
    /// Clamp absurd projections (e.g. a trickle near full).
    const MAX_SECS: u64 = 99 * 3600;

    fn new() -> Self {
        Self {
            samples: VecDeque::new(),
            charging: false,
            reported: None,
            last_report: f64::NEG_INFINITY,
        }
    }

    /// Feed the current charge fraction at time `now` (seconds) plus whether the
    /// battery is charging; returns the stable seconds-remaining to display
    /// (`None` while still warming up).
    fn update(&mut self, now: f64, soc: f64, charging: bool) -> Option<u64> {
        // A direction change invalidates the accumulated slope.
        if charging != self.charging {
            self.reset();
            self.charging = charging;
        }
        self.samples.push_back((now, soc));
        while let Some(&(t, _)) = self.samples.front() {
            if now - t > Self::WINDOW_SECS {
                self.samples.pop_front();
            } else {
                break;
            }
        }

        if let Some(secs) = self.estimate() {
            if self.reported.is_none() || now - self.last_report >= Self::REPORT_EVERY_SECS {
                self.reported = Some(secs);
                self.last_report = now;
            }
        }
        self.reported
    }

    /// Two-point slope across the whole window → projected seconds, or `None`
    /// when there isn't enough movement yet to be meaningful.
    fn estimate(&self) -> Option<u64> {
        let (t0, s0) = *self.samples.front()?;
        let (t1, s1) = *self.samples.back()?;
        let dt = t1 - t0;
        let dsoc = s1 - s0;
        if dt < Self::MIN_SPAN_SECS || dsoc.abs() < Self::MIN_DELTA {
            return None;
        }
        let rate = dsoc / dt; // charge fraction per second (signed)
        let secs = if self.charging {
            if rate <= 0.0 {
                return None; // not actually gaining charge
            }
            (1.0 - s1) / rate
        } else {
            if rate >= 0.0 {
                return None; // not actually losing charge
            }
            s1 / -rate
        };
        if secs <= 0.0 {
            return None;
        }
        Some((secs.round() as u64).min(Self::MAX_SECS))
    }

    /// Forget all history — called when the battery is neither charging nor
    /// discharging, so the next session starts from a fresh slope.
    fn reset(&mut self) {
        self.samples.clear();
        self.reported = None;
        self.last_report = f64::NEG_INFINITY;
    }
}

pub fn spawn(app: AppHandle, stop_flag: Arc<AtomicBool>) {
    tauri::async_runtime::spawn_blocking(move || {
        use sysinfo::System;
        // This loop only reads global CPU% and memory, so keep a bare `System`
        // and refresh just those two. `refresh_all()` additionally enumerates
        // every process (~340 of them) each second — about 4× the cost, for
        // data this loop never touches. See docs/benchmarks/sys_state-monitor.md.
        let mut sys = System::new();

        // Windows classifies the link via `GetAdaptersAddresses` (see
        // `detect_network`) and never reads sysinfo's `Networks`; only the
        // portable fallback maintains one.
        #[cfg(not(windows))]
        let mut networks = sysinfo::Networks::new_with_refreshed_list();

        let battery_manager = battery::Manager::new().ok();
        // Derives a stable time estimate from the real charge-rate (see doc).
        let mut battery_est = BatteryEstimator::new();
        let start = Instant::now();

        while !stop_flag.load(Ordering::Relaxed) {
            sys.refresh_cpu_usage();
            sys.refresh_memory();
            let now = start.elapsed().as_secs_f64();

            // CPU Usage
            let cpu_usage = sys.global_cpu_usage();

            // Memory Usage
            let total_mem = sys.total_memory();
            let used_mem = sys.used_memory();
            let mem_usage = if total_mem > 0 {
                (used_mem as f32 / total_mem as f32) * 100.0
            } else {
                0.0
            };

            // Network — distinguish Ethernet / Wi-Fi / other / offline.
            #[cfg(not(windows))]
            networks.refresh(true);
            #[cfg(windows)]
            let network = detect_network();
            #[cfg(not(windows))]
            let network = detect_network(&networks);

            // Uptime
            let uptime = sysinfo::System::uptime();

            // Battery
            let mut battery_status = None;
            if let Some(manager) = &battery_manager {
                if let Ok(mut batteries) = manager.batteries() {
                    if let Some(Ok(batt)) = batteries.next() {
                        let soc = batt.state_of_charge().value as f64;
                        let percent = (soc * 100.0).round() as u8;
                        match batt.state() {
                            battery::State::Charging => {
                                let time_to_full = battery_est.update(now, soc, true);
                                battery_status = Some(BatteryStatus::Charging { percent, time_to_full });
                            }
                            battery::State::Discharging => {
                                let time_to_empty = battery_est.update(now, soc, false);
                                battery_status = Some(BatteryStatus::Discharging { percent, time_to_empty });
                            }
                            _ => {
                                battery_est.reset();
                                battery_status = Some(BatteryStatus::PluggedIn { percent });
                            }
                        }
                    }
                }
            }

            let state = SystemState {
                cpu_usage,
                mem_usage,
                network,
                uptime,
                battery: battery_status,
            };

            let _ = app.emit("system-state", state);

            std::thread::sleep(Duration::from_secs(1));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feed `secs_span` seconds of 1 Hz samples where the charge changes
    /// linearly at `rate_per_sec` (positive = charging), starting at `soc0`.
    fn run(e: &mut BatteryEstimator, soc0: f64, rate_per_sec: f64, secs_span: u64, charging: bool) -> Option<u64> {
        let mut last = None;
        for i in 0..=secs_span {
            let now = i as f64;
            let soc = (soc0 + rate_per_sec * now).clamp(0.0, 1.0);
            last = e.update(now, soc, charging);
        }
        last
    }

    #[test]
    fn nothing_until_enough_history() {
        let mut e = BatteryEstimator::new();
        // Only 10 s of data — below MIN_SPAN_SECS — so no estimate yet.
        let last = run(&mut e, 0.50, -0.01 / 60.0, 10, false);
        assert_eq!(last, None);
    }

    #[test]
    fn nothing_when_charge_is_not_moving() {
        let mut e = BatteryEstimator::new();
        // Plenty of history but a flat charge level → can't project.
        let last = run(&mut e, 0.50, 0.0, 120, false);
        assert_eq!(last, None);
    }

    #[test]
    fn discharge_estimate_tracks_the_real_rate() {
        let mut e = BatteryEstimator::new();
        // 1 %/min drain from 50 %. After ~2 min, ~48 % left at 1 %/min ⇒ ~48 min.
        let last = run(&mut e, 0.50, -0.01 / 60.0, 120, false).expect("estimate");
        let mins = last / 60;
        assert!((45..=50).contains(&mins), "got {mins} min");
    }

    #[test]
    fn charge_estimate_tracks_the_real_rate() {
        let mut e = BatteryEstimator::new();
        // 1 %/min charge from 45 %. ~53 % to go at 1 %/min ⇒ ~53 min.
        let last = run(&mut e, 0.45, 0.01 / 60.0, 120, true).expect("estimate");
        let mins = last / 60;
        assert!((50..=56).contains(&mins), "got {mins} min");
    }

    #[test]
    fn reported_value_is_throttled_between_refreshes() {
        let mut e = BatteryEstimator::new();
        // Warm up to a first estimate (~30 s in, when MIN_SPAN/MIN_DELTA are met).
        run(&mut e, 0.50, -0.01 / 60.0, 30, false);
        let first = e.reported.expect("warmed up");
        // For the next few seconds (< REPORT_EVERY_SECS) the displayed value must
        // not change, even though the underlying slope keeps drifting slightly.
        for i in 31..45 {
            let soc = 0.50 - (0.01 / 60.0) * i as f64;
            assert_eq!(e.update(i as f64, soc, false), Some(first));
        }
    }

    #[test]
    fn direction_change_resets_the_estimate() {
        let mut e = BatteryEstimator::new();
        run(&mut e, 0.50, -0.01 / 60.0, 120, false);
        assert!(e.reported.is_some());
        // Plug in: the very next charging tick must drop the stale discharge
        // estimate and start warming up again.
        assert_eq!(e.update(121.0, 0.48, true), None);
    }

    #[test]
    fn reset_forgets_history() {
        let mut e = BatteryEstimator::new();
        run(&mut e, 0.50, -0.01 / 60.0, 120, false);
        e.reset();
        assert_eq!(e.update(0.0, 0.48, false), None); // fresh warm-up
    }
}
