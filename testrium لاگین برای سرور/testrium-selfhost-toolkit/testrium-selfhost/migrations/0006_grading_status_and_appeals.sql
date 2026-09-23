-- ====================================================================
-- Testrium — Migration 0006: وضعیت واقعی تصحیح + سیستم اعتراض به نمره
-- ====================================================================
-- بخش الف) رفع باگ: earned_score همیشه NOT NULL DEFAULT 0 است، پس
-- کلاینت (exam_result_screen) نمی‌تواند «تصحیح‌نشده» را از «نمره‌ی
-- صفر واقعی» تشخیص دهد. یک ستون صریح اضافه می‌شود که فقط با تایید
-- دستی استاد (یا خودکار اگر آزمون اصلاً سوال تشریحی نداشت) true می‌شود.
--
-- بخش ب) جدول exam_appeals: اعتراض دانشجو به نمره‌ی یک آزمون/سوال
-- مشخص + پاسخ استاد.
-- ====================================================================

alter table public.exam_sessions
  add column if not exists is_fully_graded boolean not null default false;

-- برای session های قدیمی که هیچ سوال تشریحی نداشتند (تمام answeredEssayCount=0)
-- امکان تشخیص دقیق از اینجا نیست (answers در jsonb است)، پس فقط پیش‌رونده
-- true می‌ماند و کد کلاینت از این به بعد هنگام submit مقدار درست را می‌فرستد.


create table if not exists public.exam_appeals (
  id                 uuid primary key default gen_random_uuid(),
  exam_id            text not null references public.exams(exam_id),
  student_code       text not null,
  question_id        integer,              -- null یعنی اعتراض کلی به کل آزمون
  message            text not null,
  status             text not null default 'pending'
                     check (status in ('pending', 'resolved', 'rejected')),
  professor_response text,
  created_at         timestamptz not null default now(),
  resolved_at        timestamptz
);

create index if not exists idx_appeals_exam_student on public.exam_appeals(exam_id, student_code);

alter table public.exam_appeals enable row level security;

-- دانشجو فقط اعتراض‌های خودش را می‌بیند/می‌سازد
create policy "appeals_select_own"
  on public.exam_appeals for select
  using (student_code = public.current_student_code());

create policy "appeals_insert_own"
  on public.exam_appeals for insert
  with check (student_code = public.current_student_code());

-- استاد اعتراض‌های مربوط به سکشن خودش را می‌بیند و پاسخ می‌دهد
create policy "appeals_select_professor_own_section"
  on public.exam_appeals for select
  using (
    exam_id in (select exam_id from public.exams where public.is_own_section(section_id))
  );

create policy "appeals_update_professor_own_section"
  on public.exam_appeals for update
  using (
    exam_id in (select exam_id from public.exams where public.is_own_section(section_id))
  );
