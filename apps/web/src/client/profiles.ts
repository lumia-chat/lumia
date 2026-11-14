// A changed name or picture (`profileUpdated`, issue #15) written into
// what the page holds of the person: a server's member list, the people
// of every shared server, a public profile, and the snapshot where it is
// this account. Only the account's own fields change; a server's nickname
// and switches are that server's and stay. What does not hold the person
// is handed back as it was, so nothing else is drawn again.
import type { ClientEvent, MemberSummary, SnapshotSummary } from "./bridge";

export type ProfileUpdate = Extract<ClientEvent, { kind: "profileUpdated" }>;

/** The member as they now are; the same object when it is someone else. */
export function withProfile(member: MemberSummary, p: ProfileUpdate): MemberSummary {
  if (member.accountId !== p.accountId) return member;
  return { ...member, handle: p.handle, displayName: p.displayName, avatarUrl: p.avatarUrl, pronouns: p.pronouns, bio: p.bio };
}

/** A member list with the person written in; the same list when they are not in it. */
export function membersWithProfile(list: MemberSummary[] | undefined, p: ProfileUpdate): MemberSummary[] | undefined {
  if (!list?.some((m) => m.accountId === p.accountId)) return list;
  return list.map((m) => withProfile(m, p));
}

/** The people by account id, with the person written in; the same map when they are not in it. */
export function peopleWithProfile(people: Map<string, MemberSummary> | undefined, p: ProfileUpdate): Map<string, MemberSummary> | undefined {
  const member = people?.get(p.accountId);
  if (!people || !member) return people;
  return new Map(people).set(p.accountId, withProfile(member, p));
}

/** The snapshot with this account's name and picture as they now are; the same snapshot for anyone else. */
export function snapshotWithProfile(snapshot: SnapshotSummary | undefined, p: ProfileUpdate): SnapshotSummary | undefined {
  if (!snapshot || snapshot.accountId !== p.accountId) return snapshot;
  return { ...snapshot, handle: p.handle, displayName: p.displayName, avatarUrl: p.avatarUrl };
}

/**
 * The updates as they came, numbered, so a fetch begun before one (a member
 * list asked of the instance, say) can bring its answer up to date: it notes
 * `mark()` as it starts and passes what it got through `since`. The newest
 * `keep` are held, far more than can arrive while one fetch is out.
 */
export function profileLog(keep = 200) {
  let last = 0;
  const held: { n: number; p: ProfileUpdate }[] = [];
  return {
    add(p: ProfileUpdate): void {
      held.push({ n: ++last, p });
      if (held.length > keep) held.shift();
    },
    mark: (): number => last,
    since<T>(mark: number, value: T, apply: (value: T, p: ProfileUpdate) => T): T {
      for (const { n, p } of held) if (n > mark) value = apply(value, p);
      return value;
    },
  };
}
