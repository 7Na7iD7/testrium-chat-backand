use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::identity::Role;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceRoomStatus {
    Open,
    Locked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceRoom {
    pub room_id: Uuid,
    pub section_id: Uuid,
    pub professor_id: String,
    pub status: VoiceRoomStatus,
    pub created_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub parent_room_id: Option<Uuid>,
    pub is_breakout: bool,
    pub breakout_label: Option<String>,
    pub recording_egress_id: Option<String>,
    pub recording_url: Option<String>,
    pub recording_started_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceParticipantStatus {
    RequestingToSpeak,
    Listener,
    Speaker,
    Muted,
    Removed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceSession {
    pub session_id: Uuid,
    pub room_id: Uuid,
    pub participant_role: Role,
    pub participant_id: String,
    pub joined_at: DateTime<Utc>,
    pub left_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceGrants {
    pub can_join: bool,
    pub can_publish: bool,
    pub can_publish_data: bool,
    pub can_subscribe: bool,
    pub is_room_admin: bool,
}

impl VoiceGrants {
    pub fn for_professor() -> Self {
        Self {
            can_join: true,
            can_publish: true,
            can_publish_data: true,
            can_subscribe: true,
            is_room_admin: true,
        }
    }

    pub fn for_student_listener() -> Self {
        Self {
            can_join: true,
            can_publish: false,
            can_publish_data: true,
            can_subscribe: true,
            is_room_admin: false,
        }
    }

    pub fn for_student_speaker() -> Self {
        Self {
            can_join: true,
            can_publish: true,
            can_publish_data: true,
            can_subscribe: true,
            is_room_admin: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceRaiseHandEntry {
    pub participant_role: Role,
    pub participant_id: String,
    pub requested_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceRoomMessage {
    pub message_id: Uuid,
    pub room_id: Uuid,
    pub sender_role: Role,
    pub sender_id: String,
    pub body: String,
    pub sent_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceAttendanceEntry {
    pub participant_role: Role,
    pub participant_id: String,
    pub present_seconds: i64,
    pub met_threshold: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceParticipationEntry {
    pub participant_role: Role,
    pub participant_id: String,
    pub present_seconds: i64,
    pub speaking_seconds: i64,
    pub raise_hand_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceRecordingEntry {
    pub recording_id: Uuid,
    pub room_id: Uuid,
    pub section_id: Uuid,
    pub recording_url: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}
