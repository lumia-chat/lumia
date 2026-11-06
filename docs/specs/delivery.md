# Delivery and Sync

Status: accepted, 2026-11-01. This specification makes section 8 of [architecture.md](../architecture.md) implementable. Wire shapes are in `proto/lumia/v1/events.proto`, `commands.proto`, and `gateway.proto`; endpoint mappings are in [protocol.md](../protocol.md). The write path it describes exists in `apps/instance` as `Lumia.Events` and the migration `create_event_log`, measured in [spikes/2026-11-01-event-log.md](../spikes/2026-11-01-event-log.md). Where this document and the architecture disagree, the architecture wins and this document is wrong.

## 1. The guarantee

*A successful acknowledgment means the event and its immediately queryable state have committed. Live delivery is at-least-once and recoverable, never exactly-once. Clients deduplicate by event ID and apply events in `seq` order per log.*

Everything below exists to make that sentence true under process death, node loss, partition, slow clients, and restores from backup.

## 2. Logs and tables

Two kinds of log, one mechanism.

**Channel logs.** One per `Channel` of any kind. Tables as built:

| Table | Key | Holds |
|---|---|---|
| `channels` | `id` | `last_seq`, `last_seq_at` (second granularity), `acl_version` |
| `events` | `(channel_id, seq)` | `id`, `kind`, actor, `command_id`, `acl_version`, `created_at`. Permanent. |
| `event_payloads` | `(channel_id, seq)` | `payload`, nullable. Nulled by deletion and retention. |
| `commands` | `(channel_id, command_id)` | `fingerprint`, `seq`, `event_id`. Purged after 30 days. |
| `messages` | `id` = the create command's ID | Materialized view of message events. |

`events`, `event_payloads`, and `commands` are hash-partitioned by `channel_id` into 32 partitions, so the sync query `(channel_id, seq > $n)` and the idempotency lookup `(channel_id, command_id)` each prune to one partition.

**The account log.** One per account per instance, carrying `AccountEvent`: memberships, channels gained and lost, requests, friendships, roster updates, profile changes, session revocations, block-list updates. Tables `account_events` and `account_event_payloads`, hash-partitioned by `account_id` the same way, with `last_seq` and `last_seq_at` on `accounts` under the same HOT rules as `channels`.

**Instance settings.** A single-row `instance_settings` table holds `recovery_generation`. Only `lumia restore` and `lumia settings bump-generation` change it. It is read at boot into `persistent_term` and sent in `Welcome` and `InstanceInfo`.

**The HOT rule.** `channels` and `accounts` are updated on every write to their logs. They use `fillfactor = 70`, and `last_seq_at` is stored at second granularity so that writes within the same second do not change an indexed column and the update stays HOT. Never index a column of these rows that changes on every write.

## 3. The write path

`Lumia.Events.append/2`, one transaction per command:

1. Look up `(channel_id, command_id)` in `commands`. Found with the same fingerprint: return the stored ack, write nothing. Found with a different fingerprint: `CONFLICT`.
2. `UPDATE channels SET last_seq = last_seq + 1, last_seq_at = date_trunc_second(now()) WHERE id = $1 RETURNING last_seq, acl_version`. This takes the row lock for the rest of the transaction; everything after it is serialized per channel.
3. Insert the `events` row with a server-minted monotonic UUIDv7 `id`, the returned `seq` and `acl_version`, and `created_at = now()`.
4. Insert the `event_payloads` row.
5. Materialize: for `message_created`, insert `messages` with `id = command_id`; for edits, deletes, reactions, pins, update the message row.
6. Insert the `commands` row with the fingerprint and the ack fields.
7. Commit.
8. **Only now**, broadcast (section 4).

The fingerprint is SHA-256 over the deterministic term encoding of `(kind, actor, payload)`. For `SendEncrypted`, the payload is the ciphertext bytes, so a retry must resend identical bytes; a client that re-encrypts after an epoch change uses a new command ID.

A concurrent identical command loses the race on the `commands` primary key, which rolls back its whole transaction including the `seq` it took; the caller then re-reads and returns the winner's ack. `seq` is therefore dense.

**Consequence.** A single channel serializes on one row. On the spike hardware a commit takes under 1 ms, so a channel sustains a few thousand writes per second and 50 concurrent writers see roughly 50 commits of queueing. No real channel has 50 concurrent writers.

## 4. Broadcast

After commit, the committing process encodes the `Event` to protobuf once and broadcasts `{event, protobuf_binary}` on `Phoenix.PubSub` topic `channel:<channel_id>` (or `account:<account_id>` for account events). Protobuf sessions forward the binary untouched. JSON sessions encode from the struct. The sender's own sessions receive and deliver the event like any other; the client deduplicates by `event_id` and matches it to its pending command by `command_id`.

Broadcast is at-most-once. If the process dies between commit and broadcast, or a node is partitioned, the event is in Postgres and not on any socket. Sections 6 and 7 close that gap.

## 5. The session process

One Phoenix Channel per WebSocket on topic `session`, with a custom `Phoenix.Socket.Serializer`:

- A raw binary (or text) frame decodes directly to a `ClientFrame`. The first frame must be `Hello`; the serializer presents it as the channel join. Any other frame first is a protocol error.
- `Welcome` is the join reply. Every other outbound frame is a push encoded directly as a bare `ServerFrame`. No Phoenix envelope reaches the wire; refs stay internal.
- Subprotocol `lumia.v1.proto` selects protobuf, `lumia.v1.json` selects JSON. Anything else fails the upgrade with `UNSUPPORTED_ENCODING`.

Session state:

```text
account_id, device_id, session_id
recovery_generation sent in Welcome
subscriptions: channel_id → %{delivered_seq, acl_version, permissions}
account_log: %{delivered_seq}
outbound queue length and bytes
```

On join the session subscribes to `account:<account_id>` and resumes the account log when the client sends `ResumeAccount`. It subscribes to `channel:<id>` topics as the client sends `Subscribe`, up to `Limits.max_subscriptions`.

**Delivery rule**, for a broadcast `{event, binary}` on a subscribed channel:

```text
if event.seq <= delivered_seq                → drop (duplicate)
if event.acl_version != cached acl_version  → re-resolve permissions from Postgres;
                                              if VIEW_CHANNEL is gone, unsubscribe and
                                              push nothing; else cache the new version
if event.seq == delivered_seq + 1           → push; delivered_seq = event.seq
if event.seq >  delivered_seq + 1           → run the gap loop (section 6), then push
```

The same `acl_version` and staleness check applies to a page read from Postgres, whether for the gap loop, a subscribe replay or a reconciliation nudge: a page carrying an `acl_version` newer than the cached one, or a subscription past the staleness window (authorization section 5), is re-resolved once before anything from the page is pushed. This is the path by which a change committed on another node during a partition takes effect after the heal, since its broadcast never arrived and reconciliation brings only the head.

Events from one session are therefore always in `seq` order per channel, which is what protocol section 5 promises clients.

**Removal.** When the session's account log delivers `ChannelRemoved` or `MembershipRemoved`, the session unsubscribes the affected channels immediately. The `acl_version` check above is the backstop for a missed account-log message. After the node rejoins the cluster following a partition, the session re-resolves permissions for every subscription before delivering anything.

**Slow consumers.** The session counts frames and bytes handed to the transport but not yet written. Past 1,000 frames or 8 MB it pushes `Disconnect{DISCONNECT_SLOW_CONSUMER}` and closes. The client reconnects and resumes from its cursors.

## 6. Replay and gap fill: one loop

```text
loop(channel):
  page = events_after(channel, delivered_seq, 200)
  push each event in page; delivered_seq = last seq in page
  if length(page) == 200, loop again
```

Used in three places:

- **Subscribe** with cursor C: set `delivered_seq = C`, subscribe the PubSub topic, answer `RESUMED` with the current head, run the loop. Live broadcasts that arrive during the loop are duplicates (dropped) or extend the gap (same loop). Subscribing takes `READ_HISTORY` as well as `VIEW_CHANNEL`, the same check as `GET /channels/{id}/events`; a member with `VIEW_CHANNEL` alone is answered `ERROR` with `PERMISSION_DENIED`. Live-only delivery for such a member (events while connected, no replay) is deferred: it needs a client rule for the gap it would see, and nothing grants `VIEW_CHANNEL` without `READ_HISTORY` yet.
- **Live gap**: a broadcast with `seq > delivered_seq + 1` runs the loop before pushing the broadcast's event (which is then a duplicate and dropped, having been delivered by the loop).
- **Reconciliation nudge** (section 7): a `{:head, seq}` with `seq > delivered_seq` runs the loop.

**Replay cap.** At subscribe time, if `head - C > 1000`, answer `SNAPSHOT_REQUIRED` with the head instead of replaying. If a live gap exceeds 1,000, push a `Nudge{heads: [cursor]}` instead of filling, and the client pulls over HTTP with `GET /channels/{id}/events`. "Cursor no longer served" means exactly this: event rows are permanent, but a replay larger than 1,000 events is not streamed through a session.

## 7. Reconciliation

One `Lumia.Gateway.Reconciler` GenServer per node. Every 5 seconds it runs:

```sql
SELECT id, last_seq FROM channels WHERE last_seq_at >= $tick_minus_1s;
SELECT id, last_seq FROM accounts WHERE last_seq_at >= $tick_minus_1s;
```

and for each row does a **node-local** broadcast of `{:head, last_seq}` on `channel:<id>` or `account:<id>`. Sessions on this node that are subscribed compare with `delivered_seq` and run the loop if behind. Topics with no local subscribers cost one ETS lookup. The query is proportional to instance write rate, not to subscriptions, and the 1-second overlap plus `>=` on second-granularity timestamps means a channel written in the same second as a tick is nudged again, which is harmless.

This closes the commit-then-die gap within one tick, closes partition losses on rejoin, and needs no outbox table or dispatcher.

**Client high-water mark.** On foreground or regained connectivity the client sends `HighWaterMarkRequest` for all its channels; the session answers from `channels.last_seq` in one query. This covers the case where the client, not the server, was the one that missed things.

## 8. Resume outcomes

Three, and the client must implement all three:

| Outcome | Trigger | Client action |
|---|---|---|
| Replay | `RESUMED` | Apply events as they arrive. |
| Snapshot | `SNAPSHOT_REQUIRED`, or `truncated_before` on an HTTP events fetch | `GET /channels/{id}/snapshot`, apply, set cursor to `snapshot.seq`, fetch events after it. |
| Full reconcile | `Welcome.recovery_generation` differs from the stored one | Discard every cursor for this instance, store the new generation, take the snapshot path for the account log and then for every channel. |

**Snapshot consistency.** The snapshot endpoint reads `channels.last_seq`, then the newest 50 messages and all pins with `seq <= last_seq`, and returns that `seq`. A message committed between the two reads has a higher `seq` and is excluded, so the snapshot is exactly the state at `seq` without a serializable transaction.

**Account log first.** A client always resumes the account log before subscribing to channels, because the account log is how it learns which channels exist. First connection and generation change both start with `GET /account/snapshot`.

## 9. Idempotency

Rules, applied in `append/2` step 1 and identically by `POST /channels/{id}/commands`:

```text
Same ID, same actor, same channel, same operation, same payload → the original ack
Same ID, any difference                                        → CONFLICT
Same ID on content the actor cannot access                     → NOT_FOUND
```

`commands` rows are purged after 30 days by Oban. A retry older than that is a client bug. For a message send it still cannot duplicate: `messages.id` is the command ID and the insert fails, which surfaces as `CONFLICT`, not a second message. For other operations a stale retry may apply twice (two identical reaction adds collapse; two edits with the same body are idempotent by content), which is acceptable at that age.

## 10. Purge and redaction

**User deletion.** `DeleteMessage` appends a `MessageDeleted{purge_cache: true}` event, nulls the original create event's payload, and sets `messages.deleted_at`. Content stays available to moderators through the edit-history endpoint until the retention window (30 days, instance-configurable) passes, then the history rows are deleted too.

**Retention.** An Oban job per channel with a retention policy nulls `event_payloads.payload` for events older than the policy, deletes the corresponding `messages` bodies and edit history, drops attachment references (blobs follow by garbage collection), removes search index entries, and appends one `PayloadsPurged{before_seq}` event so online clients drop cached bodies below it.

**Reading a redacted event.** At read time, an event whose payload is null is returned as `Redacted{was}`, so a client resuming across a purge sees tombstones with their `seq` intact and ordering and gap detection are unaffected.

**Personal DMs.** The server nulls ciphertext on the same schedule when a participant deletes their account; it cannot do content-based retention.

## 11. Timings and limits

One place to tune them.

| Parameter | Value |
|---|---|
| `Hello` deadline after upgrade | 5 s |
| Heartbeat interval | 30 s; disconnect after 2 missed |
| Reconciler tick | 5 s, with a 1 s overlap (measured: about 2 µs per channel moved in the window, [spikes/2026-11-06-load.md](../spikes/2026-11-06-load.md)) |
| Replay page | 200 events |
| Replay cap at subscribe | 1,000 events, else `SNAPSHOT_REQUIRED` |
| Live gap cap | 1,000 events, else `Nudge` |
| Snapshot messages | 50 newest |
| Slow consumer | 1,000 frames or 8 MB queued |
| `commands` retention | 30 days |
| Deleted content retention | 30 days, instance-configurable |
| `max_subscriptions` | 500 per session, in `Welcome.limits` |

## 12. Tests

Failure-injection tests that must pass before this area is considered implemented:

```text
Commit an event, kill the session process before broadcast; verify every other
  subscriber receives it within one reconciler tick.
Deliver broadcasts for seq N+2 and then N+1 to a session holding N; verify the
  client receives N+1 then N+2 and nothing twice.
Remove a member on node A while node B's session caches the old permissions;
  verify B re-resolves on the next event's acl_version and delivers nothing
  committed after the removal.
Partition node B, commit events on A, heal; verify B's sessions re-resolve
  permissions and receive the events via reconciliation.
Subscribe with a cursor 1,001 behind the head; verify SNAPSHOT_REQUIRED, and
  with 1,000 behind, verify replay.
Fill a session's outbound queue past the limit; verify DISCONNECT_SLOW_CONSUMER
  and a clean resume from the cursors.
Bump the recovery generation; verify the client discards cursors and takes the
  snapshot path for the account log and every channel.
Retry a command whose commands row was purged; verify CONFLICT and exactly one
  message.
Send the same command concurrently from two connections; verify one event, one
  seq, and both callers receive the same ack.
Apply retention to a channel; verify resumed clients see Redacted tombstones at
  the original seqs and online clients receive PayloadsPurged.
Take a snapshot while writes are in flight; verify every message in it has
  seq <= snapshot.seq and every event after snapshot.seq is fetched exactly once.
Send a client frame before Hello; verify DISCONNECT_PROTOCOL_ERROR.
```
