//! Weather fetcher.
//!
//! Geo-location uses a two-service fallback chain so the feature works in
//! regions where one provider is blocked or rate-limited:
//!
//!   1. ipapi.co   — primary   (works globally; free, no key)
//!   2. ipinfo.io  — fallback  (HTTPS, reliable in mainland China; 50k req/month free)
//!
//! Weather data comes from Open-Meteo (no API key required, Cloudflare CDN —
//! accessible globally including mainland China on all major ISPs).
//!
//! Emits two event types:
//!   • `weather-update`  — carries WMO code, label, °C, and city
//!   • `weather-status`  — carries `{ available: bool }`, fired whenever
//!                         availability changes (first success, or after
//!                         MAX_FAILURES consecutive failures)
//!
//! `WeatherState` exposes `record_success` / `record_failure` so the loop
//! can update shared state and decide whether to re-emit the status event.
//! Those methods are unit-tested directly without making HTTP calls.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

const WEATHER_INTERVAL: Duration = Duration::from_secs(30 * 60); // 30 min
const RETRY_INTERVAL:   Duration = Duration::from_secs(60);      // 1 min after failure
const HTTP_TIMEOUT:     Duration = Duration::from_secs(10);

/// After this many consecutive failures the badge is suppressed.
pub const MAX_FAILURES: u32 = 3;

// ── Public types ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct WeatherUpdate {
    pub code:   u32,
    pub label:  String,
    pub temp_c: f64,
    pub city:   Option<String>,
}

/// Payload of the `weather-status` event.
#[derive(Debug, Clone, Serialize)]
pub struct WeatherStatus {
    pub available: bool,
}

// ── Managed state ───────────────────────────────────────────────────

/// Shared state managed by Tauri.
///
/// `available` starts `false` (nothing fetched yet).
/// It flips to `true` on the first successful fetch, and back to `false`
/// once MAX_FAILURES consecutive fetches fail with no intervening success.
pub struct WeatherState {
    pub update:               Mutex<Option<WeatherUpdate>>,
    pub available:            AtomicBool,
    pub consecutive_failures: AtomicU32,
}

impl WeatherState {
    pub fn new() -> Self {
        Self {
            update:               Mutex::new(None),
            available:            AtomicBool::new(false),
            consecutive_failures: AtomicU32::new(0),
        }
    }

    /// Record a successful fetch.  Returns `true` if availability changed
    /// (i.e. the first success, or recovery after a run of failures).
    pub fn record_success(&self, update: WeatherUpdate) -> bool {
        *self.update.lock().unwrap() = Some(update);
        self.consecutive_failures.store(0, Ordering::Relaxed);
        // swap returns the *old* value; flip to true ⟹ changed iff was false
        !self.available.swap(true, Ordering::Relaxed)
    }

    /// Record a failed fetch.  Returns `true` if availability changed
    /// (i.e. we just crossed the MAX_FAILURES threshold).
    pub fn record_failure(&self) -> bool {
        // fetch_add returns the value *before* the add, so +1 for current.
        let new_count = self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
        if new_count >= MAX_FAILURES {
            // swap returns old value; flip to false ⟹ changed iff was true
            return self.available.swap(false, Ordering::Relaxed);
        }
        false
    }

    /// Current availability (safe to call from any thread).
    pub fn is_available(&self) -> bool {
        self.available.load(Ordering::Relaxed)
    }
}

// ── Tauri commands ──────────────────────────────────────────────────

/// Returns the last cached weather update (or `null` if none yet).
#[tauri::command]
pub fn get_weather(state: tauri::State<WeatherState>) -> Option<WeatherUpdate> {
    state.update.lock().unwrap().clone()
}

/// Returns whether weather data is currently available.
/// The frontend calls this on mount; live changes arrive via `weather-status`.
#[tauri::command]
pub fn get_weather_status(state: tauri::State<WeatherState>) -> bool {
    state.is_available()
}

// ── HTTP deserialisers ──────────────────────────────────────────────

/// ipapi.co response shape.
#[derive(Debug, Deserialize)]
struct IpApiResponse {
    latitude:  f64,
    longitude: f64,
    city:      Option<String>,
}

/// ipinfo.io response shape.  `loc` is a "lat,lon" string.
#[derive(Debug, Deserialize)]
struct IpInfoResponse {
    city: Option<String>,
    loc:  String,
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

// Normalised result from whichever geo service succeeded.
struct GeoResult {
    latitude:  f64,
    longitude: f64,
    city:      Option<String>,
}

// ── Weather emoji map ───────────────────────────────────────────────

/// Map a WMO weather code to (emoji, English label).
///
/// Reference: <https://open-meteo.com/en/docs>
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

// ── Fetch loop ──────────────────────────────────────────────────────

pub fn spawn(app: AppHandle, stop_flag: Arc<AtomicBool>) {
    thread::spawn(move || {
        // Build the client once and reuse it — avoids TLS re-init on every poll.
        // native-tls (Schannel on Windows) reads the OS certificate store, which
        // handles VPNs / corporate proxies that rustls-tls silently rejects.
        let client = match reqwest::blocking::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                log::error!("[weather] failed to build HTTP client: {:#}", e);
                return;
            }
        };

        while !stop_flag.load(Ordering::Relaxed) {
            let state = match app.try_state::<WeatherState>() {
                Some(s) => s,
                None    => { thread::sleep(RETRY_INTERVAL); continue; }
            };

            match fetch_once(&client) {
                Ok(update) => {
                    log::info!(
                        "[weather] {}°C ({}) in {:?}",
                        update.temp_c, update.label, update.city
                    );
                    let changed = state.record_success(update.clone());
                    if changed {
                        let _ = app.emit("weather-status", WeatherStatus { available: true });
                    }
                    let _ = app.emit("weather-update", &update);
                    thread::sleep(WEATHER_INTERVAL);
                }
                Err(e) => {
                    // {:#} prints the full error chain for easier diagnosis
                    log::warn!("[weather] fetch failed: {:#} — retrying in 60s", e);
                    let changed = state.record_failure();
                    if changed {
                        let _ = app.emit("weather-status", WeatherStatus { available: false });
                    }
                    thread::sleep(RETRY_INTERVAL);
                }
            }
        }
    });
}

fn fetch_geo(client: &reqwest::blocking::Client) -> Result<GeoResult, Box<dyn std::error::Error + Send + Sync>> {
    // Primary: ipapi.co — free, no key, works globally.
    match fetch_geo_ipapi(client) {
        Ok(r) => return Ok(r),
        Err(e) => log::warn!("[weather] ipapi.co geo failed ({:#}) — trying ipinfo.io", e),
    }
    // Fallback: ipinfo.io — HTTPS, reliable in mainland China, 50k req/month free.
    fetch_geo_ipinfo(client)
}

fn fetch_geo_ipapi(client: &reqwest::blocking::Client) -> Result<GeoResult, Box<dyn std::error::Error + Send + Sync>> {
    let r: IpApiResponse = client
        .get("https://ipapi.co/json/")
        .header("User-Agent", "mutsumi-desktop-pet/1.0")
        .send()?
        .error_for_status()?
        .json()?;
    Ok(GeoResult { latitude: r.latitude, longitude: r.longitude, city: r.city })
}

fn fetch_geo_ipinfo(client: &reqwest::blocking::Client) -> Result<GeoResult, Box<dyn std::error::Error + Send + Sync>> {
    let r: IpInfoResponse = client
        .get("https://ipinfo.io/json")
        .header("User-Agent", "mutsumi-desktop-pet/1.0")
        .send()?
        .error_for_status()?
        .json()?;
    // `loc` is "lat,lon" — e.g. "39.9075,116.3972"
    let mut parts = r.loc.splitn(2, ',');
    let lat: f64 = parts.next().ok_or("ipinfo.io: missing lat in loc")?.trim().parse()?;
    let lon: f64 = parts.next().ok_or("ipinfo.io: missing lon in loc")?.trim().parse()?;
    Ok(GeoResult { latitude: lat, longitude: lon, city: r.city })
}

fn fetch_once(client: &reqwest::blocking::Client) -> Result<WeatherUpdate, Box<dyn std::error::Error + Send + Sync>> {
    // 1. Approximate location from IP (fallback chain: ipapi.co → ipinfo.io).
    let geo = fetch_geo(client)?;

    // 2. Current weather from Open-Meteo (no API key, Cloudflare CDN — global).
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

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_update(code: u32) -> WeatherUpdate {
        WeatherUpdate { code, label: "Clear".into(), temp_c: 20.0, city: None }
    }

    // ── WeatherState availability state machine ────────────────────

    #[test]
    fn starts_unavailable() {
        let s = WeatherState::new();
        assert!(!s.is_available());
    }

    #[test]
    fn first_success_makes_available_and_reports_change() {
        let s = WeatherState::new();
        let changed = s.record_success(make_update(0));
        assert!(s.is_available());
        assert!(changed, "first success should report a status change");
    }

    #[test]
    fn repeated_success_does_not_report_change() {
        let s = WeatherState::new();
        s.record_success(make_update(0));
        let changed = s.record_success(make_update(1));
        assert!(!changed, "already-available → success should not re-emit");
    }

    #[test]
    fn success_stores_latest_update() {
        let s = WeatherState::new();
        s.record_success(make_update(63));
        assert_eq!(s.update.lock().unwrap().as_ref().unwrap().code, 63);
    }

    #[test]
    fn failures_below_threshold_do_not_change_availability() {
        let s = WeatherState::new();
        s.record_success(make_update(0)); // make available first
        for _ in 0..(MAX_FAILURES - 1) {
            let changed = s.record_failure();
            assert!(!changed);
            assert!(s.is_available(), "still available below threshold");
        }
    }

    #[test]
    fn reaching_max_failures_makes_unavailable_and_reports_change() {
        let s = WeatherState::new();
        s.record_success(make_update(0));
        for _ in 0..(MAX_FAILURES - 1) {
            s.record_failure();
        }
        let changed = s.record_failure(); // this is the MAX_FAILURES-th failure
        assert!(!s.is_available());
        assert!(changed, "crossing threshold should report a status change");
    }

    #[test]
    fn failure_when_already_unavailable_does_not_report_change() {
        let s = WeatherState::new();
        // Never succeeded → available is already false
        for _ in 0..MAX_FAILURES {
            s.record_failure();
        }
        let changed = s.record_failure(); // extra failure on already-unavailable state
        assert!(!changed);
    }

    #[test]
    fn success_after_failures_restores_availability_and_reports_change() {
        let s = WeatherState::new();
        s.record_success(make_update(0));
        for _ in 0..MAX_FAILURES { s.record_failure(); }
        assert!(!s.is_available());
        let changed = s.record_success(make_update(1));
        assert!(s.is_available());
        assert!(changed, "recovery should report a status change");
    }

    #[test]
    fn success_resets_consecutive_failure_counter() {
        let s = WeatherState::new();
        s.record_success(make_update(0));
        // Two failures — below threshold
        s.record_failure();
        s.record_failure();
        // Success resets the counter
        s.record_success(make_update(1));
        // Now MAX_FAILURES more failures should be needed to flip again
        for _ in 0..(MAX_FAILURES - 1) {
            let changed = s.record_failure();
            assert!(!changed);
        }
        let changed = s.record_failure();
        assert!(changed, "counter was reset; MAX_FAILURES failures needed again");
    }

    // ── weather_emoji mapping (existing tests kept) ────────────────

    #[test]
    fn emoji_clear() { assert_eq!(weather_emoji(0), ("☀️", "Clear")); }

    #[test]
    fn emoji_partly_cloudy() { assert_eq!(weather_emoji(1).1, "Partly cloudy"); }

    #[test]
    fn emoji_cloudy() { assert_eq!(weather_emoji(3).1, "Cloudy"); }

    #[test]
    fn emoji_fog() { assert_eq!(weather_emoji(45).1, "Foggy"); }

    #[test]
    fn emoji_drizzle() { assert_eq!(weather_emoji(51).1, "Drizzle"); }

    #[test]
    fn emoji_rain() { assert_eq!(weather_emoji(61).1, "Rain"); }

    #[test]
    fn emoji_snow() { assert_eq!(weather_emoji(71).1, "Snow"); }

    #[test]
    fn emoji_showers() { assert_eq!(weather_emoji(80).1, "Rain showers"); }

    #[test]
    fn emoji_thunderstorm() { assert_eq!(weather_emoji(95).1, "Thunderstorm"); }

    #[test]
    fn emoji_thunderstorm_hail() { assert_eq!(weather_emoji(96).1, "Thunderstorm with hail"); }

    #[test]
    fn emoji_unknown() { assert_eq!(weather_emoji(9999).1, "Unknown"); }

    #[test]
    fn emoji_gap_codes_are_unknown() {
        for code in [4, 50, 58, 70, 79, 90, 97] {
            assert_eq!(weather_emoji(code).1, "Unknown", "code {code} should be Unknown");
        }
    }

    // ── WeatherState cache (existing tests kept) ───────────────────

    #[test]
    fn weather_state_starts_empty() {
        assert!(WeatherState::new().update.lock().unwrap().is_none());
    }

    #[test]
    fn weather_state_stores_and_retrieves_update() {
        let s = WeatherState::new();
        s.record_success(WeatherUpdate {
            code: 63, label: "Rain".into(), temp_c: 14.5, city: Some("Tokyo".into()),
        });
        let stored = s.update.lock().unwrap().clone().unwrap();
        assert_eq!(stored.code,   63);
        assert_eq!(stored.label,  "Rain");
        assert_eq!(stored.temp_c, 14.5);
        assert_eq!(stored.city.as_deref(), Some("Tokyo"));
    }

    #[test]
    fn weather_state_overwrites_previous_update() {
        let s = WeatherState::new();
        s.record_success(WeatherUpdate { code: 0,  label: "Clear".into(),       temp_c: 25.0, city: None });
        s.record_success(WeatherUpdate { code: 95, label: "Thunderstorm".into(), temp_c: 18.0, city: None });
        assert_eq!(s.update.lock().unwrap().as_ref().unwrap().code, 95);
    }
}
