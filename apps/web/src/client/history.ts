// What the instance gave of a channel's history from before the store
// begins, which the page keeps for its life and does not store (`history`
// in `state.ts`): the rules for reading it again that need no page. The
// store's messages the core keeps current; these are as the instance gave
// them when they were read, so what they say that can change underneath is
// read again from it.
import type { MessageSummary } from "./bridge";

/** A channel's history from the instance, oldest first, and whether older remains. */
export interface History {
  messages: MessageSummary[];
  more: boolean;
}

/** `olderMessages` for one channel: the newest `limit` below `beforeSeq`, oldest first. */
export type ReadOlder = (beforeSeq: number, limit: number) => Promise<{ messages: MessageSummary[]; hasMore: boolean }>;

/**
 * The history held, read again from the instance over the span it holds: a
 * page at a time from just above its newest message down past its oldest.
 * What the last page brings from below the oldest is kept too, with whether
 * more remains below that. Nothing held, nothing asked.
 */
export async function historyAgain(kept: History, read: ReadOlder, page: number): Promise<History> {
  const oldest = kept.messages[0]?.seq;
  const newest = kept.messages[kept.messages.length - 1]?.seq;
  if (oldest === undefined || newest === undefined) return kept;
  let found: MessageSummary[] = [];
  let before = newest + 1;
  let more = kept.more;
  for (;;) {
    const got = await read(before, page);
    found = [...got.messages, ...found];
    more = got.hasMore && got.messages.length > 0;
    const first = got.messages[0]?.seq;
    if (first === undefined || first <= oldest || !more) break;
    before = first;
  }
  return { messages: found, more };
}

/**
 * The server emoji whose reactions in `messages` are named by a name the
 * page cannot know: each one `live` (every server's emoji listed now) does
 * not hold, removed since (a held one is drawn by its present name), and not
 * already read again by the name it had when it went (`settled`), which
 * does not change after.
 */
export function namesLeft(messages: readonly MessageSummary[], live: ReadonlySet<string>, settled: ReadonlySet<string>): string[] {
  const left = new Set<string>();
  for (const m of messages) for (const r of m.reactions) if (r.emojiId && !live.has(r.emojiId) && !settled.has(r.emojiId)) left.add(r.emojiId);
  return [...left];
}
