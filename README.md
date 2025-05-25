
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
  "email_to": "recipient@example.com",
  "filelocation": "/full/path/to/diskmon.py"
}
```

- `mail_enabled`: Set to `true` to enable email alerts.
- `smtp_server`: SMTP server hostname.
- `smtp_port`: SMTP server port (usually 587 for TLS).
- `smtp_user`: SMTP login username.
- `smtp_pass`: SMTP login password.
- `email_from`: Email address used as sender.
- `email_to`: Recipient email address.
- `filelocation`: Full path to the diskmon.py script.

---

## Autostart

### Windows

Create a scheduled task to run at startup. Use the autorun_windows script that creates a task that runs daily at 5:00 or on boot.



### Linux (daily cron job at 05:00)

Use the autorun_linux script to install a cronjob that runs daily at 5:00.

---

## License

MIT License

---

## Notes

- The script monitors the root path (`/` or `C:\`) by default.
- Adjust `THRESHOLD_PERCENT` in the script if you want a different threshold.
- Make sure the SMTP credentials are correct for sending mail.
