# Plan

The working checklist for building Lumia. Section 18 of [architecture.md](architecture.md) gives the order and why; this file tracks what is done, what is next, and what changed along the way. Update it when work lands, not after.

Legend: `[x]` done · `[~]` in progress · `[ ]` not started · `(spec §n)` the section a task implements · `(tests)` the failure-injection tests that close it.

## Status at a glance

| Step | State |
|---|---|
| 1. Schemas and protocol contract | Done |
| 2. Server: event log, delivery, account log, resume, authorization | First pass complete 2026-11-01; six items carried into step 3 (listed under "Closing step 2") |
| 3. Rust core | Done 2026-11-01: sync engine, SQLite store, reconnecting CLI on the real gateway, identity with shared vectors, scenario suite, UniFFI and wasm surfaces |
| 4. Identity across two instances | First pass 2026-11-01: lifecycle run across two dev instances with the CLI; the §12 list is partly end to end, the rest unit-level |
| 5. MLS personal DMs, backup, recovery | Built 2026-11-02 through live personal DMs on three devices (core, instance, CLI engine); history backup, recovery, and the remaining §12 cases open |
| 6. Web UI in Tauri | Not started |
| 7. Multi-node, push, SwiftUI, Compose | Not started |

Specs written: [identity](specs/identity.md), [delivery](specs/delivery.md), [authorization](specs/authorization.md). Still provisional in the architecture: E2EE backup and rejoin (11.3), deployment recovery (15).

## Completed

### Step 0: design (2026-11-01)

- [x] `docs/architecture.md`, decision-record form, hard rules in section 1.
- [x] Two external reviews worked through finding by finding; every accepted finding is in the doc.
- [x] `docs/protocol.md` and `proto/lumia/v1/` (seven files), compiling under `protoc`.
- [x] Three specs that retire three of the five provisional areas.
- [x] `README.md`, `AGENTS.md`, `LICENSE` and `NOTICE` at root, `apps/instance`, and `proto/`.

### Step 1: schemas and protocol contract (2026-11-01)

- [x] `common`, `identity`, `chat`, `events`, `commands`, `gateway`, `api` schemas.
- [x] JSON overrides (UUIDs as strings, int64 as strings, enums as names), compatibility rules, HTTP mapping, error codes, token lifetimes, identity proof.
- [x] Identity additions: device certificates, signed revocations, root rotations, link bundles, identity backup.
- [x] Delivery additions: `DISCONNECT_SLOW_CONSUMER`, `PayloadsPurged`.

### Step 2, part 1: event log (2026-11-01)

- [x] Phoenix app generated in `apps/instance` (`--no-html --no-assets --no-gettext`), stock output committed separately for a reviewable diff.
- [x] Repo defaults: UUID keys, microsecond UTC timestamps; `Lumia.Schema` minting monotonic UUIDv7 app-side.
- [x] Migration: `channels`; `events`, `event_payloads`, `commands` hash-partitioned by `channel_id` into 32; `messages`.
- [x] `Lumia.Events.append/2`: the one-transaction write path with idempotent replay and conflict detection (delivery §3, §9). 11 tests.
- [x] `mix lumia.spike.events` in `bench/` (dev only). Results in [spikes/2026-11-01-event-log.md](spikes/2026-11-01-event-log.md): 10k writes/s across 100 channels at p99 6 ms; partition pruning verified.
- [x] Tooling: Credo (tuned), Benchee, `docker-compose.yml`, GitHub Actions with Postgres 18, `.tool-versions`.

## Decisions and changes along the way

Things that moved from the first draft, with the reason. The architecture records the current decision; this list records that it changed.

| Was | Now | Why |
|---|---|---|
| Open core with a commercially licensed `ee/` directory | One AGPL edition, revenue from hosted plans (hard rule 6) | A second edition splits the codebase and the trust message; hosted plans can carry the business. |
| Home host per identity, DM-only guest accounts to reach strangers | Personal DMs live where they were started; the flagship is global because everyone has an account there | The home-host machinery served only two people on isolated instances who refuse the flagship. Recorded as deferred. |
| One identity private key copied to every device | Root key offline, per-device keys, signed roster with certificates | With identity-proof sign-in, a copied root key makes a stolen device a permanent takeover. MLS wants per-client keys anyway. |
| Roster signed as a whole, verified against history | Self-certifying roster: every device entry and revocation carries its own signature | An instance seeing version 10 first cannot verify the signer was ever authorized without every earlier version. |
| `guest` as an account kind | `guest` as a membership kind | One account can be an employee in one server and a contractor in another. |
| Content-free silent push wakeups | Visible push with generic text, on-device fetch and decrypt in the notification extension | iOS throttles and drops silent pushes; they cannot carry a timely notification. |
| Subresource integrity as the web E2EE defense | Origin-scoped web device keys; the origin operator is the trust boundary | SRI hashes are set by the party serving the HTML. |
| One Phoenix topic per chat channel | One Phoenix channel per socket, chat subscriptions as PubSub entries in that process | 100k sockets × 50 topics is 5M processes. |
| OpenMLS on the server through Rustler for delivery-service duties | No MLS library on the server; the delivery service stores opaque bytes and compares the epoch integer the client puts in the envelope | None of the duties parse an MLS message, and a server that cannot parse ciphertext cannot be asked to. |
| Web MLS keys as non-extractable WebCrypto objects where possible | Keys in WASM memory while the page runs; the OPFS store encrypted at rest under a WebCrypto key | WebCrypto has no HPKE; protecting one key while the rest sit in WASM memory is a claim the product could not make honestly. |
| Bare `Phoenix.Socket.Transport` (proposed during the delivery grill) | Phoenix Channel with a custom serializer; `Hello` is the join, no envelope on the wire | Keeps Presence, `ChannelTest`, and `phoenix.js`; the serializer removes the framing cost. |
| "Cursor expired" meaning purged rows | Event rows permanent; "no longer served" is a 1,000-event replay cap | Permanent rows were already decided; the cap is what actually bounds replay cost. |
| `acl_version` bumped on every membership change | Bumped on definition-level changes only; member-level changes travel the account log | Bumping every channel on every role assignment is a re-resolution storm. |
| Flagship as mandatory identity provider (considered) | Key-based identity, flagship directory optional | Self-hosters would depend on Lumia's uptime to log in. |
| `apps/server` | `apps/instance` | "Server" means a community in Lumia. |
| `--binary-id` generator flag | Repo migration defaults plus `Lumia.Schema` | Same effect, and the schema module is where the UUIDv7 choice lives anyway. |
| `last_seq_at` indexed at microsecond precision | Second granularity, `fillfactor = 70` on `channels` | Every write changed an indexed column, defeating HOT; reconciliation touched 43k buffers for 100 rows. |
| Spike task under `lib/` | `bench/`, compiled in dev only | Benchee must not ship; the task must not compile in test or prod. |
| Role and overwrite limits at Discord's caps (250 / 100 / 50) | 50 / 50 / 20, operator-configurable | Discord's caps are sized for communities of hundreds of thousands. |
| Credo AliasUsage at stock | Fires above two uses, excludes `test/support` | Generated test scaffolding should stay stock. |
| Gateway encoding negotiated by WebSocket subprotocol | Chosen by the opcode of the first frame (binary = protobuf, text = JSON) | Phoenix does not expose the negotiated subprotocol to the socket; the opcode is unambiguous and needs no negotiation. |
| `Authorization` header on the gateway upgrade | `?access_token=` query parameter for all clients in v1 | Phoenix sockets cannot read arbitrary headers; a custom upgrade plug is deferred. |
| Untrusted parsing (image decoding) in a Port-driven worker OS process | Dirty-scheduled Rust NIFs with input bounds checked first; a Port worker only for C-linked crates and video | Rustler catches Rust panics and Rust rules out C's memory bugs, so the Port defended against failures that cannot happen. Memory exhaustion is defended by bounds, not by process isolation. |
| BLAKE3 via a Rustler NIF (architecture §3 names it) | The pure-Elixir `b3` package, behind `Lumia.Crypto` | Identity inputs are 32-byte keys and small bodies; `b3` is maintained with no dependencies, while the `blake3` NIF package pins Rustler 0.30. Keeps the Rust toolchain out of contributor setup until MLS needs it. The facade makes a NIF for large blobs a one-line swap. |
| Overwrite with a bit in both allow and deny "treated as deny" at resolve time | Normalized on write: a denied bit is cleared from allow | The spec's own same-layer rule made allow win; property tests exposed the contradiction. Normalizing storage keeps the algorithm as written. |

## Step 2, part 2: delivery, accounts, authorization

Ordered. Each task names the spec section it implements and the tests that close it. Tests come from the spec's test list; a task is done when its tests pass in CI.

**Foundations** (done 2026-11-01)

- [x] `accounts` table with `instance_role`, `last_seq`, `last_seq_at` (second granularity, `fillfactor = 70`), and `instance_settings` with `recovery_generation` read into `persistent_term` at boot by `Lumia.Instance` (delivery §2, §8).
- [x] `account_events` and `account_event_payloads`, hash-partitioned by `account_id`; `Lumia.Accounts.append_event/3` with the same write-path shape as `Lumia.Events` (delivery §2).
- [x] `servers`, `server_members`, `roles`, `member_roles`, `channel_overwrites`, `audit_log` with `Lumia.Audit.record/4` (authorization §2, §9). `channels` gained `server_id`, `name`, `topic`, `position`.
- [x] Oban 2.24 with `Lumia.Workers.PurgeCommands` on a daily cron (delivery §9). A retry after purge surfaces as `CONFLICT`, never a duplicate message; tested.

**Authorization** (done 2026-11-01)

- [x] `Lumia.Authz.resolve/2` over bitfields, `test/support/authz_reference.ex` over `MapSet`s, property tests between them (authorization §3). `Lumia.Authz.Permission` maps names to proto enum bits.
- [x] `Lumia.Authz.Cache`: ETS keyed `(channel_id, acl_version)` with TTL sweep; `fetch/2` returns `:stale` when the channel has moved on (authorization §4).
- [x] `Lumia.Servers`: definition-level changes bump `acl_version` in-transaction; server-wide bumps enqueue `Lumia.Workers.AclChanged` to append `acl_changed` events in batches; channel-level bumps append it directly (authorization §6.1).
- [x] Member-level changes append `membership_added`, `membership_updated`, `membership_removed`, `channel_removed` to the account log and touch no `acl_version` (authorization §6.2).
- [x] `Lumia.Workers.ViewerDiff`, debounced 30 s per server, diffing against a `channel_viewers` table (authorization §6.3).
- [x] Management hierarchy, owner rules, `@everyone` undeletable, operator limits via `:limits` config (authorization §3.3, §10).
- [x] `Lumia.Audit.record/4` called in every change's transaction (authorization §9).
- [x] `Lumia.Events.append_system/3` for server-originated channel events.

**Wire types** (done 2026-11-01)

- [x] `protobuf` 0.17 with generated `Lumia.V1.*` modules in `lib/lumia/proto`, regenerated by `scripts/gen_proto.sh`. Generated code is committed.
- [x] Event and account-event payloads stored as serialized `Lumia.V1.Event` / `AccountEvent` bytes, so the stored bytes are the broadcast bytes (delivery §4). `Lumia.Events.append/4` takes a `Lumia.V1.Command` and an actor; `append_system/3` and `Accounts.append_event/3` take payload tuples. Both broadcast after commit on `channel:<id>` / `account:<id>`.
- [x] `Lumia.Proto` converts schemas and scalars (UUID bytes, timestamps) to wire structs.
- [x] Edit and delete materialize; delete redacts the create event's payload (delivery §10).

**Sessions** (done 2026-11-01)

- [x] `sessions` table and `Lumia.Auth`: opaque 15-minute access and 30-day single-use refresh tokens stored as SHA-256; `verify_access/1` checks the row on every call; `refresh/1` rotates both; `revoke/1` marks the row, broadcasts on `session:<id>` so a live socket closes, and appends `SessionRevoked` to the account log; `revoke_device/2` for the identity spec's revoke flow (architecture §6.3, protocol §12).

**Gateway** (first pass done 2026-11-01)

- [x] `LumiaWeb.Gateway.Serializer`: raw `ClientFrame` in, bare `ServerFrame` out, `Hello` as join, event bytes wrapped without re-encoding; encoding chosen by the first frame's opcode (delivery §5; tests: serializer unit tests).
- [x] `LumiaWeb.Gateway.SessionChannel`: subscription state, the four-line delivery rule, authoritative resolve on subscribe and on `acl_version` change, membership-driven unsubscribe (delivery §5; tests: N+2 before N+1 in order, duplicates dropped, kicked member receives nothing, acl change caught).
- [x] Encode-once broadcast from `Lumia.Events` and `Lumia.Accounts` after commit (delivery §4).
- [x] One replay loop for subscribe, live gaps and nudges; 1,000-event cap answers `SNAPSHOT_REQUIRED` (delivery §6; tests: cap, gap fill).
- [x] `LumiaWeb.Gateway.Reconciler`: 5 s tick with 1 s overlap, node-local head broadcasts for channels and accounts (delivery §7; test: nudge fills a suppressed broadcast).
- [x] Account-log subscription at join, `ResumeAccount`, live account events (delivery §5).
- [x] Session revocation closes the socket (test).
- [x] Heartbeat echo, `Welcome` with recovery generation and limits.
- [x] Slow-consumer disconnect: the transport mailbox past 1,000 frames sends `Disconnect{SLOW_CONSUMER}` and stops (delivery §5; test).
- [x] Bounded staleness: a subscription not re-resolved in 10 minutes is re-resolved before its next delivery; `nodeup` re-resolves every subscription (delivery §5, authorization §5; tests).
- [ ] Presence scoped per server plus the friends topic (architecture §3); `PresenceUpdate` and `Typing` are accepted and ignored for now.
- [ ] `Authorization` header on the upgrade (needs a custom upgrade plug); v1 uses `?access_token=`.
- [ ] JSON sessions use stock ProtoJSON; the protocol's JSON overrides (UUID strings, int64 strings) are not applied yet.

**HTTP** (first pass done 2026-11-01)

- [x] `LumiaWeb.Plugs.ProtoBody` reads JSON or protobuf bodies; `LumiaWeb.ProtoConn` decodes into `Lumia.V1` structs and renders JSON or protobuf by `Accept`; `FallbackController` maps error reasons to the envelope and status table (protocol §2, §10).
- [x] `LumiaWeb.Plugs.Auth`: bearer access token verified against the session row on every request (protocol §12).
- [x] `GET /instance` (trust card), `POST /accounts` with Argon2id password, `POST /auth/sign-in/password`, `POST /auth/refresh`, `POST /auth/sign-out`, `GET /sessions`, `DELETE /sessions/{id}`.
- [x] `GET /account/snapshot`, `GET /account/events` (protocol §7).
- [x] `GET /channels/{id}/events`, `/snapshot`, `/messages` with `has_more`, `POST /channels/{id}/commands` with idempotent replay (delivery §8; test: snapshot and scrollback paging).
- [x] Servers, channels, members, roles, overwrites endpoints with authoritative checks through `Lumia.Authz.Access`, shared with the gateway; non-members get `NOT_FOUND` (authorization §5).
- [x] Malformed `*_id` path parameters answer `NOT_FOUND` through `LumiaWeb.Plugs.ValidateIds` instead of raising (found by the first CLI run).
- [ ] Pagination cursors for list endpoints (members, search) and the `PageRequest` shape (protocol §11); members currently return everything.
- [ ] `truncated_before` is always false: rows are permanent, so no HTTP fetch is truncated yet.
- [x] `Lumia.Workers.Retention` (daily): per-server `retention_days` set by `Servers.set_retention/3` (MANAGE_SERVER, audited); `Events.purge_before/2` nulls payloads, deletes messages, appends `PayloadsPurged`; resumed clients see `Redacted` (delivery §10; tests: purge, idempotence, no policy untouched).
- [ ] Magic link, passkeys, OIDC, MFA (architecture §6.3).
- [ ] Invites and discovery endpoints (architecture §6.6).
- [ ] JSON overrides (UUID and int64 as strings) for JSON clients; stock ProtoJSON for now.

**Identity on the server** (first pass done 2026-11-01)

- [x] `Lumia.Identity.Roster`: all eight verification steps as a pure function, every rejection naming its step (identity §3.3; tests: certificate, chain, signature, and monotonicity cases with real Ed25519 keys).
- [x] `Lumia.Crypto` facade: Ed25519 from `:crypto`, BLAKE3 from `b3`, HMAC.
- [x] Challenges: HMAC-bound, host-bound, two-minute expiry, `used_challenges` single use across nodes (identity §9; test: replay).
- [x] `Lumia.Identity.accept_roster/2` with the revocation policy: device-signed revocations only via the revoke endpoint with reauth, root-signed bypass; on acceptance revoked devices lose their sessions and `RosterUpdated` goes on the account log (identity §4.4; tests: refused on publish, accepted with reauth, root path).
- [x] `POST /auth/reauth` with password; tokens single use, five minutes (identity §10).
- [x] Link bundles: 8-character code, single use, ten minutes, five wrong attempts delete, 64 KB cap (identity §6; tests).
- [x] Identity backup put and get, highest version wins (identity §8).
- [x] Identity-proof sign-in and account creation gated by instance policy; `POST /identity/link` binds an account with reauth (identity §4.6, architecture §6.3).
- [x] Web device origin check against the instance origin and configured flagship origins (identity §7).
- [ ] Reauth by passkey or TOTP; only password exists.
- [ ] Profile import on link (signed profile → account display fields).
- [x] `Lumia.Workers.PurgeExpired` (hourly) deletes expired `used_challenges`, `link_bundles`, `reauth_tokens` (test).
- [ ] A revoked device that is still connected over the gateway is disconnected through its session revocation; verified for sessions, not yet end to end with a live socket in the identity tests.
- [x] `RevocationBody` carries the revoked device's public key and `added_at`, so certificates it issued before revocation stay verifiable from the roster alone (found by the verifier; proto and spec amended).

**Closing step 2** (first pass complete 2026-11-01)

- [x] Every group above through its first pass with its tests green in CI.
- [x] Both spikes run and recorded; section 17 updated.
- [ ] Carried into step 3, to be built against the first real client: pagination cursors for member and search listings (protocol §11); presence scoped per server plus the friends topic (architecture §3); the JSON overrides for bot clients (protocol §2); reauth by passkey or TOTP; profile import on link; an end-to-end identity test with a live socket.
- [ ] The failure-injection lists in delivery §12, authorization §11 and identity §12 are covered piecemeal by unit and channel tests; a single named suite that runs them as listed, including the multi-node cases, waits for step 7.

## Step 3: Rust core

**Milestone 1** (2026-11-01): the sync engine talks to the real gateway.

- [x] Workspace at `apps/core`: `lumia-proto`, `lumia-core`, `lumia-client`, `lumia-ffi`, `lumia-wasm`; Rust 1.96 pinned; Apache-2.0; `AGENTS.md`; `core.yml` CI with fmt, clippy `-D warnings`, tests, wasm32 check.
- [x] `lumia-proto`: prost through protox from `proto/` at build time, nothing committed.
- [x] `lumia-core::sync::Session`: sans-I/O state machine with scripted tests for in-order apply, duplicates, gap fetch with a single in-flight fetch, snapshot path holding live events, nudges and high-water marks, generation change discarding cursors, subscription errors (protocol §5, delivery §8).
- [x] `Store` and `Keystore` contracts; `wire` scalar helpers.
- [x] `lumia-client`: SQLite store applying events, materialized messages and cursor in one transaction, refusing gaps; snapshot replace; outbox; HTTP client with protobuf bodies; gateway loop carrying out effects with heartbeat and outbox retry on welcome; file keystore (dev only).
- [x] `lumia-cli`: `instance add`, `account create`, `login`, `servers`, `server create`, `channels`, `channel create`, `send`, `tail`.
- [x] Reconnect with backoff: `gateway::run_forever` wraps one connection with exponential backoff (1s doubling to 30s, a quarter jitter), re-subscribes from the store's cursors, honours the server's `reconnect_after_seconds`, resets after a completed handshake, and stops on `SESSION_REVOKED`, `DEVICE_REVOKED`, `PROTOCOL_ERROR`, or when the application drops its sender. `tail` and `send` run on it.
- [x] Refresh-token rotation: the token pair lives behind a shared handle in `Api`; an `UNAUTHENTICATED` or `TOKEN_EXPIRED` answer with a refresh token on hand rotates once and retries the call, a 401 at the socket upgrade rotates before the next attempt, and a hook persists each new pair (the CLI and the FFI client write it to the keystore).
- [x] `lumia-cli scenario <name|all>`: the failure-injection cases a client can drive on its own against a running instance, each with its own throwaway server, channel, or identity: `token-rotation`, `frame-before-hello`, `duplicate-command`, `snapshot-under-writes`, `snapshot-threshold` (1,000 behind replays, 1,001 takes the snapshot), `stale-roster`, `device-revoke`, `reconnect`. All eight pass on 2026-11-01. Two server bugs found by them and fixed: a frame before `Hello` crashed the transport instead of answering `DISCONNECT_PROTOCOL_ERROR` and closing; a concurrent identical command that lost the race on the message row got `CONFLICT` instead of the winner's ack. The cases that need the server to misbehave (out-of-order broadcast, purged command row, generation bump, slow consumer, partition) stay server-side tests in `apps/instance`.

**Milestone 2** (2026-11-01): identity in the core.

- [x] `lumia-core::identity::phrase`: 24-word BIP39 phrase, HKDF derivation of the root and backup keys, identity id (identity §2).
- [x] `identity::keys`: Ed25519, XChaCha20-Poly1305, BLAKE3, root and device `Signature` helpers.
- [x] `identity::roster`: the eight-step verifier as a port of `Lumia.Identity.Roster`, builders for device entries, revocations, rotations and signed rosters (identity §3, §4).
- [x] `identity::link`: `LinkRequester` and `seal_bundle` over X25519 and HKDF (identity §4.2, §6).
- [x] Cross-implementation vectors: `apps/instance/test/lumia/identity/vectors_test.exs` writes `proto/vectors/identity/rosters.json` with `LUMIA_WRITE_VECTORS=1` and checks the Elixir verifier against it; `apps/core/crates/lumia-core/tests/identity_vectors.rs` checks the Rust verifier against the same file. 17 vectors, both agree.
- [x] `identity::proof` builds a proof from a host's challenge, refusing another host or purpose.
- [x] `lumia-cli`: `identity create` (phrase shown once, roster v1), `account create-by-identity`, `login --identity`, `identity link-request` / `approve` / `claim` (the full 4.2 flow with the bundle over X25519), `identity revoke --phrase` (root-signed, bypasses reauth), `identity show`.
- [x] Run end to end against the instance on 2026-11-01: create, account by proof, login by proof, link a second device, second device logs in, root revocation from the phrase, revoked device's session answers `SESSION_REVOKED` and its re-login is refused.
- [x] Older roster (identity §5): the instance now returns the held roster in `CONFLICT`'s `details.held` as protocol §9 said (it did not before). Sign-in, account creation by proof, and link approval adopt it, verifying it as a successor of the one held locally, and retry once; a device that is revoked in it gets `DeviceRevoked` and `identity show` marks it. Covered by the `stale-roster` scenario and the Elixir API test.
- [x] Device-signed revocation under reauth: `identity revoke <id> --password` reauths at `POST /auth/reauth`, signs the revocation with this device's key, and sends it to `POST /identity/devices/revoke`; `identity link-account --password` binds a password account to the identity at `POST /identity/link` under the same reauth. Covered by the `device-revoke` scenario, including the wrong-password refusal.

**Milestone 3** (2026-11-01): the binding surfaces, marshalling only.

- [x] `lumia-ffi` (UniFFI 0.32, proc macros): one `Client` object per instance with blocking methods for accounts, sign-in by password or identity, servers and channels, the local store's messages, the gateway loop (`start` with a `Listener` callback interface, `subscribe`, `send_message`, `foreground`, `stop`), and every identity flow the CLI has. A `ClientEvent` enum mirrors the gateway loop's events. The loop runs on the client's own tokio runtime and reports from a runtime thread. Uses the development store and keystore until the shells bring platform ones (steps 6 and 7); says so in its docs. Offline tests cover opening, identity creation, and the lifecycle guards.
- [x] `lumia-wasm` (wasm-bindgen): the sans-I/O `Session` as a JS class. Frames in as bytes, effects out as plain objects with a `kind` (`send`, `fetch`, `fetchAccount`, `snapshot`, `accountSnapshot`, `apply`, `applyAccount`, `discardAll`, `notify`), ids as `Uint8Array`, sequence numbers as numbers. The store and the socket are JS's, which is what step 6's SQLite-on-OPFS worker wants: the JS loop carries out `apply` with the cursor in one transaction and calls back `fetched` and `snapshotApplied`. Checked on `wasm32-unknown-unknown`.
- [ ] Binding generation (`uniffi-bindgen` for Swift and Kotlin, `wasm-pack` for the web package) and a smoke test from each language come with the shells in steps 6 and 7; nothing to generate against until then.
- [ ] Still carried from step 2 (listed under "Closing step 2"): pagination cursors, presence, JSON overrides for bots, passkey and TOTP reauth, profile import on link. Each is a server feature with a client counterpart; they land with the first UI in step 6 rather than against the headless client.

## Step 4: identity across two instances

**First pass** (2026-11-01), two dev instances on one machine rather than compose: `config/dev.exs` reads `PORT`, `LUMIA_HOST` and `SECRET_KEY_BASE`, so a second instance runs as `PORT=4001 LUMIA_HOST=127.0.0.1 SECRET_KEY_BASE=… DATABASE_URL=…/lumia_dev2 mix phx.server` beside the first. The two differ in host name (challenges are host-bound, identity §9), challenge key and database, which is what the identity flows care about.

- [x] The CLI keeps identity state in `<data>/identity/` (phrase-derived id, device key, roster), shared by every instance under that data directory; per-instance state (`base_url`, session, store) stays in `<data>/<host>/`. One device, many instances, as identity §5.1 says.
- [x] `identity status`: this device's roster version and the version each known instance holds, flagging an instance that is behind. `identity sync`: publish the held roster to every instance. `identity revoke --all`: revoke, then publish everywhere.
- [x] Run end to end: identity created on device A; an account bound to it on each instance; device B linked through instance 1 (instance 2 shows `v1 (behind)`); B signs in on both instances and instance 2 learns v2 from the proof's carried roster; A revokes B through instance 1 only (status shows instance 2 `v2 (behind)`, B's session on instance 1 answers `SESSION_REVOKED`, instance 2 still serves B); `identity sync` publishes v3 to instance 2 and B's session there answers `SESSION_REVOKED` too. The "revoked on 1 of 2" state from identity §5 is what `status` reports between the revoke and the sync.
- [x] From identity §12, run end to end with the Rust core as the client: revoke while another instance is behind, then reconcile (the first case, with the root-signed path across two instances; the device-signed path under reauth runs against one instance in the `device-revoke` scenario).
- [ ] From identity §12, covered by unit tests on both verifiers through the shared vectors but not yet driven end to end: signer revoked before and after `added_at`, future-dated certificate, omitted revocation at step 7, root rotation at step 3. Not yet covered anywhere: concurrent forks and the merge, proof replay across nodes (needs step 7), web-origin proofs (needs step 6), phrase recovery with every device gone and the MLS rejoin (needs step 5).
- [ ] Two instances in `docker-compose`, one flagship-like with a directory, for the step 7 multi-node runs.
- [ ] Phrase recovery with every device gone, against both instances.

## Step 5: MLS personal DMs, backup, recovery

- [x] OpenMLS 0.9.2 linked in a spike (2026-11-01): a two-member group with a welcome and an application message round-trips on Rust 1.96, and the crate compiles for `wasm32-unknown-unknown` with its `js` feature plus the `js`/`wasm_js` features of the three transitive getrandom majors. Its provider is crypto, randomness, and a 72-method typed key-value storage trait; WebCrypto has no HPKE, so the web proof obligation in architecture §20 resolved to keys in WASM memory with the OPFS store encrypted at rest.
- [x] `docs/specs/e2ee.md` written: suite 0x0001, basic credential of identity id and device id signed by the device key, leaf validation against the roster, key-package cadence (20, top up under 10, 90 days, one last-resort), group lifecycle (accepter creates; any device commits; epoch plus one at the delivery service; link approver adds leaves; revocation removes them), `DmOperation` inside the ciphertext with projection rules, client-encrypted attachments, SQLite storage provider in the store's transaction, web key handling, history backup (chunks under per-chunk keys, manifests under the backup key, on the hosting instance, off by default), recovery by external commit. Architecture 11.1, 11.3 and 11.4 amended; `e2ee.proto` and the history endpoints added to the protocol.
- [x] Core (2026-11-02): `lumia_core::mls` with the OpenMLS `StorageProvider` over an `MlsKv` key-value contract (`KvStorage`, `Provider`), leaf identities and `RosterLookup` validation, key packages with the last-resort capability, `Conversation` (create, join by welcome, external join from GroupInfo, add, remove, two-phase commit merge/discard, encrypt, `handle` for commits/proposals/application messages with sender validation, GroupInfo export, delete), and the `Projection`. Ten unit tests cover the three-device round trip, immediate refusal of a revoked leaf with one removal commit and a no-op retry, an unrostered key package, held future-epoch messages, external join, and reload from storage. Compiles for wasm32 with the getrandom JS features declared as target-specific dependencies.
- [x] Instance (2026-11-02): `Lumia.Dms` (`POST /dms/personal`, `dm_participants`, participant-gated access to `PERSONAL_*` channels), `Lumia.Mls` (key-package pool with 90-day expiry, one last-resort package served when the pool is empty and never deleted, claims under `FOR UPDATE SKIP LOCKED`, newest GroupInfo by epoch, purge on revocation and expiry), the epoch rule in `Events.append` (`CONTROL` at current + 1 advances, at current is a welcome or proposal, else `FAILED_PRECONDITION{state: "epoch"}` with no seq taken), `Welcome.key_package_count`, `GET /accounts/{id}/roster` across a shared DM, `Lumia.History` (chunks and manifests as opaque bytes, monotonic manifest versions, quota with `RESOURCE_EXHAUSTED` 507), encrypted snapshots. 162 tests.
- [x] Client (2026-11-02): `SqliteStore` implements `MlsKv` and runs the engine's work in `BEGIN IMMEDIATE` transactions so MLS state, the decrypted log, the processed cursor and the outbox entry commit together; `lumia_client::dm::DmEngine` (top-up on connect, rosters fetched per participant and re-fetched on a rejected leaf, group creation by the opener with claims for every device of both identities, commit then welcome then GroupInfo, own events merged or recorded when they come back through the log, removal commits on roster change, catch-up from the stored log so a restart never skips an event); `lumia-cli dm setup|open|tail|send|messages`. Run live against the dev instance with two identities and three devices: the opener creates the group with three leaves, both of the other identity's devices join from the welcome, messages go both ways and the three projections agree, a root-signed revocation makes the first device to learn of it remove the leaf in one commit, the other participant sees the removal, and the revoked device is refused at the gateway.
- [ ] Still to build in this step: history chunks and manifests from the client (the server side is in), recovery with the external-commit rejoin from the CLI, adding a newly linked device to existing groups (the link approver's duty in e2ee §4), `ContactRosterUpdated` on participants' account logs (today a participant re-fetches rosters on connect and on a rejected leaf), DM requests and the accepter-creates rule (today the opener creates), personal group DMs, the "sent before this device joined" marker for application messages before a device's welcome (today only control messages before the welcome are skipped silently; earlier application messages are recorded as before-join), and the e2ee §12 cases not yet driven live: the third linked device, concurrent commits over the gateway, the crash between persist and send, the epoch moving under an unacked send, and everything from recovery on.

## Step 6: web UI in Tauri

- [ ] Web device model with origin-scoped keys (architecture 11.4, identity §7).
- [ ] SQLite WASM on OPFS in a worker with Web Locks.
- [ ] The design system's components from the prototype, on the Rust core via WASM.
- [ ] Tauri shell with the core in-process (open question in architecture §20).

## Step 7: multi-node, push, native shells

- [ ] Write `docs/specs/deployment.md` to retire architecture 15: profiles, backup and restore commands, recovery generation, expand-and-contract.
- [ ] Multi-node with libcluster; rerun the partition tests across real nodes.
- [ ] Push relay, notification extension with the single-writer lock, UnifiedPush, Web Push.
- [ ] SwiftUI, then Compose, each on the same core.

## Open questions carried forward

From architecture §20, plus ones raised while specifying:

- Numbers in section 17 after the gateway spike.
- MLS ciphersuite and key-package cadence.
- OpenMLS on wasm32 with WebCrypto key storage, or an adjusted web trust statement.
- Tauri core in-process versus WASM.
- Link unfurling.
- Reconciliation and bounded-staleness intervals, after measurement.
- Whether `commands` retention of 30 days is acceptable for non-message operations (delivery §9 states the consequence).
