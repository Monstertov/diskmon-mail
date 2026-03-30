use hostname::get as get_hostname;
use sysinfo::System;

/// Disk health and identity information collected via SMART tools or OS fallbacks.
#[derive(Debug)]
pub struct SmartInfo {
    pub smart_status: Option<String>,
    pub serial_number: Option<String>,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub is_raid: bool,
    pub power_on_hours: Option<u64>,
    pub reallocated_sectors: Option<u64>,
    pub temperature: Option<i64>,
    pub pending_sectors: Option<u64>,
    pub uncorrectable_sectors: Option<u64>,
    pub health_method: String,
}

impl SmartInfo {
    /// Returns a SmartInfo with no data and the given health_method label.
    pub fn unknown(health_method: impl Into<String>) -> Self {
        SmartInfo {
            smart_status: None,
            serial_number: None,
            brand: None,
            model: None,
            is_raid: false,
            power_on_hours: None,
            reallocated_sectors: None,
            temperature: None,
            pending_sectors: None,
            uncorrectable_sectors: None,
            health_method: health_method.into(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SystemInfo {
    pub os_name: String,
    pub os_version: String,
    pub architecture: String,
    pub hostname: String,
    pub is_virtualized: bool,
}

pub fn get_system_info() -> SystemInfo {
    let hostname = get_hostname()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "unknown".to_string());
    
    let os_name = System::name().unwrap_or_else(|| "Unknown OS".to_string());
    let os_version = System::os_version().unwrap_or_else(|| "Unknown Version".to_string());
    let architecture = if cfg!(target_arch = "x86_64") {
        "64-bit"
    } else if cfg!(target_arch = "x86") {
        "32-bit"
    } else if cfg!(target_arch = "aarch64") {
        "ARM64"
    } else if cfg!(target_arch = "arm") {
        "ARM32"
    } else {
        "Unknown"
    };

    let is_virtualized = get_is_virtualized();

    SystemInfo {
        os_name,
        os_version,
        architecture: architecture.to_string(),
        hostname,
        is_virtualized,
    }
}

#[cfg(target_os = "linux")]
pub fn get_is_virtualized() -> bool {
    crate::linux::is_virtualized()
}

#[cfg(target_os = "windows")]
pub fn get_is_virtualized() -> bool {
    crate::windows::is_virtualized()
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn get_is_virtualized() -> bool {
    false
}

pub fn get_smart_status(disk_name: &str, debug: bool) -> SmartInfo {
    #[cfg(target_os = "linux")]
    {
        return crate::linux::get_smart_status(disk_name, debug);
    }
    #[cfg(target_os = "windows")]
    {
        return crate::windows::get_smart_status(disk_name, debug);
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        SmartInfo::unknown("unknown")
    }
}
