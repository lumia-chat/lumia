// The app shell on the Quiet Split (the design canvas's board): a server
// with the channels this account is in and its server DMs (architecture
// 6.8 and 6.4), or Home (home.tsx) with personal DMs, requests and
// devices, chosen by the route's search params. Over the account
// snapshot and the local store through the client worker. What opens
// over the frame (the menus, the pickers, the profile card) is in
// menus.tsx; Browse, Your settings and a channel's members in channels.tsx.
//
// A conversation's messages and its composer are timeline.tsx's: quote
// replies, reactions, mentions, editing and deleting, and the "New" line
// (architecture 6.6). Read state is drawn here: unread channels in the
// strong ink, a count of mentions on a row, a server's dot and count on
// the rail, the Mentions tile atop the sidebar (mentions.tsx), and the
// badge. The rail's order and folders, its drags and its tiles' menus are
// rail.tsx's (architecture 6.9).
//
// The board draws more than the product has: search, threads, saved,
// attachments, pins, calls, role tags, and the composer's formatting,
// emoji and mention buttons. None of those is drawn here until there is
// something behind it (plan step 6).
import { For, Match, Show, Switch, createEffect, createMemo, createSignal, on, onCleanup } from "solid-js";
import { useNavigate, useSearch } from "@tanstack/solid-router";
import {
  SidePanel,
  SidePanelScroll,
  SidePanelSearch,
  MemberList,
  AppShell,
  ChannelHeader,
  ChannelList,
  EmptyState,
  InvitationRow,
  JoinBar,
  Notice,
  QuickTiles,
  RailNote,
  SearchField,
  ServerHeader,
  ServerRail,
  TONES,
  type ChannelItem,
  type Tone,
} from "../design/components";
import { signedOut, type ChannelSummary, type MessageSummary, type ServerSummary } from "../client/bridge";
import { avatarSrc, channelMembersQuery, checkPush, client, commandError, connection, loadMyStatus, loadOlderMessages, membersQuery, messagesQuery, presence, customStatusOf, presenceOf, queryClient, readStatesQuery, sessionEnded, signOut, snapshotQuery, startGateway, tookSnapshot, typingIn, watchPresence } from "../client/state";
import { useNames } from "../client/names";
import { preferences, setAsideHidden } from "../client/preferences";
import { ConversationAside, HomeMain, HomeSidebar, type HomeView } from "./home";
import { Overlay, myStatusDot, type Over } from "./menus";
import { ServerSettings } from "./server-settings";
import { BrowsePage, ChannelMembersAside, YourSettingsPage, channelRefusal, joinChannel } from "./channels";
import { Compose, ComposerNote, Timeline, createMarkRead, invitedWords, notNotifiedWords, quoteOf, refusalWords, type Refusal, type Replying, type TimelineItem } from "./timeline";
import { MentionsPage } from "./mentions";
import { dismissRailNote, dropOnRail, railCache, railNote, railSaid, railTiles } from "./rail";

/** The tone an id is drawn in: the core's hash over the nine tones, as every client has it. */
export function toneFor(id: string): Tone {
  let h = 0;
  for (const c of id) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return TONES[h % TONES.length] ?? "blue";
}

export function initialsOf(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  return parts.slice(0, 2).map((p) => p[0]?.toUpperCase() ?? "").join("") || "?";
}

export function timeOf(micros: number): { time: string; datetime: string } {
  const d = new Date(micros / 1000);
  return {
    time: d.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" }),
    datetime: d.toISOString(),
  };
}

const CLOCK = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" });

/** The time without its "AM" or "PM", short enough for the avatar's column. */
export function shortTimeOf(micros: number): string {
  return CLOCK.formatToParts(new Date(micros / 1000))
    .filter((p) => p.type !== "dayPeriod")
    .map((p) => p.value)
    .join("")
    .trim();
}

/** The local calendar day of a date, as a key that compares. */
function dayKeyOf(d: Date): string {
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
}

function dayKey(micros: number): string {
  return dayKeyOf(new Date(micros / 1000));
}

/** "Today", "Yesterday", or the date, for a day divider. */
export function dayLabel(micros: number, now = Date.now()): string {
  const key = dayKey(micros);
  const today = new Date(now);
  if (key === dayKeyOf(today)) return "Today";
  // By calendar day, not 24 hours: a day around a clock change is not 24 hours long.
  if (key === dayKeyOf(new Date(today.getFullYear(), today.getMonth(), today.getDate() - 1))) return "Yesterday";
  const d = new Date(micros / 1000);
  const sameYear = d.getFullYear() === today.getFullYear();
  return d.toLocaleDateString(undefined, sameYear ? { month: "long", day: "numeric" } : { year: "numeric", month: "long", day: "numeric" });
}

/** Messages in order, split where the local day changes. */
export function byDay<T>(messages: T[], at: (m: T) => number): { label: string; messages: T[] }[] {
  const days: { key: string; label: string; messages: T[] }[] = [];
  for (const m of messages) {
    const key = dayKey(at(m));
    const last = days[days.length - 1];
    if (last && last.key === key) last.messages.push(m);
    else days.push({ key, label: dayLabel(at(m)), messages: [m] });
  }
  return days;
}

/** How close together one person's messages are one block: five minutes. */
const BLOCK_WITHIN_MICROS = 5 * 60 * 1_000_000;

/** What one person sent close together: the message the block opens with, and the rest under it. */
export interface Block<T> {
  first: T;
  rest: T[];
}

/**
 * A day's messages as blocks, as every client draws them: a message joins
 * the block of the one before it when the same person sent it within five
 * minutes of that one. Measured from the message before, not the block's
 * first, so a run of quick messages stays one block however long it goes
 * on. A message with no known author stands alone, and one that `starts`
 * says so (a quote reply) opens a block of its own, which those after it
 * may join.
 */
export function byBlock<T>(messages: T[], author: (m: T) => string, at: (m: T) => number, starts: (m: T) => boolean = () => false): Block<T>[] {
  const blocks: Block<T>[] = [];
  let before: T | undefined;
  for (const m of messages) {
    const last = blocks[blocks.length - 1];
    if (last && before !== undefined && !starts(m) && author(m) && author(m) === author(before) && Math.abs(at(m) - at(before)) < BLOCK_WITHIN_MICROS) last.rest.push(m);
    else blocks.push({ first: m, rest: [] });
    before = m;
  }
  return blocks;
}

/** What a notice says (architecture 6.8): "Grace joined", "Ada added Bo". */
export function noticeWords(m: MessageSummary, nameOf: (id: string) => string): string {
  const who = nameOf(m.authorAccountId);
  const about = nameOf(m.noticeAccountId);
  switch (m.notice) {
    case "joined":
      return `${who} joined`;
    case "added":
      return `${who} added ${about}`;
    case "left":
      return `${who} left`;
    case "removed":
      return `${who} removed ${about}`;
    default:
      return "";
  }
}

export interface AppSearch {
  place?: "home";
  view?: HomeView;
  dm?: string;
  server?: string;
}

const PERSONAL = new Set(["PERSONAL_DM", "PERSONAL_GROUP_DM"]);

export function App() {
  const navigate = useNavigate();
  const tileCache = railCache();
  const search = useSearch({ from: "/app" }) as () => AppSearch;
  const snapshot = snapshotQuery();
  const [channelId, setChannelId] = createSignal<string | undefined>();
  const [draft, setDraft] = createSignal("");
  const [error, setError] = createSignal("");
  const messages = messagesQuery(channelId);
  const readStates = readStatesQuery();
  const stateOf = (id: string | undefined) => (id ? readStates.data?.byChannel.get(id) : undefined);

  const home = () => search().place === "home";
  const serverId = () => search().server ?? snapshot.data?.servers[0]?.serverId;
  // Folded (a phone), the frame shows either the rail and the sidebar or
  // what was opened from them; a link that names a view opens on it.
  const [reading, setReading] = createSignal(!!(search().view || search().dm));
  // Where three columns do not fit, the aside comes over the conversation when asked for.
  const [asideOpen, setAsideOpen] = createSignal(false);
  // Whether the window is that narrow (the frame's breakpoint, components.css).
  const narrowQuery = matchMedia("(max-width: 1099px)");
  const [narrow, setNarrow] = createSignal(narrowQuery.matches);
  const onWidth = (e: MediaQueryListEvent) => {
    setNarrow(e.matches);
    if (!e.matches) setAsideOpen(false);
  };
  narrowQuery.addEventListener("change", onWidth);
  onCleanup(() => narrowQuery.removeEventListener("change", onWidth));
  // The aside's toggle: over the conversation or not in a narrow window; in
  // the row or not in a wide one, which this browser remembers.
  const asideShown = () => (narrow() ? asideOpen() : !preferences().asideHidden);
  const toggleAside = () => (narrow() ? setAsideOpen((open) => !open) : setAsideHidden(!preferences().asideHidden));
  const onEscape = (e: KeyboardEvent) => {
    if (e.key === "Escape" && asideOpen()) setAsideOpen(false);
  };
  document.addEventListener("keydown", onEscape);
  onCleanup(() => document.removeEventListener("keydown", onEscape));
  // The server's settings in its main pane, until a channel or another server is chosen;
  // or another page there: Browse, or this account's own settings in the server.
  const [settingsOpen, setSettingsOpen] = createSignal(false);
  const [page, setPage] = createSignal<"browse" | "you" | "mentions" | null>(null);
  // A public channel this account is not in, open to read before joining (architecture 6.8).
  const [preview, setPreview] = createSignal<ChannelSummary | undefined>();
  const [joining, setJoining] = createSignal(false);
  const [joinError, setJoinError] = createSignal("");
  const [invitationError, setInvitationError] = createSignal("");
  const goHome = (view: HomeView, dm?: string, read = true) => {
    setReading(read);
    setAsideOpen(false);
    void navigate({ to: "/app", search: { place: "home", view, dm } });
  };
  const goServer = (id: string) => {
    setSettingsOpen(false);
    setPage(null);
    setPreview(undefined);
    setReading(false);
    setAsideOpen(false);
    void navigate({ to: "/app", search: { server: id } });
  };

  const servers = createMemo<ServerSummary[]>(() => snapshot.data?.servers ?? []);
  const server = createMemo(() => servers().find((s) => s.serverId === serverId()));
  // The channels this account is in, and its server DMs there.
  const listed = createMemo<ChannelSummary[]>(() => (snapshot.data?.channels ?? []).filter((c) => c.serverId === serverId() && !PERSONAL.has(c.kind)));
  const channels = createMemo(() => listed().filter((c) => c.kind === "TEXT"));
  const serverDms = createMemo(() => listed().filter((c) => c.kind === "SERVER_DM"));
  const invitations = createMemo(() => (snapshot.data?.channelInvitations ?? []).filter((i) => i.channel.serverId === serverId()));
  const current = createMemo(() => {
    const id = channelId();
    if (!id) return undefined;
    return listed().find((c) => c.channelId === id) ?? (preview()?.channelId === id ? preview() : undefined);
  });
  const joined = () => listed().some((c) => c.channelId === channelId());
  const accountId = createMemo(() => snapshot.data?.accountId ?? "");
  const selfName = () => snapshot.data?.displayName || snapshot.data?.handle || "You";
  const { nameOf, photoOf, profile } = useNames(() => snapshot.data);
  /** A name where one is known; undefined for an account nothing names. */
  const knownName = (id: string): string | undefined => {
    if (id && id === accountId()) return selfName();
    const p = profile(id);
    return p ? p.displayName || p.nickname || p.handle || undefined : undefined;
  };

  // The gateway starts on every server channel the account can view; the
  // engine adds the personal DMs it knows, the account log the rest.
  createEffect(
    on(
      () => snapshot.data,
      (snap) => {
        if (!snap) return;
        void startGateway(snap.channels.filter((c) => !PERSONAL.has(c.kind)).map((c) => c.channelId));
      },
    ),
  );
  // A channel left or taken away goes; one being read before joining stays.
  createEffect(
    on(listed, (list) => {
      const reading = preview();
      if (reading && reading.channelId === channelId()) return;
      if (!list.some((c) => c.channelId === channelId())) setChannelId(channels()[0]?.channelId);
    }),
  );
  /** Puts a channel or a server DM on screen. */
  const openChannel = (id: string) => {
    setChannelId(id);
    setSettingsOpen(false);
    setPage(null);
    if (preview()?.channelId !== id) setPreview(undefined);
    setJoinError("");
    setReading(true);
    setAsideOpen(false);
  };
  /** Reads a channel Browse listed, before joining it. */
  const openPreview = (channel: ChannelSummary) => {
    setPreview(channel);
    void client().subscribe(channel.channelId).catch(() => {});
    openChannel(channel.channelId);
  };
  const join = async (id: string) => {
    if (joining()) return;
    setJoining(true);
    setJoinError("");
    setInvitationError("");
    const outcome = await joinChannel(id);
    setJoining(false);
    if ("error" in outcome) {
      setJoinError(outcome.error);
      setInvitationError(outcome.error);
      return;
    }
    if (preview()?.channelId === id) setPreview(undefined);
    openChannel(id);
  };
  const dismiss = async (id: string) => {
    setInvitationError("");
    try {
      tookSnapshot(await client().dismissChannelInvitation(id));
    } catch (err) {
      setInvitationError(channelRefusal(err));
    }
  };
  /** The other person of a server DM. */
  const partnerOf = (c: ChannelSummary) => c.participantAccountIds.find((id) => id !== accountId()) ?? "";
  // A session the instance no longer takes (revoked, or run out) leaves
  // nothing here that works: back to sign-in, which says why.
  let ending = false;
  createEffect(() => {
    if (ending || !signedOut(snapshot.error)) return;
    ending = true;
    void sessionEnded().finally(() => void navigate({ to: "/sign-in" }));
  });
  // The server on screen is the one whose presence this client wants (protocol 6.7).
  createEffect(on([() => (home() ? undefined : serverId()), connection], ([id]) => watchPresence(id)));
  void checkPush();
  void loadMyStatus();

  // One thing is open over the frame at a time: a menu, a popover or a profile card.
  const [over, setOver] = createSignal<Over | null>(null);
  const openPerson = (id: string, anchor: HTMLElement) => setOver({ kind: "person", anchor, accountId: id });

  // Replying, the note a send leaves (someone mentioned is not in the
  // channel), and a jump asked for from a Mentions page (architecture 6.6).
  const [replying, setReplying] = createSignal<Replying | null>(null);
  const [note, setNote] = createSignal<{ kind: "outside"; channelId: string; channel: string; ids: string[] } | { kind: "invited"; text: string } | null>(null);
  const [noteError, setNoteError] = createSignal("");
  const [noteBusy, setNoteBusy] = createSignal(false);
  const [pendingJump, setPendingJump] = createSignal<{ channelId: string; messageId: string } | null>(null);
  const [atEnd, setAtEnd] = createSignal(true);
  // A message of this account's being edited where it is drawn, and what the instance refused of an edit or a delete.
  const [editing, setEditing] = createSignal<string | undefined>();
  const [refused, setRefused] = createSignal<Refusal | null>(null);
  let composerInput: HTMLInputElement | undefined;
  createEffect(
    on(channelId, () => {
      setReplying(null);
      setNote(null);
      setNoteError("");
      setAtEnd(true);
      setEditing(undefined);
      setRefused(null);
    }),
  );
  const serverMembers = membersQuery(() => (home() ? undefined : serverId()));
  const channelMembers = channelMembersQuery(() => (current()?.kind === "TEXT" && joined() ? channelId() : undefined));
  // What the gateway refused of what this view sent (architecture 6.6: a mention of everyone without the permission, a quote of a message that is gone).
  const sent = new Set<string>();
  // Edits and deletes sent from this view, by their command id, which a refusal names.
  const waiting = new Map<string, { messageId: string; kind: "edit" | "delete" }>();
  createEffect(
    on(commandError, (err) => {
      if (!err) return;
      if (sent.has(err.commandId)) {
        sent.delete(err.commandId);
        setError(err.code === "PERMISSION_DENIED" ? "You cannot do that in this channel." : /not_found/i.test(err.code + err.message) ? "The message you quoted is gone." : err.message || err.code);
        return;
      }
      const w = waiting.get(err.commandId);
      if (!w) return;
      waiting.delete(err.commandId);
      setRefused({ messageId: w.messageId, text: refusalWords(w.kind, err.code, err.message) });
    }),
  );

  const send = async () => {
    const id = channelId();
    const ch = current();
    const body = draft().trim();
    if (!id || !ch || !body) return;
    const reply = replying();
    setDraft("");
    setError("");
    setReplying(null);
    setNote(null);
    setNoteError("");
    try {
      const srv = server();
      const isChannel = ch.kind === "TEXT";
      // Typed or picked, every @handle of a member is a mention; a DM has none.
      const mentions = isChannel && srv ? await client().findMentions(body, serverMembers.data ?? [], srv.canMentionEveryone) : [];
      sent.add(await client().sendMessageWith(id, body, mentions, reply ? quoteOf(reply) : null));
      void queryClient.invalidateQueries({ queryKey: ["messages", id] });
      const inChannel = channelMembers.data;
      if (isChannel && inChannel) {
        const outside = [...new Set(mentions.filter((m) => !m.everyone && m.accountId && !inChannel.includes(m.accountId)).map((m) => m.accountId))];
        if (outside.length) setNote({ kind: "outside", channelId: id, channel: ch.name, ids: outside });
      }
    } catch (e) {
      // The text comes back to the composer unless something new was typed meanwhile.
      setDraft((d) => d || body);
      if (reply) setReplying((r) => r ?? reply);
      setError(e instanceof Error ? e.message : String(e));
    }
  };
  /** Adds those a send mentioned to the channel; those who take adds as invitations are invited. */
  const addOutside = async () => {
    const n = note();
    if (n?.kind !== "outside" || noteBusy()) return;
    setNoteBusy(true);
    setNoteError("");
    try {
      const outcome = await client().addChannelMembers(n.channelId, n.ids);
      void queryClient.invalidateQueries({ queryKey: ["channelMembers", n.channelId] });
      if (outcome.invitedAccountIds.length) {
        const shown = { kind: "invited" as const, text: invitedWords(outcome.invitedAccountIds.map(nameOf), n.channel) };
        setNote(shown);
        setTimeout(() => setNote((now) => (now === shown ? null : now)), 4000);
      } else setNote(null);
    } catch (err) {
      setNoteError(channelRefusal(err));
    } finally {
      setNoteBusy(false);
    }
  };
  const noteShown = () => {
    const n = note();
    if (!n) return undefined;
    if (n.kind === "invited") return <ComposerNote>{n.text}</ComposerNote>;
    return (
      <ComposerNote
        error={noteError() || undefined}
        actions={[
          { label: `Add to #${n.channel}`, primary: true, disabled: noteBusy(), onClick: () => void addOutside() },
          { label: "Dismiss", onClick: () => setNote(null) },
        ]}
      >
        {notNotifiedWords(n.ids.map(nameOf), n.channel)}
      </ComposerNote>
    );
  };
  const startReply = (item: TimelineItem, text: string) => {
    // The "@ On" switch, on for each reply; none for a reply to oneself.
    setReplying({ messageId: item.id, authorAccountId: item.authorId, name: item.authorId ? nameOf(item.authorId) : "", text, notify: item.authorId && item.authorId !== accountId() ? true : undefined });
    composerInput?.focus();
  };
  const saveEdit = async (item: TimelineItem, body: string) => {
    const id = channelId();
    const ch = current();
    const srv = server();
    if (!id || !ch) return;
    setRefused(null);
    try {
      // An edit carries its mentions again, found as a send finds them; a server DM has none.
      const mentions = ch.kind === "TEXT" && srv ? await client().findMentions(body, serverMembers.data ?? [], srv.canMentionEveryone) : [];
      waiting.set(await client().editMessage(id, item.id, body, mentions), { messageId: item.id, kind: "edit" });
    } catch (e) {
      setRefused({ messageId: item.id, text: refusalWords("edit", "", e instanceof Error ? e.message : String(e)) });
    }
  };
  const deleteMessage = async (item: TimelineItem) => {
    const id = channelId();
    if (!id) return;
    setRefused(null);
    try {
      waiting.set(await client().deleteMessage(id, item.id), { messageId: item.id, kind: "delete" });
    } catch (e) {
      setRefused({ messageId: item.id, text: refusalWords("delete", "", e instanceof Error ? e.message : String(e)) });
    }
  };
  /** Up in the empty composer: this account's newest message here. */
  const editLast = () => {
    const last = [...timelineItems()].reverse().find((m) => m.mine && !m.notice && !m.deleted);
    if (last) setEditing(last.id);
  };
  const react = (item: TimelineItem, emoji: string, remove: boolean) => {
    const id = channelId();
    if (!id) return;
    client()
      .react(id, item.id, emoji, remove)
      .catch((e) => setError(e instanceof Error ? e.message : String(e)));
  };
  const timelineItems = createMemo<TimelineItem[]>(() =>
    (messages.data ?? []).map((m) => ({
      id: m.messageId,
      seq: m.seq,
      authorId: m.authorAccountId,
      blockKey: m.authorAccountId,
      at: m.createdAtMicros,
      body: m.body,
      deleted: m.deleted,
      edited: m.edited,
      mine: !!m.authorAccountId && m.authorAccountId === accountId(),
      notice: m.notice ? noticeWords(m, nameOf) : "",
      reply: m.reply,
      mentions: m.mentions,
      reactions: m.reactions,
      mentionsMe: m.mentionsMe,
    })),
  );
  // Read while it is on screen: the window in front and the list at its newest message.
  createMarkRead({
    channelId,
    newestSeq: () => messages.data?.[messages.data.length - 1]?.seq,
    atEnd,
    enabled: () => !home() && joined() && !settingsOpen() && !page(),
    state: () => stateOf(channelId()),
  });
  /** Opens the channel a message is in, at the message: from a Mentions page. */
  const toMessage = (m: MessageSummary) => {
    const ch = snapshot.data?.channels.find((c) => c.channelId === m.channelId);
    if (!ch || PERSONAL.has(ch.kind)) return;
    if (home() || ch.serverId !== serverId()) goServer(ch.serverId);
    openChannel(ch.channelId);
    setPendingJump({ channelId: ch.channelId, messageId: m.messageId });
  };
  // The badge (architecture 6.6): the app's, where the browser has one, and the title's.
  createEffect(
    on(
      () => readStates.data?.badge ?? 0,
      (n) => {
        document.title = n ? `(${n}) Lumia` : "Lumia";
        const nav = navigator as Navigator & { setAppBadge?: (n: number) => Promise<void>; clearAppBadge?: () => Promise<void> };
        if (n) void nav.setAppBadge?.(n).catch(() => {});
        else void nav.clearAppBadge?.().catch(() => {});
        // Nothing left unread: the notifications already shown go. (A push carries no conversation, so they cannot be told apart.)
        if (!n && "serviceWorker" in navigator)
          void navigator.serviceWorker
            .getRegistration("/")
            .then((r) => r?.getNotifications())
            .then((list) => list?.forEach((x) => x.close()))
            .catch(() => {});
      },
    ),
  );
  onCleanup(() => (document.title = "Lumia"));
  // Who else is typing here, by name (protocol 6.7).
  const typing = createMemo(() =>
    typingIn(channelId())
      .filter((id) => id !== accountId())
      .map(nameOf),
  );
  const typed = (text: string) => {
    setDraft(text);
    const id = channelId();
    // Every edit that leaves something in the field; the core spaces them out.
    if (id && text.trim()) void client().typing(id).catch(() => {});
  };

  // Unread is the strong ink on a row that is not the one open; mentions, a count at its end.
  const readMarks = (id: string, active: boolean) => {
    const rs = stateOf(id);
    return { unread: !active && (rs?.unread ?? 0) > 0, mentions: rs?.mentions || undefined };
  };
  const items = createMemo<ChannelItem[]>(() =>
    channels().map((c) => {
      const active = c.channelId === channelId() && !settingsOpen() && !page();
      return {
        name: c.name,
        kind: c.private ? "private" : "channel",
        active,
        ...readMarks(c.channelId, active),
        onClick: () => openChannel(c.channelId),
      };
    }),
  );
  const privateItems = createMemo(() => items().filter((i) => i.kind === "private"));
  const dmItems = createMemo<ChannelItem[]>(() =>
    serverDms().map((c) => {
      const partner = partnerOf(c);
      const active = c.channelId === channelId() && !settingsOpen() && !page();
      return {
        name: nameOf(partner),
        kind: "dm",
        initials: initialsOf(nameOf(partner)),
        src: photoOf(partner),
        tone: toneFor(partner),
        active,
        ...readMarks(c.channelId, active),
        onClick: () => openChannel(c.channelId),
      };
    }),
  );
  /** A server's (or Home's personal) unread and mentions, summed over its conversations. */
  const countsOf = (inPlace: (c: ChannelSummary) => boolean) => {
    let unread = false;
    let mentions = 0;
    for (const c of snapshot.data?.channels ?? []) {
      if (!inPlace(c)) continue;
      const rs = stateOf(c.channelId);
      if (rs?.unread) unread = true;
      mentions += rs?.mentions ?? 0;
    }
    return { unread, mentions };
  };
  const serverCounts = (id: string | undefined) => countsOf((c) => c.serverId === id && !PERSONAL.has(c.kind));
  const openMentions = () => {
    setSettingsOpen(false);
    setPage("mentions");
    setPreview(undefined);
    setReading(true);
    setAsideOpen(false);
  };

  const homeProps = () => ({
    snapshot: snapshot.data,
    view: (search().view ?? "dm") as HomeView,
    channelId: search().dm,
    go: goHome,
    toLink: () => void navigate({ to: "/link" }),
    toServer: goServer,
    back: () => setReading(false),
    asideShown: asideShown(),
    toggleAside,
    openPerson,
    toMessage,
  });

  const leave = async () => {
    try {
      await signOut();
    } finally {
      void navigate({ to: "/sign-in" });
    }
  };

  return (
    <>
      <AppShell
        reading={reading()}
        asideOpen={asideOpen()}
        asideHidden={preferences().asideHidden}
        onAsideClose={() => setAsideOpen(false)}
        sidebarLabel={home() ? "Conversations" : `${server()?.name ?? "Server"} channels`}
        rail={
          <ServerRail
            logoSrc="/brand/lumia-original-icon.svg"
            homeCurrent={home()}
            homeUnread={(snapshot.data?.pendingRequests.length ?? 0) + countsOf((c) => PERSONAL.has(c.kind)).mentions || undefined}
            onHome={() => goHome("dm", undefined, false)}
            onAddServer={() => goHome("add")}
            items={railTiles(snapshot.data, (s) => ({
              name: s.name,
              initials: initialsOf(s.name),
              src: avatarSrc(s.iconUrl),
              current: !home() && s.serverId === serverId(),
              unread: serverCounts(s.serverId).unread,
              mentions: serverCounts(s.serverId).mentions || undefined,
              onClick: () => goServer(s.serverId),
            }), tileCache)}
            onMenu={(id, anchor) => setOver({ kind: "railMenu", anchor, id })}
            onDrop={(id, drop) => void dropOnRail(id, drop)}
            announce={railSaid()}
            self={{
              name: selfName(),
              initials: initialsOf(selfName()),
              src: photoOf(accountId()),
              tone: toneFor(accountId()),
              // The dot is the status chosen while the gateway is connected, and a quiet one while it is not.
              status: connection() === "connected" ? myStatusDot() : "offline",
              hint: `${selfName()} · ${connection()}`,
              menuOpen: over()?.kind === "account",
              onClick: (anchor) => setOver({ kind: "account", anchor }),
            }}
          />
        }
        sidebar={
          <Show when={!home()} fallback={<HomeSidebar {...homeProps()} />}>
            <ServerHeader name={server()?.name ?? "Lumia"} onOpenMenu={server() ? (anchor) => setOver({ kind: "serverMenu", anchor }) : undefined} menuOpen={over()?.kind === "serverMenu"} onNewMessage={() => goHome("new")} />
            {/* The board's Threads / Mentions / Saved, of which only Mentions exists. */}
            <QuickTiles
              items={[
                {
                  icon: "mention",
                  label: "Mentions",
                  count: serverCounts(serverId()).mentions || undefined,
                  countLabel: serverCounts(serverId()).mentions === 1 ? "1 mention" : `${serverCounts(serverId()).mentions} mentions`,
                  current: page() === "mentions",
                  onClick: openMentions,
                },
              ]}
            />
            <ChannelList
              sections={[
                {
                  title: "Channels",
                  items: items().filter((i) => i.kind !== "private"),
                  action: { label: "Add channels", expanded: over()?.kind === "channelsMenu", onClick: (anchor) => setOver({ kind: "channelsMenu", anchor }) },
                  empty: items().length ? undefined : "You are in no channels here. Browse them with the plus.",
                  before: (
                    <Show when={invitations().length > 0}>
                      <div class="app__invitations" role="list" aria-label="Invitations">
                        <For each={invitations()}>{(i) => <InvitationRow channel={i.channel.name} by={nameOf(i.invitedByAccountId)} busy={joining()} onJoin={() => void join(i.channel.channelId)} onDismiss={() => void dismiss(i.channel.channelId)} />}</For>
                        <Show when={invitationError()}>
                          <p class="app__error" role="alert">
                            {invitationError()}
                          </p>
                        </Show>
                      </div>
                    </Show>
                  ),
                },
                // The boards' Private, between Channels and Direct messages, only when there are any.
                ...(privateItems().length ? [{ title: "Private", items: privateItems() }] : []),
                {
                  title: "Direct messages",
                  items: dmItems(),
                  action: server()?.canStartDms ? { label: "Message someone", expanded: over()?.kind === "newDm", onClick: (anchor) => setOver({ kind: "newDm", anchor }) } : undefined,
                },
              ]}
            />
          </Show>
        }
        main={
          <Show when={!home()} fallback={<HomeMain {...homeProps()} />}>
            <Switch>
              <Match when={settingsOpen() && server()}>{(srv) => <ServerSettings server={srv()} snapshot={snapshot.data} back={() => setReading(false)} />}</Match>
              <Match when={page() === "browse" && server()}>{(srv) => <BrowsePage server={srv()} back={() => setReading(false)} open={openPreview} joined={openChannel} />}</Match>
              <Match when={page() === "you" && server()}>{(srv) => <YourSettingsPage server={srv()} back={() => setReading(false)} />}</Match>
              <Match when={page() === "mentions" && server()}>{(srv) => <MentionsPage server={srv()} snapshot={snapshot.data} back={() => setReading(false)} open={toMessage} />}</Match>
              <Match when={current()}>
                {(first) => {
                  // The channel on screen, read from the memo rather than the Match's accessor: a page opened
                  // over it (Mentions) moves the Switch on while what is drawn here still updates once.
                  let shown = first();
                  const ch = () => (shown = current() ?? shown);
                  const isDm = () => ch().kind === "SERVER_DM";
                  const title = () => (isDm() ? nameOf(partnerOf(ch())) : ch().name);
                  return (
                    <>
                      <ChannelHeader
                        name={title()}
                        topic={isDm() ? undefined : ch().topic || undefined}
                        kind={isDm() ? "page" : ch().private ? "private" : "channel"}
                        onBack={() => setReading(false)}
                        backLabel={`Back to ${server()?.name ?? "the server"}`}
                        actions={isDm() ? [] : [{ icon: "members", label: asideShown() ? "Hide members" : "Show members", pressed: asideShown(), onClick: toggleAside }]}
                      />
                      <Show when={isDm()}>
                        <Notice icon="shield">Messages here are kept by {server()?.name ?? "this server"} and follow its message history settings. For an encrypted conversation, message privately.</Notice>
                      </Show>
                      <Show
                        when={!messages.data || timelineItems().length > 0}
                        fallback={
                          <div class="lu-page">
                            <EmptyState title={isDm() ? "No messages yet" : "Nothing here yet"}>{isDm() ? "Say hello." : joined() ? "Say something." : "Join to say something."}</EmptyState>
                          </div>
                        }
                      >
                        <Show when={ch().channelId} keyed>
                          {(id) => (
                            <Timeline
                              items={timelineItems()}
                              label={isDm() ? `Messages with ${title()}` : `Messages in #${ch().name}`}
                              display={preferences()}
                              me={accountId()}
                              authorName={(m) => nameOf(m.authorId)}
                              authorPhoto={(m) => photoOf(m.authorId)}
                              tone={(m) => toneFor(m.authorId)}
                              onOpenProfile={(m, anchor) => openPerson(m.authorId, anchor)}
                              openPerson={openPerson}
                              knownName={knownName}
                              photoOf={photoOf}
                              readState={() => (readStates.data ? (stateOf(id) ?? undefined) : null)}
                              canAct={joined()}
                              onReply={startReply}
                              replyingTo={replying()?.messageId}
                              onCancelReply={() => setReplying(null)}
                              onReact={react}
                              canDelete={(m) => !m.notice && (m.mine || (!isDm() && !!server()?.canManageMessages))}
                              editing={editing()}
                              onEditing={(mid) => {
                                setEditing(mid);
                                // Done with it: back to the composer, where Up took it up.
                                if (mid) setRefused(null);
                                else composerInput?.focus();
                              }}
                              onEdit={(m, body) => void saveEdit(m, body)}
                              onDelete={(m) => void deleteMessage(m)}
                              refused={refused()}
                              jumpTo={pendingJump()?.channelId === id ? pendingJump()?.messageId : undefined}
                              onJumped={() => setPendingJump(null)}
                              loadOlder={() => loadOlderMessages(id)}
                              onAtEnd={setAtEnd}
                            />
                          )}
                        </Show>
                      </Show>
                      <Show when={error()}>
                        <p class="app__error" role="alert">
                          {error()}
                        </p>
                      </Show>
                      <Show
                        when={joined()}
                        fallback={<JoinBar channel={ch().name} busy={joining()} error={joinError() || undefined} onJoin={() => void join(ch().channelId)} />}
                      >
                        <Compose
                          placeholder={isDm() ? `Message ${title()}` : `Message #${ch().name}`}
                          typing={typing()}
                          value={draft()}
                          onInput={typed}
                          onSend={() => void send()}
                          replying={replying()}
                          onCancelReply={() => setReplying(null)}
                          onReplyNotify={(on) => setReplying((r) => (r ? { ...r, notify: on } : r))}
                          onEditLast={editLast}
                          mentions={
                            !isDm() && server()
                              ? { members: serverMembers.data ?? [], inChannel: new Set(channelMembers.data ?? []), channel: ch().name, canEveryone: !!server()?.canMentionEveryone }
                              : undefined
                          }
                          note={noteShown()}
                          inputRef={(el) => (composerInput = el)}
                        />
                      </Show>
                    </>
                  );
                }}
              </Match>
              <Match when={true}>
                <ChannelHeader kind="page" name={server()?.name ?? "Lumia"} actions={[]} onBack={() => setReading(false)} />
                <div class="lu-page">
                  <EmptyState icon="hash" title="No channel open">
                    Choose one in the sidebar, or browse the server’s channels with the plus beside Channels.
                  </EmptyState>
                </div>
              </Match>
            </Switch>
          </Show>
        }
        aside={
          <Show when={home() ? (search().view ?? "dm") === "dm" && search().dm : server() && current()?.kind !== "SERVER_DM"}>
            <Show when={!home()} fallback={<ConversationAside {...homeProps()} />}>
              <Switch>
                <Match when={settingsOpen() || page()}>
                  <MembersAside serverId={serverId() ?? ""} me={accountId()} openPerson={openPerson} />
                </Match>
                <Match when={current()?.kind === "TEXT" && server() && current()}>
                  {(first) => {
                    // As the main pane's: from the memo, not the Match's accessor, while the Switch moves on.
                    let shown = first();
                    const ch = () => (shown = (current()?.kind === "TEXT" ? current() : undefined) ?? shown);
                    return (
                      <ChannelMembersAside
                        server={server() as ServerSummary}
                        channel={ch()}
                        joined={joined()}
                        me={accountId()}
                        openPerson={openPerson}
                        addPeople={(anchor) => setOver({ kind: "addPeople", anchor, channelId: ch().channelId })}
                        left={() => setChannelId(channels().find((c) => c.channelId !== ch().channelId)?.channelId)}
                      />
                    );
                  }}
                </Match>
                <Match when={!current()}>
                  <MembersAside serverId={serverId() ?? ""} me={accountId()} openPerson={openPerson} />
                </Match>
              </Switch>
            </Show>
          </Show>
        }
      />
      <Show when={over()} keyed>
        {(open) => (
          <Overlay
            over={open}
            snapshot={snapshot.data}
            server={home() ? undefined : server()}
            open={setOver}
            close={() => setOver(null)}
            onChannelCreated={(id) => {
              setOver(null);
              openChannel(id);
            }}
            toChannel={(id) => {
              setOver(null);
              openChannel(id);
            }}
            toBrowse={() => {
              setSettingsOpen(false);
              setPage("browse");
              setReading(true);
            }}
            toYourSettings={() => {
              setSettingsOpen(false);
              setPage("you");
              setReading(true);
            }}
            onConversation={(id) => goHome("dm", id)}
            toProfile={() => goHome("profile")}
            toPreferences={() => goHome("preferences")}
            toServerSettings={() => {
              setPage(null);
              setSettingsOpen(true);
              setReading(true);
            }}
            toDevices={() => goHome("devices")}
            toLink={() => void navigate({ to: "/link" })}
            onSignOut={() => void leave()}
          />
        )}
      </Show>
      <Show when={railNote()}>{(text) => <RailNote text={text()} onDismiss={dismissRailNote} />}</Show>
    </>
  );
}

// A server's members, in the aside panel, named as everywhere else: the
// ones "Find a member" matches when it holds something.
function MembersAside(props: { serverId: string; me: string; openPerson: (accountId: string, anchor: HTMLElement) => void }) {
  const members = membersQuery(() => props.serverId || undefined);
  const [query, setQuery] = createSignal("");
  createEffect(on(() => props.serverId, () => setQuery("")));
  const entries = createMemo(() => {
    void presence();
    return (members.data ?? [])
      .map((m) => {
        const name = m.displayName || m.nickname || m.handle || m.accountId.slice(0, 8);
        // The handle, the pronouns of the verified profile, and the custom status while here.
        const subtitle = [m.handle ? "@" + m.handle : "", m.pronouns, customStatusOf(props.serverId, m.accountId)].filter(Boolean).join(" · ") || undefined;
        return { id: m.accountId, name, handle: m.handle, initials: initialsOf(name), src: avatarSrc(m.avatarUrl), tone: toneFor(m.accountId), status: presenceOf(props.serverId, m.accountId), subtitle, tag: m.accountId === props.me ? "You" : undefined };
      })
      .sort((a, b) => a.name.localeCompare(b.name));
  });
  const found = createMemo(() => {
    const wanted = query().trim().toLowerCase();
    return wanted ? entries().filter((e) => e.name.toLowerCase().includes(wanted) || e.handle.toLowerCase().includes(wanted)) : entries();
  });
  // Grouped by presence, as the design's member list is: those here, then
  // the rest. With nobody known to be here the instance has said nothing
  // about presence, and a group named for it would only repeat the heading.
  const sections = createMemo(() => {
    const here = found().filter((e) => e.status);
    const away = found().filter((e) => !e.status);
    const out: { title?: string; members: typeof here }[] = [];
    if (here.length) out.push({ title: "Online · " + here.length, members: here });
    if (away.length) out.push({ title: here.length ? "Offline · " + away.length : undefined, members: away });
    return out;
  });
  return (
    <SidePanel tabs={[{ id: "members", label: "Members", count: entries().length }]} value="members">
      <SidePanelSearch>
        <SearchField label="Find a member" value={query()} onInput={(e) => setQuery(e.currentTarget.value)} />
      </SidePanelSearch>
      <SidePanelScroll>
        <MemberList sections={sections()} empty={query().trim() ? "No member matches that." : undefined} onOpen={(m, row) => m.id && props.openPerson(m.id, row)} />
      </SidePanelScroll>
    </SidePanel>
  );
}
