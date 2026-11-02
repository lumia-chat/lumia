//! `lumia-cli`: the headless client (plan step 3).
//!
//!     lumia-cli instance add http://127.0.0.1:4000
//!     lumia-cli account create --handle ryan --password '...'
//!     lumia-cli login --handle ryan --password '...'
//!     lumia-cli servers
//!     lumia-cli channels <server-id>
//!     lumia-cli send <channel-id> "hello"
//!     lumia-cli tail <channel-id>
//!     lumia-cli scenario all
//!
//! State lives under the platform data directory (or `--data-dir`): one
//! SQLite store and one file keystore per instance host, and the device's
//! identity under `identity/`, shared by every instance.

#![allow(clippy::print_stdout, clippy::print_stderr)]

mod scenario;

use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use lumia_client::api::{Api, Tokens};
use lumia_client::dm::{DmEngine, Outcome};
use lumia_client::gateway::{self, GatewayCommand, GatewayEvent};
use lumia_client::keystore::FileKeystore;
use lumia_client::sqlite::{DmStatus, SqliteStore};
use lumia_client::ClientError;
use lumia_core::keystore::Keystore;
use lumia_core::mls::projection::Projection;
use lumia_core::proto::{command, event, Command, DmOperation, SendMessage};
use lumia_core::store::Store;
use lumia_core::sync::Notice;
use lumia_core::wire;
use prost::Message as _;
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
    /// Follow a channel, printing events as they arrive; reconnects.
    Tail { channel_id: String },
    /// Run a failure-injection scenario against the instance (`all` for every one).
    Scenario { name: String },
    /// End-to-end encrypted personal DMs.
    Dm {
        #[command(subcommand)]
        action: DmCmd,
    },
}

#[derive(Subcommand, Debug)]
enum DmCmd {
    /// Open (or find) the personal DM with an account. The device that
    /// opens it first creates the MLS group and welcomes every other device.
    Open { account_id: String },
    /// Send a text message.
    Send { channel_id: String, body: String },
    /// Follow a personal DM, decrypting as events arrive; ctrl-c to stop.
    Tail { channel_id: String },
    /// Show the conversation as this device projects it.
    Messages { channel_id: String },
    /// Publish a fresh pool of key packages now (also done on connect when low).
    Setup,
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
    /// Bind the signed-in password account to this device's identity.
    LinkAccount {
        #[arg(long)]
        password: String,
    },
    /// On a new device: print the link request to approve elsewhere.
    LinkRequest {
        #[arg(long, default_value = "cli")]
        label: String,
    },
    /// On an existing device: approve a link request; prints the claim code.
    Approve { request: String },
    /// On the new device: claim the bundle by code.
    Claim { code: String },
    /// Revoke a device: root-signed with the recovery phrase, or signed by
    /// this device with the account password as reauth. With --all, publish
    /// to every instance this device knows and report each.
    Revoke {
        device_id: String,
        #[arg(long, required_unless_present = "password")]
        phrase: Option<String>,
        #[arg(long, required_unless_present = "phrase")]
        password: Option<String>,
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
    keys_dir: PathBuf,
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
    let api = api_for(&ctx)?;

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
            let channel = parse_uuid(&channel_id)?;
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
            let gw = tokio::spawn(gateway::run_forever(
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
                    GatewayEvent::Reconnecting {
                        attempt,
                        delay,
                        reason,
                    } => eprintln!("reconnecting in {delay:?} (attempt {attempt}): {reason}"),
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
            let gw = tokio::spawn(gateway::run_forever(
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
                        Some(GatewayEvent::SnapshotTaken { seq, .. }) => println!("-- snapshot to seq {seq}"),
                        Some(GatewayEvent::Reconnecting { attempt, delay, reason }) => {
                            println!("-- reconnecting in {delay:?} (attempt {attempt}): {reason}");
                        }
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
        Cmd::Scenario { name } => {
            let env = scenario::Env {
                api,
                device_id: ctx.device_id.clone(),
                keys_dir: ctx.keys_dir.clone(),
                scratch: ctx.data_dir.join("scenario"),
            };
            scenario::run(&name, &env).await
        }
        Cmd::Dm { action } => dm_cmd(ctx, api, action).await,
    }
}

// ---- Personal DMs ----------------------------------------------------------------

/// What a `dm` run is waiting for before it exits.
enum DmMode {
    /// Create the group (or wait for its welcome), then stop.
    Open(Vec<u8>),
    /// Send one message, wait for the ack, then stop.
    Send(Vec<u8>, String),
    /// Run until ctrl-c.
    Tail,
    /// Publish key packages, then stop.
    Setup,
}

fn dm_engine(ctx: &mut Ctx, api: &Api) -> Result<DmEngine, ClientError> {
    let roster = device(ctx).roster()?;
    let seed = ctx
        .identity_keys
        .get("device_key")?
        .ok_or_else(|| ClientError::State("no device key; create or claim an identity".into()))?;
    let account = ctx
        .store
        .meta_get("account_id")?
        .map(|h| hex::decode(h).unwrap_or_default())
        .unwrap_or_default();
    let store = SqliteStore::open(&ctx.data_dir.join(&ctx.host).join("store.sqlite"))?;
    DmEngine::new(
        api.clone(),
        store,
        account,
        ctx.device_id.clone(),
        &seed,
        roster,
    )
}

async fn dm_cmd(mut ctx: Ctx, api: Api, action: DmCmd) -> Result<(), ClientError> {
    match action {
        DmCmd::Open { account_id } => {
            let other = parse_uuid(&account_id)?;
            let opened = api.open_personal_dm(other).await?;
            let channel = opened
                .channel
                .ok_or_else(|| ClientError::State("no channel in response".into()))?;
            let mut engine = dm_engine(&mut ctx, &api)?;
            engine.record_channel(&channel)?;
            println!(
                "{}  personal DM ({})",
                wire::display(&channel.channel_id),
                if opened.existed { "existing" } else { "new" }
            );
            let mode = if opened.existed && !engine.is_member(&channel.channel_id)? {
                println!("waiting for the welcome from the device that opened it");
                DmMode::Open(channel.channel_id.clone())
            } else if opened.existed {
                println!("already a member");
                return Ok(());
            } else {
                DmMode::Open(channel.channel_id.clone())
            };
            dm_loop(
                ctx,
                api,
                engine,
                vec![channel.channel_id],
                mode,
                !opened.existed,
            )
            .await
        }
        DmCmd::Send { channel_id, body } => {
            let channel = parse_uuid(&channel_id)?;
            let mut engine = dm_engine(&mut ctx, &api)?;
            engine.load_participants(&channel)?;
            dm_loop(
                ctx,
                api,
                engine,
                vec![channel.clone()],
                DmMode::Send(channel, body),
                false,
            )
            .await
        }
        DmCmd::Tail { channel_id } => {
            let channel = parse_uuid(&channel_id)?;
            let mut engine = dm_engine(&mut ctx, &api)?;
            engine.load_participants(&channel)?;
            println!("tailing {channel_id}; ctrl-c to stop");
            dm_loop(ctx, api, engine, vec![channel], DmMode::Tail, false).await
        }
        DmCmd::Messages { channel_id } => {
            let channel = parse_uuid(&channel_id)?;
            let engine = dm_engine(&mut ctx, &api)?;
            let mut projection = Projection::default();
            for row in engine.rows(&channel)? {
                match row.status {
                    DmStatus::Decrypted => {
                        if let Ok(op) = DmOperation::decode(row.bytes.as_slice()) {
                            projection.apply(
                                row.seq,
                                &row.sender_identity_id,
                                &row.sender_device_id,
                                &op,
                            );
                        }
                    }
                    DmStatus::BeforeJoin => {
                        println!("[{}] sent before this device joined", row.seq)
                    }
                    DmStatus::Undecryptable => println!("[{}] could not be decrypted", row.seq),
                }
            }
            for m in projection.messages() {
                let flags = match (m.deleted, m.edited) {
                    (true, _) => " (deleted)",
                    (false, true) => " (edited)",
                    _ => "",
                };
                println!(
                    "[{}] {}: {}{flags}",
                    m.seq,
                    hex::encode(&m.sender_identity_id[..4]),
                    m.body
                );
            }
            Ok(())
        }
        DmCmd::Setup => {
            let engine = dm_engine(&mut ctx, &api)?;
            dm_loop(ctx, api, engine, Vec::new(), DmMode::Setup, false).await
        }
    }
}

/// Runs the gateway with the DM engine attached until `mode` is satisfied.
/// `create` means this device opened the channel and creates the group.
async fn dm_loop(
    ctx: Ctx,
    api: Api,
    mut engine: DmEngine,
    channels: Vec<Vec<u8>>,
    mode: DmMode,
    create: bool,
) -> Result<(), ClientError> {
    let (tx, rx) = mpsc::channel(16);
    let (etx, mut erx) = mpsc::channel(64);
    // Subscribe up front only to conversations whose participants this
    // device already knows; the others are subscribed when the account log
    // names them, so their events are never processed before the rosters
    // can be fetched.
    let mut known = Vec::new();
    for channel in channels {
        if !engine.load_participants(&channel)?.is_empty() {
            known.push(channel);
        }
    }
    let gw = tokio::spawn(gateway::run_forever(
        api,
        ctx.store,
        "lumia-cli",
        ctx.device_id.clone(),
        known.clone(),
        rx,
        etx,
    ));
    let mut host_store = SqliteStore::open(&ctx.data_dir.join(&ctx.host).join("store.sqlite"))?;
    let mut done = false;
    let result: Result<(), ClientError> = async {
        loop {
            let ev = tokio::select! {
                ev = erx.recv() => ev,
                _ = tokio::signal::ctrl_c() => break Ok(()),
            };
            let Some(ev) = ev else { break Ok(()) };
            let outcomes = match ev {
                GatewayEvent::Notice(Notice::Welcomed(w)) => {
                    host_store.meta_set("account_id", &hex::encode(&w.account_id))?;
                    engine.set_account(w.account_id.clone());
                    let mut out = match &mode {
                        DmMode::Setup => {
                            engine
                                .on_welcome(&lumia_core::proto::Welcome {
                                    key_package_count: 0,
                                    ..w.clone()
                                })
                                .await?
                        }
                        _ => engine.on_welcome(&w).await?,
                    };
                    match &mode {
                        DmMode::Open(channel) if create => {
                            out.extend(engine.create_group(channel).await?)
                        }
                        DmMode::Send(channel, body) => out.extend(engine.send_text(channel, body)?),
                        DmMode::Setup => {
                            println!("account {}", wire::display(&w.account_id));
                            done = true;
                        }
                        _ => {}
                    }
                    out
                }
                GatewayEvent::Notice(n) => engine.on_notice(&n).await?,
                GatewayEvent::Applied(events) => engine.on_events(&events).await?,
                GatewayEvent::AccountApplied(events) => engine.on_account_events(&events).await?,
                GatewayEvent::SnapshotTaken { seq, .. } => {
                    vec![Outcome::Print(format!("snapshot to seq {seq}"))]
                }
                GatewayEvent::Reconnecting {
                    attempt,
                    delay,
                    reason,
                } => {
                    vec![Outcome::Print(format!(
                        "reconnecting in {delay:?} (attempt {attempt}): {reason}"
                    ))]
                }
                GatewayEvent::Closed(reason) => {
                    println!("closed: {reason}");
                    break Ok(());
                }
            };
            for outcome in outcomes {
                match outcome {
                    Outcome::Print(s) => println!("-- {s}"),
                    Outcome::Send(cmd) => tx
                        .send(GatewayCommand::Send(cmd))
                        .await
                        .map_err(|e| ClientError::Transport(e.to_string()))?,
                    Outcome::Subscribe(ch) => {
                        println!("-- personal DM {} added", wire::display(&ch));
                        tx.send(GatewayCommand::Subscribe(ch))
                            .await
                            .map_err(|e| ClientError::Transport(e.to_string()))?;
                    }
                    Outcome::Operation {
                        seq, sender, op, ..
                    } => print_operation(seq, &sender, &op),
                    Outcome::GroupReady(ch) => {
                        println!("-- group ready for {}", wire::display(&ch));
                        if matches!(&mode, DmMode::Open(c) if *c == ch) {
                            done = true;
                        }
                    }
                    Outcome::Delivered(id) => {
                        println!("delivered {}", wire::display(&id));
                        if matches!(mode, DmMode::Send(..)) {
                            done = true;
                        }
                    }
                }
            }
            if done {
                break Ok(());
            }
        }
    }
    .await;
    drop(tx);
    gw.abort();
    result
}

fn print_operation(seq: u64, sender: &lumia_core::mls::credential::LeafIdentity, op: &DmOperation) {
    use lumia_core::proto::dm_operation::Op;
    let who = format!(
        "{}/{}",
        hex::encode(&sender.identity_id[..4]),
        &wire::display(&sender.device_id)[..8]
    );
    match &op.op {
        Some(Op::Send(s)) => println!("[{seq}] {who}: {}", s.body),
        Some(Op::Edit(e)) => println!(
            "[{seq}] {who} edited {}: {}",
            wire::display(&e.message_id),
            e.body
        ),
        Some(Op::Delete(d)) => println!("[{seq}] {who} deleted {}", wire::display(&d.message_id)),
        Some(Op::React(r)) => println!(
            "[{seq}] {who} {} {}",
            if r.remove { "unreacted" } else { "reacted" },
            r.emoji
        ),
        Some(Op::Pin(p)) => println!(
            "[{seq}] {who} {} {}",
            if p.unpin { "unpinned" } else { "pinned" },
            wire::display(&p.message_id)
        ),
        Some(Op::Read(r)) => println!("[{seq}] {who} read through {}", r.through_seq),
        Some(Op::Typing(_)) => println!("[{seq}] {who} is typing"),
        None => println!("[{seq}] {who}: (empty operation)"),
    }
}

fn device(ctx: &mut Ctx) -> lumia_client::identity::Device<'_, SqliteStore, FileKeystore> {
    lumia_client::identity::Device {
        store: &mut ctx.identity_store,
        keystore: &mut ctx.identity_keys,
        device_id: ctx.device_id.clone(),
    }
}

/// The API client for the current instance: its tokens from the keystore,
/// and every rotation written back there.
fn api_for(ctx: &Ctx) -> Result<Api, ClientError> {
    let base = ctx
        .store
        .meta_get("base_url")?
        .ok_or_else(|| ClientError::Transport("instance has no base url".into()))?;
    let keys_dir = ctx.keys_dir.clone();
    Ok(Api::new(&base)
        .with_tokens(tokens(&ctx.keystore)?)
        .on_rotate(Arc::new(move |pair| {
            if let Ok(mut keys) = FileKeystore::open(&keys_dir) {
                let _ = keys.put("access_token", pair.access_token.as_bytes());
                let _ = keys.put("refresh_token", pair.refresh_token.as_bytes());
            }
        })))
}

/// Every instance this data dir knows: (host, base url, tokens).
fn instances(ctx: &Ctx) -> Result<Vec<(String, String, Tokens)>, ClientError> {
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
        out.push((
            entry.file_name().to_string_lossy().to_string(),
            base,
            tokens(&keys)?,
        ));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
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
                let me = if *id == ctx.device_id {
                    " (this device)"
                } else {
                    ""
                };
                println!(
                    "  revoked {}  at {}{me}",
                    wire::display(id),
                    info.revoked_at / 1_000_000_000
                );
            }
            Ok(())
        }
        IdentityCmd::LinkAccount { password } => {
            device(ctx).link_account(api, &password).await?;
            println!("account on {} bound to this identity", ctx.host);
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
            password,
            all,
        } => {
            let target = parse_uuid(&device_id)?;
            match (phrase, password) {
                (Some(phrase), _) => {
                    device(ctx)
                        .revoke_with_phrase(api, &phrase, &target)
                        .await?;
                    println!("revoked {device_id} on {} (root-signed)", ctx.host);
                }
                (None, Some(password)) => {
                    device(ctx)
                        .revoke_with_device(api, &password, &target)
                        .await?;
                    println!(
                        "revoked {device_id} on {} (device-signed, reauth by password)",
                        ctx.host
                    );
                }
                (None, None) => return Err(ClientError::State("--phrase or --password".into())),
            }
            if all {
                publish_everywhere(ctx).await?;
            }
            Ok(())
        }
        IdentityCmd::Status => {
            let mine = device(ctx).roster()?;
            println!("this device holds v{}", mine.version);
            for (host, base, tokens) in instances(ctx)? {
                let api = Api::new(&base).with_tokens(tokens);
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
/// rosters are client-carried). A roster that adds revocations relative to
/// what an instance holds is refused at the publish endpoint unless
/// root-signed; the report says so per instance. Never stops early.
async fn publish_everywhere(ctx: &mut Ctx) -> Result<(), ClientError> {
    let roster = device(ctx).roster()?;
    for (host, base, tokens) in instances(ctx)? {
        let api = Api::new(&base).with_tokens(tokens);
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
    let keys_dir = dir.join("keys");
    let keystore = FileKeystore::open(&keys_dir)?;

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
        keys_dir,
        store,
        keystore,
        identity_store,
        identity_keys,
        device_id,
    })
}

fn tokens(keys: &FileKeystore) -> Result<Tokens, ClientError> {
    let read = |name: &str| -> Result<Option<String>, ClientError> {
        Ok(keys.get(name)?.and_then(|b| String::from_utf8(b).ok()))
    };
    Ok(Tokens {
        access: read("access_token")?,
        refresh: read("refresh_token")?,
    })
}

fn save_tokens(ctx: &mut Ctx, tokens: &lumia_core::proto::TokenPair) -> Result<(), ClientError> {
    ctx.keystore
        .put("access_token", tokens.access_token.as_bytes())?;
    ctx.keystore
        .put("refresh_token", tokens.refresh_token.as_bytes())?;
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
