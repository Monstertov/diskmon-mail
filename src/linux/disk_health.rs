use std::fs;
use std::path::Path;
use std::process::Command;

pub fn get_smart_status(disk_name: &str, debug: bool) -> (Option<String>, Option<String>, Option<String>, Option<String>, bool, Option<u64>, Option<u64>, Option<i64>, Option<u64>, Option<u64>, String) {
    if debug {
        println!("[DEBUG] Getting SMART status for: {}", disk_name);
    }

    let mut health_method = "unknown".to_string();

    // Check if smartmontools is installed
    let smartctl_available = Command::new("smartctl").arg("--version").output().is_ok();
    // Do not print smartmontools detection here; only print debug output if debug is true

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
            return (None, None, None, None, false, None, None, None, None, None, health_method);
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
    let mut power_on_hours = None;
    let mut reallocated_sectors = None;
    let mut temperature = None;
    let mut pending_sectors = None;
    let mut uncorrectable_sectors = None;

    // Check for RAID indicators
    if device_name.contains("md") || device_name.contains("dm-") {
        is_raid = true;
        if debug {
            println!("[DEBUG] RAID device detected: {}", device_name);
        }
    }

    // First, try to use smartctl if available
    if smartctl_available {
        health_method = "smartmontools".to_string();
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

                            // Check for SMART attributes
                            if line.contains("Power_On_Hours") {
                                if let Ok(value) = line.split(':').nth(1).unwrap_or("").trim().parse::<u64>() {
                                    power_on_hours = Some(value);
                                }
                            }
                            if line.contains("Reallocated_Sector_Ct") {
                                if let Ok(value) = line.split(':').nth(1).unwrap_or("").trim().parse::<u64>() {
                                    reallocated_sectors = Some(value);
                                }
                            }
                            if line.contains("Temperature_Celsius") {
                                if let Ok(value) = line.split(':').nth(1).unwrap_or("").trim().parse::<i64>() {
                                    temperature = Some(value);
                                }
                            }
                            if line.contains("Current_Pending_Sector") {
                                if let Ok(value) = line.split(':').nth(1).unwrap_or("").trim().parse::<u64>() {
                                    pending_sectors = Some(value);
                                }
                            }
                            if line.contains("Offline_Uncorrectable") {
                                if let Ok(value) = line.split(':').nth(1).unwrap_or("").trim().parse::<u64>() {
                                    uncorrectable_sectors = Some(value);
                                }
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
                            
                            return (smart_status, serial_number, brand, model, is_raid, power_on_hours, reallocated_sectors, temperature, pending_sectors, uncorrectable_sectors, health_method);
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
        health_method = "kernel".to_string();
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
            return (smart_status, serial_number, brand, model, is_raid, power_on_hours, reallocated_sectors, temperature, pending_sectors, uncorrectable_sectors, health_method);
        }
    }

    // Fallback to kernel-based methods
    health_method = "kernel".to_string();
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

    (smart_status, serial_number, brand, model, is_raid, power_on_hours, reallocated_sectors, temperature, pending_sectors, uncorrectable_sectors, health_method)
}
