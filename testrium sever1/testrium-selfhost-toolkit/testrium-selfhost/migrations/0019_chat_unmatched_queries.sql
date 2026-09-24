create table if not exists public.chat_unmatched_queries (
  id          uuid primary key default gen_random_uuid(),
  role        text not null check (role in ('student', 'professor')),
  raw_text    text not null,
  created_at  timestamptz not null default now()
);

create index if not exists idx_chat_unmatched_created_at
  on public.chat_unmatched_queries(created_at desc);

alter table public.chat_unmatched_queries enable row level security;

create policy "chat_unmatched_insert_authenticated"
  on public.chat_unmatched_queries for insert
  with check (
    public.current_student_code() is not null
    or public.current_professor_id() is not null
  );
