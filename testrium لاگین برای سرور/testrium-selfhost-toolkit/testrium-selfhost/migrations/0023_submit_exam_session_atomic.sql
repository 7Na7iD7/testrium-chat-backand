-- ====================================================================
-- Testrium — Migration 0023: بستن پنجره‌ی رقابتی Idempotency ثبت آزمون
-- ====================================================================
-- طبق بخش ۲۲.۱ گزارش: چک «قبلاً ثبت‌شده» (select) و نوشتن نتیجه‌ی
-- جدید (upsert) دو عملیات جدا از سمت کلاینت Edge Function بودند —
-- بین این دو، از نظر تئوری یک پنجره‌ی رقابتی بسیار محدود وجود داشت.
-- این تابع همان دو عملیات را در یک دستور اتمیک واحد ادغام می‌کند:
-- «فقط اگر هنوز is_submitted=false است، درج/بروزرسانی کن» — با یک
-- شرط WHERE روی خودِ DO UPDATE، نه یک چک جداگانه‌ی قبل از آن. اگر دو
-- درخواست کاملاً هم‌زمان برسند، Postgres خودش این عملیات را سریال
-- می‌کند؛ دومی چون شرط WHERE را نمی‌بیند برقرار، هیچ ردیفی
-- برنمی‌گرداند — دقیقاً همان رفتار «نتیجه‌ی قبلی را برگردان، رونویسی
-- نکن» که برای Retry معمولی هم می‌خواستیم.
-- ====================================================================

create or replace function public.submit_exam_session_if_not_submitted(
  p_exam_id text,
  p_student_code text,
  p_student_first_name text,
  p_student_last_name text,
  p_student_class text,
  p_start_time timestamptz,
  p_end_time timestamptz,
  p_answers jsonb,
  p_earned_score numeric,
  p_mcq_correct_count integer,
  p_mcq_wrong_count integer,
  p_mcq_skipped_count integer,
  p_mcq_total_count integer,
  p_is_fully_graded boolean
)
returns table (inserted boolean, earned_score numeric)
language plpgsql
security definer
set search_path = public
as $$
declare
  v_row_id uuid;
  v_final_score numeric;
begin
  insert into public.exam_sessions (
    exam_id, student_code, student_first_name, student_last_name, student_class,
    start_time, end_time, answers, earned_score,
    mcq_correct_count, mcq_wrong_count, mcq_skipped_count, mcq_total_count,
    is_submitted, is_fully_graded, synced_at
  )
  values (
    p_exam_id, p_student_code, p_student_first_name, p_student_last_name, p_student_class,
    p_start_time, p_end_time, p_answers, p_earned_score,
    p_mcq_correct_count, p_mcq_wrong_count, p_mcq_skipped_count, p_mcq_total_count,
    true, p_is_fully_graded, now()
  )
  on conflict (exam_id, student_code) do update
    set student_first_name = excluded.student_first_name,
        student_last_name = excluded.student_last_name,
        student_class = excluded.student_class,
        start_time = excluded.start_time,
        end_time = excluded.end_time,
        answers = excluded.answers,
        earned_score = excluded.earned_score,
        mcq_correct_count = excluded.mcq_correct_count,
        mcq_wrong_count = excluded.mcq_wrong_count,
        mcq_skipped_count = excluded.mcq_skipped_count,
        mcq_total_count = excluded.mcq_total_count,
        is_submitted = true,
        is_fully_graded = excluded.is_fully_graded,
        synced_at = now()
    where public.exam_sessions.is_submitted = false
  returning id into v_row_id;

  if v_row_id is not null then
    return query select true, p_earned_score;
  end if;

  select es.earned_score into v_final_score
  from public.exam_sessions es
  where es.exam_id = p_exam_id and es.student_code = p_student_code;

  return query select false, v_final_score;
end;
$$;

grant execute on function public.submit_exam_session_if_not_submitted(
  text, text, text, text, text, timestamptz, timestamptz, jsonb, numeric, integer, integer, integer, integer, boolean
) to service_role;

-- ====================================================================
-- بررسی صحت بعد از اجرا (اختیاری):
--   select proname from pg_proc where proname = 'submit_exam_session_if_not_submitted';
-- ====================================================================
