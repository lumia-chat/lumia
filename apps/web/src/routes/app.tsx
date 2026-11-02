// The app shell on the Quiet Split (the design canvas's board): a server
// with its channels, or Home (home.tsx) with personal DMs, requests and
// devices, chosen by the route's search params. Over the account
// snapshot and the local store through the client worker.
import { For, Show, createEffect, createMemo, createSignal, on } from "solid-js";
import { useNavigate, useSearch } from "@tanstack/solid-router";
import {
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
import type { ChannelSummary, MessageSummary, ServerSummary } from "../client/bridge";
import { client, connection, messagesQuery, queryClient, snapshotQuery, startGateway } from "../client/state";
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
  const messages = messagesQuery(channelId);

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

  const send = async () => {
    const id = channelId();
    const body = draft().trim();
    if (!id || !body) return;
    setDraft("");
    await client().sendMessage(id, body);
    void queryClient.invalidateQueries({ queryKey: ["messages", id] });
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
  });

  return (
    <AppShell
      height="100vh"
      rail={
        <ServerRail
          logoSrc="/brand/lumia-original-icon.svg"
          homeCurrent={home()}
          homeUnread={snapshot.data?.pendingRequests.length || undefined}
          onHome={() => goHome("dm")}
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
          <p class="app__status" aria-live="polite">
            {connection()}
          </p>
        </Show>
      }
      main={
        <Show when={!home()} fallback={<HomeMain {...homeProps()} />}>
          <Show when={current()} fallback={<div class="app__empty">No channel yet. Create a server with the CLI and it appears here.</div>}>
            {(ch) => (
              <>
                <ChannelHeader name={ch().name} />
                <MessageList>
                  <Show when={(messages.data ?? []).length === 0}>
                    <div class="app__empty">Nothing here yet. Say something.</div>
                  </Show>
                  <Show when={(messages.data ?? []).length > 0}>
                    <Divider variant="day" label="Today" />
                  </Show>
                  <For each={messages.data ?? []}>
                    {(m: MessageSummary) => {
                      const t = timeOf(m.createdAtMicros);
                      const author = m.authorAccountId === accountId() ? selfName() : m.authorAccountId.slice(0, 8);
                      return (
                        <Message author={author} initials={initialsOf(author)} tone={toneFor(m.authorAccountId)} time={t.time} datetime={t.datetime} actions={false}>
                          {m.deleted ? "(deleted)" : m.body}
                        </Message>
                      );
                    }}
                  </For>
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
      aside={home() && (search().view ?? "dm") === "dm" && search().dm ? <ConversationAside {...homeProps()} /> : undefined}
    />
  );
}
