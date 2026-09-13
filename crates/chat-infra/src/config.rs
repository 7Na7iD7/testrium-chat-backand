use anyhow::{Context, Result};

use crate::livekit::EgressS3Output;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub legacy_jwt_secret: Option<String>,
    pub supabase_url: String,
    pub supabase_service_role_key: String,
    pub chat_server_shared_secret: String,
    pub port: u16,
    pub max_message_length: usize,
    pub livekit_url: String,
    pub livekit_api_key: String,
    pub livekit_api_secret: String,
    pub voice_token_ttl_seconds: i64,
    pub voice_max_participants: usize,
    pub voice_attendance_threshold_seconds: i64,
    pub egress_s3_bucket: String,
    pub egress_s3_region: String,
    pub egress_s3_endpoint: String,
    pub egress_s3_access_key: String,
    pub egress_s3_secret_key: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            database_url: std::env::var("CHAT_DATABASE_URL")
                .context("CHAT_DATABASE_URL تنظیم نشده")?,
            legacy_jwt_secret: std::env::var("SUPABASE_JWT_SECRET").ok(),
            supabase_url: std::env::var("SUPABASE_URL").context("SUPABASE_URL تنظیم نشده")?,
            supabase_service_role_key: std::env::var("SUPABASE_SERVICE_ROLE_KEY")
                .context("SUPABASE_SERVICE_ROLE_KEY تنظیم نشده")?,
            chat_server_shared_secret: std::env::var("CHAT_SERVER_SHARED_SECRET")
                .context("CHAT_SERVER_SHARED_SECRET تنظیم نشده")?,
            port: std::env::var("CHAT_PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .context("CHAT_PORT باید عدد باشد")?,
            max_message_length: std::env::var("CHAT_MAX_MESSAGE_LENGTH")
                .unwrap_or_else(|_| "4000".to_string())
                .parse()
                .context("CHAT_MAX_MESSAGE_LENGTH باید عدد باشد")?,
            livekit_url: std::env::var("LIVEKIT_URL").context("LIVEKIT_URL تنظیم نشده")?,
            livekit_api_key: std::env::var("LIVEKIT_API_KEY")
                .context("LIVEKIT_API_KEY تنظیم نشده")?,
            livekit_api_secret: std::env::var("LIVEKIT_API_SECRET")
                .context("LIVEKIT_API_SECRET تنظیم نشده")?,
            voice_token_ttl_seconds: std::env::var("VOICE_TOKEN_TTL_SECONDS")
                .unwrap_or_else(|_| "21600".to_string())
                .parse()
                .context("VOICE_TOKEN_TTL_SECONDS باید عدد باشد")?,
            voice_max_participants: std::env::var("VOICE_MAX_PARTICIPANTS")
                .unwrap_or_else(|_| "50".to_string())
                .parse()
                .context("VOICE_MAX_PARTICIPANTS باید عدد باشد")?,
            voice_attendance_threshold_seconds: std::env::var("VOICE_ATTENDANCE_THRESHOLD_SECONDS")
                .unwrap_or_else(|_| "600".to_string())
                .parse()
                .context("VOICE_ATTENDANCE_THRESHOLD_SECONDS باید عدد باشد")?,
            egress_s3_bucket: std::env::var("EGRESS_S3_BUCKET")
                .context("EGRESS_S3_BUCKET تنظیم نشده")?,
            egress_s3_region: std::env::var("EGRESS_S3_REGION")
                .unwrap_or_else(|_| "auto".to_string()),
            egress_s3_endpoint: std::env::var("EGRESS_S3_ENDPOINT")
                .context("EGRESS_S3_ENDPOINT تنظیم نشده")?,
            egress_s3_access_key: std::env::var("EGRESS_S3_ACCESS_KEY")
                .context("EGRESS_S3_ACCESS_KEY تنظیم نشده")?,
            egress_s3_secret_key: std::env::var("EGRESS_S3_SECRET_KEY")
                .context("EGRESS_S3_SECRET_KEY تنظیم نشده")?,
        })
    }

    pub fn egress_s3_output(&self) -> EgressS3Output {
        EgressS3Output {
            bucket: self.egress_s3_bucket.clone(),
            region: self.egress_s3_region.clone(),
            endpoint: self.egress_s3_endpoint.clone(),
            access_key: self.egress_s3_access_key.clone(),
            secret_key: self.egress_s3_secret_key.clone(),
        }
    }
}
