// What opens over the Quiet Split: the menu under a server's name and
// the two popovers it leads to (a new channel, invite links), the
// account menu on the rail's self avatar, and the profile card a
// member's row or a message's author opens. From the Quiet Split and
// Profile card boards; the popovers, which have no board, are drawn in
// the card's vocabulary. One is open at a time, held by the app shell.
import { For, Match, Show, Switch, createSignal } from "solid-js";
import { Button, Choice, Menu, MenuHead, MenuItem, MenuLabel, MenuRadio, MenuRule, Pop, Popover, ProfileCard, TextField, type IconName, type Status } from "../design/components";
import { ClientError, type InviteSummary, type ServerSummary, type SnapshotSummary } from "../client/bridge";
import { avatarSrc, client, connection, customStatusOf, myStatus, presenceOf, queryClient, setMyStatus } from "../client/state";
import { useNames } from "../client/names";
import { initialsOf, toneFor } from "./app";
import { messagePerson } from "./home";

/** What is open over the frame, and the element it opened from. */
export type Over =
  | { kind: "serverMenu"; anchor: HTMLElement }
  | { kind: "newChannel"; anchor: HTMLElement }
  | { kind: "invite"; anchor: HTMLElement }
  | { kind: "account"; anchor: HTMLElement }
  | { kind: "person"; anchor: HTMLElement; accountId: string };

export interface OverlayProps {
  over: Over;
  snapshot: SnapshotSummary | undefined;
  /** The server on screen; none at Home. */
  server: ServerSummary | undefined;
  open: (over: Over) => void;
  close: () => void;
  onChannelCreated: (channelId: string) => void;
  onConversation: (channelId: string) => void;
  toProfile: () => void;
  toDevices: () => void;
  toLink: () => void;
  onSignOut: () => void;
}

export function Overlay(props: OverlayProps) {
  return (
    <Switch>
      <Match when={props.over.kind === "serverMenu" && props.server}>
        {(server) => (
          <Popover anchor={props.over.anchor} onClose={props.close} closeLabel="Close menu">
            <Menu label={server().name} compact>
              <MenuItem icon="member-joined" onSelect={() => props.open({ kind: "invite", anchor: props.over.anchor })}>
                Invite people
              </MenuItem>
              <MenuItem icon="hash" onSelect={() => props.open({ kind: "newChannel", anchor: props.over.anchor })}>
                New channel
              </MenuItem>
            </Menu>
          </Popover>
        )}
      </Match>
      <Match when={props.over.kind === "newChannel" && props.server}>{(server) => <NewChannel server={server()} anchor={props.over.anchor} close={props.close} onCreated={props.onChannelCreated} />}</Match>
      <Match when={props.over.kind === "invite" && props.server}>{(server) => <Invites server={server()} anchor={props.over.anchor} close={props.close} />}</Match>
      <Match when={props.over.kind === "account"}>
        <AccountMenu {...props} />
      </Match>
      <Match when={props.over.kind === "person" && props.over}>{(over) => <PersonCard {...props} accountId={(over() as Extract<Over, { kind: "person" }>).accountId} />}</Match>
    </Switch>
  );
}

// A new channel in the server on screen, opened once made.
function NewChannel(props: { server: ServerSummary; anchor: HTMLElement; close: () => void; onCreated: (channelId: string) => void }) {
  const [name, setName] = createSignal("");
  const [error, setError] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const create = async (e: Event) => {
    e.preventDefault();
    if (!name().trim() || busy()) return;
    setBusy(true);
    setError("");
    try {
      const channel = await client().createChannel(props.server.serverId, name().trim());
      await queryClient.invalidateQueries({ queryKey: ["snapshot"] });
      props.onCreated(channel.channelId);
    } catch (err) {
      const ce = err instanceof ClientError ? err : null;
      setError(ce?.code === "PERMISSION_DENIED" ? "You cannot add channels to this server." : err instanceof Error ? err.message : String(err));
      setBusy(false);
    }
  };
  return (
    <Popover anchor={props.anchor} onClose={props.close}>
      <Pop title="New channel" subtitle={`A text channel everyone in ${props.server.name} can see.`}>
        <form class="lu-pop__form" onSubmit={(e) => void create(e)}>
          <TextField label="Name" value={name()} onInput={(e) => setName(e.currentTarget.value)} placeholder="general" autocomplete="off" />
          <Show when={error()}>
            <p class="lu-pop__error" role="alert">
              {error()}
            </p>
          </Show>
          <Button variant="primary" size="md" type="submit" block disabled={busy() || !name().trim()}>
            Create
          </Button>
        </form>
      </Pop>
    </Popover>
  );
}

// Invite links for the server on screen: a new one with an expiry and a
// use limit, shown to copy; one mailed to an address (architecture 6.2:
// one use, a week); and the ones that exist, each revocable.
function Invites(props: { server: ServerSummary; anchor: HTMLElement; close: () => void }) {
  const [made, setMade] = createSignal<InviteSummary | null>(null);
  const [invites, setInvites] = createSignal<InviteSummary[]>([]);
  const [hours, setHours] = createSignal(0);
  const [uses, setUses] = createSignal(0);
  const [error, setError] = createSignal("");
  const [copied, setCopied] = createSignal(false);
  const [email, setEmail] = createSignal("");
  const [emailed, setEmailed] = createSignal("");
  const failed = (e: unknown) => {
    const ce = e instanceof ClientError ? e : null;
    setError(ce?.code === "PERMISSION_DENIED" ? "You cannot invite people to this server." : ce?.code === "RATE_LIMITED" ? "Too many links in the last hour; try later." : e instanceof Error ? e.message : String(e));
  };
  void client()
    .invites(props.server.serverId)
    .then(setInvites)
    .catch(failed);
  const describe = (i: InviteSummary) => {
    const count = (i.email ? `to ${i.email} · ` : "") + (i.maxUses === 0 ? `${i.uses} uses` : `${i.uses}/${i.maxUses} uses`);
    // The core leaves the field out for a link that does not expire.
    if (i.expiresAtMicros == null) return `${count} · no expiry`;
    return `${count} · expires ${new Date(i.expiresAtMicros / 1000).toLocaleString(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" })}`;
  };
  const make = async (e: Event) => {
    e.preventDefault();
    setError("");
    setCopied(false);
    try {
      const invite = await client().createInvite(props.server.serverId, uses(), hours() || undefined);
      setMade(invite);
      setInvites([invite, ...invites()]);
    } catch (err) {
      failed(err);
    }
  };
  const send = async () => {
    const to = email().trim();
    if (!to) return;
    setError("");
    setEmailed("");
    try {
      const invite = await client().emailInvite(props.server.serverId, to);
      setInvites([invite, ...invites()]);
      setEmailed(to);
      setEmail("");
    } catch (err) {
      failed(err);
    }
  };
  const revoke = async (i: InviteSummary) => {
    setError("");
    try {
      await client().revokeInvite(i.code);
      setInvites(invites().filter((x) => x.code !== i.code));
      if (made()?.code === i.code) setMade(null);
    } catch (e) {
      failed(e);
    }
  };
  const copy = async () => {
    const url = made()?.url;
    if (!url) return;
    try {
      await navigator.clipboard.writeText(url);
      setCopied(true);
    } catch {
      setCopied(false);
    }
  };
  return (
    <Popover anchor={props.anchor} onClose={props.close}>
      <Pop title="Invite people" subtitle={`A link to join ${props.server.name}.`}>
        <form class="lu-pop__form" onSubmit={(e) => void make(e)}>
          <Choice label="Expires" value={hours()} onChange={setHours} options={[{ value: 0, label: "Never" }, { value: 24, label: "In a day" }, { value: 168, label: "In a week" }]} />
          <Choice label="Uses" value={uses()} onChange={setUses} options={[{ value: 0, label: "Unlimited" }, { value: 1, label: "Once" }, { value: 10, label: "Ten" }]} />
          <Button variant="primary" size="md" type="submit" block>
            Make a link
          </Button>
          <Show when={made()}>
            {(i) => (
              <div class="lu-pop__copy">
                <input class="lu-pop__link" type="text" readonly value={i().url} aria-label="Invite link" onFocus={(e) => e.currentTarget.select()} />
                <Button size="sm" onClick={() => void copy()}>
                  {copied() ? "Copied" : "Copy link"}
                </Button>
              </div>
            )}
          </Show>
        </form>
        <div class="lu-pop__form">
          <TextField label="Email" type="email" value={email()} onInput={(e) => setEmail(e.currentTarget.value)} placeholder="pat@example.com" autocomplete="off" />
          <Button size="md" block disabled={!email().trim()} onClick={() => void send()}>
            Send by email
          </Button>
          <Show when={emailed()}>
            <p class="lu-pop__note" role="status">
              Sent to {emailed()}; it works once, within a week.
            </p>
          </Show>
        </div>
        <Show when={error()}>
          <p class="lu-pop__error" role="alert">
            {error()}
          </p>
        </Show>
        <Show when={invites().length > 0}>
          <p class="lu-pop__section">Invites</p>
          <ul class="lu-pop__list" aria-label="Invites">
            <For each={invites()}>
              {(i) => (
                <li class="lu-pop__row">
                  <span>
                    {i.code} · {describe(i)}
                  </span>
                  <Button variant="text" aria-label={`Revoke invite ${i.code}`} onClick={() => void revoke(i)}>
                    Revoke
                  </Button>
                </li>
              )}
            </For>
          </ul>
        </Show>
      </Pop>
    </Popover>
  );
}

const STATUSES: { id: string; status: Status; label: string }[] = [
  { id: "ONLINE", status: "online", label: "Online" },
  { id: "AWAY", status: "away", label: "Away" },
  { id: "DO_NOT_DISTURB", status: "dnd", label: "Do not disturb" },
];

/** The status this person chose, as the design's `Status`. */
export function myStatusDot(): Status {
  return STATUSES.find((s) => s.id === myStatus().status)?.status ?? "online";
}

// Who is signed in, the status as three either-or rows, and the ways out
// of here: the profile, the devices, signing out.
function AccountMenu(props: OverlayProps) {
  const name = () => props.snapshot?.displayName || props.snapshot?.handle || "You";
  const words = () => STATUSES.find((s) => s.id === myStatus().status)?.label ?? "Online";
  const [error, setError] = createSignal("");
  const choose = async (status: string) => {
    setError("");
    try {
      await setMyStatus(status, myStatus().customStatus);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };
  const then = (go: () => void) => () => {
    props.close();
    go();
  };
  return (
    <Popover anchor={props.over.anchor} placement="beside" onClose={props.close} closeLabel="Close menu">
      <Menu label="Your account">
        <MenuHead src={avatarSrc(props.snapshot?.avatarUrl)} name={name()} sub={[props.snapshot?.handle ? "@" + props.snapshot.handle : "", connection() === "connected" ? words() : "Not connected"].filter(Boolean).join(" · ")} initials={initialsOf(name())} tone={toneFor(props.snapshot?.accountId ?? "")} status={connection() === "connected" ? myStatusDot() : "offline"} />
        <MenuRule />
        <MenuLabel>Status</MenuLabel>
        <For each={STATUSES}>
          {(s) => (
            <MenuRadio status={s.status} checked={myStatus().status === s.id} onSelect={() => void choose(s.id)}>
              {s.label}
            </MenuRadio>
          )}
        </For>
        <Show when={error()}>
          <span class="lu-menu__label" role="alert">
            {error()}
          </span>
        </Show>
        <MenuRule />
        <MenuItem icon="user" onSelect={then(props.toProfile)}>
          View profile
        </MenuItem>
        <MenuItem icon="shield" onSelect={then(props.toDevices)}>
          Devices
        </MenuItem>
        <MenuRule />
        <MenuItem icon="sign-out" onSelect={then(props.onSignOut)}>
          Sign out
        </MenuItem>
      </Menu>
    </Popover>
  );
}

// Who someone is: the name, the handle, where they are in the server on
// screen, what their verified profile says, and a way to write to them.
function PersonCard(props: OverlayProps & { accountId: string }) {
  const { nameOf, profile, photoOf } = useNames(() => props.snapshot);
  const me = () => props.accountId === props.snapshot?.accountId;
  const name = () => nameOf(props.accountId);
  const handle = () => (me() ? props.snapshot?.handle : profile(props.accountId)?.handle) || undefined;
  const details = () =>
    (
      [
        { icon: "emoji", label: "Status", text: customStatusOf(props.server?.serverId, props.accountId) },
        { icon: "user", label: "Pronouns", text: profile(props.accountId)?.pronouns ?? "" },
        { icon: "mention", label: "About", text: profile(props.accountId)?.bio ?? "" },
      ] satisfies { icon: IconName; label: string; text: string }[]
    ).filter((d) => d.text.trim() && d.text !== name());
  const [note, setNote] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const message = async () => {
    setBusy(true);
    setNote("");
    try {
      // Personal messages are between devices: a browser with no device key is sent to link itself.
      if (!(await client().hasIdentity())) {
        props.close();
        props.toLink();
        return;
      }
      const outcome = await messagePerson(props.accountId, name());
      if ("channelId" in outcome) {
        props.close();
        props.onConversation(outcome.channelId);
      } else {
        setNote(outcome.note);
      }
    } finally {
      setBusy(false);
    }
  };
  return (
    <Popover anchor={props.over.anchor} onClose={props.close} closeLabel="Close profile">
      <ProfileCard
        name={name()}
        initials={initialsOf(name())}
        src={photoOf(props.accountId)}
        tone={toneFor(props.accountId)}
        handle={handle()}
        tag={me() ? "You" : undefined}
        // Presence is a server's to say; at Home there is none to show.
        status={props.server ? (presenceOf(props.server.serverId, props.accountId) ?? null) : undefined}
        details={details()}
        note={note() || undefined}
        action={me() ? undefined : { label: `Message ${name().split(/\s+/)[0] || name()}`, onClick: () => void message(), disabled: busy() }}
        onClose={props.close}
      />
    </Popover>
  );
}
