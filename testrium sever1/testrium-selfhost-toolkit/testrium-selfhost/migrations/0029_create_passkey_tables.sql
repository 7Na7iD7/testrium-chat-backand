-- ====================================================================
-- Testrium — Migration 0029: جدول‌های Passkey (ورود بدون کد)
-- ====================================================================
-- این متن دقیقاً از پروژه‌ی زنده (جدول supabase_migrations.schema_migrations،
-- ردیف با name = 'create_passkey_tables'، نسخه‌ی ۲۰۲۶۰۹۲۸۰۳۰۵۱۱)
-- بازیابی شده — چون فایل اصلی در پوشه‌ی migrations پروژه پیدا/ارسال
-- نشده بود.
--
-- این دو جدول پایه‌ی Edge Function جدید passkey-auth هستند:
--   user_passkeys      -> کلیدهای عمومی ثبت‌شده‌ی هر دانشجو/استاد
--                          (حداکثر ۵ تای فعال، طبق MAX_ACTIVE_PASSKEYS
--                          در کد passkey-auth)
--   passkey_challenges -> چالش‌های یک‌بارمصرف ثبت‌نام/ورود (TTL ۱۲۰
--                          ثانیه‌ای، طبق CHALLENGE_TTL_SECONDS)
--
-- هیچ policy ای برای anon/authenticated تعریف نمی‌شود — این جدول‌ها
-- فقط باید از داخل Edge Function (با service_role) دست بخورند.
-- ====================================================================

create table public.user_passkeys (
  id uuid primary key default gen_random_uuid(),
  role text not null check (role in ('student', 'professor')),
  user_code text not null,
  public_key text not null,
  device_label text,
  created_at timestamptz not null default now(),
  last_used_at timestamptz,
  revoked_at timestamptz
);

create index user_passkeys_owner_idx on public.user_passkeys (role, user_code) where revoked_at is null;

create table public.passkey_challenges (
  id uuid primary key default gen_random_uuid(),
  challenge text not null,
  purpose text not null check (purpose in ('register', 'login')),
  passkey_id uuid references public.user_passkeys(id) on delete cascade,
  auth_user_id uuid not null,
  created_at timestamptz not null default now(),
  expires_at timestamptz not null,
  used_at timestamptz
);

create index passkey_challenges_passkey_idx on public.passkey_challenges (passkey_id, created_at);
create index passkey_challenges_user_idx on public.passkey_challenges (auth_user_id, created_at);

alter table public.user_passkeys enable row level security;
alter table public.passkey_challenges enable row level security;

-- ====================================================================
-- بررسی صحت بعد از اجرا (اختیاری):
--   select relname, relrowsecurity from pg_class
--   where relname in ('user_passkeys', 'passkey_challenges');
--   -- هر دو باید relrowsecurity = t باشن
-- ====================================================================
