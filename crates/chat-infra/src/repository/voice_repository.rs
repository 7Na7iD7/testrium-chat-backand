use anyhow::Result;
use chat_domain::{
    Role, VoiceAttendanceEntry, VoiceParticipationEntry, VoiceRaiseHandEntry, VoiceRecordingEntry,
    VoiceRoom, VoiceRoomMessage, VoiceRoomStatus, VoiceSession,
};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct VoiceRepository {
    pool: PgPool,
}

impl VoiceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_or_create_room(
        &self,
        section_id: Uuid,
        professor_id: &str,
    ) -> Result<VoiceRoom> {
        let row: VoiceRoomRow = sqlx::query_as(
            r#"
            insert into voice_rooms (room_id, section_id, professor_id, status, created_at, is_breakout)
            values (gen_random_uuid(), $1, $2, 'open', now(), false)
            on conflict (section_id) where is_breakout = false
            do update set section_id = excluded.section_id
            returning room_id, section_id, professor_id, status, created_at, closed_at,
                      parent_room_id, is_breakout, breakout_label,
                      recording_egress_id, recording_url, recording_started_at
            "#,
        )
        .bind(section_id)
        .bind(professor_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.into())
    }

    pub async fn find_by_section(&self, section_id: Uuid) -> Result<Option<VoiceRoom>> {
        let row: Option<VoiceRoomRow> = sqlx::query_as(
            r#"
            select room_id, section_id, professor_id, status, created_at, closed_at,
                   parent_room_id, is_breakout, breakout_label,
                   recording_egress_id, recording_url, recording_started_at
            from voice_rooms where section_id = $1 and is_breakout = false
            "#,
        )
        .bind(section_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Into::into))
    }

    pub async fn find_by_id(&self, room_id: Uuid) -> Result<Option<VoiceRoom>> {
        let row: Option<VoiceRoomRow> = sqlx::query_as(
            r#"
            select room_id, section_id, professor_id, status, created_at, closed_at,
                   parent_room_id, is_breakout, breakout_label,
                   recording_egress_id, recording_url, recording_started_at
            from voice_rooms where room_id = $1
            "#,
        )
        .bind(room_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Into::into))
    }

    pub async fn set_status(&self, room_id: Uuid, status: VoiceRoomStatus) -> Result<()> {
        let status_str = match status {
            VoiceRoomStatus::Open => "open",
            VoiceRoomStatus::Locked => "locked",
        };
        sqlx::query("update voice_rooms set status = $2 where room_id = $1")
            .bind(room_id)
            .bind(status_str)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn start_session(
        &self,
        room_id: Uuid,
        participant_role: Role,
        participant_id: &str,
    ) -> Result<VoiceSession> {
        let row: VoiceSessionRow = sqlx::query_as(
            r#"
            insert into voice_sessions (session_id, room_id, participant_role, participant_id, joined_at)
            values (gen_random_uuid(), $1, $2, $3, now())
            returning session_id, room_id, participant_role, participant_id, joined_at, left_at
            "#,
        )
        .bind(room_id)
        .bind(participant_role.as_str())
        .bind(participant_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.into())
    }

    pub async fn end_open_sessions(&self, room_id: Uuid, participant_id: &str) -> Result<()> {
        sqlx::query(
            r#"
            update voice_sessions
            set left_at = now()
            where room_id = $1 and participant_id = $2 and left_at is null
            "#,
        )
        .bind(room_id)
        .bind(participant_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn create_breakout_rooms(
        &self,
        parent_room_id: Uuid,
        section_id: Uuid,
        professor_id: &str,
        labels: &[String],
    ) -> Result<Vec<VoiceRoom>> {
        let mut rooms = Vec::with_capacity(labels.len());
        for label in labels {
            let row: VoiceRoomRow = sqlx::query_as(
                r#"
                insert into voice_rooms (
                    room_id, section_id, professor_id, status, created_at,
                    parent_room_id, is_breakout, breakout_label
                )
                values (gen_random_uuid(), $1, $2, 'open', now(), $3, true, $4)
                returning room_id, section_id, professor_id, status, created_at, closed_at,
                          parent_room_id, is_breakout, breakout_label,
                          recording_egress_id, recording_url, recording_started_at
                "#,
            )
            .bind(section_id)
            .bind(professor_id)
            .bind(parent_room_id)
            .bind(label)
            .fetch_one(&self.pool)
            .await?;

            rooms.push(row.into());
        }
        Ok(rooms)
    }

    pub async fn list_breakout_rooms(&self, parent_room_id: Uuid) -> Result<Vec<VoiceRoom>> {
        let rows: Vec<VoiceRoomRow> = sqlx::query_as(
            r#"
            select room_id, section_id, professor_id, status, created_at, closed_at,
                   parent_room_id, is_breakout, breakout_label,
                   recording_egress_id, recording_url, recording_started_at
            from voice_rooms
            where parent_room_id = $1 and closed_at is null
            order by created_at asc
            "#,
        )
        .bind(parent_room_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn close_breakout_rooms(&self, parent_room_id: Uuid) -> Result<()> {
        sqlx::query(
            "update voice_rooms set closed_at = now() where parent_room_id = $1 and closed_at is null",
        )
        .bind(parent_room_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn add_breakout_members(&self, room_id: Uuid, student_codes: &[String]) -> Result<()> {
        for code in student_codes {
            sqlx::query(
                "insert into voice_breakout_members (room_id, student_code) values ($1, $2) on conflict do nothing",
            )
            .bind(room_id)
            .bind(code)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn list_breakout_members(&self, room_id: Uuid) -> Result<Vec<String>> {
        let rows: Vec<(String,)> =
            sqlx::query_as("select student_code from voice_breakout_members where room_id = $1")
                .bind(room_id)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().map(|(code,)| code).collect())
    }

    pub async fn enqueue_raise_hand(
        &self,
        room_id: Uuid,
        participant_role: Role,
        participant_id: &str,
    ) -> Result<bool> {
        let result = sqlx::query(
            r#"
            insert into voice_raise_hand_events (id, room_id, participant_role, participant_id, requested_at)
            select gen_random_uuid(), $1, $2, $3, now()
            where not exists (
                select 1 from voice_raise_hand_events
                where room_id = $1 and participant_id = $3 and resolved_at is null
            )
            "#,
        )
        .bind(room_id)
        .bind(participant_role.as_str())
        .bind(participant_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn resolve_raise_hand(
        &self,
        room_id: Uuid,
        participant_id: &str,
        action: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"
            update voice_raise_hand_events
            set resolved_at = now(), resolved_action = $3
            where room_id = $1 and participant_id = $2 and resolved_at is null
            "#,
        )
        .bind(room_id)
        .bind(participant_id)
        .bind(action)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_open_raise_hand_queue(&self, room_id: Uuid) -> Result<Vec<VoiceRaiseHandEntry>> {
        let rows: Vec<RaiseHandRow> = sqlx::query_as(
            r#"
            select participant_role, participant_id, requested_at
            from voice_raise_hand_events
            where room_id = $1 and resolved_at is null
            order by requested_at asc
            "#,
        )
        .bind(room_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn start_speaking_event(
        &self,
        room_id: Uuid,
        participant_role: Role,
        participant_id: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"
            insert into voice_speaking_events (id, room_id, participant_role, participant_id, started_at)
            select gen_random_uuid(), $1, $2, $3, now()
            where not exists (
                select 1 from voice_speaking_events
                where room_id = $1 and participant_id = $3 and ended_at is null
            )
            "#,
        )
        .bind(room_id)
        .bind(participant_role.as_str())
        .bind(participant_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn end_speaking_event(&self, room_id: Uuid, participant_id: &str) -> Result<()> {
        sqlx::query(
            r#"
            update voice_speaking_events
            set ended_at = now()
            where room_id = $1 and participant_id = $2 and ended_at is null
            "#,
        )
        .bind(room_id)
        .bind(participant_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn insert_room_message(
        &self,
        room_id: Uuid,
        sender_role: Role,
        sender_id: &str,
        body: &str,
    ) -> Result<VoiceRoomMessage> {
        let row: VoiceRoomMessageRow = sqlx::query_as(
            r#"
            insert into voice_room_messages (message_id, room_id, sender_role, sender_id, body, sent_at)
            values (gen_random_uuid(), $1, $2, $3, $4, now())
            returning message_id, room_id, sender_role, sender_id, body, sent_at
            "#,
        )
        .bind(room_id)
        .bind(sender_role.as_str())
        .bind(sender_id)
        .bind(body)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.into())
    }

    pub async fn list_room_messages(&self, room_id: Uuid, limit: i64) -> Result<Vec<VoiceRoomMessage>> {
        let rows: Vec<VoiceRoomMessageRow> = sqlx::query_as(
            r#"
            select message_id, room_id, sender_role, sender_id, body, sent_at
            from voice_room_messages
            where room_id = $1
            order by sent_at desc
            limit $2
            "#,
        )
        .bind(room_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let mut messages: Vec<VoiceRoomMessage> = rows.into_iter().map(Into::into).collect();
        messages.reverse();
        Ok(messages)
    }

    pub async fn set_recording_started(&self, room_id: Uuid, egress_id: &str) -> Result<()> {
        sqlx::query(
            "update voice_rooms set recording_egress_id = $2, recording_started_at = now(), recording_url = null where room_id = $1",
        )
        .bind(room_id)
        .bind(egress_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_recording_stopped(&self, room_id: Uuid, recording_url: Option<&str>) -> Result<()> {
        sqlx::query("update voice_rooms set recording_url = $2 where room_id = $1")
            .bind(room_id)
            .bind(recording_url)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn insert_recording(
        &self,
        room_id: Uuid,
        section_id: Uuid,
        egress_id: &str,
    ) -> Result<Uuid> {
        let row: (Uuid,) = sqlx::query_as(
            r#"
            insert into voice_recordings (id, room_id, section_id, egress_id, started_at)
            values (gen_random_uuid(), $1, $2, $3, now())
            returning id
            "#,
        )
        .bind(room_id)
        .bind(section_id)
        .bind(egress_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.0)
    }

    pub async fn finish_recording(&self, egress_id: &str, recording_url: Option<&str>) -> Result<()> {
        sqlx::query(
            r#"
            update voice_recordings
            set recording_url = $2, ended_at = now()
            where egress_id = $1 and ended_at is null
            "#,
        )
        .bind(egress_id)
        .bind(recording_url)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_section_recordings(&self, section_id: Uuid) -> Result<Vec<VoiceRecordingEntry>> {
        let rows: Vec<VoiceRecordingRow> = sqlx::query_as(
            r#"
            select id as recording_id, room_id, section_id, recording_url, started_at, ended_at
            from voice_recordings
            where section_id = $1 and recording_url is not null
            order by started_at desc
            "#,
        )
        .bind(section_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn attendance_report(
        &self,
        room_id: Uuid,
        threshold_seconds: i64,
    ) -> Result<Vec<VoiceAttendanceEntry>> {
        let rows: Vec<AttendanceRow> = sqlx::query_as(
            r#"
            select participant_role, participant_id,
                   coalesce(sum(extract(epoch from (coalesce(left_at, now()) - joined_at)))::bigint, 0) as present_seconds
            from voice_sessions
            where room_id = $1
            group by participant_role, participant_id
            order by present_seconds desc
            "#,
        )
        .bind(room_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| VoiceAttendanceEntry {
                participant_role: if r.participant_role == "professor" {
                    Role::Professor
                } else {
                    Role::Student
                },
                participant_id: r.participant_id,
                present_seconds: r.present_seconds,
                met_threshold: r.present_seconds >= threshold_seconds,
            })
            .collect())
    }

    pub async fn participation_report(&self, room_id: Uuid) -> Result<Vec<VoiceParticipationEntry>> {
        let rows: Vec<ParticipationRow> = sqlx::query_as(
            r#"
            with presence as (
                select participant_role, participant_id,
                       coalesce(sum(extract(epoch from (coalesce(left_at, now()) - joined_at)))::bigint, 0) as present_seconds
                from voice_sessions
                where room_id = $1
                group by participant_role, participant_id
            ),
            speaking as (
                select participant_id,
                       coalesce(sum(extract(epoch from (coalesce(ended_at, now()) - started_at)))::bigint, 0) as speaking_seconds
                from voice_speaking_events
                where room_id = $1
                group by participant_id
            ),
            hands as (
                select participant_id, count(*) as raise_hand_count
                from voice_raise_hand_events
                where room_id = $1
                group by participant_id
            )
            select p.participant_role, p.participant_id, p.present_seconds,
                   coalesce(s.speaking_seconds, 0) as speaking_seconds,
                   coalesce(h.raise_hand_count, 0) as raise_hand_count
            from presence p
            left join speaking s on s.participant_id = p.participant_id
            left join hands h on h.participant_id = p.participant_id
            order by p.present_seconds desc
            "#,
        )
        .bind(room_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }
}

#[derive(sqlx::FromRow)]
struct VoiceRoomRow {
    room_id: Uuid,
    section_id: Uuid,
    professor_id: String,
    status: String,
    created_at: chrono::DateTime<chrono::Utc>,
    closed_at: Option<chrono::DateTime<chrono::Utc>>,
    parent_room_id: Option<Uuid>,
    is_breakout: bool,
    breakout_label: Option<String>,
    recording_egress_id: Option<String>,
    recording_url: Option<String>,
    recording_started_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<VoiceRoomRow> for VoiceRoom {
    fn from(r: VoiceRoomRow) -> Self {
        VoiceRoom {
            room_id: r.room_id,
            section_id: r.section_id,
            professor_id: r.professor_id,
            status: if r.status == "locked" {
                VoiceRoomStatus::Locked
            } else {
                VoiceRoomStatus::Open
            },
            created_at: r.created_at,
            closed_at: r.closed_at,
            parent_room_id: r.parent_room_id,
            is_breakout: r.is_breakout,
            breakout_label: r.breakout_label,
            recording_egress_id: r.recording_egress_id,
            recording_url: r.recording_url,
            recording_started_at: r.recording_started_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct VoiceSessionRow {
    session_id: Uuid,
    room_id: Uuid,
    participant_role: String,
    participant_id: String,
    joined_at: chrono::DateTime<chrono::Utc>,
    left_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<VoiceSessionRow> for VoiceSession {
    fn from(r: VoiceSessionRow) -> Self {
        VoiceSession {
            session_id: r.session_id,
            room_id: r.room_id,
            participant_role: if r.participant_role == "professor" {
                Role::Professor
            } else {
                Role::Student
            },
            participant_id: r.participant_id,
            joined_at: r.joined_at,
            left_at: r.left_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct RaiseHandRow {
    participant_role: String,
    participant_id: String,
    requested_at: chrono::DateTime<chrono::Utc>,
}

impl From<RaiseHandRow> for VoiceRaiseHandEntry {
    fn from(r: RaiseHandRow) -> Self {
        VoiceRaiseHandEntry {
            participant_role: if r.participant_role == "professor" {
                Role::Professor
            } else {
                Role::Student
            },
            participant_id: r.participant_id,
            requested_at: r.requested_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct VoiceRoomMessageRow {
    message_id: Uuid,
    room_id: Uuid,
    sender_role: String,
    sender_id: String,
    body: String,
    sent_at: chrono::DateTime<chrono::Utc>,
}

impl From<VoiceRoomMessageRow> for VoiceRoomMessage {
    fn from(r: VoiceRoomMessageRow) -> Self {
        VoiceRoomMessage {
            message_id: r.message_id,
            room_id: r.room_id,
            sender_role: if r.sender_role == "professor" {
                Role::Professor
            } else {
                Role::Student
            },
            sender_id: r.sender_id,
            body: r.body,
            sent_at: r.sent_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct AttendanceRow {
    participant_role: String,
    participant_id: String,
    present_seconds: i64,
}

#[derive(sqlx::FromRow)]
struct ParticipationRow {
    participant_role: String,
    participant_id: String,
    present_seconds: i64,
    speaking_seconds: i64,
    raise_hand_count: i64,
}

impl From<ParticipationRow> for VoiceParticipationEntry {
    fn from(r: ParticipationRow) -> Self {
        VoiceParticipationEntry {
            participant_role: if r.participant_role == "professor" {
                Role::Professor
            } else {
                Role::Student
            },
            participant_id: r.participant_id,
            present_seconds: r.present_seconds,
            speaking_seconds: r.speaking_seconds,
            raise_hand_count: r.raise_hand_count,
        }
    }
}

#[derive(sqlx::FromRow)]
struct VoiceRecordingRow {
    recording_id: Uuid,
    room_id: Uuid,
    section_id: Uuid,
    recording_url: Option<String>,
    started_at: chrono::DateTime<chrono::Utc>,
    ended_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<VoiceRecordingRow> for VoiceRecordingEntry {
    fn from(r: VoiceRecordingRow) -> Self {
        VoiceRecordingEntry {
            recording_id: r.recording_id,
            room_id: r.room_id,
            section_id: r.section_id,
            recording_url: r.recording_url,
            started_at: r.started_at,
            ended_at: r.ended_at,
        }
    }
}
