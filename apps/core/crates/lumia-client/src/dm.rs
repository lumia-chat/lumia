//! The personal-DM engine (docs/specs/e2ee.md sections 3 to 6): one per
//! instance, driving the core's [`Conversation`]s from the gateway's
//! events and producing the commands the gateway must send.
//!
//! It owns its own connection to the instance store so that MLS state, the
//! decrypted log and the outbox entry for a send commit in one savepoint
//! (spec section 7). It never talks to the socket: the application feeds
//! it [`GatewayEvent`]s and carries out the [`Outcome`]s it returns.

use std::collections::{BTreeMap, HashMap, HashSet};

use lumia_core::identity::keys::ts_nanos;
use lumia_core::identity::roster::{verify_roster, Verified};
use lumia_core::mls::credential::{device_signer, LeafIdentity};
use lumia_core::mls::group::{Commit, Conversation, Incoming};
use lumia_core::mls::key_packages::{new_key_package, parse_key_package};
use lumia_core::mls::storage::Provider;
use lumia_core::mls::{
    MlsError, SignatureKeyPair, KEY_PACKAGE_LIFETIME_SECS, KEY_PACKAGE_POOL,
    KEY_PACKAGE_TOP_UP_BELOW,
};
use lumia_core::proto::{
    account_event, command, dm_operation, event, AccountEvent, Channel, ChannelKind, Command,
    DmOperation, DmSend, EncryptedKind, EncryptedPayload, Event, SendEncrypted, Welcome,
};
use lumia_core::store::{OutboxEntry, Store as _};
use lumia_core::sync::Notice;
use lumia_core::wire;
use prost::Message;

use crate::api::Api;
use crate::sqlite::{DmRow, DmStatus, SqliteStore};
use crate::ClientError;

/// What the application must do after feeding the engine.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Something to show.
    Print(String),
    /// Send this command on the gateway (it is already in the outbox).
    Send(Command),
    /// Subscribe to this channel: a personal DM the account can now see.
    Subscribe(Vec<u8>),
    /// A decrypted operation at `seq`.
    Operation {
        /// The conversation.
        channel_id: Vec<u8>,
        /// Position in its log.
        seq: u64,
        /// Who sent it.
        sender: LeafIdentity,
        /// The operation.
        op: DmOperation,
    },
    /// The group for this conversation exists and its welcome was acked.
    GroupReady(Vec<u8>),
    /// This device's message with `operation_id` was acked.
    Delivered(Vec<u8>),
}

/// A command the engine is waiting on.
#[derive(Debug, Clone)]
enum Pending {
    /// A commit; on the ack, merge and send the welcome.
    Commit {
        channel_id: Vec<u8>,
        welcome: Option<Vec<u8>>,
        epoch: u64,
        /// Whether this commit created the group (drop it on refusal).
        creating: bool,
    },
    /// A welcome; on the ack, the group is ready.
    Welcome { channel_id: Vec<u8> },
    /// An application message; on the ack, recorded at its seq.
    Message {
        channel_id: Vec<u8>,
        op: Vec<u8>,
        epoch: u64,
    },
}

/// The engine for one device on one instance.
pub struct DmEngine {
    api: Api,
    store: SqliteStore,
    my_account: Vec<u8>,
    me: LeafIdentity,
    signer: SignatureKeyPair,
    /// Rosters by identity id, this device's own included.
    rosters: HashMap<Vec<u8>, Verified>,
    /// Account id to identity id, for every participant seen.
    account_identity: HashMap<Vec<u8>, Vec<u8>>,
    /// Participant account ids per personal channel.
    participants: HashMap<Vec<u8>, Vec<Vec<u8>>>,
    pending: HashMap<Vec<u8>, Pending>,
    /// Operations this session sent, by command id, so the sender's own
    /// events can be recorded when they come back through the log.
    sent: HashMap<Vec<u8>, (Vec<u8>, u64)>,
}

impl std::fmt::Debug for DmEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DmEngine")
            .field("account", &wire::display(&self.my_account))
            .field("device", &wire::display(&self.me.device_id))
            .field("channels", &self.participants.len())
            .finish_non_exhaustive()
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .filter_map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok())
        .collect()
}

fn processed_key(channel_id: &[u8]) -> String {
    format!("dm:processed:{}", hex(channel_id))
}

fn participants_key(channel_id: &[u8]) -> String {
    format!("dm:participants:{}", hex(channel_id))
}

fn now_ts() -> prost_types::Timestamp {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    prost_types::Timestamp {
        seconds: i64::try_from(d.as_secs()).unwrap_or(i64::MAX),
        nanos: i32::try_from(d.subsec_nanos()).unwrap_or(0),
    }
}

fn control(channel_id: &[u8], mls_message: Vec<u8>, epoch: u64) -> Command {
    Command {
        command_id: wire::new_id(),
        channel_id: channel_id.to_vec(),
        op: Some(command::Op::SendEncrypted(SendEncrypted {
            payload: Some(EncryptedPayload {
                kind: EncryptedKind::Control as i32,
                mls_message,
                epoch,
            }),
        })),
    }
}

fn is_personal(channel: &Channel) -> bool {
    matches!(
        ChannelKind::try_from(channel.kind),
        Ok(ChannelKind::PersonalDm | ChannelKind::PersonalGroupDm)
    )
}

impl DmEngine {
    /// An engine over its own connection to the instance store, for the
    /// device whose key seed, roster and account are given. Participants of
    /// known personal channels are rebuilt from the stored account log.
    ///
    /// # Errors
    /// On a store failure or a malformed device key.
    pub fn new(
        api: Api,
        store: SqliteStore,
        my_account: Vec<u8>,
        device_id: Vec<u8>,
        device_key_seed: &[u8],
        my_roster: Verified,
    ) -> Result<Self, ClientError> {
        let me = LeafIdentity {
            identity_id: my_roster.identity_id.clone(),
            device_id,
        };
        let mut rosters = HashMap::new();
        let mut account_identity = HashMap::new();
        account_identity.insert(my_account.clone(), me.identity_id.clone());
        rosters.insert(me.identity_id.clone(), my_roster);
        let mut engine = Self {
            api,
            store,
            my_account,
            me,
            signer: device_signer(device_key_seed)?,
            rosters,
            account_identity,
            participants: HashMap::new(),
            pending: HashMap::new(),
            sent: HashMap::new(),
        };
        for stored in engine.store.account_events()? {
            if let Ok(ev) = AccountEvent::decode(stored.bytes.as_slice()) {
                engine.note_account_event(&ev)?;
            }
        }
        Ok(engine)
    }

    /// This device's leaf identity.
    #[must_use]
    pub fn me(&self) -> &LeafIdentity {
        &self.me
    }

    /// Records this device's account id on the instance (from `Welcome`),
    /// so its own participant entry resolves without a roster fetch.
    pub fn set_account(&mut self, account_id: Vec<u8>) {
        self.account_identity
            .insert(account_id.clone(), self.me.identity_id.clone());
        self.my_account = account_id;
    }

    /// The personal channels this engine knows, with their participants.
    #[must_use]
    pub fn channels(&self) -> &HashMap<Vec<u8>, Vec<Vec<u8>>> {
        &self.participants
    }

    /// Records a personal channel the application learned of directly
    /// (the `OpenPersonalDmResponse`).
    ///
    /// # Errors
    /// On a store failure.
    pub fn record_channel(&mut self, channel: &Channel) -> Result<(), ClientError> {
        if !is_personal(channel) {
            return Ok(());
        }
        let ids: Vec<String> = channel
            .participant_account_ids
            .iter()
            .map(|p| hex(p))
            .collect();
        self.store
            .set_meta(&participants_key(&channel.channel_id), &ids.join(","))?;
        self.participants.insert(
            channel.channel_id.clone(),
            channel.participant_account_ids.clone(),
        );
        Ok(())
    }

    fn note_account_event(&mut self, ev: &AccountEvent) -> Result<Vec<Outcome>, ClientError> {
        match &ev.payload {
            Some(account_event::Payload::ChannelAdded(added)) => {
                if let Some(channel) = &added.channel {
                    if is_personal(channel) {
                        self.record_channel(channel)?;
                        return Ok(vec![Outcome::Subscribe(channel.channel_id.clone())]);
                    }
                }
                Ok(Vec::new())
            }
            Some(account_event::Payload::RosterUpdated(updated)) => {
                if let Some(signed) = &updated.roster {
                    let held = self.rosters.get(&self.me.identity_id);
                    if let Ok(v) = verify_roster(signed, held, ts_nanos(Some(&now_ts()))) {
                        self.rosters.insert(v.identity_id.clone(), v);
                    }
                }
                Ok(Vec::new())
            }
            Some(account_event::Payload::ContactRosterUpdated(_)) => {
                // A participant's roster changed: forget cached rosters other
                // than our own so the next event re-fetches them.
                let mine = self.me.identity_id.clone();
                self.rosters.retain(|id, _| *id == mine);
                self.account_identity
                    .retain(|account, _| *account == self.my_account);
                Ok(Vec::new())
            }
            _ => Ok(Vec::new()),
        }
    }

    /// Feeds the account events the gateway applied. A roster change, ours
    /// or a contact's, reconciles every known conversation's leaves against
    /// the rosters (spec section 4, "revoking a device").
    ///
    /// # Errors
    /// On a store, MLS or API failure.
    pub async fn on_account_events(
        &mut self,
        events: &[AccountEvent],
    ) -> Result<Vec<Outcome>, ClientError> {
        let mut out = Vec::new();
        let mut rosters_changed = false;
        for ev in events {
            if matches!(
                ev.payload,
                Some(account_event::Payload::RosterUpdated(_))
                    | Some(account_event::Payload::ContactRosterUpdated(_))
            ) {
                rosters_changed = true;
            }
            out.extend(self.note_account_event(ev)?);
        }
        if rosters_changed {
            for channel in self.participants.keys().cloned().collect::<Vec<_>>() {
                if self.is_member(&channel)? {
                    out.extend(self.reconcile_members(&channel).await?);
                }
            }
        }
        Ok(out)
    }

    /// On `Welcome`: tops up this device's key-package pool when the
    /// instance reports it below the threshold (spec section 3).
    ///
    /// # Errors
    /// On an MLS, store or API failure.
    pub async fn on_welcome(&mut self, welcome: &Welcome) -> Result<Vec<Outcome>, ClientError> {
        // Rosters may have moved while we were away: judge leaves against
        // fresh ones from here on.
        for channel in self.participants.keys().cloned().collect::<Vec<_>>() {
            self.forget_rosters(&channel);
        }
        let count = welcome.key_package_count as usize;
        if count >= KEY_PACKAGE_TOP_UP_BELOW {
            return Ok(Vec::new());
        }
        let n = KEY_PACKAGE_POOL - count;
        let (packages, last_resort) = self.store.transaction(|store| {
            let provider = Provider::new(store);
            let mut packages = Vec::with_capacity(n);
            for _ in 0..n {
                packages.push(new_key_package(
                    &provider,
                    &self.signer,
                    &self.me,
                    KEY_PACKAGE_LIFETIME_SECS,
                    false,
                )?);
            }
            let last = new_key_package(
                &provider,
                &self.signer,
                &self.me,
                KEY_PACKAGE_LIFETIME_SECS,
                true,
            )?;
            Ok::<_, ClientError>((packages, last))
        })?;
        self.api.publish_key_packages(packages, last_resort).await?;
        Ok(vec![Outcome::Print(format!(
            "published {n} key packages and a last-resort package"
        ))])
    }

    /// Fetches rosters for every participant of `channel_id` not yet held.
    async fn ensure_rosters(&mut self, channel_id: &[u8]) -> Result<(), ClientError> {
        let participants = self.participants.get(channel_id).cloned().ok_or_else(|| {
            ClientError::State(format!(
                "unknown personal channel {}; sync the account log first",
                wire::display(channel_id)
            ))
        })?;
        for account in participants {
            if self.account_identity.contains_key(&account) {
                continue;
            }
            let resp = self.api.account_roster(&wire::display(&account)).await?;
            let signed = resp
                .roster
                .ok_or_else(|| ClientError::State("participant has no roster".into()))?;
            let verified = verify_roster(&signed, None, ts_nanos(Some(&now_ts())))?;
            self.account_identity
                .insert(account, verified.identity_id.clone());
            self.rosters.insert(verified.identity_id.clone(), verified);
        }
        Ok(())
    }

    /// Forgets the other participants' rosters so they are fetched again.
    fn forget_rosters(&mut self, channel_id: &[u8]) {
        if let Some(participants) = self.participants.get(channel_id) {
            for account in participants {
                if *account != self.my_account {
                    if let Some(identity) = self.account_identity.remove(account) {
                        self.rosters.remove(&identity);
                    }
                }
            }
        }
    }

    /// The device keys the lookup for `channel_id` vouches for: every
    /// non-revoked device of every participant identity (spec section 2).
    fn keys_for(&self, channel_id: &[u8]) -> BTreeMap<(Vec<u8>, Vec<u8>), Vec<u8>> {
        let mut identities: HashSet<Vec<u8>> = HashSet::new();
        identities.insert(self.me.identity_id.clone());
        if let Some(participants) = self.participants.get(channel_id) {
            for account in participants {
                if let Some(identity) = self.account_identity.get(account) {
                    identities.insert(identity.clone());
                }
            }
        }
        let mut keys = BTreeMap::new();
        for identity in identities {
            if let Some(roster) = self.rosters.get(&identity) {
                for (device, info) in &roster.devices {
                    if !roster.revoked.contains_key(device) {
                        keys.insert((identity.clone(), device.clone()), info.public_key.clone());
                    }
                }
            }
        }
        keys
    }

    /// Creates the group for a personal channel this device opened: claims
    /// every participant device's key package (this identity's other
    /// devices included), creates the group, and stages the add commit.
    /// Returns the commit to send; the welcome follows its ack.
    ///
    /// # Errors
    /// When a claimed key package fails roster validation (the group does
    /// not advance), or on an MLS, store or API failure.
    pub async fn create_group(&mut self, channel_id: &[u8]) -> Result<Vec<Outcome>, ClientError> {
        self.ensure_rosters(channel_id).await?;
        let participants = self
            .participants
            .get(channel_id)
            .cloned()
            .unwrap_or_default();
        let mut claimed = Vec::new();
        for account in &participants {
            let resp = self.api.claim_key_packages(account.clone()).await?;
            claimed.extend(resp.key_packages.into_iter().map(|c| c.key_package));
        }
        if claimed.is_empty() {
            return Err(ClientError::State(
                "no key packages to add; the other devices have not published any".into(),
            ));
        }
        let keys = self.keys_for(channel_id);
        let lookup = move |i: &[u8], d: &[u8]| keys.get(&(i.to_vec(), d.to_vec())).cloned();
        let (command, commit) = self.store.transaction(|store| {
            let provider = Provider::new(store);
            if Conversation::load(&provider, channel_id)?.is_some() {
                return Err(ClientError::State("a group already exists here".into()));
            }
            let mut packages = Vec::with_capacity(claimed.len());
            for bytes in &claimed {
                packages.push(parse_key_package(&provider, bytes, &lookup)?.0);
            }
            let mut group = Conversation::create(&provider, &self.signer, &self.me, channel_id)?;
            let commit = group.add(&provider, &self.signer, &packages)?;
            let command = control(channel_id, commit.commit.clone(), commit.epoch);
            store.push_outbox(&OutboxEntry {
                command_id: command.command_id.clone(),
                channel_id: channel_id.to_vec(),
                bytes: command.encode_to_vec(),
            })?;
            Ok::<_, ClientError>((command, commit))
        })?;
        self.pending.insert(
            command.command_id.clone(),
            Pending::Commit {
                channel_id: channel_id.to_vec(),
                welcome: commit.welcome,
                epoch: commit.epoch,
                creating: true,
            },
        );
        Ok(vec![
            Outcome::Print(format!(
                "created group for {} with {} leaves; committing",
                wire::display(channel_id),
                claimed.len() + 1
            )),
            Outcome::Send(command),
        ])
    }

    /// Encrypts a text message for the conversation and queues it.
    ///
    /// # Errors
    /// When this device is not a member yet, a commit is pending, or on an
    /// MLS or store failure.
    pub fn send_text(
        &mut self,
        channel_id: &[u8],
        body: &str,
    ) -> Result<Vec<Outcome>, ClientError> {
        let id = wire::new_id();
        let op = DmOperation {
            operation_id: id.clone(),
            sent_at: Some(now_ts()),
            op: Some(dm_operation::Op::Send(DmSend {
                message_id: id,
                body: body.to_string(),
                ..Default::default()
            })),
        };
        self.send_operation(channel_id, op)
    }

    /// Encrypts an operation and queues it (spec section 5): MLS state and
    /// the outbox entry commit together; the command id is the operation id.
    ///
    /// # Errors
    /// When this device is not a member yet, a commit is pending, or on an
    /// MLS or store failure.
    pub fn send_operation(
        &mut self,
        channel_id: &[u8],
        op: DmOperation,
    ) -> Result<Vec<Outcome>, ClientError> {
        let command = self.store.transaction(|store| {
            let provider = Provider::new(store);
            let mut group = Conversation::load(&provider, channel_id)?.ok_or_else(|| {
                ClientError::State("not a member of this conversation yet".into())
            })?;
            if group.has_pending_commit() {
                return Err(MlsError::PendingCommit.into());
            }
            let out = group.encrypt(&provider, &self.signer, &op)?;
            let command = Command {
                command_id: op.operation_id.clone(),
                channel_id: channel_id.to_vec(),
                op: Some(command::Op::SendEncrypted(SendEncrypted {
                    payload: Some(EncryptedPayload {
                        kind: EncryptedKind::Application as i32,
                        mls_message: out.bytes,
                        epoch: out.epoch,
                    }),
                })),
            };
            store.push_outbox(&OutboxEntry {
                command_id: command.command_id.clone(),
                channel_id: channel_id.to_vec(),
                bytes: command.encode_to_vec(),
            })?;
            Ok::<_, ClientError>(command)
        })?;
        let epoch = match &command.op {
            Some(command::Op::SendEncrypted(SendEncrypted {
                payload: Some(payload),
            })) => payload.epoch,
            _ => 0,
        };
        self.pending.insert(
            command.command_id.clone(),
            Pending::Message {
                channel_id: channel_id.to_vec(),
                op: op.encode_to_vec(),
                epoch,
            },
        );
        self.sent
            .insert(command.command_id.clone(), (op.encode_to_vec(), epoch));
        Ok(vec![Outcome::Send(command)])
    }

    /// Feeds a session notice: acks merge pending commits and release
    /// welcomes; an epoch refusal discards the pending commit.
    ///
    /// # Errors
    /// On an MLS, store or API failure.
    pub async fn on_notice(&mut self, notice: &Notice) -> Result<Vec<Outcome>, ClientError> {
        match notice {
            Notice::Acked { command_id, seq } => {
                let Some(pending) = self.pending.remove(command_id) else {
                    return Ok(Vec::new());
                };
                self.on_ack(command_id, *seq, pending).await
            }
            Notice::Error(error) => {
                let Some(pending) = self.pending.remove(&error.command_id) else {
                    return Ok(Vec::new());
                };
                self.on_refused(pending, error)
            }
            _ => Ok(Vec::new()),
        }
    }

    async fn on_ack(
        &mut self,
        command_id: &[u8],
        seq: u64,
        pending: Pending,
    ) -> Result<Vec<Outcome>, ClientError> {
        match pending {
            Pending::Message {
                channel_id,
                op,
                epoch,
            } => {
                // The sender's own operation enters its log at the acked seq.
                self.store.put_dm_row(
                    &channel_id,
                    &DmRow {
                        seq,
                        sender_identity_id: self.me.identity_id.clone(),
                        sender_device_id: self.me.device_id.clone(),
                        epoch,
                        status: DmStatus::Decrypted,
                        bytes: op,
                    },
                )?;
                Ok(vec![Outcome::Delivered(command_id.to_vec())])
            }
            Pending::Welcome { channel_id } => Ok(vec![Outcome::GroupReady(channel_id)]),
            Pending::Commit {
                channel_id,
                welcome,
                epoch,
                ..
            } => {
                let (info, welcome_command) = self.store.transaction(|store| {
                    let provider = Provider::new(store);
                    let mut group =
                        Conversation::load(&provider, &channel_id)?.ok_or(MlsError::NoGroup)?;
                    group.merge_pending(&provider)?;
                    let info = group.group_info(&provider, &self.signer)?;
                    let welcome_command = match welcome {
                        Some(bytes) => {
                            let command = control(&channel_id, bytes, epoch);
                            store.push_outbox(&OutboxEntry {
                                command_id: command.command_id.clone(),
                                channel_id: channel_id.clone(),
                                bytes: command.encode_to_vec(),
                            })?;
                            Some(command)
                        }
                        None => None,
                    };
                    Ok::<_, ClientError>((info, welcome_command))
                })?;
                self.api
                    .publish_group_info(channel_id.clone(), info, epoch)
                    .await?;
                let mut out = vec![Outcome::Print(format!(
                    "commit acked: epoch {epoch}; group info published"
                ))];
                match welcome_command {
                    Some(command) => {
                        self.pending.insert(
                            command.command_id.clone(),
                            Pending::Welcome {
                                channel_id: channel_id.clone(),
                            },
                        );
                        out.push(Outcome::Send(command));
                    }
                    None => out.push(Outcome::GroupReady(channel_id)),
                }
                Ok(out)
            }
        }
    }

    fn on_refused(
        &mut self,
        pending: Pending,
        error: &lumia_core::proto::Error,
    ) -> Result<Vec<Outcome>, ClientError> {
        let Pending::Commit {
            channel_id,
            creating,
            ..
        } = pending
        else {
            return Ok(vec![Outcome::Print(format!(
                "command refused: {} {}",
                error.code, error.message
            ))]);
        };
        self.store.transaction(|store| {
            let provider = Provider::new(store);
            if let Some(mut group) = Conversation::load(&provider, &channel_id)? {
                group.discard_pending(&provider)?;
                if creating {
                    group.delete(&provider)?;
                }
            }
            Ok::<_, ClientError>(())
        })?;
        Ok(vec![Outcome::Print(if creating {
            "commit refused: another device created the group first; waiting for its welcome".into()
        } else {
            format!(
                "commit refused ({}); pending commit discarded, retry after the missed events",
                error.details.get("state").cloned().unwrap_or_default()
            )
        })])
    }

    /// Feeds channel events the gateway applied. The events themselves are
    /// only the trigger: each affected conversation is caught up from the
    /// store's event log, from the processed cursor to the head, so a
    /// restart or a late subscription never skips an event.
    ///
    /// # Errors
    /// On a store or API failure. MLS outcomes per event are recorded, not
    /// raised.
    pub async fn on_events(&mut self, events: &[Event]) -> Result<Vec<Outcome>, ClientError> {
        let mut channels: Vec<Vec<u8>> = Vec::new();
        for ev in events {
            if matches!(ev.payload, Some(event::Payload::Encrypted(_)))
                && !channels.contains(&ev.channel_id)
            {
                channels.push(ev.channel_id.clone());
            }
        }
        let mut out = Vec::new();
        for channel in channels {
            out.extend(self.catch_up(&channel).await?);
        }
        Ok(out)
    }

    /// Processes every stored event of a conversation past the processed
    /// cursor, in seq order: welcomes join, commits merge, application
    /// messages decrypt into the stored log, each in one savepoint with
    /// the cursor. A conversation whose participants are not known yet is
    /// left alone until the account log names them.
    ///
    /// # Errors
    /// On a store or API failure.
    pub async fn catch_up(&mut self, channel_id: &[u8]) -> Result<Vec<Outcome>, ClientError> {
        if self.load_participants(channel_id)?.is_empty() {
            return Ok(vec![Outcome::Print(format!(
                "{}: participants not known yet; waiting for the account log",
                wire::display(channel_id)
            ))]);
        }
        let processed = self.processed(channel_id)?;
        let stored = self
            .store
            .stored_events(lumia_core::store::LogKind::Channel, channel_id)?;
        let mut out = Vec::new();
        for stored in stored.into_iter().filter(|e| e.seq > processed) {
            let Ok(ev) = Event::decode(stored.bytes.as_slice()) else {
                continue;
            };
            let Some(event::Payload::Encrypted(payload)) = &ev.payload else {
                self.set_processed(channel_id, ev.seq)?;
                continue;
            };
            if ev.actor_device_id == self.me.device_id {
                self.own_event(channel_id, &ev, payload)?;
                continue;
            }
            self.ensure_rosters(channel_id).await?;
            let mut result = self.process_one(&ev, payload)?;
            if matches!(result, Err(MlsError::FutureEpoch { .. })) {
                // Hold it: the commit it needs has not been processed, so
                // leave the cursor here and try again on the next trigger.
                out.push(Outcome::Print(format!(
                    "[{}] held: {}",
                    ev.seq,
                    result
                        .as_ref()
                        .err()
                        .map(ToString::to_string)
                        .unwrap_or_default()
                )));
                break;
            }
            if matches!(result, Err(MlsError::LeafRejected(_))) {
                // A roster may have moved: fetch again and judge once more.
                self.forget_rosters(channel_id);
                self.ensure_rosters(channel_id).await?;
                result = self.process_one(&ev, payload)?;
            }
            out.extend(self.record(&ev, payload, result)?);
        }
        Ok(out)
    }

    /// This device's own event coming back through the log. A commit whose
    /// ack never merged it (a crash in between) is merged here; an
    /// application message sent this session is recorded at its seq.
    fn own_event(
        &mut self,
        channel_id: &[u8],
        ev: &Event,
        payload: &EncryptedPayload,
    ) -> Result<(), ClientError> {
        let is_control = payload.kind == EncryptedKind::Control as i32;
        let sent = self.sent.remove(&ev.command_id);
        let me = self.me.clone();
        self.store.transaction(|store| {
            let provider = Provider::new(store);
            if is_control {
                if let Some(mut group) = Conversation::load(&provider, channel_id)? {
                    if group.has_pending_commit() && group.epoch() + 1 == payload.epoch {
                        group.merge_pending(&provider)?;
                    }
                }
            } else if let Some((op, epoch)) = sent {
                store.put_dm_row(
                    channel_id,
                    &DmRow {
                        seq: ev.seq,
                        sender_identity_id: me.identity_id.clone(),
                        sender_device_id: me.device_id.clone(),
                        epoch,
                        status: DmStatus::Decrypted,
                        bytes: op,
                    },
                )?;
            }
            store.set_meta(&processed_key(channel_id), &ev.seq.to_string())?;
            Ok::<_, ClientError>(())
        })
    }

    fn processed(&self, channel_id: &[u8]) -> Result<u64, ClientError> {
        Ok(self
            .store
            .meta_get(&processed_key(channel_id))?
            .and_then(|s| s.parse().ok())
            .unwrap_or(0))
    }

    fn set_processed(&self, channel_id: &[u8], seq: u64) -> Result<(), ClientError> {
        Ok(self
            .store
            .set_meta(&processed_key(channel_id), &seq.to_string())?)
    }

    /// One event through the group, inside a savepoint that also advances
    /// the processed cursor and records the row. Returns the MLS outcome
    /// as a value so the caller can decide to retry on a rejected leaf.
    fn process_one(
        &self,
        ev: &Event,
        payload: &EncryptedPayload,
    ) -> Result<Result<Incoming, MlsError>, ClientError> {
        let keys = self.keys_for(&ev.channel_id);
        let lookup = move |i: &[u8], d: &[u8]| keys.get(&(i.to_vec(), d.to_vec())).cloned();
        let is_control = payload.kind == EncryptedKind::Control as i32;
        self.store
            .transaction(|store| {
                let provider = Provider::new(store);
                let result = match Conversation::load(&provider, &ev.channel_id)? {
                    None if is_control => {
                        Conversation::join(&provider, &payload.mls_message, &lookup).map(|g| {
                            Incoming::Commit {
                                epoch: g.epoch(),
                                added: vec![self.me.clone()],
                                removed: Vec::new(),
                                joined: None,
                                self_removed: false,
                            }
                        })
                    }
                    None => Err(MlsError::BeforeJoin),
                    Some(mut group) => group.handle(&provider, &payload.mls_message, &lookup),
                };
                // A rejected leaf rolls the savepoint back so a retry starts clean.
                if matches!(result, Err(MlsError::LeafRejected(_))) {
                    return Err(ClientError::Mls(MlsError::LeafRejected("retry")));
                }
                Ok::<_, ClientError>(result)
            })
            .or_else(|e| match e {
                ClientError::Mls(MlsError::LeafRejected(_)) => {
                    Ok(Err(MlsError::LeafRejected("leaf rejected")))
                }
                other => Err(other),
            })
    }

    fn record(
        &mut self,
        ev: &Event,
        payload: &EncryptedPayload,
        result: Result<Incoming, MlsError>,
    ) -> Result<Vec<Outcome>, ClientError> {
        let channel = ev.channel_id.clone();
        let mut row = DmRow {
            seq: ev.seq,
            sender_identity_id: Vec::new(),
            sender_device_id: ev.actor_device_id.clone(),
            epoch: payload.epoch,
            status: DmStatus::Undecryptable,
            bytes: Vec::new(),
        };
        let mut out = Vec::new();
        match result {
            Ok(Incoming::Operation { sender, op, epoch }) => {
                row.sender_identity_id.clone_from(&sender.identity_id);
                row.sender_device_id.clone_from(&sender.device_id);
                row.epoch = epoch;
                row.status = DmStatus::Decrypted;
                row.bytes = op.encode_to_vec();
                out.push(Outcome::Operation {
                    channel_id: channel.clone(),
                    seq: ev.seq,
                    sender,
                    op,
                });
            }
            Ok(Incoming::Commit {
                epoch,
                added,
                removed,
                joined,
                self_removed,
            }) => {
                if added.len() == 1 && added[0] == self.me {
                    out.push(Outcome::GroupReady(channel.clone()));
                }
                let mut what = format!("[{}] epoch {epoch}", ev.seq);
                if !added.is_empty() {
                    what.push_str(&format!(
                        "; added {}",
                        added
                            .iter()
                            .map(|l| wire::display(&l.device_id))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
                if !removed.is_empty() {
                    what.push_str(&format!(
                        "; removed {}",
                        removed
                            .iter()
                            .map(|l| wire::display(&l.device_id))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
                if let Some(j) = joined {
                    what.push_str(&format!(
                        "; device recovered: {} (re-verify its identity)",
                        wire::display(&j.device_id)
                    ));
                }
                if self_removed {
                    what.push_str("; this device was removed");
                }
                out.push(Outcome::Print(what));
                self.set_processed(&channel, ev.seq)?;
                return Ok(out);
            }
            Ok(Incoming::Proposal { .. } | Incoming::Welcome | Incoming::Own) => {
                self.set_processed(&channel, ev.seq)?;
                return Ok(out);
            }
            Err(_) if payload.kind == EncryptedKind::Control as i32 => {
                // A welcome for another device, or a commit from before we
                // joined: nothing for us.
                self.set_processed(&channel, ev.seq)?;
                return Ok(out);
            }
            Err(MlsError::BeforeJoin) => {
                row.status = DmStatus::BeforeJoin;
                out.push(Outcome::Print(format!(
                    "[{}] sent before this device joined",
                    ev.seq
                )));
            }
            Err(e) => {
                out.push(Outcome::Print(format!(
                    "[{}] could not be decrypted: {e}",
                    ev.seq
                )));
            }
        }
        self.store.transaction(|store| {
            store.put_dm_row(&channel, &row)?;
            store.set_meta(&processed_key(&channel), &ev.seq.to_string())?;
            Ok::<_, ClientError>(())
        })?;
        Ok(out)
    }

    /// Commits the removal of every leaf in `channel_id` the rosters no
    /// longer vouch for (spec section 4, "revoking a device"). Returns the
    /// commit to send, or nothing when every leaf is in good standing.
    ///
    /// # Errors
    /// On an MLS, store or API failure.
    pub async fn reconcile_members(
        &mut self,
        channel_id: &[u8],
    ) -> Result<Vec<Outcome>, ClientError> {
        self.forget_rosters(channel_id);
        self.ensure_rosters(channel_id).await?;
        let keys = self.keys_for(channel_id);
        let staged = self.store.transaction(|store| {
            let provider = Provider::new(store);
            let Some(mut group) = Conversation::load(&provider, channel_id)? else {
                return Ok::<_, ClientError>(None);
            };
            if group.has_pending_commit() {
                return Ok(None);
            }
            let stale: Vec<Vec<u8>> = group
                .members()
                .into_iter()
                .filter(|(leaf, _)| {
                    !keys.contains_key(&(leaf.identity_id.clone(), leaf.device_id.clone()))
                })
                .map(|(leaf, _)| leaf.device_id)
                .collect();
            if stale.is_empty() {
                return Ok(None);
            }
            let Some(commit) = group.remove(&provider, &self.signer, &stale)? else {
                return Ok(None);
            };
            let command = control(channel_id, commit.commit.clone(), commit.epoch);
            store.push_outbox(&OutboxEntry {
                command_id: command.command_id.clone(),
                channel_id: channel_id.to_vec(),
                bytes: command.encode_to_vec(),
            })?;
            Ok(Some((command, commit, stale)))
        })?;
        let Some((command, commit, stale)): Option<(Command, Commit, Vec<Vec<u8>>)> = staged else {
            return Ok(Vec::new());
        };
        self.pending.insert(
            command.command_id.clone(),
            Pending::Commit {
                channel_id: channel_id.to_vec(),
                welcome: commit.welcome,
                epoch: commit.epoch,
                creating: false,
            },
        );
        Ok(vec![
            Outcome::Print(format!(
                "removing revoked leaves {}; committing epoch {}",
                stale
                    .iter()
                    .map(|d| wire::display(d))
                    .collect::<Vec<_>>()
                    .join(", "),
                commit.epoch
            )),
            Outcome::Send(command),
        ])
    }

    /// The decrypted log of a conversation, for the projection.
    ///
    /// # Errors
    /// On a store failure.
    pub fn rows(&self, channel_id: &[u8]) -> Result<Vec<DmRow>, ClientError> {
        Ok(self.store.dm_rows(channel_id)?)
    }

    /// Whether this device holds group state for the conversation.
    ///
    /// # Errors
    /// On a store failure.
    pub fn is_member(&self, channel_id: &[u8]) -> Result<bool, ClientError> {
        let provider = Provider::new(&self.store);
        Ok(Conversation::load(&provider, channel_id)?.is_some())
    }

    /// Participants recorded for a channel, from the store if not cached.
    ///
    /// # Errors
    /// On a store failure.
    pub fn load_participants(&mut self, channel_id: &[u8]) -> Result<Vec<Vec<u8>>, ClientError> {
        if let Some(p) = self.participants.get(channel_id) {
            return Ok(p.clone());
        }
        let ids: Vec<Vec<u8>> = self
            .store
            .meta_get(&participants_key(channel_id))?
            .map(|s| s.split(',').filter(|p| !p.is_empty()).map(unhex).collect())
            .unwrap_or_default();
        if !ids.is_empty() {
            self.participants.insert(channel_id.to_vec(), ids.clone());
        }
        Ok(ids)
    }
}
