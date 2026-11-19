import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { ClientError, type RoleSummary } from "./bridge";
import * as roles from "./roles";
import {
  cardMarks,
  draftOf,
  followed,
  formOf,
  memberFollowed,
  memberRefusal,
  redressed,
  roleRefusal,
  toneOf,
} from "./roles";

// The cases every client agrees on (lumia_client::roles): the desktop and the
// phones ask the core, and this copy is held to the same answers.
type Case = { rule: string; args: Record<string, unknown>; answer: unknown; why: string };
const vectors = JSON.parse(readFileSync(resolve(import.meta.dir, "../../../../proto/vectors/roles/cases.json"), "utf8")) as {
  roles: Record<string, RoleSummary[]>;
  words: Record<string, string>;
  cases: Case[];
};

const listOf = (v: unknown): RoleSummary[] => (typeof v === "string" ? vectors.roles[v]! : (v as RoleSummary[])).map((r) => ({ ...r, position: 0 }));
const named = (list: RoleSummary[], id: unknown) => list.find((r) => r.roleId === id)!;
// The file writes "none" as null where this module says undefined, and a plain chip as no tone where it says "neutral".
const asFile = (v: unknown): unknown => (v === undefined ? null : v);

/** What this module answers for a case, as the file writes an answer. */
function answer(c: Case): unknown {
  const a = c.args as Record<string, any>;
  switch (c.rule) {
    case "settingsSections":
      return roles.settingsSections(a as never);
    case "toneOf":
      return roles.toneOf(a.color);
    case "rolesCount":
      return roles.rolesCount(listOf(a.roles));
    case "membersCount":
      return roles.membersCount(a.n);
    case "grouped":
      return roles.grouped(a.n);
    case "holders":
      return asFile(roles.holders(named(listOf(a.roles), a.roleId), a.members?.map((roleIds: string[]) => ({ roleIds }))));
    case "lockedRole":
      return roles.lockedRole(named(listOf(a.roles), a.roleId));
    case "landsBefore":
      return roles.landsBefore(listOf(a.roles), a.roleId, a.beforeId);
    case "movedBefore":
      return roles.movedBefore(listOf(a.roles), a.roleId, a.beforeId);
    case "stepBefore":
      return asFile(roles.stepBefore(listOf(a.roles), a.roleId, a.up ? -1 : 1));
    case "movedWords":
      return roles.movedWords(a.roleName, a.position);
    case "savable":
      return roles.savable(a.draft, a.opened ?? undefined);
    case "followed":
      return roles.followed(a.opened, a.draft, a.now);
    case "withheld":
      return roles.withheld({ grantable: a.grantable }, a.permission);
    case "rolesHeld":
      return roles.rolesHeld(listOf(a.roles), a.roleIds);
    case "roleChoices":
      return roles.roleChoices(listOf(a.roles), a.held);
    case "toggledRole":
      return roles.toggledRole(listOf(a.roles), a.held, a.roleId);
    case "memberChanges":
      return roles.memberChanges(a.opened, a.form);
    case "changesAny":
      return roles.changesAny(a.changes);
    case "memberFollowed":
      return roles.memberFollowed(a.opened, a.form, a.now);
    case "memberOffers": {
      const o = roles.memberOffers({ canManageRoles: a.canManageRoles }, { guest: a.guest, canManage: a.canManage }, a.me);
      return { ...o, rolesNote: asFile(o.rolesNote) };
    }
    case "offersAny":
      return roles.offersAny({ ...a.offers, rolesNote: a.offers.rolesNote ?? undefined });
    case "memberMark":
      return asFile(roles.memberMark(a as never));
    case "cardMarks":
      return roles.cardMarks(a.me, a.member ?? undefined);
    case "tagOf": {
      const t = roles.tagOf(a.tag);
      return t ? { label: t.label, tone: t.tone === "neutral" ? null : t.tone } : null;
    }
    case "found":
      return roles.found(a.query, a as never);
    case "roleNameHint":
      return roles.roleNameHint(a.serverName);
    case "tagSwitchCaption":
      return roles.tagSwitchCaption(a.shown);
    case "withheldCaption":
      return roles.withheldCaption(a.detail);
    case "deleteTitle":
      return roles.deleteTitle(a.roleName);
    case "administratorQuestion":
      return roles.administratorQuestion(a.serverName, a.roleName, a.held ?? undefined);
    case "deleteQuestion":
      return roles.deleteQuestion(a.held ?? undefined);
    case "roleRefusal":
      return roles.roleRefusal(a.code ? { code: a.code, details: a.field ? { field: a.field } : {}, message: a.message } : new Error(a.message), a.serverName);
    case "memberRefusal":
      return roles.memberRefusal(a.code ? { code: a.code, details: a.field ? { field: a.field } : {}, message: a.message } : a.message);
    default:
      throw new Error(`no rule ${c.rule}`);
  }
}

describe("the shared cases", () => {
  for (const c of vectors.cases) {
    test(`${c.rule}: ${c.why}`, () => {
      expect(answer(c)).toEqual(c.answer);
    });
  }
  test("the words are the core's", () => {
    const words: Record<string, string> = {
      ROLES_HINT: roles.ROLES_HINT,
      ROLES_HINT_MENU: roles.ROLES_HINT_MENU,
      EVERYONE_SUBTITLE: roles.EVERYONE_SUBTITLE,
      TITLE_HINT: roles.TITLE_HINT,
      GUEST_ROLES_NOTE: roles.GUEST_ROLES_NOTE,
      DELETE_ROLE_HINT: roles.DELETE_ROLE_HINT,
      TAG_HEADING: roles.TAG_HEADING,
      TAG_SUBTITLE: roles.TAG_SUBTITLE,
      TAG_NOTE: roles.TAG_NOTE,
      TAG_ROLES_LABEL: roles.TAG_CHOICES.roles.label,
      TAG_ROLES_CAPTION: roles.TAG_CHOICES.roles.caption,
      TAG_TITLES_LABEL: roles.TAG_CHOICES.titles.label,
      TAG_TITLES_CAPTION: roles.TAG_CHOICES.titles.caption,
      NOBODY_HOLDS: roles.NOBODY_HOLDS,
      EVERYONE_HOLDS: roles.EVERYONE_HOLDS,
      NO_MEMBER_MATCHES: roles.NO_MEMBER_MATCHES,
      READING_MEMBERS: roles.READING_MEMBERS,
      NO_ROLES: roles.NO_ROLES,
      ADMINISTRATOR_TITLE: roles.ADMINISTRATOR_TITLE,
      ADMINISTRATOR: roles.ADMINISTRATOR,
    };
    expect(words).toEqual(vectors.words);
  });
});

// What is this page's own, beyond the shared cases: the objects a page holds
// handed back where nothing is to change, so what draws them stays as it is;
// the worker's error text read for its code; and when a snapshot means the
// members held are to be read again.

function role(roleId: string, over: Partial<RoleSummary> = {}): RoleSummary {
  return { roleId, name: roleId, color: "", position: 0, isEveryone: false, hideTag: false, permissions: [], canManage: true, ...over };
}

// Acme as Devon sees it: Admin and Moderator are at or above Devon's highest.
const ACME = [
  role("admin", { color: "indigo", canManage: false }),
  role("mod", { color: "green", canManage: false }),
  role("design", { color: "peach" }),
  role("eng", { color: "sky" }),
  role("invite", { hideTag: true }),
  role("everyone", { name: "Everyone", isEveryone: true }),
];




describe("this page's own", () => {
  const mod = role("mod", { name: "Moderator", color: "green", permissions: ["MANAGE_ROLES", "SEND_IN_THREADS"] });

  test("an editor's drafts are handed back as they were where nothing is to change", () => {
    const opened = draftOf(mod);
    const now = draftOf({ ...mod, name: "Mods", permissions: ["MANAGE_ROLES"] });
    // Touched: the drafts handed in stay.
    const touched = { ...opened, color: "sky" };
    expect(followed(opened, touched, now).opened).toBe(opened);
    expect(followed(opened, touched, now).draft).toBe(touched);
    // The role read again as it was.
    const untouched = draftOf(mod);
    expect(followed(opened, untouched, draftOf(mod)).opened).toBe(opened);
    expect(followed(opened, untouched, draftOf(mod)).draft).toBe(untouched);
    // A space typed after the name is a touch.
    const typing = { ...opened, name: "Moderator " };
    expect(followed(opened, typing, now).draft).toBe(typing);
  });

  test("a member's forms are handed back as they were where nothing is to change", () => {
    const opened = formOf({ roleIds: ["design"], title: "" });
    const untouched = formOf({ roleIds: ["design"], title: "" });
    expect(memberFollowed(opened, untouched, formOf({ roleIds: ["design"], title: "" })).form).toBe(untouched);
    const typing = { roleIds: ["design"], title: "L" };
    expect(memberFollowed(opened, typing, formOf({ roleIds: ["design", "mod"], title: "Lead" })).form).toBe(typing);
  });

  test("a tone is one of the nine, typed so", () => {
    expect(toneOf("sky")).toBe("sky");
    expect(toneOf("#ff0000")).toBeNull();
  });

  test("a card at Home has no member", () => {
    expect(cardMarks(true, undefined)).toEqual(["You"]);
  });

  test("a snapshot that dresses a server's members otherwise names it, to read them again", () => {
    const acme = { serverId: "acme", roles: ACME, tagTitles: false, ownerAccountId: "ryan", isOwner: false, canManageMembers: false, roleIds: ["eng"] };
    const other = { ...acme, serverId: "other" };
    const was = { servers: [acme, other] };
    expect(redressed(was, { servers: [{ ...acme, roles: [...ACME], roleIds: ["eng"] }, other] })).toEqual([]);
    // Given a role with Manage members: this account's roles there and what it may do, though no role changed.
    expect(redressed(was, { servers: [{ ...acme, roleIds: ["eng", "mod"], canManageMembers: true }, other] })).toEqual(["acme"]);
    expect(redressed(was, { servers: [{ ...acme, roleIds: ["eng", "mod"] }, other] })).toEqual(["acme"]);
    expect(redressed(was, { servers: [acme, { ...other, canManageMembers: true }] })).toEqual(["other"]);
    // A role renamed, recoloured, moved or no longer this account's to manage; the other of the two as the tag; another owner.
    expect(redressed(was, { servers: [{ ...acme, roles: ACME.map((r) => (r.roleId === "eng" ? { ...r, name: "Engineering" } : r)) }, other] })).toEqual(["acme"]);
    expect(redressed(was, { servers: [{ ...acme, roles: [ACME[1]!, ACME[0]!, ...ACME.slice(2)] }, other] })).toEqual(["acme"]);
    expect(redressed(was, { servers: [{ ...acme, tagTitles: true }, { ...other, ownerAccountId: "maya", isOwner: true }] })).toEqual(["acme", "other"]);
    // The first snapshot, and a server just joined: their members have not been read by another.
    expect(redressed(undefined, was)).toEqual([]);
    expect(redressed({ servers: [acme] }, was)).toEqual([]);
  });

  test("refusals as the worker reports them: the code and the field read from the core's words", () => {
    expect(memberRefusal(new ClientError("api error RATE_LIMITED: too many title changes"))).toBe("You’ve changed your title too many times this hour. Try again later.");
    expect(memberRefusal(new ClientError("api error INVALID_ARGUMENT: title details=field=title"))).toBe("A title is up to 32 characters on one line, with no hidden characters.");
    expect(roleRefusal(new ClientError("api error INVALID_ARGUMENT: name details=field=name"), "Acme")).toContain("no hidden characters");
    expect(roleRefusal(new Error("transport: offline"), "Acme")).toBe("transport: offline");
  });
});
