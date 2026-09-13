use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use chat_domain::{Role, Thread};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::rest::extractor::AuthenticatedUser;
use crate::state::AppState;
use crate::ws::protocol::{ChannelMessageDto, MessageDto};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/threads", get(list_threads))
        .route("/threads/:thread_id/messages", get(list_messages))
        .route("/channels", get(list_channels))
        .route("/channels/:channel_id/messages", get(list_channel_messages))
}

async fn health() -> &'static str {
    "ok"
}

#[derive(Debug, Serialize)]
struct ThreadDto {
    thread_id: Uuid,
    peer_id: String,
    peer_role: Role,
    last_message_at: Option<chrono::DateTime<chrono::Utc>>,
    last_message_preview: Option<String>,
    unread_count: i64,
}

impl ThreadDto {
    fn from_thread(t: Thread, viewer_role: Role) -> Self {
        match viewer_role {
            Role::Student => ThreadDto {
                thread_id: t.thread_id,
                peer_id: t.professor_id,
                peer_role: Role::Professor,
                last_message_at: t.last_message_at,
                last_message_preview: t.last_message_preview,
                unread_count: t.unread_count_for_student,
            },
            Role::Professor => ThreadDto {
                thread_id: t.thread_id,
                peer_id: t.student_code,
                peer_role: Role::Student,
                last_message_at: t.last_message_at,
                last_message_preview: t.last_message_preview,
                unread_count: t.unread_count_for_professor,
            },
        }
    }
}

async fn list_threads(
    State(state): State<AppState>,
    AuthenticatedUser(identity): AuthenticatedUser,
) -> impl IntoResponse {
    let result = match identity.role {
        Role::Student => state.threads.list_for_student(&identity.identifier).await,
        Role::Professor => {
            state
                .threads
                .list_for_professor(&identity.identifier)
                .await
        }
    };

    match result {
        Ok(threads) => {
            let dtos: Vec<ThreadDto> = threads
                .into_iter()
                .map(|t| ThreadDto::from_thread(t, identity.role))
                .collect();
            Json(dtos).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "list_threads_failed");
            axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
struct MessagesQuery {
    before: Option<Uuid>,
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    50
}

async fn list_messages(
    State(state): State<AppState>,
    AuthenticatedUser(identity): AuthenticatedUser,
    Path(thread_id): Path<Uuid>,
    Query(query): Query<MessagesQuery>,
) -> impl IntoResponse {
    let thread = match state.threads.find_by_id(thread_id).await {
        Ok(Some(t)) => t,
        Ok(None) => return axum::http::StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!(error = %e, "find_thread_failed");
            return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let is_participant = match identity.role {
        Role::Student => thread.student_code == identity.identifier,
        Role::Professor => thread.professor_id == identity.identifier,
    };
    if !is_participant {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }

    let limit = query.limit.clamp(1, 200);
    match state
        .messages
        .list_page(thread_id, query.before, limit)
        .await
    {
        Ok(messages) => {
            let dtos: Vec<MessageDto> = messages.into_iter().map(Into::into).collect();
            Json(dtos).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "list_messages_failed");
            axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Debug, Serialize)]
struct ChannelSummaryDto {
    channel_id: Uuid,
    last_message_at: Option<chrono::DateTime<chrono::Utc>>,
    last_message_preview: Option<String>,
    unread_count: i64,
}

async fn list_channels(
    State(state): State<AppState>,
    AuthenticatedUser(identity): AuthenticatedUser,
) -> impl IntoResponse {
    let section_ids: Vec<Uuid> = identity
        .accessible_section_ids
        .iter()
        .filter_map(|s| Uuid::parse_str(s).ok())
        .collect();

    match state
        .channels
        .list_summaries(&section_ids, identity.role, &identity.identifier)
        .await
    {
        Ok(summaries) => {
            let dtos: Vec<ChannelSummaryDto> = summaries
                .into_iter()
                .map(|s| ChannelSummaryDto {
                    channel_id: s.channel_id,
                    last_message_at: s.last_message_at,
                    last_message_preview: s.last_message_preview,
                    unread_count: s.unread_count,
                })
                .collect();
            Json(dtos).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "list_channels_failed");
            axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn list_channel_messages(
    State(state): State<AppState>,
    AuthenticatedUser(identity): AuthenticatedUser,
    Path(channel_id): Path<Uuid>,
    Query(query): Query<MessagesQuery>,
) -> impl IntoResponse {
    let is_member = identity
        .accessible_section_ids
        .iter()
        .any(|s| s == &channel_id.to_string());
    if !is_member {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }

    let limit = query.limit.clamp(1, 200);
    match state
        .channels
        .list_page(channel_id, query.before, limit)
        .await
    {
        Ok(messages) => {
            let dtos: Vec<ChannelMessageDto> = messages.into_iter().map(Into::into).collect();
            Json(dtos).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "list_channel_messages_failed");
            axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
