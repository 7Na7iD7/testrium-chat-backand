use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use chat_domain::Role;
use serde::Deserialize;
use tracing::{info, warn};
use uuid::Uuid;

use crate::state::AppState;
use crate::ws::protocol::{
    ChannelMessageDto, ClientEvent, MessageDto, ServerEvent, VoiceRoomMessageDto,
};

#[derive(Debug, Deserialize)]
pub struct WsAuthQuery {
    token: String,
}

pub async fn ws_upgrade_handler(
    ws: WebSocketUpgrade,
    Query(auth): Query<WsAuthQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let claims = match state.jwt_verifier.verify(&auth.token).await {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "ws_upgrade_rejected: invalid jwt");
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
    };

    let identity = match state.identity_resolver.resolve_for_claims(&claims).await {
        Ok(identity) => identity,
        Err(e) => {
            warn!(error = %e, "ws_upgrade_rejected: identity resolve failed");
            return axum::http::StatusCode::FORBIDDEN.into_response();
        }
    };

    ws.on_upgrade(move |socket| handle_socket(socket, state, identity))
}

fn parse_section_ids(identity: &chat_domain::Identity) -> Vec<Uuid> {
    identity
        .accessible_section_ids
        .iter()
        .filter_map(|s| Uuid::parse_str(s).ok())
        .collect()
}

async fn handle_socket(socket: WebSocket, state: AppState, identity: chat_domain::Identity) {
    let key = (identity.role, identity.identifier.clone());
    let mut rx = state.connections.register(key.clone());

    let channel_ids = parse_section_ids(&identity);
    state.connections.join_channels(&key, &channel_ids);

    info!(role = ?identity.role, identifier = %identity.identifier, channels = channel_ids.len(), "ws_connected");

    let (mut ws_sender, mut ws_receiver) = socket.split();

    let send_task = tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            let payload = match serde_json::to_string(&event) {
                Ok(p) => p,
                Err(e) => {
                    warn!(error = %e, "ws_serialize_failed");
                    continue;
                }
            };
            if ws_sender.send(WsMessage::Text(payload)).await.is_err() {
                break;
            }
        }
    });

    let recv_state = state.clone();
    let recv_identity = identity.clone();
    let recv_key = key.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_receiver.next().await {
            if let WsMessage::Text(text) = msg {
                handle_client_text(&text, &recv_state, &recv_identity, &recv_key).await;
            }
        }
    });

    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }

    state.connections.unregister_closed(&key);
    state.connections.leave_channels(&key, &channel_ids);

    if let Some((_, room_id)) = state.active_voice_participants.remove(&key) {
        metrics::gauge!("testrium_voice_active_rooms").decrement(1.0);
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

        if let Ok(Some(room)) = state.voice_rooms.find_by_id(room_id).await {
            broadcast_raise_hand_queue(&state, &room).await;
        }
    }

    info!(role = ?identity.role, identifier = %identity.identifier, "ws_disconnected");
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

async fn handle_client_text(
    text: &str,
    state: &AppState,
    identity: &chat_domain::Identity,
    self_key: &crate::ws::ConnectionKey,
) {
    if !state.connections.check_general_rate_limit(self_key) {
        return;
    }

    let event: ClientEvent = match serde_json::from_str(text) {
        Ok(e) => e,
        Err(e) => {
            warn!(error = %e, "ws_bad_client_event");
            state.connections.push(
                self_key,
                ServerEvent::Error {
                    message: "قالب پیام نامعتبر است".to_string(),
                },
            );
            return;
        }
    };

    match event {
        ClientEvent::Ping => {
            state.connections.push(self_key, ServerEvent::Pong);
        }
        ClientEvent::MarkThreadRead { thread_id } => {
            match state
                .messages
                .mark_thread_read(thread_id, identity.role)
                .await
            {
                Ok(message_ids) if !message_ids.is_empty() => {
                    let _ = state.threads.mark_read(thread_id, identity.role).await;

                    if let Ok(Some(thread)) = state.threads.find_by_id(thread_id).await {
                        let peer_key = match identity.role {
                            Role::Student => (Role::Professor, thread.professor_id.clone()),
                            Role::Professor => (Role::Student, thread.student_code.clone()),
                        };
                        state.connections.push(
                            &peer_key,
                            ServerEvent::ThreadRead {
                                thread_id,
                                reader_role: identity.role,
                            },
                        );
                    }
                }
                Ok(_) => {}
                Err(e) => warn!(error = %e, "mark_thread_read_failed"),
            }
        }
        ClientEvent::SendMessage {
            peer_id,
            body,
            client_message_id,
            reply_to_message_id,
        } => {
            handle_send_message(
                state,
                identity,
                self_key,
                peer_id,
                body,
                client_message_id,
                reply_to_message_id,
            )
            .await;
        }
        ClientEvent::SendChannelMessage {
            channel_id,
            body,
            client_message_id,
            reply_to_message_id,
        } => {
            handle_send_channel_message(
                state,
                identity,
                self_key,
                channel_id,
                body,
                client_message_id,
                reply_to_message_id,
            )
            .await;
        }
        ClientEvent::MarkChannelRead { channel_id } => {
            if !identity
                .accessible_section_ids
                .iter()
                .any(|s| s == &channel_id.to_string())
            {
                return;
            }
            if let Err(e) = state
                .channels
                .mark_read(channel_id, identity.role, &identity.identifier)
                .await
            {
                warn!(error = %e, "mark_channel_read_failed");
            }
        }
        ClientEvent::VoiceRequestToSpeak { room_id } => {
            if !state.connections.check_voice_control_rate_limit(self_key) {
                return;
            }
            handle_voice_request_to_speak(state, identity, self_key, room_id).await;
        }
        ClientEvent::VoiceCancelRaiseHand { room_id } => {
            if !state.connections.check_voice_control_rate_limit(self_key) {
                return;
            }
            handle_voice_cancel_raise_hand(state, identity, room_id).await;
        }
        ClientEvent::VoiceSpeakingStateChanged { room_id, speaking } => {
            if !state.connections.check_voice_control_rate_limit(self_key) {
                return;
            }
            handle_voice_speaking_state_changed(state, identity, room_id, speaking).await;
        }
        ClientEvent::VoiceScreenShareStateChanged { room_id, sharing } => {
            if !state.connections.check_voice_control_rate_limit(self_key) {
                return;
            }
            handle_voice_screen_share_state_changed(state, identity, room_id, sharing).await;
        }
        ClientEvent::SendVoiceRoomMessage {
            room_id,
            body,
            client_message_id,
        } => {
            handle_send_voice_room_message(state, identity, self_key, room_id, body, client_message_id)
                .await;
        }
        ClientEvent::VoiceResyncRoomState { room_id } => {
            handle_voice_resync_room_state(state, identity, self_key, room_id).await;
        }
    }
}

async fn voice_room_access_check(
    state: &AppState,
    identity: &chat_domain::Identity,
    room_id: uuid::Uuid,
) -> Option<chat_domain::VoiceRoom> {
    let room = match state.voice_rooms.find_by_id(room_id).await {
        Ok(Some(r)) => r,
        _ => return None,
    };

    if !identity
        .accessible_section_ids
        .iter()
        .any(|s| s == &room.section_id.to_string())
    {
        return None;
    }

    Some(room)
}

async fn handle_voice_resync_room_state(
    state: &AppState,
    identity: &chat_domain::Identity,
    self_key: &crate::ws::ConnectionKey,
    room_id: uuid::Uuid,
) {
    let room = match voice_room_access_check(state, identity, room_id).await {
        Some(r) => r,
        None => {
            state.connections.push(
                self_key,
                ServerEvent::Error {
                    message: "این روم صوتی پیدا نشد یا دسترسی ندارید".to_string(),
                },
            );
            return;
        }
    };

    let queue = state
        .voice_rooms
        .list_open_raise_hand_queue(room_id)
        .await
        .unwrap_or_default();

    state.connections.push(
        self_key,
        ServerEvent::VoiceRoomStateSnapshot {
            room_id,
            locked: matches!(room.status, chat_domain::VoiceRoomStatus::Locked),
            recording_active: room.recording_egress_id.is_some(),
            recording_url: room.recording_url.clone(),
            raise_hand_queue: queue.into_iter().map(Into::into).collect(),
        },
    );
}

async fn handle_voice_request_to_speak(
    state: &AppState,
    identity: &chat_domain::Identity,
    self_key: &crate::ws::ConnectionKey,
    room_id: uuid::Uuid,
) {
    if identity.role != Role::Student {
        return;
    }

    let room = match voice_room_access_check(state, identity, room_id).await {
        Some(r) => r,
        None => {
            state.connections.push(
                self_key,
                ServerEvent::Error {
                    message: "این روم صوتی پیدا نشد یا دسترسی ندارید".to_string(),
                },
            );
            return;
        }
    };

    match state
        .voice_rooms
        .enqueue_raise_hand(room_id, identity.role, &identity.identifier)
        .await
    {
        Ok(true) => {}
        Ok(false) => return,
        Err(e) => {
            warn!(error = %e, "voice_raise_hand_enqueue_failed");
            return;
        }
    }

    state
        .active_voice_participants
        .insert(self_key.clone(), room_id);

    let professor_key = (Role::Professor, room.professor_id.clone());
    state.connections.push(
        &professor_key,
        ServerEvent::VoiceSpeakRequested {
            room_id,
            student_id: identity.identifier.clone(),
        },
    );

    broadcast_raise_hand_queue(state, &room).await;
}

async fn handle_voice_cancel_raise_hand(
    state: &AppState,
    identity: &chat_domain::Identity,
    room_id: uuid::Uuid,
) {
    let room = match voice_room_access_check(state, identity, room_id).await {
        Some(r) => r,
        None => return,
    };

    if let Err(e) = state
        .voice_rooms
        .resolve_raise_hand(room_id, &identity.identifier, "cancelled")
        .await
    {
        warn!(error = %e, "voice_raise_hand_cancel_failed");
        return;
    }

    broadcast_raise_hand_queue(state, &room).await;
}

async fn handle_voice_speaking_state_changed(
    state: &AppState,
    identity: &chat_domain::Identity,
    room_id: uuid::Uuid,
    speaking: bool,
) {
    let room = match voice_room_access_check(state, identity, room_id).await {
        Some(r) => r,
        None => return,
    };
    let _ = room;

    let result = if speaking {
        state
            .voice_rooms
            .start_speaking_event(room_id, identity.role, &identity.identifier)
            .await
    } else {
        state
            .voice_rooms
            .end_speaking_event(room_id, &identity.identifier)
            .await
    };

    if let Err(e) = result {
        warn!(error = %e, "voice_speaking_event_failed");
    }
}

async fn handle_voice_screen_share_state_changed(
    state: &AppState,
    identity: &chat_domain::Identity,
    room_id: uuid::Uuid,
    sharing: bool,
) {
    let room = match voice_room_access_check(state, identity, room_id).await {
        Some(r) => r,
        None => return,
    };

    state.connections.push_channel(
        &room.section_id,
        ServerEvent::VoiceScreenShareStateChanged {
            room_id,
            participant_id: identity.identifier.clone(),
            sharing,
        },
        None,
    );
}

async fn handle_send_voice_room_message(
    state: &AppState,
    identity: &chat_domain::Identity,
    self_key: &crate::ws::ConnectionKey,
    room_id: uuid::Uuid,
    body: String,
    client_message_id: uuid::Uuid,
) {
    let room = match voice_room_access_check(state, identity, room_id).await {
        Some(r) => r,
        None => {
            state.connections.push(
                self_key,
                ServerEvent::Error {
                    message: "این روم صوتی پیدا نشد یا دسترسی ندارید".to_string(),
                },
            );
            return;
        }
    };

    let body = body.trim();
    if body.is_empty() {
        state.connections.push(
            self_key,
            ServerEvent::Error {
                message: "پیام نمی‌تواند خالی باشد".to_string(),
            },
        );
        return;
    }
    if body.chars().count() > 2000 {
        state.connections.push(
            self_key,
            ServerEvent::Error {
                message: "طول پیام از ۲۰۰۰ کاراکتر بیشتر است".to_string(),
            },
        );
        return;
    }

    let saved = state
        .voice_rooms
        .insert_room_message(room_id, identity.role, &identity.identifier, body)
        .await;

    let message = match saved {
        Ok(m) => m,
        Err(e) => {
            warn!(error = %e, "voice_room_message_insert_failed");
            state.connections.push(
                self_key,
                ServerEvent::Error {
                    message: "خطا در ذخیره‌ی پیام".to_string(),
                },
            );
            return;
        }
    };

    metrics::counter!("testrium_ws_messages_total", "event_type" => "voice_room_message")
        .increment(1);

    let dto: VoiceRoomMessageDto = message.into();

    state.connections.push(
        self_key,
        ServerEvent::VoiceRoomMessageAck {
            client_message_id,
            message: dto.clone(),
        },
    );

    state.connections.push_channel(
        &room.section_id,
        ServerEvent::NewVoiceRoomMessage { room_id, message: dto },
        Some(self_key),
    );
}

async fn handle_send_message(
    state: &AppState,
    identity: &chat_domain::Identity,
    self_key: &crate::ws::ConnectionKey,
    peer_id: String,
    body: String,
    client_message_id: uuid::Uuid,
    reply_to_message_id: Option<uuid::Uuid>,
) {
    let body = body.trim();
    if body.is_empty() {
        state.connections.push(
            self_key,
            ServerEvent::Error {
                message: "پیام نمی‌تواند خالی باشد".to_string(),
            },
        );
        return;
    }
    if body.chars().count() > state.config.max_message_length {
        state.connections.push(
            self_key,
            ServerEvent::Error {
                message: format!(
                    "طول پیام از {} کاراکتر بیشتر است",
                    state.config.max_message_length
                ),
            },
        );
        return;
    }

    let (professor_id, student_code) = match identity.role {
        Role::Professor => (identity.identifier.clone(), peer_id.clone()),
        Role::Student => (peer_id.clone(), identity.identifier.clone()),
    };

    let existing = state
        .threads
        .get_or_create_if_linked(&professor_id, &student_code, state.identity_resolver.as_ref())
        .await;

    let thread = match existing {
        Ok(Some(t)) => t,
        Ok(None) => {
            state.connections.push(
                self_key,
                ServerEvent::Error {
                    message: "این دانشجو در هیچ‌کدام از سکشن‌های شما ثبت‌نام نکرده است"
                        .to_string(),
                },
            );
            return;
        }
        Err(e) => {
            warn!(error = %e, "thread_resolve_failed");
            state.connections.push(
                self_key,
                ServerEvent::Error {
                    message: "خطا در برقراری گفتگو".to_string(),
                },
            );
            return;
        }
    };

    let saved = state
        .messages
        .insert(
            thread.thread_id,
            identity.role,
            &identity.identifier,
            body,
            reply_to_message_id,
        )
        .await;

    let message = match saved {
        Ok(m) => m,
        Err(e) => {
            warn!(error = %e, "message_insert_failed");
            state.connections.push(
                self_key,
                ServerEvent::Error {
                    message: "خطا در ذخیره‌ی پیام".to_string(),
                },
            );
            return;
        }
    };

    let _ = state
        .threads
        .touch_last_message(thread.thread_id, &message.body, identity.role)
        .await;

    metrics::counter!("testrium_ws_messages_total", "event_type" => "thread_message")
        .increment(1);

    let dto: MessageDto = message.clone().into();

    state.connections.push(
        self_key,
        ServerEvent::MessageAck {
            client_message_id,
            message: dto.clone(),
        },
    );

    let peer_key = match identity.role {
        Role::Student => (Role::Professor, thread.professor_id.clone()),
        Role::Professor => (Role::Student, thread.student_code.clone()),
    };
    let delivered_live = state.connections.push(
        &peer_key,
        ServerEvent::NewMessage {
            thread_id: thread.thread_id,
            message: dto,
        },
    );

    if delivered_live {
        let _ = state.messages.mark_delivered(&[message.message_id]).await;
    }
}

async fn handle_send_channel_message(
    state: &AppState,
    identity: &chat_domain::Identity,
    self_key: &crate::ws::ConnectionKey,
    channel_id: uuid::Uuid,
    body: String,
    client_message_id: uuid::Uuid,
    reply_to_message_id: Option<uuid::Uuid>,
) {
    if !identity
        .accessible_section_ids
        .iter()
        .any(|s| s == &channel_id.to_string())
    {
        state.connections.push(
            self_key,
            ServerEvent::Error {
                message: "شما عضو این کلاس نیستید".to_string(),
            },
        );
        return;
    }

    if identity.role != Role::Professor {
        state.connections.push(
            self_key,
            ServerEvent::Error {
                message: "فقط استاد می‌تواند در این کانال پیام ارسال کند".to_string(),
            },
        );
        return;
    }

    let body = body.trim();
    if body.is_empty() {
        state.connections.push(
            self_key,
            ServerEvent::Error {
                message: "پیام نمی‌تواند خالی باشد".to_string(),
            },
        );
        return;
    }
    if body.chars().count() > state.config.max_message_length {
        state.connections.push(
            self_key,
            ServerEvent::Error {
                message: format!(
                    "طول پیام از {} کاراکتر بیشتر است",
                    state.config.max_message_length
                ),
            },
        );
        return;
    }

    let saved = state
        .channels
        .insert_message(
            channel_id,
            identity.role,
            &identity.identifier,
            body,
            reply_to_message_id,
        )
        .await;

    let message = match saved {
        Ok(m) => m,
        Err(e) => {
            warn!(error = %e, "channel_message_insert_failed");
            state.connections.push(
                self_key,
                ServerEvent::Error {
                    message: "خطا در ذخیره‌ی پیام".to_string(),
                },
            );
            return;
        }
    };

    metrics::counter!("testrium_ws_messages_total", "event_type" => "channel_message")
        .increment(1);

    let dto: ChannelMessageDto = message.into();

    state.connections.push(
        self_key,
        ServerEvent::ChannelMessageAck {
            client_message_id,
            message: dto.clone(),
        },
    );

    state.connections.push_channel(
        &channel_id,
        ServerEvent::NewChannelMessage {
            channel_id,
            message: dto,
        },
        Some(self_key),
    );
}

use futures::{SinkExt, StreamExt};
