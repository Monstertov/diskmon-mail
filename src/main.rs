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
}

#[derive(Debug, Clone)]
struct DiskInfo {
    mount_point: String,
    display_name: String, // Drive letter for Windows, mount point for Unix
    free_space_percent: f64,
    total_space: u64,
    available_space: u64,
    file_system: String,
}

#[derive(Debug, Clone)]
struct SystemInfo {
    os_name: String,
    os_version: String,
    architecture: String,
    hostname: String,
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
    
    // smtp_user and smtp_pass: only require presence, not non-empty
    // (Serde will always provide a value if the key exists, even if empty)
    
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
    
    // Determine architecture
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
    
    SystemInfo {
        os_name,
        os_version,
        architecture: architecture.to_string(),
        hostname,
    }
}

fn get_monitored_disks() -> Vec<DiskInfo> {
    let disks = Disks::new_with_refreshed_list();
    let mut monitored_disks = Vec::new();
    
    for disk in disks.list() {
        // Skip removable disks (USB, CD/DVD, floppy, etc.)
        // Note: DiskKind::Removable might not be available in this version
        // We'll use a different approach to filter removable disks
        let mount_point = match disk.mount_point().to_str() {
            Some(path) => path.to_string(),
            None => continue, // Skip if we can't get the path
        };
        
        // Skip common removable disk patterns
        if cfg!(windows) {
            // Skip network drives and common removable drive patterns
            if mount_point.starts_with("\\\\") || 
               mount_point.starts_with("A:") || 
               mount_point.starts_with("B:") {
                continue;
            }
        } else {
            // Skip common removable mount points on Unix-like systems
            if mount_point.starts_with("/media/") || 
               mount_point.starts_with("/mnt/") ||
               mount_point.starts_with("/run/media/") {
                continue;
            }
        }
        
        // Calculate free space percentage
        let total = disk.total_space();
        let available = disk.available_space();
        
        // Skip disks with zero total space
        if total == 0 {
            continue;
        }
        
        let free_space_percent = (available as f64 / total as f64) * 100.0;
        
        // Create display name
        let display_name = if cfg!(windows) {
            // For Windows, use drive letter (e.g., "Drive C:")
            if mount_point.len() >= 2 && mount_point.chars().nth(1) == Some(':') {
                format!("Drive {}", mount_point.chars().nth(0).unwrap().to_uppercase())
            } else {
                mount_point.clone()
            }
        } else {
            // For Unix-like systems, use mount point
            mount_point.clone()
        };
        
        // Get file system name
        let file_system = disk.file_system()
            .to_str()
            .unwrap_or("Unknown")
            .to_string();
        
        monitored_disks.push(DiskInfo {
            mount_point,
            display_name,
            free_space_percent,
            total_space: total,
            available_space: available,
            file_system,
        });
    }
    
    monitored_disks
}

fn send_disk_alert(cfg: &Config, disk: &DiskInfo, system_info: &SystemInfo, forced: bool) -> Result<(), String> {
    if !cfg.mail_enabled {
        println!("{} Alert: {} free space is {}threshold: {:.2}% remaining. Mail not sent.", 
                 "[TEST MODE]".yellow().bold(), 
                 disk.display_name.cyan(), 
                 if forced { "(forced) below ".red() } else { "below ".red() }, 
                 disk.free_space_percent);
        return Ok(());
    }
    
    let subject = if forced {
        format!("[FORCED] Disk Space Alert - {} on {} ({})", 
                disk.display_name, 
                system_info.hostname, 
                format!("{} {} {}", system_info.os_name, system_info.os_version, system_info.architecture))
    } else {
        format!("Disk Space Alert - {} on {} ({})", 
                disk.display_name, 
                system_info.hostname, 
                format!("{} {} {}", system_info.os_name, system_info.os_version, system_info.architecture))
    };
    
    let total_gb = disk.total_space as f64 / (1024.0 * 1024.0 * 1024.0);
    let available_gb = disk.available_space as f64 / (1024.0 * 1024.0 * 1024.0);
    let used_gb = total_gb - available_gb;
    
    let os_info = format!("{} {} {}", system_info.os_name, system_info.os_version, system_info.architecture);
    let status_text = if forced { "Forced alert - below " } else { "Below " };
    
    let body = format!(
        "Disk Space Alert\n\n\
         System: {}\n\
         Hostname: {}\n\
         Disk: {} ({})\n\
         File System: {}\n\n\
         Disk Usage:\n\
         - Total Space: {:.2} GB\n\
         - Used Space: {:.2} GB\n\
         - Available Space: {:.2} GB\n\
         - Free Space: {:.2}%\n\n\
         Status: {}threshold\n\n\
         This is an automated alert from DiskMon-Mail.",
        os_info,
        system_info.hostname,
        disk.display_name,
        disk.mount_point,
        disk.file_system,
        total_gb,
        used_gb,
        available_gb,
        disk.free_space_percent,
        status_text
    );
    
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
    
    println!("{} Alert sent for {}{}", 
             "SUCCESS".green().bold(), 
             disk.display_name.cyan(), 
             if forced { " (forced)".yellow() } else { "".normal() });
    Ok(())
}

fn main() {
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

    // Get all monitored disks
    let disks = get_monitored_disks();
    
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
            "red" => disk.free_space_percent.to_string().red().bold(),
            "yellow" => disk.free_space_percent.to_string().yellow().bold(),
            _ => disk.free_space_percent.to_string().green().bold(),
        };
        
        let colored_icon = match status_color {
            "red" => status_icon.red().bold(),
            "yellow" => status_icon.yellow().bold(),
            _ => status_icon.green().bold(),
        };
        
        println!("  {} {}: {}% free ({:.2} GB available, {} filesystem)", 
                 colored_icon,
                 disk.display_name.cyan(), 
                 colored_percent,
                 disk.available_space as f64 / (1024.0 * 1024.0 * 1024.0),
                 disk.file_system.magenta());
    }

    // Handle email alerts
    let threshold = cfg.threshold_percent.unwrap_or(10.0); // Default to 10% if not specified
    let mut alerts_sent = 0;
    let mut errors_occurred = false;
    
    if cli.force_mail {
        // Force send alert for all disks
        println!("\n{}", "Forced mail mode: Sending alerts for all disks...".yellow().bold());
        for disk in &disks {
            if let Err(e) = send_disk_alert(&cfg, disk, &system_info, true) {
                eprintln!("{} {}: {}", "ERROR Failed to send alert for".red().bold(), disk.display_name.cyan(), e);
                errors_occurred = true;
            } else {
                alerts_sent += 1;
            }
        }
    } else {
        // Check each disk against threshold
        let mut low_space_disks = Vec::new();
        
        for disk in &disks {
            if disk.free_space_percent < threshold {
                low_space_disks.push(disk);
            }
        }
        
        if !low_space_disks.is_empty() {
            println!("\n{} {} disk(s):", 
                     "Disk space alerts triggered for".red().bold(), 
                     low_space_disks.len().to_string().red().bold());
            for disk in &low_space_disks {
                println!("  {} {}: {:.2}% free (below {:.1}% threshold)", 
                         "!".red().bold(),
                         disk.display_name.cyan(), 
                         disk.free_space_percent.to_string().red().bold(), 
                         threshold);
                
                if let Err(e) = send_disk_alert(&cfg, disk, &system_info, false) {
                    eprintln!("{} {}: {}", "ERROR Failed to send alert for".red().bold(), disk.display_name.cyan(), e);
                    errors_occurred = true;
                } else {
                    alerts_sent += 1;
                }
            }
        } else {
            println!("\n{} (above {:.1}% threshold).", 
                     "All disks have sufficient free space".green().bold(), 
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
