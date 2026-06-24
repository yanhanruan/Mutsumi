//! One-shot hardware specification query (CPU / RAM / GPU / storage).
//!
//! Unlike [`crate::sys_state`] — a 1 Hz live *usage* stream — this is fetched on
//! demand when the System State overlay opens. Hardware specs are essentially
//! static, so a single `invoke` is far cheaper than polling them every second.
//!
//! Sources:
//! * CPU / RAM / partitions — `sysinfo` (portable).
//! * GPU name + dedicated VRAM — DXGI `IDXGIFactory1::EnumAdapters1`
//!   (`DedicatedVideoMemory` is accurate, unlike WMI's 4 GB-capped `AdapterRAM`).
//! * Physical drive model / capacity / SSD-vs-HDD — Windows storage IOCTLs.
//!
//! Non-Windows builds still report CPU/RAM/partitions; GPU and physical-drive
//! lists come back empty (the overlay simply omits those rows).

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
        .map_err(|e| format!("hardware query task failed: {e}"))
}

fn collect() -> HardwareInfo {
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

    let gpus = collect_gpus();

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

    let drives = collect_drives();

    HardwareInfo {
        cpu,
        memory,
        gpus,
        storage: StorageInfo { drives, partitions },
    }
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
            let Ok(desc) = adapter.GetDesc1() else { continue };
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

#[cfg(not(windows))]
fn collect_gpus() -> Vec<GpuInfo> {
    Vec::new()
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

#[cfg(not(windows))]
fn collect_drives() -> Vec<DriveInfo> {
    Vec::new()
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
    let model = format!("{vendor} {product}").split_whitespace().collect::<Vec<_>>().join(" ");
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
    let bytes: Vec<u8> = buf[start..].iter().copied().take_while(|&b| b != 0).collect();
    String::from_utf8_lossy(&bytes).trim().to_string()
}
