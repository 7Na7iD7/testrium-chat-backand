-- ====================================================================
-- Testrium — Migration 0021: نامعتبرسازی خودکار claims هنگام تغییر دسترسی
-- ====================================================================
-- verify-code از این پس role/user_code/accessible_section_ids را در
-- app_metadata کاربر می‌نویسد (این JWT claim را در نشست بعدی/رفرش‌شده
-- در دسترس سرور Rust قرار می‌دهد، بدون نیاز به صدا زدن chat-identity).
-- اما اگر بعد از verify اولیه، استاد یک دانشجوی جدید را enroll کند یا
-- یک section جدید به استاد بدهد، آن تغییر تا verify بعدی در claims
-- منعکس نمی‌شود. این migration یک تریگر روی enrollments و sections
-- اضافه می‌کند که بلافاصله Edge Function sync-user-claims را با pg_net
-- صدا می‌زند تا claims همان کاربر تازه شود.
--
-- دو تنظیم زیر باید یک‌بار در هر environment (dev/prod) ست شوند —
-- این migration فقط تابع/تریگر را می‌سازد، مقداردهی این دو GUC را نه،
-- چون مقدارشان (آدرس Edge Function و رمز مشترک) به‌ازای هر پروژه فرق
-- می‌کند و نباید در یک فایل migration که ممکن است در گیت commit شود
-- hardcode شوند:
--
--   alter database postgres set app.settings.sync_claims_url =
--     'https://<PROJECT-REF>.supabase.co/functions/v1/sync-user-claims';
--   alter database postgres set app.settings.sync_claims_secret =
--     '<همان مقداری که در env سرور به SYNC_CLAIMS_SHARED_SECRET دادی>';
--
-- بعد از اجرای این دو دستور، یک بار session را reconnect کن (یا
-- سرویس postgres را reload کن) تا current_setting در تریگر مقدار
-- تازه را ببیند.
-- ====================================================================

create extension if not exists pg_net with schema extensions;

create or replace function public.notify_section_access_changed()
returns trigger
language plpgsql
security definer
set search_path = public
as $$
declare
  v_url text;
  v_secret text;
  v_row jsonb;
begin
  v_url := current_setting('app.settings.sync_claims_url', true);
  v_secret := current_setting('app.settings.sync_claims_secret', true);

  if v_url is null or v_secret is null then
    -- تنظیمات هنوز ست نشده (مثلاً روی dev محلی) — بی‌صدا رد می‌شویم،
    -- نه raise exception، چون این نباید insert/update اصلی را بشکند.
    return coalesce(new, old);
  end if;

  if tg_op = 'DELETE' then
    v_row := to_jsonb(old);
  else
    v_row := to_jsonb(new);
  end if;

  perform net.http_post(
    url := v_url,
    headers := jsonb_build_object(
      'Content-Type', 'application/json',
      'Authorization', 'Bearer ' || v_secret
    ),
    body := jsonb_build_object(
      'type', tg_op,
      'table', tg_table_name,
      'record', v_row,
      'old_record', case when tg_op = 'DELETE' then v_row else null end
    )
  );

  return coalesce(new, old);
end;
$$;

drop trigger if exists trg_enrollments_notify_claims on public.enrollments;
create trigger trg_enrollments_notify_claims
  after insert or update or delete on public.enrollments
  for each row execute function public.notify_section_access_changed();

drop trigger if exists trg_sections_notify_claims on public.sections;
create trigger trg_sections_notify_claims
  after insert or update or delete on public.sections
  for each row execute function public.notify_section_access_changed();

-- ====================================================================
-- بررسی صحت بعد از اجرا (اختیاری):
--   select tgname from pg_trigger
--   where tgname in ('trg_enrollments_notify_claims', 'trg_sections_notify_claims');
--   -- باید هر دو ردیف برگردند
--
--   -- برای تست واقعی (بعد از ست‌کردن دو GUC بالا)، یک enrollment جدید
--   -- درج کن و لاگ‌های sync-user-claims را در دشبورد Edge Functions
--   -- چک کن — باید رویداد student_claims_synced دیده شود.
-- ====================================================================
