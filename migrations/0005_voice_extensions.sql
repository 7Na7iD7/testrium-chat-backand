alter table voice_rooms drop constraint if exists voice_rooms_section_id_key;

alter table voice_rooms
    add column if not exists parent_room_id uuid references voice_rooms(room_id) on delete cascade,
    add column if not exists is_breakout boolean not null default false,
    add column if not exists breakout_label text,
    add column if not exists recording_egress_id text,
    add column if not exists recording_url text,
    add column if not exists recording_started_at timestamptz;

create unique index if not exists uq_voice_rooms_section_main on voice_rooms(section_id) where is_breakout = false;
create index if not exists idx_voice_rooms_parent on voice_rooms(parent_room_id);

create table if not exists voice_breakout_members (
    room_id uuid not null references voice_rooms(room_id) on delete cascade,
    student_code text not null,
    added_at timestamptz not null default now(),

    primary key (room_id, student_code)
);

create table if not exists voice_raise_hand_events (
    id uuid primary key default gen_random_uuid(),
    room_id uuid not null references voice_rooms(room_id) on delete cascade,
    participant_role text not null check (participant_role in ('student', 'professor')),
    participant_id text not null,
    requested_at timestamptz not null default now(),
    resolved_at timestamptz,
    resolved_action text check (resolved_action in ('allowed', 'denied', 'cancelled', 'left'))
);

create index if not exists idx_raise_hand_room_open on voice_raise_hand_events(room_id, requested_at) where resolved_at is null;
create index if not exists idx_raise_hand_room_participant_open on voice_raise_hand_events(room_id, participant_id) where resolved_at is null;

create table if not exists voice_speaking_events (
    id uuid primary key default gen_random_uuid(),
    room_id uuid not null references voice_rooms(room_id) on delete cascade,
    participant_role text not null check (participant_role in ('student', 'professor')),
    participant_id text not null,
    started_at timestamptz not null default now(),
    ended_at timestamptz
);

create index if not exists idx_speaking_room on voice_speaking_events(room_id, participant_id);
create index if not exists idx_speaking_room_open on voice_speaking_events(room_id, participant_id) where ended_at is null;

create table if not exists voice_room_messages (
    message_id uuid primary key default gen_random_uuid(),
    room_id uuid not null references voice_rooms(room_id) on delete cascade,
    sender_role text not null check (sender_role in ('student', 'professor')),
    sender_id text not null,
    body text not null check (char_length(body) > 0 and char_length(body) <= 2000),
    sent_at timestamptz not null default now()
);

create index if not exists idx_voice_room_messages_room_sent on voice_room_messages(room_id, sent_at desc);
