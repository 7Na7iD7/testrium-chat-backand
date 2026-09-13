use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::identity::Role;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageStatus {
    Sent,
    Delivered,
    Read,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub message_id: Uuid,
    pub thread_id: Uuid,
    pub sender_role: Role,
    pub sender_id: String,
    pub body: String,
    pub reply_to_message_id: Option<Uuid>,
    pub sent_at: DateTime<Utc>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub read_at: Option<DateTime<Utc>>,
}

impl Message {
    pub fn status(&self) -> MessageStatus {
        if self.read_at.is_some() {
            MessageStatus::Read
        } else if self.delivered_at.is_some() {
            MessageStatus::Delivered
        } else {
            MessageStatus::Sent
        }
    }
}
