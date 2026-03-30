pub mod disk_health;
pub use disk_health::get_smart_status;

pub fn is_virtualized() -> bool {
    use winapi::um::sysinfoapi;
    // SAFETY: SYSTEM_INFO is a plain C struct containing only integer and pointer-sized
    // fields with no non-zero validity requirements (no references, no NonZero* types).
    // Zero-initializing it produces a valid, if uninhabited, starting state.
    let mut system_info: sysinfoapi::SYSTEM_INFO = unsafe { std::mem::zeroed() };
    // SAFETY: `system_info` is stack-allocated, properly aligned, and points to writable
    // memory for the full size of SYSTEM_INFO. GetSystemInfo fills it in-place and does
    // not retain the pointer after returning.
    unsafe { sysinfoapi::GetSystemInfo(&mut system_info) };
    // Note: processor count < 2 is a heuristic only. A VM with multiple vCPUs will not
    // be detected, and a single-core physical machine will be incorrectly flagged.
    system_info.dwNumberOfProcessors < 2
} 