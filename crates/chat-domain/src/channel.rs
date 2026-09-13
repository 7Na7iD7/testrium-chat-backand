use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::identity::Role;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelMessage {
    pub message_id: Uuid,
    pub channel_id: Uuid,
    pub sender_role: Role,
    pub sender_id: String,
    pub body: String,
    pub reply_to_message_id: Option<Uuid>,
    pub sent_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelSummary {
    pub channel_id: Uuid,
    pub last_message_at: Option<DateTime<Utc>>,
    pub last_message_preview: Option<String>,
    pub unread_count: i64,
}
