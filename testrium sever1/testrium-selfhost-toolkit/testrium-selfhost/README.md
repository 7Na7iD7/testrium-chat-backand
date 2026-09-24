# Testrium Self-Host Toolkit

## ترتیب اجرا روی سرور

```bash
# ۱. ساخت کلیدهای JWT (قبل از هر چیز، از لپ‌تاپ خودت یا سرور)
pip install pyjwt --break-system-packages
python3 scripts/00_generate_jwt_keys.py "$(openssl rand -base64 48)"
# خروجی رو در env/.env.template به‌جای ANON_KEY / SERVICE_ROLE_KEY بذار
# همون JWT_SECRET استفاده‌شده رو هم در .env بذار

# ۲. راه‌اندازی اولیه سرور (نصب Docker + گرفتن Supabase Docker)
bash scripts/01_setup_server.sh

# ۳. .env رو کامل پر کن (env/.env.template را به ~/testrium-supabase/.env کپی کن)
nano ~/testrium-supabase/.env

# ۴. بالا آوردن سرویس‌ها
bash scripts/02_start_services.sh

# ۵. Nginx (بعد از گرفتن ساب‌دامین از دانشگاه)
#    nginx/testrium-supabase.conf رو با دامین واقعی ویرایش و فعال کن

# ۶. اجرای migration ها (اول migrations/*.sql رو در پوشه‌ی migrations بذار)
# ⚠️ همچنین این دو فایل رو از پوشه‌ی extra-migrations/ این پکیج به
# پوشه‌ی migrations پروژه‌ات کپی کن (چون فقط روی پروژه‌ی زنده اجرا
# شده بودن و در فایل‌های محلی پروژه پیدا/ارسال نشده بودن):
#   0025_fix_verify_code_attempts_rls.sql (رفع باگ امنیتی RLS)
#   0027_admin_roles.sql (ستون role برای جدول admins)
# (این یه باگ امنیتی واقعی بود که روی نسخه‌ی ابری هم پیدا و اصلاح شد —
# migration 0024 یادش رفته بود RLS رو روی verify_code_attempts فعال کنه)
bash scripts/03_run_migrations.sh
# ⚠️ migration 0021 نیاز به دو تنظیم دستی اضافه داره — خروجی اسکریپت
# بالا خودش دستور دقیقش رو نشون می‌ده (GUC های sync_claims_url/secret)

# ۷. مهاجرت دیتای فعلی از ساپابیس ابری
bash scripts/04_migrate_data.sh
bash scripts/05_migrate_storage.sh

# ۸. دیپلوی Edge Functions (شامل sync-user-claims جدید)
cp -r /path/to/testrium/supabase/functions/* ~/testrium-supabase/volumes/functions/
# مطمئن شو SYNC_CLAIMS_SHARED_SECRET برای تابع sync-user-claims هم در
# .env تنظیم شده (دقیقاً همون مقداری که در دستورات GUC بالا استفاده کردی)
docker compose restart functions
```

## ساختار این پکیج

```
env/.env.template          -> نمونه‌ی کامل .env با کامنت توضیحی
nginx/testrium-supabase.conf -> Nginx reverse proxy برای API + Studio
scripts/00_generate_jwt_keys.py -> تولید ANON_KEY/SERVICE_ROLE_KEY
scripts/01_setup_server.sh -> نصب Docker + گرفتن Supabase Docker رسمی
scripts/02_start_services.sh -> docker compose up + تست سلامت
scripts/03_run_migrations.sh -> اجرای دقیق ۱۸ migration به ترتیب صحیح
scripts/04_migrate_data.sh  -> export/import دیتای جدول‌ها از ابری
scripts/05_migrate_storage.sh -> sync فایل‌های Storage با rclone
docs/full-guide.md          -> راهنمای کامل با توضیح هر تصمیم
monitoring/                 -> مانیتورینگ سرور ۱ (Postgres exporter) از سرور ۲
                                (پیوند بده به Prometheus موجود روی سرور ۲؛
                                جزئیات کامل در monitoring/SERVER1_MONITORING_GUIDE.md)
```

## چیزهایی که هنوز از تو لازم دارم (برای اتصال فرانت/بک‌اند)

بعد از این‌که instance بالا اومد، برای وصل کردن فرانت و بک‌اند این فایل‌ها رو برام بفرست تا دقیق ویرایششون کنم:

**فلاتر:**
- فایلی که `Supabase.initialize(...)` توش صدا زده می‌شه (معمولاً `lib/main.dart` یا `lib/core/config/supabase_config.dart`)
- هر فایل `.env` یا تنظیمات build (`--dart-define`) که الان `SUPABASE_URL`/`ANON_KEY` توشونه

**بک‌اند چت (Rust):**
- `chat-infra/src/config.rs`
- هر فایل `.env`/`docker-compose.yml` مربوط به سرویس چت که secret ها توشه

با این فایل‌ها، دقیقاً بهت می‌گم کجا چی رو با مقادیر instance جدید عوض کنی.
