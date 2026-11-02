# Lumia Protocol Contract

Status: draft, 2026-11-01. Companion to [architecture.md](architecture.md), which owns the *why*. This document owns the *exactly how* of everything that crosses the wire. The schemas in `proto/lumia/v1/` are the source of truth for shapes; this document is the source of truth for everything the schemas cannot say: transport, ordering, auth, errors, pagination, compatibility, and the JSON mapping.

When this document and the architecture disagree, the architecture wins and this document is wrong.

## 1. Files

| File | Holds |
|---|---|
| `common.proto` | `Error`, `ErrorCode`, pagination, `Cursor` |
| `identity.proto` | Roster with device certificates, revocations and rotations; identity challenge and proof; signed profile; link bundles; identity backup |
| `chat.proto` | Materialized shapes: `Server`, `Channel`, `Member`, `Role`, `Message`, `Attachment`, presence, permissions |
| `events.proto` | `Event` (channel log) and `AccountEvent` (account log), including `EncryptedPayload` |
| `commands.proto` | `Command` and `Ack` |
| `gateway.proto` | `ClientFrame`, `ServerFrame`, and every frame body |
| `api.proto` | HTTP request and response bodies not covered above |
| `e2ee.proto` | What lives inside an MLS application message (`DmOperation`), and the history-backup chunk, manifest and request shapes (specs/e2ee.md) |

Package `lumia.v1`. Generated code is produced for Elixir (server), Rust (client core), Swift, Kotlin, and TypeScript from these files and nothing else. No client or server hand-writes a wire type.

## 2. Encodings

Two encodings, one schema.

**Protobuf binary** is used by first-party clients on the gateway. Frames are whole serialized `ClientFrame` or `ServerFrame` messages, one per WebSocket binary message. HTTP bodies may be protobuf with `Content-Type: application/x-protobuf` when the client sends `Accept: application/x-protobuf`.

**JSON** follows ProtoJSON with four Lumia rules that override it:

1. **UUIDs are strings.** Every `bytes` field whose name ends in `_id`, plus `content_address`, `nonce`, and `identity_id`, is a canonical lowercase UUID string (or lowercase hex for the non-UUID ones: `content_address`, `nonce`), never base64.
2. **64-bit integers are decimal strings.** Every `uint64` and `int64`, which includes every `seq`, `version`, `epoch`, and `size_bytes`. JavaScript cannot hold them otherwise.
3. **Enums are their symbolic names** (`"PERSONAL_DM"`), never integers. An unknown name is treated as the `_UNSPECIFIED` value.
4. **Other `bytes` fields are base64** per ProtoJSON: signatures, public keys, `mls_message`, `key_packages`, `group_info`, `challenge`, `body` on signed structures.

Field names in JSON are `lowerCamelCase` as ProtoJSON specifies. Timestamps are RFC 3339 with `Z`. Absent and default values are omitted.

## 3. Compatibility

- Field numbers are never reused, renumbered, or retyped. Fields are only added.
- A reader ignores unknown fields and preserves them when re-serializing a message it did not originate (a bot relaying an event, for example).
- A reader treats an unknown enum value as `_UNSPECIFIED` and never rejects a message for it.
- `oneof` payloads may gain cases. A reader that receives a case it does not know treats the message as present but opaque: an `Event` with an unknown payload still advances `seq` and must be stored and acknowledged, never dropped.
- `/api/v1` is bumped only for a change that cannot be expressed as an addition. The goal is that it is never bumped.
- Servers advertise `software_version` in `InstanceInfo`. Clients never branch on it; they branch on the presence of fields and the capability booleans in `Welcome` and `InstanceInfo`.

## 4. Identifiers and time

- **Message IDs** are UUIDv7 minted by the client. The message ID is also the `command_id` of the `SendMessage` that created it.
- **Command IDs** are UUIDv7 minted by the client, one per command. Edits, reactions, pins, and encrypted sends each get their own.
- **Everything else** the server mints is UUIDv7 with monotonic sub-millisecond precision, strictly increasing per node.
- **No code derives a time from an ID.** `created_at` is always carried explicitly. Client clocks are untrusted; the server stamps `created_at` on commit.

## 5. Sequence numbers

Every log has a `seq`: a per-log `uint64` assigned by the server inside the transaction that appends the event, dense, starting at 1. There are two kinds of log:

- **Channel logs**, one per `Channel` of any kind, carrying `Event`.
- **The account log**, one per account per instance, carrying `AccountEvent`: memberships, channels gained and lost, requests, friendships, roster changes, profile changes, session revocations, block-list updates.

Rules every client must implement:

- Apply events in `seq` order per log. Never apply N+1 before N.
- On receiving `seq` N+k with k > 1 while holding N, fetch N+1 through N+k-1 with `GET /channels/{id}/events?after_seq=N&before_seq=N+k` before applying N+k.
- Deduplicate by `event_id`. A fetched event already applied is dropped.
- Persist the applied `seq` with the applied state in the same local transaction.
- Every event carries the channel's `acl_version` at commit. Clients use it only for cache coherence of channel permissions; it has no ordering meaning.

Rules the server implements, stated so clients can rely on them:

- `seq` assignment, the event insert, and any materialized update commit in one Postgres transaction with the channel row locked throughout. Broadcast happens strictly after commit.
- An `Ack` for a command is sent only after that commit. An acked event is therefore queryable by `GET /channels/{id}/events` immediately.
- Live delivery is at-least-once. The same event may arrive twice (fast path plus a gap fill). Clients deduplicate.

## 6. Gateway

### 6.1 Connection

```
GET /gateway HTTP/1.1
Upgrade: websocket
Sec-WebSocket-Protocol: lumia.v1.proto      (or lumia.v1.json)
Authorization: Bearer <access_token>
```

**As implemented in v1**, encoding is chosen by the opcode of the first frame rather than by subprotocol: a binary frame selects protobuf, a text frame selects JSON, and the connection keeps that encoding. The access token travels as `?access_token=` or, for browsers, whose WebSocket API cannot set `Authorization`, as the subprotocol `base64url.bearer.phx.<base64url token>` in `Sec-WebSocket-Protocol`, which the server consumes before negotiating the rest of the header. The `Authorization` header is not read on the upgrade: the socket layer cannot see it and the one client that would send it, a native one, has the query form. The server strips the token from logs and URLs.

One connection per (device, instance). A second connection with the same session closes the first with `DISCONNECT_PROTOCOL_ERROR`.

### 6.2 Handshake

1. Client sends `Hello` within 5 seconds of the upgrade, or is disconnected. (Server-side, `Hello` is the join of the single Phoenix channel per socket; no join framing appears on the wire.)
2. Server sends `Welcome`. Anything else before `Welcome` is a protocol error.
3. If `Welcome.recovery_generation` differs from what the client stored for this instance, the client **discards every cursor** it holds for this instance, stores the new generation, and proceeds with the snapshot path for everything.
4. If `Welcome.roster_update_available` is set, the client fetches `GET /identity/roster` before doing anything that depends on its own device list.

### 6.3 Subscribing and resuming

`Subscribe` carries a `Cursor` per channel: the channel ID and the highest `seq` the client has applied, or 0. The server answers with one `ChannelSubscribeResult` per channel:

| `status` | Meaning | Client action |
|---|---|---|
| `RESUMED` | Subscribed. Events from `cursor+1` to `last_seq` will arrive as `Event` frames in order, then live events. | Apply as they arrive. |
| `SNAPSHOT_REQUIRED` | Subscribed, but the cursor is more than 1,000 events behind the head, so the replay is not streamed (specs/delivery.md 6). | `GET /channels/{id}/snapshot`, apply it, set the cursor to `snapshot.seq`, then `GET /channels/{id}/events?after_seq=snapshot.seq` until `last_seq`. Live events that arrived meanwhile are applied after by `seq`. |
| `ERROR` | Not subscribed. | Inspect `error`. `PERMISSION_DENIED` and `NOT_FOUND` mean the client should drop the channel locally. |

A subscription replay may be large. The server streams it as ordinary `Event` frames interleaved with other traffic; the client orders by `(channel_id, seq)` and nothing else.

`ResumeAccount` does the same for the account log, with `AccountResumeResult`. A client always resumes the account log first, because it is how the client learns which channels exist to subscribe to.

### 6.4 Nudges and high-water marks

A `Nudge` means the server believes the client may have missed events on the listed channels. It carries the current head `seq` and nothing else. The client compares against its cursor and fetches any gap. Nudges are produced by the server's reconciliation pass (architecture 8.2) and may be spurious; a client must treat a nudge for a channel it is already current on as a no-op.

A client sends `HighWaterMarkRequest` when it comes to the foreground or regains connectivity, listing every channel it cares about (or all subscribed when empty). The response is one `Cursor` per channel plus the account log head.

### 6.5 Commands and acks

A `Command` frame produces exactly one of `Ack` or `Error` carrying the same `command_id`. Commands on different channels may be acknowledged out of order; commands on one channel are acknowledged in the order sent. The `Event` the command produced is delivered separately through the subscription and may arrive before or after the `Ack`; clients reconcile by `command_id` on the event.

A client that disconnects before receiving an `Ack` **retries the identical command** after reconnecting. The server's idempotency rules (section 8) make this safe.

### 6.6 Heartbeats

The client sends `Heartbeat` every `heartbeat_interval_seconds`; the server echoes the nonce. The server disconnects after two missed heartbeats with `DISCONNECT_HEARTBEAT_TIMEOUT`. The client treats two unanswered heartbeats as a dead connection and reconnects.

### 6.7 Presence and typing

`PresenceUpdate` sets the client's own status and **replaces** the set of servers whose presence the client wants to receive. Clients include a server only while its member list or a channel in it is on screen. `PresenceDiff` frames arrive scoped to one server, or to the friends topic when `server_id` is empty.

`Typing` is sent at most once per 5 seconds while the user is typing in a channel. Servers fan it out as `TypingIndicator` with an `until` about 8 seconds ahead; clients stop showing it at `until` without needing a stop message. Typing is never sent for `PERSONAL_*` channels over the gateway in plaintext; clients that want typing in E2EE DMs send it inside an MLS application message.

### 6.8 Disconnect

The server sends `Disconnect` before closing. `DISCONNECT_SERVER_SHUTDOWN` and `DISCONNECT_SLOW_CONSUMER` mean reconnect with the same cursors after `reconnect_after_seconds`. `DISCONNECT_SESSION_REVOKED` and `DISCONNECT_DEVICE_REVOKED` mean the client must not reconnect with that token or device key, and must wipe the local store for that instance (architecture 12).

## 7. Account log and snapshots

A client's first connection to an instance, and any connection after a generation change, follows this path:

1. `GET /account/snapshot` → `AccountSnapshot` with `seq`. Apply it: servers, memberships, channels (the servers' and the personal DMs the account is in, with their participants), pending requests, friendships, roster, block list.
2. `ResumeAccount{after_seq: snapshot.seq}` on the gateway.
3. `Subscribe` with cursor 0 for every channel in the snapshot, which yields `SNAPSHOT_REQUIRED` for each; take the channel snapshot path for the ones the user opens first and lazily for the rest.

Thereafter, the account log tells the client about every channel it gains (`ChannelAdded`, subscribe to it) or loses (`ChannelRemoved`, unsubscribe and drop).

## 8. Idempotency

The server keeps every `command_id` it has accepted for at least the retention window of its channel. For a command whose ID has been seen:

| Condition | Response |
|---|---|
| Same actor, same channel, same operation, same payload | The original `Ack`, unchanged. No new event. |
| Same ID, any difference in actor, channel, operation, or payload | `CONFLICT`. |
| Same ID on a channel or message the actor cannot access | `NOT_FOUND`, exactly as for an unknown ID. |

"Same payload" is byte equality of the serialized `op` after normalizing field order, which generated code guarantees for a re-serialized identical message. For `SendEncrypted` it is byte equality of `mls_message`; a client that must re-encrypt because the epoch moved sends a **new** `command_id`.

Over HTTP, the same rules apply to `POST /channels/{id}/commands`.

## 9. HTTP mapping

Base path `/api/v1`. All endpoints require `Authorization: Bearer <access_token>` unless marked public. Request and response bodies are the named messages. Path parameters are UUID strings.

### Auth and identity

| Method and path | Body → Response | Notes |
|---|---|---|
| `GET /instance` | → `InstanceInfo` | Public. The trust card. |
| `POST /auth/challenge` | `ChallengeRequest` → `ChallengeResponse` | Public. Challenge expires in 2 minutes, single use. |
| `POST /auth/sign-in/password` | `PasswordSignInRequest` → `TokenPair` | Public. MFA: first call returns `FAILED_PRECONDITION` with `details.mfa`; a wrong `totp_code` is `UNAUTHENTICATED`. |
| `POST /auth/totp/enroll` | → `TotpEnrollResponse` | A pending TOTP secret for the caller; replaces an unconfirmed one. |
| `POST /auth/totp/confirm` | `TotpConfirmRequest` → 204 | The first valid code enables TOTP; sign-in and reauth then require it. |
| `POST /auth/sign-in/identity` | `IdentityProofSignInRequest` → `TokenPair` | Public. `POLICY_DENIED` when the instance disables it. |
| `POST /auth/sign-in/magic-link` | `{email}` → 202 | Public. Link completes at `POST /auth/sign-in/magic-link/complete`. |
| `GET /auth/oidc/start`, `/auth/oidc/callback` | | Public. Standard code flow. |
| `POST /auth/passkey/*` | | WebAuthn registration and assertion ceremonies. |
| `POST /accounts` | `CreateAccountRequest` → `TokenPair` | Public. `POLICY_DENIED` if registration or identity-proof creation is off. |
| `POST /auth/refresh` | `RefreshRequest` → `TokenPair` | Rotates the refresh token. The old one is invalid after use. |
| `POST /auth/sign-out` | → 204 | Revokes the current session and closes its gateway connection. |
| `GET /sessions` | → `ListSessionsResponse` | |
| `DELETE /sessions/{session_id}` | → 204 | Revokes and disconnects. |
| `POST /auth/reauth` | `ReauthRequest` → `ReauthResponse` | Five-minute token for revoke and link. |
| `GET /identity/roster` | → `GetRosterResponse` | The newest roster this instance holds for the caller's identity. |
| `GET /accounts/{account_id}/roster` | → `GetRosterResponse` | Another account's roster, for validating its MLS leaves; `NOT_FOUND` unless the caller shares a `PERSONAL_*` channel with it. |
| `POST /identity/roster` | `PublishRosterRequest` → 204 | Verified per specs/identity.md 3.3. `CONFLICT` carries the held roster in `details.held`. Rosters that add revocations are refused here with `POLICY_DENIED`. |
| `POST /identity/devices/revoke` | `RevokeDeviceRequest` → 204 | The only non-root path that may add revocations. Requires `reauth_token`. |
| `POST /identity/link` | `IdentityProof` + `reauth_token` → 204 | Binds the current account to the proven identity. |
| `POST /identity/link-bundles` | `CreateLinkBundleRequest` → `CreateLinkBundleResponse` | Existing device uploads the bundle for a device being added. |
| `GET /identity/link-bundles/{code}?device_id=` | → `GetLinkBundleResponse` | Public. Single use, ten minutes, five attempts. |
| `PUT /identity/backup` | `PutIdentityBackupRequest` → 204 | Highest version wins. |
| `GET /identity/backup` | → `GetIdentityBackupResponse` | |

### Servers, channels, members, roles

| Method and path | Body → Response |
|---|---|
| `GET /servers` | → `ListServersResponse` |
| `POST /servers` | `CreateServerRequest` → `Server` |
| `GET /servers/{server_id}` | → `Server` |
| `PATCH /servers/{server_id}` | `Server` (partial) → `Server` |
| `GET /servers/{server_id}/channels` | → `ListChannelsResponse` |
| `POST /channels` | `CreateChannelRequest` → `Channel` |
| `PATCH /channels/{channel_id}` | `Channel` (partial) → `Channel` |
| `DELETE /channels/{channel_id}` | → 204 |
| `POST /dms/personal` | `OpenPersonalDmRequest` → `OpenPersonalDmResponse` (200 existing, 201 new); `FAILED_PRECONDITION{state: "request"}` without an accepted DM request with each participant; three or more participants open a new `PERSONAL_GROUP_DM` |
| `GET /servers/{server_id}/members` | `ListMembersRequest` as query → `ListMembersResponse` |
| `PATCH /servers/{server_id}/members/{account_id}` | `UpdateMemberRequest` → `Member` |
| `DELETE /servers/{server_id}/members/{account_id}` | → 204 |
| `GET /servers/{server_id}/roles` | → `ListRolesResponse` |
| `PUT /servers/{server_id}/roles/{role_id}` | `UpsertRoleRequest` → `Role` |
| `DELETE /servers/{server_id}/roles/{role_id}` | → 204 |

Permission changes, overwrite changes, and membership changes each bump the affected channels' `acl_version` inside the same transaction and emit `AclChanged` events.

### Events, messages, commands

| Method and path | Body → Response |
|---|---|
| `GET /channels/{channel_id}/events` | `ListEventsRequest` as query → `ListEventsResponse` |
| `GET /channels/{channel_id}/snapshot` | → `ChannelSnapshot` |
| `GET /channels/{channel_id}/messages` | `ListMessagesRequest` as query → `ListMessagesResponse` |
| `GET /channels/{channel_id}/messages/{message_id}` | → `Message` |
| `GET /channels/{channel_id}/messages/{message_id}/history` | → `ListMessagesResponse` (edit history; `MANAGE_MESSAGES`) |
| `GET /channels/{channel_id}/pins` | → `ListMessagesResponse` |
| `POST /channels/{channel_id}/commands` | `Command` → `Ack` |
| `GET /account/snapshot` | → `AccountSnapshot` |
| `GET /account/events` | `ListAccountEventsRequest` as query → `ListAccountEventsResponse` |

`ListEventsResponse.truncated_before` set means `after_seq` is below the oldest served event; the client must take the snapshot path.

### Uploads

| Method and path | Body → Response |
|---|---|
| `POST /uploads` | `CreateUploadRequest` → `CreateUploadResponse` |
| `PUT <upload_url>` | raw bytes → 204 |

The upload URL is single-use and expires. The attachment ID is then referenced from `SendMessage.attachment_ids`. Server content is processed (thumbnails, EXIF) after the PUT; `client_encrypted` uploads are stored as-is.

### Search, friends, requests, blocks

| Method and path | Body → Response |
|---|---|
| `GET /search` | `SearchRequest` as query → `SearchResponse` |
| `GET /friends` | → `ListFriendsResponse` |
| `POST /friends/requests` | `FriendRequestRequest` → 204 |
| `POST /friends/requests/{account_id}/accept` | → 204 |
| `DELETE /friends/{account_id}` | → 204 |
| `POST /dm-requests` | `SendDmRequestRequest` → `DmRequest` (201); `ALREADY_EXISTS` when the pair already has a personal DM; `NOT_FOUND` for a sender the recipient blocked |
| `POST /dm-requests/{request_id}/respond` | `RespondDmRequestRequest` → `DmRequest`; only the recipient; accepting opens the pair's personal DM and puts `DmRequestUpdated` then `ChannelAdded` on both account logs |
| `PUT /blocks` | `UpdateBlockListRequest` → 204 |

Search results are filtered by the caller's channel permissions at query time.

### MLS delivery service

| Method and path | Body → Response |
|---|---|
| `POST /mls/key-packages` | `PublishKeyPackagesRequest` → 204 |
| `POST /mls/key-packages/claim` | `ClaimKeyPackageRequest` → `ClaimKeyPackageResponse` |
| `POST /mls/group-info` | `PublishGroupInfoRequest` → 204 |
| `GET /mls/group-info/{channel_id}` | → `GetGroupInfoResponse` |

A claimed key package is deleted on claim. Claiming requires that the caller be in a `PERSONAL_*` channel with the target, or hold an accepted DM request from them. An empty pool serves the device's last-resort package, which is never deleted on claim. `POST /mls/group-info` keeps the newest GroupInfo per conversation by the `epoch` the request states and ignores an older one. `Welcome.key_package_count` tells a device how many unclaimed packages it has. Cadence and counts are in specs/e2ee.md section 3.

### History backup

| Method and path | Body → Response |
|---|---|
| `PUT /history/chunks/{chunk_id}` | `PutHistoryChunkRequest` → 204; `RESOURCE_EXHAUSTED` over quota |
| `GET /history/chunks/{chunk_id}` | → `GetHistoryChunkResponse` |
| `DELETE /history/chunks/{chunk_id}` | → 204 |
| `PUT /history/manifest` | `PutHistoryManifestRequest` → 204; a lower version than held for the device is ignored |
| `GET /history/manifests` | → `ListHistoryManifestsResponse`, every device's newest manifest for the caller's identity |
| `DELETE /history` | → 204; every chunk and manifest of the caller's identity on this instance |
| `GET /history/usage` | → `HistoryUsageResponse` |

Chunks and manifests are opaque to the instance; the caller must be bound to the identity the manifest names (specs/e2ee.md section 9).

### Invites

| Method and path | Body → Response |
|---|---|
| `GET /invites/{code}` | → `InvitePreview` (public) |
| `POST /invites` | `CreateInviteRequest` → `Invite` |
| `POST /invites/{code}/accept` | → `Member` |
| `DELETE /invites/{code}` | → 204 |

### MCP

`/mcp` speaks Model Context Protocol over Streamable HTTP with the same bearer tokens. Its tools and resources are defined in a separate document; each tool is a thin mapping onto one of the endpoints above and carries the same permissions.

## 10. Errors

Every non-2xx response body is an `Error`. HTTP status is derived from `code`:

| `ErrorCode` | HTTP |
|---|---|
| `INVALID_ARGUMENT`, `FAILED_PRECONDITION`, `CURSOR_EXPIRED`, `GENERATION_CHANGED`, `UNSUPPORTED_ENCODING` | 400 |
| `UNAUTHENTICATED`, `TOKEN_EXPIRED`, `SESSION_REVOKED`, `DEVICE_REVOKED`, `IDENTITY_PROOF_INVALID` | 401 |
| `PERMISSION_DENIED`, `POLICY_DENIED` | 403 |
| `NOT_FOUND` | 404 |
| `CONFLICT`, `ALREADY_EXISTS` | 409 |
| `PAYLOAD_TOO_LARGE` | 413 |
| `RATE_LIMITED` | 429 |
| `INTERNAL` | 500 |
| `UNAVAILABLE` | 503 |
| `RESOURCE_EXHAUSTED` | 507 |

`details` keys by code: `INVALID_ARGUMENT.field`; `FAILED_PRECONDITION.mfa`, `.state`; `ALREADY_EXISTS.existing_id` (only when the caller may see it); `PAYLOAD_TOO_LARGE.max`; `RATE_LIMITED.scope`; `RESOURCE_EXHAUSTED.quota` (bytes). `NOT_FOUND` is returned both for things that do not exist and for things the caller may not know exist. Clients never distinguish the two.

## 11. Pagination

List endpoints take `page_size` (1 to 200, default 50) and an opaque `cursor`, and return `next_cursor`, empty on the last page. Cursors are valid for at least one hour and encode nothing a client should parse. Event and message listing uses `seq` ranges instead, because those are the natural cursor and clients already hold them.

## 12. Authentication tokens

- **Access token**: opaque, 15 minutes. Sent as a bearer header. Validated against the session table on every request; a revoked session fails immediately with `SESSION_REVOKED`.
- **Refresh token**: opaque, 30 days, single use. `POST /auth/refresh` returns a new pair and invalidates the old refresh token. Reuse of an old refresh token revokes the whole session.
- **Bot tokens**: long-lived, scoped, sent as bearer. Scopes are permission names from `chat.proto`; a bot's effective permissions are the intersection of its scopes and its roles.
- **OAuth 2.1** with PKCE for user-authorized integrations, standard endpoints under `/oauth/`.
- A session is bound to a `device_id`. If the device is revoked in the roster the instance holds, every call fails with `DEVICE_REVOKED` and the gateway disconnects.

## 13. Identity proof

```
1. Client: POST /auth/challenge {purpose}
2. Server: ChallengeResponse{challenge: serialize(IdentityChallengeBody{nonce, host, expires_at, purpose, mac})}
3. Client: verify body.host == the host it is talking to; refuse otherwise
4. Client: sign the exact challenge bytes with the device key
5. Client: IdentityProof{challenge, identity_id, device_id, signature, roster}
6. Server: deserialize challenge; verify mac, not expired, host == self, purpose matches
          verify the roster per specs/identity.md 3.3 (certificates, chain, monotonicity); store if newer
          verify device_id is in roster.devices
          verify signature over challenge bytes with that device's public key
          insert nonce into used_challenges; a duplicate is a replay
```

A proof is accepted exactly once. The full roster rules, the add and revoke flows, link bundles, and recovery are in [specs/identity.md](specs/identity.md). A challenge for `CREATE_ACCOUNT` is refused when the instance has disabled identity-proof creation; the client learns that in advance from `InstanceInfo` and shows the trust card before requesting any challenge (architecture 6.1).

## 14. Message body

`Message.body` is UTF-8 text in a Markdown subset: paragraphs, `*emphasis*`, `**strong**`, `~~strike~~`, inline `` `code` ``, fenced code blocks with an optional language, `> quotes`, ordered and unordered lists, and links. No raw HTML, no images (attachments carry images), no headings, no tables. Mentions are not in the text; they are `Mention` entries with byte offsets into the body, so a display name change never rewrites history. Servers validate that offsets fall on UTF-8 boundaries.

## 15. Encrypted channels

For `PERSONAL_DM` and `PERSONAL_GROUP_DM`:

- The only commands are `SendEncrypted`. Any other `op` is `INVALID_ARGUMENT`.
- The only event payloads are `EncryptedPayload` and `Redacted` (when a participant deletes their account and the ciphertext is purged).
- `EncryptedPayload.kind` is the single bit the server reads: `APPLICATION` events are eligible for push; `CONTROL` events are not. Everything else about the message is inside `mls_message`: for `APPLICATION` it is a `DmOperation` (`e2ee.proto`), for `CONTROL` an MLS commit, proposal, welcome or external commit.
- `epoch` on a `CONTROL` event is the epoch the commit produces; the delivery service accepts it when it is the conversation's current epoch plus one (a commit, which advances the epoch) or the current epoch (a welcome or proposal, which advances nothing) and answers `FAILED_PRECONDITION{state: "epoch"}` otherwise, which is how concurrent commits are serialized. Application messages carry the epoch they were encrypted in, are accepted for any epoch, and are ordered by `seq`.
- Snapshots return `encrypted_events` rather than `messages`; there are no server-side messages to return.
- `ThreadSummary`, `ReactionCount`, `pinned`, and `Mention` are never populated by the server for these channels. Clients compute them from decrypted content.

## 16. Open items

- The exact Markdown grammar (section 14) needs a reference implementation shared by the server validator and the Rust core renderer.
- `ChannelSnapshot.messages` page size and whether snapshots are cacheable server-side.
- Whether `HighWaterMarkRequest` with an empty channel list is bounded by `Limits.max_subscriptions` or by a separate cap.
- MCP tool and resource definitions.
- OpenAPI generation: which generator and which annotations carry the HTTP mapping in section 9 so the table is derived, not maintained by hand.
