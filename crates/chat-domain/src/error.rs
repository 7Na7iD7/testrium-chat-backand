use thiserror::Error;

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("پیام نمی‌تواند خالی باشد")]
    EmptyMessageBody,

    #[error("طول پیام از حد مجاز ({max} کاراکتر) بیشتر است")]
    MessageTooLong { max: usize },

    #[error("این کاربر اجازه‌ی دسترسی به این ترد را ندارد")]
    ThreadAccessDenied,

    #[error("استاد فقط می‌تواند با دانشجویانِ section خودش گفتگو را آغاز کند")]
    StudentNotInProfessorSection,
}
