use anyhow::Result;
use chat_domain::{ChannelMessage, ChannelSummary, Role};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct ChannelRepository {
    pool: PgPool,
}

impl ChannelRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn ensure_channel(&self, channel_id: Uuid) -> Result<()> {
        sqlx::query("insert into channels (channel_id) values ($1) on conflict (channel_id) do nothing")
            .bind(channel_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn insert_message(
        &self,
        channel_id: Uuid,
        sender_role: Role,
        sender_id: &str,
        body: &str,
        reply_to_message_id: Option<Uuid>,
    ) -> Result<ChannelMessage> {
        self.ensure_channel(channel_id).await?;

        let row: ChannelMessageRow = sqlx::query_as(
            r#"
            insert into channel_messages (message_id, channel_id, sender_role, sender_id, body, reply_to_message_id, sent_at)
            values (gen_random_uuid(), $1, $2, $3, $4, $5, now())
            returning message_id, channel_id, sender_role, sender_id, body, reply_to_message_id, sent_at
            "#,
        )
        .bind(channel_id)
        .bind(sender_role.as_str())
        .bind(sender_id)
        .bind(body)
        .bind(reply_to_message_id)
        .fetch_one(&self.pool)
        .await?;

        sqlx::query(
            "update channels set last_message_at = $2, last_message_preview = $3 where channel_id = $1",
        )
        .bind(channel_id)
        .bind(row.sent_at)
        .bind(body)
        .execute(&self.pool)
        .await?;

        Ok(row.into())
    }

    pub async fn list_page(
        &self,
        channel_id: Uuid,
        before: Option<Uuid>,
        limit: i64,
    ) -> Result<Vec<ChannelMessage>> {
        let rows: Vec<ChannelMessageRow> = match before {
            Some(cursor) => {
                sqlx::query_as(
                    r#"
                    select message_id, channel_id, sender_role, sender_id, body, reply_to_message_id, sent_at
                    from channel_messages
                    where channel_id = $1
                      and sent_at < (select sent_at from channel_messages where message_id = $2)
                    order by sent_at desc
                    limit $3
                    "#,
                )
                .bind(channel_id)
                .bind(cursor)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as(
                    r#"
                    select message_id, channel_id, sender_role, sender_id, body, reply_to_message_id, sent_at
                    from channel_messages
                    where channel_id = $1
                    order by sent_at desc
                    limit $2
                    "#,
                )
                .bind(channel_id)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
        };

        let mut messages: Vec<ChannelMessage> = rows.into_iter().map(Into::into).collect();
        messages.reverse();
        Ok(messages)
    }

    pub async fn mark_read(&self, channel_id: Uuid, role: Role, member_id: &str) -> Result<()> {
        self.ensure_channel(channel_id).await?;

        sqlx::query(
            r#"
            insert into channel_reads (channel_id, member_role, member_id, last_read_at)
            values ($1, $2, $3, now())
            on conflict (channel_id, member_role, member_id)
            do update set last_read_at = now()
            "#,
        )
        .bind(channel_id)
        .bind(role.as_str())
        .bind(member_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_summaries(
        &self,
        channel_ids: &[Uuid],
        role: Role,
        member_id: &str,
    ) -> Result<Vec<ChannelSummary>> {
        if channel_ids.is_empty() {
            return Ok(vec![]);
        }

        let rows: Vec<ChannelSummaryRow> = sqlx::query_as(
            r#"
            select
                s.id as channel_id,
                c.last_message_at,
                c.last_message_preview,
                coalesce(cnt.unread_count, 0) as unread_count
            from unnest($1::uuid[]) as s(id)
            left join channels c on c.channel_id = s.id
            left join lateral (
                select count(*) as unread_count
                from channel_messages cm
                left join channel_reads cr
                  on cr.channel_id = cm.channel_id
                 and cr.member_role = $2
                 and cr.member_id = $3
                where cm.channel_id = s.id
                  and cm.sent_at > coalesce(cr.last_read_at, '-infinity'::timestamptz)
                  and not (cm.sender_role = $2 and cm.sender_id = $3)
            ) cnt on true
            order by coalesce(c.last_message_at, '-infinity'::timestamptz) desc
            "#,
        )
        .bind(channel_ids)
        .bind(role.as_str())
        .bind(member_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }
}

#[derive(sqlx::FromRow)]
struct ChannelMessageRow {
    message_id: Uuid,
    channel_id: Uuid,
    sender_role: String,
    sender_id: String,
    body: String,
    reply_to_message_id: Option<Uuid>,
    sent_at: chrono::DateTime<chrono::Utc>,
}

impl From<ChannelMessageRow> for ChannelMessage {
    fn from(r: ChannelMessageRow) -> Self {
        ChannelMessage {
            message_id: r.message_id,
            channel_id: r.channel_id,
            sender_role: if r.sender_role == "professor" {
                Role::Professor
            } else {
                Role::Student
            },
            sender_id: r.sender_id,
            body: r.body,
            reply_to_message_id: r.reply_to_message_id,
            sent_at: r.sent_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ChannelSummaryRow {
    channel_id: Uuid,
    last_message_at: Option<chrono::DateTime<chrono::Utc>>,
    last_message_preview: Option<String>,
    unread_count: i64,
}

impl From<ChannelSummaryRow> for ChannelSummary {
    fn from(r: ChannelSummaryRow) -> Self {
        ChannelSummary {
            channel_id: r.channel_id,
            last_message_at: r.last_message_at,
            last_message_preview: r.last_message_preview,
            unread_count: r.unread_count,
        }
    }
}
