// The page's client state: one bridge for the instance the page is
// signed in to, the account snapshot and a channel's messages as queries
// that the gateway's events invalidate. Screens read these and call the
// bridge; nothing here renders.
import { blobUrls } from "./media";
import { QueryClient, createInfiniteQuery, createQuery, useQueryClient } from "@tanstack/solid-query";
import { createSignal } from "solid-js";
import { Bridge, ClientError, claimOwnership, type ClientEvent, type MemberSummary, type MessageSummary, type Opened, type ReadStateSummary, type SnapshotSummary, type StatusSummary } from "./bridge";
import type { DraftStore } from "./drafts";
import { type Ended, endedNotice } from "./ended";
import { type ProfileUpdate, membersWithProfile, peopleWithProfile, profileLog, snapshotWithProfile, withProfile } from "./profiles";
import { redressed } from "./roles";

export const queryClient = new QueryClient({
  defaultOptions: { queries: { staleTime: Infinity, retry: false } },
});

/**
 * A query whose `data` is undefined until it has some, instead of
 * suspending; every query here is made through it. Solid Query reads
 * `data` through a resource, and a resource read with nothing in it yet
 * suspends, in two ways that both cost a screen:
 *
 * - The navigation in progress (the router runs each one as a Solid
 *   transition) waits for the query. While it waits, whatever changes
 *   meanwhile (an event, another query answering) runs outside the
 *   transition and reads what the new screen made inside it as undefined:
 *   the conversation Accept opens fell over that way on a slow machine,
 *   "reading 'defaultQueryOptions'" of undefined or "a stale value from
 *   <Show>", with nothing on the page. So a pending query's `data` is
 *   undefined here and never asked of the resource.
 * - A query made when its answer is already held (a conversation gone back
 *   to, the members beside it) says at once that it has data, but its
 *   resource takes that answer a moment later, and a read in between
 *   suspends the route: the whole frame left the document and came straight
 *   back, the same elements, and whatever had the keyboard lost it. On a
 *   slow machine that landed after a person or a test had begun to type,
 *   and the words or the press went nowhere (the WebKit suite on the macOS
 *   runner, a different step each run). So `data` is undefined too until
 *   the resource has had its turn: two microtasks, with nothing drawn
 *   between.
 *
 * Every screen says what it is still looking for, so none needs to suspend.
 */
function settled<Q extends { readonly isPending: boolean }>(query: Q): Q {
  const [answered, setAnswered] = createSignal(false);
  queueMicrotask(() => queueMicrotask(() => setAnswered(true)));
  return new Proxy(query, { get: (q, key) => (key === "data" && (q.isPending || !answered()) ? undefined : Reflect.get(q, key)) });
}

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

// What the sign-in card says to someone the instance sent back to it, or
// whose browser another device removed: set when a session turns out to
// be over, read once by the card.
const [ended, setEnded] = createSignal<Ended | null>(null);

/** The notice for the sign-in card, taken: the next card starts without it, and a removal is over once said. */
export function takeEndedNotice(): Ended | null {
  const notice = ended();
  if (notice) setEnded(null);
  if (notice?.removed) setRemoved(false);
  return notice;
}

/**
 * The instance no longer takes this session (it was revoked, or ran out):
 * what the page held of the account goes, as on signing out, and the
 * sign-in card says what happened. Not when the device was removed,
 * which says so itself.
 */
export async function sessionEnded(): Promise<void> {
  if (removed()) return;
  setEnded(endedNotice(false));
  await signOut().catch((e) => console.warn("sign out:", e));
}

// Another device removed this browser from the identity (identity spec
// 4.4): the core erased the store, the identity, its keys and the tokens
// before the loop said so, and gave the browser a new device id. The root
// follows this to the sign-in card, after `forgetRemovedDevice`; until the
// card has said so, a session found ended on the way is not said over it.
const [removed, setRemoved] = createSignal(false);
export { removed };

/**
 * What the page still holds of a removed device's account goes: every
 * query, the scrollback and profiles held, presence, who was typing, the
 * bodies the worker keeps read and the browser's push subscription. The
 * instance is not asked anything, as the tokens went with the erase.
 */
export async function forgetRemovedDevice(): Promise<void> {
  setEnded(endedNotice(true));
  const b = client();
  await b.stop().catch(() => {});
  await b.signOut().catch(() => {});
  await b.forgetBodies().catch(() => {});
  try {
    const registration = await navigator.serviceWorker?.getRegistration("/");
    await (await registration?.pushManager.getSubscription())?.unsubscribe();
  } catch (e) {
    console.warn("push:", e);
  }
  forgetPage();
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
  forgetPage();
}

// What the page holds of the account outside the queries goes with them:
// the instance's scrollback, the pages read, the public profiles fetched.
function forgetPage(): void {
  queryClient.clear();
  history.clear();
  storedCount.clear();
  setMessagePages(new Map());
  profiles.clear();
  setProfilesVersion((v) => v + 1);
  setStarted(false);
  gatewayHeld = false;
  setConnection("idle");
  setPresence(new Map());
  setTypingAndWatch(new Map());
  setPushState("unknown");
  blobUrls.clear();
  const o = opened();
  if (o) setOpened({ ...o, signedIn: false });
}

// The last command the gateway refused, for the view that sent it to say so.
const [commandError, setCommandError] = createSignal<{ commandId: string; code: string; message: string } | null>(null);
export { commandError };

function onGatewayEvent(e: ClientEvent): void {
  switch (e.kind) {
    case "error":
      setCommandError({ commandId: e.commandId, code: e.code, message: e.message });
      break;
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
      void queryClient.invalidateQueries({ queryKey: ["readStates"] });
      // A message that names this account lands on the Mentions pages.
      if (e.events.some((event) => event.kind === "message_created")) void queryClient.invalidateQueries({ queryKey: ["mentions"] });
      // Someone came or went (architecture 6.8): who is in the channel is asked again.
      if (e.events.some((event) => event.message?.notice)) void queryClient.invalidateQueries({ queryKey: ["channelMembers", e.channelId] });
      for (const event of e.events) if (event.message) stopTyping(e.channelId, event.message.authorAccountId);
      break;
    case "typing":
      startTyping(e.channelId, e.accountId, e.untilMicros / 1000);
      break;
    case "accountApplied":
      void queryClient.invalidateQueries({ queryKey: ["snapshot"] });
      // Another device read something.
      void queryClient.invalidateQueries({ queryKey: ["readStates"] });
      void queryClient.invalidateQueries({ queryKey: ["dmChannels"] });
      break;
    case "tagsChanged":
      // What the core keeps has the change already: the members are read from it, and nothing is asked.
      void membersKept(e.serverId);
      break;
    case "emojiChanged":
      // The same: the server's emoji are read from what the core keeps and written into the snapshot held.
      void emojiKept(e.serverId);
      break;
    case "profileUpdated":
      // Written in where the page holds the person, not asked again (issue #15),
      // and kept for a list already being fetched, whose answer may be older.
      profileUpdates.add(e);
      queryClient.setQueriesData<MemberSummary[]>({ queryKey: ["members"] }, (list) => membersWithProfile(list, e));
      queryClient.setQueriesData<Map<string, MemberSummary>>({ queryKey: ["people"] }, (people) => peopleWithProfile(people, e));
      queryClient.setQueryData<SnapshotSummary>(["snapshot"], (snapshot) => snapshotWithProfile(snapshot, e));
      profileUpdated(e);
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
      // Removed by another device: nothing is left to start again or ask.
      if (e.removed) {
        gatewayStopping = false;
        setRemoved(true);
        break;
      }
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
      void queryClient.invalidateQueries({ queryKey: ["readStates"] });
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
  return settled(createQuery(() => ({
    queryKey: ["snapshot"],
    queryFn: async () => {
      const before = queryClient.getQueryData<SnapshotSummary>(["snapshot"]);
      const snapshot = await client().snapshot();
      // A snapshot recounts where everything was read.
      void queryClient.invalidateQueries({ queryKey: ["readStates"] });
      // A server that now shows its deleted messages, or no longer does, changes what the core reads of it (architecture 6.7).
      const showing = (s?: SnapshotSummary) => new Map((s?.servers ?? []).map((v) => [v.serverId, v.showDeleted]));
      const was = showing(before);
      if (before && snapshot.servers.some((v) => was.get(v.serverId) !== v.showDeleted)) void queryClient.invalidateQueries({ queryKey: ["messages"] });
      // A server whose roles changed, or where this account now stands otherwise: its members are read again from what the
      // core keeps, which dresses them by this snapshot. The `tagsChanged` before it read them by the one before (architecture 7.5).
      for (const serverId of redressed(before, snapshot)) void membersKept(serverId);
      return snapshot;
    },
  })));
}

/** Read state by conversation, and the badge (architecture 6.6): what the core counts, asked again after every event that can move it. */
export interface ReadStates {
  byChannel: Map<string, ReadStateSummary>;
  badge: number;
}

export function readStatesQuery() {
  return settled(createQuery(() => ({
    queryKey: ["readStates"],
    queryFn: async (): Promise<ReadStates> => {
      const [states, badge] = await Promise.all([client().readStates(), client().badge()]);
      return { byChannel: new Map(states.map((s) => [s.channelId, s])), badge };
    },
  })));
}

/** Where composers' drafts are kept: the core's store in this browser (`drafts.ts`). */
export const draftStore: DraftStore = {
  read: (id) => client().draft(id),
  keep: (id, body, reply) => client().setDraft(id, body, reply),
};

/** Reads a conversation through `seq` and takes the counts the instance answers with. */
export async function markRead(channelId: string, seq: number): Promise<void> {
  await client().markRead(channelId, seq);
  await queryClient.invalidateQueries({ queryKey: ["readStates"] });
}

/** The messages that notify this account, newest first, in one server or every one, a page at a time. */
export function mentionsQuery(serverId: () => string | null) {
  return settled(createInfiniteQuery(() => ({
    queryKey: ["mentions", serverId() ?? "all"],
    queryFn: ({ pageParam }: { pageParam: string | null }) => client().mentions(serverId(), pageParam),
    initialPageParam: null as string | null,
    getNextPageParam: (last: { messages: { messageId: string }[]; hasMore: boolean }) => (last.hasMore ? (last.messages[last.messages.length - 1]?.messageId ?? null) : null),
    staleTime: 0,
  })));
}

export function dmChannelsQuery(enabled: () => boolean) {
  return settled(createQuery(() => ({
    queryKey: ["dmChannels"],
    queryFn: () => client().dmChannels(),
    enabled: enabled(),
  })));
}

export function dmMessagesQuery(channelId: () => string | undefined) {
  return settled(createQuery(() => ({
    queryKey: ["dm", channelId()],
    queryFn: () => client().dmMessages(channelId() as string),
    enabled: !!channelId(),
  })));
}

// The profile updates that arrived, for a fetch begun before one to catch up with it.
const profileUpdates = profileLog();

/** Everyone this account shares a server with, by account id. */
export function peopleQuery(serverIds: () => string[]) {
  return settled(createQuery(() => ({
    queryKey: ["people", serverIds().join(",")],
    queryFn: async () => {
      const from = profileUpdates.mark();
      const people = new Map<string, MemberSummary>();
      for (const id of serverIds()) {
        for (const m of await client().members(id)) people.set(m.accountId, m);
      }
      return profileUpdates.since(from, people, (held, p) => peopleWithProfile(held, p) ?? held);
    },
    enabled: serverIds().length > 0,
  })));
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
  const from = profileUpdates.mark();
  void client()
    .account(accountId)
    .then((p) => profiles.set(accountId, profileUpdates.since(from, p, withProfile)))
    .catch((e: unknown) => {
      // Not visible stays null; any other failure is retried on the next read.
      if (e instanceof ClientError && e.code === "NOT_FOUND") profiles.set(accountId, null);
      else profiles.delete(accountId);
    })
    .finally(() => setProfilesVersion((v) => v + 1));
  return undefined;
}

/** A public profile held, as the person now is; one not held is fetched as it is when first read. */
function profileUpdated(p: ProfileUpdate): void {
  const held = profiles.get(p.accountId);
  if (!held) return;
  profiles.set(p.accountId, withProfile(held, p));
  setProfilesVersion((v) => v + 1);
}

export function hasIdentityQuery() {
  return settled(createQuery(() => ({
    queryKey: ["hasIdentity"],
    queryFn: () => client().hasIdentity(),
  })));
}

/**
 * Where private messages stand in this browser (issue #20): `ready` (it
 * holds the account's identity), `none` (neither the account nor the
 * browser has one: the phrase step sets one up), `unbound` (the browser
 * made one that a reauth did not finish binding), `elsewhere` (the
 * account's identity is on other devices: link, restore or start over),
 * `mismatched` (the browser holds an identity the account is not bound to:
 * a start-over whose rebind failed after the identity was made, which
 * finishing the start-over binds).
 */
export type PrivateState = "ready" | "none" | "unbound" | "elsewhere" | "mismatched";

export async function privateState(): Promise<PrivateState> {
  const held = await client().hasIdentity();
  let bound: string | null;
  try {
    bound = await client().accountIdentity();
  } catch {
    // Unreachable: what this browser holds is what it can say.
    return held ? "ready" : "none";
  }
  if (!held) return bound ? "elsewhere" : "none";
  if (!bound) return "unbound";
  // Both exist, but private messages work only when they are one identity.
  const mine = (await client().identityShow()).identityId;
  return mine === bound ? "ready" : "mismatched";
}

/**
 * `privateState` as a query: `state` is undefined until it is known, and
 * `failed` holds why asking failed, for a page to say so and `retry`.
 * (This query was the first found to suspend a navigation and leave the
 * Home sidebar's memos undefined; `settled` now keeps every query from it.)
 */
export function privateStateQuery() {
  const query = settled(createQuery(() => ({
    queryKey: ["privateState"],
    queryFn: privateState,
  })));
  return {
    state: (): PrivateState | undefined => (query.isSuccess ? query.data : undefined),
    failed: (): string | undefined => (query.isError ? (query.error instanceof Error ? query.error.message : String(query.error)) : undefined),
    retrying: () => query.isFetching,
    retry: () => void query.refetch(),
  };
}

/** `privateState` from what the page already holds, asked only when it holds none or it was invalidated: an account snapshot each time otherwise. */
export function privateStateNow(): Promise<PrivateState> {
  return queryClient.fetchQuery({ queryKey: ["privateState"], queryFn: privateState });
}

/** After this browser's identity changed: what follows from it is asked again. */
export function identityChanged(): void {
  for (const key of ["privateState", "hasIdentity", "roster", "dmChannels", "snapshot"]) void queryClient.invalidateQueries({ queryKey: [key] });
}

/**
 * The flagship's web origin, the only one a browser restores an identity
 * on (identity spec 7); elsewhere the page says to restore in an app or
 * there, and link this browser after.
 */
export const FLAGSHIP_ORIGIN = "https://my.lumia.chat";

export function rosterQuery(enabled: () => boolean) {
  return settled(createQuery(() => ({
    queryKey: ["roster"],
    queryFn: () => client().identityShow(),
    enabled: enabled(),
  })));
}

/** A server's members, for the aside. */
export function membersQuery(serverId: () => string | undefined) {
  return settled(createQuery(() => ({
    queryKey: ["members", serverId()],
    queryFn: async () => {
      const from = profileUpdates.mark();
      const list = await client().members(serverId() as string);
      return profileUpdates.since(from, list, (held, p) => membersWithProfile(held, p) ?? held);
    },
    enabled: !!serverId(),
  })));
}

/**
 * A server's members read again from what the core keeps, where the page
 * holds them: after `tagsChanged`, after a snapshot that dresses them
 * otherwise (`redressed`), and after a change of this account's own
 * making, whose answer the core has kept (architecture 7.5). A tag, and
 * whether a member is this account's to manage, are worked out when the
 * list is read, so this is how they follow a role renamed, a title set,
 * the server showing the other of the two, or a role given to this
 * account. A list the core never kept is left to the query.
 */
export async function membersKept(serverId: string): Promise<void> {
  if (!queryClient.getQueryData(["members", serverId])) return;
  // A fetch on its way would answer with the list as it was.
  await queryClient.cancelQueries({ queryKey: ["members", serverId] });
  const kept = await client().keptMembers(serverId);
  if (kept.length) queryClient.setQueryData(["members", serverId], kept);
}

/**
 * A server's custom emoji read again from what the core keeps (after
 * `emojiChanged`, specs/emoji.md 8) and written into the snapshot the page
 * holds, with no request: the picker, the `:` list, the messages and the
 * Emoji section all draw from the snapshot's.
 */
export async function emojiKept(serverId: string): Promise<void> {
  const emoji = await client().serverEmoji(serverId);
  queryClient.setQueryData<SnapshotSummary>(["snapshot"], (snapshot) => snapshot && { ...snapshot, servers: snapshot.servers.map((s) => (s.serverId === serverId ? { ...s, emoji } : s)) });
}

/** A member as a call answered them, written into the server's list where the page holds it; `null` takes them out (kicked). */
export function tookMember(serverId: string, accountId: string, member: MemberSummary | null): void {
  queryClient.setQueryData<MemberSummary[]>(["members", serverId], (list) => list && (member ? list.map((m) => (m.accountId === accountId ? member : m)) : list.filter((m) => m.accountId !== accountId)));
}

/** Who is in a channel, as account ids (architecture 6.8). */
export function channelMembersQuery(channelId: () => string | undefined) {
  return settled(createQuery(() => ({
    queryKey: ["channelMembers", channelId()],
    queryFn: () => client().channelMembers(channelId() as string),
    enabled: !!channelId(),
  })));
}

/** The public channels of a server this account is not in, as Browse lists them. */
export function browseQuery(serverId: () => string | undefined, enabled: () => boolean = () => true) {
  return settled(createQuery(() => ({
    queryKey: ["browse", serverId()],
    queryFn: () => client().browseChannels(serverId() as string),
    enabled: !!serverId() && enabled(),
  })));
}

/** Takes a snapshot a call answered with, as a fresh one is taken. */
export function tookSnapshot(snapshot: SnapshotSummary): void {
  queryClient.setQueryData(["snapshot"], snapshot);
}

/** How many of a channel's newest messages are read from the store: a page of 200, more as the reader scrolls up or a jump to an older one asks. */
const MESSAGE_PAGE = 200;
const [messagePages, setMessagePages] = createSignal<Map<string, number>>(new Map());

/** What the instance gave of a channel's history from before the store begins (`olderMessages`), oldest first, and whether older remains. Kept for the page's life, not stored, as the desktop shell keeps it. */
type History = { messages: MessageSummary[]; more: boolean };
const history = new Map<string, History>();
/** How many messages the store gave for a channel at its last read: fewer than asked means it has no older. */
const storedCount = new Map<string, number>();
const loadingOlder = new Set<string>();

/** What the instance takes of attachments (specs/media.md 2), asked once a session; null where it keeps none, and nothing can be attached. */
export function uploadLimitsQuery() {
  return settled(createQuery(() => ({
    queryKey: ["uploadLimits"],
    queryFn: () => client().uploadLimits().catch((e) => {
      console.warn("upload limits:", e);
      return null;
    }),
  })));
}

/** A server card's picture by its address on this instance, loaded as an avatar is; none for anything else. */
export function cardImageSrc(imageUrl: string | undefined): string | undefined {
  if (!imageUrl || !/^\/api\/v1\/cards\/[0-9A-Za-z]+$/.test(imageUrl)) return undefined;
  const base = SERVED_BY_INSTANCE ? location.origin : savedInstance();
  return base ? base.replace(/\/+$/, "") + imageUrl : undefined;
}

/** A channel's messages: what the instance gave from before the store begins, then the store's newest pages. */
export function messagesQuery(channelId: () => string | undefined) {
  return settled(createQuery(() => {
    const id = channelId();
    return {
      queryKey: ["messages", id],
      queryFn: async () => {
        // Read when it runs: a page more is asked for by setting this, then refetching.
        const pages = messagePages().get(id as string) ?? 1;
        const stored = await client().messages(id as string, MESSAGE_PAGE * pages);
        storedCount.set(id as string, stored.length);
        const first = stored[0]?.seq ?? Infinity;
        const older = (history.get(id as string)?.messages ?? []).filter((m) => m.seq < first);
        return [...older, ...stored];
      },
      enabled: !!id,
    };
  }));
}

/** Reads one more page of a channel's older messages: from the store while it has more, then from the instance. False when there were no more, or one is already on its way. */
export async function loadOlderMessages(channelId: string): Promise<boolean> {
  if (loadingOlder.has(channelId)) return false;
  loadingOlder.add(channelId);
  try {
    const pages = messagePages().get(channelId) ?? 1;
    const stored = storedCount.get(channelId) ?? 0;
    if (stored >= MESSAGE_PAGE * pages) {
      setMessagePages(new Map(messagePages()).set(channelId, pages + 1));
      await queryClient.refetchQueries({ queryKey: ["messages", channelId] });
      if ((storedCount.get(channelId) ?? 0) > stored) return true;
    }
    // The store has nothing older: the instance's scrollback, before the oldest shown.
    const kept = history.get(channelId);
    if (kept && !kept.more) return false;
    const oldest = queryClient.getQueryData<MessageSummary[]>(["messages", channelId])?.[0]?.seq;
    if (oldest === undefined || oldest <= 1) return false;
    const page = await client().olderMessages(channelId, oldest, MESSAGE_PAGE);
    history.set(channelId, { messages: [...page.messages, ...(kept?.messages ?? [])], more: page.hasMore && page.messages.length > 0 });
    await queryClient.refetchQueries({ queryKey: ["messages", channelId] });
    return page.messages.length > 0;
  } catch {
    // Not asked again until the page is reloaded.
    history.set(channelId, { messages: history.get(channelId)?.messages ?? [], more: false });
    return false;
  } finally {
    loadingOlder.delete(channelId);
  }
}

export { useQueryClient };
