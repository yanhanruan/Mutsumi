use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

#[derive(Clone, Serialize)]
#[serde(tag = "type")]
pub enum BatteryStatus {
    Charging { percent: u8, time_to_full: Option<u64> },
    Discharging { percent: u8, time_to_empty: Option<u64> },
    PluggedIn { percent: u8 },
}

#[derive(Clone, Serialize)]
pub struct SystemState {
    pub cpu_usage: f32,
    pub mem_usage: f32,
    pub temperature: Option<f32>,
    pub network_connected: bool,
    pub uptime: u64,
    pub battery: Option<BatteryStatus>,
}

pub fn spawn(app: AppHandle, stop_flag: Arc<AtomicBool>) {
    tauri::async_runtime::spawn_blocking(move || {
        use sysinfo::{System, Networks, Components};
        let mut sys = System::new_all();
        let mut networks = Networks::new_with_refreshed_list();
        let mut components = Components::new_with_refreshed_list();
        let battery_manager = battery::Manager::new().ok();

        while !stop_flag.load(Ordering::Relaxed) {
            sys.refresh_all();
            networks.refresh(true);
            components.refresh(true);

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

            // Temperature (max)
            let mut max_temp: Option<f32> = None;
            for component in &components {
                if let Some(t) = component.temperature() {
                    if max_temp.is_none() || t > max_temp.unwrap() {
                        max_temp = Some(t);
                    }
                }
            }

            // Network
            let mut network_connected = false;
            for (interface_name, data) in &networks {
                let name = interface_name.to_lowercase();
                if name.contains("loopback") || name.contains("lo") {
                    continue;
                }
                if data.total_received() > 0 || data.total_transmitted() > 0 {
                    // This is a basic heuristic for connected.
                    network_connected = true;
                    break;
                }
            }

            // Uptime
            let uptime = sysinfo::System::uptime();

            // Battery
            let mut battery_status = None;
            if let Some(manager) = &battery_manager {
                if let Ok(mut batteries) = manager.batteries() {
                    if let Some(Ok(batt)) = batteries.next() {
                        let percent = (batt.state_of_charge().value * 100.0).round() as u8;
                        match batt.state() {
                            battery::State::Charging => {
                                let time_to_full = batt.time_to_full().map(|t| t.value.round() as u64);
                                battery_status = Some(BatteryStatus::Charging { percent, time_to_full });
                            }
                            battery::State::Discharging => {
                                let time_to_empty = batt.time_to_empty().map(|t| t.value.round() as u64);
                                battery_status = Some(BatteryStatus::Discharging { percent, time_to_empty });
                            }
                            _ => {
                                battery_status = Some(BatteryStatus::PluggedIn { percent });
                            }
                        }
                    }
                }
            }

            let state = SystemState {
                cpu_usage,
                mem_usage,
                temperature: max_temp,
                network_connected,
                uptime,
                battery: battery_status,
            };

            let _ = app.emit("system-state", state);

            std::thread::sleep(Duration::from_secs(1));
        }
    });
}
