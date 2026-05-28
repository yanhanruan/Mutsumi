//! Plain JSON persistence for PetState and pomodoro durations.
//!
//! Mirrors the Python original's `persistence.py`, but minimally: synchronous
//! save on every mutation since the payload is tiny (<300 bytes). A debounce
//! layer can be added later if profiling shows it matters.
//!
//! Storage path: <app_data_dir>/state.json
//!   where app_data_dir on Windows is %APPDATA%\com.mutsumi.app

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::pomodoro::PomodoroState;
use crate::state::{Mood, PetState};

#[derive(Debug, Serialize, Deserialize)]
struct PersistedState {
    energy:     f32,
    affection:  f32,
    focus_mins: u32,
    break_mins: u32,
}

fn state_path(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_data_dir().ok()?;
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("state.json"))
}

pub fn load(app: &AppHandle) -> Option<(PetState, PomodoroState)> {
    let path = state_path(app)?;
    let bytes = std::fs::read(&path).ok()?;
    let p: PersistedState = serde_json::from_slice(&bytes).ok()?;

    let mut pet = PetState {
        energy:    p.energy.clamp(0.0, 100.0),
        affection: p.affection.clamp(0.0, 100.0),
        mood:      Mood::Content,
    };
    pet.recompute_mood();

    let pom = PomodoroState {
        phase:          crate::pomodoro::Phase::Idle,
        focus_mins:     p.focus_mins.max(1),
        break_mins:     p.break_mins.max(1),
        remaining_secs: 0,
        running:        false,
    };
    Some((pet, pom))
}

pub fn save(app: &AppHandle, pet: &PetState, pom: &PomodoroState) {
    let Some(path) = state_path(app) else { return };
    let p = PersistedState {
        energy:     pet.energy,
        affection:  pet.affection,
        focus_mins: pom.focus_mins,
        break_mins: pom.break_mins,
    };
    if let Ok(json) = serde_json::to_vec_pretty(&p) {
        let _ = std::fs::write(&path, json);
    }
}
