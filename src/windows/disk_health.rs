use std::process::Command;

pub fn get_smart_status(disk_name: &str, debug: bool) -> (Option<String>, Option<String>, Option<String>, Option<String>, bool) {
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
    // Do not print smartmontools detection here; only print debug output if debug is true

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
