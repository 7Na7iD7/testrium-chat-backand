alter table messages
  add column if not exists reply_to_message_id uuid references messages(message_id) on delete set null;

create index if not exists idx_messages_reply_to on messages(reply_to_message_id);
