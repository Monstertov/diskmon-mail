use std::fs;
use std::path::Path;
use serde::Deserialize;

const CONFIG_PATH: &str = "config.yaml";

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

pub fn load_default_config() -> Result<Config, String> {
    load_config(CONFIG_PATH)
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