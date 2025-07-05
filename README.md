# DiskMon-Mail

[![GitHub Release](https://img.shields.io/github/v/release/Monstertov/diskmon-mail?style=flat-square)](https://github.com/Monstertov/diskmon-mail/releases)
[![Rust](https://custom-icon-badges.demolab.com/badge/Rust-000000?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Windows](https://custom-icon-badges.demolab.com/badge/Windows-0078D6?logo=microsoft&logoColor=white)](https://www.microsoft.com/windows)
[![Linux](https://custom-icon-badges.demolab.com/badge/Linux-FFFFFF?logo=linux&logoColor=black)](https://linuxfoundation.org/)
[![ARM](https://custom-icon-badges.demolab.com/badge/ARM-0091BD?logo=arm&logoColor=white)](https://www.arm.com/)

A lightweight, cross-platform disk space monitoring tool that sends email alerts when disk space falls below a configurable threshold and includes optional disk health monitoring. Perfect for system administrators who need automated disk space monitoring and health status across Windows, Linux, and ARM-based systems.

## What It Does

- Monitors all local disks (excluding USB drives and network mounts)
- Checks available disk space against your configured threshold
- Provides detailed system information in alerts
- Works silently in the background
- Gathers SMART status information (may take a few seconds on Windows)
- Sends email alerts when disk space drops below the threshold or when disk errors are detected


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
```
> **Note on SMART Status**: The ability to read SMART status is not guaranteed and depends on the disk, controller, and operating system. On Linux, the tool first tries to use `smartctl` (smartmontools) if available, then falls back to built-in kernel interfaces. On Windows, it uses PowerShell and WMI. The tool does not require external dependencies but will use them if available for better accuracy. On RAID arrays, SMART status may not be accurate. The tool may take a few seconds to gather SMART information, especially on Windows systems. See the [Enhanced Disk Health Monitoring (Optional)](#enhanced-disk-health-monitoring-optional) section for more details.

## Configuration

The `config.yaml` file contains all your settings:

```yaml
mail_enabled: true
smtp_server: smtp.gmail.com
smtp_port: 587
smtp_user: your-email@gmail.com
smtp_pass: your-app-password
email_from: your-email@gmail.com
email_to: admin@yourcompany.com
smtp_security: starttls # options: none, starttls, ssl

# Disk Monitoring Configuration
threshold_percent: 10.0 # Alert when disk space drops below 10%
send_mail_on_unknown_status: false # Send email if SMART status is unknown

# Health Check Configuration
health_check_enabled: true # Enable/disable disk health checks (default: true)
```

### Configuration Options

- **mail_enabled**: Set to `false` for test mode (no emails sent)
- **smtp_server**: Your SMTP server address
- **smtp_port**: SMTP port (587 for STARTTLS, 465 for SSL, 25 for none)
- **smtp_user/smtp_pass**: Authentication credentials (can be empty for open relays)
- **email_from**: Sender email address
- **email_to**: Recipient email address
- **smtp_security**: Security method (none, starttls, ssl)
- **threshold_percent**: Disk space threshold (default: 10.0%)
- **send_mail_on_unknown_status**: Send email if SMART status is unknown (default: false)
- **health_check_enabled**: Enable/disable disk health checks (default: true)

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
# Run daily at midnight
0 0 * * * /path/to/diskmon-mail

# Run every hour
0 * * * * /path/to/diskmon-mail

# Run every 30 minutes
*/30 * * * * /path/to/diskmon-mail
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
OnCalendar=*-*-* 00:00:00
Persistent=true

[Install]
WantedBy=timers.target
```

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

This will send test emails for all disks regardless of available space.

### Common Issues

1. **"Configuration error"**: Check that `config.yaml` exists in the same directory as the executable
2. **"SMTP error"**: Verify your SMTP server settings and credentials
3. **"No monitored disks found"**: Ensure you have local disks mounted
4. **Permission denied**: Run with appropriate permissions (admin/root if needed)

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
- **Performance**: Initial disk information gathering may take 2-5 seconds, especially on Windows systems with multiple drives
- **Optional**: smartmontools for enhanced disk health monitoring (not required but recommended)

## Security Notes

- Store `config.yaml` securely - it contains email credentials
- Use app passwords for Gmail/Office 365 instead of regular passwords
- Consider using environment variables for sensitive data in production
- The tool only reads disk information and sends emails - no data collection

## Support

For issues, feature requests, or contributions:
- Review the example configuration file
- Test SMTP settings with the `--force-mail` parameter

---

**DiskMon-Mail** - Simple, reliable disk space monitoring and health status for system administrators.
