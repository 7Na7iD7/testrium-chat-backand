#!/usr/bin/env bash
# مهاجرت فایل‌های bucket exam-content از ساپابیس ابری به instance جدید
set -euo pipefail

if ! command -v rclone &> /dev/null; then
  echo "== نصب rclone =="
  curl https://rclone.org/install.sh | sudo bash
fi

echo "== باید دو remote تعریف کنی: =="
echo "   rclone config"
echo ""
echo "   remote 1 (نام: supabase-cloud):"
echo "     type: s3 (Other S3 compatible)"
echo "     endpoint: https://[PROJECT-REF].supabase.co/storage/v1/s3"
echo "     access_key_id / secret_access_key: از Settings > Storage ساپابیس ابری"
echo ""
echo "   remote 2 (نام: new-instance):"
echo "     type: s3 (Other S3 compatible)"
echo "     endpoint: https://supabase.YOUR-UNIVERSITY-DOMAIN/storage/v1/s3"
echo "     access_key_id / secret_access_key: از instance جدید (Settings > Storage)"
echo ""
read -p "بعد از تنظیم rclone config، Enter بزن تا sync شروع بشه..."

rclone sync supabase-cloud:exam-content new-instance:exam-content --progress

echo "✅ Sync تمام شد. بررسی صحت:"
echo "  rclone check supabase-cloud:exam-content new-instance:exam-content"
