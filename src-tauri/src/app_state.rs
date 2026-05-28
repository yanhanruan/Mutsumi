//! Combined app state: pet state + pomodoro, behind a Mutex.
//!
//! A single ticker thread acquires the lock once per second, advances both,
//! and emits the appropriate Tauri events. Vue calls into the backend via
//! commands defined here.

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::late_night::LateNightReminder;
use crate::pomodoro::PomodoroState;
use crate::state::PetState;

#[derive(Debug, Default)]
pub struct AppState {
    pub pet:      PetState,
    pub pomodoro: PomodoroState,
}

/// Shared handle stored in Tauri's State container.
#[derive(Clone)]
pub struct SharedState(pub Arc<Mutex<AppState>>);

#[derive(Debug, Serialize, Clone)]
struct StateSnapshot {
    pet:      PetState,
    pomodoro: PomodoroState,
}

impl SharedState {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(AppState::default())))
    }

    fn snapshot(&self) -> StateSnapshot {
        let g = self.0.lock().unwrap();
        StateSnapshot { pet: g.pet, pomodoro: g.pomodoro }
    }
}

/// Spawn the once-per-second ticker that advances pomodoro and (every 5s)
/// the pet state. Emits `pomodoro-tick`, `pomodoro-phase-change`,
/// `pet-state-update` events.
pub fn spawn_ticker(app: AppHandle, shared: SharedState) {
    thread::spawn(move || {
        use chrono::{Local, Timelike};
        let mut sec_count: u64 = 0;
        let mut late_night = LateNightReminder::new();
        // Seed the reminder with the current hour so a startup at 02:00 fires
        // immediately on the first minute-tick (rather than only after we've
        // observed an out-of-window hour first).
        // Note: observe() consumes the "fire" if currently in window; that's
        // exactly the behavior we want for the first observation post-startup.
        loop {
            thread::sleep(Duration::from_secs(1));
            sec_count += 1;

            // ── Tick state under lock ──
            let (snap, phase_change) = {
                let mut g = shared.0.lock().unwrap();
                let phase_change = g.pomodoro.tick();
                if sec_count % 5 == 0 {
                    g.pet.tick();
                }
                let snap = StateSnapshot { pet: g.pet, pomodoro: g.pomodoro };
                (snap, phase_change)
            };

            // ── Emit events outside the lock ──
            let _ = app.emit("pomodoro-tick", &snap.pomodoro);
            if let Some(new_phase) = phase_change {
                let _ = app.emit("pomodoro-phase-change", new_phase);
            }
            if sec_count % 5 == 0 {
                let _ = app.emit("pet-state-update", &snap.pet);
            }

            // ── Late-night reminder check (once per minute) ──
            if sec_count % 60 == 0 {
                let hour = Local::now().hour();
                if late_night.observe(hour) {
                    log::info!("[late-night] reminder firing at hour {}", hour);
                    let _ = app.emit("late-night-reminder", ());
                }
            }
        }
    });
}

// ── Tauri commands ─────────────────────────────────────────────────

#[tauri::command]
pub fn get_state(shared: State<SharedState>) -> serde_json::Value {
    serde_json::to_value(shared.snapshot()).unwrap_or(serde_json::Value::Null)
}

fn persist(app: &AppHandle, shared: &SharedState) {
    let g = shared.0.lock().unwrap();
    crate::persistence::save(app, &g.pet, &g.pomodoro);
}

#[tauri::command]
pub fn pet_click(app: AppHandle, shared: State<SharedState>) {
    {
        let mut g = shared.0.lock().unwrap();
        g.pet.on_click();
    }
    persist(&app, shared.inner());
}

#[tauri::command]
pub fn pet_drag_end(app: AppHandle, shared: State<SharedState>, rough: bool) {
    {
        let mut g = shared.0.lock().unwrap();
        g.pet.on_drag_release(rough);
    }
    persist(&app, shared.inner());
}

#[tauri::command]
pub fn pom_start(shared: State<SharedState>) {
    shared.0.lock().unwrap().pomodoro.start();
}

#[tauri::command]
pub fn pom_pause(shared: State<SharedState>) {
    shared.0.lock().unwrap().pomodoro.pause();
}

#[tauri::command]
pub fn pom_stop(shared: State<SharedState>) {
    shared.0.lock().unwrap().pomodoro.stop();
}

#[tauri::command]
pub fn pom_set_durations(
    app: AppHandle,
    shared: State<SharedState>,
    focus_mins: u32,
    break_mins: u32,
) {
    {
        let mut g = shared.0.lock().unwrap();
        g.pomodoro.focus_mins = focus_mins.max(1);
        g.pomodoro.break_mins = break_mins.max(1);
    }
    persist(&app, shared.inner());
}

#[tauri::command]
pub fn pet_reset(app: AppHandle, shared: State<SharedState>) {
    {
        let mut g = shared.0.lock().unwrap();
        g.pet = PetState::default();
    }
    persist(&app, shared.inner());
}

// Suppress unused warning for AppHandle::path in modules that re-import it.
#[allow(dead_code)]
fn _unused_path(app: &AppHandle) -> Option<()> {
    let _ = app.path();
    None
}
