// Roles and Members, the two sections a server's settings gain with roles
// and titles (architecture 7.5; the "Roles", "Role editor" and "Members"
// boards): a server's roles in order, each with its editor, and its members
// with their roles, their title and Kick. And what a profile card's two
// Edits open. The core says what this account may do and the instance
// decides; `client/roles.ts` holds the rules and the words.
import { For, Show, createEffect, createMemo, createResource, createSignal, on, type JSX } from "solid-js";
import {
  Avatar,
  Button,
  ConfirmPop,
  ContextMenu,
  IconButton,
  MemberEdit,
  MemberList,
  Menu,
  MenuItem,
  MenuLabel,
  MenuRule,
  PickRow,
  Pop,
  Popover,
  RoleChoices,
  RoleDot,
  RoleList,
  Roster,
  SearchField,
  SettingRow,
  SettingsGroup,
  Tag,
  TextField,
  Toggle,
  ToneSwatches,
  type RoleEntry,
  type RosterEntry,
} from "../design/components";
import type { MemberSummary, PermissionGroup, RoleSummary, ServerSummary, SnapshotSummary } from "../client/bridge";
import type { MenuPoint } from "../client/menus";
import {
  ADMINISTRATOR,
  ADMINISTRATOR_TITLE,
  DELETE_ROLE_HINT,
  EVERYONE_HOLDS,
  EVERYONE_SUBTITLE,
  LABEL_MAX,
  NOBODY_HOLDS,
  NO_MEMBER_MATCHES,
  READING_MEMBERS,
  ROLES_HINT,
  ROLES_HINT_MENU,
  TITLE_HINT,
  administratorQuestion,
  deleteQuestion,
  deleteTitle,
  changesAny,
  draftOf,
  followed,
  formOf,
  holders,
  landsBefore,
  lockedRole,
  memberChanges,
  memberFollowed,
  memberMark,
  memberOffers,
  memberRefusal,
  movedBefore,
  movedWords,
  offersAny,
  roleChoices,
  roleNameHint,
  roleRefusal,
  rolesHeld,
  savable,
  searched,
  stepBefore,
  tagSwitchCaption,
  toggledRole,
  toneOf,
  withheld,
  withheldCaption,
  type MemberForm,
  type RoleDraft,
} from "../client/roles";
import { avatarSrc, client, membersKept, tookMember, tookSnapshot } from "../client/state";
import { initialsOf, toneFor } from "./app";

/** A member's name as every list has it. */
export function memberName(m: Pick<MemberSummary, "displayName" | "nickname" | "handle" | "accountId">): string {
  return m.displayName || m.nickname || m.handle || m.accountId.slice(0, 8);
}

/** A role-changing call's answer taken: the snapshot as it now is, and the members' tags read again from what the core kept with it. */
function tookRoles(snapshot: SnapshotSummary, serverId: string): void {
  tookSnapshot(snapshot);
  void membersKept(serverId);
}

// The permissions an editor offers are the core's and never change while the page is open.
let offered: Promise<PermissionGroup[]> | undefined;
function offeredPermissions(): Promise<PermissionGroup[]> {
  offered ??= client().rolePermissions();
  offered.catch(() => (offered = undefined));
  return offered;
}

/**
 * A list's rows as they now are, each the object it was where nothing of
 * it changed: a snapshot or a member list read again (any account event
 * brings one) then leaves the rows it did not touch as the elements they
 * were, with a drag held over them or an editor open under one.
 */
function keptRows<T extends { id: string }>(had: T[], now: T[]): T[] {
  return now.map((row) => {
    const was = had.find((h) => h.id === row.id);
    return was && JSON.stringify(was) === JSON.stringify(row) ? was : row;
  });
}

function entryOf(role: RoleSummary, members: MemberSummary[] | undefined): RoleEntry {
  return { id: role.roleId, name: role.name, tone: toneOf(role.color), members: holders(role, members), untagged: role.hideTag && !role.isEveryone, everyone: role.isEveryone, locked: lockedRole(role) };
}

/**
 * A server's roles, highest first (the "Roles" board): a row each, moved
 * by its handle or from its menu, opening its editor in the list's place.
 * `menuOnly` is a narrow window's or a touch screen's way, where nothing
 * drags and each row has a More button.
 */
export function RolesSection(props: { server: ServerSummary; members: MemberSummary[] | undefined; me: MemberSummary | undefined; menuOnly: boolean }) {
  // What is in the list's place: a role's editor, or a new role's.
  const [editing, setEditing] = createSignal<string | null>(null);
  const [menu, setMenu] = createSignal<{ role: RoleEntry; anchor: HTMLElement; at?: MenuPoint } | null>(null);
  const [asking, setAsking] = createSignal<{ role: RoleEntry; anchor: HTMLElement } | null>(null);
  const [error, setError] = createSignal("");
  const [said, setSaid] = createSignal("");
  // The order as a move left it, drawn before the instance answers.
  const [moved, setMoved] = createSignal<string[] | null>(null);
  const roles = createMemo(() => {
    const order = moved();
    if (!order) return props.server.roles;
    const byId = new Map(props.server.roles.map((r) => [r.roleId, r] as const));
    const placed = order.map((id) => byId.get(id)).filter((r): r is RoleSummary => !!r);
    return [...placed, ...props.server.roles.filter((r) => !order.includes(r.roleId))];
  });
  const entries = createMemo<RoleEntry[]>((had) => keptRows(had, roles().map((r) => entryOf(r, props.members))), []);
  const serverId = createMemo(() => props.server.serverId);
  createEffect(
    on(
      serverId,
      () => {
        setEditing(null);
        setMenu(null);
        setAsking(null);
        setError("");
        setMoved(null);
      },
      { defer: true },
    ),
  );
  // A role deleted elsewhere while its editor is open: back to the list. Only one the list has
  // had goes that way: a role just made is opened before the snapshot with it has been drawn.
  let listed: string | null = null;
  createEffect(() => {
    const id = editing();
    if (!id || id === "new") return;
    if (props.server.roles.some((r) => r.roleId === id)) listed = id;
    else if (listed === id) setEditing(null);
  });

  const move = async (roleId: string, beforeId: string) => {
    if (!landsBefore(roles(), roleId, beforeId)) return;
    const order = movedBefore(roles(), roleId, beforeId);
    const name = roles().find((r) => r.roleId === roleId)?.name ?? "";
    setError("");
    setMoved(order);
    setSaid(movedWords(name, order.indexOf(roleId) + 1));
    try {
      tookRoles(await client().reorderRoles(props.server.serverId, order), props.server.serverId);
    } catch (err) {
      setSaid("");
      setError(roleRefusal(err, props.server.name));
    } finally {
      setMoved(null);
    }
  };
  const remove = async (roleId: string) => {
    setAsking(null);
    setError("");
    try {
      tookRoles(await client().deleteRole(props.server.serverId, roleId), props.server.serverId);
    } catch (err) {
      setError(roleRefusal(err, props.server.name));
    }
  };

  return (
    <Show
      when={editing()}
      fallback={
        <>
          <div class="lu-settings__head">
            <div class="lu-settings__headtext">
              <h2 class="lu-settings__title">Roles</h2>
              <p class="lu-settings__subtitle">{props.menuOnly ? ROLES_HINT_MENU : ROLES_HINT}</p>
            </div>
            <Button size="sm" icon="plus" onClick={() => setEditing("new")}>
              New role
            </Button>
          </div>
          <Show when={error()}>
            <p class="auth__error" role="alert">
              {error()}
            </p>
          </Show>
          <RoleList
            roles={entries()}
            reorder={props.menuOnly ? "menu" : "drag"}
            onOpen={(r) => setEditing(r.id)}
            onMenu={(role, anchor, at) => setMenu({ role, anchor, at })}
            lands={(id, before) => landsBefore(roles(), id, before)}
            onMove={(id, before) => void move(id, before)}
          />
          <p class="lu-sr" role="status">
            {said()}
          </p>
          <Show when={menu()}>
            {(m) => {
              // Read once: a choice closes the menu before it is acted on.
              const { role, anchor } = m();
              const step = (by: -1 | 1) => stepBefore(roles(), role.id, by);
              const go = (act: () => void) => {
                setMenu(null);
                act();
              };
              return (
                <ContextMenu anchor={anchor} at={m().at} onClose={() => setMenu(null)}>
                  <Menu label={role.name} compact class="lu-menu--context">
                    <MenuLabel>{role.name}</MenuLabel>
                    <MenuItem icon="settings" onSelect={() => go(() => setEditing(role.id))}>
                      Edit role
                    </MenuItem>
                    <MenuRule />
                    <MenuItem icon="arrow-up" disabled={!step(-1)} onSelect={() => go(() => void move(role.id, step(-1) ?? ""))}>
                      Move up
                    </MenuItem>
                    <MenuItem icon="arrow-down" disabled={!step(1)} onSelect={() => go(() => void move(role.id, step(1) ?? ""))}>
                      Move down
                    </MenuItem>
                    <MenuRule />
                    <MenuItem icon="trash" danger onSelect={() => go(() => setAsking({ role, anchor }))}>
                      Delete role
                    </MenuItem>
                  </Menu>
                </ContextMenu>
              );
            }}
          </Show>
          <Show when={asking()}>
            {(a) => {
              // Read once: the keyboard goes back to the row after the question has gone.
              const { role, anchor } = a();
              return (
                <Popover anchor={anchor} onClose={() => setAsking(null)}>
                  <ConfirmPop title={deleteTitle(role.name)} confirm="Delete role" danger onCancel={() => setAsking(null)} onConfirm={() => void remove(role.id)}>
                    <For each={deleteQuestion(role.members)}>{(line) => <p>{line}</p>}</For>
                  </ConfirmPop>
                </Popover>
              );
            }}
          </Show>
        </>
      }
    >
      {(id) => (
        <Show when={id()} keyed>
          {(roleId) => <RoleEditor server={props.server} role={props.server.roles.find((r) => r.roleId === roleId)} members={props.members} me={props.me} back={() => setEditing(null)} opened={setEditing} />}
        </Show>
      )}
    </Show>
  );
}

/**
 * A role's editor (the "Role editor" board): its name, colour and tag, its
 * permissions under the core's headings, who holds it, and Delete, with
 * one Save for what is typed and switched. Who holds it is changed at
 * once, a member at a time. With no `role` it is a new role's: nothing in
 * it yet, no holders, nothing to delete. Everyone's is its permissions.
 */
function RoleEditor(props: { server: ServerSummary; role: RoleSummary | undefined; members: MemberSummary[] | undefined; me: MemberSummary | undefined; back: () => void; opened: (roleId: string) => void }) {
  const everyone = () => !!props.role?.isEveryone;
  // Two drafts (`client/roles.ts`): the role as the editor opened on it, and the one being edited. Save is offered
  // on their difference and sends it, so what someone else changed of the role meanwhile is neither lit nor undone.
  const [opened, setOpened] = createSignal<RoleDraft>(draftOf(props.role));
  const [draft, setDraft] = createSignal<RoleDraft>(draftOf(props.role));
  const [groups] = createResource(offeredPermissions);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const [saved, setSaved] = createSignal(false);
  // Nothing of the form changes while its save is on its way: the answer is what both drafts become.
  const change = (part: Partial<RoleDraft>) => {
    if (busy()) return;
    setSaved(false);
    setDraft({ ...draft(), ...part });
  };
  // An editor nobody has touched follows the role as it changes underneath; one that was touched keeps its drafts.
  // A role just made is opened before the snapshot with it has been drawn, so its editor begins with a new role's
  // empty fields, untouched, and takes the role's this way when it comes.
  // The role's fields are read here, not only the role: a role read again can be the object it was, changed in place.
  createEffect(
    on(
      () => (props.role ? draftOf(props.role) : undefined),
      (now) => {
        if (!now) return;
        const next = followed(opened(), draft(), now);
        setOpened(next.opened);
        setDraft(next.draft);
      },
      { defer: true },
    ),
  );
  const savableNow = () => savable(draft(), props.role ? opened() : undefined);
  const held = createMemo(() => (props.role ? holders(props.role, props.members) : undefined));
  const holding = createMemo(() => (props.role && !everyone() ? (props.members ?? []).filter((m) => m.roleIds.includes(props.role!.roleId)) : []));
  const shownName = () => (props.role ? (everyone() ? props.role.name : draft().name.trim() || props.role.name) : "New role");

  const save = async () => {
    if (busy() || !savableNow()) return;
    setBusy(true);
    setError("");
    setSaved(false);
    const d = draft();
    try {
      if (props.role) {
        const roleId = props.role.roleId;
        const snapshot = await client().changeRole(props.server.serverId, roleId, opened(), d);
        tookRoles(snapshot, props.server.serverId);
        // Both drafts are the role as the answer has it: what was saved, and whatever else it has become.
        const now = snapshot.servers.find((s) => s.serverId === props.server.serverId)?.roles.find((r) => r.roleId === roleId);
        const kept = now ? draftOf(now) : { ...d, name: d.name.trim() };
        setOpened(kept);
        setDraft(kept);
        setSaved(true);
      } else {
        const snapshot = await client().createRole(props.server.serverId, d.name.trim(), d.color, d.hideTag, d.permissions);
        tookRoles(snapshot, props.server.serverId);
        // The role made is the one with that name, which is its alone whatever its capitals.
        const made = snapshot.servers.find((s) => s.serverId === props.server.serverId)?.roles.find((r) => !r.isEveryone && r.name.toLowerCase() === d.name.trim().toLowerCase());
        if (made) props.opened(made.roleId);
        else props.back();
      }
    } catch (err) {
      setError(roleRefusal(err, props.server.name));
    } finally {
      setBusy(false);
    }
  };

  // Administrator asks first, against its switch.
  const [askingAdmin, setAskingAdmin] = createSignal<HTMLElement | null>(null);
  const turn = (name: string, on: boolean, control: HTMLElement | undefined) => {
    if (on && name === ADMINISTRATOR && control) {
      setAskingAdmin(control);
      return;
    }
    change({ permissions: on ? [...draft().permissions.filter((p) => p !== name), name] : draft().permissions.filter((p) => p !== name) });
  };
  const row = (p: PermissionGroup["permissions"][number]) => {
    const kept = () => withheld(props.server, p.name);
    let control: HTMLSpanElement | undefined;
    return (
      <SettingRow
        name={p.label}
        caption={kept() ? withheldCaption(p.detail) : p.detail}
        control={(ids) => (
          <span ref={control}>
            <Toggle labelledBy={ids.name} describedBy={ids.caption} checked={draft().permissions.includes(p.name)} disabled={kept()} onChange={(checked) => turn(p.name, checked, control)} />
          </span>
        )}
      />
    );
  };

  // Who holds it: taken away or given at once, a member at a time.
  const [holdersError, setHoldersError] = createSignal("");
  const [adding, setAdding] = createSignal<HTMLElement | null>(null);
  const take = async (m: MemberSummary) => {
    const role = props.role;
    if (!role) return;
    setHoldersError("");
    try {
      // This role taken and nothing else said of their roles: never a whole set built from the list.
      tookMember(props.server.serverId, m.accountId, await client().changeMember(props.server.serverId, m.accountId, [], [role.roleId], null));
    } catch (err) {
      setHoldersError(memberRefusal(err));
    }
  };

  const [askingDelete, setAskingDelete] = createSignal<HTMLElement | null>(null);
  const remove = async () => {
    const role = props.role;
    setAskingDelete(null);
    if (!role || busy()) return;
    setBusy(true);
    setError("");
    try {
      tookRoles(await client().deleteRole(props.server.serverId, role.roleId), props.server.serverId);
      props.back();
    } catch (err) {
      setError(roleRefusal(err, props.server.name));
      setBusy(false);
    }
  };

  const previewName = () => (props.me ? memberName(props.me) : "You");
  const previewTime = new Date().toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" });

  return (
    <form
      class="lu-roleedit"
      onSubmit={(e) => {
        e.preventDefault();
        void save();
      }}
    >
      <div class="lu-settings__head">
        {/* Not left while a save is on its way: what came of it is said here. */}
        <IconButton icon="back" label="Back to roles" disabled={busy()} onClick={props.back} />
        <div class="lu-settings__headtext">
          <h2 class="lu-settings__title">
            <RoleDot tone={toneOf(draft().color)} />
            {shownName()}
          </h2>
          <Show when={everyone()}>
            <p class="lu-settings__subtitle">{EVERYONE_SUBTITLE}</p>
          </Show>
        </div>
        <Button variant="primary" size="md" type="submit" disabled={busy() || !savableNow()}>
          Save
        </Button>
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
      <Show when={!everyone()}>
        <div class="lu-roleedit__part">
          <div class="lu-roleedit__field">
            <TextField label="Name" value={draft().name} maxLength={LABEL_MAX} placeholder="Moderator" autocomplete="off" readOnly={busy()} onInput={(e) => change({ name: e.currentTarget.value })} />
            <p class="lu-roleedit__hint">{roleNameHint(props.server.name)}</p>
          </div>
          <div class="lu-roleedit__field">
            <span class="lu-roleedit__legend">Colour</span>
            <ToneSwatches label="Colour" value={toneOf(draft().color)} onChange={(tone) => change({ color: tone })} onNone={() => change({ color: "" })} />
          </div>
          <SettingsGroup>
            <SettingRow name="Show as tag" caption={tagSwitchCaption(!draft().hideTag)} control={(ids) => <Toggle labelledBy={ids.name} describedBy={ids.caption} checked={!draft().hideTag} onChange={(shown) => change({ hideTag: !shown })} />} />
            <div class="lu-roleedit__preview">
              <span class="lu-roleedit__previewlabel">Preview</span>
              <Avatar initials={initialsOf(previewName())} src={avatarSrc(props.me?.avatarUrl)} tone={props.me ? toneFor(props.me.accountId) : undefined} size={28} />
              <span class="lu-roleedit__previewname">{previewName()}</span>
              <Show when={!draft().hideTag && draft().name.trim()}>
                <Tag tone={toneOf(draft().color) ?? "neutral"}>{draft().name.trim()}</Tag>
              </Show>
              <span class="lu-roleedit__previewtime">{previewTime}</span>
            </div>
          </SettingsGroup>
        </div>
      </Show>
      <Show when={groups()}>
        {(list) => (
          <div class="lu-roleedit__part">
            <h3 class="lu-roleedit__heading">Permissions</h3>
            <For each={list()}>
              {(g) => (
                <>
                  <h4 class="lu-roleedit__group">{g.title}</h4>
                  {/* Administrator, set apart: the one that grants every other. */}
                  <Show when={g.permissions.some((p) => p.name === ADMINISTRATOR)}>
                    <SettingsGroup class="lu-setgroup--apart">
                      <For each={g.permissions.filter((p) => p.name === ADMINISTRATOR)}>{row}</For>
                    </SettingsGroup>
                  </Show>
                  <SettingsGroup>
                    <For each={g.permissions.filter((p) => p.name !== ADMINISTRATOR)}>{row}</For>
                  </SettingsGroup>
                </>
              )}
            </For>
          </div>
        )}
      </Show>
      <Show when={groups.error}>
        <p class="auth__error" role="alert">
          {String(groups.error instanceof Error ? groups.error.message : groups.error)}
        </p>
      </Show>
      <Show when={props.role && !everyone() && props.members}>
        <div class="lu-roleedit__part">
          <div class="lu-roleedit__row">
            <h3 class="lu-roleedit__heading">
              Members with this role <span class="lu-settings__count">{holding().length}</span>
            </h3>
            <Button size="sm" icon="plus" onClick={(e) => setAdding(e.currentTarget)}>
              Add members
            </Button>
          </div>
          <Show when={holdersError()}>
            <p class="auth__error" role="alert">
              {holdersError()}
            </p>
          </Show>
          <Show when={holding().length > 0} fallback={<p class="lu-roleedit__hint">{NOBODY_HOLDS}</p>}>
            <div class="lu-roleedit__holders">
              <MemberList
                sections={[
                  {
                    members: holding().map((m) => ({
                      id: m.accountId,
                      name: memberName(m),
                      initials: initialsOf(memberName(m)),
                      src: avatarSrc(m.avatarUrl),
                      tone: toneFor(m.accountId),
                      subtitle: m.handle ? "@" + m.handle : undefined,
                      action: { label: "Remove from role", description: `Remove ${memberName(m)} from ${props.role?.name ?? "this role"}`, onClick: () => void take(m) },
                    })),
                  },
                ]}
              />
            </div>
          </Show>
        </div>
      </Show>
      <Show when={props.role && !everyone()}>
        <div class="lu-roleedit__part">
          <h3 class="lu-roleedit__heading">Delete role</h3>
          <div class="lu-roleedit__row">
            <p class="lu-roleedit__hint">{DELETE_ROLE_HINT}</p>
            <Button variant="danger" size="md" disabled={busy()} onClick={(e) => setAskingDelete(e.currentTarget)}>
              Delete role
            </Button>
          </div>
        </div>
      </Show>
      <Show when={askingAdmin()}>
        {(asked) => {
          // Read once: the keyboard goes back to the switch after the question has gone.
          const anchor = asked();
          return (
            <Popover anchor={anchor} onClose={() => setAskingAdmin(null)}>
              <ConfirmPop
                title={ADMINISTRATOR_TITLE}
                confirm="Turn on"
                onCancel={() => setAskingAdmin(null)}
                onConfirm={() => {
                  setAskingAdmin(null);
                  change({ permissions: [...draft().permissions.filter((p) => p !== ADMINISTRATOR), ADMINISTRATOR] });
                }}
              >
                <For each={administratorQuestion(props.server.name, shownName(), props.role ? held() : undefined)}>{(line) => <p>{line}</p>}</For>
              </ConfirmPop>
            </Popover>
          );
        }}
      </Show>
      <Show when={askingDelete()}>
        {(asked) => {
          const anchor = asked();
          return (
            <Popover anchor={anchor} onClose={() => setAskingDelete(null)}>
              <ConfirmPop title={deleteTitle(props.role?.name ?? "this role")} confirm="Delete role" danger onCancel={() => setAskingDelete(null)} onConfirm={() => void remove()}>
                <For each={deleteQuestion(held())}>{(line) => <p>{line}</p>}</For>
              </ConfirmPop>
            </Popover>
          );
        }}
      </Show>
      <Show when={adding() && props.role}>
        <AddHolders server={props.server} role={props.role as RoleSummary} members={props.members ?? []} anchor={adding() as HTMLElement} close={() => setAdding(null)} />
      </Show>
    </form>
  );
}

// "Add members" to a role: the server's members who do not hold it, ticked
// and given it together. A guest is offered no roles, so is not listed.
function AddHolders(props: { server: ServerSummary; role: RoleSummary; members: MemberSummary[]; anchor: HTMLElement; close: () => void }) {
  const [filter, setFilter] = createSignal("");
  const [chosen, setChosen] = createSignal<string[]>([]);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const candidates = createMemo(() =>
    searched(
      props.members.filter((m) => !m.guest && !m.roleIds.includes(props.role.roleId)),
      filter(),
    ).sort((a, b) => memberName(a).localeCompare(memberName(b))),
  );
  const toggle = (id: string) => setChosen((now) => (now.includes(id) ? now.filter((x) => x !== id) : [...now, id]));
  const add = async () => {
    if (busy() || !chosen().length) return;
    setBusy(true);
    setError("");
    try {
      for (const accountId of chosen()) {
        const member = props.members.find((m) => m.accountId === accountId);
        if (!member) continue;
        // This role given and nothing else said of their roles.
        tookMember(props.server.serverId, accountId, await client().changeMember(props.server.serverId, accountId, [props.role.roleId], [], null));
        setChosen((now) => now.filter((id) => id !== accountId));
      }
      props.close();
    } catch (err) {
      setError(memberRefusal(err));
      setBusy(false);
    }
  };
  return (
    <Popover anchor={props.anchor} onClose={props.close}>
      <Pop title="Add members" subtitle={`To ${props.role.name}.`}>
        <div class="lu-pop__form">
          <SearchField label="Find a member" value={filter()} onInput={(e) => setFilter(e.currentTarget.value)} />
          <Show when={candidates().length > 0} fallback={<p class="auth__note">{filter().trim() ? NO_MEMBER_MATCHES : EVERYONE_HOLDS}</p>}>
            <ul class="lu-picklist" aria-label="Members to add">
              <For each={candidates()}>{(m) => <PickRow name={memberName(m)} initials={initialsOf(memberName(m))} src={avatarSrc(m.avatarUrl)} tone={toneFor(m.accountId)} checked={chosen().includes(m.accountId)} onPick={() => toggle(m.accountId)} />}</For>
            </ul>
          </Show>
          <Show when={error()}>
            <p class="lu-pop__error" role="alert">
              {error()}
            </p>
          </Show>
          <Button variant="primary" size="md" block disabled={busy() || chosen().length === 0} onClick={() => void add()}>
            {chosen().length > 1 ? `Add ${chosen().length} members` : "Add"}
          </Button>
        </div>
      </Pop>
    </Popover>
  );
}

/** How many of a server's members Members lists before "Show more members". */
const MEMBERS_PAGE = 50;

/**
 * A server's members (the "Members" board): found by name, handle or
 * title, each with who they are to the server, their title and their
 * roles, and a row opening to what of theirs is this account's to change.
 */
export function MembersSection(props: { server: ServerSummary; members: MemberSummary[] | undefined; myId: string; error?: string }) {
  const [query, setQuery] = createSignal("");
  const [shown, setShown] = createSignal(MEMBERS_PAGE);
  const [open, setOpen] = createSignal<string | undefined>();
  const serverId = createMemo(() => props.server.serverId);
  createEffect(
    on(
      serverId,
      () => {
        setQuery("");
        setShown(MEMBERS_PAGE);
        setOpen(undefined);
      },
      { defer: true },
    ),
  );
  const found = createMemo(() => searched(props.members ?? [], query()));
  const offersOf = (m: MemberSummary) => memberOffers(props.server, m, m.accountId === props.myId);
  const entries = createMemo<RosterEntry[]>(
    (had) =>
      keptRows(
        had,
        found()
          .slice(0, shown())
          .map((m): RosterEntry => {
            const name = memberName(m);
            return { id: m.accountId, name, initials: initialsOf(name), src: avatarSrc(m.avatarUrl), tone: toneFor(m.accountId), handle: m.handle, title: m.title || undefined, mark: memberMark(m), roles: rolesHeld(props.server.roles, m.roleIds), still: !offersAny(offersOf(m)) };
          }),
      ),
    [],
  );
  // The member whose row is open, as the list now has them; gone, the row closes with them.
  const opened = createMemo(() => (props.members ?? []).find((m) => m.accountId === open()));
  return (
    <>
      <div class="lu-settings__head">
        <div class="lu-settings__headtext">
          <h2 class="lu-settings__title">
            Members <Show when={props.members}>{(list) => <span class="lu-settings__count">{list().length.toLocaleString("en-US")}</span>}</Show>
          </h2>
        </div>
      </div>
      <SearchField label="Search members" value={query()} onInput={(e) => setQuery(e.currentTarget.value)} />
      <Show when={props.error}>
        <p class="auth__error" role="alert">
          {props.error}
        </p>
      </Show>
      <Show when={props.members} fallback={<Show when={!props.error}><p class="auth__note" role="status">{READING_MEMBERS}</p></Show>}>
        <Roster
          members={entries()}
          open={opened()?.accountId}
          onToggle={(m) => setOpen(open() === m.id ? undefined : m.id)}
          empty={query().trim() ? NO_MEMBER_MATCHES : undefined}
          editor={() => (
            // By who it is, not by the object: the list read again leaves what is being typed.
            <Show when={opened()?.accountId} keyed>
              {(id) => <Show when={opened()}>{(m) => <MemberEditor server={props.server} member={m()} me={id === props.myId} close={() => setOpen(undefined)} />}</Show>}
            </Show>
          )}
        />
        <Show when={found().length > shown()}>
          <Button size="sm" block onClick={() => setShown(shown() + MEMBERS_PAGE)}>
            Show more members
          </Button>
        </Show>
      </Show>
    </>
  );
}

/**
 * A member's form, in their row in Members and in a card's two pops
 * (`client/roles.ts`): the roles and the title as the form opened, and as it
 * now stands. One nobody has touched follows the member as the list comes to
 * have them; one that was touched keeps both, and Save asks for what it
 * changed in one request (`changeMember`): the roles ticked since given, the
 * ones unticked taken, the title if it differs, and nothing of what the form
 * did not touch, whatever someone else has made of the member meanwhile.
 * Nothing changed, nothing is asked. While a save or a kick is on its way
 * the form does not change.
 */
function memberForm(server: () => ServerSummary, member: () => MemberSummary) {
  const [opened, setOpened] = createSignal<MemberForm>(formOf(member()));
  const [form, setForm] = createSignal<MemberForm>(formOf(member()));
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  // Their roles and title are read here, not only the member, who can be the object they were, changed in place.
  createEffect(
    on(
      () => formOf(member()),
      (now) => {
        const next = memberFollowed(opened(), form(), now);
        setOpened(next.opened);
        setForm(next.form);
      },
      { defer: true },
    ),
  );
  const change = (part: Partial<MemberForm>) => {
    if (!busy()) setForm({ ...form(), ...part });
  };
  /** Does `what` and then `after`; a refusal is said and the form stays as it is. */
  const act = async (what: () => Promise<void>, after: () => void) => {
    if (busy()) return;
    setBusy(true);
    setError("");
    try {
      await what();
      after();
    } catch (err) {
      setError(memberRefusal(err));
      setBusy(false);
    }
  };
  const save = (after: () => void) =>
    act(async () => {
      const changes = memberChanges(opened(), form());
      if (!changesAny(changes)) return;
      const [serverId, accountId] = [server().serverId, member().accountId];
      tookMember(serverId, accountId, await client().changeMember(serverId, accountId, changes.give, changes.take, changes.title));
    }, after);
  return { form, busy, error, act, save, title: (title: string) => change({ title }), toggle: (roleId: string) => change({ roleIds: toggledRole(server().roles, form().roleIds, roleId) }) };
}

// A member's editing state under their row: the roles to tick, the title,
// and Kick, each where `memberOffers` says. Save keeps the roles and the
// title together, in one request; Kick asks nothing first.
function MemberEditor(props: { server: ServerSummary; member: MemberSummary; me: boolean; close: () => void }) {
  const offers = () => memberOffers(props.server, props.member, props.me);
  const edit = memberForm(
    () => props.server,
    () => props.member,
  );
  return (
    <MemberEdit
      name={memberName(props.member)}
      roles={offers().roles ? roleChoices(props.server.roles, edit.form().roleIds) : undefined}
      rolesNote={offers().rolesNote}
      title={offers().title ? edit.form().title : undefined}
      onToggleRole={edit.toggle}
      onTitle={edit.title}
      onCancel={props.close}
      onSave={() => void edit.save(props.close)}
      onKick={
        offers().kick
          ? () =>
              void edit.act(async () => {
                await client().removeMember(props.server.serverId, props.member.accountId);
                tookMember(props.server.serverId, props.member.accountId, null);
              }, props.close)
          : undefined
      }
      busy={edit.busy()}
      error={edit.error() || undefined}
    />
  );
}

/** What a card's Edit beside Roles opens, against it (the "Profile card" board): the roles to tick, those not this account's to change with a lock. */
export function MemberRolesPop(props: { server: ServerSummary; member: MemberSummary; anchor: HTMLElement; close: () => void; done: () => void }) {
  const edit = memberForm(
    () => props.server,
    () => props.member,
  );
  return (
    // Not closed by a press outside while its save is on its way: what came of it is said here.
    <Popover anchor={props.anchor} onClose={() => !edit.busy() && props.close()}>
      <Pop title="Roles" subtitle={`${memberName(props.member)} in ${props.server.name}`}>
        <div class="lu-pop__form">
          <RoleChoices label={`${memberName(props.member)}’s roles`} roles={roleChoices(props.server.roles, edit.form().roleIds)} onToggle={edit.toggle} />
          <Show when={edit.error()}>
            <p class="lu-pop__error" role="alert">
              {edit.error()}
            </p>
          </Show>
          <PopActions busy={edit.busy()} onCancel={props.done} onSave={() => void edit.save(props.done)} />
        </div>
      </Pop>
    </Popover>
  );
}

/** What a card's Edit beside the title opens: the title, up to 32 characters, cleared to have none. */
export function MemberTitlePop(props: { server: ServerSummary; member: MemberSummary; anchor: HTMLElement; close: () => void; done: () => void }) {
  const edit = memberForm(
    () => props.server,
    () => props.member,
  );
  return (
    <Popover anchor={props.anchor} onClose={() => !edit.busy() && props.close()}>
      <Pop title="Title" subtitle={`${memberName(props.member)} in ${props.server.name}`}>
        <form
          class="lu-pop__form"
          onSubmit={(e) => {
            e.preventDefault();
            void edit.save(props.done);
          }}
        >
          <TextField label="Title" value={edit.form().title} maxLength={LABEL_MAX} placeholder="No title" autocomplete="off" readOnly={edit.busy()} onInput={(e) => edit.title(e.currentTarget.value)} />
          <p class="lu-pop__note">{TITLE_HINT}</p>
          <Show when={edit.error()}>
            <p class="lu-pop__error" role="alert">
              {edit.error()}
            </p>
          </Show>
          <PopActions busy={edit.busy()} onCancel={props.done} submit />
        </form>
      </Pop>
    </Popover>
  );
}

// A pop's two buttons. Neither is pressed while its save is on its way: Cancel then would hide what came of it.
function PopActions(props: { busy: boolean; onCancel: () => void; onSave?: () => void; submit?: boolean }): JSX.Element {
  return (
    <div class="lu-pop__actions">
      <Button size="md" disabled={props.busy} onClick={props.onCancel}>
        Cancel
      </Button>
      <Button variant="primary" size="md" type={props.submit ? "submit" : "button"} disabled={props.busy} onClick={() => props.onSave?.()}>
        Save
      </Button>
    </div>
  );
}
