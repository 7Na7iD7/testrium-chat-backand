-- ====================================================================
-- Testrium — Migration 0020: فلگ صریح برای تایمر تک‌سوالی
-- ====================================================================
-- مشکل: در کد کلاینت (ExamController._questionTotalSeconds)، وقتی
-- استاد روی یک سوال duration_seconds نمی‌گذاشت، تایمر تک‌سوالی به‌طور
-- خاموش/ضمنی از minutes_per_mcq / minutes_per_essay استفاده می‌کرد.
-- این دو ستون در اصل فقط برای محاسبه‌ی خودکار «مدت زمان کل آزمون»
-- (duration_policy = 'auto_calculate') طراحی شده بودند، نه به‌عنوان
-- fallback تایمر تک‌سوالی. نتیجه: حتی وقتی استاد فقط زمان کل آزمون را
-- تنظیم می‌کرد و اصلاً قصد محدودکردن تک‌تک سوالات را نداشت، دانشجو
-- به‌زور هر ۲ دقیقه (MCQ) یا ۴ دقیقه (تشریحی) مجبور به رفتن به سوال
-- بعد می‌شد (و امکان برگشت هم چون canNavigatePrev=false همیشه false
-- است، وجود نداشت).
--
-- راه‌حل: یک ستون صریح per_question_timing_enabled اضافه می‌شود.
-- پیش‌فرض false یعنی رفتار امن/غیرمزاحم: برای همه‌ی آزمون‌های موجود
-- (created قبل از این migration)، از این به بعد تایمر تک‌سوالی خاموش
-- می‌شود مگر این‌که خودِ سوال duration_seconds صریح داشته باشد —
-- یعنی این migration هیچ رفتار واقعاً موردنیازِ فعلی را نمی‌شکند، فقط
-- fallback ناخواسته‌ی قبلی را حذف می‌کند. استاد از این پس باید صراحتاً
-- این گزینه را در پنل فعال کند تا minutes_per_mcq/minutes_per_essay
-- به‌عنوان سقف زمانی هر سوال هم اعمال شوند.
-- ====================================================================

alter table public.exams
  add column if not exists per_question_timing_enabled boolean not null default false;

-- RLS جدیدی لازم نیست: استاد از طریق پالیسی موجود
-- (exams_manage_own_section، migration 0003/0005) از قبل اجازه‌ی
-- update ستون‌های exams مربوط به section خودش را دارد؛ این ستون هم
-- همان پالیسی را به ارث می‌برد.

-- ====================================================================
-- بررسی صحت بعد از اجرا (اختیاری):
--   select column_name, data_type, column_default
--   from information_schema.columns
--   where table_name = 'exams' and column_name = 'per_question_timing_enabled';
--   -- باید یک ردیف با data_type=boolean و column_default=false برگردد
--
--   select exam_id, per_question_timing_enabled from public.exams limit 5;
--   -- همه‌ی آزمون‌های قدیمی باید false باشند
-- ====================================================================
