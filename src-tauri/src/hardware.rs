//! One-shot hardware specification query (CPU / RAM / GPU / storage).
//!
//! Unlike [`crate::sys_state`] — a 1 Hz live *usage* stream — this is fetched on
//! demand when the System State overlay opens. Hardware specs are essentially
//! static, so a single `invoke` is far cheaper than polling them every second.
//!
//! Sources:
//! * CPU / RAM / partitions — `sysinfo` (portable).
//! * GPU name + dedicated VRAM — DXGI `IDXGIFactory1::EnumAdapters1` on Windows;
//!   structured `system_profiler` JSON on macOS.
//! * Physical drive model / capacity / SSD-vs-HDD — Windows storage IOCTLs;
//!   structured `diskutil` property lists on macOS.

use serde::Serialize;

#[derive(Serialize)]
pub struct CpuInfo {
    pub model: String,
    /// Physical cores.
    pub cores: u32,
    /// Logical processors (threads).
    pub threads: u32,
    pub frequency_mhz: u64,
}

#[derive(Serialize)]
pub struct MemoryInfo {
    pub total: u64,
    pub available: u64,
    pub used: u64,
}

#[derive(Serialize)]
pub struct GpuInfo {
    pub name: String,
    /// Dedicated video memory in bytes (0 when unknown / shared-only).
    pub vram: u64,
}

/// A mounted volume / partition (drive letter on Windows).
#[derive(Serialize)]
pub struct PartitionInfo {
    pub mount_point: String,
    pub file_system: String,
    pub total: u64,
    pub available: u64,
}

/// A physical disk drive.
#[derive(Serialize)]
pub struct DriveInfo {
    pub model: String,
    /// `"ssd"` | `"hdd"` | `"unknown"`.
    pub kind: String,
    pub total: u64,
}

#[derive(Serialize)]
pub struct StorageInfo {
    pub drives: Vec<DriveInfo>,
    pub partitions: Vec<PartitionInfo>,
}

#[derive(Serialize)]
pub struct HardwareInfo {
    pub cpu: CpuInfo,
    pub memory: MemoryInfo,
    pub gpus: Vec<GpuInfo>,
    pub storage: StorageInfo,
}

/// Gathers a full hardware snapshot off the async runtime so the (blocking)
/// `sysinfo` refresh and storage IOCTLs never stall the UI thread.
#[tauri::command]
pub async fn get_hardware_info() -> Result<HardwareInfo, String> {
    tauri::async_runtime::spawn_blocking(collect)
        .await
        .map_err(|e| format!("hardware query task failed: {e}"))?
}

fn collect() -> Result<HardwareInfo, String> {
    use sysinfo::{Disks, System};

    let mut sys = System::new();
    sys.refresh_cpu_all();
    sys.refresh_memory();

    let cpus = sys.cpus();
    let model = cpus
        .first()
        .map(|c| c.brand().trim().to_string())
        .unwrap_or_default();
    let frequency_mhz = cpus.first().map(|c| c.frequency()).unwrap_or(0);
    let threads = cpus.len() as u32;
    // `physical_core_count` is an associated fn in sysinfo 0.36 (recomputed on
    // call); fall back to the logical count if the platform can't report it.
    let cores = System::physical_core_count()
        .map(|c| c as u32)
        .filter(|&c| c > 0)
        .unwrap_or(threads);

    let cpu = CpuInfo {
        model,
        cores,
        threads,
        frequency_mhz,
    };

    let memory = MemoryInfo {
        total: sys.total_memory(),
        available: sys.available_memory(),
        used: sys.used_memory(),
    };

    #[cfg(windows)]
    let gpus = collect_gpus();
    #[cfg(target_os = "macos")]
    let gpus = macos::collect_gpus()?;
    #[cfg(not(any(windows, target_os = "macos")))]
    let gpus = Vec::new();

    let disks = Disks::new_with_refreshed_list();
    let partitions = disks
        .list()
        .iter()
        .map(|d| PartitionInfo {
            mount_point: d.mount_point().to_string_lossy().into_owned(),
            file_system: d.file_system().to_string_lossy().into_owned(),
            total: d.total_space(),
            available: d.available_space(),
        })
        .collect();

    #[cfg(windows)]
    let drives = collect_drives();
    #[cfg(target_os = "macos")]
    let drives = macos::collect_drives()?;
    #[cfg(not(any(windows, target_os = "macos")))]
    let drives = Vec::new();

    Ok(HardwareInfo {
        cpu,
        memory,
        gpus,
        storage: StorageInfo { drives, partitions },
    })
}

// ── GPU enumeration (DXGI) ──────────────────────────────────────────────────

#[cfg(windows)]
fn collect_gpus() -> Vec<GpuInfo> {
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE,
    };

    let mut gpus = Vec::new();
    unsafe {
        let factory: IDXGIFactory1 = match CreateDXGIFactory1() {
            Ok(f) => f,
            Err(_) => return gpus,
        };
        let mut i = 0u32;
        while let Ok(adapter) = factory.EnumAdapters1(i) {
            i += 1;
            let Ok(desc) = adapter.GetDesc1() else {
                continue;
            };
            // Skip the Microsoft Basic Render Driver (WARP software adapter).
            if (desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0 {
                continue;
            }
            let end = desc
                .Description
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(desc.Description.len());
            let name = String::from_utf16_lossy(&desc.Description[..end])
                .trim()
                .to_string();
            if name.is_empty() {
                continue;
            }
            gpus.push(GpuInfo {
                name,
                vram: desc.DedicatedVideoMemory as u64,
            });
        }
    }
    gpus
}

// ── Physical drive enumeration (Windows storage IOCTLs) ─────────────────────

#[cfg(windows)]
fn collect_drives() -> Vec<DriveInfo> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    let mut drives = Vec::new();
    // Drive numbers are normally contiguous from 0; probe a small fixed range
    // and skip any gaps rather than stopping at the first miss.
    for n in 0..16u32 {
        let path: Vec<u16> = format!("\\\\.\\PhysicalDrive{n}")
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        unsafe {
            // Zero desired-access is enough for the read-only property/geometry
            // IOCTLs and avoids needing administrator rights.
            let handle: HANDLE = match CreateFileW(
                PCWSTR(path.as_ptr()),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            ) {
                Ok(h) if !h.is_invalid() => h,
                _ => continue,
            };

            let model = query_model(handle).unwrap_or_else(|| format!("Disk {n}"));
            let kind = query_kind(handle);
            let total = query_capacity(handle).unwrap_or(0);

            let _ = CloseHandle(handle);

            // Ignore phantom entries that report nothing useful.
            if total == 0 && model.starts_with("Disk ") {
                continue;
            }
            drives.push(DriveInfo { model, kind, total });
        }
    }
    drives
}

/// Read the vendor + product id strings from `STORAGE_DEVICE_DESCRIPTOR`.
#[cfg(windows)]
unsafe fn query_model(handle: windows::Win32::Foundation::HANDLE) -> Option<String> {
    use windows::Win32::System::Ioctl::{
        PropertyStandardQuery, StorageDeviceProperty, IOCTL_STORAGE_QUERY_PROPERTY,
        STORAGE_DEVICE_DESCRIPTOR, STORAGE_PROPERTY_QUERY,
    };

    let query = STORAGE_PROPERTY_QUERY {
        PropertyId: StorageDeviceProperty,
        QueryType: PropertyStandardQuery,
        AdditionalParameters: [0],
    };
    let mut buf = [0u8; 1024];
    device_io(
        handle,
        IOCTL_STORAGE_QUERY_PROPERTY,
        Some((&query as *const STORAGE_PROPERTY_QUERY).cast()),
        std::mem::size_of::<STORAGE_PROPERTY_QUERY>() as u32,
        &mut buf,
    )?;

    let desc = &*(buf.as_ptr() as *const STORAGE_DEVICE_DESCRIPTOR);
    let vendor = read_ansi(&buf, desc.VendorIdOffset);
    let product = read_ansi(&buf, desc.ProductIdOffset);
    let model = format!("{vendor} {product}")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if model.is_empty() {
        None
    } else {
        Some(model)
    }
}

/// `false` seek-penalty ⇒ solid-state.
#[cfg(windows)]
unsafe fn query_kind(handle: windows::Win32::Foundation::HANDLE) -> String {
    use windows::Win32::System::Ioctl::{
        PropertyStandardQuery, StorageDeviceSeekPenaltyProperty, DEVICE_SEEK_PENALTY_DESCRIPTOR,
        IOCTL_STORAGE_QUERY_PROPERTY, STORAGE_PROPERTY_QUERY,
    };

    let query = STORAGE_PROPERTY_QUERY {
        PropertyId: StorageDeviceSeekPenaltyProperty,
        QueryType: PropertyStandardQuery,
        AdditionalParameters: [0],
    };
    let mut desc = DEVICE_SEEK_PENALTY_DESCRIPTOR::default();
    let out = std::slice::from_raw_parts_mut(
        (&mut desc as *mut DEVICE_SEEK_PENALTY_DESCRIPTOR).cast::<u8>(),
        std::mem::size_of::<DEVICE_SEEK_PENALTY_DESCRIPTOR>(),
    );
    if device_io(
        handle,
        IOCTL_STORAGE_QUERY_PROPERTY,
        Some((&query as *const STORAGE_PROPERTY_QUERY).cast()),
        std::mem::size_of::<STORAGE_PROPERTY_QUERY>() as u32,
        out,
    )
    .is_none()
    {
        return "unknown".into();
    }
    if desc.IncursSeekPenalty {
        "hdd".into()
    } else {
        "ssd".into()
    }
}

/// Total physical capacity in bytes via `IOCTL_DISK_GET_DRIVE_GEOMETRY_EX`.
#[cfg(windows)]
unsafe fn query_capacity(handle: windows::Win32::Foundation::HANDLE) -> Option<u64> {
    use windows::Win32::System::Ioctl::{DISK_GEOMETRY_EX, IOCTL_DISK_GET_DRIVE_GEOMETRY_EX};

    // The trailing variable-length partition data can follow `DiskSize`, so give
    // the driver generous room; only the fixed prefix is read back.
    let mut buf = [0u8; 512];
    device_io(handle, IOCTL_DISK_GET_DRIVE_GEOMETRY_EX, None, 0, &mut buf)?;
    let geo = &*(buf.as_ptr() as *const DISK_GEOMETRY_EX);
    (geo.DiskSize > 0).then_some(geo.DiskSize as u64)
}

/// Thin `DeviceIoControl` wrapper: `Some(())` on success, `None` on failure.
#[cfg(windows)]
unsafe fn device_io(
    handle: windows::Win32::Foundation::HANDLE,
    code: u32,
    in_ptr: Option<*const std::ffi::c_void>,
    in_len: u32,
    out: &mut [u8],
) -> Option<()> {
    use windows::Win32::System::IO::DeviceIoControl;

    let mut returned = 0u32;
    DeviceIoControl(
        handle,
        code,
        in_ptr,
        in_len,
        Some(out.as_mut_ptr().cast()),
        out.len() as u32,
        Some(&mut returned),
        None,
    )
    .ok()
}

/// Null-terminated ANSI string at `offset` within `buf` (0 ⇒ absent).
#[cfg(windows)]
fn read_ansi(buf: &[u8], offset: u32) -> String {
    let start = offset as usize;
    if offset == 0 || start >= buf.len() {
        return String::new();
    }
    let bytes: Vec<u8> = buf[start..]
        .iter()
        .copied()
        .take_while(|&b| b != 0)
        .collect();
    String::from_utf8_lossy(&bytes).trim().to_string()
}

// ── macOS structured system queries ────────────────────────────────────────

#[cfg(target_os = "macos")]
mod macos {
    use super::{DriveInfo, GpuInfo};
    use serde::Deserialize;
    use serde_json::Value;
    use std::collections::HashSet;
    use std::process::{Command, Output};

    const SYSTEM_PROFILER: &str = "/usr/sbin/system_profiler";
    const DISKUTIL: &str = "/usr/sbin/diskutil";

    fn run(command: &str, args: &[&str]) -> Result<Output, String> {
        let output = Command::new(command)
            .args(args)
            .output()
            .map_err(|error| format!("failed to start {command}: {error}"))?;
        if output.status.success() {
            Ok(output)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!(
                "{} exited with {}: {}",
                command,
                output.status,
                stderr.trim()
            ))
        }
    }

    pub(super) fn collect_gpus() -> Result<Vec<GpuInfo>, String> {
        let output = run(
            SYSTEM_PROFILER,
            &["SPDisplaysDataType", "-json", "-detailLevel", "mini"],
        )?;
        parse_gpus(&output.stdout)
    }

    fn parse_gpus(bytes: &[u8]) -> Result<Vec<GpuInfo>, String> {
        let root: Value = serde_json::from_slice(bytes)
            .map_err(|error| format!("invalid system_profiler GPU JSON: {error}"))?;
        let entries = root
            .get("SPDisplaysDataType")
            .and_then(Value::as_array)
            .ok_or_else(|| "system_profiler GPU JSON omitted SPDisplaysDataType".to_string())?;

        let mut seen = HashSet::new();
        let mut gpus = Vec::new();
        for entry in entries {
            let model = entry
                .get("sppci_model")
                .or_else(|| entry.get("_name"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let Some(model) = model else { continue };
            if !seen.insert(model.to_owned()) {
                continue;
            }

            let vram = [
                "spdisplays_vram",
                "spdisplays_vram_shared",
                "spdisplays_vram_dynamic",
            ]
            .iter()
            .filter_map(|key| entry.get(*key).and_then(Value::as_str))
            .find_map(parse_capacity)
            .unwrap_or(0);

            gpus.push(GpuInfo {
                name: model.to_owned(),
                vram,
            });
        }

        if gpus.is_empty() {
            Err("system_profiler returned no GPU entries".into())
        } else {
            Ok(gpus)
        }
    }

    fn parse_capacity(value: &str) -> Option<u64> {
        let mut parts = value.split_whitespace();
        let amount: f64 = parts.next()?.replace(',', ".").parse().ok()?;
        let multiplier = match parts.next()?.to_ascii_uppercase().as_str() {
            "KB" => 1024u64,
            "MB" => 1024u64.pow(2),
            "GB" => 1024u64.pow(3),
            "TB" => 1024u64.pow(4),
            _ => return None,
        };
        (amount.is_finite() && amount >= 0.0).then(|| (amount * multiplier as f64) as u64)
    }

    #[derive(Deserialize)]
    struct DiskList {
        #[serde(rename = "WholeDisks")]
        whole_disks: Vec<String>,
    }

    #[derive(Deserialize)]
    struct DiskInfo {
        #[serde(rename = "MediaName")]
        media_name: Option<String>,
        #[serde(rename = "IORegistryEntryName")]
        registry_name: Option<String>,
        #[serde(rename = "SolidState")]
        solid_state: Option<bool>,
        #[serde(rename = "Size")]
        size: Option<u64>,
        #[serde(rename = "TotalSize")]
        total_size: Option<u64>,
        #[serde(rename = "IOKitSize")]
        io_kit_size: Option<u64>,
    }

    pub(super) fn collect_drives() -> Result<Vec<DriveInfo>, String> {
        let output = run(DISKUTIL, &["list", "-plist", "physical"])?;
        let list: DiskList = plist::from_bytes(&output.stdout)
            .map_err(|error| format!("invalid diskutil physical-disk plist: {error}"))?;
        if list.whole_disks.is_empty() {
            return Err("diskutil returned no physical disks".into());
        }

        list.whole_disks
            .iter()
            .map(|identifier| {
                let output = run(DISKUTIL, &["info", "-plist", identifier])?;
                parse_drive(&output.stdout, identifier)
            })
            .collect()
    }

    fn parse_drive(bytes: &[u8], identifier: &str) -> Result<DriveInfo, String> {
        let info: DiskInfo = plist::from_bytes(bytes)
            .map_err(|error| format!("invalid diskutil info plist for {identifier}: {error}"))?;
        let model = info
            .media_name
            .or(info.registry_name)
            .map(|value| value.trim().trim_end_matches(" Media").trim().to_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| identifier.to_owned());
        let kind = match info.solid_state {
            Some(true) => "ssd",
            Some(false) => "hdd",
            None => "unknown",
        }
        .to_owned();
        let total = info
            .size
            .or(info.total_size)
            .or(info.io_kit_size)
            .unwrap_or(0);
        if total == 0 {
            return Err(format!("diskutil returned no capacity for {identifier}"));
        }

        Ok(DriveInfo { model, kind, total })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn parses_apple_silicon_and_intel_gpu_entries() {
            let json = br#"{
              "SPDisplaysDataType": [
                {"_name":"Apple M5","sppci_model":"Apple M5","sppci_cores":"10"},
                {"_name":"AMD Radeon Pro","sppci_model":"AMD Radeon Pro 5500M","spdisplays_vram":"8 GB"}
              ]
            }"#;

            let gpus = parse_gpus(json).unwrap();
            assert_eq!(gpus.len(), 2);
            assert_eq!(gpus[0].name, "Apple M5");
            assert_eq!(gpus[0].vram, 0);
            assert_eq!(gpus[1].name, "AMD Radeon Pro 5500M");
            assert_eq!(gpus[1].vram, 8 * 1024u64.pow(3));
        }

        #[test]
        fn rejects_missing_gpu_section_instead_of_silently_returning_empty() {
            assert!(parse_gpus(br#"{}"#).is_err());
        }

        #[test]
        fn parses_disk_model_capacity_and_kind() {
            let plist = br#"<?xml version="1.0" encoding="UTF-8"?>
              <plist version="1.0"><dict>
                <key>MediaName</key><string>APPLE SSD AP1024Z</string>
                <key>SolidState</key><true/>
                <key>Size</key><integer>1000555581440</integer>
              </dict></plist>"#;

            let drive = parse_drive(plist, "disk0").unwrap();
            assert_eq!(drive.model, "APPLE SSD AP1024Z");
            assert_eq!(drive.kind, "ssd");
            assert_eq!(drive.total, 1_000_555_581_440);
        }

        #[test]
        fn rejects_disk_without_capacity() {
            let plist = br#"<?xml version="1.0" encoding="UTF-8"?>
              <plist version="1.0"><dict>
                <key>MediaName</key><string>Unknown Disk</string>
              </dict></plist>"#;
            assert!(parse_drive(plist, "disk9").is_err());
        }

        #[test]
        #[ignore = "live macOS system_profiler and diskutil smoke test"]
        fn live_collects_at_least_one_gpu_and_physical_drive() {
            let gpus = collect_gpus().expect("GPU query");
            let drives = collect_drives().expect("physical-drive query");
            assert!(!gpus.is_empty());
            assert!(gpus.iter().all(|gpu| !gpu.name.trim().is_empty()));
            assert!(!drives.is_empty());
            assert!(drives.iter().all(|drive| drive.total > 0));
        }
    }
}
