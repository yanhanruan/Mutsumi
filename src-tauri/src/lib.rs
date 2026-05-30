mod app_state;
#[cfg(windows)]
mod audio;
mod cursor;
mod idle;
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
  let weather_state = weather::WeatherState::new();
  let audio_state   = audio::AudioState(AtomicBool::new(false));

  tauri::Builder::default()
    .plugin(tauri_plugin_autostart::init(
      tauri_plugin_autostart::MacosLauncher::LaunchAgent,
      None,
    ))
    .manage(shared.clone())
    .manage(weather_state)
    .manage(audio_state)
    .invoke_handler(tauri::generate_handler![
      app_state::get_state,
      app_state::pet_click,
      app_state::pet_drag_end,
      app_state::pet_reset,
      app_state::pet_context_action,
      app_state::pom_start,
      app_state::pom_pause,
      app_state::pom_stop,
      app_state::pom_set_durations,
      weather::get_weather,
      weather::get_weather_status,
      audio::get_audio_state,
      app_state::set_tray_locale,
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

      // System idle monitor — emits `toggle-balloon-mode` events.
      {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let _ = &stop_flag;
        idle::spawn(app.handle().clone(), stop_flag);
      }

      // Pet state + pomodoro ticker (also drives late-night reminder).
      app_state::spawn_ticker(app.handle().clone(), shared.clone());

      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
