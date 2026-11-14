// The page's side of the client worker: typed async calls and a stream of
// gateway events. One bridge per instance; `connect` elects the owning
// tab with a Web Lock first (architecture 11.4), so two tabs never open
// the same OPFS store.

export interface InstanceSummary {
  host: string;
  operatorName: string;
  operatorUrl: string;
  termsUrl: string;
  privacyUrl: string;
  softwareVersion: string;
  identityProofSignInEnabled: boolean;
  identityProofCreateEnabled: boolean;
  openRegistration: boolean;
  webPushPublicKey: string;
  magicLinkEnabled: boolean;
  /** The OIDC provider's name for the sign-in button; empty without one. */
  oidcName: string;
}
export interface ServerSummary {
  serverId: string;
  name: string;
  /** The server's picture, a path of this instance (`avatarSrc` makes it a URL); empty for none. */
  iconUrl: string;
  /** How long the server keeps its messages, in days; 0 for ever. */
  retentionDays: number;
  /** Whether a deleted message keeps its place as "(deleted)" (architecture 6.7); off, the core's reads leave it out. */
  showDeleted: boolean;
  /** Whether this account may change the server's settings (architecture 6.7). */
  canManage: boolean;
  /** Whether it may create channels there (architecture 6.8). */
  canManageChannels: boolean;
  /** Whether it may start server DMs there. */
  canStartDms: boolean;
  /** Whether it may mention everyone there (`MENTION_EVERYONE`), and so is offered `@everyone` (architecture 6.6). */
  canMentionEveryone: boolean;
  /** Whether it may delete others' messages there (`MANAGE_MESSAGES`); its own it always may. */
  canManageMessages: boolean;
  /** Its own switches there: adds to channels wait as invitations; nobody may start a server DM with it. */
  channelAddsByInvitation: boolean;
  serverDmsOff: boolean;
}
/** A WebAuthn ceremony's start: the challenge's id and the options as the browser's JSON. */
export interface PasskeyChallengeSummary {
  challengeId: string;
  optionsJson: string;
}
/** One of the account's passkeys. */
export interface PasskeySummary {
  passkeyId: string;
  label: string;
  createdAtMicros: number | null;
  lastUsedAtMicros: number | null;
}
/** The signed profile this device's identity carries (architecture 6.4). */
export interface ProfileSummary {
  version: number;
  displayName: string;
  pronouns: string;
  bio: string;
  avatarUrl: string;
}
/** One account's presence in a server (`ONLINE`, `AWAY`, `DO_NOT_DISTURB`). */
export interface PresenceSummary {
  accountId: string;
  status: string;
  customStatus: string;
}
/** What an invite shows before joining (`GET /invites/{code}`). */
export interface InvitePreviewSummary {
  code: string;
  serverId: string;
  serverName: string;
  channelName: string;
  memberCount: number;
  asGuest: boolean;
  host: string;
  operatorName: string;
  openRegistration: boolean;
  identityProofCreateEnabled: boolean;
}
/** An invite this account minted: the code and the link that carries it. */
export interface InviteSummary {
  code: string;
  url: string;
  serverId: string;
  expiresAtMicros: number | null;
  maxUses: number;
  uses: number;
  /** The address an email invite went to; empty for a link. */
  email: string;
}
export interface ChannelSummary {
  channelId: string;
  serverId: string;
  name: string;
  kind: string;
  lastSeq: number;
  participantAccountIds: string[];
  host: string;
  /** TEXT: entered only by being added (architecture 6.8). */
  private: boolean;
  /** TEXT: joined by every newcomer to the server. */
  isDefault: boolean;
  /** How many are in it, where a listing says (Browse); 0 elsewhere. */
  memberCount: number;
  topic: string;
}
/** An invitation into a channel, waiting for this account's yes (architecture 6.8). */
export interface ChannelInvitationSummary {
  channel: ChannelSummary;
  invitedByAccountId: string;
  createdAtMicros: number;
}
/** Who an add brought in at once, and who it invited instead. */
export interface ChannelAddSummary {
  addedAccountIds: string[];
  invitedAccountIds: string[];
}
export interface DmRequestSummary {
  requestId: string;
  fromAccountId: string;
  toAccountId: string;
  state: string;
  sharedServerIds: string[];
  mutualFriendCount: number;
}
export interface SnapshotSummary {
  accountId: string;
  handle: string;
  displayName: string;
  /** The account's picture, as `MemberSummary.avatarUrl`; empty for none. */
  avatarUrl: string;
  servers: ServerSummary[];
  channels: ChannelSummary[];
  pendingRequests: DmRequestSummary[];
  /** Channels this account was asked into and has not answered. */
  channelInvitations: ChannelInvitationSummary[];
  /** Where this account has read each conversation, as the instance counted it; `readStates` keeps it current. */
  readStates: ReadStateSummary[];
}
/** A quote of another message of the conversation (architecture 6.6). */
export interface QuoteSummary {
  messageId: string;
  /** Its author; in a personal conversation, empty where unknown. */
  authorAccountId: string;
  /** What of it was quoted; empty once it is deleted. */
  text: string;
  originalDeleted: boolean;
  /** The reply notifies nobody: sent with the reply bar's "@ On" switch off. */
  silent: boolean;
}
/** A mention: an account, or everyone, and its UTF-8 byte range in the body (half-open). */
export interface MentionSummary {
  /** Empty when it is everyone. */
  accountId: string;
  everyone: boolean;
  start: number;
  end: number;
}
/** One emoji's reactions to a message, in the order each was first used. */
export interface ReactionSummary {
  reaction: string;
  count: number;
  /** Whether this account is among them. */
  me: boolean;
  /** Who reacted, as many as are known. */
  accountIds: string[];
}
/** A conversation's read state: the last event read, how many messages others sent after it, and how many of those notify (in a DM, every one). */
export interface ReadStateSummary {
  channelId: string;
  readSeq: number;
  unread: number;
  mentions: number;
}
/** A page of the messages that notify this account, newest first. */
export interface MentionsPage {
  messages: MessageSummary[];
  hasMore: boolean;
}
export interface StatusSummary {
  status: string;
  customStatus: string;
}
export interface MemberSummary {
  accountId: string;
  handle: string;
  displayName: string;
  nickname: string;
  /** From the account's verified profile, as the display name is. */
  pronouns: string;
  bio: string;
  /** The account's picture: a path on this instance (`/api/v1/avatars/…`); empty for none. */
  avatarUrl: string;
  /** Their switches in the server listed (architecture 6.8). */
  channelAddsByInvitation: boolean;
  serverDmsOff: boolean;
}
export interface DmChannelSummary {
  channelId: string;
  participantAccountIds: string[];
  member: boolean;
}
export interface DmMessageSummary {
  messageId: string;
  seq: number;
  senderIdentityId: string;
  senderDeviceId: string;
  senderAccountId: string;
  body: string;
  sentAtMicros: number;
  edited: boolean;
  deleted: boolean;
  mine: boolean;
  reply: QuoteSummary | null;
  reactions: ReactionSummary[];
}
export interface DmLogSummary {
  messages: DmMessageSummary[];
  beforeJoin: number;
  undecryptable: number;
}
export interface DmOpened {
  channelId: string;
  existed: boolean;
  member: boolean;
  creating: boolean;
}
export interface MessageSummary {
  messageId: string;
  channelId: string;
  seq: number;
  authorAccountId: string;
  body: string;
  createdAtMicros: number;
  deleted: boolean;
  /** A notice (architecture 6.8): `joined`, `added`, `left` or `removed`, done by the author, about `noticeAccountId`; empty for a message. */
  notice: string;
  noticeAccountId: string;
  reply: QuoteSummary | null;
  mentions: MentionSummary[];
  reactions: ReactionSummary[];
  /** It mentions this account or everyone, or quotes one of its messages. */
  mentionsMe: boolean;
  /** Its author has edited it ("(edited)"). */
  edited: boolean;
}
export interface EventSummary {
  seq: number;
  kind: string;
  message?: MessageSummary;
}
export interface DeviceSummary {
  deviceId: string;
  kind: string;
  label: string;
  webOrigin: string;
  addedAtMicros: number;
  thisDevice: boolean;
  revoked: boolean;
}
export interface RosterSummary {
  identityId: string;
  version: number;
  devices: DeviceSummary[];
}

export type ClientEvent =
  | { kind: "welcomed"; accountId: string }
  | { kind: "applied"; channelId: string; events: EventSummary[] }
  | { kind: "accountApplied"; count: number }
  | { kind: "acked"; commandId: string; seq: number }
  | { kind: "error"; code: string; message: string; commandId: string }
  | { kind: "subscriptionFailed"; channelId: string; message: string }
  | { kind: "presenceDiff"; serverId: string; joined: PresenceSummary[]; updated: PresenceSummary[]; leftAccountIds: string[] }
  /** Someone else is typing in a server's channel, until `untilMicros` on this device's clock. */
  | { kind: "typing"; channelId: string; accountId: string; untilMicros: number }
  | { kind: "snapshotTaken"; channelId: string; seq: number }
  | { kind: "disconnected"; reason: string; reconnectAfterSeconds: number }
  | { kind: "reconnecting"; attempt: number; delayMillis: number; reason: string }
  | { kind: "closed"; reason: string }
  | { kind: "dmNote"; message: string }
  | { kind: "dmError"; message: string }
  | { kind: "dmSubscribed"; channelId: string }
  | { kind: "dmOperation"; channelId: string; seq: number; senderIdentityId: string; senderDeviceId: string }
  | { kind: "dmReady"; channelId: string }
  | { kind: "dmDelivered"; operationId: string };

export interface Opened {
  host: string;
  deviceId: string;
  signedIn: boolean;
}

type Pending = { resolve: (v: unknown) => void; reject: (e: Error) => void };

/** The API error envelope as the worker reports it: `code: message`. */
export class ClientError extends Error {
  readonly code: string;
  /** `Error.details` as the protocol documents them per code (`mfa`, `state`, ...). */
  readonly details: Record<string, string>;
  constructor(text: string) {
    super(text);
    const m = /^api error ([A-Z_]+): /.exec(text);
    this.code = m?.[1] ?? "";
    this.details = {};
    const d = / details=(.*)$/.exec(text);
    for (const pair of d?.[1]?.split(",") ?? []) {
      const eq = pair.indexOf("=");
      if (eq > 0) this.details[pair.slice(0, eq)] = pair.slice(eq + 1);
    }
  }
}

/**
 * Whether a failure says the instance no longer takes this session
 * (protocol 10 and 12): it does not know the token, the refresh token ran
 * out with it, or the session or its device was revoked. Nothing but
 * signing in again follows any of them.
 */
export function signedOut(e: unknown): boolean {
  if (!(e instanceof Error)) return false;
  if (e instanceof ClientError && ["UNAUTHENTICATED", "TOKEN_EXPIRED", "SESSION_REVOKED", "DEVICE_REVOKED"].includes(e.code)) return true;
  return /\bnot signed in\b/i.test(e.message);
}

export class Bridge {
  private worker: Worker;
  private next = 1;
  private pending = new Map<number, Pending>();
  private listeners = new Set<(e: ClientEvent) => void>();

  constructor() {
    this.worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
    this.worker.onmessage = (e: MessageEvent) => {
      const data = e.data as { id?: number; ok?: boolean; result?: unknown; error?: string; event?: ClientEvent };
      if (data.event) {
        for (const l of this.listeners) l(data.event);
        return;
      }
      if (data.id === undefined) return;
      const p = this.pending.get(data.id);
      if (!p) return;
      this.pending.delete(data.id);
      if (data.ok) p.resolve(data.result);
      else p.reject(new ClientError(data.error ?? "unknown error"));
    };
  }

  private call<T>(method: string, ...args: unknown[]): Promise<T> {
    const id = this.next++;
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, { resolve: resolve as (v: unknown) => void, reject });
      this.worker.postMessage({ id, method, args });
    });
  }

  onEvent(listener: (e: ClientEvent) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  open(baseUrl: string): Promise<Opened> {
    return this.call("open", baseUrl);
  }
  instanceInfo(): Promise<InstanceSummary> {
    return this.call("instanceInfo");
  }
  /** With an invite link or code, the account joins its server too. */
  createAccount(handle: string, password: string, invite?: string): Promise<void> {
    return this.call("createAccount", handle, password, invite ?? null);
  }
  invitePreview(linkOrCode: string): Promise<InvitePreviewSummary> {
    return this.call("invitePreview", linkOrCode);
  }
  createInvite(serverId: string, maxUses = 0, expiresInHours?: number): Promise<InviteSummary> {
    return this.call("createInvite", serverId, maxUses, expiresInHours ?? null);
  }
  /** A one-use invite good for a week, mailed to the address as a link (architecture 6.2). */
  emailInvite(serverId: string, email: string): Promise<InviteSummary> {
    return this.call("emailInvite", serverId, email);
  }
  /** Joins the invite's server; resolves to its id. */
  acceptInvite(linkOrCode: string): Promise<string> {
    return this.call("acceptInvite", linkOrCode);
  }
  /** This client's status and the servers it has on screen (protocol 6.7). */
  watchPresence(watchServerIds: string[]): Promise<void> {
    return this.call("watchPresence", watchServerIds);
  }
  /** The status this person chose and their custom status text, kept across launches. */
  /** Says this person is typing in a server's channel (protocol 6.7); call it on every edit, the core spaces them out. */
  typing(channelId: string): Promise<void> {
    return this.call("typing", channelId);
  }
  status(): Promise<StatusSummary> {
    return this.call("status");
  }
  /** Chooses the status (ONLINE, AWAY, DO_NOT_DISTURB) and custom text; sent at once when connected, and on every welcome. */
  setStatus(status: string, customStatus: string): Promise<void> {
    return this.call("setStatus", status, customStatus);
  }
  invites(serverId: string): Promise<InviteSummary[]> {
    return this.call("invites", serverId);
  }
  revokeInvite(linkOrCode: string): Promise<void> {
    return this.call("revokeInvite", linkOrCode);
  }
  createServer(name: string): Promise<ServerSummary> {
    return this.call("createServer", name);
  }
  createChannel(serverId: string, name: string, isPrivate = false): Promise<ChannelSummary> {
    return this.call("createChannel", serverId, name, isPrivate);
  }
  /** The public channels of a server this account can see and is not in, with their counts (architecture 6.8). */
  browseChannels(serverId: string): Promise<ChannelSummary[]> {
    return this.call("browseChannels", serverId);
  }
  joinChannel(channelId: string): Promise<SnapshotSummary> {
    return this.call("joinChannel", channelId);
  }
  leaveChannel(channelId: string): Promise<SnapshotSummary> {
    return this.call("leaveChannel", channelId);
  }
  /** Who is in a channel, as account ids. */
  channelMembers(channelId: string): Promise<string[]> {
    return this.call("channelMembers", channelId);
  }
  addChannelMembers(channelId: string, accountIds: string[]): Promise<ChannelAddSummary> {
    return this.call("addChannelMembers", channelId, accountIds);
  }
  removeChannelMember(channelId: string, accountId: string): Promise<void> {
    return this.call("removeChannelMember", channelId, accountId);
  }
  dismissChannelInvitation(channelId: string): Promise<SnapshotSummary> {
    return this.call("dismissChannelInvitation", channelId);
  }
  /** This account's switches in a server; one left out stays. */
  updateMembership(serverId: string, channelAddsByInvitation?: boolean, serverDmsOff?: boolean): Promise<SnapshotSummary> {
    return this.call("updateMembership", serverId, channelAddsByInvitation, serverDmsOff);
  }
  /** The 1:1 server DM with a member, started if need be (architecture 6.4). */
  openServerDm(serverId: string, accountId: string): Promise<ChannelSummary> {
    return this.call("openServerDm", serverId, accountId);
  }
  setDefaultChannels(serverId: string, channelIds: string[]): Promise<SnapshotSummary> {
    return this.call("setDefaultChannels", serverId, channelIds);
  }
  passkeyRegisterStart(): Promise<PasskeyChallengeSummary> {
    return this.call("passkeyRegisterStart");
  }
  passkeyRegisterFinish(challengeId: string, credentialJson: string, label: string): Promise<void> {
    return this.call("passkeyRegisterFinish", challengeId, credentialJson, label);
  }
  passkeyAssertStart(handleOrEmail: string): Promise<PasskeyChallengeSummary> {
    return this.call("passkeyAssertStart", handleOrEmail);
  }
  signInPasskey(challengeId: string, credentialJson: string): Promise<void> {
    return this.call("signInPasskey", challengeId, credentialJson);
  }
  /** A password sign-in with the passkey factor: the assertion made for an assert challenge. */
  signInWithPasskey(handle: string, password: string, challengeId: string, credentialJson: string): Promise<void> {
    return this.call("signInWithPasskey", handle, password, challengeId, credentialJson);
  }
  /** Whether a password sign-in must also present one of this account's passkeys. */
  passkeyRequired(): Promise<boolean> {
    return this.call("passkeyRequired");
  }
  setPasskeyRequired(required: boolean): Promise<void> {
    return this.call("setPasskeyRequired", required);
  }
  passkeys(): Promise<PasskeySummary[]> {
    return this.call("passkeys");
  }
  removePasskey(passkeyId: string): Promise<void> {
    return this.call("removePasskey", passkeyId);
  }
  /** A one-time sign-in token for this account, for a native app that signed in through this page to take back. */
  handoffToken(): Promise<string> {
    return this.call("handoffToken");
  }
  /** The signed profile this browser's identity holds, if any. */
  identityProfile(): Promise<ProfileSummary | null> {
    return this.call("identityProfile");
  }
  /** Signs a new profile with this browser's device key and, signed in, sends it as the account's. */
  identitySetProfile(displayName: string, pronouns: string, bio: string, avatarUrl: string): Promise<ProfileSummary> {
    return this.call("identitySetProfile", displayName, pronouns, bio, avatarUrl);
  }
  /** Asks the instance to mail a sign-in link to `email`. */
  /**
   * `PATCH /account`: this account's handle and display name on the
   * instance. A value left out stays as it is and an empty display name
   * clears it; resolves to the snapshot as it then is. `ALREADY_EXISTS`
   * for a handle that is taken, `INVALID_ARGUMENT` for one that is not a
   * handle or a name that is too long.
   */
  updateAccount(handle?: string, displayName?: string): Promise<SnapshotSummary> {
    return this.call("updateAccount", handle, displayName);
  }
  /**
   * `PUT /account/avatar`: this account's picture, from a file's bytes and
   * its media type; the instance makes its own square of it. Resolves to
   * the snapshot as it then is. `INVALID_ARGUMENT` with `details.reason`
   * (`unsupported`, `too_large`, `invalid`), `FAILED_PRECONDITION` on an
   * instance that keeps no pictures, `RATE_LIMITED`.
   */
  setAvatar(picture: Uint8Array, mediaType: string): Promise<SnapshotSummary> {
    return this.call("setAvatar", picture, mediaType);
  }
  /** `DELETE /account/avatar`; resolves to the snapshot as it then is. */
  removeAvatar(): Promise<SnapshotSummary> {
    return this.call("removeAvatar");
  }
  /**
   * `PATCH /servers/{id}`: a server's name, how long it keeps messages in
   * days (0 for ever), or both; one left out stays. Resolves to the snapshot
   * as it then is. `PERMISSION_DENIED` without `MANAGE_SERVER`,
   * `INVALID_ARGUMENT` with `details.field` `name`.
   */
  updateServer(serverId: string, name?: string, retentionDays?: number): Promise<SnapshotSummary> {
    return this.call("updateServer", serverId, name, retentionDays);
  }
  /** Whether a server's deleted messages keep their place as "(deleted)" (architecture 6.7). */
  setShowDeleted(serverId: string, show: boolean): Promise<SnapshotSummary> {
    return this.call("setShowDeleted", serverId, show);
  }
  /** `PUT /servers/{id}/icon`: as `setAvatar`, for a server's picture. */
  setServerIcon(serverId: string, picture: Uint8Array, mediaType: string): Promise<SnapshotSummary> {
    return this.call("setServerIcon", serverId, picture, mediaType);
  }
  /** `DELETE /servers/{id}/icon`. */
  removeServerIcon(serverId: string): Promise<SnapshotSummary> {
    return this.call("removeServerIcon", serverId);
  }
  requestMagicLink(email: string): Promise<void> {
    return this.call("requestMagicLink", email);
  }
  /** Signs in with the token from a magic-link mail (the code, or the whole link). */
  signInMagicLink(token: string): Promise<void> {
    return this.call("signInMagicLink", token);
  }
  signIn(handle: string, password: string, totpCode?: string): Promise<void> {
    return this.call("signIn", handle, password, totpCode ?? null);
  }
  signOut(): Promise<void> {
    return this.call("signOut");
  }
  signedIn(): Promise<boolean> {
    return this.call("signedIn");
  }
  snapshot(): Promise<SnapshotSummary> {
    return this.call("snapshot");
  }
  messages(channelId: string, limit = 100): Promise<MessageSummary[]> {
    return this.call("messages", channelId, limit);
  }
  /** Scrollback from the instance, older than the store holds: the newest `limit` below `beforeSeq`, oldest first. Not stored. */
  olderMessages(channelId: string, beforeSeq: number, limit = 100): Promise<{ messages: MessageSummary[]; hasMore: boolean }> {
    return this.call("olderMessages", channelId, beforeSeq, limit);
  }
  start(channelIds: string[]): Promise<void> {
    return this.call("start", channelIds);
  }
  stop(): Promise<void> {
    return this.call("stop");
  }
  subscribe(channelId: string): Promise<void> {
    return this.call("subscribe", channelId);
  }
  sendMessage(channelId: string, body: string): Promise<string> {
    return this.call("sendMessage", channelId, body);
  }
  /** Sends a message with its mentions and what it quotes (only `messageId` and `text` are read); the client-minted message id. */
  sendMessageWith(channelId: string, body: string, mentions: MentionSummary[], replyTo: QuoteSummary | null): Promise<string> {
    return this.call("sendMessageWith", channelId, body, mentions, replyTo);
  }
  /** The mentions a body makes: each `@handle` of one of `members`, and `@everyone` when `everyone`. */
  findMentions(body: string, members: MemberSummary[], everyone: boolean): Promise<MentionSummary[]> {
    // A query's members are a store's proxies, which a worker message cannot carry: plain copies go.
    return this.call("findMentions", body, members.map((m) => ({ ...m })), everyone);
  }
  /** Edits one of this account's messages in a server's channel: its new body and the mentions it makes. Answers the command id an `error` event names if it is refused. */
  editMessage(channelId: string, messageId: string, body: string, mentions: MentionSummary[]): Promise<string> {
    return this.call("editMessage", channelId, messageId, body, mentions);
  }
  /** Deletes a message of a server's channel: this account's own, or another's with `MANAGE_MESSAGES`. Answers the command id an `error` event names if it is refused. */
  deleteMessage(channelId: string, messageId: string): Promise<string> {
    return this.call("deleteMessage", channelId, messageId);
  }
  /** Adds this account's reaction to a message of a server channel, or takes it away. */
  react(channelId: string, messageId: string, emoji: string, remove: boolean): Promise<void> {
    return this.call("react", channelId, messageId, emoji, remove);
  }
  /** Every conversation's read state as this client counts it. */
  readStates(): Promise<ReadStateSummary[]> {
    return this.call("readStates");
  }
  /** What notifies this account and is unread, everywhere. */
  badge(): Promise<number> {
    return this.call("badge");
  }
  /** Read through `seq`; the state as the instance then counts it. */
  markRead(channelId: string, seq: number): Promise<ReadStateSummary> {
    return this.call("markRead", channelId, seq);
  }
  /** The messages that notify this account, newest first, in one server or (`null`) all; `before` is the last message id of the page before. */
  mentions(serverId: string | null, before: string | null): Promise<MentionsPage> {
    return this.call("mentions", serverId, before);
  }
  foreground(): Promise<void> {
    return this.call("foreground");
  }
  hasIdentity(): Promise<boolean> {
    return this.call("hasIdentity");
  }
  identityLinkRequest(label: string): Promise<string> {
    return this.call("identityLinkRequest", label);
  }
  identityClaim(code: string): Promise<RosterSummary> {
    return this.call("identityClaim", code);
  }
  signInIdentity(): Promise<void> {
    return this.call("signInIdentity");
  }
  identityShow(): Promise<RosterSummary> {
    return this.call("identityShow");
  }
  identityRevoke(deviceId: string, password: string): Promise<RosterSummary> {
    return this.call("identityRevoke", deviceId, password);
  }
  pushSubscribe(endpoint: string, p256dh: string, auth: string): Promise<void> {
    return this.call("pushSubscribe", endpoint, p256dh, auth);
  }
  pushUnsubscribe(): Promise<void> {
    return this.call("pushUnsubscribe");
  }
  account(accountId: string): Promise<MemberSummary> {
    return this.call("account", accountId);
  }
  members(serverId: string, query = ""): Promise<MemberSummary[]> {
    return this.call("members", serverId, query);
  }
  dmAvailable(): Promise<boolean> {
    return this.call("dmAvailable");
  }
  dmChannels(): Promise<DmChannelSummary[]> {
    return this.call("dmChannels");
  }
  dmOpen(accountIds: string[]): Promise<DmOpened> {
    return this.call("dmOpen", accountIds);
  }
  dmRequest(accountId: string): Promise<DmRequestSummary> {
    return this.call("dmRequest", accountId);
  }
  dmRespond(requestId: string, response: "accept" | "decline" | "block"): Promise<DmOpened | DmRequestSummary> {
    return this.call("dmRespond", requestId, response);
  }
  dmSend(channelId: string, body: string): Promise<void> {
    return this.call("dmSend", channelId, body);
  }
  /** Sends a message in a personal DM quoting another, or nothing. */
  dmSendWith(channelId: string, body: string, quote: QuoteSummary | null): Promise<void> {
    return this.call("dmSendWith", channelId, body, quote);
  }
  dmReact(channelId: string, messageId: string, emoji: string, remove: boolean): Promise<void> {
    return this.call("dmReact", channelId, messageId, emoji, remove);
  }
  /** Edits one of this identity's messages in a personal DM. */
  dmEdit(channelId: string, messageId: string, body: string): Promise<void> {
    return this.call("dmEdit", channelId, messageId, body);
  }
  dmDelete(channelId: string, messageId: string): Promise<void> {
    return this.call("dmDelete", channelId, messageId);
  }
  dmRejoin(channelId: string): Promise<void> {
    return this.call("dmRejoin", channelId);
  }
  dmMessages(channelId: string): Promise<DmLogSummary> {
    return this.call("dmMessages", channelId);
  }
}

/**
 * Becomes the owning tab, or reports that another tab holds the lock. The
 * lock is held for the page's lifetime; the browser releases it when the
 * tab closes, and the next tab asking gets it.
 */
export function claimOwnership(): Promise<boolean> {
  if (!("locks" in navigator)) return Promise.resolve(true);
  return new Promise((resolve) => {
    void navigator.locks.request("lumia-owner", { ifAvailable: true }, (lock) => {
      if (!lock) {
        resolve(false);
        return Promise.resolve();
      }
      resolve(true);
      // Hold the lock until the page goes away.
      return new Promise<void>(() => {});
    });
  });
}
