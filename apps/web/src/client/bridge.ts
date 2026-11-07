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
}
export interface ChannelSummary {
  channelId: string;
  serverId: string;
  name: string;
  kind: string;
  lastSeq: number;
  participantAccountIds: string[];
  host: string;
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
  servers: ServerSummary[];
  channels: ChannelSummary[];
  pendingRequests: DmRequestSummary[];
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
  constructor(text: string) {
    super(text);
    const m = /^api error ([A-Z_]+): /.exec(text);
    this.code = m?.[1] ?? "";
  }
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

  /** `shell` means the desktop shell: the device is NATIVE, not origin-scoped WEB. */
  open(baseUrl: string, shell = false): Promise<Opened> {
    return this.call("open", baseUrl, shell);
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
  /** Joins the invite's server; resolves to its id. */
  acceptInvite(linkOrCode: string): Promise<string> {
    return this.call("acceptInvite", linkOrCode);
  }
  /** This client's status and the servers it has on screen (protocol 6.7). */
  watchPresence(watchServerIds: string[]): Promise<void> {
    return this.call("watchPresence", watchServerIds);
  }
  /** The status this person chose and their custom status text, kept across launches. */
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
  createChannel(serverId: string, name: string): Promise<ChannelSummary> {
    return this.call("createChannel", serverId, name);
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
