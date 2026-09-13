create table voice_recordings (
    id uuid primary key default gen_random_uuid(),
    room_id uuid not null references voice_rooms(room_id) on delete cascade,
    section_id uuid not null,
    egress_id text not null,
    recording_url text,
    started_at timestamptz not null default now(),
    ended_at timestamptz
);

create index voice_recordings_section_id_idx on voice_recordings (section_id, started_at desc);
create index voice_recordings_egress_id_idx on voice_recordings (egress_id);
