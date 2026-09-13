use std::sync::RwLock;
use std::time::{Duration, Instant};

use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// زیرمجموعه‌ی claim هایی که Supabase در GoTrue JWT امضا می‌کند و
/// این سرور به آن نیاز دارد. فیلدهای دیگری هم توی توکن هست
/// (aal, session_id, ...) که چون استفاده نمی‌شوند این‌جا مدل نشده‌اند؛
/// serde به‌طور پیش‌فرض فیلدهای اضافه را نادیده می‌گیرد.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupabaseClaims {
    /// auth.users.id — همان چیزی که در ستون `auth_user_id` جدول
    /// students/professors ذخیره شده است.
    pub sub: String,
    pub exp: usize,
    pub role: String,
    /// از این پس `verify-code`/`sync-user-claims` مستقیماً
    /// role/user_code/display_name/accessible_section_ids را این‌جا
    /// می‌نویسند (نگاه کن به identity_resolver.rs::resolve_for_claims).
    /// اگر توکن قدیمی باشد و این فیلد را نداشته باشد، serde آن را
    /// `Value::Null` می‌گذارد، نه خطا.
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

/// ── پس‌زمینه‌ی این بازنویسی ──
/// پروژه‌های Supabase که به سیستم جدید «JWT Signing Keys» مهاجرت
/// کرده‌اند (Project Settings → API → JWT Keys)، دیگر توکن‌های session
/// را با HS256 + یک secret مشترک ثابت امضا نمی‌کنند؛ به‌جایش از یک
/// جفت‌کلید نامتقارن (پیش‌فرض ECC P-256 → الگوریتم ES256) استفاده
/// می‌کنند. «Legacy JWT Secret» فقط برای verify توکن‌های *قدیمی* که
/// هنوز منقضی نشده‌اند نگه داشته می‌شود (طبق پیام خودِ Supabase در
/// داشبورد: "used only to verify JWTs"). یعنی هر توکن جدیدی که از این
/// به بعد صادر می‌شود را verify_supabase_jwt نسخه‌ی قبلی (فقط HS256)
/// همیشه رد می‌کرد؛ که دقیقاً همان خطای مشاهده‌شده
/// (`ws_upgrade_rejected: invalid jwt`) است.
///
/// راه‌حل این نسخه: کلیدهای عمومی پروژه از اندپوینت JWKS استاندارد
/// Supabase گرفته می‌شوند، بر اساس `kid` هدر توکن کلید درست انتخاب و
/// امضا با همان الگوریتمی که در JWKS اعلام شده (مثلاً ES256) تایید
/// می‌شود. برای عبور نرم از دوره‌ی گذار، اگر kid توکن در JWKS پیدا
/// نشد ولی یک Legacy Secret HS256 تنظیم شده باشد، به‌عنوان fallback
/// با آن هم امتحان می‌شود (پوشش توکن‌های خیلی قدیمی صادرشده قبل از
/// rotation که هنوز منقضی نشده‌اند).
pub struct JwtVerifier {
    supabase_url: String,
    /// اختیاری — اگر ست نشود فقط JWKS/ES256 امتحان می‌شود.
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

    /// توکن را تایید می‌کند. ابتدا هدر را (بدون تایید امضا) می‌خواند
    /// تا `kid` و `alg` را ببیند، سپس کلید مربوطه را در JWKS (با کش
    /// یک‌ساعته) پیدا و امضا را تایید می‌کند. اگر kid در JWKS نبود و
    /// Legacy Secret تنظیم شده، به‌عنوان مسیر دوم HS256 هم امتحان
    /// می‌شود.
    pub async fn verify(&self, token: &str) -> Result<SupabaseClaims, JwtError> {
        let header = decode_header(token).map_err(|_| JwtError::InvalidToken)?;

        // مسیر اول: JWKS (کلیدهای جدید، ES256/RS256 و غیره)
        let jwks_result = self.verify_with_jwks(token, &header).await;
        if jwks_result.is_ok() {
            return jwks_result;
        }

        // مسیر دوم (fallback): Legacy HS256 Secret، فقط اگر تنظیم شده
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

        // اگر kid پیدا نشد، شاید کلید همین تازه rotate شده — یک‌بار
        // کش را force-refresh کن قبل از تسلیم شدن.
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
