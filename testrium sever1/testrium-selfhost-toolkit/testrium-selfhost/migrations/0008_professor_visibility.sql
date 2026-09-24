-- ====================================================================
-- Testrium — Migration 0008: نمایش محدود اطلاعات استاد به دانشجوی enrolled
-- ====================================================================
-- در 0002 عمداً هیچ policy خواندنی روی professors برای anon/authenticated
-- تعریف نشده بود (چون professor_code عملاً نقش «کد ورود/رمز» را دارد و
-- نباید لیست کامل آن به هرکسی داده شود).
--
-- اما صفحه‌ی «دوره‌های من» دانشجو نیاز دارد نام استاد هر section را
-- ببیند. راه‌حل: policy محدود که فقط ردیف professors مربوط به section
-- هایی که خودِ همین دانشجو در آن‌ها enrolled است را برمی‌گرداند (نه کل
-- لیست استادها، و کد ورود استاد هم در کوئری‌های کلاینت انتخاب نمی‌شود
-- ولی چون select policy روی کل ردیف است، در سطح دیتابیس professor_code
-- هم قابل خواندن می‌شود اگر کوئری صریحاً آن را بخواهد — پس در کد
-- کلاینت هرگز professor_code را select نکن، فقط id/full_name).
--
-- استاد هم اجازه دارد ردیف خودش را بخواند (برای احتمال استفاده‌ی
-- آینده در پروفایل استاد).
-- ====================================================================

create policy "professors_select_for_enrolled_student"
  on public.professors for select
  using (
    id in (
      select s.professor_id
      from public.sections s
      where public.is_enrolled_in_section(s.section_id)
    )
    or auth_user_id = auth.uid()
  );
