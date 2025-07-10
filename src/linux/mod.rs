pub mod disk_health;
pub use disk_health::get_smart_status;

pub fn is_virtualized() -> bool {
    if let Ok(cpuinfo) = std::fs::read_to_string("/proc/cpuinfo") {
        if cpuinfo.contains("hypervisor") {
            return true;
        }
    }
    false
} 