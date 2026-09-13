create table if not exists channels (
    channel_id uuid primary key,
    last_message_at timestamptz,
    last_message_preview text,
    created_at timestamptz not null default now()
);

create table if not exists channel_messages (
    message_id uuid primary key default gen_random_uuid(),
    channel_id uuid not null references channels(channel_id) on delete cascade,
    sender_role text not null check (sender_role in ('student', 'professor')),
    sender_id text not null,
    body text not null check (char_length(body) > 0 and char_length(body) <= 8000),
    reply_to_message_id uuid references channel_messages(message_id) on delete set null,
    sent_at timestamptz not null default now()
);

create index if not exists idx_channel_messages_channel_sent on channel_messages(channel_id, sent_at desc);

create table if not exists channel_reads (
    channel_id uuid not null references channels(channel_id) on delete cascade,
    member_role text not null check (member_role in ('student', 'professor')),
    member_id text not null,
    last_read_at timestamptz not null default now(),
    primary key (channel_id, member_role, member_id)
);
