// Author: Monstertov
// Purpose: Cross-platform disk space monitor and email alert tool (Rust version of diskmon.py)

mod platform;
mod disk;
mod email;
mod config;

use clap::Parser;
use colored::*;
use crate::config::{Config, load_default_config};
use crate::disk::{DiskInfo, SystemInfo, get_system_info, get_monitored_disks};
use crate::email::send_system_report;

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

fn main() {
    // Initialize color support based on terminal capabilities
    init_colors();
    
    let cli = Cli::parse();

    // Load and validate configuration
    let cfg = match load_default_config() {
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
