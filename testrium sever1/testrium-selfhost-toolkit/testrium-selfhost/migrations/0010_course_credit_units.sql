-- ====================================================================
-- Testrium — Migration 0010: کد درس اجباری + تعداد واحد هر درس
-- ====================================================================
-- تا الان course_code اختیاری بود و credit_units اصلاً وجود نداشت.
-- طبق نیاز جدید: هر درس باید کد درس داشته باشد (اجباری، نه فقط یکتا)
-- و تعداد واحد آن بین ۰ تا ۳ مشخص شود.
--
-- استراتژی: ستون credit_units با مقدار پیش‌فرض ۳ اضافه می‌شود تا
-- درس‌های قدیمی خراب نشوند؛ سپس یک CHECK constraint عدد را بین ۰ تا ۳
-- محدود می‌کند. اجباری کردن course_code را در سطح دیتابیس (NOT NULL)
-- انجام نمی‌دهیم چون درس‌های قدیمی ممکن است کد نداشته باشند و migration
-- را می‌شکند؛ اجبار «کد درس را وارد کن» فقط در UI/Edge Function
-- (admin-academic-ops) برای درس‌های جدید اعمال می‌شود.
-- ====================================================================

alter table public.course_catalog
  add column if not exists credit_units integer not null default 3;

alter table public.course_catalog
  drop constraint if exists course_catalog_credit_units_check;

alter table public.course_catalog
  add constraint course_catalog_credit_units_check
  check (credit_units >= 0 and credit_units <= 3);
