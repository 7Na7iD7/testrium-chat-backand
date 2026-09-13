use axum::async_trait;
use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum_extra::headers::authorization::Bearer;
use axum_extra::headers::Authorization;
use axum_extra::TypedHeader;

use crate::state::AppState;

/// معادل REST همان چیزی که در ws/handler.rs برای WebSocket انجام
/// می‌شود: از هدر `Authorization: Bearer <token>` توکن را می‌خواند،
/// تایید می‌کند و هویت را resolve می‌کند.
///
/// نکته‌ی فنی: `FromRequestParts` در axum هنوز داخلاً با ماکروی
/// `#[async_trait]` (کتابخونه‌ی async-trait، نه async-fn خام Rust)
/// تعریف شده — یعنی امضای واقعی‌ای که کامپایلر انتظار داره یک تابع
/// معمولی با خروجی `Pin<Box<dyn Future<...>>>` است، نه یک `async fn`
/// خام. اگر impl را با `async fn` ساده بنویسیم (بدون همون ماکرو روی
/// خودِ impl)، امضای دو طرف از نظر lifetime یکی نمی‌شود — دقیقاً همون
/// خطای E0195 که دیدی. راه‌حل: همین `#[async_trait]` را روی impl هم
/// می‌گذاریم تا هر دو طرف یک شکل expand بشوند.
pub struct AuthenticatedUser(pub chat_domain::Identity);

#[async_trait]
impl<S> FromRequestParts<S> for AuthenticatedUser
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);

        let TypedHeader(Authorization(bearer)) =
            TypedHeader::<Authorization<Bearer>>::from_request_parts(parts, state)
                .await
                .map_err(|_| (StatusCode::UNAUTHORIZED, "هدر Authorization یافت نشد"))?;

        let claims = app_state
            .jwt_verifier
            .verify(bearer.token())
            .await
            .map_err(|_| (StatusCode::UNAUTHORIZED, "توکن نامعتبر است"))?;

        let identity = app_state
            .identity_resolver
            .resolve_for_claims(&claims)
            .await
            .map_err(|_| (StatusCode::FORBIDDEN, "هویت کاربر تایید نشد"))?;

        Ok(AuthenticatedUser(identity))
    }
}
