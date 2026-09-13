use chat_domain::{ChannelMessage, Message, MessageStatus, VoiceRaiseHandEntry, VoiceRoomMessage};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientEvent {
    SendMessage {
        peer_id: String,
        body: String,
        client_message_id: Uuid,
        #[serde(default)]
        reply_to_message_id: Option<Uuid>,
    },
    MarkThreadRead {
        thread_id: Uuid,
    },
    SendChannelMessage {
        channel_id: Uuid,
        body: String,
        client_message_id: Uuid,
        #[serde(default)]
        reply_to_message_id: Option<Uuid>,
    },
    MarkChannelRead {
        channel_id: Uuid,
    },
    VoiceRequestToSpeak {
        room_id: Uuid,
    },
    VoiceCancelRaiseHand {
        room_id: Uuid,
    },
    VoiceSpeakingStateChanged {
        room_id: Uuid,
        speaking: bool,
    },
    VoiceScreenShareStateChanged {
        room_id: Uuid,
        sharing: bool,
    },
    SendVoiceRoomMessage {
        room_id: Uuid,
        body: String,
        client_message_id: Uuid,
    },
    VoiceResyncRoomState {
        room_id: Uuid,
    },
    Ping,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    NewMessage {
        thread_id: Uuid,
        message: MessageDto,
    },
    MessageAck {
        client_message_id: Uuid,
        message: MessageDto,
    },
    MessagesDelivered {
        thread_id: Uuid,
        message_ids: Vec<Uuid>,
    },
    ThreadRead {
        thread_id: Uuid,
        reader_role: chat_domain::Role,
    },
    NewChannelMessage {
        channel_id: Uuid,
        message: ChannelMessageDto,
    },
    ChannelMessageAck {
        client_message_id: Uuid,
        message: ChannelMessageDto,
    },
    VoiceSpeakRequested {
        room_id: Uuid,
        student_id: String,
    },
    VoicePermissionChanged {
        room_id: Uuid,
        participant_id: String,
        can_publish: bool,
    },
    VoiceParticipantRemoved {
        room_id: Uuid,
        participant_id: String,
    },
    VoiceRoomLocked {
        room_id: Uuid,
        locked: bool,
    },
    VoiceRaiseHandQueueUpdated {
        room_id: Uuid,
        queue: Vec<VoiceRaiseHandEntryDto>,
    },
    VoiceScreenShareStateChanged {
        room_id: Uuid,
        participant_id: String,
        sharing: bool,
    },
    NewVoiceRoomMessage {
        room_id: Uuid,
        message: VoiceRoomMessageDto,
    },
    VoiceRoomMessageAck {
        client_message_id: Uuid,
        message: VoiceRoomMessageDto,
    },
    VoiceRecordingStarted {
        room_id: Uuid,
    },
    VoiceRecordingStopped {
        room_id: Uuid,
        recording_url: Option<String>,
    },
    VoiceBreakoutAssigned {
        parent_room_id: Uuid,
        room_id: Uuid,
        room_name: String,
        group_label: String,
        livekit_url: String,
        token: String,
    },
    VoiceBreakoutEnded {
        parent_room_id: Uuid,
    },
    VoiceRoomStateSnapshot {
        room_id: Uuid,
        locked: bool,
        recording_active: bool,
        recording_url: Option<String>,
        raise_hand_queue: Vec<VoiceRaiseHandEntryDto>,
    },
    /// وقتی دسترسی کاربر (نقش/کد/لیست section ها) در Supabase تغییر
    /// می‌کند (مثلاً استاد او را در یک section جدید ثبت‌نام می‌کند)،
    /// `sync-user-claims` بعد از آپدیت `app_metadata`، از طریق مسیر
    /// داخلی `/internal/notify-claims-updated` به این سرور خبر می‌دهد؛
    /// اگر همان لحظه کاربر آنلاین باشد، این رویداد برایش فرستاده
    /// می‌شود تا کلاینت بدون معطلی تا چرخه‌ی خودکار بعدی،
    /// `refreshSession` را صدا بزند و JWT تازه با claim های جدید بگیرد.
    ClaimsUpdated,
    Error {
        message: String,
    },
    Pong,
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageDto {
    pub message_id: Uuid,
    pub thread_id: Uuid,
    pub sender_role: chat_domain::Role,
    pub sender_id: String,
    pub body: String,
    pub reply_to_message_id: Option<Uuid>,
    pub sent_at: chrono::DateTime<chrono::Utc>,
    pub status: MessageStatus,
}

impl From<Message> for MessageDto {
    fn from(m: Message) -> Self {
        let status = m.status();
        MessageDto {
            message_id: m.message_id,
            thread_id: m.thread_id,
            sender_role: m.sender_role,
            sender_id: m.sender_id,
            body: m.body,
            reply_to_message_id: m.reply_to_message_id,
            sent_at: m.sent_at,
            status,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ChannelMessageDto {
    pub message_id: Uuid,
    pub channel_id: Uuid,
    pub sender_role: chat_domain::Role,
    pub sender_id: String,
    pub body: String,
    pub reply_to_message_id: Option<Uuid>,
    pub sent_at: chrono::DateTime<chrono::Utc>,
}

impl From<ChannelMessage> for ChannelMessageDto {
    fn from(m: ChannelMessage) -> Self {
        ChannelMessageDto {
            message_id: m.message_id,
            channel_id: m.channel_id,
            sender_role: m.sender_role,
            sender_id: m.sender_id,
            body: m.body,
            reply_to_message_id: m.reply_to_message_id,
            sent_at: m.sent_at,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct VoiceRaiseHandEntryDto {
    pub participant_role: chat_domain::Role,
    pub participant_id: String,
    pub requested_at: chrono::DateTime<chrono::Utc>,
}

impl From<VoiceRaiseHandEntry> for VoiceRaiseHandEntryDto {
    fn from(e: VoiceRaiseHandEntry) -> Self {
        VoiceRaiseHandEntryDto {
            participant_role: e.participant_role,
            participant_id: e.participant_id,
            requested_at: e.requested_at,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct VoiceRoomMessageDto {
    pub message_id: Uuid,
    pub room_id: Uuid,
    pub sender_role: chat_domain::Role,
    pub sender_id: String,
    pub body: String,
    pub sent_at: chrono::DateTime<chrono::Utc>,
}

impl From<VoiceRoomMessage> for VoiceRoomMessageDto {
    fn from(m: VoiceRoomMessage) -> Self {
        VoiceRoomMessageDto {
            message_id: m.message_id,
            room_id: m.room_id,
            sender_role: m.sender_role,
            sender_id: m.sender_id,
            body: m.body,
            sent_at: m.sent_at,
        }
    }
}
