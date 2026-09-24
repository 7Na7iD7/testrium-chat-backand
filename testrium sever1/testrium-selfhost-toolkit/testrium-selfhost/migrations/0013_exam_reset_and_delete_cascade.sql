-- ====================================================================
-- Testrium — Migration 0013: ریست نتایج آزمون + حذف آبشاری کامل
-- ====================================================================
-- مشکل: وقتی آزمونی از قبل session یا appeal دارد، حذف مستقیم آن با
-- خطای FK (23503) رد می‌شود (رفتار درست و عمدی migration 0001 برای
-- جلوگیری از یتیم‌شدن داده‌ی دانشجو). اما استاد به دو سناریوی متفاوت
-- نیاز دارد که هیچ‌کدام «حذف ساده» نیستند:
--
--   ۱) reset_exam_sessions: آزمون از اساس مشکل داشته (تایمر غلط،
--      سوال اشتباه) و استاد می‌خواهد نتایج/اعتراض‌های فعلی را پاک کند
--      تا بعد از اصلاح (updateExamAction) دوباره برای دانشجوها باز
--      شود — خودِ exam دست‌نخورده می‌ماند، فقط داده‌ی participation
--      پاک می‌شود.
--
--   ۲) delete_exam_cascade: استاد می‌خواهد کل آزمون (همراه با هر
--      نتیجه/اعتراضی که رویش ثبت شده) کاملاً از بین برود — برای
--      وقتی آزمون اصلاً نباید وجود داشته باشد.
--
-- هر دو تابع SECURITY DEFINER هستند (برای عبور از RLS به‌صورت کنترل‌شده)
-- ولی هرکدام ابتدا صریحاً چک می‌کنند که caller واقعاً استاد صاحب
-- section همان exam باشد؛ در غیر این صورت exception می‌دهند و کلاینت
-- خطای PostgrestException دریافت می‌کند (نه دسترسی بی‌قید‌وشرط).
-- ====================================================================

create or replace function public.reset_exam_sessions(p_exam_id text)
returns void
language plpgsql
security definer
set search_path = public
as $$
begin
  if not exists (
    select 1 from public.exams
    where exam_id = p_exam_id
      and public.is_own_section(section_id)
  ) then
    raise exception 'access_denied';
  end if;

  delete from public.exam_appeals where exam_id = p_exam_id;
  delete from public.exam_sessions where exam_id = p_exam_id;
end;
$$;

grant execute on function public.reset_exam_sessions(text) to authenticated;


create or replace function public.delete_exam_cascade(p_exam_id text)
returns void
language plpgsql
security definer
set search_path = public
as $$
begin
  if not exists (
    select 1 from public.exams
    where exam_id = p_exam_id
      and public.is_own_section(section_id)
  ) then
    raise exception 'access_denied';
  end if;

  delete from public.exam_appeals where exam_id = p_exam_id;
  delete from public.exam_sessions where exam_id = p_exam_id;
  delete from public.exams where exam_id = p_exam_id;
end;
$$;

grant execute on function public.delete_exam_cascade(text) to authenticated;

-- ====================================================================
-- بررسی صحت بعد از اجرا (اختیاری):
--   select proname from pg_proc
--   where proname in ('reset_exam_sessions', 'delete_exam_cascade');
--   -- باید هر دو ردیف برگردند
-- ====================================================================
