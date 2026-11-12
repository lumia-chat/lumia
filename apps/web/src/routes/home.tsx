// Home: personal DMs grouped by the instance that hosts them, message
// requests, this identity's devices, and starting a conversation. From
// the "Home", "Requests" and "Devices" boards in the gallery.
import { For, Show, createEffect, createMemo, createSignal } from "solid-js";
import {
  Avatar,
  Button,
  Card,
  ChannelHeader,
  Checkbox,
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
  SidePanelScroll,
  SegmentedTabs,
  TextField,
  type ChannelItem,
  type DeviceEntry,
} from "../design/components";
import { ClientError, type ChannelSummary, type DeviceSummary, type DmMessageSummary, type InvitePreviewSummary, type MemberSummary, type PasskeySummary, type ProfileSummary, type SnapshotSummary } from "../client/bridge";
import { SERVED_BY_INSTANCE, avatarSrc, client, savedInstance, dmChannelsQuery, dmMessagesQuery, enableNotifications, hasIdentityQuery, myStatus, peopleQuery, pushState, queryClient, rosterQuery, setMyStatus, snapshotQuery } from "../client/state";
import { useNames } from "../client/names";
import { addPasskey, passkeysAvailable } from "../client/passkeys";
import { byBlock, byDay, initialsOf, shortTimeOf, timeOf, toneFor } from "./app";

export type HomeView = "dm" | "requests" | "devices" | "profile" | "new" | "add";

export interface HomeProps {
  snapshot: SnapshotSummary | undefined;
  view: HomeView;
  channelId: string | undefined;
  go: (view: HomeView, channelId?: string) => void;
  toLink: () => void;
  toServer: (serverId: string) => void;
  /** Back to the rail and the sidebar, where the frame is folded. */
  back: () => void;
  /** The aside over the conversation, where the window is too narrow for it in the row. */
  asideOpen: boolean;
  toggleAside: () => void;
  /** The profile card of an account, against what was pressed. */
  openPerson: (accountId: string, anchor: HTMLElement) => void;
}

/**
 * Starts a conversation with someone, or asks for one where they have to
 * agree first: the conversation's channel, or a line saying what happened
 * instead. Shared by "New message" and the profile card's button.
 */
export async function messagePerson(accountId: string, name: string): Promise<{ channelId: string } | { note: string }> {
  try {
    const opened = await client().dmOpen([accountId]);
    void queryClient.invalidateQueries({ queryKey: ["snapshot"] });
    void queryClient.invalidateQueries({ queryKey: ["dmChannels"] });
    return { channelId: opened.channelId };
  } catch (e) {
    const ce = e instanceof ClientError ? e : null;
    if (ce?.code !== "FAILED_PRECONDITION") return { note: e instanceof Error ? e.message : String(e) };
    try {
      await client().dmRequest(accountId);
      return { note: `Request sent to ${name}. The conversation opens when they accept.` };
    } catch (err) {
      const ce2 = err instanceof ClientError ? err : null;
      return { note: ce2?.code === "ALREADY_EXISTS" ? "You already asked; waiting for their answer." : err instanceof Error ? err.message : String(err) };
    }
  }
}

const PERSONAL = new Set(["PERSONAL_DM", "PERSONAL_GROUP_DM"]);

function conversationName(c: ChannelSummary, me: string, nameOf: (id: string) => string): string {
  const others = c.participantAccountIds.filter((id) => id !== me);
  if (others.length === 0) return "Just you";
  return others.map((id) => nameOf(id)).join(", ");
}

export function HomeSidebar(props: HomeProps) {
  const { nameOf, photoOf } = useNames(() => props.snapshot);
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
          src: others.length === 1 ? photoOf(others[0] ?? "") : undefined,
          tone: toneFor(others[0] ?? c.channelId),
          people: others.map((id) => ({ name: nameOf(id), initials: initialsOf(nameOf(id)), src: photoOf(id), tone: toneFor(id) })),
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
      <Button size="sm" block class="app__foot" onClick={() => props.go("add")}>
        Add a server
      </Button>
    </>
  );
}

// A server of your own, or joining one from an invite link or code: the
// preview first (what the link leads to, on which instance), then the
// join (architecture 6.1).
function AddServer(props: HomeProps) {
  const [name, setName] = createSignal("");
  const [creating, setCreating] = createSignal(false);
  const [createError, setCreateError] = createSignal("");
  const create = async (e: Event) => {
    e.preventDefault();
    if (!name().trim()) return;
    setCreating(true);
    setCreateError("");
    try {
      const server = await client().createServer(name());
      await queryClient.invalidateQueries({ queryKey: ["snapshot"] });
      props.toServer(server.serverId);
    } catch (err) {
      setCreateError(err instanceof Error ? err.message : String(err));
    } finally {
      setCreating(false);
    }
  };
  const [text, setText] = createSignal("");
  const [preview, setPreview] = createSignal<InvitePreviewSummary | null>(null);
  const [error, setError] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const look = async (e: Event) => {
    e.preventDefault();
    if (!text().trim()) return;
    setBusy(true);
    setError("");
    setPreview(null);
    try {
      setPreview(await client().invitePreview(text()));
    } catch (err) {
      const ce = err instanceof ClientError ? err : null;
      setError(ce?.code === "NOT_FOUND" ? "That invite is unknown, expired or used up." : err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };
  const join = async () => {
    const p = preview();
    if (!p) return;
    setBusy(true);
    setError("");
    try {
      const serverId = await client().acceptInvite(p.code);
      await queryClient.invalidateQueries({ queryKey: ["snapshot"] });
      props.toServer(serverId);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };
  return (
    <>
      <ChannelHeader kind="page" name="Add a server" topic="Your own, or one you were invited to" actions={[]} onBack={props.back} backLabel="Back to Home" />
      <div class="lu-page">
        <Card title="Create a server" subtitle="You own it; invite people once it exists.">
          <form class="auth__form" onSubmit={(e) => void create(e)}>
            <TextField label="Name" value={name()} onInput={(e) => setName(e.currentTarget.value)} placeholder="Our place" autocomplete="off" />
            <Show when={createError()}>
              <p class="auth__error" role="alert">
                {createError()}
              </p>
            </Show>
            <Button variant="primary" type="submit" block disabled={creating() || !name().trim()}>
              Create
            </Button>
          </form>
        </Card>
        <Card title="Join with an invite" subtitle="A link such as https://chat.example.com/invite/abc23xyz, or just its code.">
          <form class="auth__form" onSubmit={(e) => void look(e)}>
            <TextField label="Invite link or code" value={text()} onInput={(e) => setText(e.currentTarget.value)} placeholder="https://chat.example.com/invite/…" autocomplete="off" />
            <Show when={error()}>
              <p class="auth__error" role="alert">
                {error()}
              </p>
            </Show>
            <Button variant="primary" type="submit" block disabled={busy() || !text().trim()}>
              Look it up
            </Button>
          </form>
        </Card>
        <Show when={preview()}>
          {(p) => (
            <Card title={p().serverName} subtitle={`${p().memberCount} ${p().memberCount === 1 ? "member" : "members"} on ${p().host}${p().asGuest ? " · you would join as a guest" : ""}`}>
              <div class="auth__form">
                <p class="auth__note">Run by {p().operatorName || "an unnamed operator"}. Your account on this instance joins it; nothing else is shared.</p>
                <Button variant="primary" type="button" block disabled={busy()} onClick={() => void join()}>
                  Join {p().serverName}
                </Button>
              </div>
            </Card>
          )}
        </Show>
      </div>
    </>
  );
}

export function HomeMain(props: HomeProps) {
  const identity = hasIdentityQuery();
  return (
    <Show when={props.view !== "add" && props.view !== "devices" && props.view !== "profile"} fallback={props.view === "add" ? <AddServer {...props} /> : props.view === "profile" ? <Profile {...props} /> : <Devices {...props} />}>
      <Show when={identity.data !== false} fallback={<NoIdentity toLink={props.toLink} back={props.back} />}>
        {props.view === "requests" ? <Requests {...props} /> : props.view === "new" ? <NewMessage {...props} /> : <Conversation {...props} />}
      </Show>
    </Show>
  );
}

function NoIdentity(props: { toLink: () => void; back: () => void }) {
  return (
    <>
      <ChannelHeader kind="page" name="Home" actions={[]} onBack={props.back} backLabel="Back to Home" />
      <div class="lu-page">
        <EmptyState icon="shield" title="Link this browser to your identity" action={{ label: "Link this browser", onClick: props.toLink }}>
          Personal messages are end-to-end encrypted between your devices and theirs. This browser has no device key yet; a device you already use approves it.
        </EmptyState>
      </div>
    </>
  );
}

function Conversation(props: HomeProps) {
  const { nameOf, photoOf } = useNames(() => props.snapshot);
  const me = () => props.snapshot?.accountId ?? "";
  const channel = createMemo(() => props.snapshot?.channels.find((c) => c.channelId === props.channelId));
  const dms = dmChannelsQuery(() => true);
  const state = createMemo(() => dms.data?.find((d) => d.channelId === props.channelId));
  const log = dmMessagesQuery(() => props.channelId);
  const [draft, setDraft] = createSignal("");
  const [error, setError] = createSignal("");
  const title = createMemo(() => (channel() ? conversationName(channel() as ChannelSummary, me(), nameOf) : "Conversation"));
  const participants = createMemo(() => (channel()?.participantAccountIds ?? []).map((id) => ({ id, name: nameOf(id) })));
  // A one-to-one conversation has one other sender, named even when the
  // engine has not yet tied the identity to an account.
  const others = () => participants().filter((p) => p.id !== me());
  const who = (m: DmMessageSummary) => (m.mine ? nameOf(me()) : m.senderAccountId || others().length !== 1 ? nameOf(m.senderAccountId) : (others()[0]?.name ?? "Someone"));
  // By day, and each day's by block: one person's quick messages under one name.
  const days = createMemo(() =>
    byDay(log.data?.messages ?? [], (m) => m.sentAtMicros).map((day) => ({
      label: day.label,
      blocks: byBlock(
        day.messages,
        (m) => (m.mine ? me() : m.senderAccountId || m.senderIdentityId),
        (m) => m.sentAtMicros,
      ),
    })),
  );

  const send = async () => {
    const id = props.channelId;
    const body = draft().trim();
    if (!id || !body) return;
    setDraft("");
    setError("");
    try {
      await client().dmSend(id, body);
    } catch (e) {
      // The text comes back to the composer unless something new was typed meanwhile.
      setDraft((d) => d || body);
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
          <ChannelHeader kind="dm" name={title()} topic={ch().host ? `on ${ch().host}` : undefined} onBack={props.back} backLabel="Back to Home" actions={[{ icon: "members", label: "Details", narrow: true, pressed: props.asideOpen, onClick: props.toggleAside }]} />
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
            <For each={days()}>
              {(day) => (
                <>
                  <Divider variant="day" label={day.label} />
                  <For each={day.blocks}>
                    {(block) => {
                      const first = block.first;
                      const t = timeOf(first.sentAtMicros);
                      const said = (m: DmMessageSummary) => (m.deleted ? "(deleted)" : m.body);
                      const account = () => (first.mine ? me() : first.senderAccountId);
                      return (
                        <Message
                          author={who(first)}
                          initials={initialsOf(who(first))}
                          src={account() ? photoOf(account()) : undefined}
                          tone={toneFor(first.mine ? me() : first.senderAccountId || first.senderIdentityId)}
                          time={t.time}
                          datetime={t.datetime}
                          actions={false}
                          onOpenProfile={account() ? (anchor) => props.openPerson(account(), anchor) : undefined}
                          parts={block.rest.map((m) => ({ time: shortTimeOf(m.sentAtMicros), datetime: timeOf(m.sentAtMicros).datetime, children: said(m) }))}
                        >
                          {said(first)}
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
            placeholder={state()?.member ? `Message ${title()}` : "Setting up the encrypted group…"}
            tools={false}
            disabled={!state()?.member}
            value={draft()}
            onInput={(e) => setDraft(e.currentTarget.value)}
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
  );
}

function ChooseConversation(props: HomeProps) {
  return (
    <>
      <ChannelHeader kind="page" name="Home" actions={[]} onBack={props.back} backLabel="Back to Home" />
      <div class="lu-page">
        <EmptyState icon="lock" title="Your conversations" action={{ label: "New message", onClick: () => props.go("new") }}>
          Personal messages are end-to-end encrypted. Pick one on the left, or start one with someone you share a server with.
        </EmptyState>
      </div>
    </>
  );
}

export function ConversationAside(props: HomeProps) {
  const { nameOf, profile, photoOf } = useNames(() => props.snapshot);
  const me = () => props.snapshot?.accountId ?? "";
  const channel = createMemo(() => props.snapshot?.channels.find((c) => c.channelId === props.channelId));
  return (
    <Show when={channel()}>
      {(ch) => (
        <SidePanel tabs={[{ id: "details", label: "Details" }]} value="details">
          <SidePanelScroll>
          <MemberList
            onOpen={(m, row) => m.id && props.openPerson(m.id, row)}
            sections={[
              {
                title: "In this conversation",
                // Each person with the pronouns and bio of their verified profile.
                members: ch().participantAccountIds.map((id) => ({
                  id,
                  name: nameOf(id),
                  initials: initialsOf(nameOf(id)),
                  src: photoOf(id),
                  tone: toneFor(id),
                  subtitle: [id === me() ? "you" : "", profile(id)?.pronouns ?? ""].filter(Boolean).join(" · ") || undefined,
                  detail: profile(id)?.bio || undefined,
                  tag: id === me() ? "You" : undefined,
                })),
              },
            ]}
          />
          </SidePanelScroll>
        </SidePanel>
      )}
    </Show>
  );
}

function Requests(props: HomeProps) {
  const { nameOf, profile, photoOf } = useNames(() => props.snapshot);
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
      <ChannelHeader kind="page" name="Requests" topic="People who want to message you" actions={[]} onBack={props.back} backLabel="Back to Home" />
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
                handle={profile(r.fromAccountId)?.handle}
                initials={initialsOf(nameOf(r.fromAccountId))}
                src={photoOf(r.fromAccountId)}
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
      const outcome = await messagePerson(p.accountId, p.displayName || p.handle);
      if ("channelId" in outcome) props.go("dm", outcome.channelId);
      else setNote(outcome.note);
    } finally {
      setBusy(null);
    }
  };
  return (
    <>
      <ChannelHeader kind="page" name="New message" topic="Someone you share a server with" actions={[]} onBack={props.back} backLabel="Back to Home" />
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
                  <Avatar initials={initialsOf(p.displayName || p.handle)} src={avatarSrc(p.avatarUrl)} tone={toneFor(p.accountId)} size={28} />
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

// The person's own profile (architecture 6.4): the signed profile the
// identity carries, display name, pronouns and bio, signed by this
// browser's device key and sent to the instance as the account's. The
// same screen on every client.
function Profile(props: HomeProps) {
  const snapshot = snapshotQuery();
  const hasIdentity = hasIdentityQuery();
  const [profile, setProfile] = createSignal<ProfileSummary | null>(null);
  const [displayName, setDisplayName] = createSignal("");
  const [pronouns, setPronouns] = createSignal("");
  const [bio, setBio] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const [saved, setSaved] = createSignal(false);
  // The account on this instance (protocol 9, `PATCH /account`): its
  // handle, and its display name where this browser holds no identity (one
  // that does keeps the name in the signed profile below). The fields take
  // the snapshot's values once, not again over what is being typed.
  const [handle, setHandle] = createSignal("");
  const [accountName, setAccountName] = createSignal("");
  const [accountBusy, setAccountBusy] = createSignal(false);
  const [accountError, setAccountError] = createSignal("");
  const [accountSaved, setAccountSaved] = createSignal(false);
  let seeded = false;
  createEffect(() => {
    const snap = snapshot.data;
    if (!snap || seeded) return;
    seeded = true;
    setHandle(snap.handle);
    setAccountName(snap.displayName);
  });
  const saveAccount = async (e: Event) => {
    e.preventDefault();
    setAccountBusy(true);
    setAccountError("");
    setAccountSaved(false);
    try {
      const wanted = handle().trim();
      const snap = await client().updateAccount(wanted === snapshot.data?.handle ? undefined : wanted, hasIdentity.data === false ? accountName().trim() : undefined);
      queryClient.setQueryData(["snapshot"], snap);
      setHandle(snap.handle);
      setAccountSaved(true);
    } catch (err) {
      const ce = err instanceof ClientError ? err : null;
      setAccountError(
        ce?.code === "ALREADY_EXISTS"
          ? "That handle is taken."
          : ce?.code === "INVALID_ARGUMENT"
            ? "A handle is 2 to 32 letters, digits, _, . or -, and a display name at most 64 characters."
            : err instanceof Error
              ? err.message
              : String(err),
      );
    } finally {
      setAccountBusy(false);
    }
  };
  // The instance as a person names it: its address without the scheme.
  const instanceAddress = () => {
    try {
      return SERVED_BY_INSTANCE ? location.host : new URL(savedInstance() ?? "").host;
    } catch {
      return "";
    }
  };
  // The account's photo: what it is now, a way to choose another, and a way
  // to have none. The instance makes its own square of the file
  // (architecture 12), so nothing of the file is passed on but what it shows.
  let photoInput: HTMLInputElement | undefined;
  const [photoBusy, setPhotoBusy] = createSignal(false);
  const [photoError, setPhotoError] = createSignal("");
  const [photoSaved, setPhotoSaved] = createSignal(false);
  const photo = () => avatarSrc(snapshot.data?.avatarUrl);
  const selfName = () => snapshot.data?.displayName || snapshot.data?.handle || "You";
  const photoFailed = (err: unknown) => {
    const ce = err instanceof ClientError ? err : null;
    setPhotoError(
      ce?.code === "INVALID_ARGUMENT" && ce.details.reason === "too_large"
        ? "That picture is too large. Choose one under 8 MB."
        : ce?.code === "INVALID_ARGUMENT" && ce.details.reason === "unsupported"
          ? "That file is not a picture Lumia can read. Choose a PNG, JPEG, WebP or GIF."
          : ce?.code === "INVALID_ARGUMENT"
            ? "That picture could not be read."
            : ce?.code === "FAILED_PRECONDITION"
              ? "This instance does not keep pictures."
              : ce?.code === "RATE_LIMITED"
                ? "The instance is busy. Try again in a moment."
                : err instanceof Error
                  ? err.message
                  : String(err),
    );
  };
  // The snapshot the call answers with is the account as it now is; the
  // lists that name this account's picture are read again.
  const photoChanged = (snap: SnapshotSummary) => {
    queryClient.setQueryData(["snapshot"], snap);
    void queryClient.invalidateQueries({ queryKey: ["people"] });
    void queryClient.invalidateQueries({ queryKey: ["members"] });
    setPhotoSaved(true);
  };
  const choosePhoto = async (file: File | undefined) => {
    if (!file) return;
    setPhotoError("");
    setPhotoSaved(false);
    // What the instance would refuse for its size is not sent at all.
    if (file.size > 8 * 1024 * 1024) {
      setPhotoError("That picture is too large. Choose one under 8 MB.");
      return;
    }
    setPhotoBusy(true);
    try {
      photoChanged(await client().setAvatar(new Uint8Array(await file.arrayBuffer()), file.type));
    } catch (err) {
      photoFailed(err);
    } finally {
      setPhotoBusy(false);
    }
  };
  const removePhoto = async () => {
    setPhotoError("");
    setPhotoSaved(false);
    setPhotoBusy(true);
    try {
      photoChanged(await client().removeAvatar());
    } catch (err) {
      photoFailed(err);
    } finally {
      setPhotoBusy(false);
    }
  };
  // The status (protocol 6.7): chosen here, kept by the core, sent with the
  // servers watched and on every welcome.
  const [status, setStatus] = createSignal(myStatus().status);
  const [customStatus, setCustomStatus] = createSignal(myStatus().customStatus);
  const [statusSaved, setStatusSaved] = createSignal(false);
  void client()
    .status()
    .then((s) => {
      setStatus(s.status);
      setCustomStatus(s.customStatus);
    })
    .catch(() => {});
  const saveStatus = async (e: Event) => {
    e.preventDefault();
    setStatusSaved(false);
    try {
      await setMyStatus(status(), customStatus().trim());
      setStatusSaved(true);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };
  // The form waits for the profile held: fields shown before it lands
  // would be overwritten by it, typing and all.
  const [loaded, setLoaded] = createSignal(false);
  void (async () => {
    try {
      const p = await client().identityProfile();
      setProfile(p);
      setDisplayName(p?.displayName || "");
      setPronouns(p?.pronouns ?? "");
      setBio(p?.bio ?? "");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoaded(true);
    }
  })();
  const save = async (e: Event) => {
    e.preventDefault();
    setBusy(true);
    setError("");
    setSaved(false);
    try {
      const p = await client().identitySetProfile(displayName().trim(), pronouns().trim(), bio().trim(), profile()?.avatarUrl ?? "");
      setProfile(p);
      setSaved(true);
      void queryClient.invalidateQueries({ queryKey: ["snapshot"] });
    } catch (err) {
      const ce = err instanceof ClientError ? err : null;
      setError(
        ce?.code === "FAILED_PRECONDITION"
          ? "This account is not linked to your identity, so the instance keeps the profile it has."
          : ce?.code === "CONFLICT"
            ? "A newer profile is already held. Reload and try again."
            : err instanceof Error
              ? err.message
              : String(err),
      );
    } finally {
      setBusy(false);
    }
  };
  return (
    <>
      <ChannelHeader kind="page" name="Profile" topic="How people see you" actions={[]} onBack={props.back} backLabel="Back to Home" />
      <div class="lu-page">
        <Card title="Account" subtitle={`Who you are on ${instanceAddress() || "this instance"}.`}>
          <div class="app__photo">
            <Avatar initials={initialsOf(selfName())} src={photo()} tone={toneFor(snapshot.data?.accountId ?? "")} size={44} />
            <div class="app__photo-words">
              <div class="app__row">
                <input
                  ref={photoInput}
                  type="file"
                  accept="image/png,image/jpeg,image/webp,image/gif"
                  class="lu-sr"
                  tabindex={-1}
                  aria-label="Photo"
                  onChange={(e) => {
                    const file = e.currentTarget.files?.[0];
                    // The same file chosen again is a change too.
                    e.currentTarget.value = "";
                    void choosePhoto(file);
                  }}
                />
                <Button size="sm" disabled={photoBusy()} onClick={() => photoInput?.click()}>
                  {photoBusy() ? "Sending…" : photo() ? "Change photo…" : "Choose a photo…"}
                </Button>
                <Show when={photo()}>
                  <Button size="sm" disabled={photoBusy()} onClick={() => void removePhoto()}>
                    Remove
                  </Button>
                </Show>
              </div>
              <Show when={photoError()} fallback={<p class="auth__note" role="status">{photoSaved() ? "Saved." : "A PNG, JPEG, WebP or GIF up to 8 MB. People see its middle, as a circle."}</p>}>
                <p class="auth__error" role="alert">
                  {photoError()}
                </p>
              </Show>
            </div>
          </div>
          <form class="auth__form" onSubmit={(e) => void saveAccount(e)}>
            <TextField label="Handle" value={handle()} onInput={(e) => setHandle(e.currentTarget.value)} placeholder="yourname" autocomplete="username" autocapitalize="off" spellcheck={false} />
            <Show when={hasIdentity.data === false}>
              <TextField label="Display name" value={accountName()} onInput={(e) => setAccountName(e.currentTarget.value)} placeholder="How people see you" autocomplete="nickname" />
            </Show>
            <Show when={accountError()}>
              <p class="auth__error" role="alert">
                {accountError()}
              </p>
            </Show>
            <Show when={accountSaved()}>
              <p class="auth__note" role="status">
                Saved.
              </p>
            </Show>
            <Button variant="primary" size="md" type="submit" disabled={accountBusy() || !handle().trim()}>
              Save
            </Button>
          </form>
        </Card>
        <Card title="Status" subtitle="What people see beside your name in a server's member list.">
          <form class="auth__form" onSubmit={(e) => void saveStatus(e)}>
            <SegmentedTabs label="Status" tabs={[{ id: "ONLINE", label: "Online" }, { id: "AWAY", label: "Away" }, { id: "DO_NOT_DISTURB", label: "Do not disturb" }]} value={status()} onChange={setStatus} />
            <TextField label="Custom status" value={customStatus()} onInput={(e) => setCustomStatus(e.currentTarget.value)} placeholder="At lunch" autocomplete="off" />
            <Show when={statusSaved()}>
              <p class="auth__note">Status saved.</p>
            </Show>
            <Button variant="primary" size="md" type="submit">
              Save status
            </Button>
          </form>
        </Card>
        <Show
          when={hasIdentity.data !== false}
          fallback={
            <Notice icon="shield" action={{ label: "Link this browser", onClick: props.toLink }}>
              Your profile travels with your identity, signed by its devices. This browser holds no device key yet; a device you already use approves it.
            </Notice>
          }
        >
          <Card title={snapshot.data?.handle ? `@${snapshot.data.handle}` : "You"} subtitle={!loaded() ? "Looking…" : profile() ? `Version ${profile()?.version}, signed by this browser's device key; every instance you sign in to takes it.` : "Nothing set yet: people see your handle."}>
            <Show when={loaded()}>
            <form class="auth__form" onSubmit={(e) => void save(e)}>
              <TextField label="Display name" value={displayName()} onInput={(e) => setDisplayName(e.currentTarget.value)} placeholder="How people see you" autocomplete="nickname" />
              <TextField label="Pronouns" value={pronouns()} onInput={(e) => setPronouns(e.currentTarget.value)} placeholder="they/them" autocomplete="off" />
              <TextField label="Bio" value={bio()} onInput={(e) => setBio(e.currentTarget.value)} placeholder="A line about you" autocomplete="off" />
              <Show when={error()}>
                <p class="auth__error" role="alert">
                  {error()}
                </p>
              </Show>
              <Show when={saved()}>
                <p class="auth__note">Saved. This instance shows it now.</p>
              </Show>
              <Button variant="primary" size="md" type="submit" disabled={busy()}>
                Save
              </Button>
            </form>
            </Show>
          </Card>
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
  // Passkeys (architecture 6.3): the account's sign-in keys, made here and removable.
  const [passkeys, setPasskeys] = createSignal<PasskeySummary[]>([]);
  const [passkeyLabel, setPasskeyLabel] = createSignal("");
  const [passkeyError, setPasskeyError] = createSignal("");
  const [passkeyBusy, setPasskeyBusy] = createSignal(false);
  // The passkey as the second factor of a password sign-in (protocol 6.1).
  const [passkeyRequired, setPasskeyRequiredState] = createSignal(false);
  const toggleRequired = async (required: boolean) => {
    setPasskeyError("");
    try {
      await client().setPasskeyRequired(required);
      setPasskeyRequiredState(required);
    } catch (e) {
      const ce = e instanceof ClientError ? e : null;
      setPasskeyError(ce?.code === "FAILED_PRECONDITION" ? "Add a passkey first." : e instanceof Error ? e.message : String(e));
    }
  };
  const loadPasskeys = async () => {
    try {
      setPasskeys(await client().passkeys());
      setPasskeyRequiredState(await client().passkeyRequired());
    } catch (e) {
      setPasskeyError(e instanceof Error ? e.message : String(e));
    }
  };
  void loadPasskeys();
  const makePasskey = async (e: Event) => {
    e.preventDefault();
    setPasskeyBusy(true);
    setPasskeyError("");
    try {
      await addPasskey(passkeyLabel().trim() || "Passkey");
      setPasskeyLabel("");
      await loadPasskeys();
    } catch (err) {
      const ce = err instanceof ClientError ? err : null;
      setPasskeyError(ce?.code === "ALREADY_EXISTS" ? "That passkey is already registered." : err instanceof Error && err.name === "NotAllowedError" ? "The passkey was not made." : err instanceof Error ? err.message : String(err));
    } finally {
      setPasskeyBusy(false);
    }
  };
  const removePasskey = async (p: PasskeySummary) => {
    setPasskeyError("");
    try {
      await client().removePasskey(p.passkeyId);
      await loadPasskeys();
    } catch (err) {
      setPasskeyError(err instanceof Error ? err.message : String(err));
    }
  };
  const when = (micros: number | null) => (micros ? new Date(micros / 1000).toLocaleDateString(undefined, { month: "short", day: "numeric" }) : "never");
  return (
    <>
      <ChannelHeader kind="page" name="Devices" topic="Everything that holds your identity" actions={[]} onBack={props.back} backLabel="Back to Home" />
      <div class="lu-page">
        <Show when={passkeysAvailable()}>
          <Card title="Passkeys" subtitle="Sign in to this instance with a key this browser or your phone keeps, instead of a password.">
            <For each={passkeys()}>
              {(p) => (
                <p class="app__line">
                  <span>
                    {p.label || "Passkey"} · added {when(p.createdAtMicros)} · last used {when(p.lastUsedAtMicros)}
                  </span>
                  <Button variant="text" aria-label={`Remove ${p.label || "passkey"}`} onClick={() => void removePasskey(p)}>
                    Remove
                  </Button>
                </p>
              )}
            </For>
            <Checkbox label="Require a passkey when signing in with a password" checked={passkeyRequired()} disabled={passkeys().length === 0} onChange={(e) => void toggleRequired(e.currentTarget.checked)} />
            <form class="auth__form" onSubmit={(e) => void makePasskey(e)}>
              <TextField label="Name for the new passkey" value={passkeyLabel()} onInput={(e) => setPasskeyLabel(e.currentTarget.value)} placeholder="This laptop" autocomplete="off" />
              <Show when={passkeyError()}>
                <p class="auth__error" role="alert">
                  {passkeyError()}
                </p>
              </Show>
              <Button variant="primary" size="md" type="submit" disabled={passkeyBusy()}>
                Add a passkey
              </Button>
            </form>
          </Card>
        </Show>
        <Notice icon="shield" action={{ label: "Link a device", onClick: props.toLink }}>
          Your identity lives on these devices, not on any instance. Revoking one takes effect on every instance that holds your roster.
        </Notice>
        <Show when={pushState() !== "unavailable"}>
          <Notice icon="mention" action={pushState() === "subscribed" ? undefined : { label: "Turn on notifications", onClick: () => void enableNotifications() }}>
            {pushState() === "subscribed"
              ? "This browser gets a notification for new personal messages and mentions while it has no live session. The notice says nothing more; the app fetches the message."
              : pushState() === "denied"
                ? "Notifications are blocked for this site in the browser's settings."
                : "This browser can get a notification for new personal messages and mentions while it has no live session. The notice says nothing more; the app fetches the message."}
          </Notice>
        </Show>
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
