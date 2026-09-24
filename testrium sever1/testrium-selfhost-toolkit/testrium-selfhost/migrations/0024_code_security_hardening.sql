-- ====================================================================
-- 0024: سخت‌سازی امنیتی verify-code
-- سه هدف: (۱) rate limiting روی تلاش‌های verify، (۲) جلوگیری از
-- relink خودکار و بی‌قیدوشرط (ریشه‌ی خطر ربوده‌شدن اکانت)،
-- (۳) امکان دادن یک پنجره‌ی زمانی محدود توسط ادمین برای relink
-- مشروع (مثلاً گوشی جدید دانشجو).
-- ====================================================================

-- جدول ثبت هر تلاش verify (موفق یا ناموفق) — پایه‌ی rate limiting.
create table if not exists verify_code_attempts (
    id              uuid primary key default gen_random_uuid(),
    auth_user_id    text not null,
    code_attempted  text not null,
    succeeded       boolean not null,
    attempted_at    timestamptz not null default now()
);

create index if not exists idx_verify_attempts_user_time
    on verify_code_attempts(auth_user_id, attempted_at desc);

create index if not exists idx_verify_attempts_code_time
    on verify_code_attempts(code_attempted, attempted_at desc);

-- پاک‌سازی خودکار رکوردهای قدیمی‌تر از ۲۴ ساعت (جلوگیری از رشد
-- بی‌رویه‌ی جدول). اجرا با pg_cron اگر در دسترس است؛ در غیر این صورت
-- می‌توان این کوئری را با یک cron خارجی هم اجرا کرد:
--   delete from verify_code_attempts where attempted_at < now() - interval '24 hours';
do $$
begin
    if exists (select 1 from pg_extension where extname = 'pg_cron') then
        perform cron.schedule(
            'cleanup_verify_code_attempts',
            '0 * * * *',
            $cron$delete from verify_code_attempts where attempted_at < now() - interval '24 hours'$cron$
        );
    end if;
end $$;

-- پنجره‌ی زمانی relink مشروع. فقط ادمین این مقدار را ست می‌کند (از
-- طریق admin-academic-ops، اکشن جدید allow_relink). تا وقتی این
-- مقدار null یا گذشته است، relink خودکار مسدود می‌ماند.
alter table students
    add column if not exists relink_allowed_until timestamptz;

alter table professors
    add column if not exists relink_allowed_until timestamptz;

create index if not exists idx_students_relink_window
    on students(student_code) where relink_allowed_until is not null;

create index if not exists idx_professors_relink_window
    on professors(professor_code) where relink_allowed_until is not null;
