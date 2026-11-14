import { describe, expect, test } from "bun:test";
import type { MemberSummary, SnapshotSummary } from "./bridge";
import { type ProfileUpdate, membersWithProfile, peopleWithProfile, snapshotWithProfile, withProfile } from "./profiles";

const PICTURE = "/api/v1/avatars/" + "a".repeat(64);
const NEW_PICTURE = "/api/v1/avatars/" + "b".repeat(64);

function member(accountId: string, over: Partial<MemberSummary> = {}): MemberSummary {
  return { accountId, handle: accountId, displayName: "", nickname: "", pronouns: "", bio: "", avatarUrl: "", channelAddsByInvitation: false, serverDmsOff: false, ...over };
}

const ada = member("ada", { displayName: "Ada", nickname: "Countess", avatarUrl: PICTURE, pronouns: "she/her", channelAddsByInvitation: true, serverDmsOff: true });
const grace = member("grace", { displayName: "Grace" });

const renamed: ProfileUpdate = { kind: "profileUpdated", accountId: "ada", handle: "ada", displayName: "Ada Lovelace", avatarUrl: NEW_PICTURE, pronouns: "she/her", bio: "Notes on the engine" };

describe("a changed profile", () => {
  test("gives the member the new name and picture, and keeps the server's nickname and switches", () => {
    const now = withProfile(ada, renamed);
    expect(now.displayName).toBe("Ada Lovelace");
    expect(now.avatarUrl).toBe(NEW_PICTURE);
    expect(now.bio).toBe("Notes on the engine");
    expect(now.nickname).toBe("Countess");
    expect(now.channelAddsByInvitation).toBe(true);
    expect(now.serverDmsOff).toBe(true);
    expect(ada.displayName).toBe("Ada");
  });

  test("a cleared picture is no picture", () => {
    expect(withProfile(ada, { ...renamed, avatarUrl: "" }).avatarUrl).toBe("");
  });

  test("leaves everyone else as they were, the same objects", () => {
    expect(withProfile(grace, renamed)).toBe(grace);
    const list = membersWithProfile([ada, grace], renamed);
    expect(list?.[0]?.displayName).toBe("Ada Lovelace");
    expect(list?.[1]).toBe(grace);
  });

  test("hands back a list or people without the person unchanged", () => {
    const list = [grace];
    expect(membersWithProfile(list, renamed)).toBe(list);
    expect(membersWithProfile(undefined, renamed)).toBeUndefined();
    const people = new Map([["grace", grace]]);
    expect(peopleWithProfile(people, renamed)).toBe(people);
    expect(peopleWithProfile(undefined, renamed)).toBeUndefined();
  });

  test("writes the person into the people held, as a new map", () => {
    const people = new Map([
      ["ada", ada],
      ["grace", grace],
    ]);
    const now = peopleWithProfile(people, renamed);
    expect(now).not.toBe(people);
    expect(now?.get("ada")?.displayName).toBe("Ada Lovelace");
    expect(now?.get("ada")?.nickname).toBe("Countess");
    expect(now?.get("grace")).toBe(grace);
    expect(people.get("ada")).toBe(ada);
  });

  test("is this account's own name and picture only when it is this account", () => {
    const snapshot = { accountId: "ada", handle: "ada", displayName: "Ada", avatarUrl: PICTURE, servers: [] } as unknown as SnapshotSummary;
    const now = snapshotWithProfile(snapshot, renamed);
    expect(now?.displayName).toBe("Ada Lovelace");
    expect(now?.avatarUrl).toBe(NEW_PICTURE);
    expect(now?.servers).toBe(snapshot.servers);
    expect(snapshotWithProfile(snapshot, { ...renamed, accountId: "grace" })).toBe(snapshot);
    expect(snapshotWithProfile(undefined, renamed)).toBeUndefined();
  });
});
