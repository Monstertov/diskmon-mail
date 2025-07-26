# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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