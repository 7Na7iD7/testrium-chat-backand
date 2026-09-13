mod rest;
mod state;
mod ws;

use std::sync::Arc;

use anyhow::Result;
use axum::routing::get;
use axum::Router;
use chat_infra::auth::{IdentityResolver, JwtVerifier, LiveKitTokenIssuer};
use chat_infra::livekit::LiveKitRoomService;
use chat_infra::repository::{ChannelRepository, MessageRepository, ThreadRepository, VoiceRepository};
use chat_infra::{db, Config};
use dashmap::DashMap;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use crate::state::AppState;
use crate::ws::{ws_upgrade_handler, ConnectionManager};

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .json()
        .init();

    let config = Config::from_env()?;
    tracing::info!(port = config.port, "starting testrium-chat-server");

    let pool = db::connect(&config.database_url).await?;

    let threads = ThreadRepository::new(pool.clone());
    let messages = MessageRepository::new(pool.clone());
    let channels = ChannelRepository::new(pool.clone());
    let voice_rooms = VoiceRepository::new(pool);

    let identity_resolver = Arc::new(IdentityResolver::new(
        config.supabase_url.clone(),
        config.chat_server_shared_secret.clone(),
    ));
    let jwt_verifier = Arc::new(JwtVerifier::new(
        config.supabase_url.clone(),
        config.legacy_jwt_secret.clone(),
    ));
    let connections = Arc::new(ConnectionManager::new());

    let voice_token_issuer = Arc::new(LiveKitTokenIssuer::new(
        config.livekit_api_key.clone(),
        config.livekit_api_secret.clone(),
        config.voice_token_ttl_seconds,
    ));
    let voice_room_service = Arc::new(LiveKitRoomService::new(
        config.livekit_url.replace("wss://", "https://").replace("ws://", "http://"),
        config.livekit_api_key.clone(),
        config.livekit_api_secret.clone(),
    ));

    let active_voice_participants = Arc::new(DashMap::new());

    let state = AppState {
        config: Arc::new(config.clone()),
        threads,
        messages,
        channels,
        voice_rooms,
        identity_resolver,
        jwt_verifier,
        connections,
        voice_token_issuer,
        voice_room_service,
        active_voice_participants,
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/ws", get(ws_upgrade_handler))
        .merge(rest::router())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", config.port)).await?;
    tracing::info!("listening on {}", listener.local_addr()?);

    axum::serve(listener, app).await?;

    Ok(())
}
