-- ====================================================================
-- Testrium — Migration 0004: دسترسی نوشتن استاد به enrollments سکشن خودش
-- ====================================================================
-- در 0003 عمداً enrollments هیچ policy نوشتنی برای کلاینت نداشت (فقط
-- service_role). اما این باعث می‌شود CSV import دانشجو در پنل استاد
-- (که قبلاً مستقیم روی course_enrollments می‌نوشت) از کار بیفتد.
-- این migration اجازه می‌دهد استاد فقط برای section هایی که خودش
-- professor_id آن‌هاست، enrollment بسازد/ویرایش کند — نه برای section
-- استادهای دیگر.

create policy "enrollments_professor_manage_own_section"
  on public.enrollments for all
  using (
    section_id in (
      select section_id from public.sections
      where professor_id in (select id from public.professors where auth_user_id = auth.uid())
    )
  )
  with check (
    section_id in (
      select section_id from public.sections
      where professor_id in (select id from public.professors where auth_user_id = auth.uid())
    )
  );
