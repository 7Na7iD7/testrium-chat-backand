create table if not exists voice_rooms (
    room_id uuid primary key default gen_random_uuid(),
    section_id uuid not null unique,
    professor_id text not null,
    status text not null default 'open' check (status in ('open', 'locked')),
    created_at timestamptz not null default now(),
    closed_at timestamptz
);

create index if not exists idx_voice_rooms_professor on voice_rooms(professor_id);

create table if not exists voice_sessions (
    session_id uuid primary key default gen_random_uuid(),
    room_id uuid not null references voice_rooms(room_id) on delete cascade,
    participant_role text not null check (participant_role in ('student', 'professor')),
    participant_id text not null,
    joined_at timestamptz not null default now(),
    left_at timestamptz
);

create index if not exists idx_voice_sessions_room on voice_sessions(room_id, joined_at desc);
create index if not exists idx_voice_sessions_open on voice_sessions(room_id, participant_id) where left_at is null;
