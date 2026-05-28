//! Weather fetcher.
//!
//! On startup, queries ipapi.co for the user's approximate location,
//! then polls Open-Meteo (no API key required) every WEATHER_INTERVAL for
//! the current weather condition. Emits `weather-update` events carrying
//! the WMO code, a short label, temperature in °C, and city name.
//!
//! `weather_emoji()` is a pure mapping kept separate so it can be
//! unit-tested without making any HTTP requests.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

const WEATHER_INTERVAL: Duration = Duration::from_secs(30 * 60); // 30 min
const RETRY_INTERVAL:   Duration = Duration::from_secs(60);      // 1 min after failure
const HTTP_TIMEOUT:     Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Serialize)]
pub struct WeatherUpdate {
    pub code:   u32,
    pub label:  String,
    pub temp_c: f64,
    pub city:   Option<String>,
}

#[derive(Debug, Deserialize)]
struct GeoResponse {
    // ipapi.co field names
    latitude:  f64,
    longitude: f64,
    city:      Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenMeteoResponse {
    current: OpenMeteoCurrent,
}

#[derive(Debug, Deserialize)]
struct OpenMeteoCurrent {
    weather_code:   u32,
    temperature_2m: f64,
}

/// Managed state — holds the most recent successful weather fetch.
/// The frontend reads this on mount to avoid missing the first event.
pub struct WeatherState(pub Mutex<Option<WeatherUpdate>>);

/// Tauri command: returns the last cached weather update (or null if none yet).
#[tauri::command]
pub fn get_weather(state: tauri::State<WeatherState>) -> Option<WeatherUpdate> {
    state.0.lock().unwrap().clone()
}

/// Map a WMO weather code to (emoji, English label).
///
/// Reference: <https://open-meteo.com/en/docs> (WMO weather interpretation codes).
pub fn weather_emoji(code: u32) -> (&'static str, &'static str) {
    match code {
        0                              => ("☀️",  "Clear"),
        1 | 2                          => ("🌤️",  "Partly cloudy"),
        3                              => ("☁️",  "Cloudy"),
        45 | 48                        => ("🌫️",  "Foggy"),
        51 | 53 | 55                   => ("🌦️",  "Drizzle"),
        56 | 57                        => ("🌧️",  "Freezing drizzle"),
        61 | 63 | 65                   => ("🌧️",  "Rain"),
        66 | 67                        => ("🌧️",  "Freezing rain"),
        71 | 73 | 75 | 77              => ("❄️",  "Snow"),
        80 | 81 | 82                   => ("🌧️",  "Rain showers"),
        85 | 86                        => ("❄️",  "Snow showers"),
        95                             => ("⛈️",  "Thunderstorm"),
        96 | 99                        => ("⛈️",  "Thunderstorm with hail"),
        _                              => ("🌡️",  "Unknown"),
    }
}

pub fn spawn(app: AppHandle, stop_flag: Arc<AtomicBool>) {
    thread::spawn(move || {
        while !stop_flag.load(Ordering::Relaxed) {
            match fetch_once() {
                Ok(update) => {
                    log::info!(
                        "[weather] {}°C ({}) in {:?}",
                        update.temp_c, update.label, update.city
                    );
                    // Cache before emitting so get_weather() is always warm.
                    if let Some(state) = app.try_state::<WeatherState>() {
                        *state.0.lock().unwrap() = Some(update.clone());
                    }
                    let _ = app.emit("weather-update", &update);
                    thread::sleep(WEATHER_INTERVAL);
                }
                Err(e) => {
                    log::warn!("[weather] fetch failed: {} — retrying in 60s", e);
                    thread::sleep(RETRY_INTERVAL);
                }
            }
        }
    });
}

fn fetch_once() -> Result<WeatherUpdate, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()?;

    // 1. Approximate location from IP.
    //    ipapi.co — free tier (no key), HTTPS, 30 k requests/month.
    let geo: GeoResponse = client
        .get("https://ipapi.co/json/")
        .header("User-Agent", "mutsumi-desktop-pet/1.0")
        .send()?
        .error_for_status()?
        .json()?;

    // 2. Current weather from Open-Meteo (no API key, no rate limits for low volume).
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&current=weather_code,temperature_2m",
        geo.latitude, geo.longitude
    );
    let resp: OpenMeteoResponse = client
        .get(&url)
        .send()?
        .error_for_status()?
        .json()?;

    let (_, label) = weather_emoji(resp.current.weather_code);
    Ok(WeatherUpdate {
        code:   resp.current.weather_code,
        label:  label.to_string(),
        temp_c: resp.current.temperature_2m,
        city:   geo.city,
    })
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── weather_emoji: every WMO arm ───────────────────────────────

    #[test]
    fn emoji_clear() {
        let (e, l) = weather_emoji(0);
        assert_eq!(e, "☀️");
        assert_eq!(l, "Clear");
    }

    #[test]
    fn emoji_partly_cloudy_code1() {
        let (e, l) = weather_emoji(1);
        assert_eq!(e, "🌤️");
        assert_eq!(l, "Partly cloudy");
    }

    #[test]
    fn emoji_partly_cloudy_code2() {
        let (e, _) = weather_emoji(2);
        assert_eq!(e, "🌤️");
    }

    #[test]
    fn emoji_cloudy() {
        let (e, l) = weather_emoji(3);
        assert_eq!(e, "☁️");
        assert_eq!(l, "Cloudy");
    }

    #[test]
    fn emoji_fog_code45() {
        let (e, l) = weather_emoji(45);
        assert_eq!(e, "🌫️");
        assert_eq!(l, "Foggy");
    }

    #[test]
    fn emoji_fog_code48() {
        let (e, _) = weather_emoji(48);
        assert_eq!(e, "🌫️");
    }

    #[test]
    fn emoji_drizzle_light() {
        let (e, l) = weather_emoji(51);
        assert_eq!(e, "🌦️");
        assert_eq!(l, "Drizzle");
    }

    #[test]
    fn emoji_drizzle_moderate() { let (e, _) = weather_emoji(53); assert_eq!(e, "🌦️"); }

    #[test]
    fn emoji_drizzle_dense()    { let (e, _) = weather_emoji(55); assert_eq!(e, "🌦️"); }

    #[test]
    fn emoji_freezing_drizzle_light() {
        let (e, l) = weather_emoji(56);
        assert_eq!(e, "🌧️");
        assert_eq!(l, "Freezing drizzle");
    }

    #[test]
    fn emoji_freezing_drizzle_dense() { let (e, _) = weather_emoji(57); assert_eq!(e, "🌧️"); }

    #[test]
    fn emoji_rain_light()    { let (e, l) = weather_emoji(61); assert_eq!(e, "🌧️"); assert_eq!(l, "Rain"); }

    #[test]
    fn emoji_rain_moderate() { let (e, _) = weather_emoji(63); assert_eq!(e, "🌧️"); }

    #[test]
    fn emoji_rain_heavy()    { let (e, _) = weather_emoji(65); assert_eq!(e, "🌧️"); }

    #[test]
    fn emoji_freezing_rain_light() {
        let (e, l) = weather_emoji(66);
        assert_eq!(e, "🌧️");
        assert_eq!(l, "Freezing rain");
    }

    #[test]
    fn emoji_freezing_rain_heavy() { let (e, _) = weather_emoji(67); assert_eq!(e, "🌧️"); }

    #[test]
    fn emoji_snow_light()    { let (e, l) = weather_emoji(71); assert_eq!(e, "❄️"); assert_eq!(l, "Snow"); }

    #[test]
    fn emoji_snow_moderate() { let (e, _) = weather_emoji(73); assert_eq!(e, "❄️"); }

    #[test]
    fn emoji_snow_heavy()    { let (e, _) = weather_emoji(75); assert_eq!(e, "❄️"); }

    #[test]
    fn emoji_snow_grains()   { let (e, _) = weather_emoji(77); assert_eq!(e, "❄️"); }

    #[test]
    fn emoji_rain_showers_slight()   { let (e, l) = weather_emoji(80); assert_eq!(e, "🌧️"); assert_eq!(l, "Rain showers"); }

    #[test]
    fn emoji_rain_showers_moderate() { let (e, _) = weather_emoji(81); assert_eq!(e, "🌧️"); }

    #[test]
    fn emoji_rain_showers_violent()  { let (e, _) = weather_emoji(82); assert_eq!(e, "🌧️"); }

    #[test]
    fn emoji_snow_showers_slight() {
        let (e, l) = weather_emoji(85);
        assert_eq!(e, "❄️");
        assert_eq!(l, "Snow showers");
    }

    #[test]
    fn emoji_snow_showers_heavy() { let (e, _) = weather_emoji(86); assert_eq!(e, "❄️"); }

    #[test]
    fn emoji_thunderstorm() {
        let (e, l) = weather_emoji(95);
        assert_eq!(e, "⛈️");
        assert_eq!(l, "Thunderstorm");
    }

    #[test]
    fn emoji_thunderstorm_with_hail_slight() {
        let (e, l) = weather_emoji(96);
        assert_eq!(e, "⛈️");
        assert_eq!(l, "Thunderstorm with hail");
    }

    #[test]
    fn emoji_thunderstorm_with_hail_heavy() { let (e, _) = weather_emoji(99); assert_eq!(e, "⛈️"); }

    #[test]
    fn emoji_unknown_falls_back() {
        let (e, l) = weather_emoji(9999);
        assert_eq!(e, "🌡️");
        assert_eq!(l, "Unknown");
    }

    // Gap codes that have no WMO meaning and must return Unknown.
    #[test]
    fn emoji_gap_between_cloudy_and_fog()      { assert_eq!(weather_emoji(4).1,  "Unknown"); }

    #[test]
    fn emoji_gap_between_fog_and_drizzle()     { assert_eq!(weather_emoji(50).1, "Unknown"); }

    #[test]
    fn emoji_gap_between_drizzle_and_rain()    { assert_eq!(weather_emoji(58).1, "Unknown"); }

    #[test]
    fn emoji_gap_between_rain_and_snow()       { assert_eq!(weather_emoji(70).1, "Unknown"); }

    #[test]
    fn emoji_gap_between_snow_and_showers()    { assert_eq!(weather_emoji(79).1, "Unknown"); }

    #[test]
    fn emoji_gap_between_showers_and_storm()   { assert_eq!(weather_emoji(90).1, "Unknown"); }

    #[test]
    fn emoji_gap_inside_storm_codes()          { assert_eq!(weather_emoji(97).1, "Unknown"); }

    // ── WeatherState cache ─────────────────────────────────────────

    #[test]
    fn weather_state_starts_empty() {
        let state = WeatherState(Mutex::new(None));
        assert!(state.0.lock().unwrap().is_none());
    }

    #[test]
    fn weather_state_stores_and_retrieves_update() {
        let state = WeatherState(Mutex::new(None));
        let update = WeatherUpdate {
            code:   63,
            label:  "Rain".to_string(),
            temp_c: 14.5,
            city:   Some("Tokyo".to_string()),
        };
        *state.0.lock().unwrap() = Some(update);

        let stored = state.0.lock().unwrap().clone().unwrap();
        assert_eq!(stored.code,   63);
        assert_eq!(stored.label,  "Rain");
        assert_eq!(stored.temp_c, 14.5);
        assert_eq!(stored.city.as_deref(), Some("Tokyo"));
    }

    #[test]
    fn weather_state_overwrites_previous_update() {
        let state = WeatherState(Mutex::new(None));

        *state.0.lock().unwrap() = Some(WeatherUpdate {
            code: 0, label: "Clear".to_string(), temp_c: 25.0, city: None,
        });
        *state.0.lock().unwrap() = Some(WeatherUpdate {
            code: 95, label: "Thunderstorm".to_string(), temp_c: 18.0, city: None,
        });

        let stored = state.0.lock().unwrap().clone().unwrap();
        assert_eq!(stored.code,  95);
        assert_eq!(stored.label, "Thunderstorm");
    }
}
