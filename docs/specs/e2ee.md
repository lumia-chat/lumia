# End-to-End Encryption: Personal DMs, History Backup, Recovery

Status: accepted, 2026-11-01. This specification makes section 11 of [architecture.md](../architecture.md) implementable and retires its provisional 11.3. Wire shapes are in `proto/lumia/v1/e2ee.proto`, `events.proto` (`EncryptedPayload`) and `api.proto` (delivery service); endpoint mappings are in [protocol.md](../protocol.md). Where this document and the architecture disagree, the architecture wins and this document is wrong.

## 1. Model

```text
Conversation       one PERSONAL_DM or PERSONAL_GROUP_DM channel on one instance
Group              one MLS group (RFC 9420) per conversation; group id = channel id
Leaf               one per device of each participant; signed by the device key
Delivery service   the hosting instance: stores key packages and GroupInfo,
                   claims packages, orders commits by epoch, never a member,
                   never parses an MLS message
Operation          the plaintext of an application message (DmOperation)
Projection         the message list a client builds from operations, locally
History backup     chunks of operations under per-chunk keys, manifests under
                   the backup key, stored on the hosting instance, off by default
```

The server stores and sees, for a personal DM, exactly what architecture 11.1 lists: ids, device ids, seq, time, ciphertext size, and the one bit `EncryptedKind`. This document adds nothing to that list. Epoch numbers in `EncryptedPayload.epoch` are the only MLS fact the server reads, and it reads them as integers.

OpenMLS 0.9 is the implementation in the client core. Its provider is three traits: crypto, randomness, and a typed key-value storage of 72 methods. The core supplies storage (section 7); crypto and randomness come from `openmls_rust_crypto` on every platform, including web (section 8).

## 2. Ciphersuite and credentials

**Ciphersuite.** `MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519` (RFC 9420 suite 0x0001), pinned. A key package or group with any other suite is rejected. A second suite is a protocol version bump.

**Credential.** A basic credential whose identity is `identity_id (32 bytes) || device_id (16 bytes)`. The leaf's signature key is the device key from the roster (specs/identity.md section 3), so the roster certificate is what authenticates a leaf: there is no separate MLS credential authority.

**Leaf validation.** A client accepts a leaf (in a key package, a welcome, a commit, or GroupInfo) only when all hold:

1. The credential's `identity_id` is a participant of the conversation.
2. The `device_id` is in that identity's roster as the client holds it, not revoked.
3. The leaf's signature key equals that device's certificate public key.

A leaf that fails is rejected and the group is not advanced. A client that holds no roster for a participant fetches one (`GET /accounts/{account_id}/roster`, served across a shared `PERSONAL_*` channel) before judging; it never accepts a leaf on the credential alone. A rejected leaf makes the client fetch the participant's roster once more and judge again before it gives up, since the rejection may be a roster it has not seen yet.

**Extensions.** Groups carry the `ratchet_tree` extension in every GroupInfo so a joiner needs nothing else. Last-resort key packages carry the `last_resort` extension. No other extensions in v1.

## 3. Key packages

- Each device keeps **20** unclaimed key packages on every instance where its identity has an account, with a **90-day** lifetime. On connect, when the instance reports fewer than 10 (the `Welcome` frame carries the count), the device publishes enough to reach 20.
- Each device also publishes **one last-resort key package**, rotated on every top-up. The instance hands it out only when the pool is empty and never deletes it on claim. A client that joins someone through a last-resort package treats the resulting leaf as any other; the reduced forward secrecy of a reused init key is bounded by the next top-up.
- Web devices publish key packages like native ones; they are leaves.
- Claiming (`POST /mls/key-packages/claim`) needs the existing gate: the caller shares a `PERSONAL_*` channel with the target, holds an accepted DM request from them, or is the target (a device adding its identity's other devices; the claiming device is left out). One package per authorized device of the target is returned and deleted. An empty pool returns the last-resort package.
- The instance deletes expired packages and, on device revocation, every package of the revoked device.
- Private key material for published packages stays in the device's storage provider until the package is used or expires; the storage provider's `delete_key_package` runs on expiry.

## 4. Group lifecycle

**Creation.** On an accepted DM request the recipient's client creates the group (architecture 6.4): it claims one key package per device of the sender, adds its own other devices from their key packages, commits, and sends the welcome as a `CONTROL` event. A personal group DM is created by its creator the same way with every member's devices. The creator publishes GroupInfo (section 6) after the first commit.

**Commits.** Any member device may commit. `EncryptedPayload.epoch` on a `CONTROL` event is the epoch the commit **produces**; the instance accepts it only when it equals the conversation's current epoch plus one, answering `FAILED_PRECONDITION{state: "epoch"}` otherwise. A client whose commit is refused discards its pending commit, processes the events it missed, and tries again. A `CONTROL` event that is not a commit (a welcome, which carries the epoch its commit produced, or a proposal, which carries the current epoch) states the **current** epoch and advances nothing; the instance accepts exactly those two values. A committer therefore sends its commit first and the welcome after the ack. Application messages carry the epoch they were encrypted in and are accepted for any epoch.

**Welcomes** travel as `CONTROL` events in the conversation's log; a joining device learns of the conversation from its account log (`ChannelJoined` for the `PERSONAL_*` channel) and reads the welcome from the channel log. Welcomes are addressed to specific key packages, so only the intended device can open one.

**Adding a device.** After a device approves a link (identity spec 4.2) and publishes the new roster, it claims the new device's key packages on every instance and adds the leaf to every group it is a member of there, one commit per group. Until that commit lands, the new device sees the conversation but cannot read it and says so.

**Revoking a device.** Every member that learns of a revocation from the roster (its own identity's, or a participant's through `RosterUpdated` or a fresh roster fetch) proposes and commits the leaf's removal; the first commit wins by epoch and the others become no-ops on retry. From the moment a client holds the revoking roster it refuses application messages from that leaf, and commits authored by it, regardless of whether the removal has landed.

**Members.** Adding a person to a group DM adds every device in their roster. Removing a person removes every leaf of theirs. A member leaves by a self-remove proposal that another member commits; if no other member remains, the conversation is closed and the instance purges it on the retention schedule.

**Device recovery** is an external commit (section 10). Other members see a `device recovered` marker and their clients re-verify the identity's root key against the pinned one before accepting further messages.

## 5. Messages and the projection

Every application message is a `DmOperation`. The sender encrypts it with `create_message`, persists the advanced MLS state and the outbox entry in **one** store transaction, then sends `SendEncrypted` with the ciphertext, `APPLICATION`, and the epoch. The command id is the `operation_id`. A retry resends identical bytes (delivery spec 3); if the epoch moved before the ack, the sender re-encrypts under a **new** command id and operation id and drops the old outbox entry.

**Ordering.** Operations apply in `seq` order of the conversation's log, across every sender; the local projection is a deterministic function of the operations in seq order. Senders put `sent_at` for display, not for ordering.

**Projection rules.**

| Operation | Effect |
|---|---|
| `send` | A message with the given id, body, parent, attachments, mentions, by the sender identity, at the event's seq. A second `send` with an existing id is ignored. |
| `edit` | Replaces the body; only the author identity may edit, from any device. |
| `delete` | Tombstones; only the author. |
| `react` | Adds or removes one reaction by the sender identity. |
| `pin` | Any member; the projection keeps the pin set. |
| `read` | The sender identity has read through `through_seq`; the newest wins. |
| `typing` | Shown until `until`, never persisted. |

**Undecryptable messages.** An application message for an epoch the client has not reached is held until the commits before it are processed. One for an epoch older than the client's join is shown as `sent before this device joined` and never decrypted. A message that fails to decrypt in the right epoch is shown as `could not be decrypted` with the sender, and the client re-fetches GroupInfo and checks its state; after two such failures in one epoch it rejoins by external commit (section 10).

**Push.** The instance pushes `APPLICATION` events with generic text (architecture 15.5); the device decrypts in its notification extension to render the preview.

## 6. GroupInfo

After every commit the committer publishes GroupInfo with the ratchet tree extension to `POST /mls/group-info` for the conversation. The instance keeps the newest per conversation, keyed by the epoch it carries, and refuses an older one. A member may read it at any time; it is needed for external commits (section 10) and for a client that has lost sync. GroupInfo is signed by the committer's leaf, so a reader validates it as any leaf.

## 7. Storage provider

The core implements `openmls_traits::storage::StorageProvider` over the client's SQLite store:

```text
mls_kv    (kind TEXT, key BLOB, value BLOB, PRIMARY KEY (kind, key))
mls_list  (kind TEXT, key BLOB, position INTEGER, value BLOB, PRIMARY KEY (kind, key, position))
```

Single-valued entities (group context, tree, epoch secrets, key pairs, PSKs) are rows in `mls_kv`; the appended kinds (own leaf nodes, queued proposals, retained key-package material) are rows in `mls_list`. Keys and values are the provider traits' serde forms. The provider runs inside the store's current transaction, so **MLS state, the outbox, the cursor, and the projection commit together** (architecture 11.2). A crash never leaves a leaf that advanced an epoch it cannot resume. The same schema serves native SQLite and SQLite WASM.

Private keys at rest on native devices are in the SQLite store under the platform's file protection; the shells may additionally wrap the database key in the platform secret store, which is their decision. On web see section 8.

## 8. Web devices

A web device is a full MLS leaf. WebCrypto has no HPKE and OpenMLS needs private key bytes for every commit and welcome, so a WebCrypto-backed crypto provider is not possible; the proof obligation in architecture 20 resolved this way on 2026-11-01.

- **While the page runs**, MLS private keys live in WASM linear memory, as every other secret the core handles there.
- **At rest**, the OPFS SQLite store that holds them is encrypted under a non-extractable AES-GCM WebCrypto key generated per origin and kept in IndexedDB. Another origin cannot read the store, and a copy of the OPFS files is useless without the browser profile.
- **The trust boundary is the origin operator.** The page's own code can use the keys and read plaintext. The device list labels web devices with their origin; a self-hosted instance's web origin connects to that instance only (architecture 11.4). Nothing below the origin is claimed.

A web device never holds the backup key and cannot restore history (identity spec 7).

## 9. History backup

**Toggle.** Off by default, per identity, stored in the identity backup. Where it is turned on the product says: an archive keeps what MLS would otherwise forget, so retained history is as confidential as the recovery phrase and not more.

**Chunks.** For each conversation, each native device writes a chunk when it holds 500 operations or 1 MB not yet written, or when it goes idle with at least one unwritten operation. A chunk is `HistoryChunkBody` for a contiguous seq range, encrypted with XChaCha20-Poly1305 under a fresh 256-bit chunk key, uploaded with `PUT /history/chunks/{chunk_id}` to the instance hosting the conversation.

**Manifest.** Each device keeps one manifest per instance (`HistoryManifestBody`): every chunk it wrote there with its range and the chunk key wrapped under the backup key. The manifest is signed by the device key and has a monotonic version; `PUT /history/manifest` ignores a lower version. Devices write independent chunks and manifests; they never coordinate.

**Restore** takes every device's manifest from `GET /history/manifests` on each host the identity backup lists, verifies each signature against the roster (a manifest signed by a device revoked **before** the manifest's `written_at` is ignored), unwraps the chunk keys, fetches the chunks, and replays the operations in seq order, deduplicating by `operation_id`. It restores a conversation only for ranges within the identity's membership as the chunk writer saw it.

**Deletion.** When a user deletes a message, the device that next writes a chunk covering that range writes it without the message and bumps its manifest; chunks are immutable, so the old chunk is deleted and rewritten. When the user turns backup off, every device deletes its chunks and manifest on every instance. On account deletion the instance purges the identity's chunks and manifests with everything else.

**Rotation.** On recovery-phrase change (identity spec 4.5) the device re-wraps every chunk key under the new backup key and writes new manifests; chunks are not touched.

**Quota.** Per identity per instance, from the instance's plan; `GET /history/usage` reports it and `PUT` answers `RESOURCE_EXHAUSTED` over it.

## 10. Recovery and rejoin

With no device left, recovery follows identity spec 4.3: a fresh device key, a root-signed roster, the identity backup from any host. For every `PERSONAL_*` channel the account log then lists, on every host:

1. `GET /mls/group-info/{channel_id}`; validate the signing leaf (section 2).
2. Build an external commit joining the group as a new leaf, with proposals removing every leaf of the recovering identity's revoked devices.
3. Send it as a `CONTROL` event with the new epoch. On `FAILED_PRECONDITION{state: "epoch"}`, refetch GroupInfo and repeat.
4. Publish the new GroupInfo (section 6).

Other members' clients see the external commit as a `device recovered` marker and re-verify the identity's root against their pinned copy, since a root rotation can accompany recovery.

History, if backup was on, is then restored (section 9). Without backup, a recovered device has the conversation from the rejoin forward. A device added by link (identity spec 4.2) is the same: it is added by an existing device (section 4), and restores from backup if on; the link bundle carries no history.

## 11. Delivery-service duties

The instance, in Elixir with no MLS library:

- Stores key packages per device as opaque bytes with their expiry, and one last-resort package; claims and deletes per section 3.
- Keeps the newest GroupInfo per conversation (section 6).
- Reads `EncryptedPayload.epoch` on `CONTROL` events: current plus one advances the conversation's epoch, current is accepted as a non-advancing control message, anything else is refused.
- Stores and serves history chunks and manifests as opaque bytes with quotas.
- Purges a revoked device's key packages, and a deleted account's packages, chunks, manifests, and ciphertext.

Nothing here decodes TLS presentation format. If a duty ever needs to, it is a change to architecture 11.1 first.

## 12. Tests

Failure-injection tests that must pass before this area is considered implemented:

```text
Two devices of A and one of B; B accepts a DM request; verify B creates the
  group, both of A's devices receive the welcome, and a message from either
  of A's devices reaches B and A's other device.
A links a third device; verify the approving device adds its leaf to every
  group within one commit per group, and the new device reads messages sent
  after that commit and marks earlier ones as sent before it joined.
A revokes a device; verify every member's client refuses that leaf's
  messages immediately, exactly one removal commit lands, and the others
  resolve as no-ops on epoch retry.
Two members commit concurrently; verify the instance accepts one, answers
  FAILED_PRECONDITION{state: "epoch"} to the other, and the loser retries
  onto the new epoch with no duplicated proposal.
Send an application message, kill the client between MLS state persist and
  the network send; restart; verify the outbox resends the identical bytes
  and the receiver decrypts exactly one message.
Epoch moves before an unacked send is retried; verify the sender re-encrypts
  under a new command id and the receiver sees one message.
Present a key package whose device is not in the identity's roster; verify
  the adder rejects it and the group does not advance.
Exhaust a device's key packages; verify the last-resort package is served,
  not deleted, and replaced on the next top-up.
Recovery with every device gone; verify the external commit joins, revoked
  leaves are removed in the same commit, members see the marker, and history
  restores from two devices' manifests with no duplicate operation.
A manifest signed by a device revoked before its written_at; verify restore
  ignores it.
Delete a message with backup on; verify the covering chunk is rewritten
  without it and the old chunk is gone from the instance.
Turn backup off; verify every chunk and manifest of the identity is deleted
  on every instance.
Web device: reload the page; verify the group state is read back from the
  encrypted OPFS store and a message sent before the reload still decrypts.
```

## 13. Open items

- Whether `DmTyping` should travel at all in group DMs with many devices, where every typing indicator is a full MLS application message.
- The client-side cap on group DM size, bounded by key-package claims per add and commit size with the ratchet tree extension.
- Whether the push relay's generic text should distinguish mentions, which would require the mention bit outside the ciphertext.
