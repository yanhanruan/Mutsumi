mod app_state;
#[cfg(windows)]
mod audio;
mod card_export;
mod chat;
mod cursor;
// Data layer (SQLite memory store). The full public API lands ahead of its
// consumers in Phases 1–4 (LLM service, RAG chat, extraction, reflection), so
// allow dead_code across the module until those pipelines wire it up.
#[allow(dead_code)]
mod db;
mod http;
mod idle;
mod late_night;
#[cfg(windows)]
mod media;
mod persistence;
mod persona;
mod pomodoro;
mod search;
mod services;
mod state;
mod tray;
mod weather;
mod window_ops;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use app_state::SharedState;

/// Tauri command: ask the frontend to fade out, then hide the main window.
/// Mirrors `tray::hide_main_faded` so the context-menu "Hide App" button goes
/// through the same Rust-owned w.hide() path as the tray icon.
#[tauri::command]
async fn hide_pet(app: tauri::AppHandle) {
    tray::hide_main_faded(&app);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  // Load `src-tauri/.env` (auth token + reCAPTCHA for external services) into the
  // process environment before any service config is read. Absent file is fine.
  let _ = dotenvy::dotenv();

  let shared = SharedState::new();
  let weather_state = weather::WeatherState::new();
  let audio_state   = audio::AudioState(AtomicBool::new(false));
  let media_state   = media::MediaState::new();
  let fish_audio_state = services::FishAudioState::new(services::fish_audio::config_from_env())
    .expect("failed to build Fish Audio TTS service");
  let qwen_state    = services::QwenState::new(services::qwen::config_from_env())
    .expect("failed to build Qwen LLM client");
  let search_state  = search::SearchState::new(search::SearchEngine::default())
    .expect("failed to build search client");

  tauri::Builder::default()
    // ── Single-instance guard ─────────────────────────────────────────────
    // If a second instance is launched while the app is already running, the
    // plugin kills the second process and focuses the existing main window.
    .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
      use tauri::Manager;
      if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
      }
    }))
    .plugin(tauri_plugin_autostart::init(
      tauri_plugin_autostart::MacosLauncher::LaunchAgent,
      None,
    ))
    .manage(shared.clone())
    .manage(weather_state)
    .manage(audio_state)
    .manage(media_state)
    .manage(fish_audio_state)
    .manage(qwen_state)
    .manage(search_state)
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
      media::get_media,
      media::media_play_pause,
      media::media_next,
      media::media_prev,
      media::media_stop,
      media::media_replay,
      media::media_skip,
      media::media_seek,
      media::media_toggle_mute,
      media::media_set_volume,
      media::media_select,
      app_state::set_tray_locale,
      window_ops::set_window_bounds,
      card_export::save_card_image,
      card_export::reveal_in_folder,
      services::tts_synthesize,
      services::tts_set_recaptcha,
      chat::chat_send,
      chat::chat_stream,
      search::set_search_engine,
      hide_pet,
    ])
    .setup(move |app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }

      use tauri::Manager;

      // Open the SQLite memory database (dynamic user data). Registered as
      // managed state; async commands access it via spawn_blocking.
      match db::Db::open(app.handle()) {
        Ok(database) => {
          app.manage(database);

          // Pipeline C: on startup, reflect on any observations that piled up
          // in previous sessions (runs in the background; no-op if too few).
          let handle = app.handle().clone();
          tauri::async_runtime::spawn(async move {
            chat::reflection::maybe_reflect(&handle, chat::reflection::REFLECTION_STARTUP_MIN).await;
          });
        }
        Err(e) => log::error!("failed to open memory database: {e}"),
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

        // Media transport controller (SMTC now-playing + transport).
        let media_stop_flag = Arc::new(AtomicBool::new(false));
        let _ = &media_stop_flag;
        media::spawn(app.handle().clone(), media_stop_flag);
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
