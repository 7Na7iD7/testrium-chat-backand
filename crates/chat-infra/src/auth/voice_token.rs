use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use chat_domain::VoiceGrants;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct LiveKitTokenIssuer {
    api_key: String,
    api_secret: String,
    ttl_seconds: i64,
}

#[derive(Debug, Serialize)]
struct VideoGrant {
    room: String,
    room_join: bool,
    can_publish: bool,
    can_publish_data: bool,
    can_subscribe: bool,
    room_admin: bool,
}

#[derive(Debug, Serialize)]
struct LiveKitClaims {
    iss: String,
    sub: String,
    name: String,
    exp: usize,
    nbf: usize,
    video: VideoGrant,
}

impl LiveKitTokenIssuer {
    pub fn new(api_key: String, api_secret: String, ttl_seconds: i64) -> Self {
        Self {
            api_key,
            api_secret,
            ttl_seconds,
        }
    }

    pub fn issue(
        &self,
        room_name: &str,
        identity: &str,
        display_name: &str,
        grants: &VoiceGrants,
    ) -> Result<String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| anyhow!("زمان سیستم نامعتبر است: {e}"))?
            .as_secs() as usize;

        let claims = LiveKitClaims {
            iss: self.api_key.clone(),
            sub: identity.to_string(),
            name: display_name.to_string(),
            exp: now + self.ttl_seconds as usize,
            nbf: now.saturating_sub(5),
            video: VideoGrant {
                room: room_name.to_string(),
                room_join: grants.can_join,
                can_publish: grants.can_publish,
                can_publish_data: grants.can_publish_data,
                can_subscribe: grants.can_subscribe,
                room_admin: grants.is_room_admin,
            },
        };

        let header = Header::new(Algorithm::HS256);
        let key = EncodingKey::from_secret(self.api_secret.as_bytes());

        encode(&header, &claims, &key).map_err(|e| anyhow!("امضای توکن ناموفق بود: {e}"))
    }
}
