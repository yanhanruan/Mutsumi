mod app_state;
#[cfg(windows)]
mod audio;
mod cursor;
mod late_night;
mod persistence;
mod pomodoro;
mod state;
mod tray;
mod weather;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use app_state::SharedState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  let shared = SharedState::new();

  tauri::Builder::default()
    .manage(shared.clone())
    .invoke_handler(tauri::generate_handler![
      app_state::get_state,
      app_state::pet_click,
      app_state::pet_drag_end,
      app_state::pet_reset,
      app_state::pom_start,
      app_state::pom_pause,
      app_state::pom_stop,
      app_state::pom_set_durations,
    ])
    .setup(move |app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }

      // Load persisted state (energy/affection + pomodoro durations) if any.
      if let Some((pet, pom)) = persistence::load(app.handle()) {
        let mut g = shared.0.lock().unwrap();
        g.pet = pet;
        // Keep transient phase/remaining from defaults; only restore durations.
        g.pomodoro.focus_mins = pom.focus_mins;
        g.pomodoro.break_mins = pom.break_mins;
      }

      // Audio detector (Windows only).
      #[cfg(windows)]
      {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let _ = &stop_flag;   // not stored — runs for app lifetime
        audio::spawn(app.handle().clone(), stop_flag);
      }

      // System tray.
      tray::build(app.handle())?;

      // Weather fetcher (runs on all platforms).
      {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let _ = &stop_flag;
        weather::spawn(app.handle().clone(), stop_flag);
      }

      // OS-level cursor poller for per-pixel click-through hit testing.
      {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let _ = &stop_flag;
        cursor::spawn(app.handle().clone(), stop_flag);
      }

      // Pet state + pomodoro ticker (also drives late-night reminder).
      app_state::spawn_ticker(app.handle().clone(), shared.clone());

      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
