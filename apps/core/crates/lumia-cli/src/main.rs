//! `lumia-cli`: the headless client (plan step 3), on `lumia-client`.
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
use lumia_client::app::{invite_code_of, magic_token_of};
use lumia_client::dm::{DmEngine, Outcome};
use lumia_client::futures_channel::mpsc;
use lumia_client::futures_util::StreamExt as _;
use lumia_client::gateway::{self, GatewayCommand, GatewayEvent};
use lumia_client::keystore::FileKeystore;
use lumia_client::sqlite::{DmStatus, SqliteStore};
use lumia_client::ClientError;
use lumia_core::keystore::Keystore;
use lumia_core::mls::projection::Projection;
use lumia_core::proto::{
    command, event, mention, AddReaction, Command, DmOperation, Mention, QuoteReply,
    RemoveReaction, SendMessage,
};
use lumia_core::store::Store;
use lumia_core::sync::Notice;
use lumia_core::wire;
use prost::Message as _;

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
    /// Ask the instance to mail a sign-in link to an address.
    MagicLink { email: String },
    /// Sign in with a password, with this device's identity (`--identity`), or with a magic link's token (`--magic`).
    Login {
        #[arg(long, required_unless_present_any = ["identity", "magic"])]
        handle: Option<String>,
        #[arg(long, required_unless_present_any = ["identity", "magic"])]
        password: Option<String>,
        /// Sign in by identity proof instead of a password.
        #[arg(long)]
        identity: bool,
        /// The token from a magic-link mail (or the whole link).
        #[arg(long)]
        magic: Option<String>,
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
    /// Change a server's settings: its name, how long it keeps messages
    /// (`--retention 0` for ever), its picture (a file), or none (`--no-icon`).
    ServerUpdate {
        server_id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        retention: Option<u32>,
        #[arg(long)]
        icon: Option<PathBuf>,
        #[arg(long)]
        no_icon: bool,
    },
    /// Create a channel in a server.
    ChannelCreate { server_id: String, name: String },
    /// Mint an invite link for a server (unlimited unless `--max-uses`, open-ended unless `--hours`).
    InviteCreate {
        server_id: String,
        #[arg(long, default_value_t = 0)]
        max_uses: u32,
        #[arg(long)]
        hours: Option<u32>,
    },
    /// Show what an invite link or code leads to, without signing in.
    Invite { link_or_code: String },
    /// Join the server an invite link or code names.
    Join { link_or_code: String },
    /// Revoke an invite you made (or any, with manage_server).
    InviteRevoke { link_or_code: String },
    /// List a server's invites (needs create_invites there).
    Invites { server_id: String },
    /// List channels in a server you can view.
    Channels { server_id: String },
    /// Send a message; `--mention <account-id>` (repeatable) mentions an account;
    /// `--reply-to <message-id>` with `--quote <text>` quotes a message of the channel.
    Send {
        channel_id: String,
        body: String,
        #[arg(long)]
        mention: Vec<String>,
        #[arg(long)]
        reply_to: Option<String>,
        #[arg(long, default_value = "")]
        quote: String,
    },
    /// React to a message with one emoji, or take the reaction away (`--remove`).
    React {
        channel_id: String,
        message_id: String,
        emoji: String,
        #[arg(long)]
        remove: bool,
    },
    /// Print this account's id.
    Whoami,
    /// Follow a channel, printing events as they arrive; reconnects.
    Tail { channel_id: String },
    /// Say this account is typing in a server's channel, once (protocol 6.7).
    Typing { channel_id: String },
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
    /// Open (or find) the personal DM with an account, or a personal group
    /// DM with several. Needs an accepted DM request with each. The device
    /// that opens it creates the MLS group and welcomes every other device.
    Open { account_ids: Vec<String> },
    /// Ask an account for a DM; they accept with `dm respond`.
    Request { account_id: String },
    /// Pending DM requests to this account.
    Requests,
    /// Answer a DM request: accept (this device creates the group), decline, or block.
    Respond {
        request_id: String,
        response: String,
    },
    /// Delete one of this identity's messages.
    Delete {
        channel_id: String,
        message_id: String,
    },
    /// Send a text message.
    Send {
        channel_id: String,
        body: String,
        /// Fault injection: persist the encrypted message and the outbox
        /// entry, then exit before the send (e2ee spec section 12).
        #[arg(long, hide = true)]
        crash_after_persist: bool,
    },
    /// Follow a personal DM, decrypting as events arrive; ctrl-c to stop.
    Tail { channel_id: String },
    /// Show the conversation as this device projects it.
    Messages { channel_id: String },
    /// Publish a fresh pool of key packages now (also done on connect when low).
    Setup,
    /// Rejoin a conversation by external commit from its GroupInfo (recovery).
    Rejoin { channel_id: String },
    /// History backup: `on`, `off`, or `status`.
    Backup { action: String },
    /// Restore history from every device's manifest on this instance.
    Restore,
}

#[derive(Subcommand, Debug)]
enum InstanceCmd {
    /// Add an instance by base URL and make it current.
    Add { url: String },
}

#[derive(Subcommand, Debug)]
enum AccountCmd {
    /// Create an account with a password and sign in; `--invite` joins a server too.
    Create {
        #[arg(long)]
        handle: String,
        #[arg(long)]
        password: String,
        #[arg(long)]
        invite: Option<String>,
        /// An address for magic-link sign-in and recovery.
        #[arg(long)]
        email: Option<String>,
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
    /// Sign a new profile for the identity (and send it when signed in).
    Profile {
        #[arg(long)]
        display_name: String,
        #[arg(long, default_value = "")]
        pronouns: String,
        #[arg(long, default_value = "")]
        bio: String,
    },
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
    /// Rotate the root: a new phrase, the roster re-signed under its root,
    /// chunk keys re-wrapped and the identity backup rewritten on every
    /// instance this device knows (identity spec 4.5).
    Rotate {
        #[arg(long)]
        phrase: String,
    },
    /// Recover with no device left: from the phrase, revoke every device the
    /// instance holds, add this one, and sign in (identity spec 4.3).
    Recover {
        #[arg(long)]
        phrase: String,
        #[arg(long, default_value = "recovered")]
        label: String,
    },
    /// Write the identity backup (roster, host accounts) to the current instance.
    Backup,
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
            action:
                AccountCmd::Create {
                    handle,
                    password,
                    invite,
                    email,
                },
        } => {
            let code = invite.as_deref().map(invite_code_of).unwrap_or_default();
            let tokens = api
                .create_account(
                    &handle,
                    email.as_deref().unwrap_or(""),
                    &password,
                    &code,
                    ctx.device_id.clone(),
                )
                .await?;
            save_tokens(&mut ctx, &tokens)?;
            println!("created and signed in as {handle}");
            Ok(())
        }
        Cmd::MagicLink { email } => {
            api.magic_link_request(&email).await?;
            println!("if {email} has an account here, a link is on its way");
            Ok(())
        }
        Cmd::Login {
            magic: Some(token), ..
        } => {
            let tokens = api
                .magic_link_complete(&magic_token_of(&token), ctx.device_id.clone())
                .await?;
            save_tokens(&mut ctx, &tokens)?;
            println!("signed in with the link");
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
        Cmd::ServerUpdate {
            server_id,
            name,
            retention,
            icon,
            no_icon,
        } => {
            let mut server = None;
            if name.is_some() || retention.is_some() {
                server = Some(api.update_server(&server_id, name, retention).await?);
            }
            if let Some(path) = icon {
                let picture = std::fs::read(&path)
                    .map_err(|e| ClientError::State(format!("{}: {e}", path.display())))?;
                server = Some(
                    api.put_server_icon(&server_id, picture, "application/octet-stream")
                        .await?,
                );
            }
            if no_icon {
                server = Some(api.delete_server_icon(&server_id).await?);
            }
            let s = server.ok_or_else(|| ClientError::State("nothing to change".into()))?;
            println!(
                "{}  {}  retention {}  icon {}",
                wire::display(&s.server_id),
                s.name,
                if s.retention_days == 0 {
                    "for ever".to_string()
                } else {
                    format!("{} days", s.retention_days)
                },
                if s.icon_url.is_empty() {
                    "none"
                } else {
                    &s.icon_url
                }
            );
            Ok(())
        }
        Cmd::ChannelCreate { server_id, name } => {
            let c = api
                .create_channel(parse_uuid(&server_id)?, &name, false)
                .await?;
            println!("{}  #{}", wire::display(&c.channel_id), c.name);
            Ok(())
        }
        Cmd::InviteCreate {
            server_id,
            max_uses,
            hours,
        } => {
            let expires_at = hours.map(|h| {
                prost_types::Timestamp::from(
                    std::time::SystemTime::now()
                        + std::time::Duration::from_secs(u64::from(h) * 3600),
                )
            });
            let req = lumia_core::proto::CreateInviteRequest {
                server_id: parse_uuid(&server_id)?,
                max_uses,
                expires_at,
                ..Default::default()
            };
            let invite = api.create_invite(&req).await?;
            println!(
                "{}  {}/invite/{}",
                invite.code,
                api.base().trim_end_matches('/'),
                invite.code
            );
            Ok(())
        }
        Cmd::Invite { link_or_code } => {
            let p = api.invite_preview(&invite_code_of(&link_or_code)).await?;
            let server = p.server.unwrap_or_default();
            let instance = p.instance.unwrap_or_default();
            println!(
                "{}  {}  {} members  on {}{}",
                wire::display(&server.server_id),
                server.name,
                p.member_count,
                instance.host,
                if p.as_guest { "  (as a guest)" } else { "" }
            );
            Ok(())
        }
        Cmd::Join { link_or_code } => {
            let member = api.accept_invite(&invite_code_of(&link_or_code)).await?;
            println!("joined {}", wire::display(&member.server_id));
            Ok(())
        }
        Cmd::Invites { server_id } => {
            let list = api.invites(&server_id).await?;
            for i in list.invites {
                let uses = if i.max_uses == 0 {
                    format!("{} uses", i.uses)
                } else {
                    format!("{}/{} uses", i.uses, i.max_uses)
                };
                let expiry = match wire::micros(i.expires_at.as_ref()) {
                    Some(us) => format!("expires at {}", us / 1_000_000),
                    None => "no expiry".to_string(),
                };
                println!("{}  {uses}  {expiry}", i.code);
            }
            Ok(())
        }
        Cmd::InviteRevoke { link_or_code } => {
            let code = invite_code_of(&link_or_code);
            api.delete_invite(&code).await?;
            println!("revoked {code}");
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
        Cmd::Whoami => {
            let snap = api.account_snapshot().await?;
            let account = snap
                .account
                .ok_or_else(|| ClientError::State("not signed in".into()))?;
            println!("{}", wire::display(&account.account_id));
            Ok(())
        }
        Cmd::Send {
            channel_id,
            body,
            mention,
            reply_to,
            quote,
        } => {
            let channel = parse_uuid(&channel_id)?;
            let mut mentions = Vec::with_capacity(mention.len());
            for id in &mention {
                mentions.push(Mention {
                    target: Some(mention::Target::AccountId(parse_uuid(id)?)),
                    start: 0,
                    end: 0,
                });
            }
            let reply_to = match reply_to {
                Some(id) => Some(QuoteReply {
                    message_id: parse_uuid(&id)?,
                    text: quote,
                    ..Default::default()
                }),
                None => None,
            };
            let op = command::Op::SendMessage(SendMessage {
                body,
                mentions,
                reply_to,
                ..Default::default()
            });
            send_command(api, ctx, channel, op).await
        }
        Cmd::React {
            channel_id,
            message_id,
            emoji,
            remove,
        } => {
            let channel = parse_uuid(&channel_id)?;
            let message_id = parse_uuid(&message_id)?;
            let op = if remove {
                command::Op::RemoveReaction(RemoveReaction {
                    message_id,
                    reaction: emoji,
                })
            } else {
                command::Op::AddReaction(AddReaction {
                    message_id,
                    reaction: emoji,
                })
            };
            send_command(api, ctx, channel, op).await
        }
        Cmd::Tail { channel_id } => {
            let channel = parse_uuid(&channel_id)?;
            let (tx, rx) = mpsc::unbounded();
            let (etx, mut erx) = mpsc::unbounded();
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
                    ev = erx.next() => match ev {
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
        Cmd::Typing { channel_id } => {
            let channel = parse_uuid(&channel_id)?;
            let (tx, rx) = mpsc::unbounded();
            let (etx, mut erx) = mpsc::unbounded();
            let gw = tokio::spawn(gateway::run_forever(
                api,
                ctx.store,
                "lumia-cli",
                ctx.device_id,
                vec![channel.clone()],
                rx,
                etx,
            ));
            // The instance takes typing only from a subscriber, and the loop does not
            // say when the channel has become one: ask a few times over two seconds.
            // The first that can go does, and the instance drops the rest as too soon.
            let mut tick = tokio::time::interval(std::time::Duration::from_millis(200));
            let (mut welcomed, mut asked) = (false, 0);
            loop {
                tokio::select! {
                    ev = erx.next() => match ev {
                        Some(GatewayEvent::Notice(Notice::Welcomed(_))) => welcomed = true,
                        Some(GatewayEvent::Notice(Notice::Error(e))) => {
                            eprintln!("error: {} {}", e.code, e.message);
                            break;
                        }
                        Some(GatewayEvent::Notice(Notice::SubscriptionFailed { error, .. })) => {
                            eprintln!("not subscribed: {}", error.map(|e| e.message).unwrap_or_default());
                            break;
                        }
                        Some(GatewayEvent::Closed(reason)) => {
                            eprintln!("closed: {reason}");
                            break;
                        }
                        Some(_) => {}
                        None => break,
                    },
                    _ = tick.tick() => if welcomed {
                        if asked == 10 {
                            println!("typing in {channel_id}");
                            break;
                        }
                        asked += 1;
                        tx.unbounded_send(GatewayCommand::Typing(channel.clone()))
                            .map_err(|e| ClientError::Transport(e.to_string()))?;
                    },
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
    /// Rejoin by external commit, then stop once the commit is acked.
    Rejoin(Vec<u8>),
    /// Delete one message, wait for the ack, then stop.
    Delete(Vec<u8>, Vec<u8>),
}

/// Sends one command to a channel over the gateway and prints its ack with the
/// command's id (a sent message's id), or the instance's refusal.
async fn send_command(
    api: Api,
    ctx: Ctx,
    channel: Vec<u8>,
    op: command::Op,
) -> Result<(), ClientError> {
    let id = wire::new_id();
    let cmd = Command {
        command_id: id.clone(),
        channel_id: channel.clone(),
        op: Some(op),
    };
    let (tx, rx) = mpsc::unbounded();
    let (etx, mut erx) = mpsc::unbounded();
    let gw = tokio::spawn(gateway::run_forever(
        api,
        ctx.store,
        "lumia-cli",
        ctx.device_id,
        vec![channel],
        rx,
        etx,
    ));
    tx.unbounded_send(GatewayCommand::Send(cmd))
        .map_err(|e| ClientError::Transport(e.to_string()))?;
    while let Some(ev) = erx.next().await {
        match ev {
            GatewayEvent::Notice(Notice::Acked { seq, .. }) => {
                // The command's id is the message's, for a quote of it.
                println!("acked at seq {seq}: {}", wire::display(&id));
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

fn dm_engine(ctx: &mut Ctx, api: &Api) -> Result<DmEngine, ClientError> {
    let host = ctx.host.clone();
    dm_engine_at(ctx, &host, api)
}

/// The DM engine for one of this device's instances, by host.
fn dm_engine_at(ctx: &mut Ctx, host: &str, api: &Api) -> Result<DmEngine, ClientError> {
    let roster = device(ctx).roster()?;
    let seed = ctx
        .identity_keys
        .get("device_key")?
        .ok_or_else(|| ClientError::State("no device key; create or claim an identity".into()))?;
    let host_store = SqliteStore::open(&ctx.data_dir.join(host).join("store.sqlite"))?;
    let account = host_store
        .meta_get("account_id")?
        .map(|h| hex::decode(h).unwrap_or_default())
        .unwrap_or_default();
    let backup_key = ctx.identity_keys.get("backup_key")?;
    let store = SqliteStore::open(&ctx.data_dir.join(host).join("store.sqlite"))?;
    DmEngine::new(
        api.clone(),
        store,
        account,
        ctx.device_id.clone(),
        &seed,
        backup_key,
        roster,
    )
}

async fn dm_cmd(mut ctx: Ctx, api: Api, action: DmCmd) -> Result<(), ClientError> {
    match action {
        DmCmd::Open { account_ids } => {
            let mut others = Vec::with_capacity(account_ids.len());
            for id in &account_ids {
                others.push(parse_uuid(id)?);
            }
            open_dm(ctx, api, others, false).await
        }
        DmCmd::Request { account_id } => {
            let request = api.send_dm_request(parse_uuid(&account_id)?).await?;
            println!(
                "request {} to {}: {:?}",
                wire::display(&request.request_id),
                account_id,
                lumia_core::proto::DmRequestState::try_from(request.state).unwrap_or_default()
            );
            Ok(())
        }
        DmCmd::Requests => {
            let snapshot = api.account_snapshot().await?;
            if snapshot.pending_requests.is_empty() {
                println!("no pending requests");
            }
            for r in snapshot.pending_requests {
                println!(
                    "{}  from {}",
                    wire::display(&r.request_id),
                    wire::display(&r.from_account_id)
                );
            }
            Ok(())
        }
        DmCmd::Respond {
            request_id,
            response,
        } => {
            use lumia_core::proto::DmRequestState;
            let state = match response.as_str() {
                "accept" => DmRequestState::Accepted,
                "decline" => DmRequestState::Declined,
                "block" => DmRequestState::Blocked,
                other => {
                    return Err(ClientError::State(format!(
                        "unknown response {other}; accept, decline or block"
                    )))
                }
            };
            let request = api.respond_dm_request(&request_id, state).await?;
            println!(
                "request {} {:?}",
                wire::display(&request.request_id),
                DmRequestState::try_from(request.state).unwrap_or_default()
            );
            if state == DmRequestState::Accepted {
                // The accepter creates the group (e2ee spec section 4).
                open_dm(ctx, api, vec![request.from_account_id], true).await
            } else {
                Ok(())
            }
        }
        DmCmd::Delete {
            channel_id,
            message_id,
        } => {
            let channel = parse_uuid(&channel_id)?;
            let message = parse_uuid(&message_id)?;
            let mut engine = dm_engine(&mut ctx, &api)?;
            engine.load_participants(&channel)?;
            dm_loop(
                ctx,
                api,
                engine,
                vec![channel.clone()],
                DmMode::Delete(channel, message),
                false,
                false,
            )
            .await
        }
        DmCmd::Send {
            channel_id,
            body,
            crash_after_persist,
        } => {
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
                crash_after_persist,
            )
            .await
        }
        DmCmd::Tail { channel_id } => {
            let channel = parse_uuid(&channel_id)?;
            let mut engine = dm_engine(&mut ctx, &api)?;
            engine.load_participants(&channel)?;
            println!("tailing {channel_id}; ctrl-c to stop");
            dm_loop(ctx, api, engine, vec![channel], DmMode::Tail, false, false).await
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
            dm_loop(ctx, api, engine, Vec::new(), DmMode::Setup, false, false).await
        }
        DmCmd::Rejoin { channel_id } => {
            let channel = parse_uuid(&channel_id)?;
            let mut engine = dm_engine(&mut ctx, &api)?;
            engine.load_participants(&channel)?;
            dm_loop(
                ctx,
                api,
                engine,
                vec![channel.clone()],
                DmMode::Rejoin(channel),
                false,
                false,
            )
            .await
        }
        DmCmd::Backup { action } => {
            let mut engine = dm_engine(&mut ctx, &api)?;
            let outcomes = match action.as_str() {
                "on" => engine.set_backup(true).await?,
                "off" => engine.set_backup(false).await?,
                "status" => {
                    let usage = api.history_usage().await?;
                    vec![Outcome::Print(format!(
                        "backup {}; {} of {} bytes used on {}",
                        if engine.backup_enabled()? {
                            "on"
                        } else {
                            "off"
                        },
                        usage.bytes_used,
                        usage.bytes_quota,
                        api.host()
                    ))]
                }
                other => return Err(ClientError::State(format!("unknown action {other}"))),
            };
            for o in outcomes {
                if let Outcome::Print(s) = o {
                    println!("{s}");
                }
            }
            Ok(())
        }
        DmCmd::Restore => {
            let mut engine = dm_engine(&mut ctx, &api)?;
            for o in engine.restore().await? {
                if let Outcome::Print(s) = o {
                    println!("{s}");
                }
            }
            Ok(())
        }
    }
}

/// Opens (or finds) the conversation with `others` and, when this device
/// is the one that should create its group (the opener of a new one, or
/// the accepter of a request), creates it and welcomes everyone.
async fn open_dm(
    mut ctx: Ctx,
    api: Api,
    others: Vec<Vec<u8>>,
    accepter: bool,
) -> Result<(), ClientError> {
    let opened = api.open_personal_dm(others).await?;
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
    let create = accepter || !opened.existed;
    if engine.is_member(&channel.channel_id)? {
        println!("already a member");
        return Ok(());
    }
    if !create {
        println!("waiting for the welcome from the device that created it");
    }
    dm_loop(
        ctx,
        api,
        engine,
        vec![channel.channel_id.clone()],
        DmMode::Open(channel.channel_id),
        create,
        false,
    )
    .await
}

/// Runs the gateway with the DM engine attached until `mode` is satisfied.
/// `create` means this device opened the channel and creates the group.
async fn dm_loop(
    mut ctx: Ctx,
    api: Api,
    mut engine: DmEngine,
    channels: Vec<Vec<u8>>,
    mode: DmMode,
    create: bool,
    crash_after_persist: bool,
) -> Result<(), ClientError> {
    let (tx, rx) = mpsc::unbounded();
    let (etx, mut erx) = mpsc::unbounded();
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
                ev = erx.next() => ev,
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
                        DmMode::Rejoin(channel) => out.extend(engine.rejoin(channel).await?),
                        DmMode::Delete(channel, message) => {
                            out.extend(engine.send_delete(channel, message.clone())?);
                        }
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
                    Outcome::BackupKeyHandoff(handoff) => {
                        match engine.accept_backup_key_handoff(&handoff) {
                            Ok(Some(key)) => {
                                ctx.keystore.put("backup_key", &key)?;
                                println!("-- the backup key of a root rotation arrived");
                            }
                            Ok(None) => {}
                            Err(e) => println!("-- backup key handoff refused: {e}"),
                        }
                    }
                    Outcome::Send(cmd) => {
                        if crash_after_persist && matches!(mode, DmMode::Send(..)) {
                            println!("-- crashing between the persist and the send");
                            std::process::exit(3);
                        }
                        tx.unbounded_send(GatewayCommand::Send(cmd))
                            .map_err(|e| ClientError::Transport(e.to_string()))?;
                    }
                    Outcome::Subscribe(ch) => {
                        println!("-- personal DM {} added", wire::display(&ch));
                        tx.unbounded_send(GatewayCommand::Subscribe(ch))
                            .map_err(|e| ClientError::Transport(e.to_string()))?;
                    }
                    Outcome::Operation {
                        seq, sender, op, ..
                    } => print_operation(seq, &sender, &op),
                    Outcome::GroupReady(ch) => {
                        println!("-- group ready for {}", wire::display(&ch));
                        if matches!(&mode, DmMode::Open(c) | DmMode::Rejoin(c) if *c == ch) {
                            done = true;
                        }
                    }
                    Outcome::Delivered(id) => {
                        println!("delivered {}", wire::display(&id));
                        if matches!(mode, DmMode::Send(..) | DmMode::Delete(..)) {
                            done = true;
                        }
                    }
                }
            }
            if done && !engine.has_pending() {
                break Ok(());
            }
        }
    }
    .await;
    drop(tx);
    gw.abort();
    // Idle: write what this run decrypted to the history backup (spec section 9).
    if result.is_ok() {
        match engine.backup_flush().await {
            Ok(outcomes) => {
                for o in outcomes {
                    if let Outcome::Print(s) = o {
                        println!("-- {s}");
                    }
                }
            }
            Err(e) => eprintln!("backup: {e}"),
        }
    }
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
        IdentityCmd::Profile {
            display_name,
            pronouns,
            bio,
        } => {
            let signed = device(ctx).set_profile(&display_name, &pronouns, &bio, "", None)?;
            if api.tokens().access.is_some() {
                api.put_profile(signed).await?;
                println!("profile signed and sent");
            } else {
                println!("profile signed; it goes with the next account made by identity proof");
            }
            Ok(())
        }
        IdentityCmd::Show => {
            let verified = device(ctx).roster()?;
            println!("identity {}", hex::encode(&verified.identity_id));
            if let Some(signed) = device(ctx).profile()? {
                let body = lumia_core::proto::ProfileBody::decode(signed.body.as_slice())?;
                println!("profile v{}: {}", body.version, body.display_name);
            }
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
        IdentityCmd::Rotate { phrase } => {
            let rotated = device(ctx).rotate(api, &phrase).await?;
            println!("root rotated: roster v{} on {}", rotated.version, ctx.host);
            // The roster everywhere, then every instance's manifests under
            // the new backup key and its identity backup rewritten.
            publish_everywhere(ctx).await?;
            // The new backup key to the identity's other native devices,
            // through their account logs on every instance (spec 4.5 step 5).
            if let Some(handoff) = &rotated.handoff {
                for (host, base, tokens) in instances(ctx)? {
                    let api = Api::new(&base).with_tokens(tokens);
                    match api.publish_backup_key_handoff(handoff.clone()).await {
                        Ok(()) => println!("  {host}: backup key handed to the other devices"),
                        Err(e) => println!("  {host}: backup key not handed over ({e})"),
                    }
                }
            }
            if !rotated.old_backup_key.is_empty() {
                for (host, base, tokens) in instances(ctx)? {
                    let api = Api::new(&base).with_tokens(tokens);
                    let mut engine = dm_engine_at(ctx, &host, &api)?;
                    match engine.rewrap_backup(&rotated.old_backup_key).await {
                        Ok(outcomes) => {
                            for o in outcomes {
                                if let Outcome::Print(s) = o {
                                    println!("  {host}: {s}");
                                }
                            }
                        }
                        Err(e) => println!("  {host}: history not re-wrapped ({e})"),
                    }
                }
            }
            println!("new recovery phrase (shown once, write it down):");
            println!("{}", rotated.phrase);
            Ok(())
        }
        IdentityCmd::Recover { phrase, label } => {
            let tokens = device(ctx).recover(api, &phrase, &label).await?;
            save_tokens(ctx, &tokens)?;
            api.set_tokens(&tokens);
            let verified = device(ctx).roster()?;
            println!(
                "recovered: roster v{} with this device only, signed in on {}",
                verified.version, ctx.host
            );
            // The backup names the other hosts to sign in to (identity spec 8).
            match device(ctx).read_backup(api).await {
                Ok(backup) => {
                    for account in backup.accounts {
                        println!(
                            "  backup v{}: account {} on {}",
                            backup.version,
                            wire::display(&account.account_id),
                            account.host
                        );
                    }
                }
                Err(e) => println!("  no identity backup read here: {e}"),
            }
            Ok(())
        }
        IdentityCmd::Backup => {
            let accounts = host_accounts(ctx)?;
            let version = device(ctx).write_backup(api, accounts).await?;
            println!("identity backup v{version} written to {}", ctx.host);
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
    let accounts = host_accounts(ctx)?;
    for (host, base, tokens) in instances(ctx)? {
        let api = Api::new(&base).with_tokens(tokens);
        match api.publish_roster(roster.signed.clone()).await {
            Ok(()) => println!("  {host}: now v{}", roster.version),
            Err(e) => println!("  {host}: not updated ({e})"),
        }
        match device(ctx).write_backup(&api, accounts.clone()).await {
            Ok(v) => println!("  {host}: identity backup v{v}"),
            Err(e) => println!("  {host}: backup not written ({e})"),
        }
    }
    Ok(())
}

/// The accounts this device holds on every instance it knows, for the
/// identity backup: each instance store records its account id on connect.
fn host_accounts(ctx: &Ctx) -> Result<Vec<lumia_core::proto::HostAccount>, ClientError> {
    let mut out = Vec::new();
    for (_, base, _) in instances(ctx)? {
        let host = url::Url::parse(&base)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
            .unwrap_or_default();
        let dir = ctx.data_dir.join(host_of(&base));
        let store = SqliteStore::open(&dir.join("store.sqlite"))?;
        if let Some(hex_id) = store.meta_get("account_id")? {
            out.push(lumia_core::proto::HostAccount {
                host,
                account_id: hex::decode(hex_id).unwrap_or_default(),
            });
        }
    }
    Ok(out)
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
