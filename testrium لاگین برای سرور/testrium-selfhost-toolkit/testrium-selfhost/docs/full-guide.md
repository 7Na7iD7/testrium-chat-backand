# راهنمای مهاجرت Testrium به Supabase Self-Hosted

این راهنما بر اساس معماری واقعی پروژه (۱۹ migration، ۵ Edge Function، Flutter + Rust chat-infra) نوشته شده. هدف: instance جدید دقیقاً معادل ساپابیس ابری فعلی، بدون گم‌شدن هیچ دیتایی.

**ترتیب پیشنهادی اجرا:** همیشه روی یک instance تست موازی کار کن، نه روی چیزی که کاربر واقعی داره ازش استفاده می‌کنه. فقط بعد از تایید کامل صحت، DNS/فرانت رو به سمت instance جدید سوییچ کن.

---

## بخش ۱ — پیش‌نیاز روی سرور

```bash
# نصب Docker + Compose (اگر از قبل نصب نیست)
curl -fsSL https://get.docker.com | sh
sudo usermod -aG docker $USER
# لاگ‌اوت/لاگین دوباره یا: newgrp docker

docker --version
docker compose version
```

فضای دیسک لازم: حداقل ۲۰ گیگ آزاد (Postgres + Storage فایل‌های آزمون + لاگ‌ها). با توجه به این‌که سرور نوبت‌دهی از قبل MinIO/Redis/Postgres/Nginx داره، از **پورت‌های متفاوت** برای جلوگیری از تداخل استفاده می‌کنیم.

---

## بخش ۲ — گرفتن Supabase Docker رسمی

```bash
mkdir -p ~/testrium-supabase && cd ~/testrium-supabase
git clone --depth 1 https://github.com/supabase/supabase.git supabase-src
cp -r supabase-src/docker/* .
cp .env.example .env
rm -rf supabase-src
```

---

## بخش ۳ — تنظیم `.env`

نکات حیاتی امنیتی (نه فقط جایگزینی مقادیر پیش‌فرض):

```bash
############
# Secrets — همه باید رندوم و یکتا تولید بشن، هرگز از مقادیر نمونه استفاده نکن
############

POSTGRES_PASSWORD=<generate: openssl rand -base64 32>
JWT_SECRET=<generate: openssl rand -base64 48>
ANON_KEY=<با JWT_SECRET بالا امضا می‌شه — بخش ۳.۱>
SERVICE_ROLE_KEY=<با JWT_SECRET بالا امضا می‌شه — بخش ۳.۱>
DASHBOARD_USERNAME=admin
DASHBOARD_PASSWORD=<generate: openssl rand -base64 24>
SECRET_KEY_BASE=<generate: openssl rand -base64 48>
VAULT_ENC_KEY=<generate: openssl rand -base64 32>

############
# دیتابیس
############
POSTGRES_HOST=db
POSTGRES_DB=postgres
POSTGRES_PORT=5432

############
# API
############
KONG_HTTP_PORT=8000
KONG_HTTPS_PORT=8443
API_EXTERNAL_URL=https://<subdomain دانشگاه — بخش ۵>
SUPABASE_PUBLIC_URL=https://<subdomain دانشگاه>

############
# Studio
############
STUDIO_PORT=3050
SUPABASE_URL=http://kong:8000

############
# Auth — چون verify-code خودت anonymous auth + custom code استفاده می‌کنه
############
DISABLE_SIGNUP=false
ENABLE_ANONYMOUS_USERS=true
JWT_EXPIRY=3600
```

### ۳.۱ — تولید `ANON_KEY` و `SERVICE_ROLE_KEY`

این دو کلید JWT هستن که با `JWT_SECRET` امضا می‌شن (نه رندوم مستقل). ساده‌ترین راه، استفاده از ابزار رسمی:

```bash
# با Node.js نصب‌شده:
npx supabase-jwt-generator  # یا استفاده از https://supabase.com/docs/guides/self-hosting#api-keys
```

یا دستی با payload زیر امضا کن (role: anon / service_role)، الگوریتم HS256، با `JWT_SECRET` بالا.

⚠️ **این دو کلید دقیقاً همون چیزی هستن که در کد فلاتر (`SupabaseFlutter.initialize`) و chat-infra (`config.rs`) باید جایگزین بشن.**

---

## بخش ۴ — بالا آوردن سرویس‌ها

```bash
docker compose pull
docker compose up -d
docker compose ps   # باید همه سرویس‌ها healthy باشن: db, kong, auth, rest, realtime, storage, meta, studio, imgproxy
```

بررسی سریع:
```bash
curl http://localhost:8000/rest/v1/ -H "apikey: $ANON_KEY"
```

---

## بخش ۵ — Nginx + دامنه دانشگاه

با توجه به این‌که دانشگاه ساب‌دامین می‌ده، نگه‌داشتن الگوی استاندارد Supabase یعنی یک دامین برای API (Kong روی پورت ۸۰۰۰) کافیه؛ Studio رو جدا و پشت IP-allowlist یا Basic Auth نگه دار (چون دسترسی مدیریتی کامل به دیتابیسه).

```nginx
# /etc/nginx/sites-available/testrium-supabase.conf

server {
    listen 443 ssl http2;
    server_name supabase.<domain-from-university>;

    ssl_certificate     /etc/letsencrypt/live/supabase.<domain>/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/supabase.<domain>/privkey.pem;

    client_max_body_size 50m;   # برای آپلود فایل سوالات به Storage

    location / {
        proxy_pass http://127.0.0.1:8000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # برای Realtime (WebSocket) — پروفایل استاد از این استفاده می‌کنه (بخش ۱۵ گزارش)
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_read_timeout 3600s;
    }
}

# Studio — محدود به IP خودت یا با Basic Auth اضافه
server {
    listen 443 ssl http2;
    server_name supabase-studio.<domain>;

    ssl_certificate     /etc/letsencrypt/live/supabase-studio.<domain>/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/supabase-studio.<domain>/privkey.pem;

    # allow <your-ip>;
    # deny all;

    location / {
        proxy_pass http://127.0.0.1:3050;
        proxy_set_header Host $host;
    }
}
```

```bash
sudo ln -s /etc/nginx/sites-available/testrium-supabase.conf /etc/nginx/sites-enabled/
sudo certbot --nginx -d supabase.<domain> -d supabase-studio.<domain>
sudo nginx -t && sudo systemctl reload nginx
```

سپس `.env` بالا رو با `API_EXTERNAL_URL=https://supabase.<domain>` بروزرسانی و `docker compose up -d` دوباره اجرا کن.

---

## بخش ۶ — اجرای Migration ها (به ترتیب دقیق)

⚠️ **حیاتی:** ترتیب اجرا دقیقاً همون شماره‌گذاریه (۰۰۰۱ تا ۰۰۱۹). چون migration های بعدی به policy/تابع migration های قبلی وابسته‌ان (مثلاً `0005` توابع `is_own_section` رو می‌سازه که `0006` تا `0018` ازش استفاده می‌کنن).

```bash
# psql رو به دیتابیس جدید وصل کن
export PGPASSWORD=<POSTGRES_PASSWORD از .env>
PSQL="psql -h localhost -p 5432 -U postgres -d postgres"

for f in 0001_init.sql \
         0002_multi_professor.sql \
         0003_semester_architecture.sql \
         0004_professor_enrollment_policy.sql \
         0005_fix_rls_recursion.sql \
         0006_grading_status_and_appeals.sql \
         0007_exam_content_storage_policy.sql \
         0008_professor_visibility.sql \
         0009_students_professor_section_policy.sql \
         0010_course_credit_units.sql \
         0011_fix_exam_sessions_update_security.sql \
         0012_cleanup_legacy_courses.sql \
         0013_exam_reset_and_delete_cascade.sql \
         0015_exam_pass_threshold.sql \
         0016_exam_max_score_and_trend_view.sql \
         0017_appeal_deadline_enforcement.sql \
         0018_lock_exam_sessions_insert_to_server.sql \
         0019_chat_unmatched_queries.sql
do
  echo "== اجرای $f =="
  $PSQL -f "migrations/$f" || { echo "❌ خطا در $f — متوقف شد"; break; }
done
```

نکته: `0012` جدول‌های قدیمی `courses`/`course_enrollments` رو حذف می‌کنه — چون روی دیتابیس تازه این جدول‌ها اصلاً وجود ندارن، `drop table if exists` بی‌خطر رد می‌شه (طراحی idempotent خودِ migration).

### ۶.۱ — بررسی صحت بعد از اجرا

```sql
-- باید همه‌ی این جدول‌ها موجود باشن
select table_name from information_schema.tables
where table_schema = 'public'
order by table_name;

-- باید توابع SECURITY DEFINER موجود باشن
select proname from pg_proc
where proname in ('current_professor_id','current_student_code',
                   'is_own_section','is_enrolled_in_section',
                   'reset_exam_sessions','delete_exam_cascade');

-- باید bucket exam-content موجود باشه
select * from storage.buckets where id = 'exam-content';
```

---

## بخش ۷ — مهاجرت دیتای فعلی (حیاتی‌ترین بخش)

با توجه به این‌که گفتی دیتا خیلی مهمه، این بخش رو با احتیاط کامل و بدون هیچ حذفی از سمت مبدا انجام بده.

### ۷.۱ — Backup کامل از ساپابیس ابری فعلی

```bash
# از پنل Supabase ابری: Settings → Database → Connection string (URI)
# یا از CLI:
supabase db dump --db-url "postgresql://postgres:[PASSWORD]@[HOST]:5432/postgres" -f cloud_backup_full.sql

# دیتای فقط جدول‌ها (بدون schema، برای merge امن‌تر با schema تازه‌ساخته‌شده):
pg_dump --data-only --no-owner --no-privileges \
  --exclude-table=auth.* \
  "postgresql://postgres:[PASSWORD]@[HOST]:5432/postgres" \
  -f cloud_data_only.sql
```

⚠️ جدول `auth.users` رو exclude کن — چون `auth_user_id` روی instance جدید باید از نو با `auth.users` جدید همون instance مچ بشه، نه از ابری import بشه (وگرنه FK می‌شکنه). این یعنی بعد از مهاجرت، همه‌ی دانشجوها/اساتید باید یک‌بار دیگه با کدشون وارد بشن تا `verify-code` دوباره `auth_user_id` رو لینک کنه — **این طراحی خودِ سیستم verify-code بدون تغییر پشتیبانی می‌کنه** (چون کد relink رو در `verify-code/index.ts` از قبل به‌صورت idempotent مدیریت می‌کنی).

### ۷.۲ — استراتژی import (به ترتیب FK)

```bash
# روی instance جدید، بعد از اجرای همه migration ها:
# ۱. جدول‌های بدون وابستگی به auth.users اول
$PSQL -c "\copy public.students(student_code, first_name, last_name, student_class, is_active, created_at) FROM 'students_export.csv' CSV HEADER"
$PSQL -c "\copy public.professors(id, professor_code, full_name, is_active, created_at) FROM 'professors_export.csv' CSV HEADER"

# ۲. semesters, course_catalog, course_offerings, sections (به ترتیب FK)
# ۳. enrollments
# ۴. exams (بعد از این‌که فایل‌های Storage هم منتقل شدن — بخش ۷.۳)
# ۵. exam_sessions, exam_appeals, chat_unmatched_queries
```

توصیه: به‌جای `pg_dump`/`psql` مستقیم برای جدول‌های حساس، از **Supabase Studio → Table Editor → Export CSV** برای هر جدول به‌ترتیب استفاده کن و با چشم مقدار ردیف‌ها رو با مبدا مقایسه کن (`select count(*)`).

### ۷.۳ — مهاجرت Storage (فایل‌های سوالات آزمون)

```bash
# نصب rclone برای sync بین دو S3-compatible endpoint
rclone config  # یک remote برای supabase-cloud، یک remote برای instance جدید

rclone sync supabase-cloud:exam-content new-instance:exam-content --progress
```

بعد از sync، چک کن `current_file` هر ردیف در `exams` با فایل واقعی روی Storage جدید مچ باشه:
```sql
select exam_id, current_file from public.exams;
```

### ۷.۴ — چک نهایی صحت مهاجرت

```sql
-- تعداد ردیف هر جدول رو با مبدا مقایسه کن
select 'students' as t, count(*) from public.students
union all select 'exam_sessions', count(*) from public.exam_sessions
union all select 'enrollments', count(*) from public.enrollments
union all select 'exams', count(*) from public.exams
union all select 'exam_appeals', count(*) from public.exam_appeals;
```

---

## بخش ۸ — دیپلوی Edge Functions

Self-host شده‌ی ساپابیس هم Edge Runtime داره (سرویس `edge-functions` در `docker-compose.yml`). فولدرهای موجودت (`chat-identity`, `chat-verify-link`, `submit-exam-result`, `verify-code`, و `admin-academic-ops` طبق گزارش) رو کپی کن:

```bash
cp -r ~/flutter_projects/testrium/supabase/functions/* ~/testrium-supabase/volumes/functions/
```

فایل `.env` مربوط به هر تابع (`CHAT_SERVER_SHARED_SECRET`, `SUPABASE_SERVICE_ROLE_KEY`, ...) رو در `docker-compose.yml` بخش `edge-functions` تنظیم کن:

```yaml
  functions:
    environment:
      CHAT_SERVER_SHARED_SECRET: <همون secret مشترک با chat-infra>
      # SUPABASE_URL و SERVICE_ROLE_KEY به‌طور پیش‌فرض inject می‌شن
```

```bash
docker compose restart functions
curl -X POST https://supabase.<domain>/functions/v1/verify-code \
  -H "Content-Type: application/json" \
  -d '{"code":"TEST","auth_user_id":"..."}'
```

---

## بخش ۹ — وصل کردن فرانت‌اند (Flutter)

فایل تنظیمات ساپابیس (معمولاً `lib/core/config/supabase_config.dart` یا مشابه) رو با URL و کلید جدید عوض کن:

```dart
await Supabase.initialize(
  url: 'https://supabase.<domain-from-university>',
  anonKey: '<ANON_KEY جدید از .env>',
);
```

اگه از `.env`/`--dart-define` استفاده می‌کنی، فقط مقادیر build رو عوض کن — کد دست‌نخورده می‌مونه.

---

## بخش ۱۰ — وصل کردن بک‌اند چت (Rust/Axum)

در `chat-infra/src/config.rs`، این مقادیر رو به‌روزرسانی کن:

```rust
supabase_url: "https://supabase.<domain>",
supabase_service_role_key: "<SERVICE_ROLE_KEY جدید>",
supabase_jwks_url: "https://supabase.<domain>/auth/v1/.well-known/jwks.json",
chat_server_shared_secret: "<همون مقداری که در .env توابع Edge گذاشتی>",
```

⚠️ **اصلاحیه‌ی مهم (نسخه‌ی قبلی این بخش اشتباه بود):** نسخه‌ی ابری قبلی از JWKS با کلید ES256 استفاده می‌کرد، اما نسخه‌ی self-hosted استاندارد (Docker Compose رسمی) این فیچر رو به‌سادگی پشتیبانی نمی‌کنه — GoTrue self-hosted پیش‌فرض توکن‌های ورود واقعی کاربر رو هم با **HS256** امضا می‌کنه، همون الگوریتمی که `ANON_KEY`/`SERVICE_ROLE_KEY` هم باهاش امضا می‌شن. تلاش برای مجبور کردن GoTrue به ES256 (با `GOTRUE_JWT_ALGORITHM=ES256`) بدون تولید و تنظیم دستی کلید EC، باعث خطای ۴۰۱ روی کل API می‌شه.

راه‌حل درست: از مسیر **legacy_jwt_secret** که در `config.rs` از قبل به‌عنوان fallback HS256 آماده شده بود، به‌عنوان مسیر *اصلی* verify استفاده کن (نه fallback). یعنی:
- در `.env` instance self-hosted، هیچ `GOTRUE_JWT_ALGORITHM` ست نکن (پیش‌فرض HS256 بمونه)
- در `.env` سرویس چت، مقدار `SUPABASE_JWT_SECRET` رو دقیقاً همون `JWT_SECRET` بذار که در `.env` instance self-hosted استفاده کردی

با این کار، `chat-infra` توکن‌های واقعی کاربر رو از همون مسیر HS256 (نه JWKS) verify می‌کنه — که دقیقاً چیزیه که self-hosted تولید می‌کنه.

---

## بخش ۱۱ — چک‌لیست نهایی قبل از سوییچ کامل

- [ ] همه‌ی ۱۹ migration بدون خطا اجرا شدن
- [ ] تعداد ردیف هر جدول با مبدا مچ شده
- [ ] فایل‌های Storage sync شدن و `current_file` ها معتبرن
- [ ] یک ورود کامل تست (verify-code) با یک کد دانشجویی واقعی موفق بوده
- [ ] یک submit آزمون تست (submit-exam-result) موفق بوده و نمره درست محاسبه شده
- [ ] chat-infra با JWT جدید موفق auth می‌کنه (نه فقط handshake، یک پیام واقعی رد و بدل بشه)
- [ ] Nginx SSL معتبره (`curl -v` بدون warning گواهی)
- [ ] بک‌اپ کامل از instance ابری قدیمی گرفته شده و جایی امن نگه‌داری می‌شه (قبل از هر تصمیم خاموش‌کردنش)

فقط بعد از تیک‌خوردن همه‌ی این‌ها، DNS/کانفیگ فرانت رو نهایی به سمت instance جدید سوییچ کن — و instance ابری قدیمی رو حداقل چند هفته نگه دار (نه حذف)، برای rollback احتمالی.
