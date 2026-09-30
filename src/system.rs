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

/// True when a smartctl exit status means its output can be used. smartctl sets bits 0-1 for
/// command line and device open errors; higher bits (for example 8 = disk failing) still come with
/// valid output, so they must not be discarded.
pub fn smartctl_output_usable(status: std::process::ExitStatus) -> bool {
    status.code().is_some_and(|code| code & 0b11 == 0)
}

/// Parses `smartctl -H -i -A` output (ATA, NVMe and SCSI). Returns None when it holds neither a
/// health result nor device identity.
pub fn parse_smartctl(output: &str) -> Option<SmartInfo> {
    let mut info = SmartInfo::unknown("smartmontools");
    for line in output.lines().map(str::trim) {
        let (key, value) = line.split_once(':').map_or((line, ""), |(k, v)| (k.trim(), v.trim()));

        if line.contains("SMART overall-health self-assessment test result:") {
            let status = if line.contains("PASSED") {
                "OK"
            } else if line.contains("FAILED") {
                "FAILING"
            } else {
                "WARNING"
            };
            info.smart_status = Some(status.to_string());
        } else if line.contains("SMART Health Status:") {
            info.smart_status = Some(if value.starts_with("OK") { "OK" } else { "WARNING" }.to_string());
        }

        match key {
            "Device Model" | "Model Number" | "Device" | "Product" => info.model = Some(value.to_string()),
            "Serial Number" | "Serial number" => info.serial_number = Some(value.to_string()),
            "Vendor" => info.brand = Some(value.to_string()),
            // NVMe health log
            "Temperature" => info.temperature = leading_number(value).map(|v| v as i64),
            "Power On Hours" => info.power_on_hours = leading_number(value),
            _ => {}
        }

        // ATA attribute table: ID NAME FLAG VALUE WORST THRESH TYPE UPDATED WHEN_FAILED RAW_VALUE
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() >= 10 && cols[0].parse::<u16>().is_ok() {
            let raw = leading_number(cols[9]);
            match cols[1] {
                "Power_On_Hours" => info.power_on_hours = raw,
                "Reallocated_Sector_Ct" => info.reallocated_sectors = raw,
                "Temperature_Celsius" => info.temperature = raw.map(|v| v as i64),
                "Current_Pending_Sector" => info.pending_sectors = raw,
                "Offline_Uncorrectable" => info.uncorrectable_sectors = raw,
                _ => {}
            }
        }
    }

    if info.smart_status.is_none() && info.model.is_none() && info.serial_number.is_none() {
        return None;
    }
    // Device identity without a health result: assume OK (unchanged behavior).
    info.smart_status.get_or_insert_with(|| "OK".to_string());
    Some(info)
}

/// Digits at the start of a value, ignoring thousands separators: "1,234" -> 1234, "24562h+12m" -> 24562.
fn leading_number(value: &str) -> Option<u64> {
    value
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == ',')
        .filter(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .ok()
}

/// Whole-disk name for a partition name: sda1 -> sda, vdb2 -> vdb, nvme0n1p1 -> nvme0n1,
/// mmcblk0p1 -> mmcblk0. Names that are not partitions come back unchanged.
pub fn parent_disk(name: &str) -> &str {
    let trimmed = name.trim_end_matches(|c: char| c.is_ascii_digit());
    if trimmed.len() == name.len() {
        return name;
    }
    // <name ending in a digit>p<N>: nvme0n1p1, mmcblk0p1, md0p1, loop0p1
    if let Some(base) = trimmed.strip_suffix('p').filter(|b| b.ends_with(|c: char| c.is_ascii_digit())) {
        return base;
    }
    // sdXN, hdXN, vdXN, xvdXN
    if ["sd", "hd", "vd", "xvd"].iter().any(|p| name.starts_with(p)) {
        return trimmed;
    }
    name
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_disk_names() {
        for (part, disk) in [
            ("sda1", "sda"), ("sdb12", "sdb"), ("vda15", "vda"), ("xvda1", "xvda"), ("hda2", "hda"),
            ("nvme0n1p1", "nvme0n1"), ("mmcblk0p1", "mmcblk0"), ("mmcblk1p2", "mmcblk1"), ("md0p1", "md0"),
            ("sda", "sda"), ("nvme0n1", "nvme0n1"), ("mmcblk0", "mmcblk0"), ("dm-0", "dm-0"), ("md0", "md0"),
            ("vg-root", "vg-root"), ("", ""),
        ] {
            assert_eq!(parent_disk(part), disk, "{part}");
        }
    }

    const ATA_FAILING: &str = "\
=== START OF INFORMATION SECTION ===
Model Family:     Western Digital Blue
Device Model:     WDC WD10EZEX-08WN4A0
Serial Number:    WD-WCC6Y0XXXXXX
=== START OF READ SMART DATA SECTION ===
SMART overall-health self-assessment test result: FAILED!
ID# ATTRIBUTE_NAME          FLAG     VALUE WORST THRESH TYPE      UPDATED  WHEN_FAILED RAW_VALUE
  5 Reallocated_Sector_Ct   0x0033   001   001   140    Pre-fail  Always   FAILING_NOW 2008
  9 Power_On_Hours          0x0032   057   057   000    Old_age   Always       -       31712h+05m+12.345s
194 Temperature_Celsius     0x0022   112   099   000    Old_age   Always       -       38 (Min/Max 18/51)
197 Current_Pending_Sector  0x0032   200   200   000    Old_age   Always       -       12
198 Offline_Uncorrectable   0x0030   200   200   000    Old_age   Offline      -       3
";

    const NVME_OK: &str = "\
Model Number:                       Samsung SSD 970 EVO Plus 1TB
Serial Number:                      S4EWNX0XXXXXXX
SMART overall-health self-assessment test result: PASSED
Temperature:                        41 Celsius
Power On Hours:                     12,345
";

    #[test]
    fn parses_ata_attributes_and_failure() {
        let info = parse_smartctl(ATA_FAILING).unwrap();
        assert_eq!(info.smart_status.as_deref(), Some("FAILING"));
        assert_eq!(info.model.as_deref(), Some("WDC WD10EZEX-08WN4A0"));
        assert_eq!(info.serial_number.as_deref(), Some("WD-WCC6Y0XXXXXX"));
        assert_eq!(info.reallocated_sectors, Some(2008));
        assert_eq!(info.power_on_hours, Some(31712));
        assert_eq!(info.temperature, Some(38));
        assert_eq!(info.pending_sectors, Some(12));
        assert_eq!(info.uncorrectable_sectors, Some(3));
        assert_eq!(info.health_method, "smartmontools");
    }

    #[test]
    fn parses_nvme() {
        let info = parse_smartctl(NVME_OK).unwrap();
        assert_eq!(info.smart_status.as_deref(), Some("OK"));
        assert_eq!(info.model.as_deref(), Some("Samsung SSD 970 EVO Plus 1TB"));
        assert_eq!(info.temperature, Some(41));
        assert_eq!(info.power_on_hours, Some(12345));
    }

    #[test]
    fn identity_only_assumes_ok_and_empty_is_none() {
        let info = parse_smartctl("Device Model: Foo\nSerial Number: 123").unwrap();
        assert_eq!(info.smart_status.as_deref(), Some("OK"));
        assert!(parse_smartctl("smartctl 7.3\n/dev/vda: Unable to detect device type").is_none());
        assert_eq!(parse_smartctl("SMART Health Status: OK").unwrap().smart_status.as_deref(), Some("OK"));
        assert_eq!(parse_smartctl("SMART Health Status: FAILURE PREDICTION THRESHOLD EXCEEDED").unwrap().smart_status.as_deref(), Some("WARNING"));
    }

    #[cfg(unix)]
    #[test]
    fn smartctl_exit_codes() {
        use std::os::unix::process::ExitStatusExt;
        let status = |code: i32| std::process::ExitStatus::from_raw(code << 8);
        assert!(smartctl_output_usable(status(0)));
        assert!(smartctl_output_usable(status(4))); // some command failed, output still valid
        assert!(smartctl_output_usable(status(8))); // disk failing: must be reported, not dropped
        assert!(smartctl_output_usable(status(64 | 128)));
        assert!(!smartctl_output_usable(status(1))); // command line error
        assert!(!smartctl_output_usable(status(2))); // device open failed
        assert!(!smartctl_output_usable(std::process::ExitStatus::from_raw(9))); // killed by signal
    }
}
