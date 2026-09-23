-- ====================================================================
-- Testrium — Migration 0016: max_score رو‌نویس‌شده + View روند کلاس
-- ====================================================================
-- بخش الف) exams.max_score
-- ------------------------
-- مجموع نمره‌ی سوالات هر آزمون فقط داخل فایل JSON روی Storage موجود
-- است، نه در دیتابیس. این یعنی برای محاسبه‌ی درصد/قبولی/روند، همیشه
-- باید فایل هر آزمون از Storage دانلود و parse شود — هم کند است (به‌خصوص
-- برای نمودار روند که چند آزمون را هم‌زمان لازم دارد) و هم اجازه نمی‌دهد
-- یک VIEW ساده‌ی SQL این مجموع را حساب کند (چون SQL به محتوای فایل
-- Storage دسترسی ندارد).
--
-- راه‌حل: یک ستون denormalized روی خودِ exams که هنگام ساخت/ویرایش
-- آزمون (createExamAction/updateExamAction در professor_exams_provider.dart)
-- از همان محتوای JSON که کلاینت در دست دارد محاسبه و همراه با
-- current_file ذخیره می‌شود — بدون هیچ round-trip اضافه.
--
-- نکته‌ی مهم درباره‌ی داده‌ی قدیمی: آزمون‌هایی که قبل از این migration
-- ساخته شده‌اند max_score را null خواهند داشت (چون محتوای Storage
-- آن‌ها را نمی‌توان از داخل یک migration SQL خواند). کد کلاینت
-- (exam_analytics_service/provider) برای این حالت fallback دارد: اگر
-- max_score نال یا صفر بود، دوباره از روی فایل Storage محاسبه می‌کند —
-- یعنی هیچ داده‌ای گم نمی‌شود، فقط بهینه‌سازی برای آزمون‌های جدید فوری
-- اعمال می‌شود و برای قدیمی‌ها به‌محض اولین ویرایش (updateExamAction)
-- تکمیل می‌شود.
-- ====================================================================

alter table public.exams
  add column if not exists max_score numeric;

-- ====================================================================
-- بخش ب) exam_section_trend (VIEW برای نمودار روند کلاس)
-- ------------------------
-- به‌جای اینکه کلاینت برای «روند بین N آزمون یک کلاس» به ازای هر آزمون
-- یک query جدا بزند (N round-trip)، این View در یک query واحد میانگین
-- نمره + تعداد شرکت‌کننده‌ی هر آزمون یک section را برمی‌گرداند.
--
-- `security_invoker = true` یعنی این View با همان نقش/RLS کاربر
-- فراخوان اجرا می‌شود (نه owner دیتابیس) — پس همان پالیسی‌های موجود
-- exams_manage_own_section و exam_sessions_professor_view_own_section
-- به‌طور خودکار روی این View هم اعمال می‌شوند و استاد فقط ردیف‌های
-- section خودش را می‌بیند؛ نیازی به policy جداگانه روی View نیست.
-- (نیازمند Postgres 15+ — نسخه‌ی استاندارد پروژه‌های جدید Supabase.)
-- ====================================================================

create or replace view public.exam_section_trend
with (security_invoker = true)
as
select
  e.exam_id,
  e.section_id,
  e.exam_title,
  e.available_from,
  e.max_score,
  e.pass_threshold_percent,
  count(es.id) filter (where es.is_submitted) as participant_count,
  avg(es.earned_score) filter (where es.is_submitted) as avg_score
from public.exams e
left join public.exam_sessions es on es.exam_id = e.exam_id
group by e.exam_id, e.section_id, e.exam_title, e.available_from, e.max_score, e.pass_threshold_percent;

grant select on public.exam_section_trend to authenticated;

-- ====================================================================
-- بخش ج) Index کمکی
-- ------------------------
-- idx_exam_sessions_exam_student (migration 0001) روی (exam_id,
-- student_code) از قبل به‌عنوان prefix برای فیلتر تنها روی exam_id هم
-- استفاده می‌شود، پس Index جدیدی برای این View لازم نیست. تنها Index
-- اضافه‌ای که برای فیلتر Realtime چندتایی (IN روی exam_id، در
-- classTrendProvider سمت کلاینت) کمک می‌کند همین ایندکس موجود است.
-- ====================================================================

-- ====================================================================
-- بررسی صحت بعد از اجرا (اختیاری):
--   select column_name from information_schema.columns
--   where table_name = 'exams' and column_name = 'max_score';
--
--   select * from public.exam_section_trend limit 1;
-- ====================================================================
