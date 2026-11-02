# Permissions, Authorization, and Revocation

Status: accepted, 2026-11-01. This specification makes section 7 of [architecture.md](../architecture.md) implementable, including the provisional 7.4. Wire shapes are in `proto/lumia/v1/chat.proto` (`Permission`, `Role`, `Overwrite`, `Member`); the session delivery rule it relies on is in [specs/delivery.md](delivery.md) section 5. Where this document and the architecture disagree, the architecture wins and this document is wrong.

## 1. Model

```text
Server       has roles, members, channels
Role         a named permission set with a position; one per server is @everyone
Member       an account's membership in a server: kind (member | guest), roles
Overwrite    per channel, per role or per member: allow set, deny set
Resolution   pure function (ACL snapshot, member) → permission set
acl_version  per channel; bumped by definition-level changes
Account log  carries member-level changes to the affected member
```

Two invariants:

- **Caches are advisory; Postgres is authoritative.** No write and no HTTP read relies on a cached decision.
- **Two propagation paths.** Definition-level changes propagate through `acl_version`. Member-level changes propagate through the affected member's account log. Both are at-least-once (delivery spec sections 5 and 7).

## 2. Storage

| Table | Columns | Notes |
|---|---|---|
| `roles` | `id`, `server_id`, `name`, `position`, `permissions` bigint, `color`, `is_everyone` | Bit index of `permissions` equals the proto `Permission` enum value. Exactly one `is_everyone` role per server, created with the server, undeletable. |
| `member_roles` | `server_id`, `account_id`, `role_id` | Primary key on all three. `@everyone` is implicit and never stored. |
| `channel_overwrites` | `channel_id`, `target_kind` (`role` \| `member`), `target_id`, `allow` bigint, `deny` bigint | Primary key `(channel_id, target_kind, target_id)`. Normalized on write: a bit present in `deny` is cleared from `allow`, so the two never overlap and the algorithm's same-layer rule (3.1) is unambiguous. |
| `server_members` | `server_id`, `account_id`, `kind`, `nickname`, `joined_at`, `expires_at` | `kind` is `member` or `guest` (architecture 6.2). |
| `servers` | …, `owner_account_id` | |
| `accounts` | …, `instance_role` (`none` \| `moderator` \| `admin` \| `owner`) | Section 7. |
| `audit_log` | `id`, `scope_kind` (`server` \| `instance`), `scope_id`, `actor_account_id`, `action`, `target_kind`, `target_id`, `before` jsonb, `after` jsonb, `override` bool, `created_at` | Section 9. |

`Lumia.Authz` converts between bitfields and the named `Permission` enum at the API boundary. Bitfields never appear in a request or response.

## 3. Resolution

### 3.1 The algorithm

`resolve(snapshot, member) → set of Permission`, where `snapshot` is the channel's ACL snapshot (section 4) and `member` carries `account_id`, `kind`, `role_ids`, and `is_owner`.

```text
1. if member.is_owner                          → ALL
2. if member.kind == guest                     → base = ∅
   else                                        → base = ∪ permissions(role) for role in {@everyone} ∪ member.role_ids
3. if ADMINISTRATOR ∈ base                     → ALL
4. ow = overwrite for @everyone (if any)       → base = (base − ow.deny) ∪ ow.allow      [guests: skipped]
5. role_ows = overwrites for member.role_ids   → base = (base − ∪ deny) ∪ (∪ allow)
6. member_ow = overwrite for member.account_id → base = (base − ow.deny) ∪ ow.allow
7. return base
```

Within a layer, deny is applied before allow, so an allow in the same layer wins over a deny in the same layer. A later layer overrides an earlier one. Guests skip step 4 entirely: they never consult `@everyone`, neither its role permissions nor its overwrite.

`ALL` means every defined `Permission`. It still does not grant anything outside the server: instance limits, instance policy, and E2EE are untouched.

### 3.2 Reference implementation

The spec's reference is a straightforward Elixir function over `MapSet`s, kept in `test/support/authz_reference.ex`. The production implementation over bitfields is property-tested against it (section 11).

### 3.3 Derived checks

| Check | Rule |
|---|---|
| Can view channel | `VIEW_CHANNEL ∈ resolve` |
| Channel is private | The `@everyone` overwrite denies `VIEW_CHANNEL` |
| Can manage a role R | `MANAGE_ROLES ∈ resolve(server-level)` and `position(R) > position(actor's highest role)` (lower position sorts first and is higher), or actor is owner |
| Can grant permission P on a role or overwrite | Actor holds P, or is owner or Administrator |
| Can kick or ban member M | `MANAGE_MEMBERS` and M is not the owner and M's highest role is below actor's |
| Can change ownership | Actor is owner, with reauth (identity spec section 10) |

The owner cannot be kicked, banned, or stripped of roles by anyone.

## 4. ACL snapshots and the cache

An **ACL snapshot** for a channel is: the server's roles with their bitfields and positions, and the channel's overwrites. It is a pure function of the database state at a given `acl_version`.

Per node, an ETS table `authz_snapshots` keyed `(channel_id, acl_version)` caches snapshots. Because the version is in the key, a snapshot is immutable once loaded and needs no invalidation. Entries older than 10 minutes are evicted by a sweeper; a miss loads from Postgres in one query per table.

Resolving for a session subscription is one ETS lookup plus bit operations. Resolving for an HTTP request is the same after reading the channel row.

## 5. Where checks happen

**Writes and HTTP reads.** The request reads the channel row from Postgres (it needs `last_seq` or the channel anyway), which yields the authoritative `acl_version`; reads the member's `server_members` row and `member_roles`; resolves against the snapshot for that version; and checks the required permission. A missing membership is `NOT_FOUND`, never `PERMISSION_DENIED`, so channel existence is not disclosed.

**Live delivery.** The session holds, per subscription, `acl_version` and the resolved set, as the delivery spec describes. It re-resolves:

- when an event arrives with a different `acl_version`;
- when its account log delivers `MembershipUpdated` (re-resolve every subscription in that server), `MembershipRemoved` or `ChannelRemoved` (unsubscribe);
- after the node rejoins the cluster following a partition (every subscription);
- by **bounded staleness**: a subscription not re-resolved in the last 10 minutes is re-resolved before its next delivery.

The partition case uses the staleness rule rather than an immediate pass over every subscription: on `nodeup` the session marks each subscription stale, so each is re-resolved before its next delivery and nothing is delivered on a pre-partition view, without every session on the node querying Postgres in the same instant. Re-resolution happens once per event: if the event's `acl_version` is older than the version just resolved, the event is delivered under the fresh view rather than re-resolved again.

If re-resolution loses `VIEW_CHANNEL`, the session unsubscribes and pushes nothing.

**Stated failure mode.** Between a member-level removal committing and the account-log event reaching a session, that session can hold a subscription for at most one reconciler tick plus the staleness bound. During that window no event committed after the removal can be delivered without a successful re-resolution, because any such event either carries a new `acl_version` (definition-level) or arrives after the account-log nudge (member-level, within one tick). The bound is therefore 5 seconds of a subscription existing, not 5 seconds of leakage.

## 6. Propagation

### 6.1 Definition-level changes bump `acl_version`

| Change | Channels bumped |
|---|---|
| Role permissions, position, or deletion | Every channel in the server |
| Channel overwrites added, changed, or removed | That channel |
| Channel created, deleted, kind changed | That channel (and `ChannelAdded`/`ChannelRemoved` on affected account logs via the diff job) |
| Ownership transferred | Every channel in the server |

The bump is `UPDATE channels SET acl_version = acl_version + 1 WHERE ...` in the same transaction as the change, followed by an `AclChanged` event appended to each bumped channel's log so subscribed sessions re-resolve on the next event even if the channel is otherwise quiet. For a server-wide bump the `AclChanged` events are appended by an Oban job in batches of 100 channels to keep the admin's transaction short; the `acl_version` update itself is synchronous.

### 6.2 Member-level changes go through the account log

| Change | Account-log event, to whom |
|---|---|
| Roles assigned or removed | `MembershipUpdated` to that member |
| Kick, ban, leave, guest expiry, account deletion | `MembershipRemoved` to that member, plus `ChannelRemoved` for each channel they could view |
| Join, guest added, guest promoted | `MembershipAdded` to that member, plus `ChannelAdded` for each channel they can view |

No `acl_version` changes. The member's sessions act on receipt; the reconciler's account-log nudge catches a missed delivery within one tick.

### 6.3 Viewer diff job

When a definition-level change can alter who may view a channel, an Oban job (debounced 30 seconds per server so a burst of admin edits runs once) recomputes the set of viewers for each affected channel, diffs it against the previous set, and appends `ChannelAdded` or `ChannelRemoved` to the account logs of members who gained or lost view. Discovery is therefore eventually consistent in the background; delivery is stopped immediately by `acl_version`.

## 7. Instance administration

`accounts.instance_role`:

| Role | May |
|---|---|
| `owner` | Everything below, plus change instance roles. Exactly one. |
| `admin` | Instance settings, suspend and erase accounts, retention, reports, overrides. |
| `moderator` | Reports, suspend accounts. |
| `none` | Nothing instance-level. |

Instance roles grant **no server permissions**. An instance admin who is not a member of a server cannot read it. Acting inside a server regardless of membership goes through the override endpoints under `/api/v1/instance/override/...`, which require `admin` or `owner`, are rate-limited, and write an `audit_log` row with `override = true`. Members of the server can see override rows in their server's audit log when that feature ships.

## 8. Bots

A bot's effective permissions in a channel are `resolve(snapshot, bot_member) ∩ scopes(token)`. Scopes are the `Permission` names plus bot-only scopes:

| Scope | Grants |
|---|---|
| `identify` | Read own account |
| `webhooks:write` | Manage webhooks the bot created |
| `commands:register` | Register slash commands and components |

A token with no scopes can do nothing regardless of roles. Scopes never add a permission the roles do not resolve.

## 9. Audit log

Every change to roles, overwrites, memberships, ownership, instance roles, and every override writes an `audit_log` row in the same transaction: scope, actor, action, target, `before` and `after` as JSON, and the override flag. The rows exist from the first release so that the audit-log viewing feature, which the architecture defers, needs no backfill. Rows are retained for the instance's retention window for audit data (default two years, operator-configurable).

## 10. Limits

Operator hard limits (architecture 16), instance-configurable:

| Limit | Default |
|---|---|
| Roles per server | 50 |
| Overwrites per channel | 50 |
| Roles per member | 20 |

System constants:

| Constant | Value |
|---|---|
| Bounded-staleness interval | 10 minutes |
| Viewer-diff debounce | 30 seconds per server |
| Snapshot cache TTL | 10 minutes |
| `AclChanged` batch size for server-wide bumps | 100 channels per job |

## 11. Tests

Property tests of the bitfield implementation against the `MapSet` reference for random snapshots and members, asserting: deny beats allow within a layer; a later layer overrides an earlier one; Administrator ignores overwrites; a guest never gains anything from `@everyone`; the owner always resolves `ALL`.

Failure-injection and behavior tests:

```text
Change a role's permissions; verify every channel in the server bumped acl_version
  by one and no channel in another server changed.
Assign a role to a member; verify no acl_version changed and MembershipUpdated
  appeared on that member's account log only.
Kick a member on node A while node B holds a session with cached permissions;
  verify B delivers nothing committed after the kick and unsubscribes within one
  reconciler tick.
Partition node B, change overwrites on A, heal; verify B re-resolves every
  subscription before delivering.
An Administrator attempts to kick the owner; verify PERMISSION_DENIED.
A member with MANAGE_ROLES attempts to edit a role above their highest; verify
  PERMISSION_DENIED. Attempts to grant a permission they lack; verify
  PERMISSION_DENIED.
A bot holding an Administrator role with a token scoped to VIEW_CHANNEL only
  attempts to send; verify PERMISSION_DENIED.
An instance admin with no membership reads a private channel; verify NOT_FOUND.
  Through the override endpoint; verify success and an audit row with override.
Make a channel private; verify non-viewers' listings omit it and ChannelRemoved
  reaches their account logs within the debounce window.
Resolve for a channel whose acl_version is not cached; verify one Postgres load
  and subsequent hits from ETS.
```
