use std::fs;
use std::process::Command;
use crate::system::{self, SmartInfo};

/// Health information for the disk behind a mounted device, e.g. "/dev/sda1" or "/dev/nvme0n1p2"
/// (the device name sysinfo reports for a filesystem).
pub fn get_smart_status(device: &str, debug: bool) -> SmartInfo {
    if debug {
        eprintln!("[DEBUG] Getting SMART status for: {}", device);
    }

    let Some(name) = device.strip_prefix("/dev/") else {
        if debug {
            eprintln!("[DEBUG] Not a block device: {}", device);
        }
        return SmartInfo::unknown("unknown");
    };
    // "mapper/vg-root" -> "vg-root"
    let name = name.rsplit('/').next().unwrap_or(name);
    // Partition -> whole disk (sda1 -> sda, nvme0n1p1 -> nvme0n1, mmcblk0p1 -> mmcblk0)
    let disk = system::parent_disk(name);
    let device_base = format!("/dev/{disk}");
    let is_raid = name.starts_with("md") || name.starts_with("dm-");

    if debug {
        eprintln!("[DEBUG] Device base: {}", device_base);
        if is_raid {
            eprintln!("[DEBUG] RAID device detected: {}", device);
        }
    }

    // First, try smartctl if available
    if Command::new("smartctl").arg("--version").output().is_ok() {
        let dev = device_base.as_str();
        let attempts: Vec<Vec<&str>> = if disk.starts_with("nvme") {
            vec![vec!["-H", "-i", "-A", dev], vec!["-H", "-i", "-A", "-d", "nvme", dev]]
        } else {
            vec![
                vec!["-H", "-i", "-A", dev],
                vec!["-H", "-i", "-A", "-d", "auto", dev],
                vec!["-H", "-i", "-A", "-d", "sat", dev],
            ]
        };
        for args in attempts {
            if debug {
                eprintln!("[DEBUG] Trying smartctl with args: {:?}", args);
            }
            let Ok(output) = Command::new("smartctl").args(&args).output() else { continue };
            if !system::smartctl_output_usable(output.status) {
                continue;
            }
            let text = String::from_utf8_lossy(&output.stdout);
            if debug {
                eprintln!("[DEBUG] smartctl output: {}", text);
            }
            if let Some(info) = system::parse_smartctl(&text) {
                return SmartInfo { is_raid, ..info };
            }
        }
        if debug {
            eprintln!("[DEBUG] smartctl didn't provide useful information, falling back to kernel methods");
        }
    }

    let mut info = SmartInfo { is_raid, ..SmartInfo::unknown("kernel") };
    let errors = dmesg_error_count(disk, debug);

    // Raspberry Pi SD cards and other MMC devices
    if disk.starts_with("mmcblk") {
        // No errors in a readable kernel log counts as OK for SD cards (they have no SMART).
        info.smart_status = errors.map(|n| if n > 0 { "WARNING" } else { "OK" }.to_string());
        let sysfs = format!("/sys/block/{disk}/device");
        info.model = read_sysfs(&sysfs, "name");
        // CID register: product serial number is hex digits 18..26
        info.serial_number = read_sysfs(&sysfs, "cid")
            .and_then(|cid| cid.get(18..26).and_then(|hex| u32::from_str_radix(hex, 16).ok()))
            .map(|serial| format!("{serial:08X}"));
        info.brand = read_sysfs(&sysfs, "manfid")
            .and_then(|id| u32::from_str_radix(id.trim_start_matches("0x"), 16).ok())
            .map(mmc_manufacturer);
    } else {
        // Kernel-logged disk errors are real evidence of trouble. Their absence proves nothing,
        // so without smartctl the status stays unknown.
        if errors.is_some_and(|n| n > 0) {
            info.smart_status = Some("WARNING".to_string());
        }
        let sysfs = format!("/sys/block/{disk}/device");
        info.model = read_sysfs(&sysfs, "model");
        info.serial_number = read_sysfs(&sysfs, "serial");
        info.brand = read_sysfs(&sysfs, "vendor").filter(|v| !v.starts_with("0x")); // skip raw PCI vendor ids
    }

    if debug {
        eprintln!("[DEBUG] Kernel-based results: SMART={:?}, Model={:?}, Serial={:?}, Brand={:?}, RAID={}",
                 info.smart_status, info.model, info.serial_number, info.brand, is_raid);
    }
    info
}

fn read_sysfs(dir: &str, file: &str) -> Option<String> {
    fs::read_to_string(format!("{dir}/{file}"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Number of kernel log lines reporting errors for `disk`. None when dmesg cannot be read
/// (it needs root on many systems), because then there is no evidence either way.
fn dmesg_error_count(disk: &str, debug: bool) -> Option<usize> {
    let output = Command::new("dmesg").output().ok().filter(|o| o.status.success())?;
    let log = String::from_utf8_lossy(&output.stdout);
    let count = log
        .lines()
        .map(str::to_lowercase)
        .filter(|line| line.contains(disk) && is_disk_error(line))
        .inspect(|line| {
            if debug {
                eprintln!("[DEBUG] Found disk error in dmesg: {}", line);
            }
        })
        .count();
    Some(count)
}

/// Kernel messages that indicate a failing disk. Narrow on purpose: mount options such as
/// "errors=remount-ro" must not count as errors.
fn is_disk_error(line: &str) -> bool {
    ["i/o error", "medium error", "error -", "crc", "timeout", "ext4-fs error"]
        .iter()
        .any(|k| line.contains(k))
}

fn mmc_manufacturer(id: u32) -> String {
    match id {
        0x01 => "Panasonic".to_string(),
        0x02 => "Toshiba".to_string(),
        0x03 => "SanDisk".to_string(),
        0x13 => "Micron".to_string(),
        0x15 => "Samsung".to_string(),
        0x27 => "Phison".to_string(),
        0x28 => "Lexar".to_string(),
        0x41 => "Kingston".to_string(),
        0x6f => "STMicroelectronics".to_string(),
        0x74 => "Transcend".to_string(),
        0x76 => "Patriot".to_string(),
        _ => format!("Unknown (0x{:02X})", id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dmesg_error_matching() {
        for line in [
            "blk_update_request: i/o error, dev sda, sector 2048 op 0x0:(read)",
            "sd 0:0:0:0: [sda] tag#0 sense key : medium error [current]",
            "mmcblk0: error -110 sending status command, retrying",
            "ext4-fs error (device mmcblk0p2): ext4_find_entry:1455: inode #2: comm init: reading directory lblock 0",
        ] {
            assert!(is_disk_error(line), "{line}");
        }
        for line in [
            "ext4-fs (sda1): re-mounted. opts: errors=remount-ro. quota mode: none.",
            "sd 0:0:0:0: [sda] attached scsi disk",
            "mmcblk0: mmc0:aaaa sc16g 14.8 gib",
        ] {
            assert!(!is_disk_error(line), "{line}");
        }
    }

    #[test]
    fn non_block_devices_are_unknown() {
        for dev in ["overlay", "tmpfs", ""] {
            let info = get_smart_status(dev, false);
            assert!(info.smart_status.is_none());
            assert_eq!(info.health_method, "unknown");
        }
    }
}
