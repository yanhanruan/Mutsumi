mod app_state;
#[cfg(windows)]
mod audio;
#[cfg(test)]
mod benchmarks;
mod card_export;
mod chat;
mod cursor;
// Data layer (SQLite memory store). The full public API lands ahead of its
// consumers in Phases 1–4 (LLM service, RAG chat, extraction, reflection), so
// allow dead_code across the module until those pipelines wire it up.
#[allow(dead_code)]
mod db;
mod flight;
mod hardware;
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
mod update_window;
mod weather;
mod window_ops;
mod sys_state;

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
  let search_state  = search::SearchState::new(search::SearchEngine::default());

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
    .plugin(tauri_plugin_dialog::init())
    .plugin(tauri_plugin_clipboard_manager::init())
    .plugin(tauri_plugin_opener::init())
    .plugin(tauri_plugin_updater::Builder::new().build())
    .plugin(tauri_plugin_process::init())
    .manage(shared.clone())
    .manage(app_state::LocaleState::default())
    .manage(update_window::PendingUpdateState::default())
    .manage(weather_state)
    .manage(audio_state)
    .manage(media_state)
    .manage(fish_audio_state)
    .manage(qwen_state)
    .manage(search_state)
    .manage(search::webview::WebviewSerp::default())
    .manage(chat::ChatBuffer::new())
    .manage(sys_state::SysMonitor::default())
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
      flight::flight_set_insets,
      flight::flight_set_screensaver,
      flight::flight_begin_motion,
      card_export::save_card_image,
      card_export::reveal_in_folder,
      services::tts_synthesize,
      services::tts_set_recaptcha,
      services::qwen_set_api_key,
      services::qwen_key_status,
      services::qwen_set_chat_model,
      chat::chat_send,
      chat::chat_stream,
      chat::chat_clear_memory,
      chat::chat_flush_memory,
      chat::chat_recent_messages,
      chat::chat_messages_after,
      chat::chat_search_history,
      chat::chat_vision_stream,
      chat::chat_stage_clipboard_image,
      search::set_search_engine,
      search::set_search_enabled,
      hardware::get_hardware_info,
      sys_state::sys_monitor_start,
      sys_state::sys_monitor_stop,
      update_window::open_update_window,
      update_window::get_pending_update,
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

      // Apply a user-set 百炼 (Qwen/DashScope) API key from Settings, overriding
      // the .env default. End users without a .env rely entirely on this.
      if let Some(key) = services::load_persisted_qwen_key() {
        if let Some(qwen) = app.try_state::<services::QwenState>() {
          if let Err(e) = qwen.set_api_key(&key) {
            log::warn!("failed to apply persisted Qwen API key: {e}");
          }
        }
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

      // System idle monitor — feeds the flying-screensaver input of the
      // flight mode controller (flight.rs).
      {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let _ = &stop_flag;
        idle::spawn(app.handle().clone(), stop_flag);
      }

      // Global shortcut: Ctrl+Alt+F toggles manual flying mode anywhere.
      //
      // The hotkey is a nice-to-have, never a launch requirement. If another
      // running app already owns Ctrl+Alt+F, the OS RegisterHotKey call rejects
      // our request — and if that error is allowed to bubble out of setup() the
      // whole app fails to start. So we register the plugin with NO pre-bound
      // shortcuts (plugin init then can't fail on a collision), then try to bind
      // Ctrl+Alt+F on its own. On collision we just log and carry on with the
      // hotkey disabled, so startup always succeeds.
      #[cfg(desktop)]
      {
        use tauri_plugin_global_shortcut::{
          Builder as ShortcutBuilder, GlobalShortcutExt, ShortcutState,
        };
        app.handle().plugin(ShortcutBuilder::new().build())?;
        if let Err(e) = app.global_shortcut().on_shortcut(
          "ctrl+alt+f",
          |app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
              flight::toggle_manual(app);
            }
          },
        ) {
          log::warn!(
            "Ctrl+Alt+F global shortcut unavailable (likely already in use by \
             another app); flying-mode hotkey disabled for this session: {e}"
          );
        }
      }

      // System state monitor is NOT started here: it runs only while the
      // System State panel is open (sys_monitor_start / _stop commands), so it
      // costs nothing when closed.

      // Pet state + pomodoro ticker (also drives late-night reminder).
      app_state::spawn_ticker(app.handle().clone(), shared.clone());

      // Bot-detection benchmark — inert unless MUTSUMI_SERP_BENCH is set.
      search::bench::maybe_spawn(app.handle());

      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
