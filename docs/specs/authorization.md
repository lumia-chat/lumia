# Permissions, Authorization, and Revocation

Status: accepted, 2026-11-01. This specification makes section 7 of [architecture.md](../architecture.md) implementable, including the provisional 7.4. Wire shapes are in `proto/lumia/v1/chat.proto` (`Permission`, `Role`, `Overwrite`, `Member`); the session delivery rule it relies on is in [specs/delivery.md](delivery.md) section 5. Amended 2026-11-18 for architecture 7.5, roles and titles on the clients: sections 3.5 and 6.4 are new, and 2, 3.3, 6.2, 9, 10 and 11 gained from it. Where this document and the architecture disagree, the architecture wins and this document is wrong.

## 1. Model

```text
Server       has roles, members, channels
Role         a named permission set with a place in the server's order, a colour
             and a tag switch; one per server is @everyone
Member       an account's membership in a server: kind (member | guest), roles,
             a title
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
| `roles` | `id`, `server_id`, `name`, `position`, `permissions` bigint, `color`, `hide_tag`, `is_everyone` | Bit index of `permissions` equals the proto `Permission` enum value. Exactly one `is_everyone` role per server, created with the server, undeletable. `position` is the instance's to set and one role's alone in its server; `name` is unique in its server ignoring case; `color` is a tone's name or null (section 3.5). |
| `member_roles` | `server_id`, `account_id`, `role_id` | Primary key on all three. `@everyone` is implicit and never stored. |
| `channel_overwrites` | `channel_id`, `target_kind` (`role` \| `member`), `target_id`, `allow` bigint, `deny` bigint | Primary key `(channel_id, target_kind, target_id)`. Normalized on write: a bit present in `deny` is cleared from `allow`, so the two never overlap and the algorithm's same-layer rule (3.1) is unambiguous. |
| `server_members` | `server_id`, `account_id`, `kind`, `nickname`, `title`, `joined_at`, `expires_at`, `channel_adds_by_invitation`, `server_dms_off` | `kind` is `member` or `guest` (architecture 6.2). The two switches are the member's own (architecture 6.8). `title` is the member's title in this server or null (section 3.5); `nickname` is kept and not yet used. |
| `member_announcements` | `id`, `server_id`, `account_id` | A member whose roles or title changed and whom the server's other members have not been told of yet: a row for each change, written in the change's transaction (section 6.4). |
| `channel_members` | `channel_id`, `account_id`, `added_by_account_id`, `joined_at` | Who is in a channel (architecture 6.8, section 3.4); a server DM's members are its participants. |
| `channel_invitations` | `channel_id`, `account_id`, `invited_by_account_id`, `created_at` | An add waiting for its member's yes. |
| `channel_viewers` | `channel_id`, `account_id` | Who the channel is shown to (in it and able to see it), as last announced on account logs (section 6.3). |
| `servers` | …, `owner_account_id`, `member_tag` | `member_tag` is `role` or `title`: what the tag beside a member's name shows (architecture 7.5). |
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
| Channel is private | Made private at creation (`channels.private`), which writes the `@everyone` overwrite that denies `VIEW_CHANNEL` |
| Can manage a role R (change, delete, give or take away) | `MANAGE_ROLES ∈ resolve(server-level)` and `position(R) > position(actor's highest role)` (lower position sorts first and is higher), or actor is owner. `@everyone` is changed by anyone with `MANAGE_ROLES`, its permissions only |
| Can create a role | `MANAGE_ROLES`, or actor is owner. It is placed last, below every role the actor holds |
| Can reorder the roles | `MANAGE_ROLES`, and every role at or above the actor's highest keeps its place, or actor is owner (section 3.5) |
| Can grant permission P on a role or overwrite | Actor holds P, or is owner or Administrator |
| Can kick or ban member M | `MANAGE_MEMBERS` and M is not the owner and M's highest role is below actor's |
| Can set member M's title | M is the actor; or the actor could kick M |
| Can change ownership | Actor is owner, with reauth (identity spec section 10) |

The owner cannot be kicked, banned, or stripped of roles by anyone.

### 3.4 Channel membership

Membership sits on top of resolution (architecture 6.8), in `Lumia.Authz.Access.permissions/4`, the one place the rule is written:

- Editing a message is its author's alone (with `SEND_MESSAGES`); deleting one is its author's or a `MANAGE_MESSAGES` holder's (with `VIEW_CHANNEL`, so a moderator who is not in the channel may still delete there). The instance checks the message's author on both, and refuses a notice as not found.
- In a server's text channel, an account not in it holds none of `SEND_MESSAGES`, `SEND_IN_THREADS`, `ATTACH_FILES`, `ADD_REACTIONS` and `START_CALLS`, whatever resolution gave; it keeps the rest, so a public channel can be read before joining and a moderator keeps `MANAGE_MESSAGES`.
- In a private channel, being in it adds `VIEW_CHANNEL` for a `member` (not a `guest`): a private channel's people need no overwrite each, so its size is not bound by the overwrite limit (section 10). A guest sees what overwrites grant it, here as anywhere.
- A `SERVER_DM` is its members' alone: each holds `VIEW_CHANNEL`, `READ_HISTORY`, `SEND_MESSAGES`, `SEND_IN_THREADS`, `ATTACH_FILES`, `ADD_REACTIONS` and `START_CALLS`, without the writes while either has blocked the other, and anyone else, an administrator included, gets `NOT_FOUND`.

Who may join: a `member` or `guest` of the server, into a public text channel it can view, or into any it holds an invitation to. Who may add: anyone in the channel, or holding `MANAGE_CHANNELS`, adding members of the server who could view it once in it. Who may remove someone else: `MANAGE_CHANNELS`.

### 3.5 Roles and titles: what a write is held to

The rules of architecture 7.5, as the instance enforces them. Each is checked in the change's transaction, with the server's row locked so two changes to one server's roles run one after the other.

**The order.** No request carries a position: `Role.position` is the instance's answer and is ignored when sent. `@everyone` is last and stays there. A created role takes the place after the last role that is not `@everyone`. A reorder (`PUT /servers/{server_id}/role-order`) names every role of the server but `@everyone`, each once, in the order wanted; anything else is `INVALID_ARGUMENT` with `details.field` `role_ids`. For an actor who is not the owner the new order must begin with the same roles in the same places as the old one, down to and including the actor's highest role: nothing the actor may not manage moves, and nothing is lifted to or above the actor's own. An actor holding no role is above nothing and may reorder nothing. The instance then numbers the roles 1, 2, 3 and on in the order given, so no two share a position. A reorder that changes nothing is answered and nothing is written.

**A role.** Its name is 1 to 32 characters once trimmed, with no line break, it is not `everyone` or `@everyone`, and no other role of the server has it, compared without case: `INVALID_ARGUMENT` with `details.field` `name`, or `ALREADY_EXISTS`. Its colour is empty or one of `peach`, `blue`, `purple`, `mint`, `amber`, `sky`, `stone`, `green`, `indigo`, the design system's tones, each of which has a ground and an ink in both themes: `INVALID_ARGUMENT` with `details.field` `color` otherwise. `hide_tag` keeps the role from being a member's tag. Of `@everyone` only the permissions change; its name, colour and tag are what the instance gave it, and a client names it itself.

**A title.** Up to 32 characters once trimmed, with no line break; empty clears it (`INVALID_ARGUMENT` with `details.field` `title` otherwise). A member or a guest sets its own through `PATCH /servers/{server_id}/membership`. Someone else's is set through `PATCH /servers/{server_id}/members/{account_id}` by an actor who could kick them (section 3.3). A title grants nothing and resolution never reads it. A member whose title a manager cleared may set it again; the remedy for one who keeps doing so is the kick.

**Giving and taking roles** is as section 3.3 says and no more: each role given or taken must be one the actor can manage, whoever the member is. The 20-role limit of section 10 applies to what the member would then hold.

**A member's tag** is not kept anywhere; a client works it out from what it holds (architecture 7.5): where the server's `member_tag` is `role`, the member's role of lowest position among those without `hide_tag`, `@everyone` never; where it is `title`, the member's title; nothing where there is none.

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
| Roles assigned or removed | `MembershipUpdated` to that member, in the same transaction; `MembersUpdated` to every member of the server afterwards (section 6.4) |
| A member's title changed | The same two |
| Server name, picture or retention changed (`MANAGE_SERVER`) | `ServerUpdated` to every member, by a job after the commit (architecture 6.7); no `acl_version` moves |
| Kick, ban, leave, guest expiry, account deletion | `MembershipRemoved` to that member, plus `ChannelRemoved` for each channel they were shown; they leave the server's text channels and lose their invitations there |
| Join, guest added, guest promoted | `MembershipAdded` to that member, plus `ChannelAdded` for each default channel they arrive in (architecture 6.8) |
| Member's own switches changed | `MembershipUpdated` to that member |
| Joined, added, left or removed from a channel; a server DM started | `ChannelAdded` or `ChannelRemoved` to that member when what it is shown changed, in the same transaction |
| Invited into a channel; invitation answered or dismissed | `ChannelInvitation` or `ChannelInvitationRemoved` to that member |

No `acl_version` changes. The member's sessions act on receipt; the reconciler's account-log nudge catches a missed delivery within one tick.

### 6.3 Viewer diff job

When a definition-level change can alter who may view a channel, an Oban job (debounced 30 seconds per server so a burst of admin edits runs once) recomputes the set of viewers for each affected channel (those in it who can view it, section 3.4), diffs it against the previous set, and appends `ChannelAdded` or `ChannelRemoved` to the account logs of members who gained or lost view. A change of channel membership settles the one account's row at once instead. Discovery is therefore eventually consistent in the background; delivery is stopped immediately by `acl_version`.

### 6.4 What every member's client is told

Sections 6.1 and 6.2 decide who may do what and are part of the change's transaction. What follows decides only what clients draw (architecture 7.5), and runs after the commit, as `ServerUpdated` does (architecture 6.7).

- **A server's roles.** `AccountSnapshot.roles` carries the roles of every server the account is in, and `MembershipAdded.roles` those of a server joined later. After a role is created, changed, reordered or deleted, `Lumia.Workers.RolesUpdated` appends `RolesUpdated`, the server's roles as they are when it runs, to every member's account log, 500 members to a transaction. It is scheduled one second after the change and is one job for a burst of edits; an edit made while one runs schedules another.
- **Members' roles and titles.** A change of either writes a `member_announcements` row in its transaction. `Lumia.Workers.MembersUpdated`, scheduled and made one as the job above is, reads the server's rows, appends `MembersUpdated` to every member's log with the members named there as they are when it runs (their `role_ids`, `title` and `kind`, as the member list gives them; at most 200 members to an event; one who has left the server is left out), then deletes the rows it read. A change made while it runs has a row of its own and a job of its own. A job that fails is run again and may tell members twice, which costs nothing: each event says how things are, not what changed.
- **A role deleted** takes itself from whoever held it. No `MembersUpdated` follows: a client drops from its members a role that `RolesUpdated` no longer lists.
- **A client's own permissions.** `Member.permissions` in the caller's own membership is the instance's resolution when it was sent. A change to a role the caller holds, or to `@everyone`, sends the caller no `MembershipUpdated`, so on `RolesUpdated` a client works out again what it may offer from the roles it holds (section 3.1, steps 1 to 3, for the server and no channel). This is for what to offer and nothing else; the instance checks every write.

The member list (`GET /servers/{server_id}/members`) carries each member's `role_ids` and `title`, and is what a client starts from.

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

Every change to roles (a reorder is one row, `role.reorder`, with the order before and after), overwrites, memberships (a title among them, `member.title`), ownership, instance roles, and every override writes an `audit_log` row in the same transaction: scope, actor, action, target, `before` and `after` as JSON, and the override flag. The rows exist from the first release so that the audit-log viewing feature, which the architecture defers, needs no backfill. Rows are retained for the instance's retention window for audit data (default two years, operator-configurable).

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
| Bounded-staleness interval | 10 minutes (measured: a re-resolution is 0.3 ms p50, 0.7 ms p99, paid per stale delivery; [spikes/2026-11-06-load.md](../spikes/2026-11-06-load.md)) |
| Viewer-diff debounce | 30 seconds per server |
| Snapshot cache TTL | 10 minutes |
| `AclChanged` batch size for server-wide bumps | 100 channels per job |
| A role's name, a member's title | 32 characters each |
| `RolesUpdated` and `MembersUpdated` | Scheduled 1 second after the change; 500 members to a transaction; 200 members to a `MembersUpdated` |

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
Create a role with a position in the request; verify it is placed last, above
  @everyone only, whatever was sent. Update one the same way; verify its place
  did not move.
A member with MANAGE_ROLES reorders: moving a role above their own highest,
  moving a role that is above them, and naming a role twice or leaving one out;
  verify each is refused and nothing changed. Reordering the roles below them;
  verify positions 1..n, one audit row, and RolesUpdated on every member's log
  once for two reorders made together.
Name a role as another is named but for its capitals; verify ALREADY_EXISTS.
  Give one a colour that is not a tone; verify INVALID_ARGUMENT.
Give a member a role; verify MembershipUpdated on that member's log at once and
  MembersUpdated, with the member's roles, on every member's log after the job.
A member sets their own title; someone with MANAGE_MEMBERS sets the title of a
  member below them, then of one above them and of the owner; verify the last
  two are PERMISSION_DENIED.
Make a role; verify it among the server's roles in a fresh account snapshot and
  in MembershipAdded for someone who joins afterwards.
```
