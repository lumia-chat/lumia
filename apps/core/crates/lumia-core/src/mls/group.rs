//! One conversation's MLS group (spec sections 4 to 6 and 10).
//!
//! The group id is the channel id. Every leaf that enters the view, by
//! welcome, commit, proposal, GroupInfo or key package, is validated
//! against the rosters through a [`RosterLookup`] before the group
//! advances; a message from a leaf the lookup no longer vouches for is
//! refused even if its removal has not landed yet.
//!
//! Commits are two-phase: [`Conversation::add`] and [`Conversation::remove`]
//! stage a commit and return its bytes; the transport sends them as a
//! `CONTROL` event and, on the ack, calls [`Conversation::merge_pending`],
//! or [`Conversation::discard_pending`] when the instance answers
//! `FAILED_PRECONDITION{state: "epoch"}` (spec section 4).

use openmls::prelude::tls_codec::{Deserialize, Serialize};
use openmls::prelude::{
    GroupId, KeyPackage, LeafNodeIndex, MlsGroup, MlsGroupCreateConfig, MlsGroupJoinConfig,
    MlsMessageBodyIn, MlsMessageIn, MlsMessageOut, ProcessMessageError, ProcessedMessageContent,
    ProtocolMessage, Sender, StagedCommit, StagedWelcome, ValidationError,
};
use openmls_basic_credential::SignatureKeyPair;
use openmls_traits::OpenMlsProvider;
use prost::Message;

use super::credential::{validate_leaf, LeafIdentity, RosterLookup};
use super::storage::{MlsKv, Provider};
use super::{lib, MlsError, SUITE};
use crate::proto::DmOperation;

/// How many past epochs' secrets a member keeps, so an application message
/// encrypted just before a commit still decrypts after it (section 5).
const PAST_EPOCHS: usize = 3;

/// One conversation's group.
pub struct Conversation {
    group: MlsGroup,
}

impl std::fmt::Debug for Conversation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Conversation")
            .field("channel_id", &self.group.group_id().as_slice())
            .field("epoch", &self.group.epoch().as_u64())
            .finish()
    }
}

/// A staged commit's wire bytes, for a `CONTROL` event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    /// The commit `MLSMessage`.
    pub commit: Vec<u8>,
    /// The welcome `MLSMessage` for added members, when any were added.
    pub welcome: Option<Vec<u8>>,
    /// The epoch this commit produces (what `EncryptedPayload.epoch` carries).
    pub epoch: u64,
}

/// A ciphertext for a `SendEncrypted` command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outgoing {
    /// The application `MLSMessage`.
    pub bytes: Vec<u8>,
    /// The epoch it was encrypted in.
    pub epoch: u64,
}

/// What an incoming `mls_message` turned out to be.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    /// A decrypted operation from a validated member.
    Operation {
        /// Who sent it.
        sender: LeafIdentity,
        /// The epoch it was encrypted in.
        epoch: u64,
        /// The operation.
        op: DmOperation,
    },
    /// A commit, now merged.
    Commit {
        /// The epoch the group is in now.
        epoch: u64,
        /// Leaves the commit added.
        added: Vec<LeafIdentity>,
        /// Leaves the commit removed.
        removed: Vec<LeafIdentity>,
        /// Set when a new member joined by external commit (a device
        /// recovered, section 10); the caller re-verifies its identity.
        joined: Option<LeafIdentity>,
        /// Whether this device was removed by it.
        self_removed: bool,
    },
    /// A proposal, queued for the next commit.
    Proposal {
        /// The epoch it was sent in.
        epoch: u64,
    },
    /// A welcome: for this device if it is not yet a member, otherwise for
    /// another device. The caller decides.
    Welcome,
    /// This device's own message coming back through the log; the state
    /// already moved when it was sent (or when its commit was merged).
    Own,
}

fn join_config() -> MlsGroupJoinConfig {
    MlsGroupJoinConfig::builder()
        .max_past_epochs(PAST_EPOCHS)
        .use_ratchet_tree_extension(true)
        .build()
}

fn to_bytes(message: &MlsMessageOut) -> Result<Vec<u8>, MlsError> {
    message.tls_serialize_detached().map_err(lib)
}

fn parse_identity(credential: &openmls::prelude::Credential) -> Option<LeafIdentity> {
    openmls::prelude::BasicCredential::try_from(credential.clone())
        .ok()
        .and_then(|b| LeafIdentity::parse(b.identity()).ok())
}

impl Conversation {
    /// Creates the group for `channel_id` with this device as its only leaf
    /// (the accepter on an accepted DM request, section 4).
    ///
    /// # Errors
    /// When OpenMLS or storage fails.
    pub fn create<K: MlsKv>(
        provider: &Provider<K>,
        signer: &SignatureKeyPair,
        me: &LeafIdentity,
        channel_id: &[u8],
    ) -> Result<Self, MlsError> {
        let config = MlsGroupCreateConfig::builder()
            .ciphersuite(SUITE)
            .use_ratchet_tree_extension(true)
            .max_past_epochs(PAST_EPOCHS)
            .build();
        let group = MlsGroup::new_with_group_id(
            provider,
            signer,
            &config,
            GroupId::from_slice(channel_id),
            me.credential(signer),
        )
        .map_err(lib)?;
        Ok(Self { group })
    }

    /// Loads the group for `channel_id` from storage, if this device has one.
    ///
    /// # Errors
    /// When storage fails.
    pub fn load<K: MlsKv>(
        provider: &Provider<K>,
        channel_id: &[u8],
    ) -> Result<Option<Self>, MlsError> {
        MlsGroup::load(provider.storage(), &GroupId::from_slice(channel_id))
            .map(|g| g.map(|group| Self { group }))
            .map_err(|e| MlsError::Storage(e.to_string()))
    }

    /// Joins from a welcome addressed to one of this device's key packages.
    /// Every member leaf is validated; one bad leaf refuses the welcome.
    ///
    /// # Errors
    /// [`MlsError::Malformed`] when the bytes are not a welcome, the
    /// OpenMLS error when it is not for this device, or
    /// [`MlsError::LeafRejected`].
    pub fn join<K: MlsKv>(
        provider: &Provider<K>,
        welcome: &[u8],
        lookup: &dyn RosterLookup,
    ) -> Result<Self, MlsError> {
        let message = MlsMessageIn::tls_deserialize_exact(welcome)
            .map_err(|e| MlsError::Malformed(e.to_string()))?;
        let welcome = match message.extract() {
            MlsMessageBodyIn::Welcome(w) => w,
            _ => return Err(MlsError::Malformed("not a welcome".into())),
        };
        if welcome.ciphersuite() != SUITE {
            return Err(MlsError::Ciphersuite);
        }
        let staged = StagedWelcome::new_from_welcome(provider, &join_config(), welcome, None)
            .map_err(lib)?;
        for member in staged.members() {
            validate_leaf(&member.credential, &member.signature_key, lookup)?;
        }
        let group = staged.into_group(provider).map_err(lib)?;
        Ok(Self { group })
    }

    /// Joins by external commit from a published GroupInfo (section 10).
    /// Returns the group, the commit to send as a `CONTROL` event (the
    /// group is already in the new epoch, so there is nothing to merge),
    /// and the leaves the rosters do not vouch for. A recovering device
    /// expects its own identity's revoked devices among those and removes
    /// them in the commit that follows; anything else there is the
    /// caller's reason to drop the group. OpenMLS 0.9 external commits
    /// cannot carry removal proposals by value, which is why the removal
    /// is a second commit rather than part of this one.
    ///
    /// # Errors
    /// [`MlsError::Malformed`] when the bytes are not a GroupInfo, or the
    /// OpenMLS error when the join fails.
    pub fn external_join<K: MlsKv>(
        provider: &Provider<K>,
        signer: &SignatureKeyPair,
        me: &LeafIdentity,
        group_info: &[u8],
        lookup: &dyn RosterLookup,
    ) -> Result<(Self, Commit, Vec<LeafIdentity>), MlsError> {
        let message = MlsMessageIn::tls_deserialize_exact(group_info)
            .map_err(|e| MlsError::Malformed(e.to_string()))?;
        let info = match message.extract() {
            MlsMessageBodyIn::GroupInfo(info) => info,
            _ => return Err(MlsError::Malformed("not a group info".into())),
        };
        if info.ciphersuite() != SUITE {
            return Err(MlsError::Ciphersuite);
        }
        let (group, bundle) = MlsGroup::external_commit_builder()
            .with_config(join_config())
            .build_group(provider, info, me.credential(signer))
            .map_err(lib)?
            .load_psks(provider.storage())
            .map_err(lib)?
            .build(provider.rand(), provider.crypto(), signer, |_| true)
            .map_err(lib)?
            .finalize(provider)
            .map_err(lib)?;
        let conversation = Self { group };
        let mut unvouched = Vec::new();
        for member in conversation.group.members() {
            if member.index != conversation.group.own_leaf_index()
                && validate_leaf(&member.credential, &member.signature_key, lookup).is_err()
            {
                if let Some(leaf) = parse_identity(&member.credential) {
                    unvouched.push(leaf);
                }
            }
        }
        let (commit, _, _) = bundle.into_contents();
        let commit = Commit {
            commit: to_bytes(&commit)?,
            welcome: None,
            epoch: conversation.epoch(),
        };
        Ok((conversation, commit, unvouched))
    }

    /// Deletes this device's state for the group, as after losing a
    /// creation race to another device whose welcome is on its way.
    ///
    /// # Errors
    /// When storage fails.
    pub fn delete<K: MlsKv>(mut self, provider: &Provider<K>) -> Result<(), MlsError> {
        self.group
            .delete(provider.storage())
            .map_err(|e| MlsError::Storage(e.to_string()))
    }

    /// The current epoch.
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.group.epoch().as_u64()
    }

    /// The channel id.
    #[must_use]
    pub fn channel_id(&self) -> &[u8] {
        self.group.group_id().as_slice()
    }

    /// Whether this device is still a member.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.group.is_active()
    }

    /// Whether a commit is staged, waiting for its ack.
    #[must_use]
    pub fn has_pending_commit(&self) -> bool {
        self.group.pending_commit().is_some()
    }

    /// Every leaf by identity, with its index.
    #[must_use]
    pub fn members(&self) -> Vec<(LeafIdentity, u32)> {
        self.group
            .members()
            .filter_map(|m| parse_identity(&m.credential).map(|id| (id, m.index.u32())))
            .collect()
    }

    /// Stages a commit adding `packages` (already validated by
    /// [`super::key_packages::parse_key_package`]).
    ///
    /// # Errors
    /// [`MlsError::PendingCommit`] when one is already staged, or the
    /// OpenMLS error.
    pub fn add<K: MlsKv>(
        &mut self,
        provider: &Provider<K>,
        signer: &SignatureKeyPair,
        packages: &[KeyPackage],
    ) -> Result<Commit, MlsError> {
        self.ensure_no_pending()?;
        let (commit, welcome, _) = self
            .group
            .add_members(provider, signer, packages)
            .map_err(lib)?;
        Ok(Commit {
            commit: to_bytes(&commit)?,
            welcome: Some(to_bytes(&welcome)?),
            epoch: self.epoch() + 1,
        })
    }

    /// Stages a commit removing every leaf whose device id is in `devices`.
    /// Devices with no leaf are ignored; with none left to remove, this is
    /// `Ok(None)` and nothing is staged (the retry after a lost race).
    ///
    /// # Errors
    /// [`MlsError::PendingCommit`] when one is already staged, or the
    /// OpenMLS error.
    pub fn remove<K: MlsKv>(
        &mut self,
        provider: &Provider<K>,
        signer: &SignatureKeyPair,
        devices: &[Vec<u8>],
    ) -> Result<Option<Commit>, MlsError> {
        self.ensure_no_pending()?;
        let indices: Vec<LeafNodeIndex> = self
            .members()
            .into_iter()
            .filter(|(id, _)| devices.contains(&id.device_id))
            .map(|(_, index)| LeafNodeIndex::new(index))
            .collect();
        if indices.is_empty() {
            return Ok(None);
        }
        let (commit, welcome, _) = self
            .group
            .remove_members(provider, signer, &indices)
            .map_err(lib)?;
        Ok(Some(Commit {
            commit: to_bytes(&commit)?,
            welcome: welcome.as_ref().map(to_bytes).transpose()?,
            epoch: self.epoch() + 1,
        }))
    }

    /// Stages a self-remove proposal and commit: this device leaves. The
    /// commit is by another member; here we only produce the proposal.
    ///
    /// # Errors
    /// The OpenMLS error.
    pub fn leave<K: MlsKv>(
        &mut self,
        provider: &Provider<K>,
        signer: &SignatureKeyPair,
    ) -> Result<Vec<u8>, MlsError> {
        let proposal = self.group.leave_group(provider, signer).map_err(lib)?;
        to_bytes(&proposal)
    }

    /// Merges the staged commit after the instance acked it.
    ///
    /// # Errors
    /// The OpenMLS error.
    pub fn merge_pending<K: MlsKv>(&mut self, provider: &Provider<K>) -> Result<(), MlsError> {
        self.group.merge_pending_commit(provider).map_err(lib)
    }

    /// Drops the staged commit after the instance refused it; the caller
    /// then processes what it missed and tries again.
    ///
    /// # Errors
    /// When storage fails.
    pub fn discard_pending<K: MlsKv>(&mut self, provider: &Provider<K>) -> Result<(), MlsError> {
        self.group
            .clear_pending_commit(provider.storage())
            .map_err(|e| MlsError::Storage(e.to_string()))
    }

    /// Encrypts an operation for the current epoch.
    ///
    /// # Errors
    /// The OpenMLS error (including when this device is no longer a member).
    pub fn encrypt<K: MlsKv>(
        &mut self,
        provider: &Provider<K>,
        signer: &SignatureKeyPair,
        op: &DmOperation,
    ) -> Result<Outgoing, MlsError> {
        let message = self
            .group
            .create_message(provider, signer, &op.encode_to_vec())
            .map_err(lib)?;
        Ok(Outgoing {
            bytes: to_bytes(&message)?,
            epoch: self.epoch(),
        })
    }

    /// Exports the GroupInfo with the ratchet tree for `POST /mls/group-info`
    /// (section 6), signed by this leaf.
    ///
    /// # Errors
    /// The OpenMLS error.
    pub fn group_info<K: MlsKv>(
        &self,
        provider: &Provider<K>,
        signer: &SignatureKeyPair,
    ) -> Result<Vec<u8>, MlsError> {
        let message = self
            .group
            .export_group_info(provider.crypto(), signer, true)
            .map_err(lib)?;
        to_bytes(&message)
    }

    /// Processes one `mls_message` from the conversation's log, in seq
    /// order. Commits are merged; proposals are queued; application
    /// messages are decrypted and their sender validated.
    ///
    /// # Errors
    /// [`MlsError::FutureEpoch`] when the group has not reached the
    /// message's epoch (hold it); [`MlsError::BeforeJoin`] for an epoch
    /// before this device joined; [`MlsError::Undecryptable`] when the
    /// epoch is right but decryption failed; [`MlsError::LeafRejected`]
    /// when the sender or an added leaf fails roster validation.
    pub fn handle<K: MlsKv>(
        &mut self,
        provider: &Provider<K>,
        bytes: &[u8],
        lookup: &dyn RosterLookup,
    ) -> Result<Incoming, MlsError> {
        let message = MlsMessageIn::tls_deserialize_exact(bytes)
            .map_err(|e| MlsError::Malformed(e.to_string()))?;
        let protocol: ProtocolMessage = match message.extract() {
            MlsMessageBodyIn::PrivateMessage(m) => m.into(),
            MlsMessageBodyIn::PublicMessage(m) => m.into(),
            MlsMessageBodyIn::Welcome(_) => return Ok(Incoming::Welcome),
            _ => return Err(MlsError::Malformed("unexpected message type".into())),
        };
        let epoch = protocol.epoch().as_u64();
        let current = self.epoch();
        if epoch > current {
            return Err(MlsError::FutureEpoch { epoch, current });
        }
        let processed = self
            .group
            .process_message(provider, protocol)
            .map_err(|e| match e {
                ProcessMessageError::ValidationError(ValidationError::NoPastEpochData) => {
                    MlsError::BeforeJoin
                }
                ProcessMessageError::ValidationError(ValidationError::UnableToDecrypt(_)) => {
                    MlsError::Undecryptable
                }
                other => lib(other),
            })?;

        // The sender's leaf, as the group knows it before this message.
        let sender_index = match processed.sender() {
            Sender::Member(index) => Some(*index),
            Sender::NewMemberCommit => None,
            Sender::External(_) | Sender::NewMemberProposal => {
                return Err(MlsError::LeafRejected("external senders are not used"))
            }
        };
        let sender = match sender_index {
            Some(index) => {
                let member = self
                    .group
                    .member_at(index)
                    .ok_or(MlsError::LeafRejected("sender is not a member"))?;
                Some(validate_leaf(
                    &member.credential,
                    &member.signature_key,
                    lookup,
                )?)
            }
            None => None,
        };

        match processed.into_content() {
            ProcessedMessageContent::ApplicationMessage(m) => {
                let sender = sender.ok_or(MlsError::LeafRejected(
                    "application message without a member sender",
                ))?;
                let op = DmOperation::decode(m.into_bytes().as_slice())
                    .map_err(|e| MlsError::Malformed(e.to_string()))?;
                Ok(Incoming::Operation { sender, epoch, op })
            }
            ProcessedMessageContent::StagedCommitMessage(staged) => {
                self.merge(provider, *staged, sender, lookup)
            }
            ProcessedMessageContent::ProposalMessage(proposal) => {
                sender.ok_or(MlsError::LeafRejected("proposal without a member sender"))?;
                self.group
                    .store_pending_proposal(provider.storage(), *proposal)
                    .map_err(|e| MlsError::Storage(e.to_string()))?;
                Ok(Incoming::Proposal { epoch })
            }
            ProcessedMessageContent::ExternalJoinProposalMessage(_) => Err(MlsError::LeafRejected(
                "external join proposals are not used",
            )),
            ProcessedMessageContent::OwnPendingCommit
            | ProcessedMessageContent::OwnPrivateMessage => Ok(Incoming::Own),
        }
    }

    fn merge<K: MlsKv>(
        &mut self,
        provider: &Provider<K>,
        staged: StagedCommit,
        sender: Option<LeafIdentity>,
        lookup: &dyn RosterLookup,
    ) -> Result<Incoming, MlsError> {
        // A new member's commit carries its own leaf in the path.
        let joined = if sender.is_none() {
            let leaf = staged
                .update_path_leaf_node()
                .ok_or(MlsError::LeafRejected("external commit without a path"))?;
            Some(validate_leaf(
                leaf.credential(),
                leaf.signature_key().as_slice(),
                lookup,
            )?)
        } else {
            None
        };
        let mut added = Vec::new();
        for add in staged.add_proposals() {
            let leaf = add.add_proposal().key_package().leaf_node();
            added.push(validate_leaf(
                leaf.credential(),
                leaf.signature_key().as_slice(),
                lookup,
            )?);
        }
        let removed: Vec<LeafIdentity> = staged
            .remove_proposals()
            .filter_map(|r| self.group.member_at(r.remove_proposal().removed()))
            .filter_map(|m| parse_identity(&m.credential))
            .collect();
        let self_removed = staged.self_removed();
        self.group
            .merge_staged_commit(provider, staged)
            .map_err(lib)?;
        Ok(Incoming::Commit {
            epoch: self.epoch(),
            added,
            removed,
            joined,
            self_removed,
        })
    }

    fn ensure_no_pending(&self) -> Result<(), MlsError> {
        if self.has_pending_commit() {
            Err(MlsError::PendingCommit)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;

    use super::super::credential::device_signer;
    use super::super::key_packages::{new_key_package, parse_key_package};
    use super::super::storage::MemoryKv;
    use super::*;
    use crate::proto::{dm_operation, DmSend};

    /// A device with its own provider, like one client.
    struct Dev {
        me: LeafIdentity,
        signer: SignatureKeyPair,
        provider: Provider<MemoryKv>,
    }

    fn dev(identity: u8, device: u8) -> Dev {
        let me = LeafIdentity {
            identity_id: vec![identity; 32],
            device_id: vec![device; 16],
        };
        Dev {
            me,
            signer: device_signer(&[device; 32]).unwrap(),
            provider: Provider::new(MemoryKv::default()),
        }
    }

    /// Rosters as every participant holds them: identity -> device -> key.
    fn rosters(devs: &[&Dev]) -> BTreeMap<(Vec<u8>, Vec<u8>), Vec<u8>> {
        devs.iter()
            .map(|d| {
                (
                    (d.me.identity_id.clone(), d.me.device_id.clone()),
                    d.signer.public().to_vec(),
                )
            })
            .collect()
    }

    fn lookup(
        rosters: &BTreeMap<(Vec<u8>, Vec<u8>), Vec<u8>>,
    ) -> impl Fn(&[u8], &[u8]) -> Option<Vec<u8>> + '_ {
        move |i: &[u8], d: &[u8]| rosters.get(&(i.to_vec(), d.to_vec())).cloned()
    }

    fn send_op(text: &str) -> DmOperation {
        DmOperation {
            operation_id: vec![5; 16],
            sent_at: None,
            op: Some(dm_operation::Op::Send(DmSend {
                message_id: vec![5; 16],
                body: text.into(),
                ..Default::default()
            })),
        }
    }

    fn key_package(d: &Dev, lookup: &dyn RosterLookup, adder: &Dev) -> KeyPackage {
        let bytes = new_key_package(&d.provider, &d.signer, &d.me, 3600, false).unwrap();
        parse_key_package(&adder.provider, &bytes, lookup)
            .unwrap()
            .0
    }

    /// Architecture section 17's MLS row: encrypt or decrypt of a message
    /// in a 10-member group on a desktop, 1 ms. Timed, so it runs only
    /// when asked for, in release:
    ///
    ///     cargo test -p lumia-core --release mls_budget -- --ignored --nocapture
    ///
    /// Ten devices join one group, one of them encrypts a thousand
    /// 1 KB operations and another decrypts each; the budget is checked
    /// at p99, in a release build only (debug builds are many times
    /// slower and say nothing about the budget).
    #[test]
    #[ignore = "a timed budget; run explicitly in release"]
    fn mls_budget() {
        let devs: Vec<Dev> = (1..=10).map(|i| dev(i, 10 + i)).collect();
        let rosters = rosters(&devs.iter().collect::<Vec<_>>());
        let look = lookup(&rosters);
        let channel = vec![7; 16];

        let creator = &devs[0];
        let mut creator_group =
            Conversation::create(&creator.provider, &creator.signer, &creator.me, &channel)
                .unwrap();
        let packages: Vec<KeyPackage> = devs[1..]
            .iter()
            .map(|d| key_package(d, &look, creator))
            .collect();
        let commit = creator_group
            .add(&creator.provider, &creator.signer, &packages)
            .unwrap();
        creator_group.merge_pending(&creator.provider).unwrap();
        let welcome = commit.welcome.unwrap();
        let sender = &devs[1];
        let mut sender_group = Conversation::join(&sender.provider, &welcome, &look).unwrap();
        assert_eq!(sender_group.members().len(), 10);

        let body = "x".repeat(1_024);
        let mut encrypts = Vec::with_capacity(1_000);
        let mut decrypts = Vec::with_capacity(1_000);
        let mut wire_bytes = 0;
        for _ in 0..1_000 {
            let op = send_op(&body);
            let started = std::time::Instant::now();
            let out = sender_group
                .encrypt(&sender.provider, &sender.signer, &op)
                .unwrap();
            encrypts.push(started.elapsed());
            wire_bytes = out.bytes.len();

            let started = std::time::Instant::now();
            let incoming = creator_group
                .handle(&creator.provider, &out.bytes, &look)
                .unwrap();
            decrypts.push(started.elapsed());
            assert!(matches!(incoming, Incoming::Operation { .. }));
        }

        let report = |label: &str, samples: &mut Vec<std::time::Duration>| {
            samples.sort();
            let at = |p: f64| samples[((samples.len() as f64 * p) as usize).min(samples.len() - 1)];
            println!(
                "{label}: p50 {:?} p95 {:?} p99 {:?} max {:?}",
                at(0.5),
                at(0.95),
                at(0.99),
                samples[samples.len() - 1],
            );
            at(0.99)
        };
        println!("ciphertext: {wire_bytes} bytes for a 1,024-byte body");
        let encrypt_p99 = report("encrypt, 10 members, 1 KB", &mut encrypts);
        let decrypt_p99 = report("decrypt, 10 members, 1 KB", &mut decrypts);
        if !cfg!(debug_assertions) {
            let budget = std::time::Duration::from_millis(1);
            assert!(
                encrypt_p99 < budget,
                "encrypt p99 {encrypt_p99:?} over the 1 ms budget"
            );
            assert!(
                decrypt_p99 < budget,
                "decrypt p99 {decrypt_p99:?} over the 1 ms budget"
            );
        }
    }

    #[test]
    fn create_add_welcome_and_message_round_trip_across_three_devices() {
        let b = dev(2, 20);
        let a1 = dev(1, 11);
        let a2 = dev(1, 12);
        let rosters = rosters(&[&b, &a1, &a2]);
        let look = lookup(&rosters);
        let channel = vec![7; 16];

        // B accepts the DM: creates the group and adds both of A's devices.
        let mut gb = Conversation::create(&b.provider, &b.signer, &b.me, &channel).unwrap();
        let packages = [key_package(&a1, &look, &b), key_package(&a2, &look, &b)];
        let commit = gb.add(&b.provider, &b.signer, &packages).unwrap();
        assert_eq!(commit.epoch, 1);
        assert!(gb.has_pending_commit());
        gb.merge_pending(&b.provider).unwrap();
        assert_eq!(gb.epoch(), 1);

        let welcome = commit.welcome.unwrap();
        let mut ga1 = Conversation::join(&a1.provider, &welcome, &look).unwrap();
        let mut ga2 = Conversation::join(&a2.provider, &welcome, &look).unwrap();
        assert_eq!(ga1.members().len(), 3);

        // A1 sends; B and A2 read it.
        let out = ga1
            .encrypt(&a1.provider, &a1.signer, &send_op("hi"))
            .unwrap();
        assert_eq!(out.epoch, 1);
        for (g, p) in [(&mut gb, &b.provider), (&mut ga2, &a2.provider)] {
            match g.handle(p, &out.bytes, &look).unwrap() {
                Incoming::Operation { sender, op, .. } => {
                    assert_eq!(sender, a1.me);
                    assert!(matches!(op.op, Some(dm_operation::Op::Send(ref s)) if s.body == "hi"));
                }
                other => panic!("{other:?}"),
            }
        }
    }

    #[test]
    fn a_revoked_leaf_is_refused_immediately_and_removed_by_one_commit() {
        let b = dev(2, 20);
        let a1 = dev(1, 11);
        let a2 = dev(1, 12);
        let mut rosters = rosters(&[&b, &a1, &a2]);
        let channel = vec![7; 16];
        let (mut gb, mut ga1, mut ga2) = {
            let look = lookup(&rosters);
            let mut gb = Conversation::create(&b.provider, &b.signer, &b.me, &channel).unwrap();
            let packages = [key_package(&a1, &look, &b), key_package(&a2, &look, &b)];
            let commit = gb.add(&b.provider, &b.signer, &packages).unwrap();
            gb.merge_pending(&b.provider).unwrap();
            let welcome = commit.welcome.unwrap();
            (
                gb,
                Conversation::join(&a1.provider, &welcome, &look).unwrap(),
                Conversation::join(&a2.provider, &welcome, &look).unwrap(),
            )
        };

        // A revokes A2: the roster no longer vouches for it.
        rosters.remove(&(a2.me.identity_id.clone(), a2.me.device_id.clone()));
        let look = lookup(&rosters);
        let out = ga2
            .encrypt(&a2.provider, &a2.signer, &send_op("ghost"))
            .unwrap();
        assert!(matches!(
            gb.handle(&b.provider, &out.bytes, &look),
            Err(MlsError::LeafRejected(_))
        ));

        // B and A1 both try to remove it; B's commit lands first.
        let cb = gb
            .remove(
                &b.provider,
                &b.signer,
                std::slice::from_ref(&a2.me.device_id),
            )
            .unwrap()
            .unwrap();
        let ca1 = ga1
            .remove(
                &a1.provider,
                &a1.signer,
                std::slice::from_ref(&a2.me.device_id),
            )
            .unwrap()
            .unwrap();
        assert_eq!((cb.epoch, ca1.epoch), (2, 2));
        gb.merge_pending(&b.provider).unwrap();

        // A1's commit is refused by the instance (epoch 2 taken): discard,
        // process B's, retry, and there is nothing left to remove.
        ga1.discard_pending(&a1.provider).unwrap();
        match ga1.handle(&a1.provider, &cb.commit, &look).unwrap() {
            Incoming::Commit { epoch, removed, .. } => {
                assert_eq!(epoch, 2);
                assert_eq!(removed, vec![a2.me.clone()]);
            }
            other => panic!("{other:?}"),
        }
        assert!(ga1
            .remove(
                &a1.provider,
                &a1.signer,
                std::slice::from_ref(&a2.me.device_id)
            )
            .unwrap()
            .is_none());
        assert_eq!(gb.members().len(), 2);
        assert_eq!(ga1.members().len(), 2);
    }

    #[test]
    fn a_key_package_from_an_unrostered_device_cannot_be_added() {
        let b = dev(2, 20);
        let stranger = dev(3, 30);
        let rosters = rosters(&[&b]);
        let look = lookup(&rosters);
        let bytes = new_key_package(
            &stranger.provider,
            &stranger.signer,
            &stranger.me,
            3600,
            false,
        )
        .unwrap();
        assert!(matches!(
            parse_key_package(&b.provider, &bytes, &look),
            Err(MlsError::LeafRejected(_))
        ));
    }

    #[test]
    fn future_epoch_messages_are_held_and_group_info_admits_an_external_join() {
        let b = dev(2, 20);
        let a1 = dev(1, 11);
        let a3 = dev(1, 13);
        let a4 = dev(1, 14);
        let mut rosters = rosters(&[&b, &a1, &a3]);
        let channel = vec![7; 16];
        let (mut gb, mut ga1, commit2, out) = {
            let look = lookup(&rosters);
            let mut gb = Conversation::create(&b.provider, &b.signer, &b.me, &channel).unwrap();
            let commit = gb
                .add(&b.provider, &b.signer, &[key_package(&a1, &look, &b)])
                .unwrap();
            gb.merge_pending(&b.provider).unwrap();
            let mut ga1 =
                Conversation::join(&a1.provider, &commit.welcome.unwrap(), &look).unwrap();

            // B commits again (a self-update by removing nobody is not possible,
            // so add A3 by key package) and sends in the new epoch before A1
            // has seen the commit: A1 holds the message.
            let commit2 = gb
                .add(&b.provider, &b.signer, &[key_package(&a3, &look, &b)])
                .unwrap();
            gb.merge_pending(&b.provider).unwrap();
            let out = gb
                .encrypt(&b.provider, &b.signer, &send_op("later"))
                .unwrap();
            assert!(matches!(
                ga1.handle(&a1.provider, &out.bytes, &look),
                Err(MlsError::FutureEpoch {
                    epoch: 2,
                    current: 1
                })
            ));
            match ga1.handle(&a1.provider, &commit2.commit, &look).unwrap() {
                Incoming::Commit { added, .. } => assert_eq!(added, vec![a3.me.clone()]),
                other => panic!("{other:?}"),
            }
            assert!(matches!(
                ga1.handle(&a1.provider, &out.bytes, &look).unwrap(),
                Incoming::Operation { .. }
            ));
            (gb, ga1, commit2, out)
        };
        let _ = (commit2, out, &mut ga1);

        // A recovered device (A4) joins from B's GroupInfo by external commit.
        rosters.insert(
            (a4.me.identity_id.clone(), a4.me.device_id.clone()),
            a4.signer.public().to_vec(),
        );
        let look = lookup(&rosters);
        let info = gb.group_info(&b.provider, &b.signer).unwrap();
        let (mut ga4, join, unvouched) =
            Conversation::external_join(&a4.provider, &a4.signer, &a4.me, &info, &look).unwrap();
        assert_eq!(join.epoch, 3);
        assert!(unvouched.is_empty());
        match gb.handle(&b.provider, &join.commit, &look).unwrap() {
            Incoming::Commit { joined, epoch, .. } => {
                assert_eq!(joined, Some(a4.me.clone()));
                assert_eq!(epoch, 3);
            }
            other => panic!("{other:?}"),
        }
        let out = gb
            .encrypt(&b.provider, &b.signer, &send_op("welcome back"))
            .unwrap();
        assert!(matches!(
            ga4.handle(&a4.provider, &out.bytes, &look).unwrap(),
            Incoming::Operation { .. }
        ));
    }

    #[test]
    fn state_survives_a_reload_from_storage() {
        let b = dev(2, 20);
        let a1 = dev(1, 11);
        let rosters = rosters(&[&b, &a1]);
        let look = lookup(&rosters);
        let channel = vec![7; 16];
        let mut gb = Conversation::create(&b.provider, &b.signer, &b.me, &channel).unwrap();
        let commit = gb
            .add(&b.provider, &b.signer, &[key_package(&a1, &look, &b)])
            .unwrap();
        gb.merge_pending(&b.provider).unwrap();
        Conversation::join(&a1.provider, &commit.welcome.unwrap(), &look).unwrap();

        // Drop the handle and load the group back from the same storage.
        let mut ga1 = Conversation::load(&a1.provider, &channel).unwrap().unwrap();
        assert_eq!(ga1.epoch(), 1);
        let out = gb
            .encrypt(&b.provider, &b.signer, &send_op("still here"))
            .unwrap();
        assert!(matches!(
            ga1.handle(&a1.provider, &out.bytes, &look).unwrap(),
            Incoming::Operation { .. }
        ));
        assert!(Conversation::load(&a1.provider, &[9; 16])
            .unwrap()
            .is_none());
    }
}
