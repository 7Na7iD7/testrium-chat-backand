use std::sync::Arc;

use chat_infra::auth::{IdentityResolver, JwtVerifier, LiveKitTokenIssuer};
use chat_infra::livekit::LiveKitRoomService;
use chat_infra::repository::{ChannelRepository, MessageRepository, ThreadRepository, VoiceRepository};
use chat_infra::Config;
use dashmap::DashMap;
use uuid::Uuid;

use crate::ws::{ConnectionKey, ConnectionManager};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub threads: ThreadRepository,
    pub messages: MessageRepository,
    pub channels: ChannelRepository,
    pub voice_rooms: VoiceRepository,
    pub identity_resolver: Arc<IdentityResolver>,
    pub jwt_verifier: Arc<JwtVerifier>,
    pub connections: Arc<ConnectionManager>,
    pub voice_token_issuer: Arc<LiveKitTokenIssuer>,
    pub voice_room_service: Arc<LiveKitRoomService>,
    pub active_voice_participants: Arc<DashMap<ConnectionKey, Uuid>>,
}
