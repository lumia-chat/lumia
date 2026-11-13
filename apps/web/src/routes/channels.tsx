// Being in channels (architecture 6.8), as the server's main pane and
// aside show it: Browse, this account's settings in the server, a
// channel's members beside it with "Add people", "Remove" and "Leave
// channel", and the default channels card of the server's settings.
// Worded as the desktop shell's and the phones'.
import { For, Show, createEffect, createMemo, createSignal, on } from "solid-js";
import { BrowseRow, Button, Card, ChannelHeader, Checkbox, EmptyState, MemberList, SearchField, SidePanel, SidePanelScroll, SidePanelSearch } from "../design/components";
import { ClientError, type ChannelSummary, type ServerSummary, type SnapshotSummary } from "../client/bridge";
import { avatarSrc, browseQuery, channelMembersQuery, client, customStatusOf, membersQuery, presence, presenceOf, queryClient, tookSnapshot } from "../client/state";
import { initialsOf, toneFor } from "./app";

/** Why something about a channel was refused. */
export function channelRefusal(err: unknown): string {
  const ce = err instanceof ClientError ? err : null;
  if (ce?.code === "PERMISSION_DENIED") return "You cannot do that in this channel.";
  if (ce?.code === "NOT_FOUND") return "That channel is gone, or you cannot see it.";
  return err instanceof Error ? err.message : String(err);
}

/** Why a server DM was not opened. */
export function dmRefusal(err: unknown): string {
  const ce = err instanceof ClientError ? err : null;
  if (ce?.code === "FAILED_PRECONDITION") return "They don’t take messages here.";
  if (ce?.code === "PERMISSION_DENIED") return "You cannot message people in this server.";
  if (ce?.code === "NOT_FOUND") return "You cannot message them here.";
  return err instanceof Error ? err.message : String(err);
}

/** Joins a channel and takes the snapshot it answers with; the channel's id once in, or why not. */
export async function joinChannel(channelId: string): Promise<{ ok: true } | { error: string }> {
  try {
    tookSnapshot(await client().joinChannel(channelId));
    void queryClient.invalidateQueries({ queryKey: ["browse"] });
    void queryClient.invalidateQueries({ queryKey: ["channelMembers", channelId] });
    return { ok: true };
  } catch (err) {
    return { error: channelRefusal(err) };
  }
}

// The public channels of the server this account is not in: a row opens
// the channel to read first, "Join" goes in.
export function BrowsePage(props: { server: ServerSummary; back: () => void; open: (channel: ChannelSummary) => void; joined: (channelId: string) => void }) {
  const list = browseQuery(() => props.server.serverId);
  const [joining, setJoining] = createSignal<string>();
  const [error, setError] = createSignal("");
  const join = async (id: string) => {
    if (joining()) return;
    setJoining(id);
    setError("");
    const outcome = await joinChannel(id);
    setJoining(undefined);
    if ("error" in outcome) setError(outcome.error);
    else props.joined(id);
  };
  return (
    <>
      <ChannelHeader kind="page" name="Browse channels" topic={props.server.name} actions={[]} onBack={props.back} backLabel={`Back to ${props.server.name}`} />
      <div class="lu-page">
        <Show when={list.data} fallback={<p class="auth__note">{list.error ? channelRefusal(list.error) : "Looking…"}</p>}>
          {(channels) => (
            <Show
              when={channels().length > 0}
              fallback={
                <EmptyState icon="hash" title="You are in every channel here">
                  Channels you leave, and new ones, show here.
                </EmptyState>
              }
            >
              <ul class="lu-browselist" aria-label="Channels to join">
                <For each={channels()}>{(c) => <BrowseRow name={c.name} topic={c.topic} count={c.memberCount} busy={joining() === c.channelId} onOpen={() => props.open(c)} onJoin={() => void join(c.channelId)} />}</For>
              </ul>
            </Show>
          )}
        </Show>
        <Show when={error()}>
          <p class="auth__error" role="alert">
            {error()}
          </p>
        </Show>
      </div>
    </>
  );
}

// "Your settings in" a server: this account's two switches there, saved together.
export function YourSettingsPage(props: { server: ServerSummary; back: () => void }) {
  const [adds, setAdds] = createSignal(!props.server.channelAddsByInvitation);
  const [dms, setDms] = createSignal(!props.server.serverDmsOff);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const [saved, setSaved] = createSignal(false);
  const serverId = createMemo(() => props.server.serverId);
  createEffect(
    on(
      serverId,
      () => {
        setAdds(!props.server.channelAddsByInvitation);
        setDms(!props.server.serverDmsOff);
        setSaved(false);
        setError("");
      },
      { defer: true },
    ),
  );
  const changed = () => adds() === props.server.channelAddsByInvitation || dms() === props.server.serverDmsOff;
  const save = async () => {
    if (busy() || !changed()) return;
    setBusy(true);
    setError("");
    setSaved(false);
    try {
      const byInvitation = !adds() === props.server.channelAddsByInvitation ? undefined : !adds();
      const dmsOff = !dms() === props.server.serverDmsOff ? undefined : !dms();
      tookSnapshot(await client().updateMembership(props.server.serverId, byInvitation, dmsOff));
      setSaved(true);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };
  return (
    <>
      <ChannelHeader kind="page" name={`Your settings in ${props.server.name}`} topic="Only you" actions={[]} onBack={props.back} backLabel={`Back to ${props.server.name}`} />
      <div class="lu-page">
        <Card title="In this server" subtitle="These are yours: nobody else here sees or changes them.">
          <div class="auth__form">
            <div class="app__switch">
              <Checkbox
                label="Let others add me to channels"
                checked={adds()}
                onChange={(e) => {
                  setAdds(e.currentTarget.checked);
                  setSaved(false);
                }}
              />
              <p class="auth__note">When off, being added becomes an invitation you can accept or dismiss.</p>
            </div>
            <div class="app__switch">
              <Checkbox
                label="Let members of this server message me"
                checked={dms()}
                onChange={(e) => {
                  setDms(e.currentTarget.checked);
                  setSaved(false);
                }}
              />
              <p class="auth__note">When off, people here can still message you privately.</p>
            </div>
            <Show when={error()}>
              <p class="auth__error" role="alert">
                {error()}
              </p>
            </Show>
            <Show when={saved()}>
              <p class="auth__note" role="status">
                Saved.
              </p>
            </Show>
            <Button variant="primary" size="md" disabled={busy() || !changed()} onClick={() => void save()}>
              Save
            </Button>
          </div>
        </Card>
      </div>
    </>
  );
}

// The channels every newcomer joins, among the server's public ones: those
// this account is in, and those Browse lists.
export function DefaultChannelsCard(props: { server: ServerSummary; snapshot: SnapshotSummary | undefined }) {
  const browse = browseQuery(() => props.server.serverId);
  const publicChannels = createMemo(() => {
    const mine = (props.snapshot?.channels ?? []).filter((c) => c.serverId === props.server.serverId && c.kind === "TEXT" && !c.private);
    const more = (browse.data ?? []).filter((c) => !mine.some((m) => m.channelId === c.channelId));
    return [...mine, ...more];
  });
  const [chosen, setChosen] = createSignal<string[] | undefined>();
  const picked = () => chosen() ?? publicChannels().filter((c) => c.isDefault).map((c) => c.channelId);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const [saved, setSaved] = createSignal(false);
  createEffect(on(() => props.server.serverId, () => setChosen(undefined), { defer: true }));
  const toggle = (id: string) => {
    const now = picked();
    setChosen(now.includes(id) ? now.filter((x) => x !== id) : [...now, id]);
    setSaved(false);
  };
  const save = async () => {
    if (busy()) return;
    setBusy(true);
    setError("");
    setSaved(false);
    try {
      tookSnapshot(await client().setDefaultChannels(props.server.serverId, picked()));
      void queryClient.invalidateQueries({ queryKey: ["browse", props.server.serverId] });
      setSaved(true);
    } catch (err) {
      const ce = err instanceof ClientError ? err : null;
      setError(ce?.code === "PERMISSION_DENIED" ? "You cannot change this server's settings." : err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card title="Default channels" subtitle="New members join these channels.">
      <div class="auth__form">
        <For each={publicChannels()}>{(c) => <Checkbox label={`#${c.name}`} checked={picked().includes(c.channelId)} onChange={() => toggle(c.channelId)} />}</For>
        <Show when={error()}>
          <p class="auth__error" role="alert">
            {error()}
          </p>
        </Show>
        <Show when={saved()}>
          <p class="auth__note" role="status">
            Saved.
          </p>
        </Show>
        <Button variant="primary" size="md" disabled={busy()} onClick={() => void save()}>
          Save
        </Button>
      </div>
    </Card>
  );
}

// The open channel's members beside it: "Add people" at the top for those
// in it, the people grouped by presence as the server's are, "Remove" for
// those who manage channels, and "Leave channel" at the end.
export function ChannelMembersAside(props: {
  server: ServerSummary;
  channel: ChannelSummary;
  joined: boolean;
  me: string;
  openPerson: (accountId: string, anchor: HTMLElement) => void;
  addPeople: (anchor: HTMLElement) => void;
  left: () => void;
}) {
  const ids = channelMembersQuery(() => props.channel.channelId);
  const members = membersQuery(() => props.server.serverId);
  const [query, setQuery] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  createEffect(on(() => props.channel.channelId, () => (setQuery(""), setError(""))));
  const entries = createMemo(() => {
    void presence();
    const all = members.data ?? [];
    return (ids.data ?? [])
      .map((id) => {
        const m = all.find((x) => x.accountId === id);
        const name = m ? m.displayName || m.nickname || m.handle || id.slice(0, 8) : id.slice(0, 8);
        const subtitle = [m?.handle ? "@" + m.handle : "", m?.pronouns ?? "", customStatusOf(props.server.serverId, id)].filter(Boolean).join(" · ") || undefined;
        // Someone who manages channels may take another out of this one.
        const action =
          props.server.canManageChannels && props.joined && id !== props.me
            ? { label: "Remove from channel", description: `Remove ${name} from #${props.channel.name}`, disabled: busy(), onClick: () => void remove(id) }
            : undefined;
        return { id, name, handle: m?.handle ?? "", initials: initialsOf(name), src: avatarSrc(m?.avatarUrl), tone: toneFor(id), status: presenceOf(props.server.serverId, id), subtitle, tag: id === props.me ? "You" : undefined, action };
      })
      .sort((a, b) => a.name.localeCompare(b.name));
  });
  const found = createMemo(() => {
    const wanted = query().trim().toLowerCase();
    return wanted ? entries().filter((e) => e.name.toLowerCase().includes(wanted) || e.handle.toLowerCase().includes(wanted)) : entries();
  });
  const sections = createMemo(() => {
    const here = found().filter((e) => e.status);
    const away = found().filter((e) => !e.status);
    const out: { title?: string; members: typeof here }[] = [];
    if (here.length) out.push({ title: "Online · " + here.length, members: here });
    if (away.length) out.push({ title: here.length ? "Offline · " + away.length : undefined, members: away });
    return out;
  });
  const remove = async (id: string) => {
    if (busy()) return;
    setBusy(true);
    setError("");
    try {
      await client().removeChannelMember(props.channel.channelId, id);
      void queryClient.invalidateQueries({ queryKey: ["channelMembers", props.channel.channelId] });
    } catch (err) {
      setError(channelRefusal(err));
    } finally {
      setBusy(false);
    }
  };
  const leave = async () => {
    if (busy()) return;
    setBusy(true);
    setError("");
    try {
      tookSnapshot(await client().leaveChannel(props.channel.channelId));
      void queryClient.invalidateQueries({ queryKey: ["browse"] });
      props.left();
    } catch (err) {
      setError(channelRefusal(err));
    } finally {
      setBusy(false);
    }
  };
  return (
    <SidePanel tabs={[{ id: "members", label: "Members", count: entries().length }]} value="members">
      <Show when={props.joined}>
        <div class="lu-memberadd">
          <Button size="md" onClick={(e) => props.addPeople(e.currentTarget)} aria-haspopup="dialog">
            Add people
          </Button>
        </div>
      </Show>
      <SidePanelSearch>
        <SearchField label="Find a member" value={query()} onInput={(e) => setQuery(e.currentTarget.value)} />
      </SidePanelSearch>
      <SidePanelScroll>
        <MemberList sections={sections()} empty={query().trim() ? "No member matches that." : undefined} onOpen={(m, row) => m.id && props.openPerson(m.id, row)} />
      </SidePanelScroll>
      <div class="lu-memberfoot">
        <Show when={error()}>
          <p class="auth__error" role="alert">
            {error()}
          </p>
        </Show>
        <Show when={props.joined}>
          <Button size="md" disabled={busy()} onClick={() => void leave()}>
            Leave channel
          </Button>
        </Show>
      </div>
    </SidePanel>
  );
}
