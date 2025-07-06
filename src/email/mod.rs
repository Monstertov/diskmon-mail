use lettre::{Message, SmtpTransport, Transport, transport::smtp::authentication::Credentials, transport::smtp::client::Tls, transport::smtp::client::TlsParameters};
use std::time::{SystemTime, UNIX_EPOCH};
use colored::*;
use crate::config::Config;
use crate::disk::{DiskInfo, SystemInfo};

pub fn send_system_report(cfg: &Config, disks: &[DiskInfo], system_info: &SystemInfo, forced: bool, debug: bool) -> Result<(), String> {
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