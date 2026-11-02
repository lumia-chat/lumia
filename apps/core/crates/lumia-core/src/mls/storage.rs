//! The OpenMLS storage provider over a key-value contract (spec section 7).
//!
//! OpenMLS persists through `openmls_traits::storage::StorageProvider`, 72
//! typed methods over two shapes: single values keyed by `(kind, key)` and
//! appended lists keyed the same way. [`MlsKv`] is exactly those two shapes;
//! a transport implements it over its store so that MLS state commits in
//! the same transaction as the outbox and the cursor. [`KvStorage`] maps
//! the typed trait onto it with the same serde-JSON forms OpenMLS's own
//! memory provider uses, and [`Provider`] bundles it with the Rust crypto
//! provider for every `MlsGroup` call.

use std::cell::RefCell;
use std::collections::BTreeMap;

use openmls_rust_crypto::RustCrypto;
use openmls_traits::storage::{traits, Entity, Key, StorageProvider, CURRENT_VERSION};
use openmls_traits::OpenMlsProvider;

/// The two shapes OpenMLS stores: values and lists, keyed by kind and key.
/// Every method takes `&self` because OpenMLS calls its provider through a
/// shared reference; an implementation over a database connection inside a
/// transaction satisfies this naturally.
pub trait MlsKv {
    /// The implementation's error.
    type Error: std::error::Error + std::fmt::Debug;

    /// Reads a value.
    fn get(&self, kind: &str, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error>;
    /// Writes or replaces a value.
    fn set(&self, kind: &str, key: &[u8], value: &[u8]) -> Result<(), Self::Error>;
    /// Removes a value; absent is not an error.
    fn delete(&self, kind: &str, key: &[u8]) -> Result<(), Self::Error>;
    /// Reads a list in append order.
    fn list(&self, kind: &str, key: &[u8]) -> Result<Vec<Vec<u8>>, Self::Error>;
    /// Appends to a list.
    fn append(&self, kind: &str, key: &[u8], value: &[u8]) -> Result<(), Self::Error>;
    /// Removes the first element equal to `value` from a list.
    fn remove_item(&self, kind: &str, key: &[u8], value: &[u8]) -> Result<(), Self::Error>;
    /// Removes a whole list.
    fn clear_list(&self, kind: &str, key: &[u8]) -> Result<(), Self::Error>;
}

type KvKey = (String, Vec<u8>);

impl<T: MlsKv> MlsKv for &T {
    type Error = T::Error;

    fn get(&self, kind: &str, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
        (**self).get(kind, key)
    }

    fn set(&self, kind: &str, key: &[u8], value: &[u8]) -> Result<(), Self::Error> {
        (**self).set(kind, key, value)
    }

    fn delete(&self, kind: &str, key: &[u8]) -> Result<(), Self::Error> {
        (**self).delete(kind, key)
    }

    fn list(&self, kind: &str, key: &[u8]) -> Result<Vec<Vec<u8>>, Self::Error> {
        (**self).list(kind, key)
    }

    fn append(&self, kind: &str, key: &[u8], value: &[u8]) -> Result<(), Self::Error> {
        (**self).append(kind, key, value)
    }

    fn remove_item(&self, kind: &str, key: &[u8], value: &[u8]) -> Result<(), Self::Error> {
        (**self).remove_item(kind, key, value)
    }

    fn clear_list(&self, kind: &str, key: &[u8]) -> Result<(), Self::Error> {
        (**self).clear_list(kind, key)
    }
}

/// An in-memory [`MlsKv`] for tests and for the WASM session before the
/// OPFS store exists.
#[derive(Debug, Default)]
pub struct MemoryKv {
    values: RefCell<BTreeMap<KvKey, Vec<u8>>>,
    lists: RefCell<BTreeMap<KvKey, Vec<Vec<u8>>>>,
}

/// An error [`MemoryKv`] never actually returns.
#[derive(Debug, thiserror::Error)]
#[error("memory kv")]
pub struct Never;

impl MlsKv for MemoryKv {
    type Error = Never;

    fn get(&self, kind: &str, key: &[u8]) -> Result<Option<Vec<u8>>, Never> {
        Ok(self
            .values
            .borrow()
            .get(&(kind.to_string(), key.to_vec()))
            .cloned())
    }

    fn set(&self, kind: &str, key: &[u8], value: &[u8]) -> Result<(), Never> {
        self.values
            .borrow_mut()
            .insert((kind.to_string(), key.to_vec()), value.to_vec());
        Ok(())
    }

    fn delete(&self, kind: &str, key: &[u8]) -> Result<(), Never> {
        self.values
            .borrow_mut()
            .remove(&(kind.to_string(), key.to_vec()));
        Ok(())
    }

    fn list(&self, kind: &str, key: &[u8]) -> Result<Vec<Vec<u8>>, Never> {
        Ok(self
            .lists
            .borrow()
            .get(&(kind.to_string(), key.to_vec()))
            .cloned()
            .unwrap_or_default())
    }

    fn append(&self, kind: &str, key: &[u8], value: &[u8]) -> Result<(), Never> {
        self.lists
            .borrow_mut()
            .entry((kind.to_string(), key.to_vec()))
            .or_default()
            .push(value.to_vec());
        Ok(())
    }

    fn remove_item(&self, kind: &str, key: &[u8], value: &[u8]) -> Result<(), Never> {
        if let Some(list) = self
            .lists
            .borrow_mut()
            .get_mut(&(kind.to_string(), key.to_vec()))
        {
            if let Some(pos) = list.iter().position(|v| v == value) {
                list.remove(pos);
            }
        }
        Ok(())
    }

    fn clear_list(&self, kind: &str, key: &[u8]) -> Result<(), Never> {
        self.lists
            .borrow_mut()
            .remove(&(kind.to_string(), key.to_vec()));
        Ok(())
    }
}

/// Why the storage provider failed: the backing store, or a value that did
/// not (de)serialize.
#[derive(Debug, thiserror::Error)]
pub enum KvStorageError {
    /// The [`MlsKv`] failed.
    #[error("{0}")]
    Backend(String),
    /// A stored value did not match its type.
    #[error("serialization: {0}")]
    Serialization(String),
}

impl From<serde_json::Error> for KvStorageError {
    fn from(e: serde_json::Error) -> Self {
        Self::Serialization(e.to_string())
    }
}

/// The typed OpenMLS storage provider over an [`MlsKv`].
#[derive(Debug)]
pub struct KvStorage<K: MlsKv> {
    kv: K,
}

impl<K: MlsKv> KvStorage<K> {
    /// Wraps a key-value store.
    pub fn new(kv: K) -> Self {
        Self { kv }
    }

    /// The store underneath.
    pub fn kv(&self) -> &K {
        &self.kv
    }

    fn backend(e: K::Error) -> KvStorageError {
        KvStorageError::Backend(format!("{e:?}"))
    }

    fn write<Kk: Key<CURRENT_VERSION>, V: Entity<CURRENT_VERSION>>(
        &self,
        kind: &str,
        key: &Kk,
        value: &V,
    ) -> Result<(), KvStorageError> {
        self.kv
            .set(kind, &serde_json::to_vec(key)?, &serde_json::to_vec(value)?)
            .map_err(Self::backend)
    }

    fn read<Kk: Key<CURRENT_VERSION>, V: Entity<CURRENT_VERSION>>(
        &self,
        kind: &str,
        key: &Kk,
    ) -> Result<Option<V>, KvStorageError> {
        match self
            .kv
            .get(kind, &serde_json::to_vec(key)?)
            .map_err(Self::backend)?
        {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    fn remove<Kk: Key<CURRENT_VERSION>>(&self, kind: &str, key: &Kk) -> Result<(), KvStorageError> {
        self.kv
            .delete(kind, &serde_json::to_vec(key)?)
            .map_err(Self::backend)
    }

    fn read_list<Kk: Key<CURRENT_VERSION>, V: Entity<CURRENT_VERSION>>(
        &self,
        kind: &str,
        key: &Kk,
    ) -> Result<Vec<V>, KvStorageError> {
        self.kv
            .list(kind, &serde_json::to_vec(key)?)
            .map_err(Self::backend)?
            .iter()
            .map(|bytes| serde_json::from_slice(bytes).map_err(KvStorageError::from))
            .collect()
    }

    fn push<Kk: Key<CURRENT_VERSION>, V: Entity<CURRENT_VERSION>>(
        &self,
        kind: &str,
        key: &Kk,
        value: &V,
    ) -> Result<(), KvStorageError> {
        self.kv
            .append(kind, &serde_json::to_vec(key)?, &serde_json::to_vec(value)?)
            .map_err(Self::backend)
    }

    fn pull<Kk: Key<CURRENT_VERSION>, V: Entity<CURRENT_VERSION>>(
        &self,
        kind: &str,
        key: &Kk,
        value: &V,
    ) -> Result<(), KvStorageError> {
        self.kv
            .remove_item(kind, &serde_json::to_vec(key)?, &serde_json::to_vec(value)?)
            .map_err(Self::backend)
    }

    fn clear<Kk: Key<CURRENT_VERSION>>(&self, kind: &str, key: &Kk) -> Result<(), KvStorageError> {
        self.kv
            .clear_list(kind, &serde_json::to_vec(key)?)
            .map_err(Self::backend)
    }
}

// Kinds, named as OpenMLS's memory provider labels them.
const KEY_PACKAGE: &str = "KeyPackage";
const PSK: &str = "Psk";
const ENCRYPTION_KEY_PAIR: &str = "EncryptionKeyPair";
const SIGNATURE_KEY_PAIR: &str = "SignatureKeyPair";
const EPOCH_KEY_PAIRS: &str = "EpochKeyPairs";
const TREE: &str = "Tree";
const GROUP_CONTEXT: &str = "GroupContext";
const INTERIM_TRANSCRIPT_HASH: &str = "InterimTranscriptHash";
const CONFIRMATION_TAG: &str = "ConfirmationTag";
const JOIN_CONFIG: &str = "MlsGroupJoinConfig";
const OWN_LEAF_NODES: &str = "OwnLeafNodes";
const GROUP_STATE: &str = "GroupState";
const QUEUED_PROPOSAL: &str = "QueuedProposal";
const PROPOSAL_QUEUE_REFS: &str = "ProposalQueueRefs";
const OWN_LEAF_NODE_INDEX: &str = "OwnLeafNodeIndex";
const EPOCH_SECRETS: &str = "EpochSecrets";
const RESUMPTION_PSK_STORE: &str = "ResumptionPsk";
const MESSAGE_SECRETS: &str = "MessageSecrets";

#[derive(serde::Serialize)]
struct EpochKeyPairsKey<'a, G: serde::Serialize, E: serde::Serialize> {
    group_id: &'a G,
    epoch: &'a E,
    leaf_index: u32,
}

impl<G: Key<CURRENT_VERSION>, E: Key<CURRENT_VERSION>> Key<CURRENT_VERSION>
    for EpochKeyPairsKey<'_, G, E>
{
}

#[derive(serde::Serialize)]
struct ProposalKey<'a, G: serde::Serialize, R: serde::Serialize> {
    group_id: &'a G,
    proposal_ref: &'a R,
}

impl<G: Key<CURRENT_VERSION>, R: Key<CURRENT_VERSION>> Key<CURRENT_VERSION>
    for ProposalKey<'_, G, R>
{
}

impl<K: MlsKv> StorageProvider<CURRENT_VERSION> for KvStorage<K> {
    type Error = KvStorageError;

    fn write_mls_join_config<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MlsGroupJoinConfig: traits::MlsGroupJoinConfig<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        config: &MlsGroupJoinConfig,
    ) -> Result<(), Self::Error> {
        self.write(JOIN_CONFIG, group_id, config)
    }

    fn append_own_leaf_node<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNode: traits::LeafNode<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        leaf_node: &LeafNode,
    ) -> Result<(), Self::Error> {
        self.push(OWN_LEAF_NODES, group_id, leaf_node)
    }

    fn queue_proposal<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
        QueuedProposal: traits::QueuedProposal<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        proposal_ref: &ProposalRef,
        proposal: &QueuedProposal,
    ) -> Result<(), Self::Error> {
        self.write(
            QUEUED_PROPOSAL,
            &ProposalKey {
                group_id,
                proposal_ref,
            },
            proposal,
        )?;
        self.push(PROPOSAL_QUEUE_REFS, group_id, proposal_ref)
    }

    fn write_tree<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        TreeSync: traits::TreeSync<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        tree: &TreeSync,
    ) -> Result<(), Self::Error> {
        self.write(TREE, group_id, tree)
    }

    fn write_interim_transcript_hash<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        InterimTranscriptHash: traits::InterimTranscriptHash<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        interim_transcript_hash: &InterimTranscriptHash,
    ) -> Result<(), Self::Error> {
        self.write(INTERIM_TRANSCRIPT_HASH, group_id, interim_transcript_hash)
    }

    fn write_context<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupContext: traits::GroupContext<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        group_context: &GroupContext,
    ) -> Result<(), Self::Error> {
        self.write(GROUP_CONTEXT, group_id, group_context)
    }

    fn write_confirmation_tag<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ConfirmationTag: traits::ConfirmationTag<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        confirmation_tag: &ConfirmationTag,
    ) -> Result<(), Self::Error> {
        self.write(CONFIRMATION_TAG, group_id, confirmation_tag)
    }

    fn write_group_state<
        GroupState: traits::GroupState<CURRENT_VERSION>,
        GroupId: traits::GroupId<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        group_state: &GroupState,
    ) -> Result<(), Self::Error> {
        self.write(GROUP_STATE, group_id, group_state)
    }

    fn write_message_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MessageSecrets: traits::MessageSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        message_secrets: &MessageSecrets,
    ) -> Result<(), Self::Error> {
        self.write(MESSAGE_SECRETS, group_id, message_secrets)
    }

    fn write_resumption_psk_store<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ResumptionPskStore: traits::ResumptionPskStore<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        resumption_psk_store: &ResumptionPskStore,
    ) -> Result<(), Self::Error> {
        self.write(RESUMPTION_PSK_STORE, group_id, resumption_psk_store)
    }

    fn write_own_leaf_index<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNodeIndex: traits::LeafNodeIndex<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        own_leaf_index: &LeafNodeIndex,
    ) -> Result<(), Self::Error> {
        self.write(OWN_LEAF_NODE_INDEX, group_id, own_leaf_index)
    }

    fn write_group_epoch_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupEpochSecrets: traits::GroupEpochSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        group_epoch_secrets: &GroupEpochSecrets,
    ) -> Result<(), Self::Error> {
        self.write(EPOCH_SECRETS, group_id, group_epoch_secrets)
    }

    fn write_signature_key_pair<
        SignaturePublicKey: traits::SignaturePublicKey<CURRENT_VERSION>,
        SignatureKeyPair: traits::SignatureKeyPair<CURRENT_VERSION>,
    >(
        &self,
        public_key: &SignaturePublicKey,
        signature_key_pair: &SignatureKeyPair,
    ) -> Result<(), Self::Error> {
        self.write(SIGNATURE_KEY_PAIR, public_key, signature_key_pair)
    }

    fn write_encryption_key_pair<
        EncryptionKey: traits::EncryptionKey<CURRENT_VERSION>,
        HpkeKeyPair: traits::HpkeKeyPair<CURRENT_VERSION>,
    >(
        &self,
        public_key: &EncryptionKey,
        key_pair: &HpkeKeyPair,
    ) -> Result<(), Self::Error> {
        self.write(ENCRYPTION_KEY_PAIR, public_key, key_pair)
    }

    fn write_encryption_epoch_key_pairs<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        EpochKey: traits::EpochKey<CURRENT_VERSION>,
        HpkeKeyPair: traits::HpkeKeyPair<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        epoch: &EpochKey,
        leaf_index: u32,
        key_pairs: &[HpkeKeyPair],
    ) -> Result<(), Self::Error> {
        let key = EpochKeyPairsKey {
            group_id,
            epoch,
            leaf_index,
        };
        self.kv
            .set(
                EPOCH_KEY_PAIRS,
                &serde_json::to_vec(&key)?,
                &serde_json::to_vec(key_pairs)?,
            )
            .map_err(Self::backend)
    }

    fn write_key_package<
        HashReference: traits::HashReference<CURRENT_VERSION>,
        KeyPackage: traits::KeyPackage<CURRENT_VERSION>,
    >(
        &self,
        hash_ref: &HashReference,
        key_package: &KeyPackage,
    ) -> Result<(), Self::Error> {
        self.write(KEY_PACKAGE, hash_ref, key_package)
    }

    fn write_psk<
        PskId: traits::PskId<CURRENT_VERSION>,
        PskBundle: traits::PskBundle<CURRENT_VERSION>,
    >(
        &self,
        psk_id: &PskId,
        psk: &PskBundle,
    ) -> Result<(), Self::Error> {
        self.write(PSK, psk_id, psk)
    }

    fn mls_group_join_config<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MlsGroupJoinConfig: traits::MlsGroupJoinConfig<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<MlsGroupJoinConfig>, Self::Error> {
        self.read(JOIN_CONFIG, group_id)
    }

    fn own_leaf_nodes<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNode: traits::LeafNode<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<LeafNode>, Self::Error> {
        self.read_list(OWN_LEAF_NODES, group_id)
    }

    fn queued_proposal_refs<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<ProposalRef>, Self::Error> {
        self.read_list(PROPOSAL_QUEUE_REFS, group_id)
    }

    fn queued_proposals<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
        QueuedProposal: traits::QueuedProposal<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<(ProposalRef, QueuedProposal)>, Self::Error> {
        let refs: Vec<ProposalRef> = self.read_list(PROPOSAL_QUEUE_REFS, group_id)?;
        let mut out = Vec::with_capacity(refs.len());
        for proposal_ref in refs {
            let key = ProposalKey {
                group_id,
                proposal_ref: &proposal_ref,
            };
            if let Some(proposal) = self.read(QUEUED_PROPOSAL, &key)? {
                out.push((proposal_ref, proposal));
            }
        }
        Ok(out)
    }

    fn tree<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        TreeSync: traits::TreeSync<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<TreeSync>, Self::Error> {
        self.read(TREE, group_id)
    }

    fn group_context<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupContext: traits::GroupContext<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<GroupContext>, Self::Error> {
        self.read(GROUP_CONTEXT, group_id)
    }

    fn interim_transcript_hash<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        InterimTranscriptHash: traits::InterimTranscriptHash<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<InterimTranscriptHash>, Self::Error> {
        self.read(INTERIM_TRANSCRIPT_HASH, group_id)
    }

    fn confirmation_tag<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ConfirmationTag: traits::ConfirmationTag<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<ConfirmationTag>, Self::Error> {
        self.read(CONFIRMATION_TAG, group_id)
    }

    fn group_state<
        GroupState: traits::GroupState<CURRENT_VERSION>,
        GroupId: traits::GroupId<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<GroupState>, Self::Error> {
        self.read(GROUP_STATE, group_id)
    }

    fn message_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        MessageSecrets: traits::MessageSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<MessageSecrets>, Self::Error> {
        self.read(MESSAGE_SECRETS, group_id)
    }

    fn resumption_psk_store<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ResumptionPskStore: traits::ResumptionPskStore<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<ResumptionPskStore>, Self::Error> {
        self.read(RESUMPTION_PSK_STORE, group_id)
    }

    fn own_leaf_index<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        LeafNodeIndex: traits::LeafNodeIndex<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<LeafNodeIndex>, Self::Error> {
        self.read(OWN_LEAF_NODE_INDEX, group_id)
    }

    fn group_epoch_secrets<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        GroupEpochSecrets: traits::GroupEpochSecrets<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<GroupEpochSecrets>, Self::Error> {
        self.read(EPOCH_SECRETS, group_id)
    }

    fn signature_key_pair<
        SignaturePublicKey: traits::SignaturePublicKey<CURRENT_VERSION>,
        SignatureKeyPair: traits::SignatureKeyPair<CURRENT_VERSION>,
    >(
        &self,
        public_key: &SignaturePublicKey,
    ) -> Result<Option<SignatureKeyPair>, Self::Error> {
        self.read(SIGNATURE_KEY_PAIR, public_key)
    }

    fn encryption_key_pair<
        HpkeKeyPair: traits::HpkeKeyPair<CURRENT_VERSION>,
        EncryptionKey: traits::EncryptionKey<CURRENT_VERSION>,
    >(
        &self,
        public_key: &EncryptionKey,
    ) -> Result<Option<HpkeKeyPair>, Self::Error> {
        self.read(ENCRYPTION_KEY_PAIR, public_key)
    }

    fn encryption_epoch_key_pairs<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        EpochKey: traits::EpochKey<CURRENT_VERSION>,
        HpkeKeyPair: traits::HpkeKeyPair<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        epoch: &EpochKey,
        leaf_index: u32,
    ) -> Result<Vec<HpkeKeyPair>, Self::Error> {
        let key = EpochKeyPairsKey {
            group_id,
            epoch,
            leaf_index,
        };
        match self
            .kv
            .get(EPOCH_KEY_PAIRS, &serde_json::to_vec(&key)?)
            .map_err(Self::backend)?
        {
            Some(bytes) => Ok(serde_json::from_slice(&bytes)?),
            None => Ok(Vec::new()),
        }
    }

    fn key_package<
        KeyPackageRef: traits::HashReference<CURRENT_VERSION>,
        KeyPackage: traits::KeyPackage<CURRENT_VERSION>,
    >(
        &self,
        hash_ref: &KeyPackageRef,
    ) -> Result<Option<KeyPackage>, Self::Error> {
        self.read(KEY_PACKAGE, hash_ref)
    }

    fn psk<PskBundle: traits::PskBundle<CURRENT_VERSION>, PskId: traits::PskId<CURRENT_VERSION>>(
        &self,
        psk_id: &PskId,
    ) -> Result<Option<PskBundle>, Self::Error> {
        self.read(PSK, psk_id)
    }

    fn remove_proposal<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        proposal_ref: &ProposalRef,
    ) -> Result<(), Self::Error> {
        self.pull(PROPOSAL_QUEUE_REFS, group_id, proposal_ref)?;
        self.remove(
            QUEUED_PROPOSAL,
            &ProposalKey {
                group_id,
                proposal_ref,
            },
        )
    }

    fn delete_own_leaf_nodes<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.clear(OWN_LEAF_NODES, group_id)
    }

    fn delete_group_config<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.remove(JOIN_CONFIG, group_id)
    }

    fn delete_tree<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.remove(TREE, group_id)
    }

    fn delete_confirmation_tag<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.remove(CONFIRMATION_TAG, group_id)
    }

    fn delete_group_state<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.remove(GROUP_STATE, group_id)
    }

    fn delete_context<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.remove(GROUP_CONTEXT, group_id)
    }

    fn delete_interim_transcript_hash<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.remove(INTERIM_TRANSCRIPT_HASH, group_id)
    }

    fn delete_message_secrets<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.remove(MESSAGE_SECRETS, group_id)
    }

    fn delete_all_resumption_psk_secrets<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.remove(RESUMPTION_PSK_STORE, group_id)
    }

    fn delete_own_leaf_index<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.remove(OWN_LEAF_NODE_INDEX, group_id)
    }

    fn delete_group_epoch_secrets<GroupId: traits::GroupId<CURRENT_VERSION>>(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        self.remove(EPOCH_SECRETS, group_id)
    }

    fn clear_proposal_queue<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        ProposalRef: traits::ProposalRef<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
    ) -> Result<(), Self::Error> {
        let refs: Vec<ProposalRef> = self.read_list(PROPOSAL_QUEUE_REFS, group_id)?;
        for proposal_ref in &refs {
            self.remove(
                QUEUED_PROPOSAL,
                &ProposalKey {
                    group_id,
                    proposal_ref,
                },
            )?;
        }
        self.clear(PROPOSAL_QUEUE_REFS, group_id)
    }

    fn delete_signature_key_pair<
        SignaturePublicKey: traits::SignaturePublicKey<CURRENT_VERSION>,
    >(
        &self,
        public_key: &SignaturePublicKey,
    ) -> Result<(), Self::Error> {
        self.remove(SIGNATURE_KEY_PAIR, public_key)
    }

    fn delete_encryption_key_pair<EncryptionKey: traits::EncryptionKey<CURRENT_VERSION>>(
        &self,
        public_key: &EncryptionKey,
    ) -> Result<(), Self::Error> {
        self.remove(ENCRYPTION_KEY_PAIR, public_key)
    }

    fn delete_encryption_epoch_key_pairs<
        GroupId: traits::GroupId<CURRENT_VERSION>,
        EpochKey: traits::EpochKey<CURRENT_VERSION>,
    >(
        &self,
        group_id: &GroupId,
        epoch: &EpochKey,
        leaf_index: u32,
    ) -> Result<(), Self::Error> {
        let key = EpochKeyPairsKey {
            group_id,
            epoch,
            leaf_index,
        };
        self.kv
            .delete(EPOCH_KEY_PAIRS, &serde_json::to_vec(&key)?)
            .map_err(Self::backend)
    }

    fn delete_key_package<KeyPackageRef: traits::HashReference<CURRENT_VERSION>>(
        &self,
        hash_ref: &KeyPackageRef,
    ) -> Result<(), Self::Error> {
        self.remove(KEY_PACKAGE, hash_ref)
    }

    fn delete_psk<PskKey: traits::PskId<CURRENT_VERSION>>(
        &self,
        psk_id: &PskKey,
    ) -> Result<(), Self::Error> {
        self.remove(PSK, psk_id)
    }
}

/// The OpenMLS provider: Rust crypto and randomness, storage over an [`MlsKv`].
#[derive(Debug)]
pub struct Provider<K: MlsKv> {
    crypto: RustCrypto,
    storage: KvStorage<K>,
}

impl<K: MlsKv> Provider<K> {
    /// A provider over `kv`.
    pub fn new(kv: K) -> Self {
        Self {
            crypto: RustCrypto::default(),
            storage: KvStorage::new(kv),
        }
    }

    /// The key-value store underneath.
    pub fn kv(&self) -> &K {
        self.storage.kv()
    }
}

impl<K: MlsKv> OpenMlsProvider for Provider<K> {
    type CryptoProvider = RustCrypto;
    type RandProvider = RustCrypto;
    type StorageProvider = KvStorage<K>;

    fn storage(&self) -> &Self::StorageProvider {
        &self.storage
    }

    fn crypto(&self) -> &Self::CryptoProvider {
        &self.crypto
    }

    fn rand(&self) -> &Self::RandProvider {
        &self.crypto
    }
}
