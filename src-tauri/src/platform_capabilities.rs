//! Runtime platform capability contract exposed to the frontend.
//!
//! Platform adapters must report their real state here so the UI never renders
//! an enabled control backed by a silent no-op. This first slice describes the
//! existing Windows implementation and the current macOS compile baseline.

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub enum CapabilityStatus {
    Available,
    PermissionRequired,
    Unavailable,
    Degraded,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformCapabilities {
    pub audio_activity: CapabilityStatus,
    pub media_metadata: CapabilityStatus,
    pub media_transport: CapabilityStatus,
    pub system_volume: CapabilityStatus,
    pub idle_detection: CapabilityStatus,
    pub global_cursor: CapabilityStatus,
    pub atomic_window_geometry: CapabilityStatus,
    pub deep_web_search: CapabilityStatus,
    pub hardware_details: CapabilityStatus,
    pub reveal_in_folder: CapabilityStatus,
}

#[tauri::command]
pub fn get_platform_capabilities() -> PlatformCapabilities {
    current()
}

#[cfg(windows)]
fn current() -> PlatformCapabilities {
    use CapabilityStatus::Available;

    PlatformCapabilities {
        audio_activity: Available,
        media_metadata: Available,
        media_transport: Available,
        system_volume: Available,
        idle_detection: Available,
        global_cursor: Available,
        atomic_window_geometry: Available,
        deep_web_search: Available,
        hardware_details: Available,
        reveal_in_folder: Available,
    }
}

#[cfg(target_os = "macos")]
fn current() -> PlatformCapabilities {
    use CapabilityStatus::{Available, Degraded, Unavailable};

    PlatformCapabilities {
        audio_activity: Unavailable,
        media_metadata: Unavailable,
        media_transport: Unavailable,
        system_volume: Unavailable,
        // CoreGraphics supplies system-wide input idle time; IOKit supplies
        // aggregate PreventUserIdleDisplaySleep power-assertion state.
        idle_detection: Available,
        global_cursor: Available,
        atomic_window_geometry: Available,
        // Rendered SERPs work through the initialization-script event bridge;
        // only arbitrary result-page deepening is unavailable in Phase 1.
        deep_web_search: Degraded,
        // sysinfo provides the portable subset; GPU and physical drive details
        // still use Windows-specific implementations.
        hardware_details: Degraded,
        reveal_in_folder: Available,
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn current() -> PlatformCapabilities {
    use CapabilityStatus::Unavailable;

    PlatformCapabilities {
        audio_activity: Unavailable,
        media_metadata: Unavailable,
        media_transport: Unavailable,
        system_volume: Unavailable,
        idle_detection: Unavailable,
        global_cursor: Unavailable,
        atomic_window_geometry: Unavailable,
        deep_web_search: Unavailable,
        hardware_details: Unavailable,
        reveal_in_folder: Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialized_contract_uses_frontend_field_names() {
        let value = serde_json::to_value(current()).unwrap();
        assert!(value.get("audioActivity").is_some());
        assert!(value.get("mediaTransport").is_some());
        assert!(value.get("atomicWindowGeometry").is_some());
        assert!(value.get("deepWebSearch").is_some());
        assert!(value.get("revealInFolder").is_some());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_baseline_reports_available_degraded_and_unavailable_features() {
        let capabilities = current();
        assert_eq!(capabilities.audio_activity, CapabilityStatus::Unavailable);
        assert_eq!(capabilities.idle_detection, CapabilityStatus::Available);
        assert_eq!(capabilities.global_cursor, CapabilityStatus::Available);
        assert_eq!(
            capabilities.atomic_window_geometry,
            CapabilityStatus::Available
        );
        assert_eq!(capabilities.deep_web_search, CapabilityStatus::Degraded);
        assert_eq!(capabilities.hardware_details, CapabilityStatus::Degraded);
        assert_eq!(capabilities.reveal_in_folder, CapabilityStatus::Available);
    }
}
