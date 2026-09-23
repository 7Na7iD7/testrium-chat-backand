use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use aws_sdk_s3::config::{BehaviorVersion, Builder as S3ConfigBuilder, Credentials, Region};
use aws_sdk_s3::Client as S3Client;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct LiveKitRoomService {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    api_secret: String,
}

#[derive(Debug, Clone)]
pub struct EgressS3Output {
    pub bucket: String,
    pub region: String,
    pub endpoint: String,
    pub access_key: String,
    pub secret_key: String,
}

impl EgressS3Output {
    fn s3_client(&self) -> S3Client {
        let credentials = Credentials::new(
            self.access_key.clone(),
            self.secret_key.clone(),
            None,
            None,
            "testrium-egress",
        );

        let config = S3ConfigBuilder::new()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new(self.region.clone()))
            .endpoint_url(self.endpoint.clone())
            .credentials_provider(credentials)
            .force_path_style(true)
            .build();

        S3Client::from_conf(config)
    }

    pub fn object_key_from_location(&self, location: &str) -> String {
        if let Some(idx) = location.find(&self.bucket) {
            let after_bucket = &location[idx + self.bucket.len()..];
            return after_bucket.trim_start_matches('/').to_string();
        }
        location.trim_start_matches('/').to_string()
    }

    pub async fn delete_object(&self, key: &str) -> Result<()> {
        self.s3_client()
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| anyhow!("حذف فایل از storage ناموفق بود: {e}"))?;
        Ok(())
    }
}

#[derive(Debug, Serialize)]
struct AdminGrant {
    room_admin: bool,
    room: String,
}

#[derive(Debug, Serialize)]
struct AdminClaims {
    iss: String,
    sub: String,
    exp: usize,
    nbf: usize,
    video: AdminGrant,
}

#[derive(Debug, Serialize)]
struct EgressGrant {
    room_record: bool,
    room: String,
}

#[derive(Debug, Serialize)]
struct EgressClaims {
    iss: String,
    sub: String,
    exp: usize,
    nbf: usize,
    video: EgressGrant,
}

#[derive(Debug, Deserialize)]
pub struct LiveKitParticipant {
    pub identity: String,
    pub name: String,
    pub state: String,
}

impl LiveKitRoomService {
    pub fn new(base_url: String, api_key: String, api_secret: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url,
            api_key,
            api_secret,
        }
    }

    fn admin_token(&self, room_name: &str) -> Result<String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| anyhow!("زمان سیستم نامعتبر است: {e}"))?
            .as_secs() as usize;

        let claims = AdminClaims {
            iss: self.api_key.clone(),
            sub: "chat-server".to_string(),
            exp: now + 60,
            nbf: now.saturating_sub(5),
            video: AdminGrant {
                room_admin: true,
                room: room_name.to_string(),
            },
        };

        let header = Header::new(Algorithm::HS256);
        let key = EncodingKey::from_secret(self.api_secret.as_bytes());
        encode(&header, &claims, &key).map_err(|e| anyhow!("امضای توکن ادمین ناموفق بود: {e}"))
    }

    fn egress_token(&self, room_name: &str) -> Result<String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| anyhow!("زمان سیستم نامعتبر است: {e}"))?
            .as_secs() as usize;

        let claims = EgressClaims {
            iss: self.api_key.clone(),
            sub: "chat-server".to_string(),
            exp: now + 60,
            nbf: now.saturating_sub(5),
            video: EgressGrant {
                room_record: true,
                room: room_name.to_string(),
            },
        };

        let header = Header::new(Algorithm::HS256);
        let key = EncodingKey::from_secret(self.api_secret.as_bytes());
        encode(&header, &claims, &key).map_err(|e| anyhow!("امضای توکن egress ناموفق بود: {e}"))
    }

    async fn post(&self, path: &str, room_name: &str, body: serde_json::Value) -> Result<reqwest::Response> {
        let token = self.admin_token(room_name)?;
        let url = format!("{}{}", self.base_url, path);

        self.http
            .post(url)
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow!("فراخوانی LiveKit ناموفق بود: {e}"))
    }

    async fn post_egress(&self, path: &str, room_name: &str, body: serde_json::Value) -> Result<reqwest::Response> {
        let token = self.egress_token(room_name)?;
        let url = format!("{}{}", self.base_url, path);

        self.http
            .post(url)
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow!("فراخوانی LiveKit Egress ناموفق بود: {e}"))
    }

    pub async fn mute_published_track(
        &self,
        room_name: &str,
        participant_identity: &str,
        track_sid: &str,
        muted: bool,
    ) -> Result<()> {
        self.post(
            "/twirp/livekit.RoomService/MutePublishedTrack",
            room_name,
            serde_json::json!({
                "room": room_name,
                "identity": participant_identity,
                "track_sid": track_sid,
                "muted": muted,
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn update_participant(
        &self,
        room_name: &str,
        participant_identity: &str,
        can_publish: bool,
    ) -> Result<()> {
        self.post(
            "/twirp/livekit.RoomService/UpdateParticipant",
            room_name,
            serde_json::json!({
                "room": room_name,
                "identity": participant_identity,
                "permission": {
                    "can_subscribe": true,
                    "can_publish": can_publish,
                    "can_publish_data": true,
                },
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn remove_participant(&self, room_name: &str, participant_identity: &str) -> Result<()> {
        self.post(
            "/twirp/livekit.RoomService/RemoveParticipant",
            room_name,
            serde_json::json!({
                "room": room_name,
                "identity": participant_identity,
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn list_participants(&self, room_name: &str) -> Result<Vec<LiveKitParticipant>> {
        let response = self
            .post(
                "/twirp/livekit.RoomService/ListParticipants",
                room_name,
                serde_json::json!({ "room": room_name }),
            )
            .await?;

        #[derive(Deserialize)]
        struct ListResponse {
            participants: Vec<LiveKitParticipant>,
        }

        let body: ListResponse = response
            .json()
            .await
            .map_err(|e| anyhow!("پاسخ نامعتبر از LiveKit: {e}"))?;

        Ok(body.participants)
    }

    pub async fn delete_room(&self, room_name: &str) -> Result<()> {
        self.post(
            "/twirp/livekit.RoomService/DeleteRoom",
            room_name,
            serde_json::json!({ "room": room_name }),
        )
        .await?;
        Ok(())
    }

    pub async fn start_room_composite_egress(
        &self,
        room_name: &str,
        output: &EgressS3Output,
    ) -> Result<String> {
        let filepath = format!(
            "voice-recordings/{room_name}-{}.ogg",
            chrono::Utc::now().timestamp()
        );

        let response = self
            .post_egress(
                "/twirp/livekit.Egress/StartRoomCompositeEgress",
                room_name,
                serde_json::json!({
                    "room_name": room_name,
                    "audio_only": true,
                    "file_outputs": [{
                        "file_type": "OGG",
                        "filepath": filepath,
                        "s3": {
                            "access_key": output.access_key,
                            "secret": output.secret_key,
                            "bucket": output.bucket,
                            "region": output.region,
                            "endpoint": output.endpoint,
                        }
                    }]
                }),
            )
            .await?;

        #[derive(Deserialize)]
        struct StartResponse {
            egress_id: String,
        }

        let body: StartResponse = response
            .json()
            .await
            .map_err(|e| anyhow!("پاسخ نامعتبر از LiveKit Egress: {e}"))?;

        Ok(body.egress_id)
    }

    pub async fn stop_egress(&self, room_name: &str, egress_id: &str) -> Result<Option<String>> {
        let response = self
            .post_egress(
                "/twirp/livekit.Egress/StopEgress",
                room_name,
                serde_json::json!({ "egress_id": egress_id }),
            )
            .await?;

        #[derive(Deserialize)]
        struct FileResult {
            filename: Option<String>,
            location: Option<String>,
        }

        #[derive(Deserialize)]
        struct StopResponse {
            #[serde(default)]
            file_results: Vec<FileResult>,
        }

        let body: StopResponse = response
            .json()
            .await
            .map_err(|e| anyhow!("پاسخ نامعتبر از LiveKit Egress: {e}"))?;

        Ok(body
            .file_results
            .into_iter()
            .next()
            .and_then(|f| f.location.or(f.filename)))
    }
}
