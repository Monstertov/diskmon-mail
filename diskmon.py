#!/usr/bin/env python3
import os
import smtplib
import subprocess
import sys
import json
import socket

try:
    import psutil
except ImportError:
    try:
        subprocess.check_call(
            [sys.executable, '-m', 'pip', 'install', 'psutil'],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL
        )
        import psutil
    except subprocess.CalledProcessError as e:
        err_str = str(e)
        if 'externally-managed-environment' in err_str or 'python3-' in err_str:
            try:
                subprocess.check_call(
                    ['sudo', 'apt', 'install', 'python3-psutil', '-y'],
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL
                )
                import psutil
            except subprocess.CalledProcessError:
                print("Please install psutil manually (e.g. sudo apt install python3-psutil) and rerun.")
                sys.exit(1)
        else:
            print("Please install psutil manually and rerun.")
            sys.exit(1)

from email.message import EmailMessage

CONFIG_PATH = 'config.json'

if os.name == 'nt':  # Windows
    CHECK_PATH = 'C:\\'
else:  # Linux / macOS
    CHECK_PATH = '/'

THRESHOLD_PERCENT = 10

def load_config(path):
    with open(path, 'r') as f:
        return json.load(f)

config = load_config(CONFIG_PATH)

MAIL_ENABLED = config.get('mail_enabled', False)
SMTP_SERVER = config.get('smtp_server', 'smtp.example.com')
SMTP_PORT = config.get('smtp_port', 587)
SMTP_USER = config.get('smtp_user', 'your-email@example.com')
SMTP_PASS = config.get('smtp_pass', 'your-password')
EMAIL_FROM = config.get('email_from', SMTP_USER)
EMAIL_TO = config.get('email_to', 'recipient@example.com')

def get_free_percentage(path):
    usage = psutil.disk_usage(path)
    return usage.free / usage.total * 100

def send_alert(free_percent):
    if not MAIL_ENABLED:
        print(f"[TEST MODE] Alert: Disk free space is below threshold: {free_percent:.2f}% remaining. Mail not sent.")
        return

    hostname = socket.gethostname()

    msg = EmailMessage()
    msg.set_content(f'Disk free space is below threshold: {free_percent:.2f}% remaining.')
    msg['Subject'] = f'Disk Space Alert on {hostname}'
    msg['From'] = EMAIL_FROM
    msg['To'] = EMAIL_TO

    with smtplib.SMTP(SMTP_SERVER, SMTP_PORT) as smtp:
        smtp.starttls()
        smtp.login(SMTP_USER, SMTP_PASS)
        smtp.send_message(msg)

def main():
    free_percent = get_free_percentage(CHECK_PATH)
    if free_percent < THRESHOLD_PERCENT:
        send_alert(free_percent)
    else:
        print(f"Disk free space is sufficient: {free_percent:.2f}%")

if __name__ == '__main__':
    main()

