use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tracing::{error, warn};
use uuid::Uuid;

use crate::state::AppState;
use crate::ws::protocol::ServerEvent;

#[derive(Debug, Deserialize)]
struct NotifyClaimsUpdatedRequest {
    role: String,
    identifier: String,
}

#[derive(Debug, Deserialize)]
struct PurgeSectionDataRequest {
    section_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize)]
struct PurgeSectionDataResponse {
    ok: bool,
    sections_processed: usize,
    recordings_deleted: usize,
    messages_deleted: i64,
    storage_errors: Vec<String>,
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

fn verify_internal_secret(headers: &HeaderMap) -> Result<(), StatusCode> {
    let expected_secret = match std::env::var("CHAT_SERVER_NOTIFY_SECRET") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            warn!("CHAT_SERVER_NOTIFY_SECRET تنظیم نشده؛ درخواست داخلی رد شد");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let provided_secret = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

    if !timing_safe_eq(provided_secret, &expected_secret) {
        warn!("internal endpoint: unauthorized caller");
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(())
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/internal/notify-claims-updated",
            post(notify_claims_updated),
        )
        .route("/internal/purge-section-data", post(purge_section_data))
}

async fn notify_claims_updated(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<NotifyClaimsUpdatedRequest>,
) -> impl IntoResponse {
    if let Err(code) = verify_internal_secret(&headers) {
        return code.into_response();
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

async fn purge_section_data(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<PurgeSectionDataRequest>,
) -> impl IntoResponse {
    if let Err(code) = verify_internal_secret(&headers) {
        return code.into_response();
    }

    let mut recordings_deleted = 0usize;
    let mut messages_deleted: i64 = 0;
    let mut storage_errors = Vec::new();

    for section_id in &body.section_ids {
        let recordings = match state.voice_rooms.purge_section_recordings(*section_id).await {
            Ok(r) => r,
            Err(e) => {
                error!(error = %e, section_id = %section_id, "voice_recordings_purge_failed");
                storage_errors.push(format!("section {section_id}: {e}"));
                continue;
            }
        };

        for recording in &recordings {
            if let Some(url) = &recording.recording_url {
                let output = state.config.egress_s3_output();
                let key = output.object_key_from_location(url);
                if let Err(e) = output.delete_object(&key).await {
                    error!(
                        error = %e,
                        recording_id = %recording.recording_id,
                        "voice_recording_object_delete_failed"
                    );
                    storage_errors.push(format!("recording {}: {e}", recording.recording_id));
                }
            }
        }
        recordings_deleted += recordings.len();

        match state.voice_rooms.purge_section_room_messages(*section_id).await {
            Ok(count) => messages_deleted += count,
            Err(e) => {
                error!(error = %e, section_id = %section_id, "voice_room_messages_purge_failed");
                storage_errors.push(format!("voice messages {section_id}: {e}"));
            }
        }

        match state.channels.purge_channel(*section_id).await {
            Ok(count) => messages_deleted += count,
            Err(e) => {
                error!(error = %e, section_id = %section_id, "channel_purge_failed");
                storage_errors.push(format!("channel {section_id}: {e}"));
            }
        }
    }

    Json(PurgeSectionDataResponse {
        ok: true,
        sections_processed: body.section_ids.len(),
        recordings_deleted,
        messages_deleted,
        storage_errors,
    })
    .into_response()
}
