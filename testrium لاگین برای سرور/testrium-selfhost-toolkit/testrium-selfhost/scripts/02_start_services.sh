#!/usr/bin/env bash
set -euo pipefail
cd ~/testrium-supabase

echo "== بررسی .env (نباید هیچ CHANGE_ME باقی مونده باشه) =="
if grep -q "CHANGE_ME" .env; then
  echo "❌ هنوز مقادیر CHANGE_ME در .env هست. اول پرشون کن."
  exit 1
fi

echo "== pull و اجرای سرویس‌ها =="
docker compose pull
docker compose up -d

echo "== وضعیت =="
sleep 5
docker compose ps

echo "== تست سریع API =="
source .env
curl -s http://localhost:8000/rest/v1/ -H "apikey: $ANON_KEY" | head -c 200
echo ""
echo "✅ اگر خطای اتصال ندیدی، سرویس‌ها بالا هستن."
