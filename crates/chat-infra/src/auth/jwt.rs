use std::sync::RwLock;
use std::time::{Duration, Instant};

use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupabaseClaims {
    pub sub: String,
    pub exp: usize,
    pub role: String,
    #[serde(default)]
    pub app_metadata: serde_json::Value,
}

#[derive(Debug, Error)]
pub enum JwtError {
    #[error("توکن نامعتبر یا منقضی‌شده است")]
    InvalidToken,
    #[error("خطا در دریافت کلیدهای عمومی Supabase (JWKS): {0}")]
    JwksFetchFailed(String),
    #[error("کلید مطابق با kid توکن در JWKS یافت نشد")]
    KeyNotFound,
}
pub struct JwtVerifier {
    supabase_url: String,
    legacy_hs256_secret: Option<String>,
    http: reqwest::Client,
    cache: RwLock<Option<(JwkSet, Instant)>>,
}

const JWKS_CACHE_TTL: Duration = Duration::from_secs(3600);

impl JwtVerifier {
    pub fn new(supabase_url: String, legacy_hs256_secret: Option<String>) -> Self {
        Self {
            supabase_url,
            legacy_hs256_secret,
            http: reqwest::Client::new(),
            cache: RwLock::new(None),
        }
    }

    fn jwks_url(&self) -> String {
        format!(
            "{}/auth/v1/.well-known/jwks.json",
            self.supabase_url.trim_end_matches('/')
        )
    }

    async fn get_jwks(&self, force_refresh: bool) -> Result<JwkSet, JwtError> {
        if !force_refresh {
            if let Some((set, fetched_at)) = self.cache.read().unwrap().clone() {
                if fetched_at.elapsed() < JWKS_CACHE_TTL {
                    return Ok(set);
                }
            }
        }

        let response = self
            .http
            .get(self.jwks_url())
            .send()
            .await
            .map_err(|e| JwtError::JwksFetchFailed(e.to_string()))?;

        let jwks: JwkSet = response
            .json()
            .await
            .map_err(|e| JwtError::JwksFetchFailed(e.to_string()))?;

        *self.cache.write().unwrap() = Some((jwks.clone(), Instant::now()));
        Ok(jwks)
    }

    pub async fn verify(&self, token: &str) -> Result<SupabaseClaims, JwtError> {
        let header = decode_header(token).map_err(|_| JwtError::InvalidToken)?;

        let jwks_result = self.verify_with_jwks(token, &header).await;
        if jwks_result.is_ok() {
            return jwks_result;
        }

        if let Some(secret) = &self.legacy_hs256_secret {
            let mut validation = Validation::new(Algorithm::HS256);
            validation.validate_aud = false;
            if let Ok(data) = decode::<SupabaseClaims>(
                token,
                &DecodingKey::from_secret(secret.as_bytes()),
                &validation,
            ) {
                return Ok(data.claims);
            }
        }

        jwks_result
    }

    async fn verify_with_jwks(
        &self,
        token: &str,
        header: &jsonwebtoken::Header,
    ) -> Result<SupabaseClaims, JwtError> {
        let mut jwks = self.get_jwks(false).await?;

        let mut jwk = header
            .kid
            .as_ref()
            .and_then(|kid| jwks.find(kid).cloned());
        
        if jwk.is_none() {
            jwks = self.get_jwks(true).await?;
            jwk = header.kid.as_ref().and_then(|kid| jwks.find(kid).cloned());
        }

        let jwk = jwk.ok_or(JwtError::KeyNotFound)?;

        let decoding_key =
            DecodingKey::from_jwk(&jwk).map_err(|_| JwtError::InvalidToken)?;

        let mut validation = Validation::new(header.alg);
        validation.validate_aud = false;

        decode::<SupabaseClaims>(token, &decoding_key, &validation)
            .map(|data| data.claims)
            .map_err(|_| JwtError::InvalidToken)
    }
}
