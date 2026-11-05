// The app shell on the Quiet Split (the design canvas's board): a server
// with its channels, or Home (home.tsx) with personal DMs, requests and
// devices, chosen by the route's search params. Over the account
// snapshot and the local store through the client worker.
import { For, Show, createEffect, createMemo, createSignal, on } from "solid-js";
import { useNavigate, useSearch } from "@tanstack/solid-router";
import {
  SidePanel,
  MemberList,
  AppShell,
  ChannelHeader,
  ChannelList,
  Composer,
  Divider,
  Message,
  MessageList,
  SearchTrigger,
  ServerHeader,
  ServerRail,
  type ChannelItem,
} from "../design/components";
import { ClientError, type ChannelSummary, type InviteSummary, type MessageSummary, type ServerSummary } from "../client/bridge";
import { checkPush, client, connection, enableNotifications, membersQuery, messagesQuery, pushState, queryClient, snapshotQuery, startGateway } from "../client/state";
import { useNames } from "../client/names";
import { ConversationAside, HomeMain, HomeSidebar, type HomeView } from "./home";

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
  const days = createMemo(() => byDay(messages.data ?? [], (m) => m.createdAtMicros));

  const home = () => search().place === "home";
  const serverId = () => search().server ?? snapshot.data?.servers[0]?.serverId;
  const goHome = (view: HomeView, dm?: string) => void navigate({ to: "/app", search: { place: "home", view, dm } });
  const goServer = (id: string) => void navigate({ to: "/app", search: { server: id } });

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
  void checkPush();

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

  const items = createMemo<ChannelItem[]>(() =>
    channels().map((c) => ({
      name: c.name,
      kind: "channel",
      active: c.channelId === channelId(),
      onClick: () => setChannelId(c.channelId),
    })),
  );

  const homeProps = () => ({
    snapshot: snapshot.data,
    view: (search().view ?? "dm") as HomeView,
    channelId: search().dm,
    go: goHome,
    toLink: () => void navigate({ to: "/link" }),
    toServer: goServer,
  });

  // An invite link for the current server, made on demand and shown to copy.
  const [invite, setInvite] = createSignal<InviteSummary | null>(null);
  const [inviteError, setInviteError] = createSignal("");
  const [copied, setCopied] = createSignal(false);
  createEffect(on(serverId, () => { setInvite(null); setInviteError(""); setCopied(false); }));
  const makeInvite = async () => {
    const id = serverId();
    if (!id) return;
    setInviteError("");
    try {
      setInvite(await client().createInvite(id));
    } catch (e) {
      const ce = e instanceof ClientError ? e : null;
      setInviteError(ce?.code === "PERMISSION_DENIED" ? "You cannot invite people to this server." : e instanceof Error ? e.message : String(e));
    }
  };
  const copyInvite = async () => {
    const url = invite()?.url;
    if (!url) return;
    try {
      await navigator.clipboard.writeText(url);
      setCopied(true);
    } catch {
      setCopied(false);
    }
  };

  // A new channel in the current server, named in the sidebar and opened once made.
  const [naming, setNaming] = createSignal(false);
  const [channelName, setChannelName] = createSignal("");
  const [channelError, setChannelError] = createSignal("");
  createEffect(on(serverId, () => { setNaming(false); setChannelName(""); setChannelError(""); }));
  const createChannel = async (e: Event) => {
    e.preventDefault();
    const id = serverId();
    const name = channelName().trim();
    if (!id || !name) return;
    setChannelError("");
    try {
      const channel = await client().createChannel(id, name);
      await queryClient.invalidateQueries({ queryKey: ["snapshot"] });
      setChannelId(channel.channelId);
      setNaming(false);
      setChannelName("");
    } catch (err) {
      const ce = err instanceof ClientError ? err : null;
      setChannelError(ce?.code === "PERMISSION_DENIED" ? "You cannot add channels to this server." : err instanceof Error ? err.message : String(err));
    }
  };

  return (
    <AppShell
      height="100vh"
      rail={
        <ServerRail
          logoSrc="/brand/lumia-original-icon.svg"
          homeCurrent={home()}
          homeUnread={snapshot.data?.pendingRequests.length || undefined}
          onHome={() => goHome("dm")}
          onAddServer={() => goHome("add")}
          servers={servers().map((s) => ({
            name: s.name,
            initials: initialsOf(s.name),
            current: !home() && s.serverId === serverId(),
            onClick: () => goServer(s.serverId),
          }))}
          self={{ name: selfName(), initials: initialsOf(selfName()), tone: toneFor(accountId()), status: "online" }}
        />
      }
      sidebar={
        <Show when={!home()} fallback={<HomeSidebar {...homeProps()} />}>
          <ServerHeader name={server()?.name ?? "Lumia"} />
          <SearchTrigger label={`Search ${server()?.name ?? ""}`} />
          <ChannelList sections={[{ title: "Channels", items: items() }]} />
          <Show when={naming()} fallback={
            <button type="button" class="lu-link app__push app__push--first" onClick={() => setNaming(true)}>
              New channel
            </button>
          }>
            <form class="app__invite" onSubmit={(e) => void createChannel(e)}>
              <input class="app__invite-url" type="text" value={channelName()} onInput={(e) => setChannelName(e.currentTarget.value)} placeholder="Channel name" aria-label="Channel name" autocomplete="off" />
              <Show when={channelError()}>
                <p class="auth__error app__status" role="alert">
                  {channelError()}
                </p>
              </Show>
              <button type="submit" class="lu-link app__push" disabled={!channelName().trim()}>
                Create channel
              </button>
            </form>
          </Show>
          <Show when={invite()} fallback={
            <button type="button" class="lu-link app__push" onClick={() => void makeInvite()}>
              Invite people
            </button>
          }>
            {(i) => (
              <div class="app__invite">
                <p class="app__status">Anyone with this link can join.</p>
                <input class="app__invite-url" type="text" readonly value={i().url} aria-label="Invite link" onFocus={(e) => e.currentTarget.select()} />
                <button type="button" class="lu-link app__push" onClick={() => void copyInvite()}>
                  {copied() ? "Copied" : "Copy link"}
                </button>
              </div>
            )}
          </Show>
          <Show when={inviteError()}>
            <p class="auth__error app__status" role="alert">
              {inviteError()}
            </p>
          </Show>
          <Show when={pushState() === "off" || pushState() === "failed"}>
            <button type="button" class="lu-link app__push" onClick={() => void enableNotifications()}>
              Turn on notifications
            </button>
          </Show>
          <p class="app__status" aria-live="polite">
            {connection()}
            {pushState() === "subscribed" ? " · notifications on" : pushState() === "denied" ? " · notifications blocked" : ""}
          </p>
        </Show>
      }
      main={
        <Show when={!home()} fallback={<HomeMain {...homeProps()} />}>
          <Show when={current()} fallback={<div class="app__empty">No channel yet. Add one with New channel in the sidebar.</div>}>
            {(ch) => (
              <>
                <ChannelHeader name={ch().name} />
                <MessageList>
                  <Show when={days().length === 0}>
                    <div class="app__empty">Nothing here yet. Say something.</div>
                  </Show>
                  <For each={days()}>
                    {(day) => (
                      <>
                        <Divider variant="day" label={day.label} />
                        <For each={day.messages}>
                          {(m: MessageSummary) => {
                            const t = timeOf(m.createdAtMicros);
                            const author = () => nameOf(m.authorAccountId);
                            return (
                              <Message author={author()} initials={initialsOf(author())} tone={toneFor(m.authorAccountId)} time={t.time} datetime={t.datetime} actions={false}>
                                {m.deleted ? "(deleted)" : m.body}
                              </Message>
                            );
                          }}
                        </For>
                      </>
                    )}
                  </For>
                  <Show when={error()}>
                    <p class="auth__error" role="alert">
                      {error()}
                    </p>
                  </Show>
                </MessageList>
                <Composer
                  placeholder={`Message #${ch().name}`}
                  value={draft()}
                  onInput={(e) => setDraft(e.currentTarget.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && !e.shiftKey) {
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
            <MembersAside serverId={serverId() ?? ""} me={accountId()} />
          </Show>
        </Show>
      }
    />
  );
}

// A server's members, in the aside panel, named as everywhere else.
function MembersAside(props: { serverId: string; me: string }) {
  const [tab, setTab] = createSignal("members");
  const members = membersQuery(() => props.serverId || undefined);
  const entries = createMemo(() =>
    (members.data ?? [])
      .map((m) => {
        const name = m.displayName || m.nickname || m.handle || m.accountId.slice(0, 8);
        return { name, initials: initialsOf(name), tone: toneFor(m.accountId), subtitle: m.handle ? "@" + m.handle : undefined, tag: m.accountId === props.me ? "You" : undefined };
      })
      .sort((a, b) => a.name.localeCompare(b.name)),
  );
  return (
    <SidePanel tabs={[{ id: "members", label: "Members", count: entries().length }]} value={tab()} onChange={setTab}>
      <MemberList sections={[{ title: "Members · " + entries().length, members: entries() }]} total={entries().length} />
    </SidePanel>
  );
}
