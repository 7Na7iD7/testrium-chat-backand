#!/usr/bin/env bash
# مهاجرت دیتا از Supabase ابری به instance جدید
# قبل از اجرا این متغیرها رو پر کن:
set -euo pipefail

CLOUD_DB_URL="postgresql://postgres:[PASSWORD]@[CLOUD-HOST]:5432/postgres"   # از Settings > Database ساپابیس ابری
NEW_DB_HOST="localhost"
NEW_DB_PORT="5432"
NEW_DB_USER="postgres"
NEW_DB_NAME="postgres"
export PGPASSWORD="CHANGE_ME"   # POSTGRES_PASSWORD instance جدید

echo "== ۱) بک‌آپ کامل از ابری (فقط برای نگهداری/rollback) =="
pg_dump --no-owner --no-privileges "$CLOUD_DB_URL" -f cloud_backup_full_$(date +%Y%m%d).sql
echo "✅ بک‌آپ کامل ذخیره شد: cloud_backup_full_$(date +%Y%m%d).sql — این فایل رو جایی امن نگه دار"

echo ""
echo "== ۲) استخراج داده‌ی جدول‌ها (بدون auth.users — دلیل در راهنما بخش ۷.۱) =="
TABLES=(
  public.students
  public.professors
  public.semesters
  public.course_catalog
  public.course_offerings
  public.sections
  public.enrollments
  public.exams
  public.exam_sessions
  public.exam_appeals
  public.chat_unmatched_queries
)

mkdir -p data_export
for t in "${TABLES[@]}"; do
  echo "  -> export $t"
  psql "$CLOUD_DB_URL" -c "\copy $t TO 'data_export/${t#public.}.csv' CSV HEADER"
done

echo ""
echo "== ۳) بررسی تعداد ردیف هر فایل =="
wc -l data_export/*.csv

echo ""
echo "⚠️  حالا به‌ترتیب FK این جدول‌ها رو import کن (به ترتیب دقیقاً همین آرایه بالا):"
echo "   psql -h $NEW_DB_HOST -p $NEW_DB_PORT -U $NEW_DB_USER -d $NEW_DB_NAME \\"
echo "     -c \"\\copy public.TABLE_NAME FROM 'data_export/TABLE_NAME.csv' CSV HEADER\""
echo ""
echo "بعد از هر import، تعداد ردیف رو با مبدا مقایسه کن:"
echo "   select count(*) from public.TABLE_NAME;"
