use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use chat_domain::{Identity, Role};
use dashmap::DashMap;
use serde::Deserialize;

use super::jwt::SupabaseClaims;

const CACHE_TTL: Duration = Duration::from_secs(300);

struct CacheEntry {
    identity: Identity,
    cached_at: Instant,
}

/// اطلاعاتی که از این پس `verify-code` مستقیماً در `app_metadata`
/// (و در نتیجه در JWT) کاربر می‌نویسد. این نوع کاملاً داخلی همین
/// ماژول است — فقط `resolve_for_claims` آن را می‌سازد و مصرف می‌کند.
#[derive(Debug, Clone, Deserialize)]
struct AppMetadataClaims {
    role: String,
    user_code: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    accessible_section_ids: Vec<String>,
}

/// نقش/کد واقعی کاربر (student_code یا professor_id + section هایی که
/// بهشان دسترسی دارد) در جداول students/professors/sections/enrollments
/// روی Supabase است، نه در JWT. این resolver با یک Edge Function سبک
/// (`chat-identity`، جدا از `verify-code`) که با service_role اجرا
/// می‌شود، auth_user_id را resolve می‌کند و نتیجه را با TTL کوتاه کش
/// می‌کند تا هر پیام یک HTTP round-trip اضافه به Supabase نزند.
///
/// به‌روزرسانی: از این پس `verify-code`/`sync-user-claims` این اطلاعات
/// را مستقیماً در `app_metadata` (و در نتیجه در JWT هر کاربر) می‌نویسند.
/// `resolve_for_claims` این حالت را همیشه اول امتحان می‌کند — بدون هیچ
/// HTTP call یا کش، چون خودِ JWT (کوتاه‌عمر و هر بار تازه) نقش کش را
/// بازی می‌کند. فقط اگر توکن این claim ها را نداشت (کاربری با session
/// قدیمی، پیش‌ از این تغییر)، به مسیر قدیمی (`resolve` + چت-آیدنتیتی)
/// برمی‌گردد.
pub struct IdentityResolver {
    http: reqwest::Client,
    supabase_url: String,
    shared_secret: String,
    cache: DashMap<String, CacheEntry>,
}

#[derive(Debug, Deserialize)]
struct ChatIdentityResponse {
    ok: bool,
    role: Option<String>,
    identifier: Option<String>,
    display_name: Option<String>,
    accessible_section_ids: Option<Vec<String>>,
    message: Option<String>,
}

impl IdentityResolver {
    pub fn new(supabase_url: String, shared_secret: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            supabase_url,
            shared_secret,
            cache: DashMap::new(),
        }
    }

    /// نقطه‌ی ورود پیشنهادی برای هر دو مسیر REST و WebSocket: هم مسیر
    /// تازه‌ی بدون-تأخیر (JWT claims) و هم fallback قدیمی را خودش
    /// امتحان می‌کند — کافیست extractor.rs/ws/handler.rs فقط این یکی
    /// را صدا بزنند، به‌جای این‌که هرکدام جدا claim ها را parse کنند.
    pub async fn resolve_for_claims(&self, claims: &SupabaseClaims) -> Result<Identity> {
        if let Ok(app_claims) =
            serde_json::from_value::<AppMetadataClaims>(claims.app_metadata.clone())
        {
            if !app_claims.role.is_empty() && !app_claims.user_code.is_empty() {
                return self.build_identity_from_claims(&claims.sub, &app_claims);
            }
        }

        self.resolve(&claims.sub).await
    }

    fn build_identity_from_claims(
        &self,
        auth_user_id: &str,
        claims: &AppMetadataClaims,
    ) -> Result<Identity> {
        let role = match claims.role.as_str() {
            "student" => Role::Student,
            "professor" => Role::Professor,
            other => return Err(anyhow!("نقش نامعتبر در JWT: {other}")),
        };

        Ok(Identity {
            auth_user_id: auth_user_id.to_string(),
            role,
            identifier: claims.user_code.clone(),
            display_name: claims.display_name.clone(),
            accessible_section_ids: claims.accessible_section_ids.clone(),
        })
    }

    /// مسیر fallback (قدیمی): وقتی JWT هنوز claim های بالا را ندارد
    /// (کاربری که از قبل session باز داشته و هنوز رفرش نشده). همان
    /// رفتار قبلی — صدا زدن `chat-identity` با کش کوتاه‌مدت.
    pub async fn resolve(&self, auth_user_id: &str) -> Result<Identity> {
        if let Some(entry) = self.cache.get(auth_user_id) {
            if entry.cached_at.elapsed() < CACHE_TTL {
                return Ok(entry.identity.clone());
            }
        }

        let identity = self.fetch_from_supabase(auth_user_id).await?;

        self.cache.insert(
            auth_user_id.to_string(),
            CacheEntry {
                identity: identity.clone(),
                cached_at: Instant::now(),
            },
        );

        Ok(identity)
    }

    /// وقتی نقش/دسترسی کاربر تغییر می‌کند (مثلاً استاد section جدیدی
    /// می‌گیرد)، برای جلوگیری از تا ۵ دقیقه داده‌ی بات، این تابع کش را
    /// برای یک کاربر خاص باطل می‌کند. فقط مسیر fallback (`resolve`) از
    /// این کش استفاده می‌کند؛ مسیر اصلی (`resolve_for_claims` وقتی
    /// claim دارد) اصلاً کش ندارد که باطل شود.
    pub fn invalidate(&self, auth_user_id: &str) {
        self.cache.remove(auth_user_id);
    }

    /// قبل از ساخت اولین ترد بین یک استاد و دانشجو، باید تایید شود که
    /// این دو واقعاً حداقل یک section مشترک دارند (دانشجو در section ی
    /// از استاد enrolled است) — این چک نمی‌تواند فقط سمت کلاینت باشد
    /// چون کلاینت (مخصوصاً استاد) می‌تواند هر student_code دلخواهی در
    /// SendMessage بفرستد. جدا از `chat-identity` نگه داشته شده چون آن
    /// تابع فقط «خودِ» کاربر متصل را resolve می‌کند، نه یک جفت
    /// دلخواه — عمداً یک Edge Function جدا (`chat-verify-link`) با
    /// امضای متفاوت.
    pub async fn verify_professor_student_link(
        &self,
        professor_id: &str,
        student_code: &str,
    ) -> Result<bool> {
        let url = format!("{}/functions/v1/chat-verify-link", self.supabase_url);

        let response = self
            .http
            .post(&url)
            .bearer_auth(&self.shared_secret)
            .json(&serde_json::json!({
                "professor_id": professor_id,
                "student_code": student_code,
            }))
            .send()
            .await
            .map_err(|e| anyhow!("خطا در ارتباط با Supabase برای تایید ارتباط: {e}"))?;

        #[derive(Deserialize)]
        struct LinkResponse {
            linked: bool,
        }

        let body: LinkResponse = response
            .json()
            .await
            .map_err(|e| anyhow!("پاسخ نامعتبر از chat-verify-link: {e}"))?;

        Ok(body.linked)
    }

    async fn fetch_from_supabase(&self, auth_user_id: &str) -> Result<Identity> {
        let url = format!("{}/functions/v1/chat-identity", self.supabase_url);

        let response = self
            .http
            .post(&url)
            .bearer_auth(&self.shared_secret)
            .json(&serde_json::json!({ "auth_user_id": auth_user_id }))
            .send()
            .await
            .map_err(|e| anyhow!("خطا در ارتباط با Supabase برای resolve هویت: {e}"))?;

        let body: ChatIdentityResponse = response
            .json()
            .await
            .map_err(|e| anyhow!("پاسخ نامعتبر از chat-identity: {e}"))?;

        if !body.ok {
            return Err(anyhow!(
                "resolve هویت ناموفق بود: {}",
                body.message.unwrap_or_else(|| "نامشخص".to_string())
            ));
        }

        let role = match body.role.as_deref() {
            Some("student") => Role::Student,
            Some("professor") => Role::Professor,
            _ => return Err(anyhow!("نقش کاربر نامعتبر است")),
        };

        Ok(Identity {
            auth_user_id: auth_user_id.to_string(),
            role,
            identifier: body
                .identifier
                .ok_or_else(|| anyhow!("identifier برنگشت"))?,
            display_name: body.display_name.unwrap_or_default(),
            accessible_section_ids: body.accessible_section_ids.unwrap_or_default(),
        })
    }
}

