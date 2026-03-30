use std::process::Command;
use crate::system::SmartInfo;

/// Default path to smartctl on Windows when it is not on PATH.
const SMARTCTL_WIN_PATH: &str = "C:\\Program Files\\smartmontools\\bin\\smartctl.exe";

pub fn get_smart_status(disk_name: &str, debug: bool) -> SmartInfo {
    if debug {
        println!("[DEBUG] Getting disk health status for: {}", disk_name);
    }

    // Extract the drive letter from disk_name (e.g., "C:", "D:").
    // The letter is validated to be a single ASCII alphabetic character before it is
    // interpolated into any PowerShell script, preventing script injection.
    let drive_letter = if disk_name.len() >= 2 && disk_name.chars().nth(1) == Some(':') {
        match disk_name.chars().next() {
            Some(c) if c.is_ascii_alphabetic() => c.to_uppercase().to_string(),
            _ => {
                if debug {
                    println!("[DEBUG] Invalid drive letter in: {}", disk_name);
                }
                return SmartInfo::unknown("unknown");
            }
        }
    } else {
        if debug {
            println!("[DEBUG] Invalid drive format: {}", disk_name);
        }
        return SmartInfo::unknown("unknown");
    };

    if debug {
        println!("[DEBUG] Looking for drive letter: {}", drive_letter);
    }

    // Check if smartmontools is installed (smartctl.exe)
    let smartctl_available = Command::new("smartctl").arg("--version").output().is_ok()
        || Command::new(SMARTCTL_WIN_PATH).arg("--version").output().is_ok();

    // Try smartctl first if available
    if smartctl_available {
        if debug {
            println!("[DEBUG] Attempting to use smartctl for drive {}", drive_letter);
        }

        // Map the drive letter to its physical disk index via PowerShell/WMI.
        // drive_letter is a single validated ASCII letter — interpolation is safe.
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
                            vec![SMARTCTL_WIN_PATH, "-H", "-i", &device_path],
                            vec!["smartctl", "-H", "-i", "-d", "auto", &device_path],
                            vec![SMARTCTL_WIN_PATH, "-H", "-i", "-d", "auto", &device_path],
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

                                            if line.contains("SMART overall-health self-assessment test result:") {
                                                if line.contains("PASSED") {
                                                    smart_status = Some("OK".to_string());
                                                } else if line.contains("FAILED") {
                                                    smart_status = Some("FAILING".to_string());
                                                } else {
                                                    smart_status = Some("WARNING".to_string());
                                                }
                                            }

                                            if line.contains("SMART Health Status:") {
                                                if line.contains("OK") {
                                                    smart_status = Some("OK".to_string());
                                                } else {
                                                    smart_status = Some("WARNING".to_string());
                                                }
                                            }

                                            if line.starts_with("Device Model:") || line.starts_with("Model Number:") {
                                                model = Some(line.split(':').nth(1).unwrap_or("").trim().to_string());
                                            }

                                            if line.starts_with("Serial Number:") {
                                                serial_number = Some(line.split(':').nth(1).unwrap_or("").trim().to_string());
                                            }

                                            if line.starts_with("Vendor:") {
                                                brand = Some(line.split(':').nth(1).unwrap_or("").trim().to_string());
                                            }
                                        }

                                        if smart_status.is_some() || model.is_some() || serial_number.is_some() {
                                            if debug {
                                                println!("[DEBUG] Using smartctl results: SMART={:?}, Model={:?}, Serial={:?}, Brand={:?}",
                                                         smart_status, model, serial_number, brand);
                                            }

                                            if smart_status.is_none() && (model.is_some() || serial_number.is_some()) {
                                                smart_status = Some("OK".to_string());
                                            }

                                            return SmartInfo {
                                                smart_status,
                                                serial_number,
                                                brand,
                                                model,
                                                is_raid: false,
                                                power_on_hours: None,
                                                reallocated_sectors: None,
                                                temperature: None,
                                                pending_sectors: None,
                                                uncorrectable_sectors: None,
                                                health_method: "smartmontools".to_string(),
                                            };
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

    // Fall back to PowerShell/WMI for disk health.
    // drive_letter is a single validated ASCII letter — interpolation is safe.
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
            return SmartInfo::unknown("unknown");
        }
    };

    if !output.status.success() {
        if debug {
            println!("[DEBUG] PowerShell command failed: {}",
                     String::from_utf8_lossy(&output.stderr));
        }
        return SmartInfo::unknown("unknown");
    }

    let json_output = match String::from_utf8(output.stdout) {
        Ok(s) => s.trim().to_string(),
        Err(e) => {
            if debug {
                println!("[DEBUG] Failed to parse PowerShell output: {:?}", e);
            }
            return SmartInfo::unknown("unknown");
        }
    };

    if debug {
        println!("[DEBUG] PowerShell output: {}", json_output);
    }

    if json_output.starts_with("ERROR:") ||
       json_output == "LOGICAL_DISK_NOT_FOUND" ||
       json_output == "PARTITION_NOT_FOUND" ||
       json_output == "PHYSICAL_DISK_NOT_FOUND" ||
       json_output == "PHYSICAL_DISK_HEALTH_NOT_FOUND" {
        if debug {
            println!("[DEBUG] PowerShell returned error: {}", json_output);
        }
        return SmartInfo::unknown("unknown");
    }

    let drive: serde_json::Value = match serde_json::from_str(&json_output) {
        Ok(d) => d,
        Err(e) => {
            if debug {
                println!("[DEBUG] Failed to parse JSON: {:?}", e);
            }
            return SmartInfo::unknown("unknown");
        }
    };

    let health_status = drive["HealthStatus"].as_str().unwrap_or("Unknown");
    let operational_status = drive["OperationalStatus"].as_str().unwrap_or("Unknown");

    let smart_status = if health_status == "Healthy" && operational_status == "OK" {
        Some("OK".to_string())
    } else if health_status == "Unhealthy" || operational_status != "OK" {
        Some("FAILING".to_string())
    } else {
        Some("WARNING".to_string())
    };

    let serial = drive["SerialNumber"].as_str().map(|s| s.to_string());
    let model = drive["Model"].as_str().map(|s| s.to_string());

    if debug {
        println!("[DEBUG] Found disk for drive {}: HealthStatus={}, OperationalStatus={}, SMART={:?}",
                 drive_letter, health_status, operational_status, smart_status);
        println!("[DEBUG] Model={:?}, Serial={:?}", model, serial);
    }

    SmartInfo {
        smart_status,
        serial_number: serial,
        brand: None, // Brand not directly available from Get-PhysicalDisk
        model,
        is_raid: false, // RAID detection would require additional queries
        power_on_hours: None,
        reallocated_sectors: None,
        temperature: None,
        pending_sectors: None,
        uncorrectable_sectors: None,
        health_method: "WMI".to_string(),
    }
}
