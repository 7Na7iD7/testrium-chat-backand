-- ====================================================================
-- Testrium — Migration 0015: آستانه‌ی قبولی دائمی برای هر آزمون
-- ====================================================================
-- قبلاً «آستانه‌ی قبولی» (مثلاً ۵۰٪) فقط یک مقدار موقت در state داشبورد
-- تحلیلی بود که با بستن صفحه از بین می‌رفت و هیچ‌جا ذخیره نمی‌شد. طبق
-- نیاز جدید، این مقدار باید per-exam و دائمی در دیتابیس ذخیره شود تا:
--   ۱) محاسبه‌ی قبولی/مردودی همیشه یکسان و قابل‌اتکا باشد (نه وابسته
--      به آخرین مقداری که استاد در UI جابه‌جا کرده)،
--   ۲) گزارش PDF/Excel هم دقیقاً همان آستانه‌ای را نشان دهد که در
--      داشبورد دیده می‌شود.
--
-- مقدار پیش‌فرض ۵۰ گذاشته شده تا آزمون‌های موجود (created قبل از این
-- migration) بدون نیاز به backfill دستی، همان رفتار قبلی (آستانه‌ی
-- ثابت ۵۰٪) را حفظ کنند — یعنی این migration هیچ رفتار فعلی را
-- نمی‌شکند، فقط قابل‌تغییر و دائمی‌اش می‌کند.
-- ====================================================================

alter table public.exams
  add column if not exists pass_threshold_percent numeric not null default 50;

alter table public.exams
  drop constraint if exists exams_pass_threshold_percent_check;

alter table public.exams
  add constraint exams_pass_threshold_percent_check
  check (pass_threshold_percent >= 0 and pass_threshold_percent <= 100);

-- RLS جدیدی لازم نیست: استاد از طریق پالیسی‌های موجود
-- (exams_manage_own_section، migration 0003/0005) از قبل اجازه‌ی
-- update ستون‌های exams مربوط به section خودش را دارد؛ این ستون هم
-- همان پالیسی را به ارث می‌برد.
