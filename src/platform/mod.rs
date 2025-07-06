#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "windows")]
pub use windows::*;

#[cfg(target_os = "linux")]
pub use linux::*;

// Common platform traits and structures
#[derive(Debug, Clone)]
pub struct SmartStatus {
    pub status: Option<String>,
    pub serial_number: Option<String>,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub is_raid: bool,
}

pub trait PlatformDiskInfo {
    fn get_smart_status(&self, disk_name: &str, debug: bool) -> SmartStatus;
    fn is_virtualized(&self) -> bool;
}

// Platform-specific implementations
#[cfg(target_os = "windows")]
pub struct WindowsPlatform;

#[cfg(target_os = "linux")]
pub struct LinuxPlatform;

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub struct GenericPlatform;

// Get the appropriate platform implementation
pub fn get_platform() -> Box<dyn PlatformDiskInfo> {
    #[cfg(target_os = "windows")]
    {
        Box::new(WindowsPlatform)
    }
    #[cfg(target_os = "linux")]
    {
        Box::new(LinuxPlatform)
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        Box::new(GenericPlatform)
    }
} 