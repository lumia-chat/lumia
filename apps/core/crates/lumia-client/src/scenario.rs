//! `lumia-cli scenario <name>`: the failure-injection cases a client can
//! drive on its own against a running instance (delivery spec 12, identity
//! spec 12, plan step 3). Each one sets up what it needs (a throwaway
//! server and channel, or throwaway identities under the scratch
//! directory), runs the case, and fails loudly with what it saw.
//!
//! Cases that need the server to misbehave (out-of-order broadcast, a
//! purged command row, a generation bump, a partition) stay server-side
//! tests in `apps/instance`.

#![allow(clippy::print_stdout)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use lumia_core::proto::{
    client_frame, command, server_frame, ClientFrame, Command, DisconnectReason, Heartbeat,
    SendMessage,
};
use lumia_core::sync::{decode_frame, encode_frame};
use lumia_core::wire;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMessage;

use lumia_client::api::{Api, Tokens};
use lumia_client::gateway::{self, GatewayCommand, GatewayEvent};
use lumia_client::identity::Device;
use lumia_client::keystore::FileKeystore;
use lumia_client::sqlite::SqliteStore;
use lumia_client::ClientError;
use lumia_core::keystore::Keystore;
use lumia_core::store::Store;

/// Every scenario by name, in the order `all` runs them.
pub const NAMES: &[&str] = &[
    "token-rotation",
    "frame-before-hello",
    "duplicate-command",
    "snapshot-under-writes",
    "snapshot-threshold",
    "stale-roster",
    "device-revoke",
    "roster-rules",
    "last-resort",
    "reconnect",
];

/// What a scenario needs from the CLI.
#[derive(Debug)]
pub struct Env {
    /// Signed in to the instance under test.
    pub api: Api,
    /// This device's id.
    pub device_id: Vec<u8>,
    /// Where this instance's tokens are persisted, to check rotation.
    pub keys_dir: PathBuf,
    /// A directory for throwaway state; created and reused.
    pub scratch: PathBuf,
}

/// Runs one scenario, or every one for `all`. Returns the first failure.
///
/// # Errors
/// A [`ClientError::State`] saying what was expected and what happened,
/// or the transport or API error that stopped the case.
pub async fn run(name: &str, env: &Env) -> Result<(), ClientError> {
    if name == "all" {
        let mut failed = Vec::new();
        for n in NAMES {
            println!("== {n}");
            match run_one(n, env).await {
                Ok(()) => println!("   PASS"),
                Err(e) => {
                    println!("   FAIL {e}");
                    failed.push(*n);
                }
            }
        }
        return if failed.is_empty() {
            Ok(())
        } else {
            Err(ClientError::State(format!("failed: {}", failed.join(", "))))
        };
    }
    run_one(name, env).await
}

async fn run_one(name: &str, env: &Env) -> Result<(), ClientError> {
    std::fs::create_dir_all(&env.scratch).map_err(|e| ClientError::Transport(e.to_string()))?;
    match name {
        "token-rotation" => token_rotation(env).await,
        "frame-before-hello" => frame_before_hello(env).await,
        "duplicate-command" => duplicate_command(env).await,
        "snapshot-under-writes" => snapshot_under_writes(env).await,
        "snapshot-threshold" => snapshot_threshold(env).await,
        "stale-roster" => stale_roster(env).await,
        "device-revoke" => device_revoke(env).await,
        "roster-rules" => roster_rules(env).await,
        "last-resort" => last_resort(env).await,
        "reconnect" => reconnect(env).await,
        other => Err(ClientError::State(format!(
            "unknown scenario {other}; one of: all, {}",
            NAMES.join(", ")
        ))),
    }
}

fn fail(msg: impl Into<String>) -> ClientError {
    ClientError::State(msg.into())
}

fn check(ok: bool, msg: impl Into<String>) -> Result<(), ClientError> {
    if ok {
        Ok(())
    } else {
        Err(fail(msg))
    }
}

// ---- Tokens -------------------------------------------------------------------------

/// A refused access token with a refresh token on hand rotates the pair
/// once, retries the call, and persists the new pair (protocol section 9,
/// `POST /auth/refresh`).
async fn token_rotation(env: &Env) -> Result<(), ClientError> {
    let before = env.api.tokens();
    let refresh = before
        .refresh
        .clone()
        .ok_or_else(|| fail("sign in with a refresh token first"))?;
    let api = env.api.clone().with_tokens(Tokens {
        access: Some("not-a-token".into()),
        refresh: Some(refresh),
    });
    api.servers().await?;
    let after = api.tokens();
    check(
        after.access.as_deref() != Some("not-a-token"),
        "the access token was not rotated",
    )?;
    check(
        after.refresh != before.refresh,
        "the refresh token was not rotated",
    )?;
    let persisted = FileKeystore::open(&env.keys_dir)?
        .get("access_token")?
        .and_then(|b| String::from_utf8(b).ok());
    check(
        persisted == after.access,
        "the rotated access token was not persisted to the keystore",
    )?;
    println!("   rotated and persisted a new pair after a refused access token");
    Ok(())
}

// ---- Gateway framing ----------------------------------------------------------------

/// A client frame before `Hello` is a protocol error and the socket closes
/// (protocol section 6, delivery spec 12).
async fn frame_before_hello(env: &Env) -> Result<(), ClientError> {
    let (ws, _) = tokio_tungstenite::connect_async(env.api.gateway_url()?).await?;
    let (mut sink, mut stream) = ws.split();
    let frame = ClientFrame {
        frame: Some(client_frame::Frame::Heartbeat(Heartbeat { nonce: 1 })),
    };
    sink.send(WsMessage::Binary(encode_frame(&frame).into()))
        .await?;
    let mut disconnect = None;
    let outcome = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(message) = stream.next().await {
            match message {
                Ok(WsMessage::Binary(bytes)) => {
                    if let Some(server_frame::Frame::Disconnect(d)) = decode_frame(&bytes)?.frame {
                        disconnect = DisconnectReason::try_from(d.reason).ok();
                    }
                }
                Ok(WsMessage::Close(_)) | Err(_) => return Ok::<_, ClientError>(true),
                Ok(_) => {}
            }
        }
        Ok(true)
    })
    .await;
    check(
        matches!(outcome, Ok(Ok(true))),
        "the socket stayed open for 5s after a frame before Hello",
    )?;
    check(
        disconnect == Some(DisconnectReason::DisconnectProtocolError),
        format!("expected DISCONNECT_PROTOCOL_ERROR, got {disconnect:?}"),
    )?;
    println!("   DISCONNECT_PROTOCOL_ERROR, then close");
    Ok(())
}

// ---- Idempotent commands ------------------------------------------------------------

/// The same command from two connections at once yields one event, one
/// seq, and the same ack to both (protocol section 7, delivery spec 12).
async fn duplicate_command(env: &Env) -> Result<(), ClientError> {
    let channel = fresh_channel(&env.api).await?;
    let id = wire::display(&channel);
    let cmd = message(&channel, "once");
    let (a, b) = tokio::join!(
        env.api.post_command(&id, &cmd),
        env.api.post_command(&id, &cmd)
    );
    let (a, b) = (a?, b?);
    check(
        a.seq == b.seq && a.event_id == b.event_id,
        format!("acks differ: seq {} vs {}", a.seq, b.seq),
    )?;
    let page = env.api.channel_events(&id, 0, 0).await?;
    check(
        page.events.len() == 1 && page.events[0].seq == a.seq,
        format!(
            "expected one event at seq {}, got {}",
            a.seq,
            page.events.len()
        ),
    )?;
    println!("   both acks at seq {}, one event in the log", a.seq);
    Ok(())
}

// ---- Snapshots ----------------------------------------------------------------------

/// A snapshot taken under concurrent writes has every message at
/// `seq <= snapshot.seq`, and the events after it fill the rest exactly
/// once (delivery spec 7 and 12).
async fn snapshot_under_writes(env: &Env) -> Result<(), ClientError> {
    const WRITES: u64 = 300;
    let channel = fresh_channel(&env.api).await?;
    let id = wire::display(&channel);
    let written = Arc::new(AtomicU64::new(0));
    let writer = {
        let api = env.api.clone();
        let channel = channel.clone();
        let id = id.clone();
        let written = Arc::clone(&written);
        tokio::spawn(async move {
            for i in 0..WRITES {
                api.post_command(&id, &message(&channel, &format!("m{i}")))
                    .await?;
                written.fetch_add(1, Ordering::SeqCst);
            }
            Ok::<_, ClientError>(())
        })
    };
    while written.load(Ordering::SeqCst) < 50 {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let snap = env.api.channel_snapshot(&id).await?;
    writer
        .await
        .map_err(|e| ClientError::Transport(e.to_string()))??;

    check(
        snap.messages.iter().all(|m| m.seq <= snap.seq),
        "a snapshot message has seq > snapshot.seq",
    )?;
    check(
        snap.seq < WRITES,
        format!("the snapshot at seq {} did not race the writer", snap.seq),
    )?;
    let mut after = snap.seq;
    let mut fetched = 0u64;
    loop {
        let page = env.api.channel_events(&id, after, 0).await?;
        if page.events.is_empty() {
            break;
        }
        for e in &page.events {
            check(
                e.seq == after + 1,
                format!("gap or duplicate: got seq {} after {after}", e.seq),
            )?;
            after = e.seq;
            fetched += 1;
        }
    }
    check(
        after == WRITES && fetched == WRITES - snap.seq,
        format!(
            "after the snapshot at {} fetched {fetched} events up to {after}",
            snap.seq
        ),
    )?;
    println!(
        "   snapshot at seq {} with {} messages; {fetched} events after it, contiguous to {WRITES}",
        snap.seq,
        snap.messages.len()
    );
    Ok(())
}

/// Subscribing 1,000 behind the head replays; 1,001 behind takes the
/// snapshot path (delivery spec 8 and 12).
async fn snapshot_threshold(env: &Env) -> Result<(), ClientError> {
    const CAP: u64 = 1_000;
    let channel = fresh_channel(&env.api).await?;
    let id = wire::display(&channel);
    for i in 0..CAP {
        env.api
            .post_command(&id, &message(&channel, &format!("m{i}")))
            .await?;
    }
    let (applied, snapshot) = observe(env, &channel, CAP).await?;
    check(
        applied == CAP && snapshot.is_none(),
        format!("{CAP} behind: expected replay of {CAP}, got {applied} applied and snapshot {snapshot:?}"),
    )?;
    println!("   {CAP} behind: replayed {applied}");

    env.api
        .post_command(&id, &message(&channel, "one more"))
        .await?;
    let (applied, snapshot) = observe(env, &channel, CAP + 1).await?;
    check(
        snapshot == Some(CAP + 1),
        format!("{} behind: expected the snapshot at {}, got {applied} applied and snapshot {snapshot:?}", CAP + 1, CAP + 1),
    )?;
    println!("   {} behind: snapshot at seq {}", CAP + 1, CAP + 1);
    Ok(())
}

/// Subscribes a fresh (empty) store to `channel` and reports how many
/// events were applied and whether a snapshot was taken, once the client
/// has caught up to `head` or 20s pass.
async fn observe(env: &Env, channel: &[u8], head: u64) -> Result<(u64, Option<u64>), ClientError> {
    let (tx, mut rx) = mpsc::channel::<GatewayCommand>(1);
    let (etx, mut erx) = mpsc::channel(256);
    let api = env.api.clone();
    let device_id = env.device_id.clone();
    let mut channels = vec![channel.to_vec()];
    let task = tokio::spawn(async move {
        let mut store = SqliteStore::in_memory()?;
        gateway::run(
            &api,
            &mut store,
            "lumia-cli/scenario",
            &device_id,
            &mut channels,
            &mut rx,
            &etx,
        )
        .await?;
        let cursor = store
            .cursors(lumia_core::store::LogKind::Channel)?
            .first()
            .map_or(0, |(_, s)| *s);
        Ok::<_, ClientError>(cursor)
    });
    let mut applied = 0u64;
    let mut snapshot = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        let ev = tokio::time::timeout_at(deadline, erx.recv()).await;
        match ev {
            Ok(Some(GatewayEvent::Applied(events))) => applied += events.len() as u64,
            Ok(Some(GatewayEvent::SnapshotTaken { seq, .. })) => snapshot = Some(seq),
            Ok(Some(GatewayEvent::Closed(reason))) => {
                return Err(fail(format!("closed: {reason}")))
            }
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(_) => break,
        }
        if applied + snapshot.unwrap_or(0) >= head {
            break;
        }
    }
    drop(tx);
    let cursor = task
        .await
        .map_err(|e| ClientError::Transport(e.to_string()))??;
    check(
        cursor == head,
        format!("the store's cursor is {cursor}, head is {head}"),
    )?;
    Ok((applied, snapshot))
}

// ---- Identity -----------------------------------------------------------------------

/// A revoked device that presents its older roster gets the held one back,
/// adopts it, and sees itself revoked (identity spec 5, "older roster").
async fn stale_roster(env: &Env) -> Result<(), ClientError> {
    let base = env.api.base().to_string();
    let anon = Api::new(&base);
    let mut a = Throwaway::open(&env.scratch, "stale-a")?;
    let mut b = Throwaway::open(&env.scratch, "stale-b")?;

    let created = a.device().create("a")?;
    let tokens = a.device().create_account(&anon, &handle("stale")).await?;
    let api_a = Api::new(&base).with_tokens(pair(&tokens));

    link(&mut a, &mut b, &api_a, &anon).await?;
    b.device().sign_in(&anon).await?;
    println!("   b linked (v2) and signed in");

    a.device()
        .revoke_with_phrase(&api_a, &created.phrase, &b.device_id)
        .await?;
    match b.device().sign_in(&anon).await {
        Err(ClientError::DeviceRevoked { roster_version: 3 }) => {}
        other => return Err(fail(format!("expected DeviceRevoked at v3, got {other:?}"))),
    }
    let held = b.device().roster()?;
    check(
        held.version == 3 && held.revoked.contains_key(&b.device_id),
        "b did not adopt the held roster",
    )?;
    println!("   b presented v2, adopted held v3, and is revoked in it");
    Ok(())
}

/// A device revokes another with its own key under reauth by password
/// (identity spec 4.3 and 10), after a password account was linked to the
/// identity under reauth (`POST /identity/link`).
async fn device_revoke(env: &Env) -> Result<(), ClientError> {
    let base = env.api.base().to_string();
    let anon = Api::new(&base);
    let mut a = Throwaway::open(&env.scratch, "revoke-a")?;
    let mut b = Throwaway::open(&env.scratch, "revoke-b")?;
    let password = "correct horse battery staple";

    let tokens = anon.create_account(&handle("revoke"), password).await?;
    let api_a = Api::new(&base).with_tokens(pair(&tokens));
    a.device().create("a")?;
    a.device().link_account(&api_a, password).await?;
    println!("   password account linked to a's identity under reauth");

    link(&mut a, &mut b, &api_a, &anon).await?;
    b.device().sign_in(&anon).await?;

    let b_id = b.device_id.clone();
    match a.device().revoke_with_device(&api_a, "wrong", &b_id).await {
        Err(ClientError::Api { code, .. }) if code == "UNAUTHENTICATED" => {}
        other => {
            return Err(fail(format!(
                "expected UNAUTHENTICATED for a wrong password, got {other:?}"
            )))
        }
    }
    a.device()
        .revoke_with_device(&api_a, password, &b_id)
        .await?;
    match b.device().sign_in(&anon).await {
        Err(ClientError::DeviceRevoked { roster_version: 3 }) => {}
        other => return Err(fail(format!("expected DeviceRevoked at v3, got {other:?}"))),
    }
    println!("   a revoked b with its device key and a password reauth; b is out");
    Ok(())
}

/// Key-package exhaustion (e2ee spec sections 3 and 12): a device's one
/// package is claimed and deleted, the pool is then empty so the
/// last-resort package is served, served again (never deleted), and
/// replaced by the device's next publish.
async fn last_resort(env: &Env) -> Result<(), ClientError> {
    let base = env.api.base().to_string();
    let anon = Api::new(&base);
    let mut a = Throwaway::open(&env.scratch, "lr-a")?;
    let mut b = Throwaway::open(&env.scratch, "lr-b")?;
    a.device().create("a")?;
    let tokens = a.device().create_account(&anon, &handle("lr")).await?;
    let api_a = Api::new(&base).with_tokens(pair(&tokens));
    link(&mut a, &mut b, &api_a, &anon).await?;
    let tokens_b = b.device().sign_in(&anon).await?;
    let api_b = Api::new(&base).with_tokens(pair(&tokens_b));
    let account = api_a
        .account_snapshot()
        .await?
        .account
        .unwrap_or_default()
        .account_id;

    // The instance stores packages as opaque bytes, so labels will do.
    api_b
        .publish_key_packages(vec![b"kp-1".to_vec()], b"last-1".to_vec())
        .await?;
    let claim = |label: &'static str| {
        let api_a = api_a.clone();
        let account = account.clone();
        async move {
            let resp = api_a.claim_key_packages(account).await?;
            let packages: Vec<Vec<u8>> = resp
                .key_packages
                .into_iter()
                .map(|c| c.key_package)
                .collect();
            check(
                packages.len() == 1,
                format!(
                    "{label}: expected one package for B, got {}",
                    packages.len()
                ),
            )?;
            Ok::<_, ClientError>(packages.into_iter().next().unwrap_or_default())
        }
    };
    check(
        claim("first claim").await? == b"kp-1",
        "the pool's package was not served first",
    )?;
    check(
        claim("second claim").await? == b"last-1",
        "an empty pool did not serve the last-resort package",
    )?;
    check(
        claim("third claim").await? == b"last-1",
        "the last-resort package was deleted on claim",
    )?;
    println!("   one package claimed and gone; the last-resort package served twice and kept");

    api_b
        .publish_key_packages(vec![], b"last-2".to_vec())
        .await?;
    check(
        claim("after top-up").await? == b"last-2",
        "the next publish did not replace the last-resort package",
    )?;
    println!("   the next publish replaced it");
    Ok(())
}

/// A call that must succeed, with the error's details when it does not.
fn must(result: Result<(), ClientError>, what: &str) -> Result<(), ClientError> {
    match result {
        Ok(()) => Ok(()),
        Err(ClientError::Api { code, details, .. }) => {
            Err(fail(format!("{what}: {code} {details:?}")))
        }
        Err(e) => Err(fail(format!("{what}: {e}"))),
    }
}

/// The verifier's rules at the instance, driven with hand-built rosters
/// (identity spec 12): a certificate signed by a device revoked before its
/// `added_at` is refused at the certificate step, one signed before the
/// revocation is accepted, a future-dated certificate is refused, a roster
/// that drops a held revocation is refused at the monotonic step, and a
/// root rotation keeps old certificates valid and lets the device sign in.
async fn roster_rules(env: &Env) -> Result<(), ClientError> {
    use lumia_core::identity::keys::{generate_signing_key, signing_key};
    use lumia_core::identity::phrase::Phrase;
    use lumia_core::identity::roster::{
        device_entry, revoked_entry, roster_body, rotation, sign_roster, NewDevice, Signer,
    };
    use lumia_core::proto::DeviceKind;

    let base = env.api.base().to_string();
    let anon = Api::new(&base);
    let mut a = Throwaway::open(&env.scratch, "rules-a")?;
    let created = a.device().create("a")?;
    let tokens = a.device().create_account(&anon, &handle("rules")).await?;
    let api_a = Api::new(&base).with_tokens(pair(&tokens));

    let derived = Phrase::parse(&created.phrase)?.derive();
    let identity = derived.identity_id();
    let root = derived.root;
    let d1_key = signing_key(&a.keys.get("device_key")?.unwrap_or_default())?;
    let d1 = a.device_id.clone();
    let v1 = a.device().roster()?;
    let d1_entry = v1.body.devices[0].clone();

    let ts = |offset_secs: i64| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        prost_types::Timestamp {
            seconds: i64::try_from(now.as_secs()).unwrap_or(i64::MAX) + offset_secs,
            nanos: 0,
        }
    };
    let new_device = |id: &[u8], key: &ed25519_dalek::SigningKey, added_at| NewDevice {
        device_id: id.to_vec(),
        public_key: key.verifying_key().to_bytes().to_vec(),
        kind: DeviceKind::Native,
        web_origin: String::new(),
        label: "scenario".into(),
        added_at,
    };
    let expect_rejection = |result: Result<(), ClientError>, step: &str, reason: &str| match result
    {
        Err(ClientError::Api { code, details, .. })
            if code == "IDENTITY_PROOF_INVALID"
                && details.get("step").map(String::as_str) == Some(step)
                && details.get("reason").map(String::as_str) == Some(reason) =>
        {
            Ok(())
        }
        other => Err(fail(format!(
            "expected rejection at {step}/{reason}, got {other:?}"
        ))),
    };

    // v2: D1 adds D2. v3: the root revokes D2 at T.
    let d2_key = generate_signing_key()?;
    let d2 = wire::new_id();
    let d2_entry = device_entry(
        &identity,
        &new_device(&d2, &d2_key, ts(1)),
        Signer::Device(&d1, &d1_key),
    );
    let v2 = roster_body(
        &identity,
        2,
        &v1.root_public_key,
        vec![],
        vec![d1_entry.clone(), d2_entry.clone()],
        vec![],
        ts(0),
    );
    must(
        api_a
            .publish_roster(sign_roster(&v2, Signer::Device(&d1, &d1_key)))
            .await,
        "publishing v2",
    )?;
    let revoked_at = ts(60);
    let d2_revoked = revoked_entry(
        &identity,
        &d2,
        &d2_key.verifying_key().to_bytes(),
        ts(1),
        revoked_at,
        Signer::Root(&root),
    );
    let v3 = roster_body(
        &identity,
        3,
        &v1.root_public_key,
        vec![],
        vec![d1_entry.clone()],
        vec![d2_revoked.clone()],
        ts(0),
    );
    must(
        api_a
            .publish_roster(sign_roster(&v3, Signer::Root(&root)))
            .await,
        "publishing v3",
    )?;
    println!("   v2 added D2, v3 revoked it by the root");

    // A roster that forgets D2's revocation: refused at the monotonic step.
    // (Checked here, where every certificate still verifies, so the
    // rejection is the monotonic one and not an unknown signer.)
    let bad = roster_body(
        &identity,
        4,
        &v1.root_public_key,
        vec![],
        vec![d1_entry.clone()],
        vec![],
        ts(0),
    );
    expect_rejection(
        api_a
            .publish_roster(sign_roster(&bad, Signer::Device(&d1, &d1_key)))
            .await,
        "monotonic",
        "revocation_dropped",
    )?;
    println!("   a roster that drops a held revocation is refused (monotonic/revocation_dropped)");

    // A certificate D2 signed after its revocation: refused at the certificate step.
    let d3_key = generate_signing_key()?;
    let d3 = wire::new_id();
    let late = device_entry(
        &identity,
        &new_device(&d3, &d3_key, ts(90)),
        Signer::Device(&d2, &d2_key),
    );
    let bad = roster_body(
        &identity,
        4,
        &v1.root_public_key,
        vec![],
        vec![d1_entry.clone(), late],
        vec![d2_revoked.clone()],
        ts(0),
    );
    expect_rejection(
        api_a
            .publish_roster(sign_roster(&bad, Signer::Device(&d1, &d1_key)))
            .await,
        "certs",
        "signer",
    )?;
    println!("   a certificate signed by D2 after its revocation is refused (certs/signer)");

    // The same device certified before the revocation: accepted.
    let early = device_entry(
        &identity,
        &new_device(&d3, &d3_key, ts(30)),
        Signer::Device(&d2, &d2_key),
    );
    let v4 = roster_body(
        &identity,
        4,
        &v1.root_public_key,
        vec![],
        vec![d1_entry.clone(), early.clone()],
        vec![d2_revoked.clone()],
        ts(0),
    );
    must(
        api_a
            .publish_roster(sign_roster(&v4, Signer::Device(&d1, &d1_key)))
            .await,
        "publishing v4",
    )?;
    println!("   a certificate signed by D2 before its revocation is accepted (v4)");

    // A certificate dated beyond the allowed skew: refused.
    let d4_key = generate_signing_key()?;
    let d4 = wire::new_id();
    let future = device_entry(
        &identity,
        &new_device(&d4, &d4_key, ts(600)),
        Signer::Device(&d1, &d1_key),
    );
    let bad = roster_body(
        &identity,
        5,
        &v1.root_public_key,
        vec![],
        vec![d1_entry.clone(), early.clone(), future],
        vec![d2_revoked.clone()],
        ts(0),
    );
    expect_rejection(
        api_a
            .publish_roster(sign_roster(&bad, Signer::Device(&d1, &d1_key)))
            .await,
        "certs",
        "future_dated",
    )?;
    println!("   a future-dated certificate is refused (certs/future_dated)");

    // Root rotation: old certificates stay valid, and D1 signs in under it.
    let new_root = Phrase::generate()?.derive().root;
    let new_public = new_root.verifying_key().to_bytes();
    let v5 = roster_body(
        &identity,
        5,
        &new_public,
        vec![rotation(&identity, &root, &new_public, ts(0))],
        vec![d1_entry, early],
        vec![d2_revoked],
        ts(0),
    );
    must(
        api_a
            .publish_roster(sign_roster(&v5, Signer::Root(&new_root)))
            .await,
        "publishing v5",
    )?;
    // D1 still holds v1 locally: sign-in reconciles to v5 and succeeds.
    a.device().sign_in(&anon).await?;
    let held = a.device().roster()?;
    check(
        held.version == 5 && held.root_public_key == new_public && held.roots.len() == 2,
        format!(
            "expected D1 to adopt v5 under the rotated root, got v{} with {} roots",
            held.version,
            held.roots.len()
        ),
    )?;
    println!(
        "   root rotated at v5: D1's old certificate verifies and D1 signed in under the new root"
    );
    Ok(())
}

async fn link(
    a: &mut Throwaway,
    b: &mut Throwaway,
    api_a: &Api,
    anon: &Api,
) -> Result<(), ClientError> {
    let request = b.device().link_request("b")?;
    let code = a.device().approve(api_a, &request).await?;
    b.device().claim(anon, &code).await?;
    Ok(())
}

/// A throwaway device: its own store and keystore under the scratch dir.
struct Throwaway {
    store: SqliteStore,
    keys: FileKeystore,
    device_id: Vec<u8>,
}

impl Throwaway {
    fn open(scratch: &Path, name: &str) -> Result<Self, ClientError> {
        let dir = scratch.join(format!("{name}-{}", wire::display(&wire::new_id())));
        std::fs::create_dir_all(&dir).map_err(|e| ClientError::Transport(e.to_string()))?;
        Ok(Self {
            store: SqliteStore::open(&dir.join("meta.sqlite"))?,
            keys: FileKeystore::open(&dir.join("keys"))?,
            device_id: wire::new_id(),
        })
    }

    fn device(&mut self) -> Device<'_, SqliteStore, FileKeystore> {
        Device {
            store: &mut self.store,
            keystore: &mut self.keys,
            device_id: self.device_id.clone(),
        }
    }
}

// ---- Reconnect ----------------------------------------------------------------------

/// Against a closed port, the loop retries with growing delays and stops
/// when the application drops its sender.
async fn reconnect(env: &Env) -> Result<(), ClientError> {
    let api = Api::new("http://127.0.0.1:9").with_token(Some("x".into()));
    let (tx, rx) = mpsc::channel::<GatewayCommand>(1);
    let (etx, mut erx) = mpsc::channel(16);
    let device_id = env.device_id.clone();
    let task = tokio::spawn(async move {
        let store = SqliteStore::in_memory()?;
        gateway::run_forever(api, store, "lumia-cli/scenario", device_id, vec![], rx, etx).await;
        Ok::<_, ClientError>(())
    });
    let mut delays = Vec::new();
    while delays.len() < 3 {
        match tokio::time::timeout(Duration::from_secs(10), erx.recv()).await {
            Ok(Some(GatewayEvent::Reconnecting { attempt, delay, .. })) => {
                println!("   attempt {attempt} in {delay:?}");
                delays.push(delay);
            }
            Ok(Some(GatewayEvent::Closed(reason))) => {
                return Err(fail(format!("closed: {reason}")))
            }
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => return Err(fail("the loop stopped on its own")),
        }
    }
    check(
        delays[0] < delays[1] && delays[1] < delays[2],
        format!("delays did not grow: {delays:?}"),
    )?;
    drop(tx);
    task.await
        .map_err(|e| ClientError::Transport(e.to_string()))??;
    println!("   stopped once the sender was dropped");
    Ok(())
}

// ---- Fixtures -----------------------------------------------------------------------

async fn fresh_channel(api: &Api) -> Result<Vec<u8>, ClientError> {
    let server = api.create_server(&handle("scenario")).await?;
    let channel = api.create_channel(server.server_id, "general").await?;
    Ok(channel.channel_id)
}

fn message(channel: &[u8], body: &str) -> Command {
    Command {
        command_id: wire::new_id(),
        channel_id: channel.to_vec(),
        op: Some(command::Op::SendMessage(SendMessage {
            body: body.into(),
            ..Default::default()
        })),
    }
}

fn handle(prefix: &str) -> String {
    let id = wire::display(&wire::new_id());
    format!("{prefix}{}", &id[id.len() - 8..])
}

fn pair(tokens: &lumia_core::proto::TokenPair) -> Tokens {
    Tokens {
        access: Some(tokens.access_token.clone()),
        refresh: Some(tokens.refresh_token.clone()),
    }
}
