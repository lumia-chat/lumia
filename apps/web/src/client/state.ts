// The page's client state: one bridge for the instance the page is
// signed in to, the account snapshot and a channel's messages as queries
// that the gateway's events invalidate. Screens read these and call the
// bridge; nothing here renders.
import { QueryClient, createQuery, useQueryClient } from "@tanstack/solid-query";
import { createSignal } from "solid-js";
import { Bridge, ClientError, claimOwnership, type ClientEvent, type MemberSummary, type Opened, type SnapshotSummary, type StatusSummary } from "./bridge";

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

/**
 * Where a picture is, for an `<img>`: an account names its picture by a
 * path on the instance, and only that path with a name of 64 hexadecimal
 * digits is taken (the core's `is_own_avatar`), so nothing an instance
 * says can make the page fetch from anywhere else.
 */
export function avatarSrc(avatarUrl: string | undefined): string | undefined {
  if (!avatarUrl || !/^\/api\/v1\/avatars\/[0-9a-f]{64}$/.test(avatarUrl)) return undefined;
  const base = SERVED_BY_INSTANCE ? location.origin : savedInstance();
  return base ? base.replace(/\/+$/, "") + avatarUrl : undefined;
}

/**
 * Why a picture was not taken, an account's or a server's, worded as every
 * client words it.
 */
export function pictureRefusal(err: unknown): string {
  const ce = err instanceof ClientError ? err : null;
  if (ce?.code === "INVALID_ARGUMENT" && ce.details.reason === "too_large") return "That picture is too large. Choose one under 8 MB.";
  if (ce?.code === "INVALID_ARGUMENT" && ce.details.reason === "unsupported") return "That file is not a picture Lumia can read. Choose a PNG, JPEG, WebP or GIF.";
  if (ce?.code === "INVALID_ARGUMENT") return "That picture could not be read.";
  if (ce?.code === "FAILED_PRECONDITION") return "This instance does not keep pictures.";
  if (ce?.code === "RATE_LIMITED") return "The instance is busy. Try again in a moment.";
  return err instanceof Error ? err.message : String(err);
}

/** Opens the client for `baseUrl` and remembers it. */
export async function openInstance(baseUrl: string): Promise<Opened> {
  const result = await client().open(baseUrl);
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
  if (typeof navigator === "undefined" || !("serviceWorker" in navigator) || !("PushManager" in window)) {
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

// When the loop was last started, and whether it is being held back: one
// that ends within ten seconds of starting is not started again by the
// snapshot that follows. Two clients on one session knock each other off
// (the instance closes the first connection for the second), and each
// starting again at once would go on for ever; the one that lasted is the
// one that comes back, and the other waits to be looked at.
const LOOP_TOO_BRIEF_MS = 10_000;
let gatewayStartedAt = 0;
let gatewayHeld = false;
// Set while this page is stopping the loop itself (signing out): that end is not one to hold a later start for.
let gatewayStopping = false;

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
/** The channels the gateway loop has been asked for, since it last started. */
let subscribed = new Set<string>();

export async function startGateway(channelIds: string[]): Promise<void> {
  if (gatewayHeld) return;
  if (started()) {
    // A channel gained since the loop started (made, joined, added, a new
    // server DM): the loop subscribes only to what it was started with.
    for (const id of channelIds) {
      if (subscribed.has(id)) continue;
      subscribed.add(id);
      void client().subscribe(id).catch(() => subscribed.delete(id));
    }
    return;
  }
  setStarted(true);
  subscribed = new Set(channelIds);
  setConnection("connecting");
  gatewayStartedAt = Date.now();
  const b = client();
  if (!listening) {
    listening = true;
    b.onEvent(onGatewayEvent);
    // Looked at again: a loop that was held back gets another start, through the snapshot.
    document.addEventListener("visibilitychange", () => {
      if (document.visibilityState !== "visible" || !gatewayHeld) return;
      gatewayHeld = false;
      void queryClient.invalidateQueries({ queryKey: ["snapshot"] });
    });
  }
  try {
    await b.start(channelIds);
  } catch (e) {
    setStarted(false);
    setConnection(`failed to start: ${e instanceof Error ? e.message : String(e)}`);
    console.error("gateway:", e);
  }
}

// Presence (protocol 6.7): who is present in the servers on screen, by
// server then account, fed by the gateway's diffs. `watchPresence` names
// the server on screen (none for Home); the set is re-sent on every
// welcome, since a connection starts with nothing watched.
const [presence, setPresence] = createSignal<Map<string, Map<string, { status: string; customStatus: string }>>>(new Map());
export { presence };
let watching: string[] = [];

export function watchPresence(serverId: string | undefined): void {
  watching = serverId ? [serverId] : [];
  if (connection() !== "connected") return;
  client()
    .watchPresence(watching)
    .catch((e) => console.warn("presence:", e));
}

/** The status of an account in a server, as the design's `Status`, or undefined when away from it. */
export function presenceOf(serverId: string | undefined, accountId: string): "online" | "away" | "dnd" | undefined {
  const status = serverId ? presence().get(serverId)?.get(accountId)?.status : undefined;
  return status === "ONLINE" ? "online" : status === "AWAY" ? "away" : status === "DO_NOT_DISTURB" ? "dnd" : undefined;
}

/** The custom status an account set, as its presence in a server carries it; empty when none or away. */
export function customStatusOf(serverId: string | undefined, accountId: string): string {
  return (serverId ? presence().get(serverId)?.get(accountId)?.customStatus : undefined) ?? "";
}

// The status this person chose (protocol 6.7): kept by the core, read
// once the page has a client, and set from the account menu or the
// profile page. The rail's dot and the menu's check read it.
const [myStatus, setMyStatusSignal] = createSignal<StatusSummary>({ status: "ONLINE", customStatus: "" });
export { myStatus };

export async function loadMyStatus(): Promise<void> {
  try {
    setMyStatusSignal(await client().status());
  } catch {
    /* the default stands until the core answers */
  }
}

export async function setMyStatus(status: string, customStatus: string): Promise<void> {
  await client().setStatus(status, customStatus);
  setMyStatusSignal({ status, customStatus });
}

// Who is typing (protocol 6.7): by channel, each account with the time
// theirs runs out on this device's clock. A message from someone ends
// theirs at once; silence ends it when its time comes, for which a timer
// is kept on the earliest.
const [typing, setTyping] = createSignal<Map<string, Map<string, number>>>(new Map());
let typingTimer: ReturnType<typeof setTimeout> | undefined;

function setTypingAndWatch(next: Map<string, Map<string, number>>): void {
  setTyping(next);
  clearTimeout(typingTimer);
  let earliest = Infinity;
  for (const people of next.values()) for (const until of people.values()) earliest = Math.min(earliest, until);
  if (earliest === Infinity) return;
  typingTimer = setTimeout(() => {
    const now = Date.now();
    const kept = new Map<string, Map<string, number>>();
    for (const [channelId, people] of typing()) {
      const still = new Map([...people].filter(([, until]) => until > now));
      if (still.size) kept.set(channelId, still);
    }
    setTypingAndWatch(kept);
  }, Math.max(0, earliest - Date.now()) + 50);
}

function startTyping(channelId: string, accountId: string, untilMillis: number): void {
  const next = new Map(typing());
  next.set(channelId, new Map(next.get(channelId) ?? []).set(accountId, untilMillis));
  setTypingAndWatch(next);
}

function stopTyping(channelId: string, accountId: string): void {
  if (!typing().get(channelId)?.has(accountId)) return;
  const next = new Map(typing());
  const people = new Map(next.get(channelId) ?? []);
  people.delete(accountId);
  if (people.size) next.set(channelId, people);
  else next.delete(channelId);
  setTypingAndWatch(next);
}

/** The accounts typing in a channel, in the order they started. */
export function typingIn(channelId: string | undefined): string[] {
  return channelId ? [...(typing().get(channelId)?.keys() ?? [])] : [];
}

// What the sign-in card says to someone the instance sent back to it:
// set when a session turns out to be over, read once by the card.
const [endedNotice, setEndedNotice] = createSignal("");

/** The line for the sign-in card, taken: the next card starts without it. */
export function takeEndedNotice(): string {
  const notice = endedNotice();
  if (notice) setEndedNotice("");
  return notice;
}

/**
 * The instance no longer takes this session (it was revoked, or ran out):
 * what the page held of the account goes, as on signing out, and the
 * sign-in card says what happened.
 */
export async function sessionEnded(): Promise<void> {
  setEndedNotice("You were signed out of this browser. Sign in again to carry on.");
  await signOut().catch((e) => console.warn("sign out:", e));
}

/**
 * Signs this browser out and forgets what the page held of the account:
 * its push subscription, the gateway loop, every query, presence and who
 * was typing.
 */
export async function signOut(): Promise<void> {
  const b = client();
  // While the session can still say so: a signed-out browser gets no more notices for the account.
  if (pushState() === "subscribed") await (await import("./push")).disablePush();
  gatewayStopping = started();
  await b.stop().catch(() => {});
  await b.signOut();
  queryClient.clear();
  setStarted(false);
  gatewayHeld = false;
  setConnection("idle");
  setPresence(new Map());
  setTypingAndWatch(new Map());
  setPushState("unknown");
  const o = opened();
  if (o) setOpened({ ...o, signedIn: false });
}

function onGatewayEvent(e: ClientEvent): void {
  switch (e.kind) {
    case "welcomed":
      setConnection("connected");
      void queryClient.invalidateQueries({ queryKey: ["snapshot"] });
      setPresence(new Map());
      watchPresence(watching[0]);
      break;
    case "presenceDiff": {
      const next = new Map(presence());
      const server = new Map(next.get(e.serverId) ?? []);
      for (const p of [...e.joined, ...e.updated]) server.set(p.accountId, { status: p.status, customStatus: p.customStatus });
      for (const id of e.leftAccountIds) server.delete(id);
      next.set(e.serverId, server);
      setPresence(next);
      break;
    }
    case "applied":
      void queryClient.invalidateQueries({ queryKey: ["messages", e.channelId] });
      // Someone came or went (architecture 6.8): who is in the channel is asked again.
      if (e.events.some((event) => event.message?.notice)) void queryClient.invalidateQueries({ queryKey: ["channelMembers", e.channelId] });
      for (const event of e.events) if (event.message) stopTyping(e.channelId, event.message.authorAccountId);
      break;
    case "typing":
      startTyping(e.channelId, e.accountId, e.untilMicros / 1000);
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
      gatewayHeld = !gatewayStopping && Date.now() - gatewayStartedAt < LOOP_TOO_BRIEF_MS;
      gatewayStopping = false;
      // The loop has ended and will not come back by itself. Why is in
      // words only, so the snapshot is asked: a session the instance has
      // revoked answers so there and the app goes to sign-in; anything
      // else is a snapshot later, which starts the loop again.
      void queryClient.invalidateQueries({ queryKey: ["snapshot"] });
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

/** Who is in a channel, as account ids (architecture 6.8). */
export function channelMembersQuery(channelId: () => string | undefined) {
  return createQuery(() => ({
    queryKey: ["channelMembers", channelId()],
    queryFn: () => client().channelMembers(channelId() as string),
    enabled: !!channelId(),
  }));
}

/** The public channels of a server this account is not in, as Browse lists them. */
export function browseQuery(serverId: () => string | undefined, enabled: () => boolean = () => true) {
  return createQuery(() => ({
    queryKey: ["browse", serverId()],
    queryFn: () => client().browseChannels(serverId() as string),
    enabled: !!serverId() && enabled(),
  }));
}

/** Takes a snapshot a call answered with, as a fresh one is taken. */
export function tookSnapshot(snapshot: SnapshotSummary): void {
  queryClient.setQueryData(["snapshot"], snapshot);
}

export function messagesQuery(channelId: () => string | undefined) {
  return createQuery(() => ({
    queryKey: ["messages", channelId()],
    queryFn: () => client().messages(channelId() as string, 200),
    enabled: !!channelId(),
  }));
}

export { useQueryClient };
