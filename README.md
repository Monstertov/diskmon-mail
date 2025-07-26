# DiskMon-Mail

[![GitHub Release](https://img.shields.io/github/v/release/Monstertov/diskmon-mail?style=flat-square)](https://github.com/Monstertov/diskmon-mail/releases)
[![Rust](https://custom-icon-badges.demolab.com/badge/Rust-000000?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Windows](https://custom-icon-badges.demolab.com/badge/Windows-0078D6?logo=microsoft&logoColor=white)](https://www.microsoft.com/windows)
[![Linux](https://custom-icon-badges.demolab.com/badge/Linux-FFFFFF?logo=linux&logoColor=black)](https://linuxfoundation.org/)
[![ARM](https://custom-icon-badges.demolab.com/badge/ARM-0091BD?logo=arm&logoColor=white)](https://www.arm.com/)

A lightweight, cross-platform disk space monitoring tool that sends email alerts when disk space falls below a configurable threshold and includes optional disk health monitoring. Perfect for system administrators who need automated disk space monitoring and health status across Windows, Linux, and ARM-based systems.

## What It Does

- **Fast Disk Monitoring**: Monitors all local disks (excluding USB drives and network mounts) with parallel health checks for speed
- **Smart Alerting**: Checks available disk space against your configured threshold with automatic SMTP retry for reliable delivery
- **Comprehensive Reports**: Provides detailed system information and disk health status in alerts
- **Background Operation**: Works silently in the background with configurable timeouts to prevent hanging
- **Enhanced SMART Monitoring**: Gathers SMART status information quickly using parallel processing (significant speed improvement in v0.3.0)
- **Reliable Email Delivery**: Automatic retry logic ensures critical alerts reach you even during temporary network issues
- **Monitoring Integration**: JSON output mode for seamless integration with monitoring systems (Nagios, Zabbix, Prometheus, etc.)


## Quick Start

### 1. Download the Binary

Download the appropriate binary for your system from the [GitHub releases page](https://github.com/Monstertov/diskmon-mail/releases).

**Available platforms:**
- **Windows**: `diskmon-mail-windows-x86_64.zip`
- **Linux x86_64**: `diskmon-mail-linux-x86_64.zip`
- **Linux ARM64**: `diskmon-mail-linux-aarch64.zip`
- **Linux ARM32**: `diskmon-mail-linux-armv7.zip`
- **Linux ARM**: `diskmon-mail-linux-arm.zip`

### 2. Set Up Configuration

1. Extract the downloaded zip file
2. Copy `config.example.yaml` to the same directory as the executable
3. Rename it to `config.yaml`
4. Edit the configuration file with your settings

### 3. Test Your Setup

```bash
# Test SMTP settings (sends email regardless of disk space)
./diskmon-mail --force-mail

# Normal run (only sends alerts if disk space is low)
./diskmon-mail

# Display SMART status for all disks
./diskmon-mail --smart

# Machine-readable output for monitoring systems
./diskmon-mail --json

# Custom timeout for SMART collection (useful for slow drives)
./diskmon-mail --smart-timeout 60
```
> **Note on SMART Status**: The ability to read SMART status is not guaranteed and depends on the disk, controller, and operating system. On Linux, the tool first tries to use `smartctl` (smartmontools) if available, then falls back to built-in kernel interfaces. On Windows, it uses PowerShell and WMI. The tool does not require external dependencies but will use them if available for better accuracy. On RAID arrays, SMART status may not be accurate. The tool may take a few seconds to gather SMART information, especially on Windows systems. See the [Enhanced Disk Health Monitoring (Optional)](#enhanced-disk-health-monitoring-optional) section for more details.

## Configuration

The `config.yaml` file controls all monitoring and alerting behavior. As a system administrator, you use this file to:
- Set up email notifications for disk space and health alerts
- Define which SMTP server and credentials to use
- Specify which disks to monitor or exclude
- Set the disk space threshold for alerts
- Enable or disable health checks and SMART-based alerts
- Optionally set a friendly name for the system in reports

**Example Configuration:**

```yaml
# Enable or disable email alerts
mail_enabled: true
# SMTP server address
smtp_server: smtp.example.com
# SMTP server port
smtp_port: 587
# SMTP username (leave blank if not required)
smtp_user: user@example.com
# SMTP password (leave blank if not required)
smtp_pass: password
# Sender email address
email_from: admin@example.com
# Recipient email address
email_to: alerts@example.com
# SMTP security: none, starttls, or ssl
smtp_security: starttls
# Alert if free space is below this percent (1.0-100.0)
threshold_percent: 10.0
# Send mail if SMART status is unknown
send_mail_on_unknown_status: false
# List of disks to exclude (Linux: e.g. ["sda", "nvme0n1"]; Windows: e.g. ["C:", "D:"]). Empty values are ignored.
excluded_disks: [""]
# Enable disk health checks (disable to only check free space)
health_check_enabled: true
# Enable SMART-based alerts (disable to ignore SMART failures)
smart_enabled: true
# Optional: friendly name for this device in reports
friendly_name: "Example device"
```

### Configuration Options (Explained)

- **mail_enabled**: Enables or disables email notifications for disk alerts.
- **smtp_server / smtp_port**: The SMTP server and port used to send alert emails.
- **smtp_user / smtp_pass**: Credentials for SMTP authentication (leave blank if not required).
- **email_from / email_to**: The sender and recipient email addresses for alerts.
- **smtp_security**: Security protocol for SMTP (`none`, `starttls`, or `ssl`).
- **threshold_percent**: The minimum free disk space percentage before an alert is sent (1.0–100.0).
- **send_mail_on_unknown_status**: If `true`, sends an alert even if disk health (SMART) status is unknown.
- **excluded_disks**: List of disks to exclude from monitoring (by device name or drive letter).
- **health_check_enabled**: Enables or disables disk health checks (if `false`, only free space is monitored).
- **smart_enabled**: Enables or disables SMART-based alerts (if `false`, SMART failures are ignored).
- **friendly_name**: (Optional) Custom name for this system in alert emails (useful for identifying multiple systems).

**Tip:** All options are documented in the example config. Only change what you need for your environment.

### Secure Credential Management (New in v0.3.0)

For enhanced security, you can store SMTP credentials outside the configuration file using environment variables:

```bash
# Set environment variables (Linux/macOS)
export DISKMON_SMTP_USER="your-email@domain.com"
export DISKMON_SMTP_PASS="your-app-password"
export DISKMON_EMAIL_FROM="monitoring@yourdomain.com"
export DISKMON_EMAIL_TO="admin@yourdomain.com"

# Then run diskmon-mail (credentials will override config file values)
./diskmon-mail
```

```cmd
# Set environment variables (Windows)
set DISKMON_SMTP_USER=your-email@domain.com
set DISKMON_SMTP_PASS=your-app-password
set DISKMON_EMAIL_FROM=monitoring@yourdomain.com
set DISKMON_EMAIL_TO=admin@yourdomain.com

# Then run diskmon-mail
diskmon-mail.exe
```

This approach keeps sensitive credentials out of configuration files and supports modern security practices.

## New in Version 0.3.0 - Performance & Reliability Improvements

### Faster Execution
- **Parallel SMART Collection**: Disk health checks now run simultaneously instead of one-by-one, reducing scan time from 30+ seconds to under 10 seconds on multi-drive systems
- **Configurable Timeouts**: Use `--smart-timeout N` to prevent hanging on unresponsive drives (default: 30 seconds)
- **Efficient for Regular Monitoring**: Now fast enough for daily or even twice-daily monitoring without performance concerns

### More Reliable Alerts
- **Automatic SMTP Retry**: Failed email deliveries automatically retry with smart backoff (up to 3 attempts)
- **Network Resilience**: Temporary network issues no longer cause missed critical alerts
- **Better Error Recovery**: Enhanced error handling prevents crashes on transient issues

### Enhanced Security
- **Environment Variable Support**: Store SMTP credentials securely outside config files
- **Permission Warnings**: Alerts when config files have overly permissive permissions
- **Improved TLS**: Enhanced certificate validation for secure email delivery

### Monitoring System Integration
- **JSON Output**: Use `--json` for machine-readable output compatible with:
  - Nagios/Icinga monitoring systems
  - Zabbix infrastructure monitoring
  - Prometheus metrics collection
  - Custom monitoring dashboards
- **Structured Data**: Complete system and disk information in JSON format
- **Alert Classification**: Clear separation of disk space and SMART health alerts

### System Administrator Benefits
- **Drop-in Upgrade**: 100% backward compatible - existing configurations work unchanged
- **Faster Response**: Significantly reduced execution time for better user experience
- **Fewer Missed Alerts**: SMTP retry logic ensures critical notifications reach you
- **Better Integration**: JSON output enables seamless monitoring system integration
- **Enhanced Debugging**: Improved logging and error messages for easier troubleshooting

## Automation Examples

### Windows - Scheduled Task

1. Open Task Scheduler
2. Create Basic Task
3. Set trigger to Daily at 12:00 AM
4. Action: Start a program
5. Program: `C:\path\to\diskmon-mail.exe`
6. Start in: `C:\path\to\` (directory containing config.yaml)

**Command Line:**
```cmd
schtasks /create /tn "DiskMon-Mail" /tr "C:\path\to\diskmon-mail.exe" /sc daily /st 00:00 /f
```

### Windows - Run on Boot (Recommended for Desktop Users)

For desktop users who don't leave their computer running overnight, running DiskMon-Mail on system boot is more effective than scheduled tasks.

#### Method 1: Task Scheduler (Recommended)

1. **Open Task Scheduler** (search for "Task Scheduler" in Start menu)
2. **Create Basic Task**:
   - Name: `DiskMon-Mail Boot Check`
   - Description: `Check disk space on system startup`
3. **Trigger**: Select "When the computer starts"
4. **Action**: Start a program
5. **Program/script**: `C:\path\to\diskmon-mail.exe`
6. **Start in**: `C:\path\to\` (directory containing config.yaml)
7. **Finish**: Check "Open the Properties dialog" and click Finish
8. **Properties**:
   - **General tab**: Check "Run whether user is logged on or not"
   - **Conditions tab**: Uncheck "Start the task only if the computer is on AC power"
   - **Settings tab**: Check "Allow task to be run on demand"

**Command Line (Run as Administrator):**
```cmd
schtasks /create /tn "DiskMon-Mail Boot Check" /tr "C:\path\to\diskmon-mail.exe" /sc onstart /ru "SYSTEM" /f
```

#### Method 2: Startup Folder

1. Press `Win + R`, type `shell:startup`, and press Enter
2. Create a shortcut to `diskmon-mail.exe` in the startup folder
3. Right-click the shortcut → Properties
4. In "Start in" field, enter the directory containing `config.yaml`

**Note**: This method runs when the user logs in, not when the system boots.

#### Method 3: Registry (Advanced)

1. Press `Win + R`, type `regedit`, and press Enter
2. Navigate to: `HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows\CurrentVersion\Run`
3. Create a new String Value named `DiskMon-Mail`
4. Set the value to: `"C:\path\to\diskmon-mail.exe"`

**Command Line (Run as Administrator):**
```cmd
reg add "HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Run" /v "DiskMon-Mail" /t REG_SZ /d "C:\path\to\diskmon-mail.exe" /f
```

### Linux - Cron Job

Add to crontab (`crontab -e`):

```bash
# Run daily at midnight (recommended)
0 0 * * * /path/to/diskmon-mail

# Run weekly on Sundays at 2 AM
0 2 * * 0 /path/to/diskmon-mail

# Run twice daily (morning and evening)
0 8,20 * * * /path/to/diskmon-mail

# Run with custom timeout for slow systems
0 0 * * * /path/to/diskmon-mail --smart-timeout 60
```

### Systemd Service (Linux)

Create `/etc/systemd/system/diskmon-mail.service`:

```ini
[Unit]
Description=DiskMon-Mail Service
After=network.target

[Service]
Type=oneshot
ExecStart=/path/to/diskmon-mail
User=root
WorkingDirectory=/path/to/

[Install]
WantedBy=multi-user.target
```

Create `/etc/systemd/system/diskmon-mail.timer`:

```ini
[Unit]
Description=Run DiskMon-Mail daily
Requires=diskmon-mail.service

[Timer]
OnCalendar=daily
Persistent=true

[Install]
WantedBy=timers.target
```

For weekly monitoring, use `OnCalendar=weekly` instead.

Enable and start:
```bash
sudo systemctl enable diskmon-mail.timer
sudo systemctl start diskmon-mail.timer
```

## What It Monitors

DiskMon-Mail automatically detects and monitors:
- **Windows**: All local drives (C:, D:, etc.) excluding network drives
- **Linux**: All mounted filesystems excluding removable media (/media/, /mnt/, etc.)
- **File Systems**: NTFS, ext4, ext3, xfs, and others
- **Threshold**: Configurable percentage (default: 10% free space)

The tool skips:
- USB drives and removable media
- Network drives and mounted shares
- CD/DVD drives
- Temporary filesystems

## Troubleshooting

### Test SMTP Settings

If emails aren't being sent, test your SMTP configuration:

```bash
./diskmon-mail --force-mail
```

This will send test emails for all disks regardless of available space. In v0.3.0, failed email deliveries will automatically retry up to 3 times with smart backoff timing.

### Common Issues

1. **"Configuration error"**: Check that `config.yaml` exists in the same directory as the executable
2. **"SMTP error"**: Verify your SMTP server settings and credentials (v0.3.0 includes automatic retry for transient issues)
3. **"No monitored disks found"**: Ensure you have local disks mounted
4. **Permission denied**: Run with appropriate permissions (admin/root if needed)
5. **"SMART collection timed out"**: Use `--smart-timeout 60` for slower systems or drives (new in v0.3.0)
6. **Slow execution**: Upgrade to v0.3.0 for significantly faster parallel SMART collection

### Debug Mode

For detailed output, check the console output for:
- System information
- Disk detection results
- SMTP connection status
- Email sending confirmation

## Enhanced Disk Health Monitoring (Optional)

For more accurate disk health monitoring, you can install **smartmontools**:

### Linux Installation

**Debian/Ubuntu:**
```bash
sudo apt-get update
sudo apt-get install smartmontools
```

**CentOS/RHEL/Rocky/AlmaLinux:**
```bash
sudo yum install smartmontools
# or for newer versions:
sudo dnf install smartmontools
```

**Fedora:**
```bash
sudo dnf install smartmontools
```

**Arch Linux:**
```bash
sudo pacman -S smartmontools
```

**openSUSE:**
```bash
sudo zypper install smartmontools
```

### Windows Installation

1. Download the Windows installer from: https://www.smartmontools.org/wiki/Download#InstalltheWindowspackage
2. Run the `setup.exe` installer
3. Install to the default location: `C:\Program Files\smartmontools`
4. The tool will automatically detect and use smartctl.exe for enhanced disk monitoring

### Raspberry Pi / SD Card Monitoring

For Raspberry Pi systems with SD cards, smartmontools provides limited support, but the tool includes specialized MMC/SD card health detection that:
- Checks system logs (dmesg) for I/O errors
- Reads manufacturer information from the kernel
- Detects CRC errors and timeouts
- Provides SD card-specific health status

## System Requirements

- **Windows**: Windows 7 or later
- **Linux**: Most distributions (glibc-based)
- **ARM**: Raspberry Pi, ARM servers, embedded systems
- **Memory**: Minimal (typically < 10MB RAM)
- **Network**: Internet access for SMTP (if using external email)
- **Performance**: v0.3.0 significantly improved - disk information gathering now typically under 10 seconds even on Windows systems with multiple drives
- **Optional**: smartmontools for enhanced disk health monitoring (not required but recommended)

## Security Notes

- **v0.3.0 Enhancement**: Use environment variables for SMTP credentials instead of storing them in config files
- Store `config.yaml` securely if it contains email credentials (v0.3.0 warns about overly permissive permissions)
- Use app passwords for Gmail/Office 365 instead of regular passwords
- **Recommended**: Use the new environment variable support for sensitive data in production
- The tool only reads disk information and sends emails - no data collection or external reporting

## Support

For issues, feature requests, or contributions:
- Review the example configuration file
- Test SMTP settings with the `--force-mail` parameter
- Test disk health info with the `--smart` parameter

---

**DiskMon-Mail** - Simple, reliable disk space monitoring and health status for system administrators.
