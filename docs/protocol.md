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

1. **Ids are strings a person can read.** Every `bytes` field whose name ends in `_id` or `_ids`, plus `content_address` and `nonce`, is a canonical lowercase UUID string when it holds sixteen bytes (account, channel, message, device and the other ids) and lowercase hex otherwise (an identity id is a 32-byte hash, a content address a 32-byte digest, a nonce 24 bytes), never base64. A server reading JSON still takes base64 in such a field, so a client written against stock ProtoJSON keeps working.
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
GET /gateway/websocket?vsn=1.0.0 HTTP/1.1
Upgrade: websocket
Sec-WebSocket-Protocol: phoenix, base64url.bearer.phx.<base64(access_token), unpadded>
```

**As implemented in v1**, encoding is chosen by the opcode of the first frame rather than by subprotocol: a binary frame selects protobuf, a text frame selects JSON, and the connection keeps that encoding. The access token travels in `Sec-WebSocket-Protocol` as `phoenix, base64url.bearer.phx.<base64 of the token, unpadded>`; the server answers with the `phoenix` subprotocol. It is never a query parameter, which would land in every access log, and browsers cannot set `Authorization` on a WebSocket.

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

`PresenceUpdate` sets the client's own status and **replaces** the set of servers whose presence the client wants to receive. Clients include a server only while its member list or a channel in it is on screen; a server the account is not a member of is ignored. A newly watched server answers first with a `PresenceDiff` whose `joined` lists everyone present (one entry per account, the most present of its devices' statuses), and then with the diffs as they happen: `joined` for an account that arrived, `updated` for one whose status changed, `left_account_ids` for one whose last device stopped watching or disconnected. `OFFLINE` and the unspecified status are taken as `ONLINE`. `PresenceDiff` frames arrive scoped to one server, or to the friends topic when `server_id` is empty (the friends topic is not served yet).

`Typing` is sent at most once per 5 seconds while the user is typing in a channel. Servers fan it out as `TypingIndicator` with an `until` about 8 seconds ahead; clients stop showing it at `until` without needing a stop message. Typing is never sent for `PERSONAL_*` channels over the gateway in plaintext; clients that want typing in E2EE DMs send it inside an MLS application message.

The server takes `Typing` only from a session subscribed to the channel, for an account that holds `SEND_MESSAGES` there by the subscription's cached view, and fans it out to the channel's other subscribed sessions, never to the sender's own account. Anything else is dropped without an answer: a channel the session is not subscribed to, a `PERSONAL_*` channel, and a second `Typing` for the same channel within 3 seconds of the last. Nothing is stored and nothing is replayed; a client that connects while someone is typing learns of it at their next `Typing`. `until` is on the server's clock: a client measures it against `Welcome.server_time`, not against its own clock, shows no indicator for longer than 10 seconds whatever `until` says, never shows its own account, and stops showing an account when a message of theirs arrives in the channel.

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
| `POST /auth/sign-in/magic-link` | `MagicLinkRequest` → 202 | Public. 202 whether or not the address has an account; the mail is sent only when it does, with the link `https://{host}/sign-in?magic={token}` and the token as a code. Five an hour per address (`RATE_LIMITED`); `POLICY_DENIED` when `InstanceInfo.magic_link_enabled` is off. |
| `POST /auth/sign-in/magic-link/complete` | `MagicLinkCompleteRequest` → `TokenPair` | Public. The token, once, within fifteen minutes; `UNAUTHENTICATED` otherwise. |
| `GET /auth/oidc/start?return={url}` | → 302 to the provider | Public. The code flow with PKCE and a nonce, for the one provider the operator configured (`InstanceInfo.oidc_name` names it; `POLICY_DENIED` without one). `return` must be on the instance or a trusted web origin, or on the apps' own `lumia:` scheme (the system browser sheet comes back to the app on it; the one-time token is what bounds a stranger's app claiming the scheme); the flow is kept ten minutes. |
| `GET /auth/oidc/callback` | → 302 to `{return}?magic={token}` | Public. The provider's `code` and `state`; the ID token is verified against the provider's keys, the account bound to the provider's subject is found or made (an email in the claims fills a new account's address but never matches an existing one), and the token completes at `POST /auth/sign-in/magic-link/complete`. A stale or failed flow lands at `{return}?error=oidc`. |
| `POST /auth/passkey/register/start` | → `PasskeyChallenge` | The creation options for the caller, as the browser's JSON (binary fields base64url); the challenge is kept five minutes, single use. |
| `POST /auth/passkey/register/finish` | `PasskeyRegisterFinishRequest` → 204 | The credential `PublicKeyCredential.toJSON()` made; `ALREADY_EXISTS` for a credential id already stored, `UNAUTHENTICATED` for a spent or unknown challenge. Attestation is not required. |
| `POST /auth/passkey/assert/start` | `PasskeyAssertStartRequest` → `PasskeyChallenge` | Public. With a handle or email, the account's passkeys fill `allowCredentials` (an unknown one gets an empty list); without one, a discoverable passkey is expected. |
| `POST /auth/passkey/assert/finish` | `PasskeyAssertFinishRequest` → `TokenPair` | Public. A signature count that did not move on an authenticator that counts is a clone and `UNAUTHENTICATED`. |
| `GET /auth/passkeys` | → `ListPasskeysResponse` | `required_for_password` says whether a password sign-in must also present one of them. |
| `PUT /auth/passkeys/required` | `PasskeyRequirementRequest` → 204 | The passkey as the second factor: a password sign-in then answers `FAILED_PRECONDITION` with `details.mfa` `passkey` until the request carries an assertion (`passkey_challenge_id` and `passkey_credential_json`, from `POST /auth/passkey/assert/start` and the browser's ceremony) by one of the account's passkeys; a wrong one is `UNAUTHENTICATED`. `FAILED_PRECONDITION` with `state` `no_passkey` when the account has none to require. |
| `DELETE /auth/passkeys/{passkey_id}` | → 204 | |
| `POST /accounts` | `CreateAccountRequest` → `TokenPair` | Public. `POLICY_DENIED` if registration or identity-proof creation is off. |
| `POST /auth/refresh` | `RefreshRequest` → `TokenPair` | Rotates the refresh token. The old one is invalid after use. |
| `POST /auth/sign-out` | → 204 | Revokes the current session and closes its gateway connection. |
| `POST /auth/handoff` | → `HandoffResponse` | A one-time sign-in token for the caller's own account, completed at `POST /auth/sign-in/magic-link/complete` as a mailed token is, within fifteen minutes: a native app that signs in through the instance's web page (a passkey, say) takes the token back by its return URL, and an app hands its own session to that page the same way (to make a passkey there). |
| `GET /sessions` | → `ListSessionsResponse` | |
| `DELETE /sessions/{session_id}` | → 204 | Revokes and disconnects. |
| `POST /auth/reauth` | `ReauthRequest` → `ReauthResponse` | Five-minute token for revoke and link. |
| `GET /identity/roster` | → `GetRosterResponse` | The newest roster this instance holds for the caller's identity. |
| `GET /accounts/{account_id}` | → `Account` | Another account's public profile (handle, display name, avatar); `NOT_FOUND` unless the caller is it, shares a server or a `PERSONAL_*` channel with it, or has a DM request waiting between them. |
| `PATCH /account` | `UpdateAccountRequest` → `Account` | The caller's own handle, display name, or both; a field left out is left as it is and an empty display name clears it (architecture 6.2, "Names"). `ALREADY_EXISTS` for a handle another account has, or holds from a rename of its own within the last 30 days; `INVALID_ARGUMENT` for one that is not a handle or a name over 64 characters. `ProfileUpdated` goes on the account log. Creating an account answers the same two codes for its handle. |
| `PUT /account/avatar` | a picture's bytes → `Account` | The caller's picture: the body is the file as chosen (PNG, JPEG, WebP or GIF, at most 8 MB, 8,192 pixels a side and 25 million in all), whatever `Content-Type` says. The instance keeps its own square of it (architecture 12, "Avatars") and names it in `Account.avatar_url`. `INVALID_ARGUMENT` with `details.field` `picture` and `details.reason` `unsupported`, `too_large` or `invalid`; `FAILED_PRECONDITION` with `details.state` `blobs` on an instance that keeps no blobs; `RATE_LIMITED` when it is busy with others. `ProfileUpdated` goes on the account log. |
| `DELETE /account/avatar` | → `Account` | The caller has no picture again. |
| `GET /avatars/{address}` | → the picture | **Public, no session**: `address` is 64 hexadecimal digits nobody can compute. `image/jpeg` or `image/png`, immutable (`Cache-Control: public, max-age=31536000, immutable`). `Account.avatar_url` is this path, relative to the instance (`/api/v1/avatars/…`), when the picture is the instance's own; a client resolves it against the instance it is talking to. An absolute URL there came from a signed profile issued elsewhere, and a client does not fetch it unless the person asks (it would tell that host who is looking). |
| `PUT /account/profile` | `SignedProfile` → `Account` | The caller's new profile, signed by a device of its identity's held roster (`IDENTITY_PROOF_INVALID` otherwise), with a version above the one held (`CONFLICT` otherwise); the display name follows, and the avatar when the profile names one (one that names none leaves the account's picture as it is), and `ProfileUpdated` goes on the account log. `FAILED_PRECONDITION` for an account with no identity. `CreateAccountRequest.profile` imports one the same way at creation by identity proof. |
| `GET /accounts/{account_id}/roster` | → `GetRosterResponse` | Another account's roster, for validating its MLS leaves; `NOT_FOUND` unless the caller shares a `PERSONAL_*` channel with it. |
| `GET /identity/rosters/by-root/{root}` | → `GetRosterResponse` | Public. The roster whose current root public key is `root` (base64url), for a device recovering from a phrase after a root rotation (specs/identity.md 4.3); `NOT_FOUND` when no identity here has that root. |
| `POST /identity/roster` | `PublishRosterRequest` → 204 | Verified per specs/identity.md 3.3. `CONFLICT` carries the held roster in `details.held`. Rosters that add revocations are refused here with `POLICY_DENIED`. |
| `POST /identity/backup-key-handoff` | `PublishBackupKeyHandoffRequest` → 204 | After a root rotation (specs/identity.md 4.5 step 5): the new backup key sealed to the identity's other devices, signed by the new root. Checked against the held roster's version and root (`FAILED_PRECONDITION` with `state` `roster_version` when the roster has not arrived, `IDENTITY_PROOF_INVALID` for another signer, `PERMISSION_DENIED` for another identity), then `BackupKeyHandedOff` on every bound account's log. |
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
| `POST /channels` | `CreateChannelRequest` → `Channel`; needs `MANAGE_CHANNELS` in the server (`PERMISSION_DENIED` otherwise, also for a non-member) |
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
| `PUT /push/subscription` | `PushSubscription` → 204 | One per (device, kind), replacing the previous. Content-free pushes ("New message") go to devices with no live session, for personal DM messages and account mentions (architecture 15.5). |
| `DELETE /push/subscription?kind=` | → 204 | Removes this device's subscription of that kind. A client does this before it signs out. The instance also removes every subscription a device holds for an account when the device's last usable session of that account ends (signed out, revoked from elsewhere, the device revoked from the identity): a push is read with a session, and a signed-out device must not be told the account has mail. |
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
| `POST /invites/email` | `EmailInviteRequest` → `Invite` (architecture 6.2): a one-use invite good for a week, mailed to the address as a link to this instance's web page; the same permission and limit as a link; `FAILED_PRECONDITION` with `state` `mail` when the instance sends no mail (`InstanceInfo.magic_link_enabled`), `INVALID_ARGUMENT` for a malformed address. The address stays on the `Invite` as `email`. |
| `GET /servers/{server_id}/invites` | → `ListInvitesResponse`, newest first; `CREATE_INVITES` |
| `POST /invites/{code}/accept` | → `Member` |
| `DELETE /invites/{code}` | → 204 |

Creating needs `CREATE_INVITES` in the server; `max_uses` 0 is unlimited; a `channel_id` must be the server's. The preview and accepting answer `NOT_FOUND` alike for an unknown, expired or spent code. Accepting joins the caller as a member, or as a guest whose membership expires after the invite's `guest_days`, and counts a use; a caller already in the server gets the membership back without a use. `CreateAccountRequest.invite_code` joins the new account in the same request and admits it when registration is closed; an unknown code makes no account. Deleting is the creator's or needs `MANAGE_SERVER`. Minting and accepting are rate-limited per account per hour (`RATE_LIMITED`), counted from the rows themselves so every node shares the count. An invite link is `https://{host}/invite/{code}`, which the instance's web UI answers (architecture 6.1: the trust card first).

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
- **Refresh token**: opaque, 30 days, single use. `POST /auth/refresh` returns a new pair and invalidates the old refresh token. Reuse of an old refresh token revokes the whole session, with one grace: for 60 seconds after a rotation, the token it retired is answered with the same pair again and nothing else changes, because the first answer may never have arrived and a client must not lose its session to a dropped connection. A client still makes one rotation at a time and never presents a token it knows it has rotated; the grace is for the answer it did not get. A token retired longer ago than that, presented again, is the reuse that revokes.
- **Bot tokens**: long-lived, scoped, sent as bearer. Scopes are permission names from `chat.proto`; a bot's effective permissions are the intersection of its scopes and its roles.
- **OAuth 2.1** with PKCE for user-authorized integrations, standard endpoints under `/oauth/`.
- A session is bound to a `device_id`. If the device is revoked in the roster the instance holds, every call fails with `DEVICE_REVOKED` and the gateway disconnects.
- A client holds one session. A sign-in made while it holds one (linking a device that had signed in with a password, signing in again without signing out) revokes the held session once the new pair has arrived, so nothing is left alive on the instance that no client can sign out of.
- A client gives every request a time to be answered in (the core: 20 seconds, more for what carries a picture or a chunk of history, 8 for the sign-out itself) and treats one that runs out as a transport failure. A refresh that ran out is the answer that never arrived, which the grace above is for.

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
- `EncryptedPayload.kind` is all the server reads: `APPLICATION` events are eligible for push; `CONTROL` (a commit or external commit) and `WELCOME` (a welcome) are not. Everything else about the message is inside `mls_message`: for `APPLICATION` it is a `DmOperation` (`e2ee.proto`), for `CONTROL` an MLS commit, for `WELCOME` an MLS welcome.
- `epoch` on a `CONTROL` event is the epoch the commit produces; the delivery service accepts it only when it is the conversation's current epoch plus one, advancing the epoch, and answers `FAILED_PRECONDITION{state: "epoch"}` otherwise, which is how concurrent commits are serialized: the second of two commits for the same epoch finds the conversation already there. `epoch` on a `WELCOME` event is the epoch its commit produced, which must be the current one; it advances nothing. Application messages carry the epoch they were encrypted in, are accepted for any epoch, and are ordered by `seq`.
- Snapshots return `encrypted_events` rather than `messages`; there are no server-side messages to return.
- `ThreadSummary`, `ReactionCount`, `pinned`, and `Mention` are never populated by the server for these channels. Clients compute them from decrypted content.

## 16. Open items

- The exact Markdown grammar (section 14) needs a reference implementation shared by the server validator and the Rust core renderer.
- `ChannelSnapshot.messages` page size and whether snapshots are cacheable server-side.
- Whether `HighWaterMarkRequest` with an empty channel list is bounded by `Limits.max_subscriptions` or by a separate cap.
- MCP tool and resource definitions.
- OpenAPI generation: which generator and which annotations carry the HTTP mapping in section 9 so the table is derived, not maintained by hand.
