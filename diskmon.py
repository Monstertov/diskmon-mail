#!/usr/bin/env python3
import os
import smtplib
import subprocess
import sys

try:
    import psutil
except ImportError:
    subprocess.check_call([sys.executable, '-m', 'pip', 'install', 'psutil'])
    import psutil

from email.message import EmailMessage

if os.name == 'nt':  # Windows
    CHECK_PATH = 'C:\\'
else:  # Linux / macOS
    CHECK_PATH = '/'

THRESHOLD_PERCENT = 10
MAIL_ENABLED = False  # Set to True to enable sending mail

SMTP_SERVER = 'smtp.example.com'
SMTP_PORT = 587
SMTP_USER = 'your-email@example.com'
SMTP_PASS = 'your-password'
EMAIL_FROM = SMTP_USER
EMAIL_TO = 'recipient@example.com'

def get_free_percentage(path):
    usage = psutil.disk_usage(path)
    return usage.free / usage.total * 100

def send_alert(free_percent):
    if not MAIL_ENABLED:
        print(f"[TEST MODE] Alert: Disk free space is below threshold: {free_percent:.2f}% remaining. Mail not sent.")
        return

    msg = EmailMessage()
    msg.set_content(f'Disk free space is below threshold: {free_percent:.2f}% remaining.')
    msg['Subject'] = 'Disk Space Alert'
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
