#!/usr/bin/env bash
# اجرا روی سرور: bash 01_setup_server.sh
set -euo pipefail

echo "== نصب Docker =="
if ! command -v docker &> /dev/null; then
  curl -fsSL https://get.docker.com | sh
  sudo usermod -aG docker "$USER"
  echo "⚠️  حالا لاگ‌اوت/لاگین کن (یا: newgrp docker) و دوباره اسکریپت رو اجرا کن"
  exit 0
fi

echo "== گرفتن Supabase Docker رسمی =="
mkdir -p ~/testrium-supabase && cd ~/testrium-supabase
if [ ! -d docker ]; then
  git clone --depth 1 https://github.com/supabase/supabase.git supabase-src
  cp -r supabase-src/docker/* .
  rm -rf supabase-src
fi

echo "== کپی env template (باید دستی مقادیر CHANGE_ME رو پر کنی) =="
if [ ! -f .env ]; then
  cp "$(dirname "$0")/../env/.env.template" .env
  echo "⚠️  فایل .env ساخته شد — قبل از ادامه مقادیر CHANGE_ME رو با openssl rand پر کن"
fi

echo "== انجام شد. مراحل بعدی: =="
echo "  1) .env رو پر کن (openssl rand -base64 32/48/24)"
echo "  2) bash 02_start_services.sh"
