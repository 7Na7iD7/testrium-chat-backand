use serde::{Deserialize, Serialize};

/// نقش کاربری که پیام‌رسانی می‌کند. دقیقاً هم‌ارز `UserRole` سمت
/// Flutter/Supabase (auth_repository.dart) — چون این سرور کاملاً
/// مستقل از Supabase است، این enum را دوباره این‌جا تعریف می‌کنیم و
/// در لایه‌ی auth با مقدار رشته‌ای برگشتی از Edge Function نگاشت
/// می‌دهیم.
///
/// `Hash` عمداً اضافه شده: `ConnectionManager` (chat-server/ws) از
/// `(Role, String)` به‌عنوان کلید یک `DashMap` استفاده می‌کند تا هر
/// کاربرِ آنلاین را به کانال WebSocket‌اش نگاشت بدهد؛ بدون `Hash` روی
/// `Role`، آن تاپل نمی‌تواند کلید یک HashMap/DashMap باشد.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Student,
    Professor,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Student => "student",
            Role::Professor => "professor",
        }
    }
}

/// هویت نهاییِ resolve‌شده‌ی یک اتصال، بعد از تایید JWT و واکشی
/// نقش/کدها از Supabase. `identifier` همان چیزی است که در جداول
/// `threads`/`messages` به‌عنوان `student_code` یا `professor_id`
/// ذخیره می‌شود.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub auth_user_id: String,
    pub role: Role,
    pub identifier: String,
    pub display_name: String,
    /// section_id هایی که این کاربر (چه به‌عنوان استاد صاحب سکشن، چه
    /// دانشجوی enrolled) به آن‌ها دسترسی دارد — برای اعتبارسنجی این‌که
    /// یک استاد فقط با دانشجوهای section خودش می‌تواند ترد بسازد.
    pub accessible_section_ids: Vec<String>,
}
