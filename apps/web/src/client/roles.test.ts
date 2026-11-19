import { describe, expect, test } from "bun:test";
import { ClientError, type RoleSummary } from "./bridge";
import {
  administratorQuestion,
  cardMarks,
  changesAny,
  deleteQuestion,
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
  offersAny,
  redressed,
  roleChoices,
  roleRefusal,
  rolesCount,
  rolesHeld,
  savable,
  searched,
  settingsSections,
  stepBefore,
  tagOf,
  toggledRole,
  toneOf,
  withheld,
} from "./roles";

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

describe("sections", () => {
  test("each permission opens its own, and Manage roles opens Members too", () => {
    expect(settingsSections({ canManage: true, canManageRoles: true, canManageMembers: true })).toEqual(["overview", "roles", "members"]);
    expect(settingsSections({ canManage: true, canManageRoles: false, canManageMembers: false })).toEqual(["overview"]);
    expect(settingsSections({ canManage: false, canManageRoles: true, canManageMembers: false })).toEqual(["roles", "members"]);
    expect(settingsSections({ canManage: false, canManageRoles: false, canManageMembers: true })).toEqual(["members"]);
    expect(settingsSections({ canManage: false, canManageRoles: false, canManageMembers: false })).toEqual([]);
  });
});

describe("the list", () => {
  test("counts roles without Everyone", () => {
    expect(rolesCount(ACME)).toBe("5 roles");
    expect(rolesCount([role("a"), role("everyone", { isEveryone: true })])).toBe("1 role");
  });
  test("counts who holds a role, and everyone holds Everyone", () => {
    const members = [{ roleIds: ["eng"] }, { roleIds: ["eng", "design"] }, { roleIds: [] }];
    expect(holders(ACME[3]!, members)).toBe(2);
    expect(holders(ACME[5]!, members)).toBe(3);
    expect(holders(ACME[3]!, undefined)).toBeUndefined();
  });
  test("a role not managed is locked, and Everyone never is", () => {
    expect(lockedRole(ACME[0]!)).toBe(true);
    expect(lockedRole(ACME[2]!)).toBe(false);
    expect(lockedRole(role("everyone", { isEveryone: true, canManage: false }))).toBe(false);
  });
  test("a tone is one of the nine", () => {
    expect(toneOf("sky")).toBe("sky");
    expect(toneOf("")).toBeNull();
    expect(toneOf("#ff0000")).toBeNull();
  });
});

describe("moving a role", () => {
  test("lands before a role it manages or before Everyone", () => {
    expect(landsBefore(ACME, "eng", "design")).toBe(true);
    expect(landsBefore(ACME, "design", "everyone")).toBe(true);
    expect(landsBefore(ACME, "design", "invite")).toBe(true);
  });
  test("never before a locked role, never where it is, and a locked role and Everyone do not move", () => {
    expect(landsBefore(ACME, "eng", "mod")).toBe(false);
    expect(landsBefore(ACME, "eng", "admin")).toBe(false);
    expect(landsBefore(ACME, "eng", "eng")).toBe(false);
    expect(landsBefore(ACME, "eng", "invite")).toBe(false);
    expect(landsBefore(ACME, "mod", "everyone")).toBe(false);
    expect(landsBefore(ACME, "everyone", "design")).toBe(false);
    expect(landsBefore(ACME, "gone", "design")).toBe(false);
  });
  test("the order sent is every role but Everyone", () => {
    expect(movedBefore(ACME, "eng", "design")).toEqual(["admin", "mod", "eng", "design", "invite"]);
    expect(movedBefore(ACME, "design", "everyone")).toEqual(["admin", "mod", "eng", "invite", "design"]);
  });
  test("a step up or down, where there is one", () => {
    expect(stepBefore(ACME, "eng", -1)).toBe("design");
    expect(stepBefore(ACME, "eng", 1)).toBe("everyone");
    expect(stepBefore(ACME, "design", 1)).toBe("invite");
    // Design is the highest Devon manages, and Can invite the last above Everyone.
    expect(stepBefore(ACME, "design", -1)).toBeUndefined();
    expect(stepBefore(ACME, "invite", 1)).toBeUndefined();
    expect(stepBefore(ACME, "admin", 1)).toBeUndefined();
  });
});

describe("the editor", () => {
  const mod = role("mod", { name: "Moderator", color: "green", permissions: ["MANAGE_ROLES", "SEND_IN_THREADS"] });
  test("a new role saves once it has a name", () => {
    expect(savable(draftOf())).toBe(false);
    expect(savable({ ...draftOf(), name: "  " })).toBe(false);
    expect(savable({ ...draftOf(), name: "Design" })).toBe(true);
  });
  test("a role saves once something differs from what the editor opened on", () => {
    const opened = draftOf(mod);
    expect(savable(draftOf(mod), opened)).toBe(false);
    expect(savable({ ...opened, name: " Moderator " }, opened)).toBe(false);
    expect(savable({ ...opened, name: "Mods" }, opened)).toBe(true);
    expect(savable({ ...opened, name: "" }, opened)).toBe(false);
    expect(savable({ ...opened, color: "" }, opened)).toBe(true);
    expect(savable({ ...opened, hideTag: true }, opened)).toBe(true);
    expect(savable({ ...opened, permissions: ["SEND_IN_THREADS", "MANAGE_ROLES"] }, opened)).toBe(false);
    expect(savable({ ...opened, permissions: ["MANAGE_ROLES"] }, opened)).toBe(true);
  });
  test("of Everyone only the permissions change", () => {
    const everyone = draftOf(role("everyone", { name: "Everyone", isEveryone: true, permissions: ["VIEW_CHANNEL"] }));
    expect(savable({ ...everyone }, everyone)).toBe(false);
    expect(savable({ ...everyone, permissions: [] }, everyone)).toBe(true);
  });
  test("what someone else changed of the role does not light Save, and is not this editor's to send", () => {
    const opened = draftOf(mod);
    // Another admin switched a permission off and renamed it; this editor, touched or not, is compared with what it opened on.
    const now = draftOf({ ...mod, name: "Mods", permissions: ["MANAGE_ROLES"] });
    expect(savable(draftOf(mod), opened)).toBe(false);
    const touched = { ...opened, color: "sky" };
    expect(savable(touched, opened)).toBe(true);
    // Touched, it keeps both drafts: the save is the colour and nothing else.
    expect(followed(opened, touched, now)).toEqual({ opened, draft: touched });
    expect(followed(opened, touched, now).opened).toBe(opened);
  });
  test("an editor nobody has touched follows the role as it changes underneath", () => {
    const opened = draftOf(mod);
    const now = draftOf({ ...mod, name: "Mods", permissions: ["MANAGE_ROLES"] });
    const next = followed(opened, draftOf(mod), now);
    expect(next).toEqual({ opened: now, draft: now });
    expect(savable(next.draft, next.opened)).toBe(false);
    // The role read again as it was is no change: the drafts handed in are handed back.
    const same = draftOf(mod);
    const untouched = draftOf(mod);
    expect(followed(opened, untouched, same).opened).toBe(opened);
    expect(followed(opened, untouched, same).draft).toBe(untouched);
    // A space typed after the name is a touch, though nothing to save yet: it is not typed over.
    const typing = { ...opened, name: "Moderator " };
    expect(savable(typing, opened)).toBe(false);
    expect(followed(opened, typing, now).draft).toBe(typing);
    // Changed and changed back, it is untouched again, and follows.
    expect(followed(opened, { ...opened, permissions: ["SEND_IN_THREADS", "MANAGE_ROLES"] }, now).draft).toBe(now);
    // A role just made: its editor opens on nothing, before the snapshot with the role, and takes it when it comes.
    expect(followed(draftOf(), draftOf(), opened)).toEqual({ opened, draft: opened });
  });
  test("a permission not held is withheld", () => {
    expect(withheld({ grantable: ["MANAGE_ROLES"] }, "ADMINISTRATOR")).toBe(true);
    expect(withheld({ grantable: ["MANAGE_ROLES"] }, "MANAGE_ROLES")).toBe(false);
  });
  test("the questions count who holds the role", () => {
    expect(administratorQuestion("Acme", "Moderator", 3)).toEqual(["Administrator allows everything in Acme, in every channel, whatever a channel’s own permissions say.", "3 members hold Moderator."]);
    expect(administratorQuestion("Acme", "Moderator", 1)[1]).toBe("1 member holds Moderator.");
    expect(administratorQuestion("Acme", "", undefined)).toHaveLength(1);
    expect(administratorQuestion("Acme", "Moderator", 0)).toHaveLength(1);
    expect(deleteQuestion(3)).toEqual(["3 members hold this role. They lose what it allowed.", "This can’t be undone."]);
    expect(deleteQuestion(1)[0]).toBe("1 member holds this role. They lose what it allowed.");
    expect(deleteQuestion(0)).toEqual(["Nobody holds this role.", "This can’t be undone."]);
  });
  test("a role is not said to be nobody's while who holds it is not known", () => {
    // The members still being read, or not read at all.
    expect(holders(ACME[1]!, undefined)).toBeUndefined();
    expect(deleteQuestion(undefined)).toEqual(["This can’t be undone."]);
  });
});

describe("a member", () => {
  test("their roles in the server's order, without Everyone", () => {
    expect(rolesHeld(ACME, ["invite", "everyone", "design"])).toEqual([
      { roleId: "design", label: "design", tone: "peach" },
      { roleId: "invite", label: "invite", tone: null },
    ]);
  });
  test("the roles to tick mark what is not this account's to give", () => {
    const choices = roleChoices(ACME, ["mod", "eng"]);
    expect(choices.map((c) => c.id)).toEqual(["admin", "mod", "design", "eng", "invite"]);
    expect(choices.filter((c) => c.checked).map((c) => c.id)).toEqual(["mod", "eng"]);
    expect(choices.filter((c) => c.locked).map((c) => c.id)).toEqual(["admin", "mod"]);
  });
  test("a tick gives and takes, and leaves a locked role as it was", () => {
    expect(toggledRole(ACME, ["eng"], "design")).toEqual(["eng", "design"]);
    expect(toggledRole(ACME, ["eng", "design"], "eng")).toEqual(["design"]);
    expect(toggledRole(ACME, ["mod"], "mod")).toEqual(["mod"]);
    expect(toggledRole(ACME, [], "admin")).toEqual([]);
    expect(toggledRole(ACME, [], "everyone")).toEqual([]);
  });
  test("what is offered of a member", () => {
    const roles = { canManageRoles: true };
    const none = { canManageRoles: false };
    expect(memberOffers(roles, { guest: false, canManage: true }, false)).toEqual({ roles: true, rolesNote: undefined, title: true, kick: true });
    // Above this account: their roles below its own can still be given and taken.
    expect(memberOffers(roles, { guest: false, canManage: false }, false)).toEqual({ roles: true, rolesNote: undefined, title: false, kick: false });
    expect(memberOffers(roles, { guest: true, canManage: true }, false)).toEqual({ roles: false, rolesNote: "Guests aren’t given roles.", title: true, kick: true });
    expect(memberOffers(none, { guest: false, canManage: true }, false)).toEqual({ roles: false, rolesNote: undefined, title: true, kick: true });
    // This account itself: its title is its own, and it is never offered Kick.
    expect(memberOffers(none, { guest: false, canManage: false }, true)).toEqual({ roles: false, rolesNote: undefined, title: true, kick: false });
    expect(offersAny(memberOffers(none, { guest: false, canManage: false }, false))).toBe(false);
    expect(offersAny(memberOffers(roles, { guest: true, canManage: false }, false))).toBe(true);
  });
  test("a save of a member's form asks for what the form changed, against the form as it opened", () => {
    const opened = formOf({ roleIds: ["eng", "design"], title: "Staff Engineer" });
    expect(memberChanges(opened, { roleIds: ["design", "eng"], title: " Staff Engineer " })).toEqual({ give: [], take: [], title: null });
    expect(changesAny(memberChanges(opened, formOf({ roleIds: ["design", "eng"], title: "Staff Engineer" })))).toBe(false);
    // One ticked, one unticked, the title retyped: one request with all three.
    const changes = memberChanges(opened, { roleIds: ["eng", "invite"], title: " Principal " });
    expect(changes).toEqual({ give: ["invite"], take: ["design"], title: "Principal" });
    expect(changesAny(changes)).toBe(true);
    // A title cleared is a title to set: empty, not left alone.
    expect(memberChanges(opened, { ...opened, title: "  " })).toEqual({ give: [], take: [], title: "" });
    expect(changesAny(memberChanges(opened, { ...opened, title: "" }))).toBe(true);
    expect(changesAny(memberChanges(opened, { ...opened, roleIds: ["eng"] }))).toBe(true);
  });
  test("a role someone else gave the member meanwhile is not taken by a save from a form opened before it", () => {
    // The row was open on [design]; Mod was given elsewhere; here Design is unticked.
    const opened = formOf({ roleIds: ["design"], title: "" });
    const form = { roleIds: [], title: "" };
    expect(memberChanges(opened, form)).toEqual({ give: [], take: ["design"], title: null });
    // Touched, the form keeps what it opened with, so Mod is in neither list whenever the save is made.
    const now = formOf({ roleIds: ["design", "mod"], title: "" });
    expect(memberFollowed(opened, form, now)).toEqual({ opened, form });
    expect(memberChanges(memberFollowed(opened, form, now).opened, form).take).toEqual(["design"]);
  });
  test("a member's form nobody has touched follows the member", () => {
    const opened = formOf({ roleIds: ["design"], title: "" });
    const now = formOf({ roleIds: ["design", "mod"], title: "Lead" });
    expect(memberFollowed(opened, formOf({ roleIds: ["design"], title: "" }), now)).toEqual({ opened: now, form: now });
    // Read again as they were: the forms handed in are handed back.
    const untouched = formOf({ roleIds: ["design"], title: "" });
    expect(memberFollowed(opened, untouched, formOf({ roleIds: ["design"], title: "" })).form).toBe(untouched);
    // A title being typed is a touch.
    const typing = { roleIds: ["design"], title: "L" };
    expect(memberFollowed(opened, typing, now).form).toBe(typing);
  });
  test("the owner and a guest are marked", () => {
    expect(memberMark({ owner: true, guest: false })).toBe("Owner");
    expect(memberMark({ owner: false, guest: true })).toBe("Guest");
    expect(memberMark({ owner: false, guest: false })).toBeUndefined();
  });
  test("this account's own card says “You” and who it is to the server, both", () => {
    expect(cardMarks(true, { owner: true, guest: false })).toEqual(["You", "Owner"]);
    expect(cardMarks(true, { owner: false, guest: true })).toEqual(["You", "Guest"]);
    expect(cardMarks(true, { owner: false, guest: false })).toEqual(["You"]);
    expect(cardMarks(false, { owner: true, guest: false })).toEqual(["Owner"]);
    expect(cardMarks(false, { owner: false, guest: false })).toEqual([]);
    // At Home, or before the server's members are read.
    expect(cardMarks(true, undefined)).toEqual(["You"]);
    expect(cardMarks(false, undefined)).toEqual([]);
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
  test("a tag on its tone or the plain chip", () => {
    expect(tagOf({ label: "Moderator", color: "green" })).toEqual({ label: "Moderator", tone: "green" });
    expect(tagOf({ label: "Staff Engineer", color: "" })).toEqual({ label: "Staff Engineer", tone: "neutral" });
    expect(tagOf(null)).toBeUndefined();
    expect(tagOf({ label: "", color: "green" })).toBeUndefined();
  });
  test("a search reads the name, the handle and the title", () => {
    const people = [
      { displayName: "Devon Park", nickname: "", handle: "devon", title: "Staff Engineer" },
      { displayName: "Maya Chen", nickname: "", handle: "maya", title: "" },
    ];
    expect(searched(people, "")).toHaveLength(2);
    expect(searched(people, "ENGINEER").map((p) => p.handle)).toEqual(["devon"]);
    expect(searched(people, "@maya").map((p) => p.handle)).toEqual(["maya"]);
    expect(searched(people, "chen").map((p) => p.handle)).toEqual(["maya"]);
    expect(searched(people, "nobody")).toEqual([]);
  });
});

describe("refusals", () => {
  test("a role's", () => {
    expect(roleRefusal({ code: "ALREADY_EXISTS" }, "Acme")).toBe("Another role in Acme has that name.");
    expect(roleRefusal({ code: "INVALID_ARGUMENT", details: { field: "name" } }, "Acme")).toBe("A role’s name is 1 to 32 characters on one line, with no hidden characters, and can’t be “everyone”.");
    expect(roleRefusal({ code: "INVALID_ARGUMENT", details: { field: "role_ids" } }, "Acme")).toContain("Try again");
    expect(roleRefusal({ code: "PERMISSION_DENIED" }, "Acme")).toContain("isn’t yours to change");
    expect(roleRefusal(new Error("transport: offline"), "Acme")).toBe("transport: offline");
  });
  test("a member's", () => {
    expect(memberRefusal({ code: "NOT_FOUND" })).toBe("They are no longer a member.");
    expect(memberRefusal({ code: "INVALID_ARGUMENT", details: { field: "title" } })).toBe("A title is up to 32 characters on one line, with no hidden characters.");
    expect(memberRefusal({ code: "RATE_LIMITED" })).toBe("You’ve changed your title too many times this hour. Try again later.");
    expect(memberRefusal("nope")).toBe("nope");
  });
  test("as the worker reports them: the code and the field read from the core's words", () => {
    expect(memberRefusal(new ClientError("api error RATE_LIMITED: too many title changes"))).toBe("You’ve changed your title too many times this hour. Try again later.");
    expect(memberRefusal(new ClientError("api error INVALID_ARGUMENT: title details=field=title"))).toBe("A title is up to 32 characters on one line, with no hidden characters.");
    expect(roleRefusal(new ClientError("api error INVALID_ARGUMENT: name details=field=name"), "Acme")).toContain("no hidden characters");
  });
});
