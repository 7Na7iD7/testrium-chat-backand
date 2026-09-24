-- ====================================================================
-- Testrium — Migration 0011: بستن حفره‌ی امنیتی در بروزرسانی exam_sessions
-- ====================================================================
-- مشکل (در migration 0001):
-- سیاست exam_sessions_update_own فقط مالکیت ردیف (student_code) را
-- چک می‌کند و هیچ WITH CHECK جداگانه‌ای ندارد. طبق رفتار پیش‌فرض
-- Postgres، وقتی WITH CHECK تعریف نشود، همان عبارت USING برای
-- اعتبارسنجی مقدار جدید هم استفاده می‌شود.
--
-- نتیجه: هر دانشجو، حتی بعد از ثبت نهایی آزمونش (is_submitted=true)،
-- می‌تواند مستقیماً (بدون نیاز به اپ Testrium، صرفاً با یک درخواست
-- ساده به Supabase با همان نشست خودش) مقدار earned_score و
-- is_fully_graded را به دلخواه تغییر دهد و بدون تایید استاد، نمره‌ی
-- نهایی خودش را جعل کند.
--
-- راه‌حل: دانشجو فقط تا وقتی ردیف هنوز is_submitted=false است اجازه‌ی
-- بروزرسانی دارد. در جریان فعلی اپ، session همیشه یک‌جا (با
-- is_submitted=true) insert می‌شود، پس این تغییر هیچ رفتار قانونی
-- فعلی را نمی‌شکند. به محض ثبت نهایی، دانشجو دیگر هیچ اجازه‌ی نوشتنی
-- روی آن ردیف ندارد؛ فقط استاد (از طریق سیاست‌های جداگانه‌ی
-- exam_sessions_professor_grade_own_section که در migration 0005
-- تعریف شده) می‌تواند آن را ویرایش کند.
-- ====================================================================

drop policy if exists "exam_sessions_update_own" on public.exam_sessions;

create policy "exam_sessions_update_own"
  on public.exam_sessions for update
  using (
    student_code = public.current_student_code()
    and is_submitted = false
  )
  with check (
    student_code = public.current_student_code()
  );

-- ── بررسی صحت اعمال شدن (اختیاری، برای اجرای دستی بعد از migration) ──
-- select polname, pg_get_expr(polqual, polrelid) as using_expr,
--        pg_get_expr(polwithcheck, polrelid) as check_expr
-- from pg_policy
-- where polrelid = 'public.exam_sessions'::regclass
--   and polname = 'exam_sessions_update_own';
