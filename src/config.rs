use std::fs;
use std::path::Path;

pub const CONFIG_PATH: &str = "config.yaml";

#[derive(serde::Deserialize)]
pub struct Config {
    pub mail_enabled: bool,
    pub smtp_server: String,
    pub smtp_port: u16,
    pub smtp_user: String,
    pub smtp_pass: String,
    pub email_from: String,
    pub email_to: String,
    pub smtp_security: Option<String>, // "none", "starttls", "ssl"
    pub threshold_percent: Option<f64>, // Disk space threshold percentage
    pub send_mail_on_unknown_status: Option<bool>,
    pub debug: Option<bool>, // Enable debug output
    pub health_check_enabled: Option<bool>, // Enable/disable disk health checks (default: true)
    pub smart_enabled: Option<bool>, // Enable/disable SMART-based alerts (default: true)
    pub friendly_name: Option<String>, // New: single friendly name
}

pub fn load_config<P: AsRef<Path>>(path: P) -> Result<Config, String> {
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
    let mut warnings = Vec::new();
    
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
        missing_keys.push("smtp_port (must be 1-65535)");
    }
    
    // Validate threshold_percent if provided
    if let Some(threshold) = config.threshold_percent {
        if threshold < 1.0 || threshold > 100.0 {
            missing_keys.push("threshold_percent (must be between 1.0 and 100.0)");
        }
    }
    
    // Validate smtp_security
    if let Some(ref sec) = config.smtp_security {
        let sec = sec.to_lowercase();
        if sec != "none" && sec != "starttls" && sec != "ssl" {
            missing_keys.push("smtp_security (must be one of: none, starttls, ssl)");
        }
        if sec == "none" {
            warnings.push("SMTP security is set to 'none'. This is insecure and not recommended.");
        }
    }
    
    // Validate email addresses (basic check)
    if !config.email_from.contains('@') {
        missing_keys.push("email_from (must be a valid email address)");
    }
    if !config.email_to.contains('@') {
        missing_keys.push("email_to (must be a valid email address)");
    }
    
    // Warn if debug is enabled
    if config.debug.unwrap_or(false) {
        warnings.push("Debug mode is enabled. This may expose sensitive information in logs.");
    }
    
    // Warn if health checks are disabled
    if config.health_check_enabled == Some(false) {
        warnings.push("Disk health checks are disabled. Only free space will be monitored.");
    }
    
    // Warn if send_mail_on_unknown_status is enabled
    if config.send_mail_on_unknown_status == Some(true) {
        warnings.push("send_mail_on_unknown_status is enabled. Emails will be sent even if SMART status is unknown.");
    }
    
    if !missing_keys.is_empty() {
        return Err(format!("Missing or invalid required configuration keys: {}", missing_keys.join(", ")));
    }
    if !warnings.is_empty() {
        eprintln!("[CONFIG WARNING] {}", warnings.join(" | "));
    }
    Ok(())
}
