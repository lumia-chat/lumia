import { describe, expect, test } from "bun:test";
import type { RoleSummary } from "./bridge";
import {
  administratorQuestion,
  deleteQuestion,
  draftOf,
  holders,
  landsBefore,
  lockedRole,
  memberMark,
  memberOffers,
  memberRefusal,
  movedBefore,
  offersAny,
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
  test("a role saves once something differs", () => {
    expect(savable(draftOf(mod), mod)).toBe(false);
    expect(savable({ ...draftOf(mod), name: " Moderator " }, mod)).toBe(false);
    expect(savable({ ...draftOf(mod), name: "Mods" }, mod)).toBe(true);
    expect(savable({ ...draftOf(mod), name: "" }, mod)).toBe(false);
    expect(savable({ ...draftOf(mod), color: "" }, mod)).toBe(true);
    expect(savable({ ...draftOf(mod), hideTag: true }, mod)).toBe(true);
    expect(savable({ ...draftOf(mod), permissions: ["SEND_IN_THREADS", "MANAGE_ROLES"] }, mod)).toBe(false);
    expect(savable({ ...draftOf(mod), permissions: ["MANAGE_ROLES"] }, mod)).toBe(true);
  });
  test("of Everyone only the permissions count", () => {
    const everyone = role("everyone", { name: "Everyone", isEveryone: true, permissions: ["VIEW_CHANNEL"] });
    expect(savable(draftOf(everyone), everyone)).toBe(false);
    expect(savable({ ...draftOf(everyone), permissions: [] }, everyone)).toBe(true);
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
    expect(deleteQuestion(0)[0]).toBe("Nobody holds this role.");
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
  test("the owner and a guest are marked", () => {
    expect(memberMark({ owner: true, guest: false })).toBe("Owner");
    expect(memberMark({ owner: false, guest: true })).toBe("Guest");
    expect(memberMark({ owner: false, guest: false })).toBeUndefined();
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
    expect(roleRefusal({ code: "INVALID_ARGUMENT", details: { field: "name" } }, "Acme")).toContain("1 to 32 characters");
    expect(roleRefusal({ code: "INVALID_ARGUMENT", details: { field: "role_ids" } }, "Acme")).toContain("Try again");
    expect(roleRefusal({ code: "PERMISSION_DENIED" }, "Acme")).toContain("isn’t yours to change");
    expect(roleRefusal(new Error("transport: offline"), "Acme")).toBe("transport: offline");
  });
  test("a member's", () => {
    expect(memberRefusal({ code: "NOT_FOUND" })).toBe("They are no longer a member.");
    expect(memberRefusal({ code: "INVALID_ARGUMENT", details: { field: "title" } })).toContain("32 characters");
    expect(memberRefusal("nope")).toBe("nope");
  });
});
