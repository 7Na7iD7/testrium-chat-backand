-- ====================================================================
-- Testrium — Migration 0002: پشتیبانی چند-استادی
-- استاد هم دقیقاً مثل دانشجو با یک کد یکتا (professor_code) و
-- anonymous auth وارد می‌شود — نه ایمیل/پسورد. تشخیص نقش (دانشجو/استاد)
-- توسط Edge Function واحد `verify-code` انجام می‌شود.
-- ====================================================================

-- ── جدول professors ──
-- دقیقاً هم‌ساختار با students: از قبل توسط ادمین/بولک-اسکریپت پر
-- می‌شود، auth_user_id فقط بعد از اولین ورود موفق پر می‌شود.
create table if not exists public.professors (
  id             uuid primary key default gen_random_uuid(),
  professor_code text unique not null,
  full_name      text not null,
  is_active      boolean not null default true,
  auth_user_id   uuid unique references auth.users(id),
  created_at     timestamptz not null default now()
);

create index if not exists idx_professors_auth_user_id on public.professors(auth_user_id);

alter table public.professors enable row level security;

-- خواندن/نوشتن مستقیم این جدول از کلاینت مجاز نیست؛ فقط از طریق
-- Edge Function با service_role (مثل جدول students).


-- ── جدول courses ──
create table if not exists public.courses (
  course_id     uuid primary key default gen_random_uuid(),
  professor_id  uuid not null references public.professors(id) on delete cascade,
  course_name   text not null,
  created_at    timestamptz not null default now()
);

create index if not exists idx_courses_professor on public.courses(professor_id);

alter table public.courses enable row level security;

create policy "courses_all_own"
  on public.courses for all
  using (
    professor_id in (select id from public.professors where auth_user_id = auth.uid())
  )
  with check (
    professor_id in (select id from public.professors where auth_user_id = auth.uid())
  );


-- ── ربط exams به courses ──
alter table public.exams
  add column if not exists course_id uuid references public.courses(course_id);

create index if not exists idx_exams_course on public.exams(course_id);

create policy "exams_manage_own_course"
  on public.exams for all
  using (
    course_id in (
      select course_id from public.courses
      where professor_id in (select id from public.professors where auth_user_id = auth.uid())
    )
  )
  with check (
    course_id in (
      select course_id from public.courses
      where professor_id in (select id from public.professors where auth_user_id = auth.uid())
    )
  );


-- ── جدول course_enrollments ──
create table if not exists public.course_enrollments (
  course_id     uuid not null references public.courses(course_id) on delete cascade,
  student_code  text not null references public.students(student_code) on delete cascade,
  created_at    timestamptz not null default now(),
  primary key (course_id, student_code)
);

create index if not exists idx_enrollments_student on public.course_enrollments(student_code);

alter table public.course_enrollments enable row level security;

create policy "enrollments_manage_own_course"
  on public.course_enrollments for all
  using (
    course_id in (
      select course_id from public.courses
      where professor_id in (select id from public.professors where auth_user_id = auth.uid())
    )
  )
  with check (
    course_id in (
      select course_id from public.courses
      where professor_id in (select id from public.professors where auth_user_id = auth.uid())
    )
  );


-- ── دسترسی استاد به exam_sessions درس خودش (برای مشاهده/تصحیح) ──
create policy "exam_sessions_professor_view_own_course"
  on public.exam_sessions for select
  using (
    exam_id in (
      select exam_id from public.exams
      where course_id in (
        select course_id from public.courses
        where professor_id in (select id from public.professors where auth_user_id = auth.uid())
      )
    )
  );

create policy "exam_sessions_professor_grade_own_course"
  on public.exam_sessions for update
  using (
    exam_id in (
      select exam_id from public.exams
      where course_id in (
        select course_id from public.courses
        where professor_id in (select id from public.professors where auth_user_id = auth.uid())
      )
    )
  );


-- ── دسترسی استاد به students (برای دیدن/مدیریت دانشجویان درس خودش) ──
create policy "students_professor_view_enrolled"
  on public.students for select
  using (
    student_code in (
      select student_code from public.course_enrollments
      where course_id in (
        select course_id from public.courses
        where professor_id in (select id from public.professors where auth_user_id = auth.uid())
      )
    )
  );

create policy "students_professor_insert"
  on public.students for insert
  with check (
    auth.uid() in (select auth_user_id from public.professors where auth_user_id is not null)
  );

create policy "students_professor_update_enrolled"
  on public.students for update
  using (
    auth.uid() in (select auth_user_id from public.professors where auth_user_id is not null)
  );
