// Author: Monstertov
// Purpose: Cross-platform disk space monitor and email alert tool (Rust version of diskmon.py)

use std::fs;
use std::path::Path;
use sysinfo::{Disks, System};
use lettre::{Message, SmtpTransport, Transport, transport::smtp::authentication::Credentials, transport::smtp::client::Tls, transport::smtp::client::TlsParameters};
use hostname::get as get_hostname;
use serde_yaml;
use clap::Parser;
use colored::*;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(target_os = "windows")]
use winapi::um::sysinfoapi;

const CONFIG_PATH: &str = "config.yaml";

/// Cross-platform disk space monitor and email alert tool
#[derive(Parser)]
#[command(name = "diskmon-mail")]
#[command(about = "Monitor disk space and send email alerts when below threshold")]
#[command(version)]
struct Cli {
    /// Force send email alert regardless of disk space threshold (for testing SMTP settings)
    #[arg(long)]
    force_mail: bool,
    /// Display SMART status for all detected disks
    #[arg(long)]
    smart: bool,
}

#[derive(Debug, Clone)]
struct DiskInfo {
    mount_point: String,
    display_name: String, // Drive letter for Windows, mount point for Unix
    free_space_percent: f64,
    total_space: u64,
    available_space: u64,
    file_system: String,
    smart_status: Option<String>,
    serial_number: Option<String>,
    brand: Option<String>,
    model: Option<String>,
    is_raid: bool,
}

#[derive(Debug, Clone)]
struct SystemInfo {
    os_name: String,
    os_version: String,
    architecture: String,
    hostname: String,
    is_virtualized: bool,
}

#[derive(serde::Deserialize)]
struct Config {
    mail_enabled: bool,
    smtp_server: String,
    smtp_port: u16,
    smtp_user: String,
    smtp_pass: String,
    email_from: String,
    email_to: String,
    smtp_security: Option<String>, // "none", "starttls", "ssl"
    threshold_percent: Option<f64>, // Disk space threshold percentage
    send_mail_on_unknown_status: Option<bool>,
    debug: Option<bool>, // Enable debug output
    health_check_enabled: Option<bool>, // Enable/disable disk health checks (default: true)
}

// Check if terminal supports colors
fn supports_colors() -> bool {
    // Check if we're in a terminal that supports colors
    if let Some(_term) = std::env::var("TERM").ok() {
        // Most Unix terminals support colors
        if cfg!(unix) {
            return true;
        }
    }
    
    // On Windows, check if we're in a modern terminal
    if cfg!(windows) {
        // Check for Windows Terminal, ConPTY, or other modern terminals
        if let Some(term_program) = std::env::var("TERM_PROGRAM").ok() {
            return term_program == "vscode" || term_program == "WindowsTerminal";
        }
        
        // Check if ANSI colors are supported
        if let Some(ansi_colors) = std::env::var("ANSICON").ok() {
            return !ansi_colors.is_empty();
        }
        
        // Check for ConPTY (Windows 10+)
        if let Some(wt_session) = std::env::var("WT_SESSION").ok() {
            return !wt_session.is_empty();
        }
    }
    
    // Default to false for safety
    false
}

// Initialize color support
fn init_colors() {
    if !supports_colors() {
        // Disable colors globally
        colored::control::set_override(false);
    }
}

fn load_config<P: AsRef<Path>>(path: P) -> Result<Config, String> {
    // Check if config file exists
    if !path.as_ref().exists() {
        return Err(format!("Configuration file not found: {}", path.as_ref().display()));
    }
    
    // Read config file
    let data = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read config file: {e}"))?;
    
    // Parse YAML
    let config: Config = serde_yaml::from_str(&data)
        .map_err(|e| format!("Failed to parse config YAML: {e}"))?;
    
    // Validate required fields
    validate_config(&config)?;
    
    Ok(config)
}

fn validate_config(config: &Config) -> Result<(), String> {
    let mut missing_keys = Vec::new();
    
    // Check for empty required string fields (except smtp_user and smtp_pass)
    if config.smtp_server.trim().is_empty() {
        missing_keys.push("smtp_server");
    }
    if config.email_from.trim().is_empty() {
        missing_keys.push("email_from");
    }
    if config.email_to.trim().is_empty() {
        missing_keys.push("email_to");
    }
    
    // Check port is valid
    if config.smtp_port == 0 {
        missing_keys.push("smtp_port (must be > 0)");
    }
    
    // Validate threshold_percent if provided
    if let Some(threshold) = config.threshold_percent {
        if threshold < 1.0 || threshold > 100.0 {
            missing_keys.push("threshold_percent (must be between 1.0 and 100.0)");
        }
    }
    
    if !missing_keys.is_empty() {
        return Err(format!("Missing or invalid required configuration keys: {}", missing_keys.join(", ")));
    }
    
    Ok(())
}

fn get_system_info() -> SystemInfo {
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

    let is_virtualized = is_virtualized();

    SystemInfo {
        os_name,
        os_version,
        architecture: architecture.to_string(),
        hostname,
        is_virtualized,
    }
}

#[cfg(target_os = "linux")]
fn is_virtualized() -> bool {
    if let Ok(cpuinfo) = fs::read_to_string("/proc/cpuinfo") {
        if cpuinfo.contains("hypervisor") {
            return true;
        }
    }
    false
}

#[cfg(target_os = "windows")]
fn is_virtualized() -> bool {
    let mut system_info: sysinfoapi::SYSTEM_INFO = unsafe { std::mem::zeroed() };
    unsafe { sysinfoapi::GetSystemInfo(&mut system_info) };
    // This is a simple check, a more robust solution would check for specific vendor IDs
    system_info.dwNumberOfProcessors < 2
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn is_virtualized() -> bool {
    false
}

#[cfg(target_os = "linux")]
fn get_smart_status(disk_name: &str, debug: bool) -> (Option<String>, Option<String>, Option<String>, Option<String>, bool) {
    use std::fs;
    use std::path::Path;
    use std::process::Command;

    if debug {
        println!("[DEBUG] Getting SMART status for: {}", disk_name);
    }

    // Check if smartmontools is installed
    let smartctl_available = Command::new("smartctl").arg("--version").output().is_ok();
    
    // Always show smartmontools detection status (not just in debug mode)
    if smartctl_available {
        println!("smartmontools detected - using smartctl for enhanced disk health monitoring");
    } else {
        println!("smartmontools not detected - falling back to kernel interfaces");
        println!("For better disk health monitoring, install smartmontools:");
        println!("  Debian/Ubuntu: sudo apt-get install smartmontools");
        println!("  CentOS/RHEL: sudo yum install smartmontools");
        println!("  Fedora: sudo dnf install smartmontools");
        println!("  Arch: sudo pacman -S smartmontools");
    }

    // Map mount point to device name using /proc/mounts
    let device_name = if let Ok(mounts) = fs::read_to_string("/proc/mounts") {
        let mut found_device = None;
        for line in mounts.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 && parts[1] == disk_name {
                found_device = Some(parts[0].to_string());
                break;
            }
        }
        found_device
    } else {
        None
    };

    let device_name = match device_name {
        Some(device) if device.starts_with("/dev/") => device,
        _ => {
            if debug {
                println!("[DEBUG] Could not determine device for mount point: {}", disk_name);
            }
            return (None, None, None, None, false);
        }
    };

    if debug {
        println!("[DEBUG] Found device: {}", device_name);
    }

    // Extract device name without partition (e.g., /dev/sda1 -> /dev/sda, /dev/mmcblk0p1 -> /dev/mmcblk0)
    let device_base = if let Some(name) = device_name.split('/').last() {
        if name.starts_with("mmcblk") {
            // For MMC devices, remove partition number (e.g., mmcblk0p1 -> mmcblk0)
            let base = name.chars().take_while(|c| !c.is_ascii_digit() || *c == '0').collect::<String>();
            format!("/dev/{}", base)
        } else if name.starts_with("nvme") {
            // For NVMe devices, remove partition number (e.g., nvme0n1p1 -> nvme0n1)
            let parts: Vec<&str> = name.split('p').collect();
            format!("/dev/{}", parts[0])
        } else if name.starts_with("sd") || name.starts_with("hd") {
            // For SATA/IDE devices, remove partition number (e.g., sda1 -> sda)
            let base = name.chars().take_while(|c| c.is_alphabetic()).collect::<String>();
            format!("/dev/{}", base)
        } else {
            device_name.clone()
        }
    } else {
        device_name.clone()
    };

    if debug {
        println!("[DEBUG] Device base: {}", device_base);
    }

    let mut smart_status = None;
    let mut serial_number = None;
    let mut model = None;
    let mut brand = None;
    let mut is_raid = false;

    // Check for RAID indicators
    if device_name.contains("md") || device_name.contains("dm-") {
        is_raid = true;
        if debug {
            println!("[DEBUG] RAID device detected: {}", device_name);
        }
    }

    // First, try to use smartctl if available
    if smartctl_available {
        if debug {
            println!("[DEBUG] Using smartctl for device: {}", device_base);
        }
        
        // Special handling for different device types
        let smartctl_args = if device_base.contains("mmcblk") {
            // For MMC/SD cards, try different device types
            vec![
                vec!["-H", "-i", &device_base],
                vec!["-H", "-i", "-d", "auto", &device_base],
                vec!["-H", "-i", "-d", "sat", &device_base],
            ]
        } else if device_base.contains("nvme") {
            // For NVMe devices
            vec![
                vec!["-H", "-i", &device_base],
                vec!["-H", "-i", "-d", "nvme", &device_base],
            ]
        } else {
            // For SATA/IDE devices
            vec![
                vec!["-H", "-i", &device_base],
                vec!["-H", "-i", "-d", "auto", &device_base],
                vec!["-H", "-i", "-d", "sat", &device_base],
            ]
        };

        // Try different smartctl command variations
        for args in smartctl_args {
            if debug {
                println!("[DEBUG] Trying smartctl with args: {:?}", args);
            }
            
            if let Ok(smartctl_output) = Command::new("smartctl").args(&args).output() {
                if smartctl_output.status.success() || smartctl_output.status.code() == Some(4) {
                    // Exit code 4 means some SMART or other ATA command failed, but basic info might be available
                    if let Ok(output_str) = String::from_utf8(smartctl_output.stdout) {
                        if debug {
                            println!("[DEBUG] smartctl output: {}", output_str);
                        }

                        // Parse SMART status from smartctl output
                        for line in output_str.lines() {
                            let line = line.trim();
                            
                            // Check for SMART overall-health self-assessment
                            if line.contains("SMART overall-health self-assessment test result:") {
                                if line.contains("PASSED") {
                                    smart_status = Some("OK".to_string());
                                } else if line.contains("FAILED") {
                                    smart_status = Some("FAILING".to_string());
                                } else {
                                    smart_status = Some("WARNING".to_string());
                                }
                            }
                            
                            // Alternative SMART status formats
                            if line.contains("SMART Health Status:") {
                                if line.contains("OK") {
                                    smart_status = Some("OK".to_string());
                                } else {
                                    smart_status = Some("WARNING".to_string());
                                }
                            }
                            
                            // Check for device model
                            if line.starts_with("Device Model:") || line.starts_with("Model Number:") {
                                model = Some(line.split(':').nth(1).unwrap_or("").trim().to_string());
                            }
                            
                            // Check for serial number
                            if line.starts_with("Serial Number:") {
                                serial_number = Some(line.split(':').nth(1).unwrap_or("").trim().to_string());
                            }
                            
                            // Check for vendor/product
                            if line.starts_with("Vendor:") {
                                brand = Some(line.split(':').nth(1).unwrap_or("").trim().to_string());
                            }

                            // Check for MMC/SD card specific info
                            if line.starts_with("Device:") {
                                model = Some(line.split(':').nth(1).unwrap_or("").trim().to_string());
                            }
                        }

                        // If we got useful information from smartctl, use it
                        if smart_status.is_some() || model.is_some() || serial_number.is_some() {
                            if debug {
                                println!("[DEBUG] Using smartctl results: SMART={:?}, Model={:?}, Serial={:?}, Brand={:?}", 
                                         smart_status, model, serial_number, brand);
                            }
                            
                            // If no SMART status but we got device info, assume OK
                            if smart_status.is_none() && (model.is_some() || serial_number.is_some()) {
                                smart_status = Some("OK".to_string());
                            }
                            
                            return (smart_status, serial_number, brand, model, is_raid);
                        }
                    }
                }
            }
        }
        
        if debug {
            println!("[DEBUG] smartctl didn't provide useful information, falling back to kernel methods");
        }
    }

    // Special handling for Raspberry Pi SD cards and MMC devices
    if device_base.contains("mmcblk") {
        if debug {
            println!("[DEBUG] MMC/SD card detected, using specialized detection methods");
        }
        
        // Check dmesg for MMC/SD card errors
        if let Ok(dmesg_output) = Command::new("dmesg").output() {
            if let Ok(dmesg_str) = String::from_utf8(dmesg_output.stdout) {
                let device_short = device_base.split('/').last().unwrap_or("");
                let mut error_count = 0;
                
                for line in dmesg_str.lines().rev().take(1000) { // Check last 1000 lines
                    if line.to_lowercase().contains(device_short) {
                        if line.to_lowercase().contains("error") || 
                           line.to_lowercase().contains("fail") || 
                           line.to_lowercase().contains("timeout") ||
                           line.to_lowercase().contains("crc") {
                            error_count += 1;
                            if debug {
                                println!("[DEBUG] Found MMC error in dmesg: {}", line);
                            }
                        }
                    }
                }
                
                if error_count > 0 {
                    smart_status = Some("WARNING".to_string());
                    if debug {
                        println!("[DEBUG] Found {} MMC errors in dmesg", error_count);
                    }
                } else {
                    smart_status = Some("OK".to_string());
                    if debug {
                        println!("[DEBUG] No MMC errors found in dmesg");
                    }
                }
            }
        }
        
        // Try to get MMC device info from sysfs
        let device_short = device_base.split('/').last().unwrap_or("");
        let sysfs_path = format!("/sys/block/{}/device", device_short);
        if Path::new(&sysfs_path).exists() {
            // Read MMC device name
            if let Ok(name_data) = fs::read_to_string(format!("{}/name", sysfs_path)) {
                model = Some(name_data.trim().to_string());
            }
            
            // Read MMC CID (Card Identification) for serial
            if let Ok(cid_data) = fs::read_to_string(format!("{}/cid", sysfs_path)) {
                // CID contains serial number in a specific format
                if cid_data.len() >= 32 {
                    let serial_hex = &cid_data[18..26]; // Serial number is at specific position
                    if let Ok(serial_num) = u32::from_str_radix(serial_hex, 16) {
                        serial_number = Some(format!("{:08X}", serial_num));
                    }
                }
            }
            
            // Read MMC manufacturer ID
            if let Ok(manfid_data) = fs::read_to_string(format!("{}/manfid", sysfs_path)) {
                if let Ok(manfid) = manfid_data.trim().parse::<u32>() {
                    brand = Some(match manfid {
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
                        _ => format!("Unknown (0x{:02X})", manfid),
                    });
                }
            }
        }
        
        if smart_status.is_some() {
            if debug {
                println!("[DEBUG] Using MMC-specific results: SMART={:?}, Model={:?}, Serial={:?}, Brand={:?}", 
                         smart_status, model, serial_number, brand);
            }
            return (smart_status, serial_number, brand, model, is_raid);
        }
    }

    // Fallback to kernel-based methods
    if debug {
        println!("[DEBUG] Using kernel-based health detection");
    }

    // Try to read from /sys/block/{device}/device/
    let sysfs_path = format!("/sys/block/{}/device", device_base);
    if Path::new(&sysfs_path).exists() {
        // Read model
        if let Ok(model_data) = fs::read_to_string(format!("{}/model", sysfs_path)) {
            model = Some(model_data.trim().to_string());
        }

        // Read serial
        if let Ok(serial_data) = fs::read_to_string(format!("{}/serial", sysfs_path)) {
            serial_number = Some(serial_data.trim().to_string());
        }

        // Read vendor
        if let Ok(vendor_data) = fs::read_to_string(format!("{}/vendor", sysfs_path)) {
            brand = Some(vendor_data.trim().to_string());
        }

        // Check for SMART status in /sys/block/{device}/queue/
        let queue_path = format!("/sys/block/{}/queue", device_base);
        if Path::new(&queue_path).exists() {
            // Try to read some basic health indicators
            if let Ok(rotational) = fs::read_to_string(format!("{}/rotational", queue_path)) {
                let is_ssd = rotational.trim() == "0";
                if debug {
                    println!("[DEBUG] Device type: {}", if is_ssd { "SSD" } else { "HDD" });
                }
            }
        }

        // Check for RAID indicators
        if device_name.contains("md") || device_name.contains("dm-") {
            is_raid = true;
        }

        // Try to read SMART attributes from /sys/block/{device}/device/
        let smart_path = format!("{}/smart_attributes", sysfs_path);
        if Path::new(&smart_path).exists() {
            if let Ok(smart_data) = fs::read_to_string(&smart_path) {
                // Parse SMART attributes if available
                for line in smart_data.lines() {
                    if line.contains("FAILING_NOW") || line.contains("Pre-fail") {
                        smart_status = Some("FAILING".to_string());
                        break;
                    }
                }
            }
        }

        // If no SMART status found, try alternative methods
        if smart_status.is_none() {
            // Check for any error indicators in /sys/block/{device}/
            let error_path = format!("/sys/block/{}/stat", device_base);
            if let Ok(stat_data) = fs::read_to_string(error_path) {
                let parts: Vec<&str> = stat_data.split_whitespace().collect();
                if parts.len() >= 4 {
                    // Check for I/O errors (field 3 in /proc/diskstats)
                    if let Ok(io_errors) = parts[3].parse::<u64>() {
                        if io_errors > 0 {
                            smart_status = Some("WARNING".to_string());
                        } else {
                            smart_status = Some("OK".to_string());
                        }
                    }
                }
            }
        }

        // If still no status, try reading from /proc/diskstats
        if smart_status.is_none() {
            if let Ok(diskstats) = fs::read_to_string("/proc/diskstats") {
                for line in diskstats.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 14 && parts[2] == device_base {
                        // Check for I/O errors (field 12)
                        if let Ok(io_errors) = parts[11].parse::<u64>() {
                            if io_errors > 0 {
                                smart_status = Some("WARNING".to_string());
                            } else {
                                smart_status = Some("OK".to_string());
                            }
                        }
                        break;
                    }
                }
            }
        }

        // Additional kernel-based health checks
        if smart_status.is_none() {
            // Check dmesg for disk errors
            if let Ok(dmesg_output) = Command::new("dmesg").output() {
                if let Ok(dmesg_str) = String::from_utf8(dmesg_output.stdout) {
                    // Look for recent disk-related errors
                    let error_patterns = [
                        &format!("{}.*error", device_base),
                        &format!("{}.*fail", device_base),
                        &format!("{}.*warning", device_base),
                        &format!("{}.*i/o error", device_base),
                    ];

                    for _pattern in &error_patterns {
                        if dmesg_str.lines().any(|line| {
                            line.to_lowercase().contains(&device_base.to_lowercase()) &&
                            (line.to_lowercase().contains("error") ||
                             line.to_lowercase().contains("fail") ||
                             line.to_lowercase().contains("warning") ||
                             line.to_lowercase().contains("i/o error"))
                        }) {
                            smart_status = Some("WARNING".to_string());
                            if debug {
                                println!("[DEBUG] Found disk errors in dmesg for {}", device_base);
                            }
                            break;
                        }
                    }
                }
            }

            // Check for filesystem errors (read-only check)
            if let Ok(fsck_output) = Command::new("fsck")
                .args(&["-n", &device_name])
                .output() {
                if !fsck_output.status.success() {
                    if let Ok(fsck_str) = String::from_utf8(fsck_output.stderr) {
                        if fsck_str.contains("error") || fsck_str.contains("corruption") {
                            smart_status = Some("WARNING".to_string());
                            if debug {
                                println!("[DEBUG] Found filesystem errors for {}", device_name);
                            }
                        }
                    }
                }
            }

            // If still no status, default to OK
            if smart_status.is_none() {
                smart_status = Some("OK".to_string());
            }
        }
    }

    if debug {
        println!("[DEBUG] Kernel-based results: SMART={:?}, Model={:?}, Serial={:?}, Brand={:?}, RAID={}", 
                 smart_status, model, serial_number, brand, is_raid);
    }

    (smart_status, serial_number, brand, model, is_raid)
}

#[cfg(target_os = "windows")]
fn get_smart_status(disk_name: &str, debug: bool) -> (Option<String>, Option<String>, Option<String>, Option<String>, bool) {
    use std::process::Command;

    if debug {
        println!("[DEBUG] Getting disk health status for: {}", disk_name);
    }

    // First, get the drive letter from the disk_name (e.g., "C:", "D:")
    let drive_letter = if disk_name.len() >= 2 && disk_name.chars().nth(1) == Some(':') {
        disk_name.chars().nth(0).unwrap().to_uppercase().to_string()
    } else {
        if debug {
            println!("[DEBUG] Invalid drive format: {}", disk_name);
        }
        return (None, None, None, None, false);
    };

    if debug {
        println!("[DEBUG] Looking for drive letter: {}", drive_letter);
    }

    // Check if smartmontools is installed (smartctl.exe)
    let smartctl_available = Command::new("smartctl").arg("--version").output().is_ok() ||
                            Command::new("C:\\Program Files\\smartmontools\\bin\\smartctl.exe").arg("--version").output().is_ok();
    
    // Always show smartmontools detection status (not just in debug mode)
    if smartctl_available {
        println!("smartmontools detected - using smartctl for enhanced disk health monitoring");
    } else {
        println!("smartmontools not detected - falling back to PowerShell/WMI");
        println!("For better disk health monitoring, install smartmontools:");
        println!("  Download from: https://www.smartmontools.org/wiki/Download#InstalltheWindowspackage");
        println!("  Install to: C:\\Program Files\\smartmontools");
    }

    // Try smartctl first if available
    if smartctl_available {
        if debug {
            println!("[DEBUG] Attempting to use smartctl for drive {}", drive_letter);
        }

        // First, map drive letter to physical disk using PowerShell
        let ps_script = format!(r#"
            try {{
                # Get the logical disk
                $logicalDisk = Get-WmiObject -Class Win32_LogicalDisk -Filter "DeviceID='{}:'"
                if (-not $logicalDisk) {{
                    Write-Output "LOGICAL_DISK_NOT_FOUND"
                    exit 1
                }}
                
                # Get the partition associated with this logical disk
                $partition = Get-WmiObject -Query "ASSOCIATORS OF {{Win32_LogicalDisk.DeviceID='{}:'}} WHERE AssocClass=Win32_LogicalDiskToPartition"
                if (-not $partition) {{
                    Write-Output "PARTITION_NOT_FOUND"
                    exit 1
                }}
                
                # Get the physical disk associated with this partition
                $physicalDisk = Get-WmiObject -Query "ASSOCIATORS OF {{Win32_DiskPartition.DeviceID='$($partition.DeviceID)'}} WHERE AssocClass=Win32_DiskDriveToDiskPartition"
                if (-not $physicalDisk) {{
                    Write-Output "PHYSICAL_DISK_NOT_FOUND"
                    exit 1
                }}
                
                # Output the physical disk index
                Write-Output $physicalDisk.Index
            }}
            catch {{
                Write-Output "ERROR: $($_.Exception.Message)"
                exit 1
            }}
        "#, drive_letter, drive_letter);

        if let Ok(output) = Command::new("powershell").args(&["-Command", &ps_script]).output() {
            if output.status.success() {
                if let Ok(disk_index_str) = String::from_utf8(output.stdout) {
                    let disk_index = disk_index_str.trim();
                    if !disk_index.starts_with("ERROR") && !disk_index.contains("NOT_FOUND") {
                        if debug {
                            println!("[DEBUG] Found physical disk index: {}", disk_index);
                        }

                        // Try different smartctl commands
                        let device_path = format!("/dev/pd{}", disk_index);
                        let smartctl_commands = vec![
                            vec!["smartctl", "-H", "-i", &device_path],
                            vec!["C:\\Program Files\\smartmontools\\bin\\smartctl.exe", "-H", "-i", &device_path],
                            vec!["smartctl", "-H", "-i", "-d", "auto", &device_path],
                            vec!["C:\\Program Files\\smartmontools\\bin\\smartctl.exe", "-H", "-i", "-d", "auto", &device_path],
                        ];

                        for cmd_args in smartctl_commands {
                            if debug {
                                println!("[DEBUG] Trying smartctl command: {:?}", cmd_args);
                            }

                            if let Ok(smartctl_output) = Command::new(&cmd_args[0]).args(&cmd_args[1..]).output() {
                                if smartctl_output.status.success() || smartctl_output.status.code() == Some(4) {
                                    if let Ok(output_str) = String::from_utf8(smartctl_output.stdout) {
                                        if debug {
                                            println!("[DEBUG] smartctl output: {}", output_str);
                                        }

                                        let mut smart_status = None;
                                        let mut serial_number = None;
                                        let mut model = None;
                                        let mut brand = None;

                                        // Parse smartctl output
                                        for line in output_str.lines() {
                                            let line = line.trim();
                                            
                                            // Check for SMART overall-health self-assessment
                                            if line.contains("SMART overall-health self-assessment test result:") {
                                                if line.contains("PASSED") {
                                                    smart_status = Some("OK".to_string());
                                                } else if line.contains("FAILED") {
                                                    smart_status = Some("FAILING".to_string());
                                                } else {
                                                    smart_status = Some("WARNING".to_string());
                                                }
                                            }
                                            
                                            // Alternative SMART status formats
                                            if line.contains("SMART Health Status:") {
                                                if line.contains("OK") {
                                                    smart_status = Some("OK".to_string());
                                                } else {
                                                    smart_status = Some("WARNING".to_string());
                                                }
                                            }
                                            
                                            // Check for device model
                                            if line.starts_with("Device Model:") || line.starts_with("Model Number:") {
                                                model = Some(line.split(':').nth(1).unwrap_or("").trim().to_string());
                                            }
                                            
                                            // Check for serial number
                                            if line.starts_with("Serial Number:") {
                                                serial_number = Some(line.split(':').nth(1).unwrap_or("").trim().to_string());
                                            }
                                            
                                            // Check for vendor
                                            if line.starts_with("Vendor:") {
                                                brand = Some(line.split(':').nth(1).unwrap_or("").trim().to_string());
                                            }
                                        }

                                        // If we got useful information from smartctl, use it
                                        if smart_status.is_some() || model.is_some() || serial_number.is_some() {
                                            if debug {
                                                println!("[DEBUG] Using smartctl results: SMART={:?}, Model={:?}, Serial={:?}, Brand={:?}", 
                                                         smart_status, model, serial_number, brand);
                                            }
                                            
                                            // If no SMART status but we got device info, assume OK
                                            if smart_status.is_none() && (model.is_some() || serial_number.is_some()) {
                                                smart_status = Some("OK".to_string());
                                            }
                                            
                                            return (smart_status, serial_number, brand, model, false);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        if debug {
            println!("[DEBUG] smartctl didn't provide useful information, falling back to PowerShell/WMI");
        }
    }

    // Use PowerShell to map logical drive to physical disk and get SMART status
    let ps_script = format!(r#"
        try {{
            # Get the logical disk
            $logicalDisk = Get-WmiObject -Class Win32_LogicalDisk -Filter "DeviceID='{}:'"
            if (-not $logicalDisk) {{
                Write-Output "LOGICAL_DISK_NOT_FOUND"
                exit 1
            }}
            
            # Get the partition associated with this logical disk
            $partition = Get-WmiObject -Query "ASSOCIATORS OF {{Win32_LogicalDisk.DeviceID='{}:'}} WHERE AssocClass=Win32_LogicalDiskToPartition"
            if (-not $partition) {{
                Write-Output "PARTITION_NOT_FOUND"
                exit 1
            }}
            
            # Get the physical disk associated with this partition
            $physicalDisk = Get-WmiObject -Query "ASSOCIATORS OF {{Win32_DiskPartition.DeviceID='$($partition.DeviceID)'}} WHERE AssocClass=Win32_DiskDriveToDiskPartition"
            if (-not $physicalDisk) {{
                Write-Output "PHYSICAL_DISK_NOT_FOUND"
                exit 1
            }}
            
            # Get the physical disk health using Get-PhysicalDisk
            $physicalDiskHealth = Get-PhysicalDisk | Where-Object {{ $_.DeviceID -eq $physicalDisk.Index }}
            if (-not $physicalDiskHealth) {{
                Write-Output "PHYSICAL_DISK_HEALTH_NOT_FOUND"
                exit 1
            }}
            
            # Return the health information
            [PSCustomObject]@{{
                DeviceID = $physicalDiskHealth.DeviceID
                FriendlyName = $physicalDiskHealth.FriendlyName
                Model = $physicalDiskHealth.Model
                SerialNumber = $physicalDiskHealth.SerialNumber
                Size = $physicalDiskHealth.Size
                HealthStatus = $physicalDiskHealth.HealthStatus
                OperationalStatus = $physicalDiskHealth.OperationalStatus
            }} | ConvertTo-Json -Compress
        }}
        catch {{
            Write-Output "ERROR: $($_.Exception.Message)"
            exit 1
        }}
    "#, drive_letter, drive_letter);

    let output = match Command::new("powershell")
        .args(&["-Command", &ps_script])
        .output() {
        Ok(output) => output,
        Err(e) => {
            if debug {
                println!("[DEBUG] Failed to execute PowerShell command: {:?}", e);
            }
            return (None, None, None, None, false);
        }
    };

    if !output.status.success() {
        if debug {
            println!("[DEBUG] PowerShell command failed: {}", 
                     String::from_utf8_lossy(&output.stderr));
        }
        return (None, None, None, None, false);
    }

    let json_output = match String::from_utf8(output.stdout) {
        Ok(s) => s.trim().to_string(),
        Err(e) => {
            if debug {
                println!("[DEBUG] Failed to parse PowerShell output: {:?}", e);
            }
            return (None, None, None, None, false);
        }
    };

    if debug {
        println!("[DEBUG] PowerShell output: {}", json_output);
    }

    // Check for error messages
    if json_output.starts_with("ERROR:") || 
       json_output == "LOGICAL_DISK_NOT_FOUND" ||
       json_output == "PARTITION_NOT_FOUND" ||
       json_output == "PHYSICAL_DISK_NOT_FOUND" ||
       json_output == "PHYSICAL_DISK_HEALTH_NOT_FOUND" {
        if debug {
            println!("[DEBUG] PowerShell returned error: {}", json_output);
        }
        return (None, None, None, None, false);
    }

    // Parse the JSON output
    let drive: serde_json::Value = match serde_json::from_str(&json_output) {
        Ok(d) => d,
        Err(e) => {
            if debug {
                println!("[DEBUG] Failed to parse JSON: {:?}", e);
            }
            return (None, None, None, None, false);
        }
    };

    // Extract health information
    let health_status = drive["HealthStatus"].as_str().unwrap_or("Unknown");
    let operational_status = drive["OperationalStatus"].as_str().unwrap_or("Unknown");
    
    // Determine SMART status based on health and operational status
    let smart_status = if health_status == "Healthy" && operational_status == "OK" {
        Some("OK".to_string())
    } else if health_status == "Unhealthy" || operational_status != "OK" {
        Some("FAILING".to_string())
    } else {
        Some("WARNING".to_string())
    };

    let serial = drive["SerialNumber"].as_str().map(|s| s.to_string());
    let model = drive["Model"].as_str().map(|s| s.to_string());
    let brand = None; // Brand not directly available from Get-PhysicalDisk
    let is_raid = false; // RAID detection would require additional queries

    if debug {
        println!("[DEBUG] Found disk for drive {}: HealthStatus={}, OperationalStatus={}, SMART={:?}", 
                 drive_letter, health_status, operational_status, smart_status);
        println!("[DEBUG] Model={:?}, Serial={:?}", model, serial);
    }

    (smart_status, serial, brand, model, is_raid)
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn get_smart_status(_disk_name: &str, _debug: bool) -> (Option<String>, Option<String>, Option<String>, Option<String>, bool) {
    (None, None, None, None, false)
}

fn get_monitored_disks(cfg: &Config, debug: bool) -> Vec<DiskInfo> {
    let disks = Disks::new_with_refreshed_list();
    let mut monitored_disks = Vec::new();
    
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
        let (smart_status, serial_number, brand, model, is_raid) = if health_check_enabled {
            // For Windows, pass the mount point (drive letter) instead of disk name
            let smart_input = if cfg!(windows) {
                &mount_point
            } else {
                disk.name().to_str().unwrap_or("")
            };
            get_smart_status(smart_input, debug)
        } else {
            (None, None, None, None, false)
        };

        monitored_disks.push(DiskInfo {
            mount_point,
            display_name,
            free_space_percent,
            total_space: total,
            available_space: available,
            file_system,
            smart_status,
            serial_number,
            brand,
            model,
            is_raid,
        });
    }

    monitored_disks
}

fn send_system_report(cfg: &Config, disks: &[DiskInfo], system_info: &SystemInfo, forced: bool, debug: bool) -> Result<(), String> {
    if !cfg.mail_enabled {
        println!("{} System report: {} disk(s) monitored. Mail not sent.", 
                 "[TEST MODE]".yellow().bold(), 
                 disks.len().to_string().cyan());
        return Ok(());
    }
    
    let subject = if forced {
        format!("[FORCED] System Disk Report - {} ({})", 
                system_info.hostname, 
                format!("{} {} {}", system_info.os_name, system_info.os_version, system_info.architecture))
    } else {
        format!("System Disk Report - {} ({})", 
                system_info.hostname, 
                format!("{} {} {}", system_info.os_name, system_info.os_version, system_info.architecture))
    };
    
    let os_info = format!("{} {} {}", system_info.os_name, system_info.os_version, system_info.architecture);
    let threshold = cfg.threshold_percent.unwrap_or(10.0);
    
    // Format current time in DD-MM-YYYY HH:MM:SS format
    let datetime = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => {
            let secs = duration.as_secs();
            let datetime = chrono::DateTime::from_timestamp(secs as i64, 0)
                .unwrap_or_else(|| chrono::Utc::now());
            let local_datetime = datetime.with_timezone(&chrono::Local);
            local_datetime.format("%d-%m-%Y %H:%M:%S").to_string()
        },
        Err(_) => "unknown time".to_string(),
    };
    
    // Check if smartmontools is available for the email report
    let smartctl_available = if cfg!(windows) {
        std::process::Command::new("smartctl").arg("--version").output().is_ok() ||
        std::process::Command::new("C:\\Program Files\\smartmontools\\bin\\smartctl.exe").arg("--version").output().is_ok()
    } else {
        std::process::Command::new("smartctl").arg("--version").output().is_ok()
    };

    let mut body = format!(
        "System Disk Report\n\n\
         System: {} {}\n\
         Hostname: {}\n\
         Report Time: {}\n\
         Mode: {}\n\
         SMART Tools: {}\n\
         Virtualization: {}\n\n",
        os_info,
        if system_info.is_virtualized { "(Virtualized)" } else { "" },
        system_info.hostname,
        datetime,
        if forced { "Forced Report" } else if debug { "Debug Mode" } else { "Normal Scan" },
        if smartctl_available { "smartmontools detected - enhanced disk health monitoring" } else { "smartmontools not detected - using fallback methods" },
        if system_info.is_virtualized { "Yes - Running in virtualized environment" } else { "No - Running on physical hardware" }
    );

    // Add disk summary
    let total_disks = disks.len();
    let low_space_disks = disks.iter().filter(|d| d.free_space_percent < threshold).count();
    let smart_failing_disks = disks.iter().filter(|d| {
        d.smart_status.as_deref().unwrap_or("OK").to_uppercase() != "OK"
    }).count();
    let unknown_smart_disks = disks.iter().filter(|d| d.smart_status.is_none()).count();

    body.push_str(&format!(
        "Disk Summary:\n\
         - Total Disks: {}\n\
         - Low Space (<{}%): {}\n\
         - SMART Failing: {}\n\
         - SMART Unknown: {}\n\n",
        total_disks, threshold, low_space_disks, smart_failing_disks, unknown_smart_disks
    ));

    // Add detailed disk information
    body.push_str("Detailed Disk Information:\n");
    body.push_str(&"=".repeat(50));
    body.push_str("\n\n");

    // Add warnings for RAID and missing health info
    let mut no_health_info = false;
    let mut any_raid = false;
    for disk in disks {
        if disk.smart_status.is_none() || disk.smart_status.as_deref() == Some("N/A") {
            no_health_info = true;
        }
        if disk.is_raid {
            any_raid = true;
        }
    }
    if no_health_info {
        body.push_str("\nWARNING: No health information available for one or more disks. This tool should NOT be used for health monitoring tasks on these systems.\n");
    }
    if any_raid {
        body.push_str("\nWARNING: RAID device(s) detected. Health information may be unavailable or unreliable. This tool should NOT be used for health monitoring tasks on RAID systems.\n");
    }

    for (i, disk) in disks.iter().enumerate() {
        let total_gb = disk.total_space as f64 / (1024.0 * 1024.0 * 1024.0);
        let available_gb = disk.available_space as f64 / (1024.0 * 1024.0 * 1024.0);
        let used_gb = total_gb - available_gb;
        
        let status_indicator = if disk.free_space_percent < threshold {
            "[LOW SPACE]"
        } else if disk.smart_status.as_deref().unwrap_or("OK").to_uppercase() != "OK" {
            "[SMART FAILING]"
        } else {
            "[OK]"
        };

        body.push_str(&format!(
            "Disk {}: {} {}\n\
             - Mount Point: {}\n\
             - File System: {}\n\
             - Total Space: {:.2} GB\n\
             - Used Space: {:.2} GB\n\
             - Available Space: {:.2} GB\n\
             - Free Space: {:.2}%\n",
            i + 1,
            status_indicator,
            disk.display_name,
            disk.mount_point,
            disk.file_system,
            total_gb,
            used_gb,
            available_gb,
            disk.free_space_percent
        ));

        // Add SMART information
        if let Some(status) = &disk.smart_status {
            body.push_str(&format!(
                " - SMART Status: {}\n",
                status
            ));
        } else {
            body.push_str(" - SMART Status: Unknown/N/A\n");
        }

        if let Some(serial) = &disk.serial_number {
            body.push_str(&format!(" - Serial Number: {}\n", serial));
        }
        if let Some(brand) = &disk.brand {
            body.push_str(&format!(" - Brand: {}\n", brand));
        }
        if let Some(model) = &disk.model {
            body.push_str(&format!(" - Model: {}\n", model));
        }
        if disk.is_raid {
            body.push_str(" - RAID: Yes (SMART status may not be accurate)\n");
        }

        body.push_str("\n");
    }

    // Add recommendations if there are issues
    let has_issues = low_space_disks > 0 || smart_failing_disks > 0;
    if has_issues {
        body.push_str("Recommendations:\n");
        body.push_str(&"=".repeat(50));
        body.push_str("\n");
        
        if low_space_disks > 0 {
            body.push_str(&format!(
                "- {} disk(s) have low space. Consider:\n\
                 * Cleaning temporary files\n\
                 * Removing unused applications\n\
                 * Expanding storage capacity\n",
                low_space_disks
            ));
        }
        
        if smart_failing_disks > 0 {
            body.push_str(&format!(
                "- {} disk(s) have SMART failures. Consider:\n\
                 * Backing up important data immediately\n\
                 * Replacing the failing disk(s)\n\
                 * Monitoring disk health more frequently\n",
                smart_failing_disks
            ));
        }
        
        body.push_str("\n");
    }

    body.push_str("This is an automated report from DiskMon-Mail.\n");
    body.push_str("For more information, visit: https://github.com/Monstertov/diskmon-mail");
    
    let email = Message::builder()
        .from(cfg.email_from.parse().map_err(|e| format!("Invalid sender email address: {e}"))?)
        .to(cfg.email_to.parse().map_err(|e| format!("Invalid recipient email address: {e}"))?)
        .subject(subject)
        .body(body)
        .map_err(|e| format!("Failed to build email message: {e}"))?;
    
    let use_auth = !(cfg.smtp_user.trim().is_empty() && cfg.smtp_pass.trim().is_empty());
    let security = cfg.smtp_security.as_deref().unwrap_or("starttls").to_lowercase();
    
    let mailer = match security.as_str() {
        "none" => {
            let mut builder = SmtpTransport::builder_dangerous(&cfg.smtp_server).port(cfg.smtp_port);
            if use_auth {
                builder = builder.credentials(Credentials::new(cfg.smtp_user.clone(), cfg.smtp_pass.clone()));
            }
            builder.build()
        },
        "ssl" => {
            let tls = TlsParameters::new(cfg.smtp_server.clone())
                .map_err(|e| format!("TLS parameter error: {e}"))?;
            let mut builder = SmtpTransport::relay(&cfg.smtp_server)
                .map_err(|e| format!("SMTP relay error: {e}"))?
                .port(cfg.smtp_port)
                .tls(Tls::Wrapper(tls));
            if use_auth {
                builder = builder.credentials(Credentials::new(cfg.smtp_user.clone(), cfg.smtp_pass.clone()));
            }
            builder.build()
        },
        _ => { // starttls (default)
            let mut builder = SmtpTransport::relay(&cfg.smtp_server)
                .map_err(|e| format!("SMTP relay error: {e}"))?
                .port(cfg.smtp_port);
            if use_auth {
                builder = builder.credentials(Credentials::new(cfg.smtp_user.clone(), cfg.smtp_pass.clone()));
            }
            builder.build()
        }
    };
    
    // Send email and provide detailed error information
    mailer.send(&email)
        .map_err(|e| format!("SMTP error: {e}"))?;
    
    println!("{} System report sent for {} disk(s){}", 
             "SUCCESS".green().bold(), 
             disks.len().to_string().cyan(),
             if forced { " (forced)".yellow() } else if debug { " (debug)".yellow() } else { "".normal() });
    Ok(())
}

fn main() {
    // Initialize color support based on terminal capabilities
    init_colors();
    
    let cli = Cli::parse();

    // Load and validate configuration
    let cfg = match load_config(CONFIG_PATH) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{} {}", "Configuration error:".red().bold(), e);
            std::process::exit(2);
        }
    };

    // Get system information
    let system_info = get_system_info();
    println!("{} {} {} {} ({})", 
             "System:".blue().bold(), 
             system_info.os_name.green(), 
             system_info.os_version.green(), 
             system_info.architecture.green(),
             system_info.hostname.cyan());

    // Show loading message
    println!("{}", "Loading information, please wait...".yellow().italic());

    // Get debug setting
    let debug = cfg.debug.unwrap_or(false);
    
    // Get all monitored disks
    let disks = get_monitored_disks(&cfg, debug);
    
    if disks.is_empty() {
        eprintln!("{} This could indicate a system error or all disks are removable/network drives.", 
                  "No monitored disks found.".red().bold());
        std::process::exit(1);
    }

    println!("{} {} disk(s):", "Monitoring".blue().bold(), disks.len().to_string().green());
    
    // Display disk information
    for disk in &disks {
        let status_color = if disk.free_space_percent < 20.0 {
            "red"
        } else if disk.free_space_percent < 50.0 {
            "yellow"
        } else {
            "green"
        };
        
        let status_icon = if disk.free_space_percent < 20.0 {
            "!"
        } else if disk.free_space_percent < 50.0 {
            "*"
        } else {
            "OK"
        };
        
        let colored_percent = match status_color {
            "red" => format!("{:.2}", disk.free_space_percent).red().bold(),
            "yellow" => format!("{:.2}", disk.free_space_percent).yellow().bold(),
            _ => format!("{:.2}", disk.free_space_percent).green().bold(),
        };
        
        let colored_icon = match status_color {
            "red" => status_icon.red().bold(),
            "yellow" => status_icon.yellow().bold(),
            _ => status_icon.green().bold(),
        };
        
        let smart_status_output = if let Some(status) = &disk.smart_status {
            if status.to_uppercase() == "OK" {
                format!("(SMART: {})", "OK".green())
            } else {
                format!("(SMART: {})", status.red().bold())
            }
        } else {
            "(SMART: N/A)".dimmed().to_string()
        };

        let raid_output = if disk.is_raid {
            " (RAID)".dimmed().to_string()
        } else {
            "".to_string()
        };

        println!("  {} {}: {}% free ({:.2} GB available, {} filesystem) {}{}", 
                 colored_icon,
                 disk.display_name.cyan(), 
                 colored_percent,
                 disk.available_space as f64 / (1024.0 * 1024.0 * 1024.0),
                 disk.file_system.magenta(),
                 smart_status_output,
                 raid_output);
    }

    // Add warnings for RAID and missing health info
    let mut no_health_info = false;
    let mut any_raid = false;
    for disk in &disks {
        if disk.smart_status.is_none() || disk.smart_status.as_deref() == Some("N/A") {
            no_health_info = true;
        }
        if disk.is_raid {
            any_raid = true;
        }
    }
    if no_health_info {
        println!("{}", "WARNING: No health information available for one or more disks. This tool should NOT be used for health monitoring tasks on these systems.".red().bold());
    }
    if any_raid {
        println!("{}", "WARNING: RAID device(s) detected. Health information may be unavailable or unreliable. This tool should NOT be used for health monitoring tasks on RAID systems.".red().bold());
    }

    if cli.smart {
        println!("\n{}", "SMART Status Details:".blue().bold());
        for disk in &disks {
            let status = disk.smart_status.as_deref().unwrap_or("N/A");
            let color = if status.to_uppercase() == "OK" { "green" } else { "red" };
            let colored_status = match color {
                "green" => status.green().bold(),
                _ => status.red().bold(),
            };
            println!("  {}: {}", disk.display_name.cyan(), colored_status);
            println!("    Serial: {}", disk.serial_number.as_deref().unwrap_or("N/A").dimmed());
            println!("    Brand: {}", disk.brand.as_deref().unwrap_or("N/A").dimmed());
            println!("    Model: {}", disk.model.as_deref().unwrap_or("N/A").dimmed());
            if disk.is_raid {
                println!("    {}", "(RAID)".dimmed());
            }
        }
        return;
    }

    // Handle email alerts
    let threshold = cfg.threshold_percent.unwrap_or(10.0); // Default to 10% if not specified
    let mut alerts_sent = 0;
    let mut errors_occurred = false;
    
    if cli.force_mail {
        // Force send comprehensive system report for all disks
        println!("\n{}", "Forced mail mode: Sending comprehensive system report...".yellow().bold());
        if let Err(e) = send_system_report(&cfg, &disks, &system_info, true, debug) {
            eprintln!("{} {}", "ERROR Failed to send system report:".red().bold(), e);
            errors_occurred = true;
        } else {
            alerts_sent = 1;
        }
    } else {
        // Check each disk against threshold and SMART status
        let mut problem_disks = Vec::new();
        
        for disk in &disks {
            let is_low_space = disk.free_space_percent < threshold;
            let is_smart_fail = disk.smart_status.as_deref().unwrap_or("OK").to_uppercase() != "OK";
            let send_on_unknown = cfg.send_mail_on_unknown_status.unwrap_or(false) && disk.smart_status.is_none();
            let debug_mode = debug; // Always send mail when debug is enabled

            if is_low_space || is_smart_fail || send_on_unknown || debug_mode {
                problem_disks.push(disk);
            }
        }
        
        if !problem_disks.is_empty() {
            println!("\n{} {} disk(s):", 
                     "Alerts triggered for".red().bold(), 
                     problem_disks.len().to_string().red().bold());
            for disk in &problem_disks {
                let mut reasons = Vec::new();
                if disk.free_space_percent < threshold {
                    reasons.push(format!("low space ({:.2}%)", disk.free_space_percent));
                }
                if disk.smart_status.as_deref().unwrap_or("OK").to_uppercase() != "OK" {
                    reasons.push(format!("SMART status: {}", disk.smart_status.as_deref().unwrap_or("N/A")));
                } else if disk.smart_status.is_none() && cfg.send_mail_on_unknown_status.unwrap_or(false) {
                    reasons.push("SMART status: Unknown".to_string());
                }
                if debug {
                    reasons.push("debug mode enabled".to_string());
                }

                println!("  {} {}: {}", 
                         "!".red().bold(),
                         disk.display_name.cyan(), 
                         reasons.join(", ").red().bold());
            }
            
            // Send one comprehensive report with all problem disks
            if let Err(e) = send_system_report(&cfg, &disks, &system_info, false, debug) {
                eprintln!("{} {}", "ERROR Failed to send system report:".red().bold(), e);
                errors_occurred = true;
            } else {
                alerts_sent = 1;
            }
        } else {
            println!("\n{} (above {:.1}% threshold and SMART status OK).", 
                     "All disks are healthy".green().bold(), 
                     threshold);
        }
    }

    // Summary
    if alerts_sent > 0 {
        println!("\n{} {} alert(s) sent successfully.", 
                 "Summary:".blue().bold(), 
                 alerts_sent.to_string().green().bold());
    }
    
    if errors_occurred {
        eprintln!("{}", "Some errors occurred during alert processing.".red().bold());
        std::process::exit(2);
    }
}
