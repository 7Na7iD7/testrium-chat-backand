#!/usr/bin/env bash
# اجرای migration ها از داخل کانتینر دیتابیس (بدون نیاز به پورت ۵۴۳۲ یا pooler)
# فایل‌های migration باید توی پوشه‌ی ../migrations کنار این اسکریپت باشن
set -euo pipefail

DB_CONTAINER="${DB_CONTAINER:-supabase-db}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MIGRATIONS_DIR="$SCRIPT_DIR/../migrations"

if [ ! -d "$MIGRATIONS_DIR" ]; then
  echo "❌ پوشه‌ی migrations پیدا نشد: $MIGRATIONS_DIR"
  exit 1
fi

# بررسی اینکه کانتینر دیتابیس بالاست
if ! docker exec "$DB_CONTAINER" pg_isready -U postgres -d postgres > /dev/null 2>&1; then
  echo "❌ کانتینر $DB_CONTAINER در دسترس نیست یا دیتابیس آماده نیست."
  echo "   بررسی کن: docker ps --format 'table {{.Names}}\t{{.Status}}'"
  exit 1
fi

# psql داخل کانتینر؛ فایل‌ها از stdin خونده می‌شن چون روی هاست هستن
PSQL="docker exec -i $DB_CONTAINER psql -U postgres -d postgres -v ON_ERROR_STOP=1"

# ترتیب دقیق و اجباری — migration های بعدی به توابع/policy قبلی وابسته‌ان
FILES=(
  "0001_init.sql"
  "0002_multi_professor.sql"
  "0003_semester_architecture.sql"
  "0004_professor_enrollment_policy.sql"
  "0005_fix_rls_recursion.sql"
  "0006_grading_status_and_appeals.sql"
  "0007_exam_content_storage_policy.sql"
  "0008_professor_visibility.sql"
  "0009_students_professor_section_policy.sql"
  "0010_course_credit_units.sql"
  "0011_fix_exam_sessions_update_security.sql"
  "0012_cleanup_legacy_courses.sql"
  "0013_exam_reset_and_delete_cascade.sql"
  "0015_exam_pass_threshold.sql"
  "0016_exam_max_score_and_trend_view.sql"
  "0017_appeal_deadline_enforcement.sql"
  "0018_lock_exam_sessions_insert_to_server.sql"
  "0019_chat_unmatched_queries.sql"
  "0020_exam_per_question_timing_flag.sql"
  "0021_sync_claims_webhook.sql"
  "0022_admin_audit_log.sql"
  "0023_submit_exam_session_atomic.sql"
  "0024_code_security_hardening.sql"
  "0025_fix_verify_code_attempts_rls.sql"
  "0026_admin_identity_system.sql"
  "0027_admin_roles.sql"
  "0028_add_purged_at_to_semesters.sql"
  "0029_create_passkey_tables.sql"
)

# قبل از شروع مطمئن شو همه‌ی فایل‌ها هستن (تا وسط کار متوقف نشه)
for f in "${FILES[@]}"; do
  if [ ! -f "$MIGRATIONS_DIR/$f" ]; then
    echo "❌ فایل پیدا نشد: $f — قبل از شروع متوقف شد"
    exit 1
  fi
done

for f in "${FILES[@]}"; do
  echo "== اجرای $f =="
  $PSQL < "$MIGRATIONS_DIR/$f"
done

echo ""
echo "✅ همه‌ی migration ها اجرا شدن. بررسی صحت:"
$PSQL -c "select table_name from information_schema.tables where table_schema='public' order by table_name;"
$PSQL -c "select proname from pg_proc where proname in ('current_professor_id','current_student_code','is_own_section','is_enrolled_in_section','reset_exam_sessions','delete_exam_cascade');"
$PSQL -c "select id, name, public from storage.buckets;"

echo ""
echo "⚠️  migration 0021 دو تنظیم دیتابیسی جدا لازم داره. دستی اجرا کن:"
echo ""
echo "    docker exec -i $DB_CONTAINER psql -U postgres -d postgres -c \"alter database postgres set app.settings.sync_claims_url = 'https://<API_EXTERNAL_URL>/functions/v1/sync-user-claims';\""
echo "    docker exec -i $DB_CONTAINER psql -U postgres -d postgres -c \"alter database postgres set app.settings.sync_claims_secret = '<SYNC_CLAIMS_SHARED_SECRET از .env>';\""
echo ""
echo "    بعد: cd ~/testrium-supabase && docker compose restart db"
echo ""
echo "⚠️  migration 0024 از pg_cron اختیاری استفاده می‌کنه. اگه فعال نبود، این کوئری"
echo "    رو با cron خارجی (crontab -e) هر ساعت اجرا کن:"
echo "    docker exec -i $DB_CONTAINER psql -U postgres -d postgres -c \"delete from verify_code_attempts where attempted_at < now() - interval '24 hours';\""
echo ""
echo "⚠️  migration 0028 پیش‌نیاز اکشن جدید purge_semester_data در admin-academic-ops هست."
echo "    برای این‌که اون اکشن کار کنه، این دو env var هم باید در .env تابع"
echo "    admin-academic-ops ست بشن (توجه: CHAT_SERVER_BASE_URL با CHAT_SERVER_URL"
echo "    که برای sync-user-claims استفاده کردیم فرق داره، اسم متغیر جداست):"
echo "        CHAT_SERVER_BASE_URL=https://<دامنه چت>"
echo "        CHAT_SERVER_NOTIFY_SECRET=<همون مقدار قبلی>"
echo "    و سمت Rust باید endpoint POST /internal/purge-section-data پیاده‌سازی شده باشه."
