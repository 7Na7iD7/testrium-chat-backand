-- ====================================================================
-- Testrium Chat — Migration 0001: Init
-- این دیتابیس کاملاً مستقل از Supabase است؛ هیچ کلاینتی مستقیم به آن
-- وصل نمی‌شود، همه‌چیز از پشت سرور Rust (chat-server) رد می‌شود. به
-- همین دلیل RLS در این‌جا معنا ندارد — کنترل دسترسی در application
-- layer (auth::IdentityResolver + بررسی section) انجام می‌شود.
-- ====================================================================

create extension if not exists pgcrypto;

create table if not exists threads (
    thread_id               uuid primary key default gen_random_uuid(),
    professor_id            text not null,
    student_code            text not null,
    section_id              uuid,
    created_at              timestamptz not null default now(),
    last_message_at         timestamptz,
    last_message_preview    text,
    unread_count_for_student    bigint not null default 0,
    unread_count_for_professor  bigint not null default 0,

    constraint uq_thread_professor_student unique (professor_id, student_code)
);

create index if not exists idx_threads_professor on threads(professor_id, last_message_at desc);
create index if not exists idx_threads_student on threads(student_code, last_message_at desc);

create table if not exists messages (
    message_id      uuid primary key default gen_random_uuid(),
    thread_id       uuid not null references threads(thread_id) on delete cascade,
    sender_role     text not null check (sender_role in ('student', 'professor')),
    sender_id       text not null,
    body            text not null check (char_length(body) > 0 and char_length(body) <= 8000),
    sent_at         timestamptz not null default now(),
    delivered_at    timestamptz,
    read_at         timestamptz
);

create index if not exists idx_messages_thread_sent_at on messages(thread_id, sent_at desc);

-- دستگاه‌های کاربر برای مسیریابی push notification زمانی که گیرنده
-- WebSocket باز ندارد. این جدول به‌عمد جدا از device_tokens سمت
-- Supabase نگه‌داشته می‌شود چون این سرور نباید مستقیم به دیتابیس
-- Supabase وصل شود؛ sync سبک بین این دو در لایه‌ی application (هنگام
-- resolve هویت) انجام می‌شود — نگاه کن به یادداشت در auth/identity_resolver.rs.
create table if not exists push_tokens (
    id              uuid primary key default gen_random_uuid(),
    owner_role      text not null check (owner_role in ('student', 'professor')),
    owner_id        text not null,
    fcm_token       text not null,
    updated_at      timestamptz not null default now(),

    constraint uq_push_token unique (owner_role, owner_id, fcm_token)
);

create index if not exists idx_push_tokens_owner on push_tokens(owner_role, owner_id);
