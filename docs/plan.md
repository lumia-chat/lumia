# Plan

The working checklist for building Lumia. Section 18 of [architecture.md](architecture.md) gives the order and why; this file tracks what is done, what is next, and what changed along the way. Update it when work lands, not after.

Legend: `[x]` done · `[~]` in progress · `[ ]` not started · `(spec §n)` the section a task implements · `(tests)` the failure-injection tests that close it.

## Status at a glance

| Step | State |
|---|---|
| 1. Schemas and protocol contract | Done |
| 2. Server: event log, delivery, account log, resume, authorization | Event log, account log, authorization, sessions, and the gateway's first pass done; HTTP and identity endpoints next |
| 3. Rust core | Not started |
| 4. Identity across two instances | Specified, not built |
| 5. MLS personal DMs, backup, recovery | Not specified, not built |
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

**Gateway** (first pass done 2026-11-01)

- [x] `LumiaWeb.Gateway.Serializer`: raw `ClientFrame` in, bare `ServerFrame` out, `Hello` as join, event bytes wrapped without re-encoding; encoding chosen by the first frame's opcode (delivery §5; tests: serializer unit tests).
- [x] `LumiaWeb.Gateway.SessionChannel`: subscription state, the four-line delivery rule, authoritative resolve on subscribe and on `acl_version` change, membership-driven unsubscribe (delivery §5; tests: N+2 before N+1 in order, duplicates dropped, kicked member receives nothing, acl change caught).
- [x] Encode-once broadcast from `Lumia.Events` and `Lumia.Accounts` after commit (delivery §4).
- [x] One replay loop for subscribe, live gaps and nudges; 1,000-event cap answers `SNAPSHOT_REQUIRED` (delivery §6; tests: cap, gap fill).
- [x] `LumiaWeb.Gateway.Reconciler`: 5 s tick with 1 s overlap, node-local head broadcasts for channels and accounts (delivery §7; test: nudge fills a suppressed broadcast).
- [x] Account-log subscription at join, `ResumeAccount`, live account events (delivery §5).
- [x] Session revocation closes the socket (test).
- [x] Heartbeat echo, `Welcome` with recovery generation and limits.
- [ ] Slow-consumer disconnect is implemented against the transport mailbox but has no test yet (delivery §5).
- [ ] Bounded staleness (10 min re-resolve) and partition-heal re-resolve (delivery §5).
- [ ] Presence scoped per server plus the friends topic (architecture §3); `PresenceUpdate` and `Typing` are accepted and ignored for now.
- [ ] `Authorization` header on the upgrade (needs a custom upgrade plug); v1 uses `?access_token=`.
- [ ] JSON sessions use stock ProtoJSON; the protocol's JSON overrides (UUID strings, int64 strings) are not applied yet.

**HTTP**

- [ ] `GET /instance`, `GET /account/snapshot`, `GET /account/events` (protocol §9).
- [ ] `GET /channels/{id}/events`, `/snapshot`, `/messages`, `POST /channels/{id}/commands` with `truncated_before` and snapshot consistency (delivery §8; test: snapshot during writes).
- [ ] Servers, channels, members, roles endpoints with authoritative checks and `NOT_FOUND` for non-members (authorization §5).
- [ ] Error envelope, pagination, token issuance and refresh, session revocation closing sockets (protocol §10 to §12).
- [ ] Purge and redaction: `MessageDeleted`, retention job, `Redacted` at read, `PayloadsPurged` (delivery §10; tests: tombstones on resume, live purge event).

**Identity on the server** (needed before step 4 can be exercised against two instances)

- [ ] Roster verification, all eight steps, with `details.step` on rejection (identity §3.3; tests: the certificate and monotonicity cases).
- [ ] Challenge issuance with HMAC, `used_challenges` replay table (identity §9; test: replay on a different node).
- [ ] `POST /auth/reauth`, revoke endpoint, revocation policy (identity §4.4, §10; test: thief without reauth is rejected everywhere).
- [ ] Link bundles with codes, single use, attempt cap (identity §6; tests: second fetch is `NOT_FOUND`, five wrong attempts delete).
- [ ] Identity backup put and get (identity §8).
- [ ] Identity-proof sign-in and account creation gated by instance policy (identity §4.6, architecture §6.3).

**Closing step 2**

- [ ] Every test in delivery §12, authorization §11, and identity §12 green in CI.
- [ ] Rerun the event-log spike with the gateway attached and record send-to-delivered numbers in [spikes/](spikes/).
- [ ] Section 17 budgets updated with measured values.

## Step 3: Rust core

- [ ] Crate layout under `apps/core`: protocol (generated from `proto/`), store (SQLite), sync, identity, mls, with UniFFI and WASM bindings.
- [ ] Protocol encode and decode against the gateway, including the JSON overrides for debugging.
- [ ] SQLite store with per-log cursors persisted in the same transaction as applied state (protocol §5).
- [ ] Sync engine: the three resume outcomes, gap fetch, dedup by event ID, high-water-mark on foreground (delivery §8).
- [ ] Identity: phrase, derivation, roster build and verify, link request and bundle (identity §2 to §6).
- [ ] A headless test client that drives the gateway for the failure-injection tests.

## Step 4: identity across two instances

- [ ] Two instances in `docker-compose`, one flagship-like with a directory.
- [ ] Create identity, add a second device by link bundle, revoke it, recover from the phrase, all against both instances.
- [ ] Every test in identity §12 run end to end with the Rust core as the client.

## Step 5: MLS personal DMs, backup, recovery

- [ ] Write `docs/specs/e2ee.md` to retire architecture 11.3, after OpenMLS is linked and its provider traits are understood (open question in architecture §20).
- [ ] Delivery-service endpoints: key packages, GroupInfo, epoch-ordered commits.
- [ ] Personal DM send and receive, history chunks and manifests, recovery with external-commit rejoin.

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
