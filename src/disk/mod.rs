use sysinfo::{Disks, System};
use crate::config::Config;
use crate::platform::{get_platform, PlatformDiskInfo, SmartStatus};

#[derive(Debug, Clone)]
pub struct DiskInfo {
    pub mount_point: String,
    pub display_name: String, // Drive letter for Windows, mount point for Unix
    pub free_space_percent: f64,
    pub total_space: u64,
    pub available_space: u64,
    pub file_system: String,
    pub smart_status: Option<String>,
    pub serial_number: Option<String>,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub is_raid: bool,
}

#[derive(Debug, Clone)]
pub struct SystemInfo {
    pub os_name: String,
    pub os_version: String,
    pub architecture: String,
    pub hostname: String,
    pub is_virtualized: bool,
}

pub fn get_system_info() -> SystemInfo {
    let hostname = hostname::get()
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

    let platform = get_platform();
    let is_virtualized = platform.is_virtualized();

    SystemInfo {
        os_name,
        os_version,
        architecture: architecture.to_string(),
        hostname,
        is_virtualized,
    }
}

pub fn get_monitored_disks(cfg: &Config, debug: bool) -> Vec<DiskInfo> {
    let disks = Disks::new_with_refreshed_list();
    let mut monitored_disks = Vec::new();
    let platform = get_platform();
    
    // Check if health checks are enabled (default to true if not specified)
    let health_check_enabled = cfg.health_check_enabled.unwrap_or(true);

    for disk in disks.list() {
        let mount_point = match disk.mount_point().to_str() {
            Some(path) => path.to_string(),
            None => continue,
        };

        if cfg!(windows) {
            if mount_point.starts_with("\\\\") || mount_point.starts_with("A:") || mount_point.starts_with("B:") {
                continue;
            }
        } else {
            if mount_point.starts_with("/media/") || mount_point.starts_with("/mnt/") || mount_point.starts_with("/run/media/") {
                continue;
            }
        }

        let total = disk.total_space();
        let available = disk.available_space();

        if total == 0 {
            continue;
        }

        let free_space_percent = (available as f64 / total as f64) * 100.0;

        let display_name = if cfg!(windows) {
            if mount_point.len() >= 2 && mount_point.chars().nth(1) == Some(':') {
                format!("Drive {}", mount_point.chars().nth(0).unwrap().to_uppercase())
            } else {
                mount_point.clone()
            }
        } else {
            mount_point.clone()
        };

        let file_system = disk.file_system().to_str().unwrap_or("Unknown").to_string();

        // Only perform health checks if enabled
        let smart_status_result = if health_check_enabled {
            // For Windows, pass the mount point (drive letter) instead of disk name
            let smart_input = if cfg!(windows) {
                &mount_point
            } else {
                disk.name().to_str().unwrap_or("")
            };
            platform.get_smart_status(smart_input, debug)
        } else {
            SmartStatus {
                status: None,
                serial_number: None,
                brand: None,
                model: None,
                is_raid: false,
            }
        };

        monitored_disks.push(DiskInfo {
            mount_point,
            display_name,
            free_space_percent,
            total_space: total,
            available_space: available,
            file_system,
            smart_status: smart_status_result.status,
            serial_number: smart_status_result.serial_number,
            brand: smart_status_result.brand,
            model: smart_status_result.model,
            is_raid: smart_status_result.is_raid,
        });
    }

    monitored_disks
} 