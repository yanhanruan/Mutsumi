//! Weather fetcher.
//!
//! On startup, queries ip-api.com for the user's approximate location,
//! then polls Open-Meteo (no API key required) every WEATHER_INTERVAL for
//! the current weather condition. Emits `weather-update` events with the
//! WMO code, a matching emoji, a short label, and the temperature in °C.
//!
//! The pure code-to-emoji mapping is exposed as `weather_emoji()` so it
//! can be unit-tested without making any HTTP requests.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

const WEATHER_INTERVAL: Duration = Duration::from_secs(30 * 60); // 30 min
const RETRY_INTERVAL:   Duration = Duration::from_secs(60);      // 1 min after failure
const HTTP_TIMEOUT:     Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Serialize)]
pub struct WeatherUpdate {
    pub code:   u32,
    pub emoji:  String,
    pub label:  String,
    pub temp_c: f64,
    pub city:   Option<String>,
}

#[derive(Debug, Deserialize)]
struct GeoResponse {
    lat:  f64,
    lon:  f64,
    city: Option<String>,
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
                        "[weather] {} {}°C ({}) in {:?}",
                        update.emoji, update.temp_c, update.label, update.city
                    );
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

    // 1. Approximate location from IP. ip-api.com is free for non-commercial use
    //    with up to 45 requests/minute. We need it only once per startup.
    let geo: GeoResponse = client
        .get("http://ip-api.com/json/")
        .send()?
        .error_for_status()?
        .json()?;

    // 2. Current weather from Open-Meteo (no API key, no rate limits for low volume).
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&current=weather_code,temperature_2m",
        geo.lat, geo.lon
    );
    let resp: OpenMeteoResponse = client
        .get(&url)
        .send()?
        .error_for_status()?
        .json()?;

    let (emoji, label) = weather_emoji(resp.current.weather_code);
    Ok(WeatherUpdate {
        code:   resp.current.weather_code,
        emoji:  emoji.to_string(),
        label:  label.to_string(),
        temp_c: resp.current.temperature_2m,
        city:   geo.city,
    })
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weather_emoji_clear() {
        let (e, l) = weather_emoji(0);
        assert_eq!(e, "☀️");
        assert_eq!(l, "Clear");
    }

    #[test]
    fn weather_emoji_rain() {
        let (e, _) = weather_emoji(63);
        assert_eq!(e, "🌧️");
    }

    #[test]
    fn weather_emoji_snow() {
        let (e, _) = weather_emoji(73);
        assert_eq!(e, "❄️");
    }

    #[test]
    fn weather_emoji_thunderstorm() {
        let (e, _) = weather_emoji(95);
        assert_eq!(e, "⛈️");
    }

    #[test]
    fn weather_emoji_unknown_code_falls_back() {
        let (e, l) = weather_emoji(9999);
        assert_eq!(e, "🌡️");
        assert_eq!(l, "Unknown");
    }
}
