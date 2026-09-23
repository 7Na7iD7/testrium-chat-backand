use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chat_domain::{Role, VoiceGrants, VoiceRoomStatus};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::rest::extractor::AuthenticatedUser;
use crate::state::AppState;
use crate::ws::protocol::{ServerEvent, VoiceRoomMessageDto};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/voice/sections/:section_id/token", post(join_voice_room))
        .route("/voice/rooms/:room_id/leave", post(leave_room))
        .route("/voice/rooms/:room_id/lock", post(lock_room))
        .route("/voice/rooms/:room_id/unlock", post(unlock_room))
        .route(
            "/voice/rooms/:room_id/participants/:student_code/allow-speak",
            post(allow_speak),
        )
        .route(
            "/voice/rooms/:room_id/participants/:student_code/deny-speak",
            post(deny_speak),
        )
        .route(
            "/voice/rooms/:room_id/participants/:student_code/mute",
            post(mute_participant),
        )
        .route(
            "/voice/rooms/:room_id/participants/:student_code/remove",
            post(remove_participant),
        )
        .route("/voice/rooms/:room_id/messages", get(list_room_messages))
        .route("/voice/rooms/:room_id/recording/start", post(start_recording))
        .route("/voice/rooms/:room_id/recording/stop", post(stop_recording))
        .route("/voice/rooms/:room_id/recording", get(recording_status))
        .route(
            "/voice/sections/:section_id/recordings",
            get(list_section_recordings),
        )
        .route("/voice/recordings/:recording_id", delete(delete_recording))
        .route("/voice/rooms/:room_id/breakout/start", post(start_breakout))
        .route("/voice/rooms/:room_id/breakout/end", post(end_breakout))
        .route("/voice/rooms/:room_id/breakout", get(list_breakout))
        .route("/voice/rooms/:room_id/attendance", get(attendance_report))
        .route("/voice/rooms/:room_id/participation", get(participation_report))
}

#[derive(Debug, Serialize)]
struct VoiceTokenResponse {
    livekit_url: String,
    token: String,
    room_id: Uuid,
    room_name: String,
    max_participants: usize,
}

fn room_name_for(room_id: Uuid) -> String {
    format!("class-{room_id}")
}

async fn join_voice_room(
    State(state): State<AppState>,
    AuthenticatedUser(identity): AuthenticatedUser,
    Path(section_id): Path<Uuid>,
) -> impl IntoResponse {
    let has_access = identity
        .accessible_section_ids
        .iter()
        .any(|s| s == &section_id.to_string());
    if !has_access {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }

    if identity.role != Role::Professor && identity.role != Role::Student {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }

    let professor_id = match identity.role {
        Role::Professor => identity.identifier.clone(),
        Role::Student => match find_section_professor(&state, section_id).await {
            Some(p) => p,
            None => return axum::http::StatusCode::FORBIDDEN.into_response(),
        },
    };

    let room = match state
        .voice_rooms
        .get_or_create_room(section_id, &professor_id)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "voice_room_create_failed");
            return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if room.status == VoiceRoomStatus::Locked && identity.role != Role::Professor {
        return (axum::http::StatusCode::FORBIDDEN, "روم صوتی قفل شده است").into_response();
    }

    let grants = match identity.role {
        Role::Professor => VoiceGrants::for_professor(),
        Role::Student => VoiceGrants::for_student_listener(),
    };

    let room_name = room_name_for(room.room_id);

    let token = match state.voice_token_issuer.issue(
        &room_name,
        &identity.identifier,
        &identity.display_name,
        &grants,
    ) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = %e, "voice_token_issue_failed");
            return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if let Err(e) = state
        .voice_rooms
        .start_session(room.room_id, identity.role, &identity.identifier)
        .await
    {
        tracing::warn!(error = %e, "voice_session_log_failed");
    }

    let was_already_in_room = state
        .active_voice_participants
        .insert((identity.role, identity.identifier.clone()), room.room_id)
        .is_some();
    if !was_already_in_room {
        metrics::gauge!("testrium_voice_active_rooms").increment(1.0);
    }

    Json(VoiceTokenResponse {
        livekit_url: state.config.livekit_url.clone(),
        token,
        room_id: room.room_id,
        room_name,
        max_participants: state.config.voice_max_participants,
    })
    .into_response()
}

async fn find_section_professor(state: &AppState, section_id: Uuid) -> Option<String> {
    state
        .voice_rooms
        .find_by_section(section_id)
        .await
        .ok()
        .flatten()
        .map(|r| r.professor_id)
}

fn require_professor(identity: &chat_domain::Identity) -> Result<(), axum::http::StatusCode> {
    if identity.role != Role::Professor {
        Err(axum::http::StatusCode::FORBIDDEN)
    } else {
        Ok(())
    }
}

async fn room_belongs_to_professor(
    state: &AppState,
    room_id: Uuid,
    professor_id: &str,
) -> Result<chat_domain::VoiceRoom, axum::http::StatusCode> {
    let room = state
        .voice_rooms
        .find_by_id(room_id)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(axum::http::StatusCode::NOT_FOUND)?;

    if room.professor_id != professor_id {
        return Err(axum::http::StatusCode::FORBIDDEN);
    }

    Ok(room)
}

async fn broadcast_raise_hand_queue(state: &AppState, room: &chat_domain::VoiceRoom) {
    let queue = state
        .voice_rooms
        .list_open_raise_hand_queue(room.room_id)
        .await
        .unwrap_or_default();

    state.connections.push_channel(
        &room.section_id,
        ServerEvent::VoiceRaiseHandQueueUpdated {
            room_id: room.room_id,
            queue: queue.into_iter().map(Into::into).collect(),
        },
        None,
    );
}

async fn leave_room(
    State(state): State<AppState>,
    AuthenticatedUser(identity): AuthenticatedUser,
    Path(room_id): Path<Uuid>,
) -> impl IntoResponse {
    let room = match state.voice_rooms.find_by_id(room_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return axum::http::StatusCode::NOT_FOUND.into_response(),
        Err(_) => return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let _ = state
        .voice_rooms
        .end_open_sessions(room_id, &identity.identifier)
        .await;
    let _ = state
        .voice_rooms
        .resolve_raise_hand(room_id, &identity.identifier, "left")
        .await;
    let _ = state
        .voice_rooms
        .end_speaking_event(room_id, &identity.identifier)
        .await;

    if state
        .active_voice_participants
        .remove(&(identity.role, identity.identifier.clone()))
        .is_some()
    {
        metrics::gauge!("testrium_voice_active_rooms").decrement(1.0);
    }

    broadcast_raise_hand_queue(&state, &room).await;

    axum::http::StatusCode::NO_CONTENT.into_response()
}

async fn lock_room(
    State(state): State<AppState>,
    AuthenticatedUser(identity): AuthenticatedUser,
    Path(room_id): Path<Uuid>,
) -> impl IntoResponse {
    if let Err(code) = require_professor(&identity) {
        return code.into_response();
    }
    let room = match room_belongs_to_professor(&state, room_id, &identity.identifier).await {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };

    if state
        .voice_rooms
        .set_status(room.room_id, VoiceRoomStatus::Locked)
        .await
        .is_err()
    {
        return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    state.connections.push_channel(
        &room.section_id,
        ServerEvent::VoiceRoomLocked {
            room_id: room.room_id,
            locked: true,
        },
        None,
    );

    axum::http::StatusCode::NO_CONTENT.into_response()
}

async fn unlock_room(
    State(state): State<AppState>,
    AuthenticatedUser(identity): AuthenticatedUser,
    Path(room_id): Path<Uuid>,
) -> impl IntoResponse {
    if let Err(code) = require_professor(&identity) {
        return code.into_response();
    }
    let room = match room_belongs_to_professor(&state, room_id, &identity.identifier).await {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };

    if state
        .voice_rooms
        .set_status(room.room_id, VoiceRoomStatus::Open)
        .await
        .is_err()
    {
        return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    state.connections.push_channel(
        &room.section_id,
        ServerEvent::VoiceRoomLocked {
            room_id: room.room_id,
            locked: false,
        },
        None,
    );

    axum::http::StatusCode::NO_CONTENT.into_response()
}

async fn allow_speak(
    State(state): State<AppState>,
    AuthenticatedUser(identity): AuthenticatedUser,
    Path((room_id, student_code)): Path<(Uuid, String)>,
) -> impl IntoResponse {
    if let Err(code) = require_professor(&identity) {
        return code.into_response();
    }
    let room = match room_belongs_to_professor(&state, room_id, &identity.identifier).await {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };

    let room_name = room_name_for(room.room_id);
    if let Err(e) = state
        .voice_room_service
        .update_participant(&room_name, &student_code, true)
        .await
    {
        tracing::error!(error = %e, "voice_allow_speak_failed");
        return axum::http::StatusCode::BAD_GATEWAY.into_response();
    }

    let _ = state
        .voice_rooms
        .resolve_raise_hand(room.room_id, &student_code, "allowed")
        .await;

    state.connections.push(
        &(Role::Student, student_code.clone()),
        ServerEvent::VoicePermissionChanged {
            room_id: room.room_id,
            participant_id: student_code,
            can_publish: true,
        },
    );

    broadcast_raise_hand_queue(&state, &room).await;

    axum::http::StatusCode::NO_CONTENT.into_response()
}

async fn deny_speak(
    State(state): State<AppState>,
    AuthenticatedUser(identity): AuthenticatedUser,
    Path((room_id, student_code)): Path<(Uuid, String)>,
) -> impl IntoResponse {
    if let Err(code) = require_professor(&identity) {
        return code.into_response();
    }
    let room = match room_belongs_to_professor(&state, room_id, &identity.identifier).await {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };

    let _ = state
        .voice_rooms
        .resolve_raise_hand(room.room_id, &student_code, "denied")
        .await;

    broadcast_raise_hand_queue(&state, &room).await;

    axum::http::StatusCode::NO_CONTENT.into_response()
}

async fn mute_participant(
    State(state): State<AppState>,
    AuthenticatedUser(identity): AuthenticatedUser,
    Path((room_id, student_code)): Path<(Uuid, String)>,
) -> impl IntoResponse {
    if let Err(code) = require_professor(&identity) {
        return code.into_response();
    }
    let room = match room_belongs_to_professor(&state, room_id, &identity.identifier).await {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };

    let room_name = room_name_for(room.room_id);
    if let Err(e) = state
        .voice_room_service
        .update_participant(&room_name, &student_code, false)
        .await
    {
        tracing::error!(error = %e, "voice_mute_failed");
        return axum::http::StatusCode::BAD_GATEWAY.into_response();
    }

    state.connections.push(
        &(Role::Student, student_code.clone()),
        ServerEvent::VoicePermissionChanged {
            room_id: room.room_id,
            participant_id: student_code,
            can_publish: false,
        },
    );

    axum::http::StatusCode::NO_CONTENT.into_response()
}

async fn remove_participant(
    State(state): State<AppState>,
    AuthenticatedUser(identity): AuthenticatedUser,
    Path((room_id, student_code)): Path<(Uuid, String)>,
) -> impl IntoResponse {
    if let Err(code) = require_professor(&identity) {
        return code.into_response();
    }
    let room = match room_belongs_to_professor(&state, room_id, &identity.identifier).await {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };

    let room_name = room_name_for(room.room_id);
    if let Err(e) = state
        .voice_room_service
        .remove_participant(&room_name, &student_code)
        .await
    {
        tracing::error!(error = %e, "voice_remove_failed");
        return axum::http::StatusCode::BAD_GATEWAY.into_response();
    }

    let _ = state
        .voice_rooms
        .end_open_sessions(room.room_id, &student_code)
        .await;
    let _ = state
        .voice_rooms
        .resolve_raise_hand(room.room_id, &student_code, "denied")
        .await;
    let _ = state
        .voice_rooms
        .end_speaking_event(room.room_id, &student_code)
        .await;

    state
        .connections
        .push(
            &(Role::Student, student_code.clone()),
            ServerEvent::VoiceParticipantRemoved {
                room_id: room.room_id,
                participant_id: student_code,
            },
        );

    broadcast_raise_hand_queue(&state, &room).await;

    axum::http::StatusCode::NO_CONTENT.into_response()
}

#[derive(Debug, Deserialize)]
struct ListMessagesQuery {
    limit: Option<i64>,
}

fn has_section_access(identity: &chat_domain::Identity, section_id: Uuid) -> bool {
    identity
        .accessible_section_ids
        .iter()
        .any(|s| s == &section_id.to_string())
}

async fn list_room_messages(
    State(state): State<AppState>,
    AuthenticatedUser(identity): AuthenticatedUser,
    Path(room_id): Path<Uuid>,
    Query(params): Query<ListMessagesQuery>,
) -> impl IntoResponse {
    let room = match state.voice_rooms.find_by_id(room_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return axum::http::StatusCode::NOT_FOUND.into_response(),
        Err(_) => return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    if !has_section_access(&identity, room.section_id) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }

    let limit = params.limit.unwrap_or(50).clamp(1, 200);

    match state.voice_rooms.list_room_messages(room_id, limit).await {
        Ok(messages) => Json(
            messages
                .into_iter()
                .map(VoiceRoomMessageDto::from)
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "voice_room_messages_list_failed");
            axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn start_recording(
    State(state): State<AppState>,
    AuthenticatedUser(identity): AuthenticatedUser,
    Path(room_id): Path<Uuid>,
) -> impl IntoResponse {
    if let Err(code) = require_professor(&identity) {
        return code.into_response();
    }
    let room = match room_belongs_to_professor(&state, room_id, &identity.identifier).await {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };

    let room_name = room_name_for(room.room_id);
    match state
        .voice_room_service
        .start_room_composite_egress(&room_name, &state.config.egress_s3_output())
        .await
    {
        Ok(egress_id) => {
            let _ = state
                .voice_rooms
                .set_recording_started(room.room_id, &egress_id)
                .await;
            let _ = state
                .voice_rooms
                .insert_recording(room.room_id, room.section_id, &egress_id)
                .await;

            state.connections.push_channel(
                &room.section_id,
                ServerEvent::VoiceRecordingStarted { room_id: room.room_id },
                None,
            );

            Json(serde_json::json!({ "egress_id": egress_id })).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "voice_recording_start_failed");
            axum::http::StatusCode::BAD_GATEWAY.into_response()
        }
    }
}

async fn stop_recording(
    State(state): State<AppState>,
    AuthenticatedUser(identity): AuthenticatedUser,
    Path(room_id): Path<Uuid>,
) -> impl IntoResponse {
    if let Err(code) = require_professor(&identity) {
        return code.into_response();
    }
    let room = match room_belongs_to_professor(&state, room_id, &identity.identifier).await {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };

    let egress_id = match &room.recording_egress_id {
        Some(id) => id.clone(),
        None => return (axum::http::StatusCode::BAD_REQUEST, "ضبطی در حال اجرا نیست").into_response(),
    };

    let room_name = room_name_for(room.room_id);
    match state.voice_room_service.stop_egress(&room_name, &egress_id).await {
        Ok(url) => {
            let _ = state
                .voice_rooms
                .set_recording_stopped(room.room_id, url.as_deref())
                .await;
            let _ = state
                .voice_rooms
                .finish_recording(&egress_id, url.as_deref())
                .await;

            state.connections.push_channel(
                &room.section_id,
                ServerEvent::VoiceRecordingStopped {
                    room_id: room.room_id,
                    recording_url: url.clone(),
                },
                None,
            );

            Json(serde_json::json!({ "recording_url": url })).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "voice_recording_stop_failed");
            axum::http::StatusCode::BAD_GATEWAY.into_response()
        }
    }
}

async fn recording_status(
    State(state): State<AppState>,
    AuthenticatedUser(identity): AuthenticatedUser,
    Path(room_id): Path<Uuid>,
) -> impl IntoResponse {
    let room = match state.voice_rooms.find_by_id(room_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return axum::http::StatusCode::NOT_FOUND.into_response(),
        Err(_) => return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    if !has_section_access(&identity, room.section_id) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }

    Json(serde_json::json!({
        "recording_egress_id": room.recording_egress_id,
        "recording_url": room.recording_url,
        "recording_started_at": room.recording_started_at,
    }))
    .into_response()
}

#[derive(Debug, Deserialize)]
struct BreakoutGroupRequest {
    label: String,
    student_codes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct BreakoutStartRequest {
    groups: Vec<BreakoutGroupRequest>,
}

#[derive(Debug, Serialize)]
struct BreakoutGroupResponse {
    room_id: Uuid,
    label: String,
    student_codes: Vec<String>,
}

async fn start_breakout(
    State(state): State<AppState>,
    AuthenticatedUser(identity): AuthenticatedUser,
    Path(room_id): Path<Uuid>,
    Json(payload): Json<BreakoutStartRequest>,
) -> impl IntoResponse {
    if let Err(code) = require_professor(&identity) {
        return code.into_response();
    }
    let room = match room_belongs_to_professor(&state, room_id, &identity.identifier).await {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };

    let groups: Vec<BreakoutGroupRequest> = payload
        .groups
        .into_iter()
        .filter(|g| !g.label.trim().is_empty() && !g.student_codes.is_empty())
        .collect();

    if groups.is_empty() {
        return (axum::http::StatusCode::BAD_REQUEST, "حداقل یک گروه معتبر لازم است").into_response();
    }

    let labels: Vec<String> = groups.iter().map(|g| g.label.clone()).collect();

    let created_rooms = match state
        .voice_rooms
        .create_breakout_rooms(room.room_id, room.section_id, &identity.identifier, &labels)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "voice_breakout_create_failed");
            return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let mut response = Vec::with_capacity(created_rooms.len());

    for (breakout_room, group) in created_rooms.into_iter().zip(groups.into_iter()) {
        if let Err(e) = state
            .voice_rooms
            .add_breakout_members(breakout_room.room_id, &group.student_codes)
            .await
        {
            tracing::error!(error = %e, "voice_breakout_members_failed");
        }

        let breakout_room_name = room_name_for(breakout_room.room_id);

        for student_code in &group.student_codes {
            let token = match state.voice_token_issuer.issue(
                &breakout_room_name,
                student_code,
                student_code,
                &VoiceGrants::for_student_speaker(),
            ) {
                Ok(t) => t,
                Err(e) => {
                    tracing::error!(error = %e, "voice_breakout_token_failed");
                    continue;
                }
            };

            state.connections.push(
                &(Role::Student, student_code.clone()),
                ServerEvent::VoiceBreakoutAssigned {
                    parent_room_id: room.room_id,
                    room_id: breakout_room.room_id,
                    room_name: breakout_room_name.clone(),
                    group_label: group.label.clone(),
                    livekit_url: state.config.livekit_url.clone(),
                    token,
                },
            );
        }

        response.push(BreakoutGroupResponse {
            room_id: breakout_room.room_id,
            label: group.label,
            student_codes: group.student_codes,
        });
    }

    Json(response).into_response()
}

async fn end_breakout(
    State(state): State<AppState>,
    AuthenticatedUser(identity): AuthenticatedUser,
    Path(room_id): Path<Uuid>,
) -> impl IntoResponse {
    if let Err(code) = require_professor(&identity) {
        return code.into_response();
    }
    let room = match room_belongs_to_professor(&state, room_id, &identity.identifier).await {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };

    let breakout_rooms = match state.voice_rooms.list_breakout_rooms(room.room_id).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "voice_breakout_list_failed");
            return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    for breakout_room in &breakout_rooms {
        let members = state
            .voice_rooms
            .list_breakout_members(breakout_room.room_id)
            .await
            .unwrap_or_default();

        for student_code in members {
            state.connections.push(
                &(Role::Student, student_code),
                ServerEvent::VoiceBreakoutEnded {
                    parent_room_id: room.room_id,
                },
            );
        }

        let breakout_room_name = room_name_for(breakout_room.room_id);
        let _ = state.voice_room_service.delete_room(&breakout_room_name).await;
    }

    if let Err(e) = state.voice_rooms.close_breakout_rooms(room.room_id).await {
        tracing::error!(error = %e, "voice_breakout_close_failed");
        return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    axum::http::StatusCode::NO_CONTENT.into_response()
}

async fn list_breakout(
    State(state): State<AppState>,
    AuthenticatedUser(identity): AuthenticatedUser,
    Path(room_id): Path<Uuid>,
) -> impl IntoResponse {
    if let Err(code) = require_professor(&identity) {
        return code.into_response();
    }
    let room = match room_belongs_to_professor(&state, room_id, &identity.identifier).await {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };

    let breakout_rooms = match state.voice_rooms.list_breakout_rooms(room.room_id).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "voice_breakout_list_failed");
            return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let mut response = Vec::with_capacity(breakout_rooms.len());
    for breakout_room in breakout_rooms {
        let student_codes = state
            .voice_rooms
            .list_breakout_members(breakout_room.room_id)
            .await
            .unwrap_or_default();

        response.push(BreakoutGroupResponse {
            room_id: breakout_room.room_id,
            label: breakout_room.breakout_label.unwrap_or_default(),
            student_codes,
        });
    }

    Json(response).into_response()
}

async fn attendance_report(
    State(state): State<AppState>,
    AuthenticatedUser(identity): AuthenticatedUser,
    Path(room_id): Path<Uuid>,
) -> impl IntoResponse {
    if let Err(code) = require_professor(&identity) {
        return code.into_response();
    }
    let room = match room_belongs_to_professor(&state, room_id, &identity.identifier).await {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };

    match state
        .voice_rooms
        .attendance_report(room.room_id, state.config.voice_attendance_threshold_seconds)
        .await
    {
        Ok(report) => Json(report).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "voice_attendance_report_failed");
            axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn participation_report(
    State(state): State<AppState>,
    AuthenticatedUser(identity): AuthenticatedUser,
    Path(room_id): Path<Uuid>,
) -> impl IntoResponse {
    if let Err(code) = require_professor(&identity) {
        return code.into_response();
    }
    let room = match room_belongs_to_professor(&state, room_id, &identity.identifier).await {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };

    match state.voice_rooms.participation_report(room.room_id).await {
        Ok(report) => Json(report).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "voice_participation_report_failed");
            axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn list_section_recordings(
    State(state): State<AppState>,
    AuthenticatedUser(identity): AuthenticatedUser,
    Path(section_id): Path<Uuid>,
) -> impl IntoResponse {
    if !has_section_access(&identity, section_id) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }

    match state.voice_rooms.list_section_recordings(section_id).await {
        Ok(recordings) => Json(recordings).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "voice_recordings_list_failed");
            axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn delete_recording(
    State(state): State<AppState>,
    AuthenticatedUser(identity): AuthenticatedUser,
    Path(recording_id): Path<Uuid>,
) -> impl IntoResponse {
    if let Err(code) = require_professor(&identity) {
        return code.into_response();
    }

    let recording = match state.voice_rooms.find_recording(recording_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return axum::http::StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!(error = %e, "voice_recording_find_failed");
            return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let room = match state.voice_rooms.find_by_section(recording.section_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return axum::http::StatusCode::FORBIDDEN.into_response(),
        Err(_) => return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    if room.professor_id != identity.identifier {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }

    if let Some(url) = &recording.recording_url {
        let output = state.config.egress_s3_output();
        let key = output.object_key_from_location(url);
        if let Err(e) = output.delete_object(&key).await {
            tracing::error!(error = %e, "voice_recording_object_delete_failed");
            return (
                axum::http::StatusCode::BAD_GATEWAY,
                "حذف فایل از storage ناموفق بود",
            )
                .into_response();
        }
    }

    match state.voice_rooms.delete_recording(recording_id).await {
        Ok(true) => axum::http::StatusCode::NO_CONTENT.into_response(),
        Ok(false) => axum::http::StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!(error = %e, "voice_recording_delete_failed");
            axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
