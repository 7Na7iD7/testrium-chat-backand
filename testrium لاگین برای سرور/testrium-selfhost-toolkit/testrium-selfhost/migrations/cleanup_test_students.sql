-- ====================================================================
-- Testrium — پاک‌سازی دستی: حذف تاریخچه‌ی آزمون قدیمی که به‌اشتباه
-- به کدهای دانشجویی تازه‌استفاده‌شده (40200100, 40200101) چسبیده بود
-- ====================================================================
-- این اسکریپت دستی است، نه یک migration شماره‌دار — فقط برای پاک‌سازی
-- وضعیت فعلی محیط تست اجرا می‌شود. قبل از اجرا کدهای مربوطه را در
-- خط پایین تنظیم کن.
--
-- ترتیب حذف مهم است (به‌خاطر FK):
--   ۱) exam_appeals (اگر اعتراضی هم ثبت شده بود)
--   ۲) exam_sessions
--   ۳) enrollments (اختیاری — اگر می‌خواهی enrollment فعلی هم پاک شود
--      تا از صفر دوباره enroll کنی؛ اگر enrollment فعلی درست است و فقط
--      exam_sessions قدیمی مشکل داشت، این خط را کامنت نگه‌دار)
-- ====================================================================

do $$
declare
  v_codes text[] := array['40200100', '40200101'];
begin
  delete from public.exam_appeals
  where student_code = any(v_codes);

  delete from public.exam_sessions
  where student_code = any(v_codes);

  -- در صورت نیاز به ریست کامل enrollment هم، این خط را از کامنت خارج کن:
  -- delete from public.enrollments where student_code = any(v_codes);

  raise notice 'پاک‌سازی برای کدهای % انجام شد', v_codes;
end $$;

-- ====================================================================
-- بررسی صحت بعد از اجرا (اختیاری):
--   select * from public.exam_sessions where student_code in ('40200100','40200101');
--   -- باید هیچ ردیفی برنگردد
-- ====================================================================


-- ====================================================================
-- کوئری کمکی: پیدا کردن همه‌ی کدهایی که احتمالاً دچار همین مشکل هستند
-- (یعنی نام ثبت‌شده در students با نامی که در یکی از exam_sessions
-- قدیمی همان کد ذخیره شده فرق دارد — نشانه‌ی این‌که کد قبلاً برای فرد
-- دیگری استفاده شده بوده)
-- ====================================================================

select
  s.student_code,
  s.first_name || ' ' || s.last_name as current_name,
  es.student_first_name || ' ' || es.student_last_name as name_in_old_session,
  es.exam_id,
  es.synced_at
from public.students s
join public.exam_sessions es on es.student_code = s.student_code
where s.first_name is distinct from es.student_first_name
   or s.last_name is distinct from es.student_last_name
order by es.synced_at desc;
