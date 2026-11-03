// The page's client state: one bridge for the instance the page is
// signed in to, the account snapshot and a channel's messages as queries
// that the gateway's events invalidate. Screens read these and call the
// bridge; nothing here renders.
import { QueryClient, createQuery, useQueryClient } from "@tanstack/solid-query";
import { createSignal } from "solid-js";
import { Bridge, ClientError, claimOwnership, type ClientEvent, type MemberSummary, type Opened } from "./bridge";

export const queryClient = new QueryClient({
  defaultOptions: { queries: { staleTime: Infinity, retry: false } },
});

const INSTANCE_KEY = "lumia:instance";

export function savedInstance(): string | null {
  try {
    return localStorage.getItem(INSTANCE_KEY);
  } catch {
    return null;
  }
}

export function rememberInstance(url: string) {
  try {
    localStorage.setItem(INSTANCE_KEY, url);
  } catch {
    /* ignore */
  }
}

let bridge: Bridge | null = null;
const [opened, setOpened] = createSignal<Opened | null>(null);
const [owner, setOwner] = createSignal<boolean | null>(null);
const [started, setStarted] = createSignal(false);
const [connection, setConnection] = createSignal<string>("idle");

export { opened, owner, started, connection };

/** The bridge, created on first use. */
export function client(): Bridge {
  bridge ??= new Bridge();
  return bridge;
}

/** Elects the owning tab once; false means another tab has the store. */
export async function ensureOwner(): Promise<boolean> {
  if (owner() !== null) return owner() as boolean;
  const ok = await claimOwnership();
  setOwner(ok);
  return ok;
}

/**
 * Whether this build is served by an instance (`vite build --mode
 * instance`): then the instance is the page's own origin and the only one
 * this web origin connects to (architecture 11.4).
 */
export const SERVED_BY_INSTANCE = import.meta.env.MODE === "instance";

/** Whether the page runs inside the desktop shell (apps/desktop). */
export function inShell(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** Opens the client for `baseUrl` and remembers it. */
export async function openInstance(baseUrl: string): Promise<Opened> {
  const result = await client().open(baseUrl, inShell());
  rememberInstance(baseUrl);
  setOpened(result);
  return result;
}

/**
 * Whether the open client is signed in, asked of the worker: `opened`
 * records the state at open time, and a sign-in (by password, a new
 * account or an identity claim) happens after that. Updates `opened`.
 */
export async function refreshSignedIn(): Promise<boolean> {
  const o = opened();
  if (!o) return false;
  const signedIn = await client().signedIn();
  if (signedIn !== o.signedIn) setOpened({ ...o, signedIn });
  return signedIn;
}

export type PushState = "unknown" | "off" | "subscribed" | "unavailable" | "denied" | "failed";
const [pushState, setPushState] = createSignal<PushState>("unknown");
export { pushState };

/** Whether this browser already registered the worker (and so asked before). */
export async function checkPush(): Promise<void> {
  if (pushState() !== "unknown") return;
  if (inShell() || typeof navigator === "undefined" || !("serviceWorker" in navigator) || !("PushManager" in window)) {
    setPushState("unavailable");
    return;
  }
  try {
    const registration = await navigator.serviceWorker.getRegistration("/");
    const subscription = registration && (await registration.pushManager.getSubscription());
    setPushState(subscription ? "subscribed" : "off");
  } catch {
    setPushState("off");
  }
}

/**
 * Turns notifications on from a user gesture (browsers require one for
 * the permission): registers the service worker, subscribes with the
 * instance's key, and registers the subscription with the instance.
 */
export async function enableNotifications(): Promise<PushState> {
  const { enablePush } = await import("./push");
  const info = await client().instanceInfo();
  const outcome = await enablePush(info.webPushPublicKey);
  setPushState(outcome);
  return outcome;
}

// The one gateway listener, registered on the first start and kept: the
// loop can end (`closed`) and start again, and the listener must not
// accumulate with it.
let listening = false;

/**
 * Starts the gateway loop once and routes its events into the queries.
 * `started` turns on before the worker answers, so a second call during
 * that round trip is a no-op rather than a second start, which the core
 * refuses. A failed start reports on `connection` and leaves `started`
 * off, so the next snapshot can try again.
 */
export async function startGateway(channelIds: string[]): Promise<void> {
  if (started()) return;
  setStarted(true);
  setConnection("connecting");
  const b = client();
  if (!listening) {
    listening = true;
    b.onEvent(onGatewayEvent);
  }
  try {
    await b.start(channelIds);
  } catch (e) {
    setStarted(false);
    setConnection(`failed to start: ${e instanceof Error ? e.message : String(e)}`);
    console.error("gateway:", e);
  }
}

function onGatewayEvent(e: ClientEvent): void {
  switch (e.kind) {
    case "welcomed":
      setConnection("connected");
      void queryClient.invalidateQueries({ queryKey: ["snapshot"] });
      break;
    case "applied":
      void queryClient.invalidateQueries({ queryKey: ["messages", e.channelId] });
      break;
    case "accountApplied":
      void queryClient.invalidateQueries({ queryKey: ["snapshot"] });
      void queryClient.invalidateQueries({ queryKey: ["dmChannels"] });
      break;
    case "snapshotTaken":
      void queryClient.invalidateQueries({ queryKey: ["messages", e.channelId] });
      break;
    case "reconnecting":
      setConnection(`reconnecting (attempt ${e.attempt})`);
      break;
    case "disconnected":
      setConnection(`disconnected: ${e.reason}`);
      break;
    case "closed":
      setConnection(`closed: ${e.reason}`);
      setStarted(false);
      break;
    case "dmSubscribed":
    case "dmReady":
      void queryClient.invalidateQueries({ queryKey: ["dmChannels"] });
      void queryClient.invalidateQueries({ queryKey: ["dm", e.channelId] });
      break;
    case "dmOperation":
      void queryClient.invalidateQueries({ queryKey: ["dm", e.channelId] });
      break;
    case "dmDelivered":
      void queryClient.invalidateQueries({ queryKey: ["dm"] });
      break;
    case "dmError":
      console.error("dm:", e.message);
      break;
    default:
      break;
  }
}

export function snapshotQuery() {
  return createQuery(() => ({
    queryKey: ["snapshot"],
    queryFn: () => client().snapshot(),
  }));
}

export function dmChannelsQuery(enabled: () => boolean) {
  return createQuery(() => ({
    queryKey: ["dmChannels"],
    queryFn: () => client().dmChannels(),
    enabled: enabled(),
  }));
}

export function dmMessagesQuery(channelId: () => string | undefined) {
  return createQuery(() => ({
    queryKey: ["dm", channelId()],
    queryFn: () => client().dmMessages(channelId() as string),
    enabled: !!channelId(),
  }));
}

/** Everyone this account shares a server with, by account id. */
export function peopleQuery(serverIds: () => string[]) {
  return createQuery(() => ({
    queryKey: ["people", serverIds().join(",")],
    queryFn: async () => {
      const people = new Map<string, MemberSummary>();
      for (const id of serverIds()) {
        for (const m of await client().members(id)) people.set(m.accountId, m);
      }
      return people;
    },
    enabled: serverIds().length > 0,
  }));
}

// Public profiles of people outside a shared server (a DM participant, a
// request's sender), fetched once each and read reactively; null when the
// instance will not show one.
const profiles = new Map<string, MemberSummary | null | undefined>();
const [profilesVersion, setProfilesVersion] = createSignal(0);

export function profileOf(accountId: string): MemberSummary | null | undefined {
  profilesVersion();
  if (!accountId) return null;
  if (profiles.has(accountId)) return profiles.get(accountId);
  profiles.set(accountId, undefined); // in flight
  void client()
    .account(accountId)
    .then((p) => profiles.set(accountId, p))
    .catch((e: unknown) => {
      // Not visible stays null; any other failure is retried on the next read.
      if (e instanceof ClientError && e.code === "NOT_FOUND") profiles.set(accountId, null);
      else profiles.delete(accountId);
    })
    .finally(() => setProfilesVersion((v) => v + 1));
  return undefined;
}

export function hasIdentityQuery() {
  return createQuery(() => ({
    queryKey: ["hasIdentity"],
    queryFn: () => client().hasIdentity(),
  }));
}

export function rosterQuery(enabled: () => boolean) {
  return createQuery(() => ({
    queryKey: ["roster"],
    queryFn: () => client().identityShow(),
    enabled: enabled(),
  }));
}

/** A server's members, for the aside. */
export function membersQuery(serverId: () => string | undefined) {
  return createQuery(() => ({
    queryKey: ["members", serverId()],
    queryFn: () => client().members(serverId() as string),
    enabled: !!serverId(),
  }));
}

export function messagesQuery(channelId: () => string | undefined) {
  return createQuery(() => ({
    queryKey: ["messages", channelId()],
    queryFn: () => client().messages(channelId() as string, 200),
    enabled: !!channelId(),
  }));
}

export { useQueryClient };
