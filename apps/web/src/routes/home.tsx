// Home: personal DMs grouped by the instance that hosts them, message
// requests, this identity's devices, and starting a conversation. From
// the "Home", "Requests" and "Devices" boards in the gallery.
import { For, Show, createMemo, createSignal } from "solid-js";
import {
  Avatar,
  Button,
  Card,
  ChannelHeader,
  ChannelList,
  Composer,
  DeviceList,
  Divider,
  EmptyState,
  MemberList,
  Message,
  MessageList,
  Notice,
  QuickTiles,
  RequestCard,
  SearchField,
  ServerHeader,
  SidePanel,
  TextField,
  type ChannelItem,
  type DeviceEntry,
} from "../design/components";
import { ClientError, type ChannelSummary, type DeviceSummary, type DmMessageSummary, type MemberSummary, type SnapshotSummary } from "../client/bridge";
import { client, dmChannelsQuery, dmMessagesQuery, hasIdentityQuery, peopleQuery, profileOf, queryClient, rosterQuery } from "../client/state";
import { initialsOf, timeOf, toneFor } from "./app";

export type HomeView = "dm" | "requests" | "devices" | "new";

export interface HomeProps {
  snapshot: SnapshotSummary | undefined;
  view: HomeView;
  channelId: string | undefined;
  go: (view: HomeView, channelId?: string) => void;
  toLink: () => void;
}

const PERSONAL = new Set(["PERSONAL_DM", "PERSONAL_GROUP_DM"]);

/** Names for the people in a conversation, from the people we share servers with. */
function useNames(props: { snapshot: SnapshotSummary | undefined }) {
  const serverIds = createMemo(() => (props.snapshot?.servers ?? []).map((s) => s.serverId));
  const people = peopleQuery(serverIds);
  const nameOf = (accountId: string): string => {
    if (accountId && accountId === props.snapshot?.accountId) return props.snapshot?.displayName || props.snapshot?.handle || "You";
    const p = people.data?.get(accountId) ?? profileOf(accountId);
    return p?.displayName || p?.nickname || p?.handle || (accountId ? accountId.slice(0, 8) : "Someone");
  };
  return { people, nameOf };
}

function conversationName(c: ChannelSummary, me: string, nameOf: (id: string) => string): string {
  const others = c.participantAccountIds.filter((id) => id !== me);
  if (others.length === 0) return "Just you";
  return others.map((id) => nameOf(id)).join(", ");
}

export function HomeSidebar(props: HomeProps) {
  const { nameOf } = useNames(props);
  const me = () => props.snapshot?.accountId ?? "";
  const byHost = createMemo(() => {
    const groups = new Map<string, ChannelSummary[]>();
    for (const c of props.snapshot?.channels ?? []) {
      if (!PERSONAL.has(c.kind)) continue;
      const host = c.host || "this instance";
      groups.set(host, [...(groups.get(host) ?? []), c]);
    }
    return [...groups.entries()];
  });
  const sections = createMemo(() =>
    byHost().map(([host, channels]) => ({
      title: host,
      items: channels.map((c): ChannelItem => {
        const others = c.participantAccountIds.filter((id) => id !== me());
        const name = conversationName(c, me(), nameOf);
        return {
          name,
          kind: others.length > 1 ? "group" : "dm",
          initials: initialsOf(name),
          tone: toneFor(others[0] ?? c.channelId),
          people: others.map((id) => ({ name: nameOf(id), initials: initialsOf(nameOf(id)), tone: toneFor(id) })),
          active: props.view === "dm" && c.channelId === props.channelId,
          onClick: () => props.go("dm", c.channelId),
        };
      }),
    })),
  );
  const pending = () => props.snapshot?.pendingRequests.length ?? 0;
  return (
    <>
      <ServerHeader name="Home" onNewMessage={() => props.go("new")} />
      <QuickTiles
        items={[
          { icon: "mention", label: "Requests", count: pending() || undefined, onClick: () => props.go("requests") },
          { icon: "shield", label: "Devices", onClick: () => props.go("devices") },
          { icon: "compose", label: "New", onClick: () => props.go("new") },
        ]}
      />
      <Show when={sections().length > 0} fallback={<p class="app__empty app__empty--side">No conversations yet.</p>}>
        <ChannelList sections={sections()} />
      </Show>
    </>
  );
}

export function HomeMain(props: HomeProps) {
  const identity = hasIdentityQuery();
  return (
    <Show when={identity.data !== false} fallback={<NoIdentity toLink={props.toLink} />}>
      {props.view === "requests" ? <Requests {...props} /> : props.view === "devices" ? <Devices {...props} /> : props.view === "new" ? <NewMessage {...props} /> : <Conversation {...props} />}
    </Show>
  );
}

function NoIdentity(props: { toLink: () => void }) {
  return (
    <>
      <ChannelHeader kind="page" name="Home" actions={[]} />
      <div class="lu-page">
        <EmptyState icon="shield" title="Link this browser to your identity" action={{ label: "Link this browser", onClick: props.toLink }}>
          Personal messages are end-to-end encrypted between your devices and theirs. This browser has no device key yet; a device you already use approves it.
        </EmptyState>
      </div>
    </>
  );
}

function Conversation(props: HomeProps) {
  const { nameOf } = useNames(props);
  const me = () => props.snapshot?.accountId ?? "";
  const channel = createMemo(() => props.snapshot?.channels.find((c) => c.channelId === props.channelId));
  const dms = dmChannelsQuery(() => true);
  const state = createMemo(() => dms.data?.find((d) => d.channelId === props.channelId));
  const log = dmMessagesQuery(() => props.channelId);
  const [draft, setDraft] = createSignal("");
  const [error, setError] = createSignal("");
  const title = createMemo(() => (channel() ? conversationName(channel() as ChannelSummary, me(), nameOf) : "Conversation"));
  const participants = createMemo(() => (channel()?.participantAccountIds ?? []).map((id) => ({ id, name: nameOf(id) })));

  const send = async () => {
    const id = props.channelId;
    const body = draft().trim();
    if (!id || !body) return;
    setDraft("");
    setError("");
    try {
      await client().dmSend(id, body);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };
  const rejoin = async () => {
    if (!props.channelId) return;
    setError("");
    try {
      await client().dmRejoin(props.channelId);
      void queryClient.invalidateQueries({ queryKey: ["dmChannels"] });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <Show when={channel()} fallback={<ChooseConversation {...props} />}>
      {(ch) => (
        <>
          <ChannelHeader kind="dm" name={title()} topic={ch().host ? `on ${ch().host}` : undefined} actions={[]} />
          <MessageList>
            <Notice icon="shield">
              Messages here are end-to-end encrypted. Only your devices and those of {participants().filter((p) => p.id !== me()).map((p) => p.name).join(", ") || "the other participants"} can read them.
            </Notice>
            <Show when={state() && !state()?.member}>
              <Notice icon="lock" tone="accent" action={{ label: "Rejoin", onClick: () => void rejoin() }}>
                This device is not in the conversation's group yet. The device that created it sends a welcome; if that never comes, rejoin from the instance's group info.
              </Notice>
            </Show>
            <Show when={(log.data?.beforeJoin ?? 0) > 0}>
              <Notice icon="lock">
                {log.data?.beforeJoin} earlier {log.data?.beforeJoin === 1 ? "message was" : "messages were"} sent before this device joined and cannot be read here.
              </Notice>
            </Show>
            <Show when={(log.data?.undecryptable ?? 0) > 0}>
              <Notice icon="lock">{log.data?.undecryptable} could not be decrypted.</Notice>
            </Show>
            <Show when={(log.data?.messages.length ?? 0) > 0}>
              <Divider variant="day" label="Messages" />
            </Show>
            <For each={log.data?.messages ?? []}>
              {(m: DmMessageSummary) => {
                // A one-to-one conversation has one other sender, named even
                // when the engine has not yet tied the identity to an account.
                const others = () => participants().filter((p) => p.id !== me());
                const who = () => (m.mine ? nameOf(me()) : m.senderAccountId || others().length !== 1 ? nameOf(m.senderAccountId) : (others()[0]?.name ?? "Someone"));
                const t = timeOf(m.sentAtMicros);
                return (
                  <Message author={who()} initials={initialsOf(who())} tone={toneFor(m.mine ? me() : m.senderAccountId || m.senderIdentityId)} time={t.time} datetime={t.datetime} actions={false}>
                    {m.deleted ? "(deleted)" : m.body}
                  </Message>
                );
              }}
            </For>
            <Show when={error()}>
              <p class="auth__error" role="alert">
                {error()}
              </p>
            </Show>
          </MessageList>
          <Composer
            placeholder={`Message ${title()}`}
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
  );
}

function ChooseConversation(props: HomeProps) {
  return (
    <>
      <ChannelHeader kind="page" name="Home" actions={[]} />
      <div class="lu-page">
        <EmptyState icon="lock" title="Your conversations" action={{ label: "New message", onClick: () => props.go("new") }}>
          Personal messages are end-to-end encrypted. Pick one on the left, or start one with someone you share a server with.
        </EmptyState>
      </div>
    </>
  );
}

export function ConversationAside(props: HomeProps) {
  const { nameOf } = useNames(props);
  const me = () => props.snapshot?.accountId ?? "";
  const channel = createMemo(() => props.snapshot?.channels.find((c) => c.channelId === props.channelId));
  return (
    <Show when={channel()}>
      {(ch) => (
        <SidePanel tabs={[{ id: "details", label: "Details" }]} value="details" onChange={() => {}}>
          <MemberList
            sections={[
              {
                title: "In this conversation",
                members: ch().participantAccountIds.map((id) => ({
                  name: nameOf(id),
                  initials: initialsOf(nameOf(id)),
                  tone: toneFor(id),
                  subtitle: id === me() ? "you" : undefined,
                  tag: id === me() ? "You" : undefined,
                })),
              },
            ]}
          />
        </SidePanel>
      )}
    </Show>
  );
}

function Requests(props: HomeProps) {
  const { nameOf, people } = useNames(props);
  const [busy, setBusy] = createSignal<string | null>(null);
  const [error, setError] = createSignal("");
  const respond = async (requestId: string, response: "accept" | "decline" | "block") => {
    setBusy(requestId);
    setError("");
    try {
      const r = await client().dmRespond(requestId, response);
      void queryClient.invalidateQueries({ queryKey: ["snapshot"] });
      void queryClient.invalidateQueries({ queryKey: ["dmChannels"] });
      if (response === "accept" && "channelId" in r) props.go("dm", r.channelId);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  };
  const requests = () => props.snapshot?.pendingRequests ?? [];
  const meta = (r: { sharedServerIds: string[]; mutualFriendCount: number }) => {
    const parts: string[] = [];
    if (r.sharedServerIds.length) parts.push(`Shares ${r.sharedServerIds.length === 1 ? "a server" : r.sharedServerIds.length + " servers"} with you`);
    if (r.mutualFriendCount) parts.push(`${r.mutualFriendCount} mutual ${r.mutualFriendCount === 1 ? "friend" : "friends"}`);
    return parts.join(" · ") || "Nothing in common yet";
  };
  return (
    <>
      <ChannelHeader kind="page" name="Requests" topic="People who want to message you" actions={[]} />
      <div class="lu-page">
        <Notice icon="shield">A request shows who is asking and what you share, nothing else. Accepting opens an encrypted conversation; the sender cannot see that you looked.</Notice>
        <Show when={error()}>
          <p class="auth__error" role="alert">
            {error()}
          </p>
        </Show>
        <Show when={requests().length > 0} fallback={<EmptyState icon="mention" title="Nothing waiting">Declined requests disappear; blocked people cannot ask again.</EmptyState>}>
          <For each={requests()}>
            {(r) => (
              <RequestCard
                name={nameOf(r.fromAccountId)}
                handle={(people.data?.get(r.fromAccountId) ?? profileOf(r.fromAccountId))?.handle}
                initials={initialsOf(nameOf(r.fromAccountId))}
                tone={toneFor(r.fromAccountId)}
                meta={meta(r)}
                busy={busy() === r.requestId}
                onAccept={() => void respond(r.requestId, "accept")}
                onDecline={() => void respond(r.requestId, "decline")}
                onBlock={() => void respond(r.requestId, "block")}
              />
            )}
          </For>
        </Show>
      </div>
    </>
  );
}

function NewMessage(props: HomeProps) {
  const serverIds = createMemo(() => (props.snapshot?.servers ?? []).map((s) => s.serverId));
  const people = peopleQuery(serverIds);
  const [query, setQuery] = createSignal("");
  const [note, setNote] = createSignal("");
  const [busy, setBusy] = createSignal<string | null>(null);
  const me = () => props.snapshot?.accountId ?? "";
  const list = createMemo(() => {
    const q = query().trim().toLowerCase();
    return [...(people.data?.values() ?? [])]
      .filter((p) => p.accountId !== me())
      .filter((p) => !q || p.handle.toLowerCase().includes(q) || p.displayName.toLowerCase().includes(q) || p.nickname.toLowerCase().includes(q))
      .sort((a, b) => (a.displayName || a.handle).localeCompare(b.displayName || b.handle));
  });
  const message = async (p: MemberSummary) => {
    setBusy(p.accountId);
    setNote("");
    try {
      const opened = await client().dmOpen([p.accountId]);
      void queryClient.invalidateQueries({ queryKey: ["snapshot"] });
      void queryClient.invalidateQueries({ queryKey: ["dmChannels"] });
      props.go("dm", opened.channelId);
    } catch (e) {
      const ce = e instanceof ClientError ? e : null;
      if (ce?.code === "FAILED_PRECONDITION") {
        try {
          await client().dmRequest(p.accountId);
          setNote(`Request sent to ${p.displayName || p.handle}. The conversation opens when they accept.`);
        } catch (err) {
          const ce2 = err instanceof ClientError ? err : null;
          setNote(ce2?.code === "ALREADY_EXISTS" ? "You already asked; waiting for their answer." : err instanceof Error ? err.message : String(err));
        }
      } else {
        setNote(e instanceof Error ? e.message : String(e));
      }
    } finally {
      setBusy(null);
    }
  };
  return (
    <>
      <ChannelHeader kind="page" name="New message" topic="Someone you share a server with" actions={[]} />
      <div class="lu-page">
        <SearchField label="Find a person" placeholder="Name or handle" value={query()} onInput={(e) => setQuery(e.currentTarget.value)} />
        <Show when={note()}>
          <Notice icon="mention">{note()}</Notice>
        </Show>
        <Show when={list().length > 0} fallback={<EmptyState icon="mention" title={people.isLoading ? "Looking…" : "No one to show"}>Join a server to find people to message.</EmptyState>}>
          <ul class="lu-members__list app__people">
            <For each={list()}>
              {(p) => (
                <li class="lu-member app__person">
                  <Avatar initials={initialsOf(p.displayName || p.handle)} tone={toneFor(p.accountId)} size={28} />
                  <span class="lu-member__text">
                    <span class="lu-member__name">{p.displayName || p.handle}</span>
                    <span class="lu-member__sub">@{p.handle}</span>
                  </span>
                  <Button size="sm" disabled={busy() === p.accountId} onClick={() => void message(p)}>
                    Message
                  </Button>
                </li>
              )}
            </For>
          </ul>
        </Show>
      </div>
    </>
  );
}

function Devices(props: HomeProps) {
  const roster = rosterQuery(() => true);
  const [revoking, setRevoking] = createSignal<DeviceSummary | null>(null);
  const [password, setPassword] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const label = (d: DeviceSummary) => (d.thisDevice ? "This browser" : d.kind === "web" ? "Browser" : d.kind === "native" ? "Computer" : d.kind === "mobile" ? "Phone" : d.kind || "Device");
  const meta = (d: DeviceSummary) => {
    const parts: string[] = [];
    if (d.webOrigin) parts.push(d.webOrigin);
    if (d.addedAtMicros) parts.push("added " + new Date(d.addedAtMicros / 1000).toLocaleDateString(undefined, { month: "short", day: "numeric" }));
    parts.push(d.deviceId.slice(0, 8));
    return parts.join(" · ");
  };
  const entries = createMemo<DeviceEntry[]>(() =>
    (roster.data?.devices ?? []).filter((d) => !d.revoked).map((d) => ({ id: d.deviceId, kind: d.kind, label: label(d), meta: meta(d), thisDevice: d.thisDevice })),
  );
  const revoked = createMemo<DeviceEntry[]>(() =>
    (roster.data?.devices ?? []).filter((d) => d.revoked).map((d) => ({ id: d.deviceId, kind: d.kind, label: d.deviceId.slice(0, 8), revoked: true })),
  );
  const revoke = async (e: Event) => {
    e.preventDefault();
    const target = revoking();
    if (!target) return;
    setBusy(true);
    setError("");
    try {
      const updated = await client().identityRevoke(target.deviceId, password());
      queryClient.setQueryData(["roster"], updated);
      setRevoking(null);
      setPassword("");
    } catch (err) {
      const ce = err instanceof ClientError ? err : null;
      setError(ce?.code === "UNAUTHENTICATED" ? "That password was not accepted." : err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };
  return (
    <>
      <ChannelHeader kind="page" name="Devices" topic="Everything that holds your identity" actions={[]} />
      <div class="lu-page">
        <Notice icon="shield" action={{ label: "Link a device", onClick: props.toLink }}>
          Your identity lives on these devices, not on any instance. Revoking one takes effect on every instance that holds your roster.
        </Notice>
        <Show when={roster.data} fallback={<EmptyState icon="shield" title={roster.isLoading ? "Looking…" : "No identity on this device"} />}>
          <DeviceList devices={entries()} onRevoke={(d) => setRevoking(roster.data?.devices.find((x) => x.deviceId === d.id) ?? null)} />
        </Show>
        <Show when={revoking()}>
          {(d) => (
            <Card title={`Revoke ${label(d())}?`} subtitle="It stops reading new messages the moment every instance has the change. Confirm with your password.">
              <form class="auth__form" onSubmit={(e) => void revoke(e)}>
                <TextField label="Password" type="password" reveal value={password()} onInput={(e) => setPassword(e.currentTarget.value)} autocomplete="current-password" placeholder="Your password" />
                <Show when={error()}>
                  <p class="auth__error" role="alert">
                    {error()}
                  </p>
                </Show>
                <div class="app__row">
                  <Button variant="primary" size="md" type="submit" disabled={busy() || !password()}>
                    Revoke {label(d())}
                  </Button>
                  <Button size="md" type="button" onClick={() => setRevoking(null)}>
                    Keep it
                  </Button>
                </div>
              </form>
            </Card>
          )}
        </Show>
        <Show when={revoked().length > 0}>
          <p class="lu-page__title">Revoked</p>
          <DeviceList devices={revoked()} />
        </Show>
      </div>
    </>
  );
}
