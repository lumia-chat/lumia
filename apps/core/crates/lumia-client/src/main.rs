//! `lumia-cli`: the headless client (plan step 3).
//!
//!     lumia-cli instance add http://127.0.0.1:4000
//!     lumia-cli account create --handle ryan --password '...'
//!     lumia-cli login --handle ryan --password '...'
//!     lumia-cli servers
//!     lumia-cli channels <server-id>
//!     lumia-cli send <channel-id> "hello"
//!     lumia-cli tail <channel-id>
//!
//! State lives under the platform data directory (or `--data-dir`), one
//! SQLite store and one file keystore per instance host.

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use lumia_client::api::Api;
use lumia_client::gateway::{self, GatewayCommand, GatewayEvent};
use lumia_client::keystore::FileKeystore;
use lumia_client::sqlite::SqliteStore;
use lumia_client::ClientError;
use lumia_core::keystore::Keystore;
use lumia_core::proto::{command, event, Command, SendMessage};
use lumia_core::store::Store;
use lumia_core::sync::Notice;
use lumia_core::wire;
use tokio::sync::mpsc;

#[derive(Parser, Debug)]
#[command(name = "lumia-cli", about = "Headless Lumia client")]
struct Cli {
    /// Where state lives (default: the platform data directory).
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,
    /// Instance host to act on (default: the last one added).
    #[arg(long, global = true)]
    instance: Option<String>,
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Manage instances.
    Instance {
        #[command(subcommand)]
        action: InstanceCmd,
    },
    /// Manage the account on the current instance.
    Account {
        #[command(subcommand)]
        action: AccountCmd,
    },
    /// Sign in with a password, or with this device's identity (`--identity`).
    Login {
        #[arg(long, required_unless_present = "identity")]
        handle: Option<String>,
        #[arg(long, required_unless_present = "identity")]
        password: Option<String>,
        /// Sign in by identity proof instead of a password.
        #[arg(long)]
        identity: bool,
    },
    /// Identity: create, show, link devices, revoke.
    Identity {
        #[command(subcommand)]
        action: IdentityCmd,
    },
    /// List servers you belong to.
    Servers,
    /// Create a server you own.
    ServerCreate { name: String },
    /// Create a channel in a server.
    ChannelCreate { server_id: String, name: String },
    /// List channels in a server you can view.
    Channels { server_id: String },
    /// Send a message.
    Send { channel_id: String, body: String },
    /// Follow a channel, printing events as they arrive.
    Tail { channel_id: String },
}

#[derive(Subcommand, Debug)]
enum InstanceCmd {
    /// Add an instance by base URL and make it current.
    Add { url: String },
}

#[derive(Subcommand, Debug)]
enum AccountCmd {
    /// Create an account with a password and sign in.
    Create {
        #[arg(long)]
        handle: String,
        #[arg(long)]
        password: String,
    },
    /// Create an account bound to this device's identity and sign in.
    CreateByIdentity {
        #[arg(long)]
        handle: String,
    },
}

#[derive(Subcommand, Debug)]
enum IdentityCmd {
    /// Create a new identity on this device; prints the recovery phrase once.
    Create {
        #[arg(long, default_value = "cli")]
        label: String,
    },
    /// Show the identity and its roster.
    Show,
    /// On a new device: print the link request to approve elsewhere.
    LinkRequest {
        #[arg(long, default_value = "cli")]
        label: String,
    },
    /// On an existing device: approve a link request; prints the claim code.
    Approve { request: String },
    /// On the new device: claim the bundle by code.
    Claim { code: String },
    /// Revoke a device with the recovery phrase (root-signed). With --all,
    /// publish to every instance this device knows and report each.
    Revoke {
        device_id: String,
        #[arg(long)]
        phrase: String,
        #[arg(long)]
        all: bool,
    },
    /// Which roster version each instance holds: the honest "revoked on 1 of 2".
    Status,
    /// Publish this device's current roster to every instance it knows.
    Sync,
}

struct Ctx {
    host: String,
    data_dir: PathBuf,
    store: SqliteStore,
    keystore: FileKeystore,
    /// Identity state is per device, shared by every instance: lives at
    /// `<data>/identity/`, not under an instance host.
    identity_store: SqliteStore,
    identity_keys: FileKeystore,
    device_id: Vec<u8>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    if let Err(e) = run(Cli::parse()).await {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<(), ClientError> {
    let data_dir = cli.data_dir.clone().unwrap_or_else(default_data_dir);
    std::fs::create_dir_all(&data_dir).map_err(|e| ClientError::Transport(e.to_string()))?;
    let current_file = data_dir.join("current-instance");

    if let Cmd::Instance {
        action: InstanceCmd::Add { url },
    } = &cli.command
    {
        let host = host_of(url);
        let mut ctx = open(&data_dir, &host)?;
        ctx.store.meta_set("base_url", url.trim_end_matches('/'))?;
        std::fs::write(&current_file, &host).map_err(|e| ClientError::Transport(e.to_string()))?;
        println!("added {host}");
        return Ok(());
    }

    let host = match cli.instance {
        Some(h) => h,
        None => std::fs::read_to_string(&current_file)
            .map_err(|_| ClientError::Transport("no instance; run `instance add <url>`".into()))?,
    };
    let mut ctx = open(&data_dir, host.trim())?;
    let base = ctx
        .store
        .meta_get("base_url")?
        .ok_or_else(|| ClientError::Transport("instance has no base url".into()))?;
    let api = Api::new(&base).with_token(token(&ctx)?);

    match cli.command {
        Cmd::Instance { .. } => Ok(()),
        Cmd::Account {
            action: AccountCmd::Create { handle, password },
        } => {
            let tokens = api.create_account(&handle, &password).await?;
            save_tokens(&mut ctx, &tokens)?;
            println!("created and signed in as {handle}");
            Ok(())
        }
        Cmd::Login { identity: true, .. } => {
            let tokens = device(&mut ctx).sign_in(&api).await?;
            save_tokens(&mut ctx, &tokens)?;
            println!("signed in by identity");
            Ok(())
        }
        Cmd::Login {
            handle, password, ..
        } => {
            let (handle, password) = (handle.unwrap_or_default(), password.unwrap_or_default());
            let tokens = api
                .sign_in(&handle, &password, ctx.device_id.clone())
                .await?;
            save_tokens(&mut ctx, &tokens)?;
            println!("signed in as {handle}");
            Ok(())
        }
        Cmd::Account {
            action: AccountCmd::CreateByIdentity { handle },
        } => {
            let tokens = device(&mut ctx).create_account(&api, &handle).await?;
            save_tokens(&mut ctx, &tokens)?;
            println!("created {handle} bound to this identity and signed in");
            Ok(())
        }
        Cmd::Identity { action } => identity_cmd(&mut ctx, &api, action).await,
        Cmd::Servers => {
            let list = api.servers().await?;
            for s in list.servers {
                println!("{}  {}", wire::display(&s.server_id), s.name);
            }
            Ok(())
        }
        Cmd::ServerCreate { name } => {
            let s = api.create_server(&name).await?;
            println!("{}  {}", wire::display(&s.server_id), s.name);
            Ok(())
        }
        Cmd::ChannelCreate { server_id, name } => {
            let c = api.create_channel(parse_uuid(&server_id)?, &name).await?;
            println!("{}  #{}", wire::display(&c.channel_id), c.name);
            Ok(())
        }
        Cmd::Channels { server_id } => {
            let list = api.channels(&server_id).await?;
            for c in list.channels {
                println!(
                    "{}  #{}  head={}",
                    wire::display(&c.channel_id),
                    c.name,
                    c.last_seq
                );
            }
            Ok(())
        }
        Cmd::Send { channel_id, body } => {
            let channel = wire::uuid_from(&parse_uuid(&channel_id)?)
                .map(wire::uuid_bytes)
                .ok_or(ClientError::Transport("bad channel id".into()))?;
            let cmd = Command {
                command_id: wire::new_id(),
                channel_id: channel.clone(),
                op: Some(command::Op::SendMessage(SendMessage {
                    body,
                    ..Default::default()
                })),
            };
            let (tx, rx) = mpsc::channel(8);
            let (etx, mut erx) = mpsc::channel(64);
            let gw = tokio::spawn(gateway::run(
                api,
                ctx.store,
                "lumia-cli",
                ctx.device_id,
                vec![channel],
                rx,
                etx,
            ));
            tx.send(GatewayCommand::Send(cmd))
                .await
                .map_err(|e| ClientError::Transport(e.to_string()))?;
            while let Some(ev) = erx.recv().await {
                match ev {
                    GatewayEvent::Notice(Notice::Acked { seq, .. }) => {
                        println!("acked at seq {seq}");
                        break;
                    }
                    GatewayEvent::Notice(Notice::Error(e)) => {
                        eprintln!("error: {} {}", e.code, e.message);
                        break;
                    }
                    GatewayEvent::Closed(reason) => {
                        eprintln!("closed: {reason}");
                        break;
                    }
                    _ => {}
                }
            }
            drop(tx);
            gw.abort();
            Ok(())
        }
        Cmd::Tail { channel_id } => {
            let channel = parse_uuid(&channel_id)?;
            let (tx, rx) = mpsc::channel(8);
            let (etx, mut erx) = mpsc::channel(64);
            let gw = tokio::spawn(gateway::run(
                api,
                ctx.store,
                "lumia-cli",
                ctx.device_id,
                vec![channel],
                rx,
                etx,
            ));
            println!("tailing {channel_id}; ctrl-c to stop");
            loop {
                tokio::select! {
                    ev = erx.recv() => match ev {
                        Some(GatewayEvent::Applied(events)) => {
                            for e in events { print_event(&e); }
                        }
                        Some(GatewayEvent::Notice(n)) => println!("-- {n:?}"),
                        Some(GatewayEvent::Closed(reason)) => { println!("closed: {reason}"); break; }
                        Some(GatewayEvent::AccountApplied(_)) => {}
                        None => break,
                    },
                    _ = tokio::signal::ctrl_c() => break,
                }
            }
            drop(tx);
            gw.abort();
            Ok(())
        }
    }
}

fn device(ctx: &mut Ctx) -> lumia_client::identity::Device<'_, SqliteStore, FileKeystore> {
    lumia_client::identity::Device {
        store: &mut ctx.identity_store,
        keystore: &mut ctx.identity_keys,
        device_id: ctx.device_id.clone(),
    }
}

/// Every instance this data dir knows: (host, base url, access token).
fn instances(ctx: &Ctx) -> Result<Vec<(String, String, Option<String>)>, ClientError> {
    let mut out = Vec::new();
    let entries =
        std::fs::read_dir(&ctx.data_dir).map_err(|e| ClientError::Transport(e.to_string()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.join("store.sqlite").exists() || path.file_name().is_some_and(|n| n == "identity")
        {
            continue;
        }
        let store = SqliteStore::open(&path.join("store.sqlite"))?;
        let Some(base) = store.meta_get("base_url")? else {
            continue;
        };
        let keys = FileKeystore::open(&path.join("keys"))?;
        let token = keys
            .get("access_token")?
            .and_then(|b| String::from_utf8(b).ok());
        out.push((entry.file_name().to_string_lossy().to_string(), base, token));
    }
    out.sort();
    Ok(out)
}

async fn identity_cmd(ctx: &mut Ctx, api: &Api, action: IdentityCmd) -> Result<(), ClientError> {
    match action {
        IdentityCmd::Create { label } => {
            let created = device(ctx).create(&label)?;
            println!("identity {}", created.identity_id);
            println!("recovery phrase (shown once, write it down):");
            println!("{}", created.phrase);
            Ok(())
        }
        IdentityCmd::Show => {
            let verified = device(ctx).roster()?;
            println!("identity {}", hex::encode(&verified.identity_id));
            println!(
                "roster v{} root_signed={}",
                verified.version, verified.root_signed
            );
            for (id, info) in &verified.devices {
                let me = if *id == ctx.device_id {
                    " (this device)"
                } else {
                    ""
                };
                println!(
                    "  device {}  {:?}  {}{me}",
                    wire::display(id),
                    info.kind,
                    if info.cert.added_by.is_empty() {
                        "root"
                    } else {
                        "device"
                    }
                );
            }
            for (id, info) in &verified.revoked {
                println!(
                    "  revoked {}  at {}",
                    wire::display(id),
                    info.revoked_at / 1_000_000_000
                );
            }
            Ok(())
        }
        IdentityCmd::LinkRequest { label } => {
            let request = device(ctx).link_request(&label)?;
            println!("{request}");
            Ok(())
        }
        IdentityCmd::Approve { request } => {
            let code = device(ctx).approve(api, &request).await?;
            println!("code {code} on {}", api.host());
            Ok(())
        }
        IdentityCmd::Claim { code } => {
            let verified = device(ctx).claim(api, &code).await?;
            println!(
                "linked to identity {} (roster v{})",
                hex::encode(&verified.identity_id),
                verified.version
            );
            Ok(())
        }
        IdentityCmd::Revoke {
            device_id,
            phrase,
            all,
        } => {
            device(ctx)
                .revoke_with_phrase(api, &phrase, &parse_uuid(&device_id)?)
                .await?;
            println!("revoked {device_id} on {}", ctx.host);
            if all {
                publish_everywhere(ctx).await?;
            }
            Ok(())
        }
        IdentityCmd::Status => {
            let mine = device(ctx).roster()?;
            println!("this device holds v{}", mine.version);
            for (host, base, token) in instances(ctx)? {
                let api = Api::new(&base).with_token(token);
                match api.get_roster().await {
                    Ok(resp) => match resp.roster.map(|r| {
                        lumia_core::identity::roster::verify_roster(
                            &r,
                            None,
                            lumia_client::identity::now_nanos(),
                        )
                    }) {
                        Some(Ok(v)) => {
                            let behind = if v.version < mine.version {
                                " (behind)"
                            } else {
                                ""
                            };
                            println!("  {host}: v{}{behind}", v.version);
                        }
                        _ => println!("  {host}: unverifiable roster"),
                    },
                    Err(e) => println!("  {host}: unreachable ({e})"),
                }
            }
            Ok(())
        }
        IdentityCmd::Sync => publish_everywhere(ctx).await,
    }
}

/// Carries the current roster to every instance (identity spec 5.1:
/// rosters are client-carried). Reports per instance; never stops early.
async fn publish_everywhere(ctx: &mut Ctx) -> Result<(), ClientError> {
    let roster = device(ctx).roster()?;
    for (host, base, token) in instances(ctx)? {
        let api = Api::new(&base).with_token(token);
        match api.publish_roster(roster.signed.clone()).await {
            Ok(()) => println!("  {host}: now v{}", roster.version),
            Err(e) => println!("  {host}: not updated ({e})"),
        }
    }
    Ok(())
}

fn print_event(e: &lumia_core::proto::Event) {
    match &e.payload {
        Some(event::Payload::MessageCreated(c)) => {
            let body = c.message.as_ref().map(|m| m.body.as_str()).unwrap_or("");
            let author = c
                .message
                .as_ref()
                .map(|m| wire::display(&m.author_account_id))
                .unwrap_or_default();
            println!("[{}] {author}: {body}", e.seq);
        }
        Some(event::Payload::MessageEdited(ed)) => println!(
            "[{}] edit {}: {}",
            e.seq,
            wire::display(&ed.message_id),
            ed.body
        ),
        Some(event::Payload::MessageDeleted(d)) => {
            println!("[{}] delete {}", e.seq, wire::display(&d.message_id))
        }
        Some(event::Payload::Redacted(_)) => println!("[{}] (redacted)", e.seq),
        Some(other) => println!("[{}] {other:?}", e.seq),
        None => println!("[{}] (empty)", e.seq),
    }
}

fn open(data_dir: &std::path::Path, host: &str) -> Result<Ctx, ClientError> {
    let dir = data_dir.join(host);
    std::fs::create_dir_all(&dir).map_err(|e| ClientError::Transport(e.to_string()))?;
    let store = SqliteStore::open(&dir.join("store.sqlite"))?;
    let keystore = FileKeystore::open(&dir.join("keys"))?;

    let identity_dir = data_dir.join("identity");
    std::fs::create_dir_all(&identity_dir).map_err(|e| ClientError::Transport(e.to_string()))?;
    let identity_store = SqliteStore::open(&identity_dir.join("meta.sqlite"))?;
    let mut identity_keys = FileKeystore::open(&identity_dir.join("keys"))?;
    let device_id = match identity_keys.get("device_id")? {
        Some(id) => id,
        None => {
            let id = wire::new_id();
            identity_keys.put("device_id", &id)?;
            id
        }
    };
    Ok(Ctx {
        host: host.to_string(),
        data_dir: data_dir.to_path_buf(),
        store,
        keystore,
        identity_store,
        identity_keys,
        device_id,
    })
}

fn token(ctx: &Ctx) -> Result<Option<String>, ClientError> {
    Ok(ctx
        .keystore
        .get("access_token")?
        .and_then(|b| String::from_utf8(b).ok()))
}

fn save_tokens(ctx: &mut Ctx, tokens: &lumia_core::proto::TokenPair) -> Result<(), ClientError> {
    ctx.keystore
        .put("access_token", tokens.access_token.as_bytes())?;
    ctx.keystore
        .put("refresh_token", tokens.refresh_token.as_bytes())?;
    let _ = &ctx.host;
    Ok(())
}

fn parse_uuid(s: &str) -> Result<Vec<u8>, ClientError> {
    uuid::Uuid::parse_str(s)
        .map(wire::uuid_bytes)
        .map_err(|_| ClientError::Transport(format!("not a uuid: {s}")))
}

fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| {
            u.host_str().map(|h| match u.port() {
                Some(p) => format!("{h}_{p}"),
                None => h.to_string(),
            })
        })
        .unwrap_or_else(|| url.replace(['/', ':'], "_"))
}

fn default_data_dir() -> PathBuf {
    directories::ProjectDirs::from("chat", "lumia", "lumia-cli")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".lumia-cli"))
}
