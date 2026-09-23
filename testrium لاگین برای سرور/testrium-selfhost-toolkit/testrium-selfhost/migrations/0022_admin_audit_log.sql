create table if not exists public.admin_audit_log (
    id uuid primary key default gen_random_uuid(),
    action text not null,
    payload_summary jsonb,
    result text not null check (result in ('success', 'failure', 'unauthorized', 'blocked')),
    error_message text,
    ip_address text,
    created_at timestamptz not null default now()
);

create index if not exists idx_admin_audit_log_created_at on public.admin_audit_log(created_at desc);
create index if not exists idx_admin_audit_log_unauthorized_ip on public.admin_audit_log(ip_address, created_at desc) where result in ('unauthorized', 'blocked');

alter table public.admin_audit_log enable row level security;
-- هیچ policy ای برای anon/authenticated تعریف نمی‌شود؛ فقط service_role
-- (از داخل خودِ Edge Function) می‌تواند بنویسد یا بخواند.
