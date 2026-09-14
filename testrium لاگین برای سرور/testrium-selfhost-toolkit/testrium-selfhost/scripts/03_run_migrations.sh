#!/usr/bin/env bash
# قبل از اجرا، فایل‌های migration رو کنار این اسکریپت (پوشه ../migrations) بذار
set -euo pipefail
cd ~/testrium-supabase
source .env

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MIGRATIONS_DIR="$SCRIPT_DIR/../migrations"

if [ ! -d "$MIGRATIONS_DIR" ]; then
  echo "❌ پوشه‌ی migrations پیدا نشد: $MIGRATIONS_DIR"
  echo "فایل‌های 0001_init.sql تا 0019_chat_unmatched_queries.sql رو اونجا بذار"
  exit 1
fi

export PGPASSWORD="$POSTGRES_PASSWORD"
PSQL="psql -h localhost -p 5432 -U postgres -d postgres -v ON_ERROR_STOP=1"

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
)

for f in "${FILES[@]}"; do
  path="$MIGRATIONS_DIR/$f"
  if [ ! -f "$path" ]; then
    echo "❌ فایل پیدا نشد: $f — متوقف شد"
    exit 1
  fi
  echo "== اجرای $f =="
  $PSQL -f "$path"
done

echo ""
echo "✅ همه‌ی migration ها اجرا شدن. بررسی صحت:"
$PSQL -c "select table_name from information_schema.tables where table_schema='public' order by table_name;"
$PSQL -c "select proname from pg_proc where proname in ('current_professor_id','current_student_code','is_own_section','is_enrolled_in_section','reset_exam_sessions','delete_exam_cascade');"
$PSQL -c "select id, name, public from storage.buckets;"

echo ""
echo "⚠️  migration 0021 دو تنظیم دیتابیسی جدا لازم داره که عمداً توی"
echo "    خودِ فایل SQL نیست (چون به‌ازای هر پروژه فرق می‌کنه). دستی اجرا کن:"
echo ""
echo "    \$PSQL -c \"alter database postgres set app.settings.sync_claims_url = 'https://<API_EXTERNAL_URL>/functions/v1/sync-user-claims';\""
echo "    \$PSQL -c \"alter database postgres set app.settings.sync_claims_secret = '<همون مقدار SYNC_CLAIMS_SHARED_SECRET که در .env تابع sync-user-claims گذاشتی>';\""
echo ""
echo "    بعد یک بار Postgres رو reload کن تا current_setting مقدار تازه رو ببینه:"
echo "    docker compose restart db"
echo ""
echo "    نکته‌ی دیگه: pg_net باید توی ایمیج Postgres self-hosted در دسترس باشه."
echo "    ایمیج رسمی supabase/postgres این extension رو از قبل داره، پس اگه از"
echo "    docker-compose رسمی استفاده کرده باشی معمولاً مشکلی پیش نمیاد؛ اگه"
echo "    'create extension pg_net' با خطا مواجه شد یعنی ایمیج سفارشی/قدیمیه."
