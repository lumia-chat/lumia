// What is typed in a conversation's composer and not sent (issue #23,
// architecture 13, "Local-first"): each conversation keeps its own, the
// words and the reply they answer, in the core's store in this browser
// (`draft`, `setDraft`), so another conversation, a reload or a crash loses
// none of it. This is the page's side: when a draft is read and when it is
// kept. `drafts.test.ts` tests the keeper.
//
// A draft is read when its conversation comes under the composer. It is
// kept once what the composer holds has rested, and at once when the
// conversation leaves and when the page is hidden or goes. An emptied
// field keeps nothing, which clears what was kept. The files waiting over
// the composer are not kept, and nothing here is sent to the instance.
//
// A send lets go of nothing until it has gone (`sending`): the words stay
// in the field and in the draft while the core's send is asked, which can
// wait on an upload, and only once that has answered without error are
// the draft cleared and the field emptied of what went (`afterSend`). One
// that throws leaves both as they are. If another conversation came under
// the composer meanwhile, the words were kept as their own conversation's
// draft when it left, so they are never put in the field on screen. One
// send goes at a time in a conversation, of what the field held at Return
// (`inTurn`).
//
// The store answers later, and may not answer. Until it has, the field is
// empty because nothing was put in it, which is no change to keep; words
// typed meanwhile are the conversation's, kept like any others, and an
// answer that comes after they were kept or sent is of a draft they took
// the place of. A read that fails leaves what is kept alone.
import { createEffect, createSignal, on, onCleanup, type Accessor } from "solid-js";
import type { QuoteSummary } from "./bridge";

/** How long the composer rests before what it holds is kept: a crash loses that much typing at most, and a run of keys is one write. */
export const KEEP_AFTER_MS = 400;

/** A composer's words and the reply they answer. */
export interface Held {
  body: string;
  reply: QuoteSummary | null;
}

/** Where drafts are kept: the core's store, through the bridge (`draftStore` in `state.ts`). */
export interface DraftStore {
  read: (conversationId: string) => Promise<Held | null>;
  keep: (conversationId: string, body: string, reply: QuoteSummary | null) => Promise<void>;
}

/**
 * What is said on screen when a send failed in a conversation that is no
 * longer the one there: why, and where the message waits (`where` is
 * "#general", or "your conversation with Ada").
 */
export function keptElsewhere(why: string, where: string): string {
  const said = why.trim();
  return `${/[.!?…]$/.test(said) ? said : said + "."} Your message is kept in ${where}.`;
}

/**
 * What a composer's field holds once the words it held as `sent` have
 * gone, `now` being what it holds as the send answers: nothing, or
 * whatever was typed after them while they were on their way. A field
 * changed in some other way meanwhile is left as it is. The same on every
 * shell.
 */
export function afterSend(sent: string, now: string): string {
  if (now === sent) return "";
  return now.startsWith(sent) ? now.slice(sent.length).trimStart() : now;
}

/**
 * One send at a time in each conversation, each of what the field held
 * when it was asked for (`typed`). The next waits for the one before it,
 * whatever came of that, and then sends its words less those of every send
 * that went meanwhile (`afterSend`): the words on their way stay in the
 * field, so Return pressed twice sends once (the second has nothing left),
 * words typed after them and sent while they wait go after them, in order,
 * and words put in the field in place of them go as they were when Return
 * was pressed, whatever the field holds by their turn. `send` answers
 * whether its words went. Another conversation's sends do not wait behind
 * this one's.
 */
export function inTurn(): (conversationId: string, typed: string, send: (words: string) => Promise<boolean>) => Promise<void> {
  const last = new Map<string, Promise<void>>();
  /** The sends asked for in each conversation whose turn has not come, each with its words as they now stand. */
  const asked = new Map<string, Set<{ words: string }>>();
  return (id, typed, send) => {
    const mine = { words: typed };
    const waiting = asked.get(id) ?? new Set();
    asked.set(id, waiting);
    waiting.add(mine);
    const go = async () => {
      waiting.delete(mine);
      if (!waiting.size && asked.get(id) === waiting) asked.delete(id);
      const words = mine.words;
      if ((await send(words)) && words.trim()) for (const next of asked.get(id) ?? []) next.words = afterSend(words, next.words);
    };
    const turn = (last.get(id) ?? Promise.resolve()).then(go, go);
    last.set(id, turn);
    const done = () => {
      if (last.get(id) === turn) last.delete(id);
    };
    turn.then(done, done);
    return turn;
  };
}

const NOTHING: Held = { body: "", reply: null };
/** What the store makes of a composer's words: blank ones keep nothing, the reply neither. */
const kept = (held: Held): string => (held.body.trim() ? JSON.stringify([held.body, held.reply]) : "");

/**
 * Keeps what composers hold. `changed` is told of every change and keeps it
 * once it has rested for `ms`; `flush` keeps what waits now; `sent` lets
 * go of the words that went, in what waits and in the store, and `went`
 * does so for a conversation that has left the composer. A write that
 * would change nothing in the store is not made.
 */
export class DraftKeeper {
  private waiting: { id: string; held: Held } | null = null;
  private timer: unknown;
  /** What the store holds of each conversation, as last read or kept. */
  private stored = new Map<string, string>();
  /** The same as the words and the reply they were, for the conversations that hold some. */
  private drafts = new Map<string, Held>();
  /** The read of each conversation's kept draft that has not answered. */
  private reading = new Map<string, object>();
  /** Each conversation's writes to the store that have not all answered, as one promise. */
  private writing = new Map<string, Promise<void>>();

  constructor(
    private store: DraftStore,
    private timers: { set: (f: () => void, ms: number) => unknown; clear: (handle: unknown) => void } = { set: (f, ms) => setTimeout(f, ms), clear: (h) => clearTimeout(h as ReturnType<typeof setTimeout>) },
    private ms = KEEP_AFTER_MS,
  ) {}

  /**
   * A conversation's kept draft, to put in its empty field. None when
   * nothing is kept, when the store cannot be read, and when words were
   * kept or sent there while it was being read: those took its place.
   */
  async read(id: string): Promise<Held | null> {
    const asked = {};
    this.reading.set(id, asked);
    let held: Held | null = null;
    try {
      held = await this.store.read(id);
    } catch {
      // What it holds is not known and none of it is in the field. An empty field is then no change to keep, as
      // over a store that holds nothing, so a kept draft stays for the next read; words typed are kept over it.
    }
    if (this.reading.get(id) !== asked) return null;
    this.reading.delete(id);
    this.holds(id, held ?? NOTHING);
    return held;
  }

  /** The store holds `held` of conversation `id`, as far as is known. */
  private holds(id: string, held: Held) {
    const next = kept(held);
    this.stored.set(id, next);
    if (next) this.drafts.set(id, held);
    else this.drafts.delete(id);
  }

  /** The composer of conversation `id` holds `held` now. */
  changed(id: string, held: Held) {
    // Another conversation's words still waiting are kept before these take their place.
    if (this.waiting && this.waiting.id !== id) this.flush();
    this.stop();
    const next = kept(held);
    // A field with no words in it while its kept draft is still being read was not emptied: the draft has yet to be put in it.
    if (this.stored.get(id) === next || (!next && this.reading.has(id))) {
      this.waiting = null;
      return;
    }
    this.waiting = { id, held };
    this.timer = this.timers.set(() => this.flush(), this.ms);
  }

  /** Keeps what waits, now: the conversation is leaving, or the page is. */
  flush() {
    this.stop();
    const w = this.waiting;
    this.waiting = null;
    if (w) this.write(w.id, w.held);
  }

  /**
   * Conversation `id`'s words `sent` have gone, and it is the one under the
   * composer: nothing is kept of them, now or once the field has rested,
   * so a save that was waiting cannot bring them back after the core let
   * them go. What was typed after them, or in their place, while they were
   * on their way is the field's still (`afterSend`), and is kept: a save of
   * it that waits goes on waiting, less the words that went, and one made
   * already stays.
   */
  sent(id: string, sent: string) {
    const w = this.waiting;
    if (w?.id === id) {
      const left = afterSend(sent, w.held.body);
      if (!left.trim()) {
        this.stop();
        this.waiting = null;
      } else if (left !== w.held.body) this.waiting = { id, held: { body: left, reply: null } };
    }
    // A kept draft still being read was never in the field: what was sent took its place.
    this.reading.delete(id);
    const draft = this.drafts.get(id);
    if (!draft) return;
    // What the store holds was kept before the send went or while it was on its way: words the sent ones begin
    // with are an older state of them, and go; words typed after them stay, and so do words put in their place.
    const left = sent.trim().startsWith(draft.body.trim()) ? "" : afterSend(sent, draft.body);
    // The reply went with the message.
    if (left !== draft.body) this.write(id, { body: left, reply: null });
  }

  /**
   * Conversation `id`'s words `sent` have gone after it left the composer,
   * where what its field held was kept as its draft: the draft lets go of
   * what went and only of that (`afterSend`), so words typed after them
   * before it left are still its draft, and a draft of other words is left
   * alone.
   */
  went(id: string, sent: string) {
    const draft = this.drafts.get(id);
    if (!draft) return;
    const left = afterSend(sent, draft.body);
    // The reply went with the message.
    if (left !== draft.body) this.write(id, { body: left, reply: null });
  }

  private stop() {
    if (this.timer !== undefined) this.timers.clear(this.timer);
    this.timer = undefined;
  }

  private write(id: string, held: Held) {
    const next = kept(held);
    if (this.stored.get(id) === next) return;
    // A read still out answers with what these words take the place of.
    this.reading.delete(id);
    this.holds(id, held);
    // A store that refuses costs the draft and nothing else.
    const done = this.store.keep(id, held.body, held.reply).then(
      () => {},
      () => {
        this.stored.delete(id);
        this.drafts.delete(id);
      },
    );
    // Answered once this write and every one before it has: the worker answers calls as each finishes.
    const all = Promise.all([this.writing.get(id), done]).then(() => {});
    this.writing.set(id, all);
    void all.then(() => {
      if (this.writing.get(id) === all) this.writing.delete(id);
    });
  }

  /**
   * Once the writes asked of conversation `id` have answered, whatever came
   * of them. A send waits for this before it is asked: the core clears the
   * draft in the same write that puts the message in the outbox, and a keep
   * still on its way could land after that and put the sent words back.
   */
  async settled(id: string): Promise<void> {
    for (let w = this.writing.get(id); w; w = this.writing.get(id)) await w;
  }
}

/**
 * Gives a composer its conversation's draft and keeps what is typed in it.
 * `held` is what the composer holds; `restore` puts a kept draft in it and
 * `empty` clears it for another conversation. Answers two things a send
 * uses. `sending` wraps the core's send of the words `sent` typed in a
 * conversation: what waits is kept first, so the store has the words while
 * they are on their way; then, once the store has answered every keep of
 * that conversation (`settled`), the send is asked; and only once it has
 * answered without error does the draft let go of what went, and of that
 * only (`sent` where the conversation is still under the composer, the
 * caller then emptying the field of it with `afterSend`, and `went` where
 * it has left). A send that
 * throws leaves the draft as it is, and throws on. `here` says whether a
 * conversation is the one under the composer, its words in the field.
 */
export function keepDrafts(opts: { store: DraftStore; conversation: Accessor<string | undefined>; held: Accessor<Held>; restore: (held: Held) => void; empty: () => void }): {
  here: (conversationId: string) => boolean;
  sending: <T>(conversationId: string, sent: string, send: () => Promise<T>) => Promise<T>;
} {
  const keeper = new DraftKeeper(opts.store);
  // The conversation the field's words belong to: the one under the composer, from when the field was emptied for
  // it. Until then the field still holds the words of the one that left, which are not this one's to keep.
  const [under, setUnder] = createSignal<string>();
  let gone = false;
  createEffect(
    on(opts.conversation, (id) => {
      // The one that left keeps what it held, and the field is the next one's.
      keeper.flush();
      opts.empty();
      setUnder(id);
      if (!id) return;
      void keeper.read(id).then((held) => {
        if (!held || gone || opts.conversation() !== id) return;
        // Words typed before the answer came stay: they are this conversation's now. So does a reply chosen by then.
        const now = opts.held();
        if (!now.body.trim()) opts.restore(now.reply ? { body: held.body, reply: now.reply } : held);
      });
    }),
  );
  createEffect(() => {
    const id = opts.conversation();
    const held = opts.held();
    if (id && under() === id) keeper.changed(id, held);
  });
  // A hidden page may never come back: a phone's browser, a closed tab.
  const hidden = () => {
    if (document.visibilityState === "hidden") keeper.flush();
  };
  const going = () => keeper.flush();
  document.addEventListener("visibilitychange", hidden);
  window.addEventListener("pagehide", going);
  onCleanup(() => {
    document.removeEventListener("visibilitychange", hidden);
    window.removeEventListener("pagehide", going);
    keeper.flush();
    gone = true;
  });
  const here = (id: string) => !gone && under() === id && opts.conversation() === id;
  return {
    here,
    sending: async (id, sent, send) => {
      // What waits is kept now, and the send is asked once the store has answered every keep of this conversation:
      // one landing after the core's send cleared the draft would put the sent words back.
      keeper.flush();
      await keeper.settled(id);
      const answer = await send();
      // Not cut short by the composer going away: a draft left behind for words that went would send them twice.
      if (here(id)) keeper.sent(id, sent);
      else keeper.went(id, sent);
      return answer;
    },
  };
}
