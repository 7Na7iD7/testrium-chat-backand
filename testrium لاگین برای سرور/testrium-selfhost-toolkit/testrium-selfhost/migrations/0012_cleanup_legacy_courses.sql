-- ====================================================================
-- Testrium — Migration 0012: حذف نهایی مدل قدیمی courses
-- ====================================================================
-- طبق کامنت خودِ migration 0003: جداول قدیمی courses/course_enrollments
-- و ستون exams.course_id عمداً حذف نشدند تا در صورت باگ در معماری جدید
-- (semesters/sections/enrollments) امکان rollback سریع وجود داشته
-- باشد. حالا که مدل جدید کاملاً پایدار و در پروداکشن است، این migration
-- باقی‌مانده‌های مدل قدیمی را به‌طور کامل و تمیز حذف می‌کند.
--
-- ترتیب حذف مهم است: اول سیاست‌هایی که به این جداول/ستون رفرنس
-- می‌دهند حذف می‌شوند (وگرنه Postgres به‌خاطر وابستگی pg_depend اجازه‌ی
-- drop نمی‌دهد)، بعد ستون، بعد خودِ جداول (به ترتیب معکوس وابستگی FK:
-- اول course_enrollments که به courses رفرنس می‌دهد، بعد courses).
-- ====================================================================

begin;

-- ── ۱. حذف سیاست‌های RLS مبتنی بر مدل قدیمی ──
drop policy if exists "exams_manage_own_course" on public.exams;
drop policy if exists "students_professor_view_enrolled" on public.students;

-- ── ۲. حذف ستون قدیمی exams.course_id (کاملاً جایگزین‌شده با section_id) ──
alter table public.exams drop column if exists course_id;

-- ── ۳. حذف جداول قدیمی (به‌ترتیب وابستگی FK) ──
-- سیاست‌ها و ایندکس‌های خودِ این دو جدول (courses_all_own،
-- enrollments_manage_own_course، idx_courses_professor،
-- idx_enrollments_student روی course_enrollments) خودکار با حذف جدول
-- پاک می‌شوند.
drop table if exists public.course_enrollments;
drop table if exists public.courses;

commit;

-- ====================================================================
-- بررسی صحت بعد از اجرا (اختیاری، برای تایید دستی):
--   select table_name from information_schema.tables
--   where table_schema = 'public' and table_name in ('courses', 'course_enrollments');
--   -- باید هیچ ردیفی برنگردد
--
--   select column_name from information_schema.columns
--   where table_schema = 'public' and table_name = 'exams' and column_name = 'course_id';
--   -- باید هیچ ردیفی برنگردد
-- ====================================================================
