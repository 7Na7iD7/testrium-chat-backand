use anyhow::Result;
use chat_domain::{Message, Role};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct MessageRepository {
    pool: PgPool,
}

impl MessageRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn insert(
        &self,
        thread_id: Uuid,
        sender_role: Role,
        sender_id: &str,
        body: &str,
        reply_to_message_id: Option<Uuid>,
    ) -> Result<Message> {
        let row: MessageRow = sqlx::query_as(
            r#"
            insert into messages (message_id, thread_id, sender_role, sender_id, body, reply_to_message_id, sent_at)
            values (gen_random_uuid(), $1, $2, $3, $4, $5, now())
            returning message_id, thread_id, sender_role, sender_id, body, reply_to_message_id, sent_at,
                      delivered_at, read_at
            "#,
        )
        .bind(thread_id)
        .bind(sender_role.as_str())
        .bind(sender_id)
        .bind(body)
        .bind(reply_to_message_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.into())
    }

    pub async fn list_page(
        &self,
        thread_id: Uuid,
        before: Option<Uuid>,
        limit: i64,
    ) -> Result<Vec<Message>> {
        let rows: Vec<MessageRow> = match before {
            Some(cursor) => {
                sqlx::query_as(
                    r#"
                    select message_id, thread_id, sender_role, sender_id, body, reply_to_message_id, sent_at,
                           delivered_at, read_at
                    from messages
                    where thread_id = $1
                      and sent_at < (select sent_at from messages where message_id = $2)
                    order by sent_at desc
                    limit $3
                    "#,
                )
                .bind(thread_id)
                .bind(cursor)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as(
                    r#"
                    select message_id, thread_id, sender_role, sender_id, body, reply_to_message_id, sent_at,
                           delivered_at, read_at
                    from messages
                    where thread_id = $1
                    order by sent_at desc
                    limit $2
                    "#,
                )
                .bind(thread_id)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
        };

        let mut messages: Vec<Message> = rows.into_iter().map(Into::into).collect();
        messages.reverse();
        Ok(messages)
    }

    pub async fn mark_delivered(&self, message_ids: &[Uuid]) -> Result<()> {
        if message_ids.is_empty() {
            return Ok(());
        }
        sqlx::query(
            "update messages set delivered_at = now() where message_id = any($1) and delivered_at is null",
        )
        .bind(message_ids)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn mark_thread_read(&self, thread_id: Uuid, reader_role: Role) -> Result<Vec<Uuid>> {
        let sender_role_to_mark = match reader_role {
            Role::Student => Role::Professor,
            Role::Professor => Role::Student,
        };

        let rows: Vec<(Uuid,)> = sqlx::query_as(
            r#"
            update messages
            set read_at = now()
            where thread_id = $1
              and sender_role = $2
              and read_at is null
            returning message_id
            "#,
        )
        .bind(thread_id)
        .bind(sender_role_to_mark.as_str())
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|(id,)| id).collect())
    }
}

#[derive(sqlx::FromRow)]
struct MessageRow {
    message_id: Uuid,
    thread_id: Uuid,
    sender_role: String,
    sender_id: String,
    body: String,
    reply_to_message_id: Option<Uuid>,
    sent_at: chrono::DateTime<chrono::Utc>,
    delivered_at: Option<chrono::DateTime<chrono::Utc>>,
    read_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<MessageRow> for Message {
    fn from(r: MessageRow) -> Self {
        Message {
            message_id: r.message_id,
            thread_id: r.thread_id,
            sender_role: if r.sender_role == "professor" {
                Role::Professor
            } else {
                Role::Student
            },
            sender_id: r.sender_id,
            body: r.body,
            reply_to_message_id: r.reply_to_message_id,
            sent_at: r.sent_at,
            delivered_at: r.delivered_at,
            read_at: r.read_at,
        }
    }
}
