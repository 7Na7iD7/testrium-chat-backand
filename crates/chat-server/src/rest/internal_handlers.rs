use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use tracing::warn;

use crate::state::AppState;
use crate::ws::protocol::ServerEvent;

/// این ماژول فقط برای فراخوانی از سمت Supabase (نه از سمت اپ فلاتر)
/// طراحی شده — دقیقاً برعکس جهت `chat-identity`/`chat-verify-link`
/// (که آنجا سرور Rust به Supabase درخواست می‌زد). برای همین به‌جای
/// `AuthenticatedUser` (که JWT کاربر عادی را می‌خواهد)، یک رمز مشترک
/// جداگانه (`CHAT_SERVER_NOTIFY_SECRET`) در متغیرهای محیطی بررسی
/// می‌شود.
#[derive(Debug, Deserialize)]
struct NotifyClaimsUpdatedRequest {
    role: String,
    identifier: String,
}

fn timing_safe_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    let max_len = a.len().max(b.len());
    let mut diff: u8 = if a.len() == b.len() { 0 } else { 1 };
    for i in 0..max_len {
        diff |= a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0);
    }
    diff == 0
}

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/internal/notify-claims-updated",
        post(notify_claims_updated),
    )
}

async fn notify_claims_updated(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<NotifyClaimsUpdatedRequest>,
) -> impl IntoResponse {
    let expected_secret = match std::env::var("CHAT_SERVER_NOTIFY_SECRET") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            warn!("CHAT_SERVER_NOTIFY_SECRET تنظیم نشده؛ notify-claims-updated رد شد");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let provided_secret = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

    if !timing_safe_eq(provided_secret, &expected_secret) {
        warn!("notify_claims_updated: unauthorized caller");
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let role = match body.role.as_str() {
        "student" => chat_domain::Role::Student,
        "professor" => chat_domain::Role::Professor,
        other => {
            warn!(role = other, "notify_claims_updated: invalid role");
            return StatusCode::BAD_REQUEST.into_response();
        }
    };

    let key = (role, body.identifier.clone());
    let delivered = state.connections.push(&key, ServerEvent::ClaimsUpdated);

    Json(serde_json::json!({ "ok": true, "delivered": delivered })).into_response()
}
