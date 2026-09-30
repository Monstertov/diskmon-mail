# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.5.1] - 2026-09-30

Audit release: bug and security fixes. No configuration changes required.

### Bug Fixes
- **Linux disk health checks never ran**: the device name was looked up as a mount point, so every Linux disk showed `SMART: N/A` even with smartmontools installed. The device is now resolved directly.
- **Failing disks were not reported**: smartctl signals a failing disk with a non-zero exit code (for example 8), and that output was thrown away. Output is now used unless smartctl reports a command line or device open error (Linux and Windows).
- **SMART attributes were always empty**: power-on hours, reallocated/pending/uncorrectable sectors and temperature are now read (`-A` was missing and the attribute table was parsed incorrectly). ATA and NVMe formats are supported.
- **Linux kernel fallback**: fixed the sysfs path (model, serial and vendor are now read), SD card partition names such as `mmcblk0p1`, and SD card manufacturer ids. Removed checks that read I/O timing counters as error counts and ran `fsck -n` on mounted filesystems. Without smartctl, a disk shows `WARNING` only when the kernel log has I/O errors for it; otherwise its status stays unknown (as before).
- **Linux `excluded_disks` did not work**: entries such as `sda` or `nvme0n1` never matched. Whole-disk names now exclude all their partitions, partition names exclude that partition.
- **Windows `excluded_disks`**: an entry without a colon, such as `D`, excluded every drive. It now only matches that drive.
- **Spurious warning** "The following excluded_disks were not found:" with the example config's `[""]`.
- **`--json` output was not valid JSON**: human-readable lines were printed before the JSON document. stdout now contains only JSON; debug output goes to stderr.
- **`--help` and `--version` needed a config file**.
- **Config next to the executable**: `config.yaml` is now also found next to the executable when it is not in the working directory, which is what the README and the cron examples assume.
- **Environment variable overrides**: `DISKMON_*` values are now applied before validation. Credentials and addresses can live only in the environment, and their values are validated at startup.
- **`--smart-timeout` did not end the run**: after a timeout the process waited for the hung disk check at exit. It now exits right away.
- **SMTP retries**: permanent errors (5xx, such as a rejected login or recipient) are no longer retried. Transient errors are still retried up to 3 times.
- `threshold_percent: .nan` was accepted; an empty `friendly_name` produced an empty device name in reports.

### Security
- Debug mode printed the SMTP password as part of the loaded configuration. It is now redacted.

### Maintenance
- Removed the unmaintained `backoff` crate and the unused `wmi` crate; trimmed `winapi` and `tokio` features. OpenSSL is only built for Linux (Windows uses the system TLS stack, as before).
- `Cargo.lock` is now committed for reproducible builds.
- Unit tests for recipient parsing, smartctl parsing, exit codes, partition names and kernel log matching.
- Documentation: Windows 10 or later is required (Rust dropped Windows 7/8 support).

## [0.5.0] - 2026-09-30

### Added
- **Multiple email recipients** (fixes #5): `email_to` now accepts several addresses, either comma-separated (`"a@example.com, b@example.com"`) or as a YAML list. Every recipient receives the report. The `DISKMON_EMAIL_TO` environment variable accepts a comma-separated list too.

### Changed
- Recipient addresses are checked when the configuration is loaded, so a typo is reported at startup (naming the bad address) instead of after the disk scan.

### Backward Compatibility
- Existing configurations with a single `email_to` address work unchanged.

## [0.4.1] - 2026-03-30

### Bug Fixes
- **Windows `excluded_disks` not working**: Drive letter exclusions (e.g. `H:`, `S:`) were silently ignored and the drives were still scanned and reported. The exclusion check now correctly matches drive letter entries against the Windows mount point format.

## [0.4.0] - 2026-03-30

### Code Quality
- **Critical & High Severity Fixes**: Addressed critical and high severity code quality issues identified during static analysis
- **Improved Error Handling**: Replaced panic-prone patterns with proper `Result` propagation throughout the codebase
- **Resource Management**: Fixed potential resource leaks and improved cleanup on error paths
- **Unsafe Code Reduction**: Reduced reliance on unsafe patterns and replaced with idiomatic safe Rust alternatives

## [0.3.0] - 2025-07-26

### Performance Improvements
- **Faster SMART Collection**: Disk health checks now run in parallel instead of sequentially, reducing scan time from 30+ seconds to under 10 seconds on systems with multiple drives
- **Configurable Timeouts**: Added `--smart-timeout` option to prevent hanging on unresponsive drives (default: 30 seconds)
- **Improved Responsiveness**: System administrators will notice significantly faster execution, especially on Windows systems with multiple drives

### Reliability Enhancements
- **SMTP Retry Logic**: Email delivery now automatically retries failed attempts with smart backoff (up to 3 attempts), reducing missed alerts due to temporary network issues
- **Better Error Recovery**: Enhanced error handling prevents the tool from crashing on transient issues
- **Graceful Timeouts**: No more infinite waits when drives become unresponsive

### Security & Configuration
- **Environment Variable Support**: SMTP credentials can now be stored securely outside config files using environment variables:
  - `DISKMON_SMTP_USER` - SMTP username
  - `DISKMON_SMTP_PASS` - SMTP password  
  - `DISKMON_EMAIL_FROM` - Sender email
  - `DISKMON_EMAIL_TO` - Recipient email
- **Configuration Security Check**: Warns system administrators when config.yaml has overly permissive file permissions on Unix systems
- **Enhanced TLS Validation**: Improved certificate validation for secure SMTP connections

### Monitoring Integration
- **JSON Output Mode**: New `--json` flag provides machine-readable output for integration with monitoring systems (Nagios, Zabbix, Prometheus, etc.)
- **Structured Logging**: Better debug information and logging for troubleshooting
- **Alert Details**: JSON output includes comprehensive disk status, system information, and active alerts

### Command Line Improvements
- **New Options**:
  - `--json` - Machine-readable output for monitoring systems
  - `--smart-timeout N` - Set SMART collection timeout in seconds (default: 30)
- **Better Debug Output**: Enhanced debugging information when debug mode is enabled
- **Improved Error Messages**: Clearer error reporting for configuration and runtime issues

### System Administrator Benefits
- **Faster Execution**: Significantly reduced execution time, especially beneficial for frequent monitoring
- **More Reliable Alerts**: SMTP retry logic ensures critical alerts reach administrators even during network hiccups  
- **Better Security**: Ability to externalize credentials from configuration files
- **Monitoring Integration**: Easy integration with existing monitoring infrastructure via JSON output
- **Improved Diagnostics**: Better logging and debug information for troubleshooting issues

### Backward Compatibility
- **100% Compatible**: All existing configurations continue to work without modification
- **No Breaking Changes**: All new features are opt-in via command-line flags or environment variables
- **Seamless Upgrade**: Drop-in replacement for previous versions

## [0.2.1] - 2025-07-10

### Changed
- **Configuration File Improvements**: The example config and documentation have been updated for clarity and accuracy. All options are now clearly documented for system administrators.
- **Documentation**: The README and config example now provide clear, end-user-focused explanations for each configuration option.

### Fixed
- Minor documentation and config validation improvements for better user experience.

## [0.2.0] - 2025-07-05

### Added
- **Windows Disk Status Support**: Full production-ready SMART status monitoring for Windows systems
- **Linux Disk Status Support**: Complete SMART status monitoring for Linux systems using hybrid approach (smartctl + kernel interfaces)
- **WMI Integration**: Proper mapping of logical drives to physical drives using Windows Management Instrumentation
- **Cross-Platform SMART Support**: Consistent SMART status monitoring across Windows and Linux platforms
- **Smartmontools**: Uses smartctl if available, falls back to kernel interfaces (dmesg, fsck, /proc/diskstats) or WMI

### Changed
- **Windows SMART Implementation**: Replaced placeholder with robust WMI-based SMART status detection
- **Linux SMART Implementation**: Replaced placeholder with hybrid smartctl + kernel-based SMART status detection
- **Drive Mapping**: Accurate mapping between drive letters (C:, D:, etc.) and physical drives
- **Mount Point Mapping**: Accurate mapping between mount points and device names on Linux
- **Documentation**: Updated README to reflect hybrid SMART status support approach

### Technical Improvements
- **WMI Associations**: Uses proper WMI associations (Win32_LogicalDiskToPartition, Win32_DiskDriveToDiskPartition)
- **Physical Drive Detection**: Maps logical drives to physical drives for accurate SMART data
- **Cross-Platform Compatibility**: Maintains existing Linux support while adding Windows functionality

## [0.1.0] - 2025-07-05

### Added
- **Core Functionality**: Cross-platform disk space monitoring tool
- **Email Alerts**: Automated email notifications when disk space falls below threshold
- **Multi-Platform Support**: Windows, Linux (x86_64, ARM64, ARM32), and ARM-based systems
- **Configuration System**: YAML-based configuration with comprehensive validation
- **SMTP Integration**: Support for various SMTP servers (Gmail, Office 365, custom servers)
- **CLI Interface**: Command-line interface with `--force-mail` testing option
- **System Information**: Detailed system and disk information in alerts
- **Colored Output**: Enhanced console output with status indicators
- **Disk Filtering**: Automatic exclusion of removable media and network drives
- **Threshold Configuration**: Configurable disk space threshold (default: 10%)
- **Security Options**: Support for none, STARTTLS, and SSL/TLS encryption
- **Test Mode**: Built-in SMTP testing capability for configuration validation
- **Cross-Compilation**: Support for multiple target architectures using Cross
- **Automated Build Script**: Comprehensive build process with error handling
- **Configuration Validation**: Detailed error messages for configuration issues

### Features
- **Cross-Platform Binary**: Single executable for each target platform
- **Lightweight**: Minimal resource usage (< 10MB RAM typical)
- **Automated**: Perfect for scheduled tasks, cron jobs, and systemd services
- **Configurable**: Customizable email settings, thresholds, and alert conditions
- **Robust Error Handling**: Comprehensive error messages and graceful failure handling
- **File System Support**: Works with NTFS, ext4, ext3, xfs, and other filesystems
- **Hostname Detection**: Automatic hostname inclusion in alert messages
- **Architecture Detection**: Automatic detection and reporting of system architecture

### Technical Implementation
- **Rust 2024 Edition**: Modern Rust with latest language features
- **Dependencies**: 
  - `sysinfo` for system and disk information
  - `lettre` for SMTP email functionality
  - `serde_yaml` for configuration parsing
  - `clap` for command-line argument parsing
  - `colored` for enhanced console output
  - `hostname` for system hostname detection
- **Cross-Compilation**: Support for multiple target architectures using Cross
- **Static Linking**: Self-contained binaries with minimal external dependencies
- **Configuration Validation**: Comprehensive validation of all configuration parameters
- **Error Reporting**: Detailed error messages for troubleshooting

### Documentation
- **README.md**: Comprehensive user documentation with examples
- **Configuration Guide**: Detailed configuration options and examples
- **Automation Examples**: Windows scheduled tasks, Linux cron jobs, and systemd services
- **Troubleshooting Guide**: Common issues and solutions
- **Security Notes**: Best practices for secure deployment
- **System Requirements**: Platform compatibility and requirements

### Build System
- **Cross-Platform Builds**: Automated builds for Windows, Linux, and ARM platforms
- **Build Script**: `compile.sh` with comprehensive error handling and reporting
- **Target Platforms**:
  - Windows (x86_64)
  - Linux (x86_64, ARM64, ARM32)
  - Raspberry Pi (32-bit and 64-bit ARM)
- **Binary Distribution**: Organized builds folder with platform-specific directories
- **Configuration Distribution**: Automatic copying of config files to build directories

### Security Features
- **Credential Protection**: Support for app passwords and secure SMTP authentication
- **TLS Support**: Full support for STARTTLS and SSL/TLS encryption
- **Input Validation**: Comprehensive validation of all user inputs and configuration
- **Error Sanitization**: Secure error reporting without exposing sensitive information
- **Permission Handling**: Graceful handling of file permission errors

---

**Note**: This is the initial release of DiskMon-Mail, providing a complete cross-platform disk space monitoring solution with email alerting capabilities. 