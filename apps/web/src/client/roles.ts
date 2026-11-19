// Roles and titles (architecture 7.5): what a server's settings, a role's
// editor, a member's row and the profile card share that needs no page.
// The core says what this account may do (each role's and member's
// `canManage`, the server's `grantable` and its `canManage…` switches) and
// the instance decides every write; this reads those into what a screen
// offers, and holds the words every screen says the same way.
import type { MemberSummary, RoleSummary, ServerSummary, TagSummary } from "./bridge";

/** The nine tones a role may have, as the design system names them. */
export const TONES = ["peach", "blue", "purple", "mint", "amber", "sky", "stone", "green", "indigo"] as const;
export type RoleTone = (typeof TONES)[number];

/** The most characters a role's name or a title keeps. */
export const LABEL_MAX = 32;

/** A role's or a tag's colour as a tone; null for none, and for anything that is no tone. */
export function toneOf(color: string): RoleTone | null {
  return (TONES as readonly string[]).includes(color) ? (color as RoleTone) : null;
}

export type SettingsSection = "overview" | "roles" | "members";

/**
 * The sections of a server's settings this account may use, in order:
 * Overview with Manage server, Roles with Manage roles, and Members with
 * Manage members or Manage roles, since a member's roles are changed there
 * too. None, and the settings do not open.
 */
export function settingsSections(server: Pick<ServerSummary, "canManage" | "canManageRoles" | "canManageMembers">): SettingsSection[] {
  const sections: SettingsSection[] = [];
  if (server.canManage) sections.push("overview");
  if (server.canManageRoles) sections.push("roles");
  if (server.canManageMembers || server.canManageRoles) sections.push("members");
  return sections;
}

/** "5 roles": a server's roles counted without Everyone. */
export function rolesCount(roles: Pick<RoleSummary, "isEveryone">[]): string {
  const n = roles.filter((r) => !r.isEveryone).length;
  return n === 1 ? "1 role" : `${n} roles`;
}

/** "1 member", "1,204 members". */
export function membersCount(n: number): string {
  return n === 1 ? "1 member" : `${n.toLocaleString("en-US")} members`;
}

/** How many of `members` hold a role; Everyone is held by all of them. Undefined until the members are known. */
export function holders(role: Pick<RoleSummary, "roleId" | "isEveryone">, members: Pick<MemberSummary, "roleIds">[] | undefined): number | undefined {
  if (!members) return undefined;
  return role.isEveryone ? members.length : members.filter((m) => m.roleIds.includes(role.roleId)).length;
}

/** A role at or above this account's own highest: listed with a lock, and it does not open. Everyone always opens. */
export function lockedRole(role: Pick<RoleSummary, "isEveryone" | "canManage">): boolean {
  return !role.isEveryone && !role.canManage;
}

type Ordered = Pick<RoleSummary, "roleId" | "isEveryone" | "canManage">;

/**
 * Where a role may land when it is moved: before `beforeId`, a role this
 * account manages, or before Everyone, which is last. A role is never put
 * before one that is locked, since those are at or above this account's
 * own highest, and landing where it already is moves nothing.
 */
export function landsBefore(roles: Ordered[], roleId: string, beforeId: string): boolean {
  const from = roles.findIndex((r) => r.roleId === roleId);
  const to = roles.findIndex((r) => r.roleId === beforeId);
  const moved = roles[from];
  const before = roles[to];
  if (!moved || !before || lockedRole(moved) || moved.isEveryone) return false;
  if (to === from || to === from + 1) return false;
  return before.isEveryone || !lockedRole(before);
}

/** The whole order a move sends: every role but Everyone, with `roleId` now before `beforeId`. */
export function movedBefore(roles: Ordered[], roleId: string, beforeId: string): string[] {
  const order: string[] = [];
  for (const r of roles) {
    if (r.roleId === roleId) continue;
    if (r.roleId === beforeId) order.push(roleId);
    if (!r.isEveryone) order.push(r.roleId);
  }
  return order;
}

/** The role a step up (-1) or down (1) puts `roleId` before; undefined where there is no such step (the highest it may go, or last above Everyone). */
export function stepBefore(roles: Ordered[], roleId: string, step: -1 | 1): string | undefined {
  const at = roles.findIndex((r) => r.roleId === roleId);
  const before = roles[step === -1 ? at - 1 : at + 2];
  return at >= 0 && before && landsBefore(roles, roleId, before.roleId) ? before.roleId : undefined;
}

/** What a role's editor holds while it is open. */
export interface RoleDraft {
  name: string;
  /** A tone, or empty for none. */
  color: string;
  hideTag: boolean;
  /** The permissions switched on, by their wire names. */
  permissions: string[];
}

/** The editor's fields for a role, or for a new one: no name, no colour, shown as a tag, nothing granted. */
export function draftOf(role?: Pick<RoleSummary, "name" | "color" | "hideTag" | "permissions">): RoleDraft {
  return role ? { name: role.name, color: role.color, hideTag: role.hideTag, permissions: [...role.permissions] } : { name: "", color: "", hideTag: false, permissions: [] };
}

function sameSet(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((x) => b.includes(x));
}

/**
 * Whether Save is offered: a new role once it has a name, a role once
 * something of it differs and it still has one. Of Everyone only the
 * permissions are compared, since nothing else of it changes.
 */
export function savable(draft: RoleDraft, role?: Pick<RoleSummary, "name" | "color" | "hideTag" | "permissions" | "isEveryone">): boolean {
  if (!role) return draft.name.trim() !== "";
  if (role.isEveryone) return !sameSet(draft.permissions, role.permissions);
  if (draft.name.trim() === "") return false;
  return draft.name.trim() !== role.name || draft.color !== role.color || draft.hideTag !== role.hideTag || !sameSet(draft.permissions, role.permissions);
}

/** A permission this account does not hold, and so cannot give or take: its switch is shown and not turned. */
export function withheld(server: Pick<ServerSummary, "grantable">, permission: string): boolean {
  return !server.grantable.includes(permission);
}

/** The permission whose switch asks before it is turned on. */
export const ADMINISTRATOR = "ADMINISTRATOR";

/** A role a member holds, as a card and a settings row list it: its name, on its tone or the plain chip. */
export interface HeldRole {
  roleId: string;
  label: string;
  tone: RoleTone | null;
}

/** Every role a member holds, in the server's order, without Everyone. */
export function rolesHeld(roles: RoleSummary[], roleIds: string[]): HeldRole[] {
  return roles.filter((r) => !r.isEveryone && roleIds.includes(r.roleId)).map((r) => ({ roleId: r.roleId, label: r.name, tone: toneOf(r.color) }));
}

/** A member's roles to tick: the server's in order without Everyone, each not this account's to give or take marked. */
export function roleChoices(roles: RoleSummary[], held: string[]): { id: string; name: string; tone: RoleTone | null; checked: boolean; locked: boolean }[] {
  return roles.filter((r) => !r.isEveryone).map((r) => ({ id: r.roleId, name: r.name, tone: toneOf(r.color), checked: held.includes(r.roleId), locked: !r.canManage }));
}

/** The roles a tick leaves a member with; a locked role is not changed by one. */
export function toggledRole(roles: RoleSummary[], held: string[], roleId: string): string[] {
  const role = roles.find((r) => r.roleId === roleId);
  if (!role || role.isEveryone || !role.canManage) return held;
  return held.includes(roleId) ? held.filter((id) => id !== roleId) : [...held, roleId];
}

/** What this account may change of a member, on their row in Members and on their card. */
export interface MemberOffers {
  /** Their roles to tick: with Manage roles, for anyone but a guest. */
  roles: boolean;
  /** In the roles' place, for a guest. */
  rolesNote?: string;
  /** Their title: this account's own, or that of a member below it with Manage members. */
  title: boolean;
  /** Kick: with Manage members, a member below this account, never itself. */
  kick: boolean;
}

export function memberOffers(server: Pick<ServerSummary, "canManageRoles">, member: Pick<MemberSummary, "guest" | "canManage">, me: boolean): MemberOffers {
  const roles = server.canManageRoles && !member.guest;
  return {
    roles,
    rolesNote: server.canManageRoles && member.guest ? GUEST_ROLES_NOTE : undefined,
    title: me || member.canManage,
    kick: !me && member.canManage,
  };
}

/** Whether a member's row in Members opens at all: there is something of theirs to change. */
export function offersAny(offers: MemberOffers): boolean {
  return offers.roles || !!offers.rolesNote || offers.title || offers.kick;
}

/** The word beside a name on a card and in Members: the server's owner, or a guest. */
export function memberMark(member: Pick<MemberSummary, "owner" | "guest">): "Owner" | "Guest" | undefined {
  return member.owner ? "Owner" : member.guest ? "Guest" : undefined;
}

/**
 * The words beside the name on a profile card, in order: "You" on this
 * account's own, then who they are to the server the card was opened in.
 * Both are said of this account where both are so, on every shell: "You"
 * and "Owner", or "You" and "Guest". At Home there is no member, and no
 * second word.
 */
export function cardMarks(me: boolean, member: Pick<MemberSummary, "owner" | "guest"> | undefined): string[] {
  const mark = member && memberMark(member);
  return [...(me ? ["You"] : []), ...(mark ? [mark] : [])];
}

/** A member's tag as a message and a member row draw it: its words, on its tone or the plain chip. */
export function tagOf(tag: TagSummary | null | undefined): { label: string; tone: RoleTone | "neutral" } | undefined {
  return tag?.label ? { label: tag.label, tone: toneOf(tag.color) ?? "neutral" } : undefined;
}

type Dressing = Pick<ServerSummary, "serverId" | "roles" | "tagTitles" | "ownerAccountId" | "isOwner" | "canManageMembers" | "roleIds">;

/**
 * The servers of `now` whose members `was` dressed otherwise. The core
 * works out each member's tag, `owner` and `canManage` when their list is
 * read, from the snapshot it last took: the server's roles, which of the
 * two a tag shows, its owner, and this account's standing there. So when a
 * snapshot changes any of those, the members the page holds are read again
 * (`membersKept`), as the desktop shell does. A `tagsChanged` alone comes
 * before the snapshot that says what this account may now do: given a role
 * with Manage members, every member would stay `canManage: false`.
 */
export function redressed(was: { servers: Dressing[] } | undefined, now: { servers: Dressing[] }): string[] {
  if (!was) return [];
  const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
  return now.servers
    .filter((server) => {
      const old = was.servers.find((s) => s.serverId === server.serverId);
      return (
        !!old &&
        (!same(old.roles, server.roles) || !same(old.roleIds, server.roleIds) || old.tagTitles !== server.tagTitles || old.ownerAccountId !== server.ownerAccountId || old.isOwner !== server.isOwner || old.canManageMembers !== server.canManageMembers)
      );
    })
    .map((server) => server.serverId);
}

/** Members whose name, handle or title has `query` in it, whatever its capitals; all of them for nothing typed. */
export function searched<M extends Pick<MemberSummary, "displayName" | "nickname" | "handle" | "title">>(members: M[], query: string): M[] {
  const q = query.trim().toLowerCase().replace(/^@/, "");
  if (!q) return members;
  return members.filter((m) => [m.displayName, m.nickname, m.handle, m.title].some((s) => s.toLowerCase().includes(q)));
}

// ── Words ────────────────────────────────────────────────────

export const ROLES_HINT = "In order, highest first. Drag a role to move it. A new role starts last, just above Everyone.";
/** The same where a role is moved from its menu alone (a phone, a narrow window). */
export const ROLES_HINT_MENU = "In order, highest first. A role is moved from its menu. A new role starts last, just above Everyone.";
export const EVERYONE_SUBTITLE = "What every member may do before any role adds to it. Guests are not among them.";
export const TITLE_HINT = "Up to 32 characters. A title grants nothing.";
export const GUEST_ROLES_NOTE = "Guests aren’t given roles.";
export const DELETE_ROLE_HINT = "Its members lose what it allowed. This can’t be undone.";
export const TAG_HEADING = "Tag beside names";
export const TAG_SUBTITLE = "What shows beside a member’s name in messages and in the member list.";
export const TAG_NOTE = "A profile shows a member’s title and roles either way.";
/** Where a role's holders would be listed, and where its picker's members would be. */
export const NOBODY_HOLDS = "Nobody holds this role yet.";
export const EVERYONE_HOLDS = "Everyone who can hold this role already does.";
/** A search that found nobody, in Members and in "Add members". */
export const NO_MEMBER_MATCHES = "No member matches that.";
export const READING_MEMBERS = "Reading the members…";
/** On a profile card, under Roles, for a member who holds none where they may be given some. */
export const NO_ROLES = "No roles";
export const TAG_CHOICES = {
  roles: { label: "Roles", caption: "Each member’s highest role that is shown as a tag, in the role’s colour." },
  titles: { label: "Titles", caption: "Each member’s title, in no colour. A member without a title has no tag." },
} as const;

export function roleNameHint(serverName: string): string {
  return `1 to 32 characters. No other role in ${serverName} can have the same name.`;
}

/** Under "Show as tag", as it stands. */
export function tagSwitchCaption(shown: boolean): string {
  return shown ? "This role can be the tag beside a member’s name." : "This role only carries permissions. It is nobody’s tag.";
}

/** A permission's line where its switch is not this account's to turn. */
export function withheldCaption(detail: string): string {
  return `${detail} You don’t hold it, so it is not yours to give or take.`;
}

/** What is asked before Administrator is turned on: the lines under "Turn on Administrator?". A new role has no holders to count. */
export function administratorQuestion(serverName: string, roleName: string, held: number | undefined): string[] {
  const lines = [`Administrator allows everything in ${serverName}, in every channel, whatever a channel’s own permissions say.`];
  if (held) lines.push(held === 1 ? `1 member holds ${roleName}.` : `${held.toLocaleString("en-US")} members hold ${roleName}.`);
  return lines;
}

/**
 * What is asked before a role is deleted: the lines under "Delete
 * Moderator?". How many hold it is said only when it is known: while the
 * members are being read, or could not be, nothing is said of them, since
 * "Nobody holds this role." would be said of one forty hold.
 */
export function deleteQuestion(held: number | undefined): string[] {
  const last = "This can’t be undone.";
  if (held === undefined) return [last];
  const first = held === 0 ? "Nobody holds this role." : held === 1 ? "1 member holds this role. They lose what it allowed." : `${held.toLocaleString("en-US")} members hold this role. They lose what it allowed.`;
  return [first, last];
}

interface Refusal {
  code?: string;
  details?: Record<string, string>;
  message?: string;
}

function refusal(err: unknown): Refusal {
  return typeof err === "object" && err !== null ? (err as Refusal) : { message: String(err) };
}

/** Why a role was not saved, moved or deleted. */
export function roleRefusal(err: unknown, serverName: string): string {
  const r = refusal(err);
  if (r.code === "PERMISSION_DENIED") return "That isn’t yours to change. A role at or above your own highest, or a permission you don’t hold, is not yours to give.";
  if (r.code === "ALREADY_EXISTS") return `Another role in ${serverName} has that name.`;
  if (r.code === "NOT_FOUND") return "That role is gone. Someone deleted it.";
  if (r.code === "INVALID_ARGUMENT" && r.details?.field === "name") return "A role’s name is 1 to 32 characters on one line, with no hidden characters, and can’t be “everyone”.";
  if (r.code === "INVALID_ARGUMENT" && r.details?.field === "color") return "A role’s colour is one of the nine, or none.";
  if (r.code === "INVALID_ARGUMENT" && r.details?.field === "role_ids") return "The roles changed while you were moving one. Try again.";
  if (r.code === "FAILED_PRECONDITION") return "This server has as many roles as it can hold.";
  return r.message ?? String(err);
}

/** Why a member's roles or title were not saved, or they were not kicked. */
export function memberRefusal(err: unknown): string {
  const r = refusal(err);
  if (r.code === "PERMISSION_DENIED") return "That isn’t yours to change. They may be at or above your own highest role.";
  if (r.code === "NOT_FOUND") return "They are no longer a member.";
  if (r.code === "FAILED_PRECONDITION") return "A member can’t hold that many roles.";
  if (r.code === "INVALID_ARGUMENT" && r.details?.field === "title") return "A title is up to 32 characters on one line, with no hidden characters.";
  // A member's own title, changed more often than the instance takes in an hour.
  if (r.code === "RATE_LIMITED") return "You’ve changed your title too many times this hour. Try again later.";
  return r.message ?? String(err);
}
