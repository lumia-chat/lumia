// The app shell on the Quiet Split (the design canvas's board): a server
// with its channels, or Home (home.tsx) with personal DMs, requests and
// devices, chosen by the route's search params. Over the account
// snapshot and the local store through the client worker. What opens
// over the frame (the menus, the profile card) is in menus.tsx.
//
// The board draws more than the product has: search, threads, mentions
// and saved, reactions, attachments, pins, calls, role tags, and the
// composer's formatting, emoji and mention buttons. None of those is
// drawn here until there is something behind it (plan step 6).
import { For, Show, createEffect, createMemo, createSignal, on } from "solid-js";
import { useNavigate, useSearch } from "@tanstack/solid-router";
import {
  SidePanel,
  SidePanelScroll,
  SidePanelSearch,
  MemberList,
  AppShell,
  ChannelHeader,
  ChannelList,
  Composer,
  Divider,
  EmptyState,
  Message,
  MessageList,
  SearchField,
  ServerHeader,
  ServerRail,
  type ChannelItem,
} from "../design/components";
import { signedOut, type ChannelSummary, type MessageSummary, type ServerSummary } from "../client/bridge";
import { checkPush, client, connection, loadMyStatus, membersQuery, messagesQuery, presence, customStatusOf, presenceOf, queryClient, sessionEnded, signOut, snapshotQuery, startGateway, typingIn, watchPresence } from "../client/state";
import { useNames } from "../client/names";
import { ConversationAside, HomeMain, HomeSidebar, type HomeView } from "./home";
import { Overlay, myStatusDot, type Over } from "./menus";

const TONES = ["peach", "blue", "purple", "mint", "amber", "sky", "stone", "green", "indigo"] as const;

export function toneFor(id: string): (typeof TONES)[number] {
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
 * on. A message with no known author stands alone.
 */
export function byBlock<T>(messages: T[], author: (m: T) => string, at: (m: T) => number): Block<T>[] {
  const blocks: Block<T>[] = [];
  let before: T | undefined;
  for (const m of messages) {
    const last = blocks[blocks.length - 1];
    if (last && before !== undefined && author(m) && author(m) === author(before) && Math.abs(at(m) - at(before)) < BLOCK_WITHIN_MICROS) last.rest.push(m);
    else blocks.push({ first: m, rest: [] });
    before = m;
  }
  return blocks;
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
  const search = useSearch({ from: "/app" }) as () => AppSearch;
  const snapshot = snapshotQuery();
  const [channelId, setChannelId] = createSignal<string | undefined>();
  const [draft, setDraft] = createSignal("");
  const [error, setError] = createSignal("");
  const messages = messagesQuery(channelId);
  const days = createMemo(() =>
    byDay(messages.data ?? [], (m) => m.createdAtMicros).map((day) => ({
      label: day.label,
      blocks: byBlock(
        day.messages,
        (m) => m.authorAccountId,
        (m) => m.createdAtMicros,
      ),
    })),
  );

  const home = () => search().place === "home";
  const serverId = () => search().server ?? snapshot.data?.servers[0]?.serverId;
  // Folded (a phone), the frame shows either the rail and the sidebar or
  // what was opened from them; a link that names a view opens on it.
  const [reading, setReading] = createSignal(!!(search().view || search().dm));
  // Where three columns do not fit, the aside comes over the conversation when asked for.
  const [asideOpen, setAsideOpen] = createSignal(false);
  const goHome = (view: HomeView, dm?: string, read = true) => {
    setReading(read);
    setAsideOpen(false);
    void navigate({ to: "/app", search: { place: "home", view, dm } });
  };
  const goServer = (id: string) => {
    setReading(false);
    setAsideOpen(false);
    void navigate({ to: "/app", search: { server: id } });
  };

  const servers = createMemo<ServerSummary[]>(() => snapshot.data?.servers ?? []);
  const server = createMemo(() => servers().find((s) => s.serverId === serverId()));
  const channels = createMemo<ChannelSummary[]>(() => (snapshot.data?.channels ?? []).filter((c) => c.serverId === serverId() && !PERSONAL.has(c.kind)));
  const current = createMemo(() => channels().find((c) => c.channelId === channelId()));
  const accountId = createMemo(() => snapshot.data?.accountId ?? "");
  const selfName = () => snapshot.data?.displayName || snapshot.data?.handle || "You";
  const { nameOf } = useNames(() => snapshot.data);

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
  createEffect(
    on(channels, (list) => {
      if (!list.some((c) => c.channelId === channelId())) setChannelId(list[0]?.channelId);
    }),
  );
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

  const send = async () => {
    const id = channelId();
    const body = draft().trim();
    if (!id || !body) return;
    setDraft("");
    setError("");
    try {
      await client().sendMessage(id, body);
      void queryClient.invalidateQueries({ queryKey: ["messages", id] });
    } catch (e) {
      // The text comes back to the composer unless something new was typed meanwhile.
      setDraft((d) => d || body);
      setError(e instanceof Error ? e.message : String(e));
    }
  };
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

  const items = createMemo<ChannelItem[]>(() =>
    channels().map((c) => ({
      name: c.name,
      kind: "channel",
      active: c.channelId === channelId(),
      onClick: () => {
        setChannelId(c.channelId);
        setReading(true);
        setAsideOpen(false);
      },
    })),
  );

  const homeProps = () => ({
    snapshot: snapshot.data,
    view: (search().view ?? "dm") as HomeView,
    channelId: search().dm,
    go: goHome,
    toLink: () => void navigate({ to: "/link" }),
    toServer: goServer,
    back: () => setReading(false),
    asideOpen: asideOpen(),
    toggleAside: () => setAsideOpen((open) => !open),
    openPerson,
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
        onAsideClose={() => setAsideOpen(false)}
        sidebarLabel={home() ? "Conversations" : `${server()?.name ?? "Server"} channels`}
        rail={
          <ServerRail
            logoSrc="/brand/lumia-original-icon.svg"
            homeCurrent={home()}
            homeUnread={snapshot.data?.pendingRequests.length || undefined}
            onHome={() => goHome("dm", undefined, false)}
            onAddServer={() => goHome("add")}
            servers={servers().map((s) => ({
              name: s.name,
              initials: initialsOf(s.name),
              current: !home() && s.serverId === serverId(),
              onClick: () => goServer(s.serverId),
            }))}
            self={{
              name: selfName(),
              initials: initialsOf(selfName()),
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
            <Show when={items().length > 0} fallback={<p class="app__empty app__empty--side">No channels yet. Add one from the menu under the server’s name.</p>}>
              <ChannelList sections={[{ title: "Channels", items: items() }]} />
            </Show>
          </Show>
        }
        main={
          <Show when={!home()} fallback={<HomeMain {...homeProps()} />}>
            <Show
              when={current()}
              fallback={
                <>
                  <ChannelHeader kind="page" name={server()?.name ?? "Lumia"} actions={[]} onBack={() => setReading(false)} />
                  <div class="lu-page">
                    <EmptyState icon="hash" title="No channel yet">
                      Add a channel from the menu under the server’s name, or a server with the rail’s plus.
                    </EmptyState>
                  </div>
                </>
              }
            >
              {(ch) => (
                <>
                  <ChannelHeader name={ch().name} onBack={() => setReading(false)} backLabel={`Back to ${server()?.name ?? "the server"}`} actions={[{ icon: "members", label: "Members", narrow: true, pressed: asideOpen(), onClick: () => setAsideOpen((open) => !open) }]} />
                  <Show
                    when={!messages.data || days().length > 0}
                    fallback={
                      <div class="lu-page">
                        <EmptyState title="Nothing here yet">Say something.</EmptyState>
                      </div>
                    }
                  >
                    <MessageList label={`Messages in #${ch().name}`}>
                      <For each={days()}>
                        {(day) => (
                          <>
                            <Divider variant="day" label={day.label} />
                            <For each={day.blocks}>
                              {(block) => {
                                const first = block.first;
                                const t = timeOf(first.createdAtMicros);
                                const author = () => nameOf(first.authorAccountId);
                                const said = (m: MessageSummary) => (m.deleted ? "(deleted)" : m.body);
                                return (
                                  <Message
                                    author={author()}
                                    initials={initialsOf(author())}
                                    tone={toneFor(first.authorAccountId)}
                                    time={t.time}
                                    datetime={t.datetime}
                                    actions={false}
                                    onOpenProfile={(anchor) => openPerson(first.authorAccountId, anchor)}
                                    parts={block.rest.map((m) => ({ time: shortTimeOf(m.createdAtMicros), datetime: timeOf(m.createdAtMicros).datetime, children: said(m) }))}
                                  >
                                    {said(first)}
                                  </Message>
                                );
                              }}
                            </For>
                          </>
                        )}
                      </For>
                    </MessageList>
                  </Show>
                  <Show when={error()}>
                    <p class="app__error" role="alert">
                      {error()}
                    </p>
                  </Show>
                  <Composer
                    placeholder={`Message #${ch().name}`}
                    tools={false}
                    typing={typing()}
                    value={draft()}
                    onInput={(e) => typed(e.currentTarget.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
                        e.preventDefault();
                        void send();
                      }
                    }}
                    onSend={() => void send()}
                  />
                </>
              )}
            </Show>
          </Show>
        }
        aside={
          <Show when={home() ? (search().view ?? "dm") === "dm" && search().dm : server()}>
            <Show when={!home()} fallback={<ConversationAside {...homeProps()} />}>
              <MembersAside serverId={serverId() ?? ""} me={accountId()} openPerson={openPerson} />
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
              setChannelId(id);
              setReading(true);
            }}
            onConversation={(id) => goHome("dm", id)}
            toProfile={() => goHome("profile")}
            toDevices={() => goHome("devices")}
            toLink={() => void navigate({ to: "/link" })}
            onSignOut={() => void leave()}
          />
        )}
      </Show>
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
        return { id: m.accountId, name, handle: m.handle, initials: initialsOf(name), tone: toneFor(m.accountId), status: presenceOf(props.serverId, m.accountId), subtitle, tag: m.accountId === props.me ? "You" : undefined };
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
