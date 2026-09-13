use anyhow::Result;
use chat_domain::Thread;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct ThreadRepository {
    pool: PgPool,
}

impl ThreadRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// ترد بین یک استاد و دانشجوی مشخص را برمی‌گرداند؛ اگر وجود نداشت
    /// می‌سازد. یکتایی (professor_id, student_code) در سطح دیتابیس
    /// تضمین می‌شود، پس این عملیات با ON CONFLICT ایمن در برابر race
    /// (دو پیام هم‌زمان که هر دو سعی می‌کنند ترد را بسازند) است.
    ///
    /// نکته: عمداً `sqlx::query_as!` (نسخه‌ی چک‌شونده در زمان کامپایل)
    /// استفاده نشده، چون آن ماکرو برای بیلد نیاز به یک دیتابیس زنده و
    /// migrate‌شده در همان لحظه‌ی `cargo build` دارد. نسخه‌ی runtime
    /// (`query_as` + `FromRow`) اجازه می‌دهد CI/بیلد بدون دیتابیس هم
    /// موفق شود؛ خطای تایپ فقط در تست/اجرا دیده می‌شود نه در کامپایل —
    /// اگر بعداً `sqlx-cli` و offline cache (`cargo sqlx prepare`) به
    /// پروژه اضافه شد می‌توان به query_as! بازگشت.
    pub async fn get_or_create(
        &self,
        professor_id: &str,
        student_code: &str,
        section_id: Option<Uuid>,
    ) -> Result<Thread> {
        let row: ThreadRow = sqlx::query_as(
            r#"
            insert into threads (thread_id, professor_id, student_code, section_id, created_at)
            values (gen_random_uuid(), $1, $2, $3, now())
            on conflict (professor_id, student_code)
            do update set section_id = coalesce(threads.section_id, excluded.section_id)
            returning thread_id, professor_id, student_code, section_id, created_at,
                      last_message_at, last_message_preview,
                      unread_count_for_student, unread_count_for_professor
            "#,
        )
        .bind(professor_id)
        .bind(student_code)
        .bind(section_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.into())
    }

    /// نسخه‌ی امن‌تر `get_or_create`: اگر ترد از قبل بین این دو نفر
    /// وجود دارد همان را برمی‌گرداند (بدون هیچ round-trip اضافه به
    /// Supabase — یعنی این چک فقط یک‌بار، برای اولین پیامِ یک گفتگو،
    /// هزینه دارد). اگر وجود نداشت، ابتدا از طریق
    /// `IdentityResolver::verify_professor_student_link` از Supabase
    /// تایید می‌گیرد که این دانشجو واقعاً در section ای از این استاد
    /// enrolled است؛ در غیر این صورت `Ok(None)` برمی‌گرداند (نه Err،
    /// چون این یک خطای سیستمی نیست بلکه رد قانونی درخواست است) و لایه‌ی
    /// بالاتر (handler) پیام خطای مناسب را به کلاینت برمی‌گرداند.
    pub async fn get_or_create_if_linked(
        &self,
        professor_id: &str,
        student_code: &str,
        identity_resolver: &crate::auth::IdentityResolver,
    ) -> Result<Option<Thread>> {
        let row: Option<ThreadRow> = sqlx::query_as(
            r#"
            select thread_id, professor_id, student_code, section_id, created_at,
                   last_message_at, last_message_preview,
                   unread_count_for_student, unread_count_for_professor
            from threads where professor_id = $1 and student_code = $2
            "#,
        )
        .bind(professor_id)
        .bind(student_code)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            return Ok(Some(row.into()));
        }

        let linked = identity_resolver
            .verify_professor_student_link(professor_id, student_code)
            .await?;

        if !linked {
            return Ok(None);
        }

        Ok(Some(self.get_or_create(professor_id, student_code, None).await?))
    }

    pub async fn find_by_id(&self, thread_id: Uuid) -> Result<Option<Thread>> {
        let row: Option<ThreadRow> = sqlx::query_as(
            r#"
            select thread_id, professor_id, student_code, section_id, created_at,
                   last_message_at, last_message_preview,
                   unread_count_for_student, unread_count_for_professor
            from threads where thread_id = $1
            "#,
        )
        .bind(thread_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Into::into))
    }

    /// همه‌ی تردهای یک استاد، مرتب‌شده بر اساس آخرین پیام (برای صفحه‌ی
    /// لیست گفتگوهای پنل استاد).
    pub async fn list_for_professor(&self, professor_id: &str) -> Result<Vec<Thread>> {
        let rows: Vec<ThreadRow> = sqlx::query_as(
            r#"
            select thread_id, professor_id, student_code, section_id, created_at,
                   last_message_at, last_message_preview,
                   unread_count_for_student, unread_count_for_professor
            from threads
            where professor_id = $1
            order by coalesce(last_message_at, created_at) desc
            "#,
        )
        .bind(professor_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// همه‌ی تردهای یک دانشجو (با هر استادی که به او پیام داده یا او
    /// به آن استاد پیام داده است).
    pub async fn list_for_student(&self, student_code: &str) -> Result<Vec<Thread>> {
        let rows: Vec<ThreadRow> = sqlx::query_as(
            r#"
            select thread_id, professor_id, student_code, section_id, created_at,
                   last_message_at, last_message_preview,
                   unread_count_for_student, unread_count_for_professor
            from threads
            where student_code = $1
            order by coalesce(last_message_at, created_at) desc
            "#,
        )
        .bind(student_code)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn touch_last_message(
        &self,
        thread_id: Uuid,
        preview: &str,
        sent_by: chat_domain::Role,
    ) -> Result<()> {
        // بسته به این‌که فرستنده استاد است یا دانشجو، شمارنده‌ی
        // نخوانده‌ی طرف مقابل افزایش پیدا می‌کند.
        match sent_by {
            chat_domain::Role::Professor => {
                sqlx::query(
                    r#"
                    update threads
                    set last_message_at = now(),
                        last_message_preview = $2,
                        unread_count_for_student = unread_count_for_student + 1
                    where thread_id = $1
                    "#,
                )
                .bind(thread_id)
                .bind(preview)
                .execute(&self.pool)
                .await?;
            }
            chat_domain::Role::Student => {
                sqlx::query(
                    r#"
                    update threads
                    set last_message_at = now(),
                        last_message_preview = $2,
                        unread_count_for_professor = unread_count_for_professor + 1
                    where thread_id = $1
                    "#,
                )
                .bind(thread_id)
                .bind(preview)
                .execute(&self.pool)
                .await?;
            }
        }
        Ok(())
    }

    /// وقتی طرف مقابل ترد را باز می‌کند، شمارنده‌ی نخوانده‌ی خودش صفر
    /// می‌شود.
    pub async fn mark_read(&self, thread_id: Uuid, reader_role: chat_domain::Role) -> Result<()> {
        match reader_role {
            chat_domain::Role::Student => {
                sqlx::query("update threads set unread_count_for_student = 0 where thread_id = $1")
                    .bind(thread_id)
                    .execute(&self.pool)
                    .await?;
            }
            chat_domain::Role::Professor => {
                sqlx::query(
                    "update threads set unread_count_for_professor = 0 where thread_id = $1",
                )
                .bind(thread_id)
                .execute(&self.pool)
                .await?;
            }
        }
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct ThreadRow {
    thread_id: Uuid,
    professor_id: String,
    student_code: String,
    section_id: Option<Uuid>,
    created_at: chrono::DateTime<chrono::Utc>,
    last_message_at: Option<chrono::DateTime<chrono::Utc>>,
    last_message_preview: Option<String>,
    unread_count_for_student: i64,
    unread_count_for_professor: i64,
}

impl From<ThreadRow> for Thread {
    fn from(r: ThreadRow) -> Self {
        Thread {
            thread_id: r.thread_id,
            professor_id: r.professor_id,
            student_code: r.student_code,
            section_id: r.section_id,
            created_at: r.created_at,
            last_message_at: r.last_message_at,
            last_message_preview: r.last_message_preview,
            unread_count_for_student: r.unread_count_for_student,
            unread_count_for_professor: r.unread_count_for_professor,
        }
    }
}
