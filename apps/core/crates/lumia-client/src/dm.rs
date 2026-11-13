//! The personal-DM engine (docs/specs/e2ee.md sections 3 to 6): one per
//! instance, driving the core's [`Conversation`]s from the gateway's
//! events and producing the commands the gateway must send.
//!
//! It owns its own connection to the instance store so that MLS state, the
//! decrypted log and the outbox entry for a send commit in one savepoint
//! (spec section 7). It never talks to the socket: the application feeds
//! it [`GatewayEvent`]s and carries out the [`Outcome`]s it returns.

use std::collections::{BTreeMap, HashMap, HashSet};

use base64::Engine as _;
use lumia_core::identity::handoff::open_handoff;
use lumia_core::identity::keys::{
    device_signature, open, random, seal, signing_key, ts_nanos, verify,
};
use lumia_core::identity::roster::{verify_roster, Verified};
use lumia_core::mls::credential::{device_signer, LeafIdentity};
use lumia_core::mls::group::{Commit, Conversation, Incoming};
use lumia_core::mls::key_packages::{new_key_package, parse_key_package};
use lumia_core::mls::projection::Projection;
use lumia_core::mls::storage::Provider;
use lumia_core::mls::{
    MlsError, SignatureKeyPair, KEY_PACKAGE_LIFETIME_SECS, KEY_PACKAGE_POOL,
    KEY_PACKAGE_TOP_UP_BELOW,
};
use lumia_core::proto::{
    account_event, command, dm_operation, event, AccountEvent, BackupKeyHandoff, Channel,
    ChannelKind, Command, DmDelete, DmOperation, DmSend, EncryptedKind, EncryptedPayload, Event,
    HistoryChunkBody, HistoryEntry, HistoryManifestBody, HistoryManifestEntry, SendEncrypted,
    SignedHistoryManifest, Welcome,
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
    /// A root rotation's new backup key arrived on the account log, sealed
    /// to this device among others: the app opens it with the identity's
    /// stores and keeps it.
    BackupKeyHandoff(BackupKeyHandoff),
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
        /// Leaves to remove in a follow-up commit once this one is acked
        /// (a rejoin's revoked devices, spec section 10).
        remove_after: Vec<Vec<u8>>,
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
    /// The device key seed, for signing history manifests.
    seed: Vec<u8>,
    /// The backup key, when this device holds one (native devices do).
    backup_key: Option<Vec<u8>>,
    /// Rosters by identity id, this device's own included.
    rosters: HashMap<Vec<u8>, Verified>,
    /// Account id to identity id, for every participant seen.
    account_identity: HashMap<Vec<u8>, Vec<u8>>,
    /// Participant account ids per personal channel.
    participants: HashMap<Vec<u8>, Vec<Vec<u8>>>,
    pending: HashMap<Vec<u8>, Pending>,
    /// Operations this session sent, by command id, so the sender's own
    /// events can be recorded when they come back through the log. Also
    /// kept in the store (`dm:sent:<id>`) for a send that outlives the
    /// process: the outbox resends it, and the ack or the event records it.
    sent: HashMap<Vec<u8>, (Vec<u8>, u64)>,
    /// Conversations whose commit was refused on the epoch rule: their
    /// leaves are reconciled again once the missed events are processed.
    retry_after_refusal: HashSet<Vec<u8>>,
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

fn joined_key(channel_id: &[u8]) -> String {
    format!("dm:joined_epoch:{}", hex(channel_id))
}

/// A sent operation read back from the store: channel, operation bytes, epoch.
type StoredSent = (Vec<u8>, Vec<u8>, u64);

fn sent_key(command_id: &[u8]) -> String {
    format!("dm:sent:{}", hex(command_id))
}

/// A sent operation as the store keeps it: the channel, the epoch and the
/// operation bytes.
fn sent_value(channel_id: &[u8], op: &[u8], epoch: u64) -> String {
    format!(
        "{}:{epoch}:{}",
        hex(channel_id),
        base64::engine::general_purpose::STANDARD.encode(op)
    )
}

fn parse_sent(value: &str) -> Option<StoredSent> {
    let mut parts = value.splitn(3, ':');
    let channel = unhex(parts.next()?);
    let epoch = parts.next()?.parse().ok()?;
    let op = base64::engine::general_purpose::STANDARD
        .decode(parts.next()?)
        .ok()?;
    Some((channel, op, epoch))
}

fn participants_key(channel_id: &[u8]) -> String {
    format!("dm:participants:{}", hex(channel_id))
}

fn now_ts() -> prost_types::Timestamp {
    crate::time::now_ts()
}

fn encrypted(kind: EncryptedKind, channel_id: &[u8], mls_message: Vec<u8>, epoch: u64) -> Command {
    Command {
        command_id: wire::new_id(),
        channel_id: channel_id.to_vec(),
        op: Some(command::Op::SendEncrypted(SendEncrypted {
            payload: Some(EncryptedPayload {
                kind: kind as i32,
                mls_message,
                epoch,
            }),
        })),
    }
}

/// A commit (or external commit): advances the epoch to `epoch`.
fn control(channel_id: &[u8], mls_message: Vec<u8>, epoch: u64) -> Command {
    encrypted(EncryptedKind::Control, channel_id, mls_message, epoch)
}

/// A welcome: carries the epoch its commit produced, advances nothing.
fn welcome(channel_id: &[u8], mls_message: Vec<u8>, epoch: u64) -> Command {
    encrypted(EncryptedKind::Welcome, channel_id, mls_message, epoch)
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
        backup_key: Option<Vec<u8>>,
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
            seed: device_key_seed.to_vec(),
            backup_key,
            rosters,
            account_identity,
            participants: HashMap::new(),
            pending: HashMap::new(),
            sent: HashMap::new(),
            retry_after_refusal: HashSet::new(),
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
    /// Opens this device's copy of a root rotation's backup-key handoff
    /// (identity spec 4.5 step 5) against the roster this engine holds for
    /// its identity, the rotated one once `RosterUpdated` came before it
    /// on the log, and keeps the key here; the caller stores it. `None`
    /// when the handoff carries no copy for this device.
    ///
    /// # Errors
    /// When the handoff is not by the held root or for the held version,
    /// or the copy does not open.
    pub fn accept_backup_key_handoff(
        &mut self,
        handoff: &BackupKeyHandoff,
    ) -> Result<Option<Vec<u8>>, ClientError> {
        let held = self
            .rosters
            .get(&self.me.identity_id)
            .ok_or_else(|| ClientError::State("no roster held for this identity".into()))?;
        let key = open_handoff(
            handoff,
            &held.root_public_key,
            held.version,
            &self.me.device_id,
            &signing_key(&self.seed)?,
        )?;
        if let Some(key) = &key {
            self.backup_key = Some(key.clone());
        }
        Ok(key)
    }

    pub fn set_account(&mut self, account_id: Vec<u8>) {
        self.account_identity
            .insert(account_id.clone(), self.me.identity_id.clone());
        self.my_account = account_id;
    }

    /// The account on this instance an identity is known by, when a
    /// conversation or the account log has named it.
    #[must_use]
    pub fn account_of(&self, identity_id: &[u8]) -> Option<&Vec<u8>> {
        self.account_identity
            .iter()
            .find(|(_, id)| id.as_slice() == identity_id)
            .map(|(account, _)| account)
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
            Some(account_event::Payload::BackupKeyHandedOff(handed)) => Ok(handed
                .handoff
                .clone()
                .map(Outcome::BackupKeyHandoff)
                .into_iter()
                .collect()),
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
            store.set_meta(&joined_key(channel_id), "0")?;
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
                remove_after: Vec::new(),
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
        self.send_text_quoting(channel_id, body, None)
    }

    /// [`DmEngine::send_text`] with a quote of another message of the
    /// conversation (architecture 6.6).
    ///
    /// # Errors
    /// As [`DmEngine::send_operation`].
    pub fn send_text_quoting(
        &mut self,
        channel_id: &[u8],
        body: &str,
        quote: Option<lumia_core::proto::DmQuote>,
    ) -> Result<Vec<Outcome>, ClientError> {
        let id = wire::new_id();
        let op = DmOperation {
            operation_id: id.clone(),
            sent_at: Some(now_ts()),
            op: Some(dm_operation::Op::Send(DmSend {
                message_id: id,
                body: body.to_string(),
                quote,
                ..Default::default()
            })),
        };
        self.send_operation(channel_id, op)
    }

    /// Reacts to a message with an emoji, or takes the reaction back (a
    /// `DmReact` operation).
    ///
    /// # Errors
    /// As [`DmEngine::send_operation`].
    pub fn send_react(
        &mut self,
        channel_id: &[u8],
        message_id: Vec<u8>,
        emoji: &str,
        remove: bool,
    ) -> Result<Vec<Outcome>, ClientError> {
        let op = DmOperation {
            operation_id: wire::new_id(),
            sent_at: Some(now_ts()),
            op: Some(dm_operation::Op::React(lumia_core::proto::DmReact {
                message_id,
                emoji: emoji.to_string(),
                remove,
            })),
        };
        self.send_operation(channel_id, op)
    }

    /// Edits one of this identity's messages (a `DmEdit` operation).
    ///
    /// # Errors
    /// As [`DmEngine::send_operation`].
    pub fn send_edit(
        &mut self,
        channel_id: &[u8],
        message_id: Vec<u8>,
        body: &str,
    ) -> Result<Vec<Outcome>, ClientError> {
        let op = DmOperation {
            operation_id: wire::new_id(),
            sent_at: Some(now_ts()),
            op: Some(dm_operation::Op::Edit(lumia_core::proto::DmEdit {
                message_id,
                body: body.to_string(),
            })),
        };
        self.send_operation(channel_id, op)
    }

    /// Deletes one of this identity's messages (a `DmDelete` operation).
    ///
    /// # Errors
    /// As [`DmEngine::send_operation`].
    pub fn send_delete(
        &mut self,
        channel_id: &[u8],
        message_id: Vec<u8>,
    ) -> Result<Vec<Outcome>, ClientError> {
        let op = DmOperation {
            operation_id: wire::new_id(),
            sent_at: Some(now_ts()),
            op: Some(dm_operation::Op::Delete(DmDelete { message_id })),
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
            store.set_meta(
                &sent_key(&command.command_id),
                &sent_value(channel_id, &op.encode_to_vec(), out.epoch),
            )?;
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
                let pending = match self.pending.remove(command_id) {
                    Some(pending) => pending,
                    // A send from before a restart, resent from the outbox:
                    // the store still knows what it was.
                    None => match self.stored_sent(command_id)? {
                        Some((channel_id, op, epoch)) => Pending::Message {
                            channel_id,
                            op,
                            epoch,
                        },
                        None => return Ok(Vec::new()),
                    },
                };
                self.on_ack(command_id, *seq, pending).await
            }
            Notice::Error(error) => {
                let Some(pending) = self.pending.remove(&error.command_id) else {
                    return Ok(Vec::new());
                };
                let mut out = self.on_refused(pending, error)?;
                // The missed events may already be in; catching up runs the
                // retry either way.
                for channel in self.retry_after_refusal.clone() {
                    out.extend(self.catch_up(&channel).await?);
                }
                Ok(out)
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
                let me = self.me.clone();
                self.store.transaction(|store| {
                    store.put_dm_row(
                        &channel_id,
                        &DmRow {
                            seq,
                            sender_identity_id: me.identity_id.clone(),
                            sender_device_id: me.device_id.clone(),
                            epoch,
                            status: DmStatus::Decrypted,
                            bytes: op,
                        },
                    )?;
                    store.set_meta(&sent_key(command_id), "")?;
                    Ok::<_, ClientError>(())
                })?;
                self.sent.remove(command_id);
                Ok(vec![Outcome::Delivered(command_id.to_vec())])
            }
            Pending::Welcome { channel_id } => Ok(vec![Outcome::GroupReady(channel_id)]),
            Pending::Commit {
                channel_id,
                welcome,
                epoch,
                remove_after,
                ..
            } => {
                let (info, welcome_command) = self.store.transaction(|store| {
                    let provider = Provider::new(store);
                    let mut group =
                        Conversation::load(&provider, &channel_id)?.ok_or(MlsError::NoGroup)?;
                    if group.has_pending_commit() {
                        group.merge_pending(&provider)?;
                    }
                    let info = group.group_info(&provider, &self.signer)?;
                    let welcome_command = match welcome {
                        Some(bytes) => {
                            let command = self::welcome(&channel_id, bytes, epoch);
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
                    None if !remove_after.is_empty() => {
                        out.extend(self.commit_removal(&channel_id, &remove_after)?);
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
        if !creating {
            self.retry_after_refusal.insert(channel_id);
        }
        Ok(vec![Outcome::Print(if creating {
            "commit refused: another device created the group first; waiting for its welcome".into()
        } else {
            format!(
                "commit refused ({}); pending commit discarded, retrying after the missed events",
                error.details.get("state").cloned().unwrap_or_default()
            )
        })])
    }

    /// A sent operation the store remembers, for an ack after a restart:
    /// its channel, operation bytes and epoch.
    fn stored_sent(&self, command_id: &[u8]) -> Result<Option<StoredSent>, ClientError> {
        Ok(self
            .store
            .meta_get(&sent_key(command_id))?
            .as_deref()
            .and_then(parse_sent))
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
        if self.retry_after_refusal.contains(channel_id) {
            // The losing side of two concurrent commits (spec section 12):
            // now on the winner's epoch, reconcile again; what the winner
            // already did is a no-op here, so nothing is proposed twice.
            self.retry_after_refusal.remove(channel_id);
            let retried = self.reconcile_members(channel_id).await?;
            out.push(Outcome::Print(if retried.is_empty() {
                "retrying after the refusal: the other commit covered it; nothing to commit".into()
            } else {
                "retrying after the refusal on the new epoch".into()
            }));
            out.extend(retried);
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
        let sent = match self.sent.remove(&ev.command_id) {
            Some(sent) => Some(sent),
            None => self
                .stored_sent(&ev.command_id)?
                .map(|(_, op, epoch)| (op, epoch)),
        };
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
                store.set_meta(&sent_key(&ev.command_id), "")?;
            }
            store.set_meta(&processed_key(channel_id), &ev.seq.to_string())?;
            Ok::<_, ClientError>(())
        })
    }

    fn joined_epoch(&self, channel_id: &[u8]) -> Result<u64, ClientError> {
        Ok(self
            .store
            .meta_get(&joined_key(channel_id))?
            .and_then(|s| s.parse().ok())
            .unwrap_or(0))
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
        let is_control = payload.kind == EncryptedKind::Control as i32
            || payload.kind == EncryptedKind::Welcome as i32;
        self.store
            .transaction(|store| {
                let provider = Provider::new(store);
                let result = match Conversation::load(&provider, &ev.channel_id)? {
                    None if is_control => {
                        match Conversation::join(&provider, &payload.mls_message, &lookup) {
                            Ok(g) => {
                                store.set_meta(
                                    &joined_key(&ev.channel_id),
                                    &g.epoch().to_string(),
                                )?;
                                Ok(Incoming::Commit {
                                    epoch: g.epoch(),
                                    added: vec![self.me.clone()],
                                    removed: Vec::new(),
                                    joined: None,
                                    self_removed: false,
                                })
                            }
                            Err(e) => Err(e),
                        }
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
            Err(_)
                if payload.kind == EncryptedKind::Control as i32
                    || payload.kind == EncryptedKind::Welcome as i32 =>
            {
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
            // Encrypted in an epoch before this device joined (the sender
            // had not processed the commit yet): history, not a failure.
            Err(MlsError::Undecryptable) if payload.epoch < self.joined_epoch(&channel)? => {
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
        let (stale, missing) = {
            let provider = Provider::new(&self.store);
            let Some(group) = Conversation::load(&provider, channel_id)? else {
                return Ok(Vec::new());
            };
            if group.has_pending_commit() {
                return Ok(Vec::new());
            }
            let members = group.members();
            let stale: Vec<Vec<u8>> = members
                .iter()
                .filter(|(leaf, _)| {
                    !keys.contains_key(&(leaf.identity_id.clone(), leaf.device_id.clone()))
                })
                .map(|(leaf, _)| leaf.device_id.clone())
                .collect();
            let missing: Vec<(Vec<u8>, Vec<u8>)> = keys
                .keys()
                .filter(|(identity, device)| {
                    *device != self.me.device_id
                        && !members
                            .iter()
                            .any(|(l, _)| l.identity_id == *identity && l.device_id == *device)
                })
                .cloned()
                .collect();
            (stale, missing)
        };
        if !stale.is_empty() {
            return self.commit_removal(channel_id, &stale);
        }
        if missing.is_empty() {
            return Ok(Vec::new());
        }
        // Devices in the rosters with no leaf: a newly linked device (spec
        // section 4, "adding a device"). Claim each one's key package.
        let mut claimed = Vec::new();
        for (identity, device) in &missing {
            let Some(account) = self
                .account_identity
                .iter()
                .find(|(_, id)| *id == identity)
                .map(|(a, _)| a.clone())
            else {
                continue;
            };
            match self
                .api
                .claim_key_package_for(account, device.clone())
                .await
            {
                Ok(resp) => claimed.extend(resp.key_packages.into_iter().map(|c| c.key_package)),
                Err(e) => {
                    return Ok(vec![Outcome::Print(format!(
                        "cannot claim a key package for {}: {e}",
                        wire::display(device)
                    ))])
                }
            }
        }
        if claimed.is_empty() {
            return Ok(vec![Outcome::Print(format!(
                "{} device(s) in the rosters have no leaf and no key packages yet",
                missing.len()
            ))]);
        }
        let lookup = move |i: &[u8], d: &[u8]| keys.get(&(i.to_vec(), d.to_vec())).cloned();
        let (command, commit) = self.store.transaction(|store| {
            let provider = Provider::new(store);
            let mut group = Conversation::load(&provider, channel_id)?.ok_or(MlsError::NoGroup)?;
            let mut packages = Vec::with_capacity(claimed.len());
            for bytes in &claimed {
                packages.push(parse_key_package(&provider, bytes, &lookup)?.0);
            }
            let commit = group.add(&provider, &self.signer, &packages)?;
            let command = control(channel_id, commit.commit.clone(), commit.epoch);
            store.push_outbox(&OutboxEntry {
                command_id: command.command_id.clone(),
                channel_id: channel_id.to_vec(),
                bytes: command.encode_to_vec(),
            })?;
            Ok::<_, ClientError>((command, commit))
        })?;
        let n = claimed.len();
        self.pending.insert(
            command.command_id.clone(),
            Pending::Commit {
                channel_id: channel_id.to_vec(),
                welcome: commit.welcome,
                epoch: commit.epoch,
                creating: false,
                remove_after: Vec::new(),
            },
        );
        Ok(vec![
            Outcome::Print(format!(
                "adding {n} new device leaf(s); committing epoch {}",
                commit.epoch
            )),
            Outcome::Send(command),
        ])
    }

    fn commit_removal(
        &mut self,
        channel_id: &[u8],
        stale: &[Vec<u8>],
    ) -> Result<Vec<Outcome>, ClientError> {
        let staged = self.store.transaction(|store| {
            let provider = Provider::new(store);
            let mut group = Conversation::load(&provider, channel_id)?.ok_or(MlsError::NoGroup)?;
            let Some(commit) = group.remove(&provider, &self.signer, stale)? else {
                return Ok::<_, ClientError>(None);
            };
            let command = control(channel_id, commit.commit.clone(), commit.epoch);
            store.push_outbox(&OutboxEntry {
                command_id: command.command_id.clone(),
                channel_id: channel_id.to_vec(),
                bytes: command.encode_to_vec(),
            })?;
            Ok(Some((command, commit)))
        })?;
        let Some((command, commit)): Option<(Command, Commit)> = staged else {
            return Ok(Vec::new());
        };
        self.pending.insert(
            command.command_id.clone(),
            Pending::Commit {
                channel_id: channel_id.to_vec(),
                welcome: commit.welcome,
                epoch: commit.epoch,
                creating: false,
                remove_after: Vec::new(),
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

    /// Rejoins a conversation by external commit from the published
    /// GroupInfo (spec section 10): the group is in the new epoch at once;
    /// the commit goes out as a `CONTROL` event and, on its ack, the new
    /// GroupInfo is published. Leaves of this identity's revoked devices
    /// are removed by the reconciliation that follows.
    ///
    /// # Errors
    /// When no GroupInfo is published, a leaf fails validation, or on an
    /// MLS, store or API failure.
    pub async fn rejoin(&mut self, channel_id: &[u8]) -> Result<Vec<Outcome>, ClientError> {
        self.ensure_rosters(channel_id).await?;
        let info = self.api.group_info(&wire::display(channel_id)).await?;
        let keys = self.keys_for(channel_id);
        let lookup = move |i: &[u8], d: &[u8]| keys.get(&(i.to_vec(), d.to_vec())).cloned();
        let my_revoked: HashSet<Vec<u8>> = self
            .rosters
            .get(&self.me.identity_id)
            .map(|r| r.revoked.keys().cloned().collect())
            .unwrap_or_default();
        let (command, commit, stale) = self.store.transaction(|store| {
            let provider = Provider::new(store);
            if let Some(old) = Conversation::load(&provider, channel_id)? {
                old.delete(&provider)?;
            }
            let (_group, commit, unvouched) = Conversation::external_join(
                &provider,
                &self.signer,
                &self.me,
                &info.group_info,
                &lookup,
            )?;
            store.set_meta(&joined_key(channel_id), &commit.epoch.to_string())?;
            // Our own revoked devices are expected and removed next; any
            // other unvouched leaf means the GroupInfo is not to be trusted.
            let mut stale = Vec::new();
            for leaf in unvouched {
                if leaf.identity_id == self.me.identity_id && my_revoked.contains(&leaf.device_id) {
                    stale.push(leaf.device_id);
                } else {
                    return Err(ClientError::Mls(MlsError::LeafRejected(
                        "a leaf in the GroupInfo is not in a participant's roster",
                    )));
                }
            }
            let command = control(channel_id, commit.commit.clone(), commit.epoch);
            store.push_outbox(&OutboxEntry {
                command_id: command.command_id.clone(),
                channel_id: channel_id.to_vec(),
                bytes: command.encode_to_vec(),
            })?;
            Ok::<_, ClientError>((command, commit, stale))
        })?;
        // Events before the join are history for this device.
        let head = self
            .store
            .stored_events(lumia_core::store::LogKind::Channel, channel_id)?
            .last()
            .map_or(0, |e| e.seq);
        self.set_processed(channel_id, head)?;
        self.pending.insert(
            command.command_id.clone(),
            Pending::Commit {
                channel_id: channel_id.to_vec(),
                welcome: None,
                epoch: commit.epoch,
                creating: false,
                remove_after: stale,
            },
        );
        Ok(vec![
            Outcome::Print(format!(
                "rejoining {} by external commit from GroupInfo at epoch {}; committing epoch {}",
                wire::display(channel_id),
                info.epoch,
                commit.epoch
            )),
            Outcome::Send(command),
        ])
    }

    // ---- History backup (spec section 9) ----

    /// Whether history backup is on for this identity on this instance.
    ///
    /// # Errors
    /// On a store failure.
    pub fn backup_enabled(&self) -> Result<bool, ClientError> {
        Ok(self.store.meta_get("history:enabled")?.as_deref() == Some("on"))
    }

    /// Turns backup on (and writes what is held so far) or off (and deletes
    /// every chunk and manifest of the identity on this instance).
    ///
    /// # Errors
    /// Without a backup key, or on a store or API failure.
    pub async fn set_backup(&mut self, on: bool) -> Result<Vec<Outcome>, ClientError> {
        if on {
            if self.backup_key.is_none() {
                return Err(ClientError::State(
                    "this device holds no backup key; web devices cannot back up".into(),
                ));
            }
            self.store.set_meta("history:enabled", "on")?;
            return self.backup_flush().await;
        }
        self.api.delete_history().await?;
        self.store.transaction(|store| {
            store.set_meta("history:enabled", "off")?;
            store.set_meta("history:manifest", "")?;
            for channel in self.participants.keys() {
                store.set_meta(&format!("history:backed:{}", hex(channel)), "0")?;
            }
            Ok::<_, ClientError>(())
        })?;
        Ok(vec![Outcome::Print(
            "history backup off; chunks and manifest deleted on this instance".into(),
        )])
    }

    fn manifest_body(&self) -> Result<HistoryManifestBody, ClientError> {
        let stored = self.store.meta_get("history:manifest")?.unwrap_or_default();
        if stored.is_empty() {
            return Ok(HistoryManifestBody {
                identity_id: self.me.identity_id.clone(),
                device_id: self.me.device_id.clone(),
                ..Default::default()
            });
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(stored)
            .map_err(|e| ClientError::State(e.to_string()))?;
        Ok(HistoryManifestBody::decode(bytes.as_slice())?)
    }

    /// Re-wraps every chunk key in this device's manifest on this instance
    /// under the backup key now held, after a root rotation (identity spec
    /// 4.5, e2ee spec section 9): the chunks are not touched; the manifest
    /// is written again with a bumped version. A no-op when backup is off
    /// or nothing was written here.
    ///
    /// # Errors
    /// Without a backup key, when a chunk key does not unwrap under
    /// `old_backup_key`, or on a store, crypto or API failure.
    pub async fn rewrap_backup(
        &mut self,
        old_backup_key: &[u8],
    ) -> Result<Vec<Outcome>, ClientError> {
        let Some(backup_key) = self.backup_key.clone() else {
            return Err(ClientError::State("this device holds no backup key".into()));
        };
        let mut manifest = self.manifest_body()?;
        if !self.backup_enabled()? || manifest.chunks.is_empty() {
            return Ok(Vec::new());
        }
        for entry in &mut manifest.chunks {
            let chunk_key = open(old_backup_key, &entry.wrap_nonce, &entry.wrapped_key)?;
            let (wrap_nonce, wrapped_key) = seal(&backup_key, &chunk_key)?;
            entry.wrap_nonce = wrap_nonce;
            entry.wrapped_key = wrapped_key;
        }
        let n = manifest.chunks.len();
        manifest.version += 1;
        manifest.written_at = Some(now_ts());
        let body = manifest.encode_to_vec();
        let key = signing_key(&self.seed)?;
        let signed = SignedHistoryManifest {
            body: body.clone(),
            signature: Some(device_signature(&self.me.device_id, &key, &body)),
        };
        self.api.put_history_manifest(signed).await?;
        self.store.set_meta(
            "history:manifest",
            &base64::engine::general_purpose::STANDARD.encode(&body),
        )?;
        Ok(vec![Outcome::Print(format!(
            "re-wrapped {n} chunk key(s) under the new backup key (manifest v{})",
            manifest.version
        ))])
    }

    /// Writes one chunk per conversation with operations not yet backed up,
    /// then the manifest with a bumped version. A no-op when backup is off
    /// or nothing is new.
    ///
    /// # Errors
    /// On a store, crypto or API failure (`RESOURCE_EXHAUSTED` over quota).
    pub async fn backup_flush(&mut self) -> Result<Vec<Outcome>, ClientError> {
        if !self.backup_enabled()? {
            return Ok(Vec::new());
        }
        let Some(backup_key) = self.backup_key.clone() else {
            return Ok(Vec::new());
        };
        let mut manifest = self.manifest_body()?;
        let mut written = Vec::new();
        let channels: Vec<Vec<u8>> = self.participants.keys().cloned().collect();
        for channel in channels {
            let backed: u64 = self
                .store
                .meta_get(&format!("history:backed:{}", hex(&channel)))?
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let rows = self.store.dm_rows(&channel)?;
            // Deleted messages leave the archive (spec section 9): their
            // `send` is never written, and a chunk that already holds one is
            // rewritten without it.
            let mut projection = Projection::default();
            let mut ops: Vec<(DmRow, DmOperation)> = Vec::new();
            for r in rows.into_iter().filter(|r| r.status == DmStatus::Decrypted) {
                if let Ok(op) = DmOperation::decode(r.bytes.as_slice()) {
                    projection.apply(r.seq, &r.sender_identity_id, &r.sender_device_id, &op);
                    ops.push((r, op));
                }
            }
            let deleted_seqs: HashSet<u64> = projection
                .messages()
                .filter(|m| m.deleted)
                .map(|m| m.seq)
                .collect();
            let entry_of = |r: &DmRow, op: &DmOperation| HistoryEntry {
                seq: r.seq,
                sender_identity_id: r.sender_identity_id.clone(),
                sender_device_id: r.sender_device_id.clone(),
                operation: Some(op.clone()),
            };

            // Chunks already written that now cover a deleted message.
            let handled_key = format!("history:rewritten:{}", hex(&channel));
            let mut handled: HashSet<u64> = self
                .store
                .meta_get(&handled_key)?
                .map(|s| s.split(',').filter_map(|x| x.parse().ok()).collect())
                .unwrap_or_default();
            let stale: Vec<usize> = manifest
                .chunks
                .iter()
                .enumerate()
                .filter(|(_, e)| {
                    e.channel_id == channel
                        && deleted_seqs.iter().any(|s| {
                            *s >= e.from_seq && *s <= e.through_seq && !handled.contains(s)
                        })
                })
                .map(|(i, _)| i)
                .collect();
            for i in stale {
                let old = manifest.chunks[i].clone();
                let entries: Vec<HistoryEntry> = ops
                    .iter()
                    .filter(|(r, _)| {
                        r.seq >= old.from_seq
                            && r.seq <= old.through_seq
                            && !deleted_seqs.contains(&r.seq)
                    })
                    .map(|(r, op)| entry_of(r, op))
                    .collect();
                let rewritten = self
                    .write_chunk(
                        &backup_key,
                        &channel,
                        old.from_seq,
                        old.through_seq,
                        entries,
                    )
                    .await?;
                self.api
                    .delete_history_chunk(&wire::display(&old.chunk_id))
                    .await?;
                manifest.chunks[i] = rewritten;
                for s in &deleted_seqs {
                    if *s >= old.from_seq && *s <= old.through_seq {
                        handled.insert(*s);
                    }
                }
                written.push((channel.clone(), backed, 0));
            }
            self.store.set_meta(
                &handled_key,
                &handled
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
            )?;

            // New operations since the last chunk.
            let entries: Vec<HistoryEntry> = ops
                .iter()
                .filter(|(r, _)| r.seq > backed && !deleted_seqs.contains(&r.seq))
                .map(|(r, op)| entry_of(r, op))
                .collect();
            let newest = ops.iter().map(|(r, _)| r.seq).max().unwrap_or(0);
            if entries.is_empty() {
                if newest > backed {
                    // Everything new was deleted; the range is still covered.
                    self.store.set_meta(
                        &format!("history:backed:{}", hex(&channel)),
                        &newest.to_string(),
                    )?;
                }
                continue;
            }
            let n = entries.len();
            let entry = self
                .write_chunk(&backup_key, &channel, backed + 1, newest, entries)
                .await?;
            manifest.chunks.push(entry);
            written.push((channel, newest, n));
        }
        if written.is_empty() {
            return Ok(Vec::new());
        }
        manifest.version += 1;
        manifest.written_at = Some(now_ts());
        let body = manifest.encode_to_vec();
        let key = signing_key(&self.seed)?;
        let signed = SignedHistoryManifest {
            body: body.clone(),
            signature: Some(device_signature(&self.me.device_id, &key, &body)),
        };
        self.api.put_history_manifest(signed).await?;
        self.store.transaction(|store| {
            store.set_meta(
                "history:manifest",
                &base64::engine::general_purpose::STANDARD.encode(&body),
            )?;
            for (channel, through, _) in &written {
                store.set_meta(
                    &format!("history:backed:{}", hex(channel)),
                    &through.to_string(),
                )?;
            }
            Ok::<_, ClientError>(())
        })?;
        Ok(written
            .iter()
            .map(|(channel, through, n)| {
                if *n == 0 {
                    Outcome::Print(format!(
                        "rewrote a chunk of {} without a deleted message (manifest v{})",
                        wire::display(channel),
                        manifest.version
                    ))
                } else {
                    Outcome::Print(format!(
                        "backed up {n} operation(s) of {} through seq {through} (manifest v{})",
                        wire::display(channel),
                        manifest.version
                    ))
                }
            })
            .collect())
    }

    /// Seals and uploads one chunk, returning its manifest entry.
    async fn write_chunk(
        &self,
        backup_key: &[u8],
        channel: &[u8],
        from_seq: u64,
        through_seq: u64,
        entries: Vec<HistoryEntry>,
    ) -> Result<HistoryManifestEntry, ClientError> {
        let body = HistoryChunkBody {
            channel_id: channel.to_vec(),
            host: self.api.host(),
            from_seq,
            through_seq,
            entries,
        };
        let mut chunk_key = [0u8; 32];
        random(&mut chunk_key)?;
        let (nonce, ciphertext) = seal(&chunk_key, &body.encode_to_vec())?;
        let (wrap_nonce, wrapped_key) = seal(backup_key, &chunk_key)?;
        let chunk_id = wire::new_id();
        let size = ciphertext.len() as u64;
        self.api
            .put_history_chunk(
                &wire::display(&chunk_id),
                channel.to_vec(),
                ciphertext,
                nonce,
            )
            .await?;
        Ok(HistoryManifestEntry {
            chunk_id,
            channel_id: channel.to_vec(),
            host: self.api.host(),
            from_seq,
            through_seq,
            wrapped_key,
            wrap_nonce,
            size,
        })
    }

    /// Restores history from every device's manifest on this instance:
    /// each manifest is verified against this identity's roster (a device
    /// revoked before the manifest's `written_at` is ignored), chunk keys
    /// are unwrapped under the backup key, and operations are replayed in
    /// seq order, deduplicated by operation id and by seq already held.
    ///
    /// # Errors
    /// Without a backup key, or on a store, crypto or API failure.
    pub async fn restore(&mut self) -> Result<Vec<Outcome>, ClientError> {
        let Some(backup_key) = self.backup_key.clone() else {
            return Err(ClientError::State("this device holds no backup key".into()));
        };
        let Some(roster) = self.rosters.get(&self.me.identity_id).cloned() else {
            return Err(ClientError::State("no roster for this identity".into()));
        };
        let mut out = Vec::new();
        let mut restored = 0usize;
        let mut skipped = 0usize;
        let mut seen_ops: HashSet<Vec<u8>> = HashSet::new();
        let mut held: HashMap<Vec<u8>, HashSet<u64>> = HashMap::new();
        for signed in self.api.list_history_manifests().await?.manifests {
            let body = HistoryManifestBody::decode(signed.body.as_slice())?;
            let written_at = ts_nanos(body.written_at.as_ref());
            let Some(signature) = &signed.signature else {
                continue;
            };
            let public = match roster.devices.get(&body.device_id) {
                Some(info) => Some(info.public_key.clone()),
                None => roster
                    .revoked
                    .get(&body.device_id)
                    .filter(|r| r.revoked_at > written_at)
                    .map(|r| r.public_key.clone()),
            };
            let Some(public) = public else {
                out.push(Outcome::Print(format!(
                    "ignoring manifest v{} from {}: device unknown or revoked before it was written",
                    body.version,
                    wire::display(&body.device_id)
                )));
                continue;
            };
            if body.identity_id != self.me.identity_id
                || signature.signer_id != body.device_id
                || !verify(&signed.body, &signature.signature, &public)
            {
                out.push(Outcome::Print(format!(
                    "ignoring manifest from {}: bad signature",
                    wire::display(&body.device_id)
                )));
                continue;
            }
            for entry in body.chunks {
                let chunk_key = match open(&backup_key, &entry.wrap_nonce, &entry.wrapped_key) {
                    Ok(k) => k,
                    Err(_) => {
                        out.push(Outcome::Print("a chunk key did not unwrap; skipped".into()));
                        continue;
                    }
                };
                let chunk = self
                    .api
                    .get_history_chunk(&wire::display(&entry.chunk_id))
                    .await?;
                let plain = open(&chunk_key, &chunk.nonce, &chunk.ciphertext)?;
                let chunk = HistoryChunkBody::decode(plain.as_slice())?;
                let seqs = match held.get(&chunk.channel_id) {
                    Some(s) => s.clone(),
                    None => {
                        let s: HashSet<u64> = self
                            .store
                            .dm_rows(&chunk.channel_id)?
                            .iter()
                            .filter(|r| r.status == DmStatus::Decrypted)
                            .map(|r| r.seq)
                            .collect();
                        held.insert(chunk.channel_id.clone(), s.clone());
                        s
                    }
                };
                for e in chunk.entries {
                    let Some(op) = e.operation else { continue };
                    if seqs.contains(&e.seq) || !seen_ops.insert(op.operation_id.clone()) {
                        skipped += 1;
                        continue;
                    }
                    self.store.put_dm_row(
                        &chunk.channel_id,
                        &DmRow {
                            seq: e.seq,
                            sender_identity_id: e.sender_identity_id,
                            sender_device_id: e.sender_device_id,
                            epoch: 0,
                            status: DmStatus::Decrypted,
                            bytes: op.encode_to_vec(),
                        },
                    )?;
                    held.entry(chunk.channel_id.clone())
                        .or_default()
                        .insert(e.seq);
                    restored += 1;
                }
            }
        }
        out.push(Outcome::Print(format!(
            "restored {restored} operation(s), {skipped} duplicate(s) skipped"
        )));
        Ok(out)
    }

    /// Whether commands are still waiting for their ack.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Reconciles every conversation this device is a member of against
    /// the rosters, as on connect.
    ///
    /// # Errors
    /// On an MLS, store or API failure.
    pub async fn reconcile_all(&mut self) -> Result<Vec<Outcome>, ClientError> {
        let mut out = Vec::new();
        for channel in self.participants.keys().cloned().collect::<Vec<_>>() {
            if self.is_member(&channel)? {
                out.extend(self.reconcile_members(&channel).await?);
            }
        }
        Ok(out)
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
