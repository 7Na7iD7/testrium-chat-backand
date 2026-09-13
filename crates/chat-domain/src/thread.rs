use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// یک گفتگوی مستقیم بین یک استاد و یک دانشجوی مشخص. هر جفت
/// (professor_id, student_code) دقیقاً یک thread دارد (constraint
/// یکتایی در migration دیتابیس چت اعمال می‌شود)، صرف‌نظر از این‌که
/// دانشجو در چند section از همان استاد enrolled باشد.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    pub thread_id: Uuid,
    pub professor_id: String,
    pub student_code: String,
    pub section_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub last_message_at: Option<DateTime<Utc>>,
    pub last_message_preview: Option<String>,
    pub unread_count_for_student: i64,
    pub unread_count_for_professor: i64,
}
