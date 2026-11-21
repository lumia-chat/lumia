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

/** An applied event as far as these rules read it (`EventSummary`). */
export interface Touch {
  seq: number;
  kind: string;
  messageId: string;
  beforeSeq: number;
}

/**
 * What of a channel's messages held outside the store is to be read again
 * from the instance: these messages by id, every one below `below` (0 for
 * none), or all of them (`whole`: the events were skipped, or an emoji
 * named in them was removed), with the removed emoji whose names that read
 * settles (`namesLeft`).
 */
export interface Ask {
  ids: Set<string>;
  below: number;
  whole: boolean;
  names: Set<string>;
}

export function ask(over: Partial<Ask> = {}): Ask {
  return { ids: new Set(), below: 0, whole: false, names: new Set(), ...over };
}

/** Two asks as one. */
export function joined(held: Ask | undefined, more: Ask): Ask {
  if (!held) return more;
  return { ids: new Set([...held.ids, ...more.ids]), below: Math.max(held.below, more.below), whole: held.whole || more.whole, names: new Set([...held.names, ...more.names]) };
}

/** Whether an ask asks for nothing. */
export function nothing(a: Ask): boolean {
  return !a.whole && !a.below && !a.ids.size;
}

/**
 * Which of `held` a batch of a channel's events changed, as the core names
 * them: each event about a message (an edit, cards, a delete, a reaction, a
 * pin, a thread) whose message is held; a redaction whose seq is a held
 * message's (the event that made it, its payload gone); and, for an
 * attachment expiring or a purge, every held message below its seq. A new
 * message is never one held.
 */
export function touched(events: readonly Touch[], held: readonly MessageSummary[]): Ask {
  const out = ask();
  if (!held.length) return out;
  const ids = new Set(held.map((m) => m.messageId));
  // Not `Math.min(...)`: a long scrollback is more arguments than an engine takes.
  const oldest = held.reduce((low, m) => Math.min(low, m.seq), Infinity);
  for (const e of events) {
    if (e.kind === "message_created") continue;
    if (e.messageId && ids.has(e.messageId)) out.ids.add(e.messageId);
    if (e.kind === "redacted") {
      const m = held.find((h) => h.seq === e.seq);
      if (m) out.ids.add(m.messageId);
    }
    if (e.beforeSeq > oldest) out.below = Math.max(out.below, e.beforeSeq);
  }
  return out;
}

/** One message as the instance has it now, read by its seq: null where it is gone (deleted, where the server shows none). */
export async function oneAgain(id: string, seq: number, read: ReadOlder): Promise<MessageSummary | null> {
  const got = await read(seq + 1, 1);
  return got.messages.find((m) => m.messageId === id) ?? null;
}

/** `list` with the message `id` as it now is, or without it where it is gone; the same list where it holds none. */
export function withMessage<M extends { messageId: string }>(list: M[], id: string, now: M | null): M[] {
  if (!list.some((m) => m.messageId === id)) return list;
  return now ? list.map((m) => (m.messageId === id ? now : m)) : list.filter((m) => m.messageId !== id);
}

/**
 * The history held after an ask: all of it read again (`whole`), or the
 * span below `below` read again (the newer held kept as they were), then
 * each message asked for by id read again on its own, unless that span
 * read it. Nothing asked, nothing read.
 */
export async function followed(kept: History, a: Ask, read: ReadOlder, page: number): Promise<History> {
  if (a.whole) return historyAgain(kept, read, page);
  let h = kept;
  if (a.below > 0) {
    const under = h.messages.filter((m) => m.seq < a.below);
    const top = under[under.length - 1]?.seq;
    if (top !== undefined) {
      const again = await historyAgain({ messages: under, more: h.more }, read, page);
      h = { messages: [...again.messages, ...h.messages.filter((m) => m.seq > top)], more: again.more };
    }
  }
  for (const id of a.ids) {
    const m = h.messages.find((x) => x.messageId === id);
    if (!m || m.seq < a.below) continue;
    h = { ...h, messages: withMessage(h.messages, id, await oneAgain(id, m.seq, read)) };
  }
  return h;
}

/**
 * Reads of what is held again from the instance, one at a time for each
 * key (a channel), the asks made while one is on its way taken together by
 * one more after it. `ask` adds to what a key's next read is to read;
 * `run` starts the read where one is asked and none is on its way, and
 * `ready` says whether it may start now (the channel is on screen); one
 * asked while it is not starts when `run` is called for it again. A read
 * that fails is not asked again until the next ask.
 */
export class Rereads {
  private asked = new Map<string, Ask>();
  private running = new Set<string>();

  constructor(
    private read: (key: string, a: Ask) => Promise<void>,
    private ready: (key: string) => boolean = () => true,
  ) {}

  ask(key: string, a: Ask): void {
    if (nothing(a)) return;
    this.asked.set(key, joined(this.asked.get(key), a));
    this.run(key);
  }

  run(key: string): void {
    const a = this.asked.get(key);
    if (!a || this.running.has(key) || !this.ready(key)) return;
    this.asked.delete(key);
    this.running.add(key);
    void this.read(key, a)
      .catch(() => {})
      .finally(() => {
        this.running.delete(key);
        this.run(key);
      });
  }

  /** Lets go of every ask not yet begun (signed out); a read on its way finds what it read for gone. */
  forget(): void {
    this.asked.clear();
  }
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
