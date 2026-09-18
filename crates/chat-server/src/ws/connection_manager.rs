use std::time::Instant;

use dashmap::DashMap;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::ws::protocol::ServerEvent;

pub type ConnectionKey = (chat_domain::Role, String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum RateLimitKind {
    General,
    VoiceControl,
}

#[derive(Debug, Clone, Copy)]
struct Bucket {
    tokens: f64,
    capacity: f64,
    refill_per_sec: f64,
    last_refill: Instant,
}

impl Bucket {
    fn new(capacity: f64, refill_per_sec: f64) -> Self {
        Self {
            tokens: capacity,
            capacity,
            refill_per_sec,
            last_refill: Instant::now(),
        }
    }

    fn try_consume(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        self.last_refill = now;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

const GENERAL_RATE_CAPACITY: f64 = 20.0;
const GENERAL_RATE_REFILL_PER_SEC: f64 = 20.0;
const VOICE_CONTROL_RATE_CAPACITY: f64 = 2.0;
const VOICE_CONTROL_RATE_REFILL_PER_SEC: f64 = 2.0;

#[derive(Default)]
pub struct ConnectionManager {
    connections: DashMap<ConnectionKey, Vec<mpsc::UnboundedSender<ServerEvent>>>,
    channel_members: DashMap<Uuid, DashMap<ConnectionKey, ()>>,
    rate_buckets: DashMap<(RateLimitKind, ConnectionKey), Bucket>,
}

impl ConnectionManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, key: ConnectionKey) -> mpsc::UnboundedReceiver<ServerEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.connections.entry(key).or_default().push(tx);
        metrics::gauge!("testrium_ws_active_connections").increment(1.0);
        rx
    }

    pub fn unregister_closed(&self, key: &ConnectionKey) {
        if let Some(mut entry) = self.connections.get_mut(key) {
            entry.retain(|tx| !tx.is_closed());
            if entry.is_empty() {
                drop(entry);
                self.connections.remove(key);
            }
        }
        self.rate_buckets.remove(&(RateLimitKind::General, key.clone()));
        self.rate_buckets.remove(&(RateLimitKind::VoiceControl, key.clone()));
        metrics::gauge!("testrium_ws_active_connections").decrement(1.0);
    }

    pub fn is_online(&self, key: &ConnectionKey) -> bool {
        self.connections
            .get(key)
            .map(|v| v.iter().any(|tx| !tx.is_closed()))
            .unwrap_or(false)
    }

    pub fn push(&self, key: &ConnectionKey, event: ServerEvent) -> bool {
        match self.connections.get(key) {
            Some(entry) => {
                let mut sent = false;
                for tx in entry.iter() {
                    if !tx.is_closed() && tx.send(event.clone()).is_ok() {
                        sent = true;
                    }
                }
                sent
            }
            None => false,
        }
    }

    pub fn join_channels(&self, key: &ConnectionKey, channel_ids: &[Uuid]) {
        for id in channel_ids {
            self.channel_members
                .entry(*id)
                .or_default()
                .insert(key.clone(), ());
        }
    }

    pub fn leave_channels(&self, key: &ConnectionKey, channel_ids: &[Uuid]) {
        for id in channel_ids {
            if let Some(members) = self.channel_members.get(id) {
                members.remove(key);
            }
        }
    }

    pub fn push_channel(&self, channel_id: &Uuid, event: ServerEvent, exclude: Option<&ConnectionKey>) {
        if let Some(members) = self.channel_members.get(channel_id) {
            for entry in members.iter() {
                let key = entry.key();
                if Some(key) == exclude {
                    continue;
                }
                self.push(key, event.clone());
            }
        }
    }

    fn check_rate_limit(&self, kind: RateLimitKind, key: &ConnectionKey, capacity: f64, refill_per_sec: f64) -> bool {
        let mut bucket = self
            .rate_buckets
            .entry((kind, key.clone()))
            .or_insert_with(|| Bucket::new(capacity, refill_per_sec));
        bucket.try_consume()
    }

    pub fn check_general_rate_limit(&self, key: &ConnectionKey) -> bool {
        let allowed = self.check_rate_limit(
            RateLimitKind::General,
            key,
            GENERAL_RATE_CAPACITY,
            GENERAL_RATE_REFILL_PER_SEC,
        );
        if !allowed {
            metrics::counter!("testrium_rate_limit_rejections_total", "kind" => "general").increment(1);
        }
        allowed
    }

    pub fn check_voice_control_rate_limit(&self, key: &ConnectionKey) -> bool {
        let allowed = self.check_rate_limit(
            RateLimitKind::VoiceControl,
            key,
            VOICE_CONTROL_RATE_CAPACITY,
            VOICE_CONTROL_RATE_REFILL_PER_SEC,
        );
        if !allowed {
            metrics::counter!("testrium_rate_limit_rejections_total", "kind" => "voice_control").increment(1);
        }
        allowed
    }
}
