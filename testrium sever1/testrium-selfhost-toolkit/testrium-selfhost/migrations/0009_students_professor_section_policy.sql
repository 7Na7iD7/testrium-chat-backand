-- ====================================================================
-- Testrium — Migration 0009: رفع باگ RLS در خواندن students توسط استاد
-- ====================================================================
-- علت باگ: policy «students_professor_view_enrolled» در 0002 به جدول
-- قدیمی course_enrollments/courses رفرنس می‌دهد. بعد از مهاجرت به
-- sections/enrollments (0003 به بعد)، این policy هیچ‌وقت بروزرسانی
-- نشد. نتیجه: استاد نمی‌تواند اطلاعات دانشجویی را که از طریق section
-- جدید enroll شده بخواند (چون هیچ policy match نمی‌شود)، در حالی که
-- ادمین چون از service_role در edge function استفاده می‌کند این مشکل
-- را ندارد. این باعث می‌شود join تو کوئری کلاینت null برگردد و کد
-- (بدون چک null) کرش کند: «type 'Null' is not a subtype of type
-- 'Map<String, dynamic>'».
--
-- policy قدیمی 0002 عمداً حذف نمی‌شود (بی‌ضرر است، برای داده‌ی
-- migrate-نشده‌ی احتمالی)؛ policy جدید کنارش اضافه می‌شود.
-- ====================================================================

create policy "students_professor_view_enrolled_section"
  on public.students for select
  using (
    student_code in (
      select e.student_code
      from public.enrollments e
      where public.is_own_section(e.section_id)
    )
  );
