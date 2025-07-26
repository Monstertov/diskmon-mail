// Author: Monstertov
// Purpose: Cross-platform disk space monitor and email alert tool (Rust version of diskmon.py)

use lettre::{Message, SmtpTransport, Transport, transport::smtp::authentication::Credentials, transport::smtp::client::Tls, transport::smtp::client::TlsParameters};
use clap::Parser;
use colored::*;
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use tokio::time::timeout;
use futures::future::join_all;
use backoff::{ExponentialBackoff, backoff::Backoff};
use log::{info, warn, error, debug};

mod config;
mod system;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "windows")]
pub mod windows;

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
    /// Output results in JSON format
    #[arg(long)]
    json: bool,
    /// SMART collection timeout in seconds (default: 30)
    #[arg(long, default_value = "30")]
    smart_timeout: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
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
    power_on_hours: Option<u64>,
    reallocated_sectors: Option<u64>,
    temperature: Option<i64>,
    pending_sectors: Option<u64>,
    uncorrectable_sectors: Option<u64>,
    health_method: String, // New: method used for health check
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

async fn get_monitored_disks(cfg: &config::Config, debug: bool, smart_timeout: u64) -> Vec<DiskInfo> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let mut disk_candidates = Vec::new();
    let mut excluded_not_found = Vec::new();
    let excluded = cfg.excluded_disks.clone().unwrap_or_default();
    let mut found_excluded = vec![false; excluded.len()];
    
    // Check if health checks are enabled (default to true if not specified)
    let health_check_enabled = cfg.health_check_enabled.unwrap_or(true);

    if debug {
        debug!("sysinfo found {} disks:", disks.list().len());
        for (i, disk) in disks.list().iter().enumerate() {
            debug!("Disk {}: mount_point={:?}, name={:?}, fs={:?}, total={} available={}",
                i,
                disk.mount_point(),
                disk.name(),
                disk.file_system(),
                disk.total_space(),
                disk.available_space()
            );
        }
    }

    for (_disk_idx, disk) in disks.list().iter().enumerate() {
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

        // Exclude disks if in excluded_disks
        let is_excluded = if cfg!(windows) {
            excluded.iter().enumerate().any(|(i, ex)| {
                let ex = ex.trim();
                if ex.is_empty() { return false; }
                let ex = ex.to_uppercase();
                let disp = display_name.to_uppercase();
                let found = disp.contains(&ex);
                if found { found_excluded[i] = true; }
                found
            })
        } else {
            excluded.iter().enumerate().any(|(i, ex)| {
                let ex = ex.trim();
                if ex.is_empty() { return false; }
                let dev = disk.name().to_str().unwrap_or("");
                let found = dev == ex;
                if found { found_excluded[i] = true; }
                found
            })
        };
        if debug && is_excluded {
            debug!("Excluding disk: {} (display_name: {}, dev: {:?})", mount_point, display_name, disk.name());
        }
        if is_excluded { continue; }

        let file_system = disk.file_system().to_str().unwrap_or("Unknown").to_string();

        // Store disk information for parallel SMART collection
        disk_candidates.push((mount_point, display_name, free_space_percent, total, available, file_system, disk.name().to_str().unwrap_or("").to_string()));
    }

    // Collect excluded disks that were not found
    for (i, found) in found_excluded.iter().enumerate() {
        if !*found {
            excluded_not_found.push(excluded[i].clone());
        }
    }
    if !excluded_not_found.is_empty() {
        warn!("The following excluded_disks were not found: {}", excluded_not_found.join(", "));
    }

    // Parallel SMART status collection with timeout
    if health_check_enabled {
        let smart_futures = disk_candidates.iter().map(|(mount_point, _, _, _, _, _, disk_name)| {
            let smart_input = if cfg!(windows) {
                mount_point.clone()
            } else {
                disk_name.clone()
            };
            let timeout_duration = Duration::from_secs(smart_timeout);
            
            async move {
                let smart_input_clone = smart_input.clone();
                let smart_input_clone2 = smart_input.clone();
                match timeout(timeout_duration, tokio::task::spawn_blocking(move || {
                    system::get_smart_status(&smart_input, debug)
                })).await {
                    Ok(Ok(result)) => result,
                    Ok(Err(_)) => {
                        warn!("SMART collection task panicked for disk: {}", smart_input_clone);
                        (None, None, None, None, false, None, None, None, None, None, "error".to_string())
                    },
                    Err(_) => {
                        warn!("SMART collection timed out for disk: {} ({}s)", smart_input_clone2, smart_timeout);
                        (None, None, None, None, false, None, None, None, None, None, "timeout".to_string())
                    }
                }
            }
        });

        let smart_results = join_all(smart_futures).await;

        // Combine disk info with SMART results
        let final_disks: Vec<DiskInfo> = disk_candidates.into_iter().zip(smart_results.into_iter())
            .map(|((mount_point, display_name, free_space_percent, total, available, file_system, _), (smart_status, serial_number, brand, model, is_raid, power_on_hours, reallocated_sectors, temperature, pending_sectors, uncorrectable_sectors, health_method))| {
                DiskInfo {
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
                    power_on_hours,
                    reallocated_sectors,
                    temperature,
                    pending_sectors,
                    uncorrectable_sectors,
                    health_method,
                }
            }).collect();

        if debug {
            debug!("Final monitored disks:");
            for (i, disk) in final_disks.iter().enumerate() {
                debug!("Monitored Disk {}: mount_point={}, display_name={}, fs={}, total={}, available={}",
                    i,
                    disk.mount_point,
                    disk.display_name,
                    disk.file_system,
                    disk.total_space,
                    disk.available_space
                );
            }
        }

        final_disks
    } else {
        // Health checks disabled - convert to final format
        let final_disks: Vec<DiskInfo> = disk_candidates.into_iter().map(|(mount_point, display_name, free_space_percent, total, available, file_system, _)| {
            DiskInfo {
                mount_point,
                display_name,
                free_space_percent,
                total_space: total,
                available_space: available,
                file_system,
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
                health_method: "disabled".to_string(),
            }
        }).collect();

        if debug {
            debug!("Final monitored disks (health checks disabled):");
            for (i, disk) in final_disks.iter().enumerate() {
                debug!("Monitored Disk {}: mount_point={}, display_name={}, fs={}, total={}, available={}",
                    i,
                    disk.mount_point,
                    disk.display_name,
                    disk.file_system,
                    disk.total_space,
                    disk.available_space
                );
            }
        }

        final_disks
    }
}

async fn send_system_report(cfg: &config::Config, disks: &[DiskInfo], system_info: &system::SystemInfo, forced: bool, debug: bool) -> Result<(), String> {
    if !cfg.mail_enabled {
        println!("{} System report: {} disk(s) monitored. Mail not sent.", 
                 "[TEST MODE]".yellow().bold(), 
                 disks.len().to_string().cyan());
        return Ok(());
    }
    
    // Determine friendly name for this device (by hostname)
    let display_name = cfg.friendly_name.as_deref().unwrap_or(&system_info.hostname);

    let subject = if forced {
        format!("[FORCED] System Disk Report - {} ({})", display_name, format!("{} {} {}", system_info.os_name, system_info.os_version, system_info.architecture))
    } else {
        format!("System Disk Report - {} ({})", display_name, format!("{} {} {}", system_info.os_name, system_info.os_version, system_info.architecture))
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
         Device: {} ({})\n\
         System: {} {}\n\
         Hostname: {}\n\
         Report Time: {}\n\
         Mode: {}\n\
         SMART Tools: {}\n\
         Virtualization: {}\n\n",
        display_name,
        system_info.hostname,
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
        } else if disk.reallocated_sectors.unwrap_or(0) > 0 || disk.pending_sectors.unwrap_or(0) > 0 || disk.uncorrectable_sectors.unwrap_or(0) > 0 || disk.temperature.unwrap_or(0) > 55 {
            "[SMART WARNING]"
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
     - Free Space: {:.2}%\n\
     - Health Check Method: {}\n",
    i + 1,
    status_indicator,
    &disk.display_name,
    disk.mount_point,
    disk.file_system,
    total_gb,
    used_gb,
    available_gb,
    disk.free_space_percent,
    disk.health_method
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
        if let Some(val) = disk.power_on_hours {
            body.push_str(&format!(" - Power On Hours: {}\n", val));
        }
        if let Some(val) = disk.reallocated_sectors {
            body.push_str(&format!(" - Reallocated Sectors: {}\n", val));
            if val > 0 {
                body.push_str("   * WARNING: Reallocated sectors detected!\n");
            }
        }
        if let Some(val) = disk.pending_sectors {
            body.push_str(&format!(" - Pending Sectors: {}\n", val));
            if val > 0 {
                body.push_str("   * WARNING: Pending sectors detected!\n");
            }
        }
        if let Some(val) = disk.uncorrectable_sectors {
            body.push_str(&format!(" - Uncorrectable Sectors: {}\n", val));
            if val > 0 {
                body.push_str("   * WARNING: Uncorrectable sectors detected!\n");
            }
        }
        if let Some(val) = disk.temperature {
            body.push_str(&format!(" - Temperature: {} C\n", val));
            if val > 55 {
                body.push_str("   * WARNING: High temperature!\n");
            }
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
        if disk.health_method != "smartmontools" && disk.health_method != "WMI" {
            body.push_str("   * WARNING: Health info from fallback method; may be incomplete or unreliable.\n");
        }
        if disk.is_raid {
            body.push_str("   * WARNING: RAID device detected; health info may be unreliable.\n");
        }
        if system_info.is_virtualized {
            body.push_str("   * WARNING: Running in virtualized environment; health info may be unreliable.\n");
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
    if debug {
        println!("[DEBUG] smtp_security from config: {:?}", cfg.smtp_security);
    }
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
    
    // Send email with retry logic
    let mut backoff = ExponentialBackoff::default();
    backoff.max_elapsed_time = Some(Duration::from_secs(300)); // 5 minutes max
    backoff.initial_interval = Duration::from_secs(1);
    backoff.max_interval = Duration::from_secs(30);
    
    let mut attempt = 1;
    let max_attempts = 3;
    
    loop {
        match mailer.send(&email) {
            Ok(_) => break,
            Err(e) => {
                error!("SMTP attempt {} failed: {}", attempt, e);
                
                if attempt >= max_attempts {
                    return Err(format!("SMTP error after {} attempts: {}", max_attempts, e));
                }
                
                if let Some(delay) = backoff.next_backoff() {
                    warn!("Retrying SMTP in {:?}...", delay);
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                } else {
                    return Err(format!("SMTP error: {}", e));
                }
            }
        }
    }
    
    println!("{} System report sent for {} disk(s){}", 
             "SUCCESS".green().bold(), 
             disks.len().to_string().cyan(),
             if forced { " (forced)".yellow() } else if debug { " (debug)".yellow() } else { "".normal() });
    Ok(())
}

#[tokio::main]
async fn main() {
    // Load and validate configuration first to check debug setting
    let cfg = match config::load_config(config::CONFIG_PATH) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{} {}", "Configuration error:".red().bold(), e);
            std::process::exit(2);
        }
    };

    // Get debug setting and initialize logging with appropriate level
    let debug = cfg.debug.unwrap_or(false);
    let log_level = if debug {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };
    
    env_logger::Builder::from_default_env()
        .filter_level(log_level)
        .init();

    if debug {
        debug!("Debug mode enabled");
        debug!("Loaded config: {:#?}", cfg);
    }

    // Initialize color support based on terminal capabilities
    init_colors();
    
    let cli = Cli::parse();

    // Print smartmontools detection ONCE
    let smartctl_available = if cfg!(windows) {
        std::process::Command::new("smartctl").arg("--version").output().is_ok() ||
        std::process::Command::new("C:\\Program Files\\smartmontools\\bin\\smartctl.exe").arg("--version").output().is_ok()
    } else {
        std::process::Command::new("smartctl").arg("--version").output().is_ok()
    };
    if smartctl_available {
        info!("smartmontools detected - using smartctl for enhanced disk health monitoring");
    } else {
        info!("smartmontools not detected - using fallback methods");
    }

    // Get system information
    let system_info = system::get_system_info();
    if debug {
        debug!("System info: {:#?}", system_info);
    }
    println!("{} {} {} {} ({})", 
             "System:".blue().bold(), 
             system_info.os_name.green(), 
             system_info.os_version.green(), 
             system_info.architecture.green(),
             system_info.hostname.cyan());

    // Show loading message
    println!("{}", "Loading information, please wait...".yellow().italic());
    
    // Get all monitored disks
    let disks = get_monitored_disks(&cfg, debug, cli.smart_timeout).await;
    
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

        let method_output = match disk.health_method.as_str() {
            "smartmontools" => "[smartmontools]".green().to_string(),
            "WMI" => "[WMI]".green().to_string(),
            "kernel" => "[kernel fallback]".yellow().to_string(),
            "disabled" => "[health check disabled]".dimmed().to_string(),
            _ => "[unknown method]".red().to_string(),
        };

        println!("  {} {}: {}% free ({:.2} GB available, {} filesystem) {}{} {}", 
                 colored_icon,
                 disk.display_name.cyan(), 
                 colored_percent,
                 disk.available_space as f64 / (1024.0 * 1024.0 * 1024.0),
                 disk.file_system.magenta(),
                 smart_status_output,
                 raid_output,
                 method_output);
        if disk.health_method != "smartmontools" && disk.health_method != "WMI" {
            println!("    {}", "WARNING: Health info from fallback method; may be incomplete or unreliable.".yellow());
        }
        if disk.is_raid {
            println!("    {}", "WARNING: RAID device detected; health info may be unreliable.".yellow());
        }
        if system_info.is_virtualized {
            println!("    {}", "WARNING: Running in virtualized environment; health info may be unreliable.".yellow());
        }
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

    if cli.json {
        // JSON output mode
        #[derive(serde::Serialize)]
        struct JsonOutput {
            system_info: system::SystemInfo,
            disks: Vec<DiskInfo>,
            threshold_percent: f64,
            smartctl_available: bool,
            alerts: Vec<String>,
        }
        
        let threshold = cfg.threshold_percent.unwrap_or(10.0);
        let mut alerts = Vec::new();
        
        for disk in &disks {
            if disk.free_space_percent < threshold {
                alerts.push(format!("{}: Low space ({:.2}%)", disk.display_name, disk.free_space_percent));
            }
            if disk.smart_status.as_deref().unwrap_or("OK").to_uppercase() != "OK" {
                alerts.push(format!("{}: SMART failure ({})", disk.display_name, disk.smart_status.as_deref().unwrap_or("N/A")));
            }
        }
        
        let output = JsonOutput {
            system_info,
            disks,
            threshold_percent: threshold,
            smartctl_available,
            alerts,
        };
        
        match serde_json::to_string_pretty(&output) {
            Ok(json) => println!("{}", json),
            Err(e) => {
                error!("Failed to serialize JSON output: {}", e);
                std::process::exit(1);
            }
        }
        return;
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
            if disk.reallocated_sectors.unwrap_or(0) > 0 {
                println!("    {}", "WARNING: Reallocated sectors detected!".red().bold());
            }
            if disk.pending_sectors.unwrap_or(0) > 0 {
                println!("    {}", "WARNING: Pending sectors detected!".red().bold());
            }
            if disk.uncorrectable_sectors.unwrap_or(0) > 0 {
                println!("    {}", "WARNING: Uncorrectable sectors detected!".red().bold());
            }
            if disk.temperature.unwrap_or(0) > 55 {
                println!("    {}", "WARNING: High temperature!".red().bold());
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
        if let Err(e) = send_system_report(&cfg, &disks, &system_info, true, debug).await {
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
            let smart_enabled = cfg.smart_enabled.unwrap_or(true);

            if is_low_space || (smart_enabled && (is_smart_fail || send_on_unknown)) || debug_mode {
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
            if let Err(e) = send_system_report(&cfg, &disks, &system_info, false, debug).await {
                eprintln!("{} {}", "ERROR Failed to send system report:".red().bold(), e);
                errors_occurred = true;
            } else {
                alerts_sent = 1;
            }
        } else {
            let any_unknown_smart = disks.iter().any(|d| d.smart_status.is_none());
            if any_unknown_smart {
                println!("\n{} (above {:.1}% threshold, but health status is unknown for one or more disks).", 
                         "All disks are above threshold".yellow().bold(), 
                         threshold);
        } else {
            println!("\n{} (above {:.1}% threshold and SMART status OK).", 
                     "All disks are healthy".green().bold(), 
                     threshold);
            }
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
