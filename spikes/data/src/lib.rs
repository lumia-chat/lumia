//! The desktop spikes' data: a real `lumia_client` store on disk, opened and read through
//! `lumia_client::app::App` as every shell does, in-process with no binding layer between.
//! The Rune and GPUI spikes share it, so both draw the same channels.
//!
//! The core keeps no channel list locally (a shell asks the instance for the account snapshot),
//! so the seeder writes one beside the stores as `manifest.json`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lumia_client::ClientError;
use lumia_client::app::{App, Opener, Place, StoreName};
use lumia_client::sqlite::SqliteStore;
use lumia_core::proto::{Event, Message, MessageCreated, event};
use lumia_core::store::{LogKind, Store, StoredEvent};
use lumia_core::wire;
use prost::Message as _;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Channel {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Author {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Manifest {
    pub server: String,
    pub channels: Vec<Channel>,
    pub authors: Vec<Author>,
    /// Index into `channels` of the channel the app opens on.
    pub last_viewed: usize,
}

/// `LUMIA_SPIKE_DATA`, else `store/` beside this crate's manifest.
pub fn dir() -> PathBuf {
    std::env::var_os("LUMIA_SPIKE_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("store"))
}

/// How many of a channel's newest messages a shell's list holds (`LUMIA_SPIKE_LIMIT`).
pub fn limit() -> u32 {
    std::env::var("LUMIA_SPIKE_LIMIT").ok().and_then(|v| v.parse().ok()).unwrap_or(10_000)
}

/// Whether to print store timings (`LUMIA_SPIKE_TIMING`).
pub fn timing() -> bool {
    std::env::var_os("LUMIA_SPIKE_TIMING").is_some()
}

/// `HH:MM` (UTC) of a store timestamp.
pub fn clock(micros: i64) -> String {
    let seconds = micros.div_euclid(1_000_000);
    format!("{:02}:{:02}", seconds.div_euclid(3600).rem_euclid(24), seconds.div_euclid(60).rem_euclid(60))
}

fn store_path(dir: &Path, name: StoreName) -> PathBuf {
    dir.join(match name {
        StoreName::Instance => "instance.db",
        StoreName::Keys => "keys.db",
        StoreName::IdentityStore => "identity.db",
        StoreName::IdentityKeys => "identity-keys.db",
    })
}

/// The client over the stores in `dir`. Nothing here touches the network: the spike reads what
/// the store holds and never signs in.
pub fn open(dir: &Path) -> Result<Arc<App>, ClientError> {
    let dir = dir.to_path_buf();
    let opener: Opener = Arc::new(move |name| SqliteStore::open(&store_path(&dir, name)));
    App::open("http://localhost:4000", Place::Native, opener)
}

pub fn manifest(dir: &Path) -> std::io::Result<Manifest> {
    let bytes = std::fs::read(dir.join("manifest.json"))?;
    serde_json::from_slice(&bytes).map_err(std::io::Error::other)
}

const CHANNELS: &[&str] = &[
    "general", "design", "engineering", "releases", "support", "random", "ops", "mobile", "web", "core",
    "docs", "security", "infra", "research", "hiring", "music",
];
const AUTHORS: &[&str] = &[
    "Ada Marsh", "Bo Lindqvist", "Chidi Okafor", "Dana Reyes", "Emi Tanaka", "Farid Haddad", "Greta Novak",
    "Hugo Brandt", "Ines Duarte", "Jun Park", "Kira Volkova", "Leo Fontaine",
];

/// Writes `channels` channels of `per_channel` messages each into fresh stores in `dir`, as
/// `MessageCreated` events through `Store::apply`: the path a synced event takes.
pub fn seed(dir: &Path, channels: usize, per_channel: usize) -> Result<Manifest, ClientError> {
    std::fs::create_dir_all(dir).map_err(|e| ClientError::State(e.to_string()))?;
    for file in ["instance.db", "keys.db", "identity.db", "identity-keys.db"] {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(dir.join(format!("{file}{suffix}")));
        }
    }
    let mut store = SqliteStore::open(&store_path(dir, StoreName::Instance))?;
    let authors: Vec<(Vec<u8>, &str)> = AUTHORS.iter().map(|name| (wire::new_id(), *name)).collect();
    let mut manifest = Manifest {
        server: "Acme".into(),
        channels: Vec::new(),
        authors: authors.iter().map(|(id, name)| Author { id: wire::display(id), name: (*name).into() }).collect(),
        last_viewed: 0,
    };
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    // 2026-11-01T00:00:00Z, a message every 47 seconds.
    let start = 1_793_491_200_i64;
    for c in 0..channels {
        let channel_id = wire::new_id();
        let name = match CHANNELS.get(c) {
            Some(name) => (*name).to_string(),
            None => format!("channel-{c}"),
        };
        manifest.channels.push(Channel { id: wire::display(&channel_id), name });
        let mut seq = 0_u64;
        while (seq as usize) < per_channel {
            let batch: Vec<StoredEvent> = (0..1000.min(per_channel - seq as usize))
                .map(|_| {
                    seq += 1;
                    let (author, _) = &authors[rng.below(authors.len())];
                    let created_at = Some(prost_types::Timestamp { seconds: start + seq as i64 * 47, nanos: 0 });
                    let message = Message {
                        message_id: wire::new_id(),
                        channel_id: channel_id.clone(),
                        seq,
                        author_account_id: author.clone(),
                        body: body(&mut rng),
                        created_at,
                        ..Default::default()
                    };
                    let event = Event {
                        event_id: wire::new_id(),
                        channel_id: channel_id.clone(),
                        seq,
                        actor_account_id: author.clone(),
                        created_at,
                        payload: Some(event::Payload::MessageCreated(MessageCreated { message: Some(message) })),
                        ..Default::default()
                    };
                    StoredEvent { seq, bytes: event.encode_to_vec() }
                })
                .collect();
            store.apply(LogKind::Channel, &channel_id, &batch)?;
        }
    }
    let json = serde_json::to_vec_pretty(&manifest).map_err(|e| ClientError::State(e.to_string()))?;
    std::fs::write(dir.join("manifest.json"), json).map_err(|e| ClientError::State(e.to_string()))?;
    Ok(manifest)
}

/// xorshift64: the same store on every run, so numbers compare across runs.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

const WORDS: &[&str] = &[
    "the", "build", "passes", "on", "main", "after", "rebase", "I", "think", "we", "should", "ship", "it", "today",
    "gateway", "resume", "cursor", "channel", "invite", "roster", "device", "sync", "store", "snapshot", "retry",
    "looks", "good", "to", "me", "can", "you", "check", "staging", "again", "tomorrow", "morning", "before", "standup",
    "latency", "budget", "p99", "under", "load", "with", "a", "thousand", "subscribers", "and", "still", "fine",
    "composer", "sidebar", "tokens", "spacing", "hover", "state", "feels", "right", "now", "thanks", "for", "review",
];
const SCRIPTS: &[&str] = &[
    "了解しました。明日の朝に確認します 🙏",
    "好的，我明天早上看一下，然后在这里回复。",
    "알겠습니다, 내일 아침에 확인할게요.",
    "تمام، سأراجعها غدًا صباحًا ثم أرد هنا.",
    "Deployed 🚀🎉 and the dashboards are green 👩🏽‍💻",
    "Спасибо, посмотрю после обеда.",
];

fn sentence(rng: &mut Rng, min: usize, max: usize) -> String {
    let n = min + rng.below(max - min + 1);
    let mut s: Vec<&str> = (0..n).map(|_| WORDS[rng.below(WORDS.len())]).collect();
    let first = s.remove(0);
    let mut out = String::new();
    let mut chars = first.chars();
    if let Some(c) = chars.next() {
        out.extend(c.to_uppercase());
        out.push_str(chars.as_str());
    }
    for word in s {
        out.push(' ');
        out.push_str(word);
    }
    out.push('.');
    out
}

/// A message body in the protocol's Markdown subset: mostly one line, some paragraphs, and a
/// tail of lists, code, quotes and other scripts, so rows vary in height as a real channel's do.
fn body(rng: &mut Rng) -> String {
    match rng.below(100) {
        0..55 => sentence(rng, 3, 12),
        55..80 => {
            let n = 2 + rng.below(3);
            (0..n).map(|_| sentence(rng, 6, 16)).collect::<Vec<_>>().join(" ")
        }
        80..88 => format!(
            "{} **{}** `{}_{}` [the thread](https://lumia.chat/t/{})",
            sentence(rng, 4, 9),
            WORDS[rng.below(WORDS.len())],
            WORDS[rng.below(WORDS.len())],
            WORDS[rng.below(WORDS.len())],
            rng.below(9000),
        ),
        88..93 => {
            let n = 2 + rng.below(3);
            let items: Vec<String> = (0..n).map(|_| format!("- {}", sentence(rng, 3, 8))).collect();
            format!("{}\n\n{}", sentence(rng, 3, 6), items.join("\n"))
        }
        93..96 => format!(
            "{}\n\n```rust\nfn {}(seq: u64) -> u64 {{\n    seq + {}\n}}\n```",
            sentence(rng, 3, 7),
            WORDS[14 + rng.below(10)],
            rng.below(100),
        ),
        96..98 => SCRIPTS[rng.below(SCRIPTS.len())].to_string(),
        _ => format!("> {}\n\n{}", sentence(rng, 5, 10), sentence(rng, 3, 8)),
    }
}
