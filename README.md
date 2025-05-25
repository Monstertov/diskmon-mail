
# diskmon-mail

Monitors disk free space and sends email alerts when free space drops below a configured threshold.

---

## Features

- Checks free disk space on Windows, Linux, and macOS
- Sends email alerts when free space is below threshold
- Configuration via JSON file
- Automatically installs `psutil` if missing
- Includes hostname in alert email subject

---

## Requirements

- Python 3.6+
- Internet connection for sending email
- Access to SMTP server

---

## Setup

1. Clone or download the script and `config.json` to your system.
2. Edit `config.json` with your email and SMTP server details.
3. Ensure Python 3 and pip are installed.
4. Run the script:

```bash
python3 diskmon.py
```

---

## Configuration (`config.json`)

```json
{
  "mail_enabled": false,
  "smtp_server": "smtp.example.com",
  "smtp_port": 587,
  "smtp_user": "your-email@example.com",
  "smtp_pass": "your-password",
  "email_from": "your-email@example.com",
  "email_to": "recipient@example.com"
}
```

- `mail_enabled`: Set to `true` to enable email alerts.
- `smtp_server`: SMTP server hostname.
- `smtp_port`: SMTP server port (usually 587 for TLS).
- `smtp_user`: SMTP login username.
- `smtp_pass`: SMTP login password.
- `email_from`: Email address used as sender.
- `email_to`: Recipient email address.

---

## Autostart

### Windows

Create a scheduled task to run at startup. Example PowerShell script:

```powershell
$taskName = "DiskMonitorTask"
$pythonExe = (Get-Command python).Source
$scriptPath = "C:\path\to\diskmon.py"
$action = "$pythonExe `"$scriptPath`""

schtasks /Create /TN $taskName /TR $action /SC ONSTART /RL HIGHEST /F
```

### Linux (daily cron job at 05:00)

Use this shell script to add a cron job that runs the script daily at 05:00:

```bash
#!/bin/bash

SCRIPT_PATH="$1"

if [ -z "$SCRIPT_PATH" ]; then
  echo "Usage: $0 /path/to/diskmon.py"
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is not installed or not in PATH."
  exit 1
fi

# Check if cron job exists
(crontab -l 2>/dev/null | grep -F "0 5 * * * python3 $SCRIPT_PATH") && {
  echo "Cron job already exists."
  exit 0
}

(crontab -l 2>/dev/null; echo "0 5 * * * python3 $SCRIPT_PATH > /dev/null 2>&1") | crontab -

echo "Cron job added to run script daily at 05:00."
```

Run this script with the full path to your `diskmon.py` file:

```bash
./autorun_linux /home/user/diskmon.py
```

---

## License

MIT License

---

## Notes

- The script monitors the root path (`/` or `C:\`) by default.
- Adjust `THRESHOLD_PERCENT` in the script if you want a different threshold.
- Make sure the SMTP credentials are correct for sending mail.
