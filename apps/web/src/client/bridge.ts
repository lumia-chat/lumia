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
}
export interface ServerSummary {
  serverId: string;
  name: string;
}
export interface ChannelSummary {
  channelId: string;
  serverId: string;
  name: string;
  kind: string;
  lastSeq: number;
}
export interface SnapshotSummary {
  accountId: string;
  handle: string;
  displayName: string;
  servers: ServerSummary[];
  channels: ChannelSummary[];
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
  | { kind: "snapshotTaken"; channelId: string; seq: number }
  | { kind: "disconnected"; reason: string; reconnectAfterSeconds: number }
  | { kind: "reconnecting"; attempt: number; delayMillis: number; reason: string }
  | { kind: "closed"; reason: string };

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

  open(baseUrl: string): Promise<Opened> {
    return this.call("open", baseUrl);
  }
  instanceInfo(): Promise<InstanceSummary> {
    return this.call("instanceInfo");
  }
  createAccount(handle: string, password: string): Promise<void> {
    return this.call("createAccount", handle, password);
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
