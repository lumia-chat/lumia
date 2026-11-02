// Home and the channel view on the Quiet Split (the design canvas's
// board), over the account snapshot and the local store through the
// client worker. Personal DMs come in the next pass; the rail's Home tile
// and the DM section are placeholders until then.
import { For, Show, createEffect, createMemo, createSignal, on } from "solid-js";
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

const TONES = ["peach", "blue", "purple", "mint", "amber", "sky", "stone", "green", "indigo"] as const;

function toneFor(id: string): (typeof TONES)[number] {
  let h = 0;
  for (const c of id) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return TONES[h % TONES.length] ?? "blue";
}

function initialsOf(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  return parts.slice(0, 2).map((p) => p[0]?.toUpperCase() ?? "").join("") || "?";
}

function timeOf(micros: number): { time: string; datetime: string } {
  const d = new Date(micros / 1000);
  return {
    time: d.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" }),
    datetime: d.toISOString(),
  };
}

export function App() {
  const snapshot = snapshotQuery();
  const [serverId, setServerId] = createSignal<string | undefined>();
  const [channelId, setChannelId] = createSignal<string | undefined>();
  const [draft, setDraft] = createSignal("");
  const messages = messagesQuery(channelId);

  const servers = createMemo<ServerSummary[]>(() => snapshot.data?.servers ?? []);
  const channels = createMemo<ChannelSummary[]>(() => (snapshot.data?.channels ?? []).filter((c) => c.serverId === serverId()));
  const current = createMemo(() => channels().find((c) => c.channelId === channelId()));
  const accountId = createMemo(() => snapshot.data?.accountId ?? "");

  // First server and first channel, once the snapshot is in; the gateway
  // starts on every channel the account can view.
  createEffect(
    on(
      () => snapshot.data,
      (snap) => {
        if (!snap) return;
        if (!serverId()) setServerId(snap.servers[0]?.serverId);
        void startGateway(snap.channels.map((c) => c.channelId));
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

  return (
    <AppShell
      height="100vh"
      rail={
        <ServerRail
          logoSrc="/brand/lumia-original-icon.svg"
          servers={servers().map((s) => ({
            name: s.name,
            initials: initialsOf(s.name),
            current: s.serverId === serverId(),
            onClick: () => setServerId(s.serverId),
          }))}
          self={{ name: snapshot.data?.displayName || snapshot.data?.handle || "You", initials: initialsOf(snapshot.data?.displayName || snapshot.data?.handle || "You"), tone: toneFor(accountId()), status: "online" }}
        />
      }
      sidebar={
        <>
          <ServerHeader name={servers().find((s) => s.serverId === serverId())?.name ?? "Lumia"} />
          <SearchTrigger label={`Search ${servers().find((s) => s.serverId === serverId())?.name ?? ""}`} />
          <ChannelList sections={[{ title: "Channels", items: items() }]} />
          <p class="app__status" aria-live="polite">
            {connection()}
          </p>
        </>
      }
      main={
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
                    const author = m.authorAccountId === accountId() ? (snapshot.data?.displayName || snapshot.data?.handle || "You") : m.authorAccountId.slice(0, 8);
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
      }
    />
  );
}
