// What opens over the Quiet Split: the menu under a server's name and
// the popovers it leads to (a new channel, invite links), the Channels
// heading's plus (create a channel, browse them), "Add people" to a
// channel and "Message someone" in a server (architecture 6.8), the
// account menu on the rail's self avatar, the profile card a member's
// row or a message's author opens and the menu a right-click on a
// person opens (issue #25); and a rail tile's menu and Folder settings,
// which are rail.tsx's. From the Quiet Split and Profile card boards;
// the popovers, which have no board, are drawn in the card's
// vocabulary. One is open at a time, held by the app shell.
import { For, Match, Show, Switch, createMemo, createSignal } from "solid-js";
import { Button, Checkbox, Choice, ContextMenu, Menu, MenuHead, MenuItem, MenuLabel, MenuRadio, MenuRule, PersonMenu, PickRow, Pop, Popover, ProfileCard, SearchField, TextField, type IconName, type Status } from "../design/components";
import { ClientError, type InviteSummary, type MemberSummary, type ServerSummary, type SnapshotSummary } from "../client/bridge";
import { avatarSrc, channelMembersQuery, client, connection, customStatusOf, membersQuery, myStatus, presenceOf, privateStateNow, queryClient, setMyStatus } from "../client/state";
import { channelRefusal, dmRefusal } from "./channels";
import { offeredWays, personOffers, type MenuPoint, type PersonOffer, type PersonWay } from "../client/menus";
import { useNames } from "../client/names";
import { cardMarks, memberOffers, rolesHeld, settingsSections } from "../client/roles";
import { initialsOf, toneFor } from "./app";
import { messagePerson } from "./home";
import { FolderSettingsPop, RailMenu } from "./rail";
import { MemberRolesPop, MemberTitlePop } from "./roles";

/** What is open over the frame, and the element it opened from. */
export type Over =
  | { kind: "serverMenu"; anchor: HTMLElement }
  | { kind: "newChannel"; anchor: HTMLElement }
  | { kind: "channelsMenu"; anchor: HTMLElement }
  | { kind: "addPeople"; anchor: HTMLElement; channelId: string }
  | { kind: "newDm"; anchor: HTMLElement }
  | { kind: "invite"; anchor: HTMLElement }
  | { kind: "account"; anchor: HTMLElement }
  | { kind: "person"; anchor: HTMLElement; accountId: string; /** What a way chosen from their menu led to instead, said in the card. */ note?: string }
  | { kind: "personMenu"; anchor: HTMLElement; accountId: string; /** Where the pointer asked; none for the keyboard. */ at?: MenuPoint }
  /** What a card's Edit opens, in the server on screen (architecture 7.5): a member's roles to tick, or their title. The card comes back as it closes. */
  | { kind: "memberRoles"; anchor: HTMLElement; accountId: string }
  | { kind: "memberTitle"; anchor: HTMLElement; accountId: string }
  | { kind: "railMenu"; anchor: HTMLElement; id: string }
  | { kind: "folderSettings"; anchor: HTMLElement; folderId: string };

export interface OverlayProps {
  over: Over;
  snapshot: SnapshotSummary | undefined;
  /** The server on screen; none at Home. */
  server: ServerSummary | undefined;
  open: (over: Over) => void;
  close: () => void;
  onChannelCreated: (channelId: string) => void;
  /** A channel or a server DM of the server on screen, to open. */
  toChannel: (channelId: string) => void;
  toBrowse: () => void;
  /** This account's own settings in the server on screen (architecture 6.8). */
  toYourSettings: () => void;
  onConversation: (channelId: string) => void;
  toProfile: () => void;
  toPreferences: () => void;
  /** The settings of the server on screen (architecture 6.7). */
  toServerSettings: () => void;
  toDevices: () => void;
  /** Setting up private messages here, or bringing this browser in (issue #20). */
  toPrivate: () => void;
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
              <MenuItem
                icon="search"
                onSelect={() => {
                  props.close();
                  props.toBrowse();
                }}
              >
                Browse channels
              </MenuItem>
              <Show when={server().canManageChannels}>
                <MenuItem icon="hash" onSelect={() => props.open({ kind: "newChannel", anchor: props.over.anchor })}>
                  New channel
                </MenuItem>
              </Show>
              <MenuRule />
              <MenuItem
                icon="user"
                onSelect={() => {
                  props.close();
                  props.toYourSettings();
                }}
              >
                Your settings in {server().name}
              </MenuItem>
              <Show when={settingsSections(server()).length > 0}>
                <MenuItem
                  icon="settings"
                  onSelect={() => {
                    props.close();
                    props.toServerSettings();
                  }}
                >
                  Server settings
                </MenuItem>
              </Show>
            </Menu>
          </Popover>
        )}
      </Match>
      <Match when={props.over.kind === "newChannel" && props.server}>{(server) => <NewChannel server={server()} anchor={props.over.anchor} close={props.close} onCreated={props.onChannelCreated} />}</Match>
      <Match when={props.over.kind === "channelsMenu" && props.server}>
        {(server) => (
          <Popover anchor={props.over.anchor} onClose={props.close} closeLabel="Close menu">
            <Menu label="Channels" compact>
              <Show when={server().canManageChannels}>
                <MenuItem icon="plus" onSelect={() => props.open({ kind: "newChannel", anchor: props.over.anchor })}>
                  Create a channel
                </MenuItem>
              </Show>
              <MenuItem
                icon="search"
                onSelect={() => {
                  props.close();
                  props.toBrowse();
                }}
              >
                Browse channels
              </MenuItem>
            </Menu>
          </Popover>
        )}
      </Match>
      <Match when={props.over.kind === "addPeople" && props.server && props.over}>
        {(over) => <AddPeople server={props.server as ServerSummary} snapshot={props.snapshot} channelId={(over() as Extract<Over, { kind: "addPeople" }>).channelId} anchor={props.over.anchor} close={props.close} />}
      </Match>
      <Match when={props.over.kind === "newDm" && props.server}>{(server) => <NewDm {...props} server={server()} />}</Match>
      <Match when={props.over.kind === "invite" && props.server}>{(server) => <Invites server={server()} anchor={props.over.anchor} close={props.close} />}</Match>
      <Match when={props.over.kind === "account"}>
        <AccountMenu {...props} />
      </Match>
      <Match when={props.over.kind === "person" && props.over}>
        {(over) => <PersonCard {...props} accountId={(over() as Extract<Over, { kind: "person" }>).accountId} note={(over() as Extract<Over, { kind: "person" }>).note} />}
      </Match>
      <Match when={props.over.kind === "personMenu" && props.over}>
        {(over) => <PersonMenuOver {...props} accountId={(over() as Extract<Over, { kind: "personMenu" }>).accountId} at={(over() as Extract<Over, { kind: "personMenu" }>).at} />}
      </Match>
      <Match when={(props.over.kind === "memberRoles" || props.over.kind === "memberTitle") && props.server && props.over}>
        {(over) => <MemberPop {...props} server={props.server as ServerSummary} kind={over().kind as "memberRoles" | "memberTitle"} accountId={(over() as Extract<Over, { kind: "memberRoles" }>).accountId} />}
      </Match>
      <Match when={props.over.kind === "railMenu" && props.over}>
        {(over) => (
          <RailMenu
            id={(over() as Extract<Over, { kind: "railMenu" }>).id}
            anchor={props.over.anchor}
            snapshot={props.snapshot}
            close={props.close}
            toSettings={(folderId, anchor) => props.open({ kind: "folderSettings", anchor: anchor ?? props.over.anchor, folderId })}
          />
        )}
      </Match>
      <Match when={props.over.kind === "folderSettings" && props.over}>
        {(over) => <FolderSettingsPop folderId={(over() as Extract<Over, { kind: "folderSettings" }>).folderId} anchor={props.over.anchor} snapshot={props.snapshot} close={props.close} />}
      </Match>
    </Switch>
  );
}

// A new channel in the server on screen, opened once made.
function NewChannel(props: { server: ServerSummary; anchor: HTMLElement; close: () => void; onCreated: (channelId: string) => void }) {
  const [name, setName] = createSignal("");
  const [isPrivate, setPrivate] = createSignal(false);
  const [error, setError] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const create = async (e: Event) => {
    e.preventDefault();
    if (!name().trim() || busy()) return;
    setBusy(true);
    setError("");
    try {
      const channel = await client().createChannel(props.server.serverId, name().trim(), isPrivate());
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
      <Pop title="New channel" subtitle={isPrivate() ? "A text channel only the people in it can see." : `A text channel everyone in ${props.server.name} can see.`}>
        <form class="lu-pop__form" onSubmit={(e) => void create(e)}>
          <TextField label="Name" value={name()} onInput={(e) => setName(e.currentTarget.value)} placeholder="general" autocomplete="off" />
          <div class="app__switch">
            <Checkbox label="Private" checked={isPrivate()} onChange={(e) => setPrivate(e.currentTarget.checked)} />
            <p class="auth__note">Only people added can see it.</p>
          </div>
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
        <MenuItem icon="preferences" onSelect={then(props.toPreferences)}>
          Preferences
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

// Someone as their card and their menu both meet them: who they are, what
// they are offered there (`personOffers` holds the rules and the words) and
// the ways themselves. A way answers with nothing once it has gone where it
// leads, and with what to say where it did not.
function usePerson(props: OverlayProps & { accountId: string }) {
  const names = useNames(() => props.snapshot);
  // Who it is, read once: what is over the frame is made anew for each thing opened, and a way
  // chosen from the menu goes on after the menu has closed.
  const accountId = props.accountId;
  const me = () => accountId === props.snapshot?.accountId;
  const name = () => names.nameOf(accountId);
  // In a server, a server DM first (architecture 6.4), unless they turned those off there.
  const members = membersQuery(() => props.server?.serverId);
  const offers = () =>
    personOffers({
      me: me(),
      name: name(),
      server: props.server ? { canStartDms: props.server.canStartDms, dmsOff: !!(members.data ?? []).find((m) => m.accountId === accountId)?.serverDmsOff } : undefined,
    });
  const ways: Record<PersonWay, () => Promise<string>> = {
    serverDm: async () => {
      const server = props.server;
      if (!server) return "";
      try {
        const channel = await client().openServerDm(server.serverId, accountId);
        await queryClient.invalidateQueries({ queryKey: ["snapshot"] });
        props.close();
        props.toChannel(channel.channelId);
        return "";
      } catch (err) {
        return dmRefusal(err);
      }
    },
    private: async () => {
      // Personal messages are between devices: a browser that cannot read
      // them is sent to set them up, or to be brought in (issue #20).
      if ((await privateStateNow()) !== "ready") {
        props.close();
        props.toPrivate();
        return "";
      }
      const outcome = await messagePerson(accountId, name());
      if ("note" in outcome) return outcome.note;
      props.close();
      props.onConversation(outcome.channelId);
      return "";
    },
  };
  return { ...names, me, name, offers, ways, members };
}

// Who someone is: the name, the handle, where they are in the server on
// screen, what their verified profile says, and a way to write to them.
function PersonCard(props: OverlayProps & { accountId: string; note?: string }) {
  const { profile, photoOf, me, name, offers, ways, members } = usePerson(props);
  const handle = () => (me() ? props.snapshot?.handle : profile(props.accountId)?.handle) || undefined;
  const details = () =>
    (
      [
        { icon: "emoji", label: "Status", text: customStatusOf(props.server?.serverId, props.accountId) },
        { icon: "user", label: "Pronouns", text: profile(props.accountId)?.pronouns ?? "" },
        { icon: "mention", label: "About", text: profile(props.accountId)?.bio ?? "" },
      ] satisfies { icon: IconName; label: string; text: string }[]
    ).filter((d) => d.text.trim() && d.text !== name());
  // What a way led to instead; the card opens with what one chosen from the menu did.
  const [note, setNote] = createSignal(props.note ?? "");
  const [busy, setBusy] = createSignal(false);
  const go = async (way: PersonWay) => {
    setBusy(true);
    setNote("");
    try {
      setNote(await ways[way]());
    } finally {
      setBusy(false);
    }
  };
  const button = (offer: PersonOffer | undefined) => offer && { label: offer.label, onClick: () => void go(offer.way), disabled: busy() };
  const action = () => button(offers().action);
  const secondary = () => button(offers().secondary);
  const cardNote = () => note() || offers().note || "";
  // In a server: their title and roles there, who they are to it, and what of those is this account's to change (architecture 7.5).
  const member = () => (props.server ? members.data?.find((m) => m.accountId === props.accountId) : undefined);
  const edits = () => {
    const [server, m] = [props.server, member()];
    if (!server || !m) return undefined;
    const may = memberOffers(server, m, me());
    const open = (kind: "memberRoles" | "memberTitle") => () => props.open({ kind, anchor: props.over.anchor, accountId: props.accountId });
    return may.roles || may.title ? { roles: may.roles ? open("memberRoles") : undefined, title: may.title ? open("memberTitle") : undefined } : undefined;
  };
  return (
    <Popover anchor={props.over.anchor} onClose={props.close} closeLabel="Close profile">
      <ProfileCard
        name={name()}
        initials={initialsOf(name())}
        src={photoOf(props.accountId)}
        tone={toneFor(props.accountId)}
        handle={handle()}
        tag={cardMarks(me(), member())}
        title={member()?.title || undefined}
        roles={props.server && member() ? rolesHeld(props.server.roles, member()!.roleIds) : undefined}
        edit={edits()}
        // Presence is a server's to say; at Home there is none to show.
        status={props.server ? (presenceOf(props.server.serverId, props.accountId) ?? null) : undefined}
        details={details()}
        note={cardNote() || undefined}
        action={action()}
        secondary={secondary()}
        onClose={props.close}
      />
    </Popover>
  );
}

// A person's menu (#25), where a right-click or the keyboard asked for it on
// their row, their name over a message, a mention of them or a conversation
// with them alone: View profile, which opens the card against the same
// control as a press on it does, then what the card would offer there. A way
// that leads nowhere is said in the card, since the menu has gone by then.
function PersonMenuOver(props: OverlayProps & { accountId: string; at?: MenuPoint }) {
  const { name, offers, ways } = usePerson(props);
  // Read once: the card opens against the control after this menu has gone.
  const anchor = props.over.anchor;
  const accountId = props.accountId;
  const card = (note?: string) => props.open({ kind: "person", anchor, accountId, note });
  const go = async (way: PersonWay) => {
    props.close();
    const note = await ways[way]();
    if (note) card(note);
  };
  return (
    <ContextMenu anchor={anchor} at={props.at} onClose={props.close}>
      <PersonMenu name={name()} onProfile={() => card()} ways={offeredWays(offers()).map((o) => ({ label: o.label, onSelect: () => void go(o.way) }))} />
    </ContextMenu>
  );
}

// What a card's Edit opened, for the member as the server's list now has
// them; the card comes back when it is saved or cancelled, and a press
// outside closes both.
function MemberPop(props: OverlayProps & { server: ServerSummary; kind: "memberRoles" | "memberTitle"; accountId: string }) {
  const members = membersQuery(() => props.server.serverId);
  // Read once: the card opens against the control after this has gone.
  const anchor = props.over.anchor;
  const accountId = props.accountId;
  // The member as the list now has them: the pop's form follows them until it is touched and keeps what was changed
  // after (`memberForm` in `roles.tsx`). One who has left the list stays as last seen, and a save says they are gone.
  const member = createMemo<MemberSummary | undefined>((had) => members.data?.find((m) => m.accountId === accountId) ?? had);
  const card = () => props.open({ kind: "person", anchor, accountId });
  return (
    <Show when={member()}>
      {(m) => (
        <Show when={props.kind === "memberRoles"} fallback={<MemberTitlePop server={props.server} member={m()} anchor={anchor} close={props.close} done={card} />}>
          <MemberRolesPop server={props.server} member={m()} anchor={anchor} close={props.close} done={card} />
        </Show>
      )}
    </Show>
  );
}

/** The server's members whose name or handle has `filter` in it, by name, without this account. */
function others(members: MemberSummary[] | undefined, me: string | undefined, filter: string): MemberSummary[] {
  const wanted = filter.trim().toLowerCase();
  const nameOf = (m: MemberSummary) => m.displayName || m.nickname || m.handle || m.accountId.slice(0, 8);
  return (members ?? [])
    .filter((m) => m.accountId !== me && (!wanted || nameOf(m).toLowerCase().includes(wanted) || m.handle.toLowerCase().includes(wanted)))
    .sort((a, b) => nameOf(a).localeCompare(nameOf(b)));
}

// "Add people" to a channel: the server's members not in it, ticked and
// added together; one who takes adds as invitations is marked so.
function AddPeople(props: { server: ServerSummary; snapshot: SnapshotSummary | undefined; channelId: string; anchor: HTMLElement; close: () => void }) {
  const members = membersQuery(() => props.server.serverId);
  const inside = channelMembersQuery(() => props.channelId);
  const channel = () => (props.snapshot?.channels ?? []).find((c) => c.channelId === props.channelId);
  const [filter, setFilter] = createSignal("");
  const [chosen, setChosen] = createSignal<string[]>([]);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const candidates = createMemo(() => others(members.data, props.snapshot?.accountId, filter()).filter((m) => !(inside.data ?? []).includes(m.accountId)));
  const toggle = (id: string) => setChosen((now) => (now.includes(id) ? now.filter((x) => x !== id) : [...now, id]));
  const add = async () => {
    if (busy() || !chosen().length) return;
    setBusy(true);
    setError("");
    try {
      await client().addChannelMembers(props.channelId, chosen());
      void queryClient.invalidateQueries({ queryKey: ["channelMembers", props.channelId] });
      props.close();
    } catch (err) {
      const ce = err instanceof ClientError ? err : null;
      setError(ce?.code === "INVALID_ARGUMENT" ? "Someone chosen cannot see this channel." : channelRefusal(err));
      setBusy(false);
    }
  };
  return (
    <Popover anchor={props.anchor} onClose={props.close}>
      <Pop title="Add people" subtitle={`To #${channel()?.name ?? "this channel"}.`}>
        <div class="lu-pop__form">
          <SearchField label="Find a member" value={filter()} onInput={(e) => setFilter(e.currentTarget.value)} />
          <Show when={candidates().length > 0} fallback={<p class="auth__note">Everyone here is in this channel already.</p>}>
            <ul class="lu-picklist" aria-label="People to add">
              <For each={candidates()}>
                {(m) => {
                  const name = m.displayName || m.nickname || m.handle || m.accountId.slice(0, 8);
                  return <PickRow name={name} initials={initialsOf(name)} src={avatarSrc(m.avatarUrl)} tone={toneFor(m.accountId)} note={m.channelAddsByInvitation ? "Will be invited" : undefined} checked={chosen().includes(m.accountId)} onPick={() => toggle(m.accountId)} />;
                }}
              </For>
            </ul>
          </Show>
          <Show when={error()}>
            <p class="lu-pop__error" role="alert">
              {error()}
            </p>
          </Show>
          <Button variant="primary" size="md" block disabled={busy() || chosen().length === 0} onClick={() => void add()}>
            {chosen().length > 1 ? `Add ${chosen().length} people` : "Add"}
          </Button>
        </div>
      </Pop>
    </Popover>
  );
}

// "Message someone" in a server: its members, one to open a server DM
// with; one who turned server DMs off cannot be chosen.
function NewDm(props: OverlayProps & { server: ServerSummary }) {
  const members = membersQuery(() => props.server.serverId);
  const [filter, setFilter] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const open = async (id: string) => {
    if (busy()) return;
    setBusy(true);
    setError("");
    try {
      const channel = await client().openServerDm(props.server.serverId, id);
      await queryClient.invalidateQueries({ queryKey: ["snapshot"] });
      props.close();
      props.toChannel(channel.channelId);
    } catch (err) {
      setError(dmRefusal(err));
      setBusy(false);
    }
  };
  return (
    <Popover anchor={props.over.anchor} onClose={props.close}>
      <Pop title="Message someone" subtitle={`A conversation ${props.server.name} keeps.`}>
        <div class="lu-pop__form">
          <SearchField label="Find a member" value={filter()} onInput={(e) => setFilter(e.currentTarget.value)} />
          <ul class="lu-picklist" aria-label="Members">
            <For each={others(members.data, props.snapshot?.accountId, filter())}>
              {(m) => {
                const name = m.displayName || m.nickname || m.handle || m.accountId.slice(0, 8);
                return <PickRow name={name} initials={initialsOf(name)} src={avatarSrc(m.avatarUrl)} tone={toneFor(m.accountId)} note={m.serverDmsOff ? "Unavailable" : undefined} disabled={m.serverDmsOff || busy()} onPick={() => void open(m.accountId)} />;
              }}
            </For>
          </ul>
          <Show when={error()}>
            <p class="lu-pop__error" role="alert">
              {error()}
            </p>
          </Show>
        </div>
      </Pop>
    </Popover>
  );
}
