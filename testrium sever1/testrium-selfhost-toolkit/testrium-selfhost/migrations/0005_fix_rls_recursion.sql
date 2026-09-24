-- ====================================================================
-- Testrium — Migration 0005: رفع recursion در RLS بین sections/enrollments
-- ====================================================================
-- علت خطا: پالیسی sections_select_enrolled_student به enrollments رفرنس
-- می‌دهد و پالیسی‌های enrollments_select_professor_own_section /
-- enrollments_professor_manage_own_section به sections رفرنس می‌دهند.
-- Postgres برای ارزیابی RLS یک جدول باید RLS جدول دیگر را هم ارزیابی کند
-- که همان حلقه است. راه‌حل: توابع SECURITY DEFINER که RLS را دور می‌زنند.

create or replace function public.current_professor_id()
returns uuid
language sql
security definer
stable
set search_path = public
as $$
  select id from public.professors where auth_user_id = auth.uid()
$$;

create or replace function public.current_student_code()
returns text
language sql
security definer
stable
set search_path = public
as $$
  select student_code from public.students where auth_user_id = auth.uid()
$$;

create or replace function public.is_own_section(p_section_id uuid)
returns boolean
language sql
security definer
stable
set search_path = public
as $$
  select exists (
    select 1 from public.sections
    where section_id = p_section_id
      and professor_id = public.current_professor_id()
  )
$$;

create or replace function public.is_enrolled_in_section(p_section_id uuid)
returns boolean
language sql
security definer
stable
set search_path = public
as $$
  select exists (
    select 1 from public.enrollments
    where section_id = p_section_id
      and student_code = public.current_student_code()
      and status = 'enrolled'
  )
$$;

-- ── بازنویسی پالیسی‌های sections ──
drop policy if exists "sections_manage_own" on public.sections;
drop policy if exists "sections_select_enrolled_student" on public.sections;

create policy "sections_manage_own"
  on public.sections for all
  using (professor_id = public.current_professor_id())
  with check (professor_id = public.current_professor_id());

create policy "sections_select_enrolled_student"
  on public.sections for select
  using (public.is_enrolled_in_section(section_id));

-- ── بازنویسی پالیسی‌های enrollments ──
drop policy if exists "enrollments_select_own" on public.enrollments;
drop policy if exists "enrollments_select_professor_own_section" on public.enrollments;
drop policy if exists "enrollments_professor_manage_own_section" on public.enrollments;

create policy "enrollments_select_own"
  on public.enrollments for select
  using (student_code = public.current_student_code());

create policy "enrollments_select_professor_own_section"
  on public.enrollments for select
  using (public.is_own_section(section_id));

create policy "enrollments_professor_manage_own_section"
  on public.enrollments for all
  using (public.is_own_section(section_id))
  with check (public.is_own_section(section_id));

-- ── همین اصلاح برای exams (همون الگو، برای جلوگیری از مشکل مشابه در آینده) ──
drop policy if exists "exams_manage_own_section" on public.exams;
drop policy if exists "exams_select_enrolled_student" on public.exams;

create policy "exams_manage_own_section"
  on public.exams for all
  using (public.is_own_section(section_id))
  with check (public.is_own_section(section_id));

create policy "exams_select_enrolled_student"
  on public.exams for select
  using (public.is_enrolled_in_section(section_id));

-- ── و exam_sessions ──
drop policy if exists "exam_sessions_professor_view_own_section" on public.exam_sessions;
drop policy if exists "exam_sessions_professor_grade_own_section" on public.exam_sessions;

create policy "exam_sessions_professor_view_own_section"
  on public.exam_sessions for select
  using (
    exam_id in (select exam_id from public.exams where public.is_own_section(section_id))
  );

create policy "exam_sessions_professor_grade_own_section"
  on public.exam_sessions for update
  using (
    exam_id in (select exam_id from public.exams where public.is_own_section(section_id))
  );