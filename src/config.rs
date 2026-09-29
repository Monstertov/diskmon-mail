use std::fs;
use std::path::Path;
use std::env;
use lettre::message::Mailboxes;
use serde::{Deserialize, Deserializer, de::Error as _};

pub const CONFIG_PATH: &str = "config.yaml";

#[derive(serde::Deserialize, Debug)]
pub struct Config {
    pub mail_enabled: bool,
    pub smtp_server: String,
    pub smtp_port: u16,
    pub smtp_user: String,
    pub smtp_pass: String,
    pub email_from: String,
    #[serde(deserialize_with = "deserialize_recipients")]
    pub email_to: Vec<String>, // One address, a comma-separated string, or a YAML list
    pub smtp_security: Option<String>, // "none", "starttls", "ssl"
    pub threshold_percent: Option<f64>, // Disk space threshold percentage
    pub send_mail_on_unknown_status: Option<bool>,
    pub debug: Option<bool>, // Enable debug output
    pub health_check_enabled: Option<bool>, // Enable/disable disk health checks (default: true)
    pub smart_enabled: Option<bool>, // Enable/disable SMART-based alerts (default: true)
    pub friendly_name: Option<String>, // New: single friendly name
    pub excluded_disks: Option<Vec<String>>, // List of disks to exclude (drive letters or device names)
}

/// Accepts `email_to` as a single string (old format, may be comma-separated) or a YAML list of strings.
fn deserialize_recipients<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    use serde_yaml::Value;
    let bad = || D::Error::custom("expected an email address or a list of email addresses");
    match Value::deserialize(d)? {
        Value::String(s) => Ok(vec![s]),
        Value::Sequence(items) => items
            .into_iter()
            .map(|v| v.as_str().map(String::from).ok_or_else(bad))
            .collect(),
        _ => Err(bad()),
    }
}

/// Parses all `email_to` entries into mailboxes. Each entry may hold several comma-separated addresses.
pub fn parse_recipients(entries: &[String]) -> Result<Mailboxes, String> {
    let mut all = Mailboxes::new();
    for entry in entries.iter().map(|e| e.trim()).filter(|e| !e.is_empty()) {
        let parsed: Mailboxes = entry.parse().map_err(|e| format!("invalid address '{entry}' ({e}); separate multiple addresses with commas"))?;
        for mailbox in parsed {
            all.push(mailbox);
        }
    }
    if all.iter().next().is_none() {
        return Err("no recipient address".to_string());
    }
    Ok(all)
}

pub fn load_config<P: AsRef<Path>>(path: P) -> Result<Config, String> {
    // Check if config file exists
    if !path.as_ref().exists() {
        return Err(format!("Configuration file not found: {}", path.as_ref().display()));
    }
    
    // Check file permissions on Unix systems
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = fs::metadata(&path) {
            let permissions = metadata.permissions();
            let mode = permissions.mode();
            // Check if file is readable by group or others (world-readable)
            if mode & 0o044 != 0 {
                eprintln!("[SECURITY WARNING] Configuration file {} has overly permissive permissions (readable by group/others). Consider: chmod 600 {}", 
                    path.as_ref().display(), path.as_ref().display());
            }
        }
    }
    
    // Read config file
    let data = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read config file: {e}"))?;
    
    // Parse YAML
    let config: Config = serde_yaml::from_str(&data)
        .map_err(|e| format!("Failed to parse config YAML: {e}"))?;
    
    // Validate required fields
    validate_config(&config)?;
    
    // Apply environment variable overrides for sensitive data
    let config = apply_env_overrides(config);
    
    Ok(config)
}

fn apply_env_overrides(mut config: Config) -> Config {
    // Override SMTP credentials from environment variables if available
    if let Ok(smtp_user) = env::var("DISKMON_SMTP_USER") {
        if !smtp_user.trim().is_empty() {
            config.smtp_user = smtp_user;
        }
    }
    
    if let Ok(smtp_pass) = env::var("DISKMON_SMTP_PASS") {
        if !smtp_pass.trim().is_empty() {
            config.smtp_pass = smtp_pass;
        }
    }
    
    if let Ok(email_from) = env::var("DISKMON_EMAIL_FROM") {
        if !email_from.trim().is_empty() {
            config.email_from = email_from;
        }
    }
    
    if let Ok(email_to) = env::var("DISKMON_EMAIL_TO") {
        if !email_to.trim().is_empty() {
            config.email_to = vec![email_to];
        }
    }
    
    config
}

fn validate_config(config: &Config) -> Result<(), String> {
    let mut missing_keys: Vec<String> = Vec::new();
    let mut warnings = Vec::new();
    
    // Check for empty required string fields (except smtp_user and smtp_pass)
    if config.smtp_server.trim().is_empty() {
        missing_keys.push("smtp_server".to_string());
    }
    if config.email_from.trim().is_empty() {
        missing_keys.push("email_from".to_string());
    }
    let email_to_empty = config.email_to.iter().all(|e| e.trim().is_empty());
    if email_to_empty {
        missing_keys.push("email_to".to_string());
    }
    
    // Check port is valid
    if config.smtp_port == 0 {
        missing_keys.push("smtp_port (must be 1-65535)".to_string());
    }
    
    // Validate threshold_percent if provided
    if let Some(threshold) = config.threshold_percent {
        if threshold < 1.0 || threshold > 100.0 {
            missing_keys.push("threshold_percent (must be between 1.0 and 100.0)".to_string());
        }
    }
    
    // Validate smtp_security
    if let Some(ref sec) = config.smtp_security {
        let sec = sec.to_lowercase();
        if sec != "none" && sec != "starttls" && sec != "ssl" {
            missing_keys.push("smtp_security (must be one of: none, starttls, ssl)".to_string());
        }
        if sec == "none" {
            warnings.push("SMTP security is set to 'none'. This is insecure and not recommended.".to_string());
        }
    }
    
    // Validate email addresses (basic check)
    if !config.email_from.contains('@') {
        missing_keys.push("email_from (must be a valid email address)".to_string());
    }
    if !email_to_empty {
        if let Err(e) = parse_recipients(&config.email_to) {
            // Only fatal when mail is sent; with mail disabled the old check (just an '@') was enough.
            if config.mail_enabled {
                missing_keys.push(format!("email_to ({e})"));
            } else {
                warnings.push(format!("email_to: {e}"));
            }
        }
    }
    
    // Warn if debug is enabled
    if config.debug.unwrap_or(false) {
        warnings.push("Debug mode is enabled. This may expose sensitive information in logs.".to_string());
    }
    
    // Warn if health checks are disabled
    if config.health_check_enabled == Some(false) {
        warnings.push("Disk health checks are disabled. Only free space will be monitored.".to_string());
    }
    
    // Warn if send_mail_on_unknown_status is enabled
    if config.send_mail_on_unknown_status == Some(true) {
        warnings.push("send_mail_on_unknown_status is enabled. Emails will be sent even if SMART status is unknown.".to_string());
    }
    
    // Validate excluded_disks
    if let Some(ref excluded) = config.excluded_disks {
        for disk in excluded {
            if disk.trim().is_empty() {
                continue; // Ignore empty values
            }
            if cfg!(windows) {
                // Should be like "C:", "D:", etc.
                if !(disk.len() == 2 && disk.chars().nth(1) == Some(':')) {
                    warnings.push(format!("Invalid excluded disk '{}': must be a drive letter like 'C:'", disk));
                }
            } else {
                // Should be like "sda", "nvme0n1", etc.
                if disk.contains('/') || disk.is_empty() {
                    warnings.push(format!("Invalid excluded disk '{}': must be a device name like 'sda' or 'nvme0n1'", disk));
                }
            }
        }
    }
    
    if !missing_keys.is_empty() {
        return Err(format!("Missing or invalid required configuration keys: {}", missing_keys.join(", ")));
    }
    if !warnings.is_empty() {
        eprintln!("[CONFIG WARNING] {}", &warnings.join(" | "));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lettre::{Message, message::header::To};

    fn cfg_with(email_to: &str) -> Result<Config, serde_yaml::Error> {
        serde_yaml::from_str(&format!(
            "mail_enabled: true\nsmtp_server: smtp.example.com\nsmtp_port: 587\nsmtp_user: \"\"\nsmtp_pass: \"\"\nemail_from: a@example.com\nemail_to: {email_to}\n"
        ))
    }

    fn count(email_to: &str) -> usize {
        let cfg = cfg_with(email_to).unwrap();
        parse_recipients(&cfg.email_to).unwrap().iter().count()
    }

    #[test]
    fn email_to_formats() {
        assert_eq!(count("alerts@example.com"), 1); // pre-0.5.0 format
        assert_eq!(count("\"a@example.com, b@example.com\""), 2);
        assert_eq!(count("a@example.com,b@example.com"), 2);
        assert_eq!(count("[a@example.com, b@example.com, c@example.com]"), 3);
        assert_eq!(count("\n  - a@example.com\n  - b@example.com, c@example.com"), 3);
        assert_eq!(count("\"Ops Team <ops@example.com>, b@example.com\""), 2);
    }

    #[test]
    fn email_to_rejects_bad_input() {
        assert!(cfg_with("42").is_err());
        assert!(cfg_with("[a@example.com, {x: 1}]").is_err());
        for bad in ["a@example.com, not-an-address", "a@example.com; b@example.com", "a@example.com b@example.com"] {
            let err = parse_recipients(&[bad.to_string()]);
            assert!(err.is_err(), "accepted {bad:?} as {:?}", err.map(|m| m.to_string()));
        }
        assert!(parse_recipients(&[" ".to_string()]).is_err());

        let cfg = cfg_with("\"a@example.com, broken\"").unwrap();
        assert!(validate_config(&cfg).is_err());
        let cfg = Config { mail_enabled: false, ..cfg };
        assert!(validate_config(&cfg).is_ok(), "mail disabled keeps running as before");
    }

    #[test]
    fn message_goes_to_every_recipient() {
        let to = parse_recipients(&["a@example.com, b@example.com".into(), "c@example.com".into()]).unwrap();
        let msg = Message::builder()
            .from("x@example.com".parse().unwrap())
            .mailbox(To::from(to))
            .subject("t")
            .body(String::new())
            .unwrap();
        assert_eq!(msg.envelope().to().len(), 3);
    }
}
