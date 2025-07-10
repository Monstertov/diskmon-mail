pub mod disk_health;
pub use disk_health::get_smart_status;

pub fn is_virtualized() -> bool {
    use winapi::um::sysinfoapi;
    let mut system_info: sysinfoapi::SYSTEM_INFO = unsafe { std::mem::zeroed() };
    unsafe { sysinfoapi::GetSystemInfo(&mut system_info) };
    // This is a simple check, a more robust solution would check for specific vendor IDs
    system_info.dwNumberOfProcessors < 2
} 