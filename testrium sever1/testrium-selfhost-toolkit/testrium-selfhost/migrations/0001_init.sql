-- ====================================================================
-- Testrium — Migration 0001: Init
-- طبق سند بخش ۹ (معماری Backend)
-- ====================================================================

-- ── جدول students ──
-- از قبل توسط استاد/ادمین پر می‌شود (import دستی یا از طریق پنل جدا).
-- auth_user_id فقط بعد از اولین verify موفق پر می‌شود (link به session
-- ناشناس Supabase Auth).
create table if not exists public.students (
  student_code   text primary key,
  first_name     text not null,
  last_name      text not null,
  student_class  text not null default '',
  is_active      boolean not null default true,
  auth_user_id   uuid unique references auth.users(id),
  created_at     timestamptz not null default now()
);

create index if not exists idx_students_auth_user_id on public.students(auth_user_id);

alter table public.students enable row level security;

-- خواندن/نوشتن مستقیم این جدول از کلاینت مجاز نیست؛ فقط از طریق
-- Edge Function با service_role انجام می‌شود. هیچ policy ای برای
-- anon/authenticated تعریف نمی‌شود (پیش‌فرض: دسترسی صفر).


-- ── جدول exams ──
-- فقط اشاره‌گر سبک به فایل فعلی روی Storage؛ متادیتای غیرحساس.
create table if not exists public.exams (
  exam_id                  text primary key,
  current_file             text not null,
  exam_title               text not null,
  exam_type                text not null default 'midterm',
  course_name              text not null default '',
  available_from           timestamptz not null,
  available_until          timestamptz not null,
  duration_minutes         integer not null default 0,
  duration_policy          text not null default 'fixed'
                            check (duration_policy in ('fixed', 'auto_calculate')),
  minutes_per_mcq           numeric not null default 2.0,
  minutes_per_essay         numeric not null default 4.0,
  timer_policy              text not null default 'from_student_start'
                            check (timer_policy in ('from_student_start', 'until_exam_end')),
  show_answers_after_exam   boolean not null default false,
  allow_late_join           boolean not null default false,
  created_at                timestamptz not null default now()
);

create index if not exists idx_exams_available_from on public.exams(available_from desc);

alter table public.exams enable row level security;

-- خواندن عمومی آزاد — فقط متادیتای غیرحساس است، نه پاسخ صحیح سوالات
-- (که در فایل روی Storage است و مسیر جدا دارد).
create policy "exams_select_public"
  on public.exams for select
  using (true);


-- ── جدول exam_sessions ──
-- دیتای پویا/حساس؛ هر سطر معادل یک participation یک دانشجو در یک آزمون.
create table if not exists public.exam_sessions (
  id                   uuid primary key default gen_random_uuid(),
  exam_id              text not null references public.exams(exam_id),
  student_code         text not null,
  student_first_name   text not null,
  student_last_name    text not null,
  student_class        text not null default '',
  start_time           timestamptz,
  end_time             timestamptz,
  is_submitted         boolean not null default false,
  answers              jsonb not null default '[]'::jsonb,
  earned_score         numeric not null default 0,
  mcq_correct_count    integer not null default 0,
  mcq_wrong_count      integer not null default 0,
  mcq_skipped_count    integer not null default 0,
  mcq_total_count      integer not null default 0,
  percent_correct      numeric generated always as (
                          case when mcq_total_count > 0
                            then round((mcq_correct_count::numeric / mcq_total_count) * 100, 1)
                            else 0 end
                        ) stored,
  percent_wrong        numeric generated always as (
                          case when mcq_total_count > 0
                            then round((mcq_wrong_count::numeric / mcq_total_count) * 100, 1)
                            else 0 end
                        ) stored,
  percent_skipped      numeric generated always as (
                          case when mcq_total_count > 0
                            then round((mcq_skipped_count::numeric / mcq_total_count) * 100, 1)
                            else 0 end
                        ) stored,
  synced_at            timestamptz not null default now(),

  constraint uq_exam_student unique (exam_id, student_code)
);

create index if not exists idx_exam_sessions_exam_student
  on public.exam_sessions(exam_id, student_code);

alter table public.exam_sessions enable row level security;

-- هر دانشجو فقط به سطر خودش دسترسی دارد (بر اساس auth.uid() که در
-- Edge Function تایید کد دانشجویی به student_code متصل شده است).
create policy "exam_sessions_select_own"
  on public.exam_sessions for select
  using (
    student_code = (
      select student_code from public.students where auth_user_id = auth.uid()
    )
  );

create policy "exam_sessions_insert_own"
  on public.exam_sessions for insert
  with check (
    student_code = (
      select student_code from public.students where auth_user_id = auth.uid()
    )
  );

create policy "exam_sessions_update_own"
  on public.exam_sessions for update
  using (
    student_code = (
      select student_code from public.students where auth_user_id = auth.uid()
    )
  );


-- ── Storage bucket ──
insert into storage.buckets (id, name, public)
values ('exam-content', 'exam-content', true)
on conflict (id) do nothing;

-- خواندن عمومی فایل‌های سوالات (immutable، content-hash naming)
create policy "exam_content_public_read"
  on storage.objects for select
  using (bucket_id = 'exam-content');

-- نوشتن فقط از طریق service_role (پنل استاد/ادمین)، نه از کلاینت دانشجو.
