-- ====================================================================
-- Testrium — Migration 0026: سیستم هویت per-admin
-- ====================================================================
-- جایگزین رمز مشترک x-admin-secret با یک رمز جداگانه به‌ازای هر
-- ادمین. هیچ ایمیل/SMTP لازم نیست — هر ادمین یک کلید تصادفی
-- پرانتروپی می‌گیرد (نمایش‌داده‌شده فقط یک‌بار، لحظه‌ی ساخت) که
-- هش SHA-256اش در دیتابیس ذخیره می‌شود، نه خودِ کلید.
--
-- admin_audit_log از این پس admin_id را هم ثبت می‌کند تا گزارش
-- بگوید "کدام ادمین" یک عملیات را انجام داده، نه فقط IP/زمان.
-- ====================================================================

create extension if not exists pgcrypto;

create table if not exists admins (
    id              uuid primary key default gen_random_uuid(),
    full_name       text not null,
    secret_hash     text not null unique,
    is_active       boolean not null default true,
    created_at      timestamptz not null default now(),
    last_used_at    timestamptz
);

alter table admins enable row level security;

alter table admin_audit_log
    add column if not exists admin_id uuid references admins(id);

create index if not exists idx_admin_audit_log_admin_id on admin_audit_log(admin_id, created_at desc);

-- ====================================================================
-- بوت‌استرپ اولین ادمین (فقط یک‌بار، دستی اجرا کن — این بخش عمداً در
-- بدنه‌ی migration نیست چون هر environment باید کلید خودش را بسازد،
-- نه یک مقدار ثابت و مشترک بین همه‌ی نصب‌ها):
--
--   with gen as (
--     select encode(gen_random_bytes(32), 'hex') as plain_secret
--   ),
--   ins as (
--     insert into admins (full_name, secret_hash, is_active)
--     select 'ادمین اصلی', encode(digest(plain_secret, 'sha256'), 'hex'), true
--     from gen
--     returning id
--   )
--   select gen.plain_secret, ins.id as admin_id
--   from gen, ins;
--
-- خروجی plain_secret را فوراً یک‌جای امن کپی کن — دیگر هیچ‌وقت به‌صورت
-- خوانا نمایش داده نمی‌شود.
-- ====================================================================
