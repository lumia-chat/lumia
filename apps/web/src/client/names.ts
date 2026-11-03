// Names for account ids: this account, then everyone in a shared server
// (one query over the snapshot's servers), then a public profile fetched
// once; a short id when nothing is known yet. Shared by the server
// channel view and Home, so a sender is named the same everywhere.
import { createMemo } from "solid-js";
import type { MemberSummary, SnapshotSummary } from "./bridge";
import { peopleQuery, profileOf } from "./state";

/** The display name, else the nickname, else the handle; empty when there is no profile. */
function nameOfProfile(p: MemberSummary | null | undefined): string {
  return p?.displayName || p?.nickname || p?.handle || "";
}

export function useNames(snapshot: () => SnapshotSummary | undefined) {
  const serverIds = createMemo(() => (snapshot()?.servers ?? []).map((s) => s.serverId));
  const people = peopleQuery(serverIds);
  const profile = (accountId: string): MemberSummary | null | undefined => people.data?.get(accountId) ?? profileOf(accountId);
  const nameOf = (accountId: string): string => {
    const me = snapshot();
    if (accountId && accountId === me?.accountId) return me?.displayName || me?.handle || "You";
    return nameOfProfile(profile(accountId)) || (accountId ? accountId.slice(0, 8) : "Someone");
  };
  return { people, profile, nameOf };
}
