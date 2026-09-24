-- ====================================================================
-- Testrium — Migration 0007: رفع خطای «ثبت آزمون» — اجازه‌ی آپلود
-- فایل سوالات به bucket exam-content برای استادها
-- ====================================================================
-- در 0001_init.sql فقط policy خواندن عمومی (select) روی
-- storage.objects برای bucket 'exam-content' تعریف شده بود؛ طبق کامنت
-- خودِ آن migration قرار بود نوشتن «فقط از طریق service_role» انجام
-- شود. اما کد فعلی پنل استاد (professor_exams_provider.dart) مستقیماً
-- از کلاینت (با session عادی استاد) uploadBinary صدا می‌زند — نه از
-- یک Edge Function با service_role. چون هیچ INSERT policy ای برای
-- authenticated/anon وجود نداشت، این آپلود همیشه با خطای RLS شکست
-- می‌خورد و createExamAction آن را در catch قورت می‌داد (همان چیزی که
-- در گفتگوی قبلی دیدیم و رفعش کردیم تا لاگ‌پذیر شود).
--
-- این policy فقط به کسانی که واقعاً «استاد» هستند (یعنی در جدول
-- professors با auth_user_id لینک شده‌اند) اجازه‌ی insert می‌دهد —
-- دانشجو یا هر کاربر anon دیگر نمی‌تواند مستقیم فایل آپلود کند.
-- ====================================================================

create policy "exam_content_professor_insert"
  on storage.objects for insert
  with check (
    bucket_id = 'exam-content'
    and public.current_professor_id() is not null
  );

-- (اختیاری ولی توصیه‌شده) اگر بعداً استاد بخواهد فایل قبلی را جایگزین
-- کند یا حذف کند (مثلاً اصلاح سوالات قبل از شروع آزمون)، این دو خط را
-- هم اضافه کن. فعلاً چون فایل‌ها content-hash/immutable هستند لازم
-- نیست، ولی برای کامل بودن گذاشته شده و کامنت است:

create policy "exam_content_professor_update"
  on storage.objects for update
  using (bucket_id = 'exam-content' and public.current_professor_id() is not null);

create policy "exam_content_professor_delete"
  on storage.objects for delete
  using (bucket_id = 'exam-content' and public.current_professor_id() is not null);