// What is typed in a conversation's composer and not sent (issue #23,
// architecture 13, "Local-first"): each conversation keeps its own, the
// words and the reply they answer, in the core's store in this browser
// (`draft`, `setDraft`), so another conversation, a reload or a crash loses
// none of it. This is the page's side: when a draft is read and when it is
// kept. `drafts.test.ts` tests the keeper.
//
// A draft is read when its conversation comes under the composer. It is
// kept once what the composer holds has rested, and at once when the
// conversation leaves and when the page is hidden or goes. A send keeps
// nothing, which clears what was kept, and so does an emptied field. The
// files waiting over the composer are not kept, and nothing here is sent
// to the instance.
//
// The store answers later, and may not answer. Until it has, the field is
// empty because nothing was put in it, which is no change to keep; words
// typed meanwhile are the conversation's, kept like any others, and an
// answer that comes after they were kept or sent is of a draft they took
// the place of. A read that fails leaves what is kept alone. Words whose
// send failed after their conversation left the composer go back to that
// conversation's draft (`unsent`), never into the field on screen.
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

const NOTHING: Held = { body: "", reply: null };
/** What the store makes of a composer's words: blank ones keep nothing, the reply neither. */
const kept = (held: Held): string => (held.body.trim() ? JSON.stringify([held.body, held.reply]) : "");

/**
 * Keeps what composers hold. `changed` is told of every change and keeps it
 * once it has rested for `ms`; `flush` keeps what waits now; `sent` forgets
 * what waits for a conversation and keeps nothing of it. A write that
 * would change nothing in the store is not made.
 */
export class DraftKeeper {
  private waiting: { id: string; held: Held } | null = null;
  private timer: unknown;
  /** What the store holds of each conversation, as last read or kept. */
  private stored = new Map<string, string>();
  /** The read of each conversation's kept draft that has not answered. */
  private reading = new Map<string, object>();

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
    this.stored.set(id, held ? kept(held) : "");
    return held;
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

  /** Conversation `id`'s words were sent: nothing of them is kept, now and not once the field has rested. */
  sent(id: string) {
    if (this.waiting?.id === id) {
      this.stop();
      this.waiting = null;
    }
    // A kept draft still being read was never in the field: what was sent took its place.
    this.reading.delete(id);
    this.write(id, NOTHING);
  }

  /**
   * Conversation `id`'s words did not go after all, and it is no longer
   * under the composer: they are its draft again, unless words typed
   * there since are.
   */
  unsent(id: string, held: Held) {
    if (this.stored.get(id)) return;
    this.write(id, held);
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
    this.stored.set(id, next);
    // A store that refuses costs the draft and nothing else.
    void this.store.keep(id, held.body, held.reply).catch(() => this.stored.delete(id));
  }
}

/**
 * Gives a composer its conversation's draft and keeps what is typed in it.
 * `held` is what the composer holds; `restore` puts a kept draft in it and
 * `empty` clears it for another conversation. Answers `sent`, to call when
 * the conversation's words go, and `unsent`, to call when they did not go
 * after all: true when it kept them as the draft of their conversation,
 * which has left the composer, and false when that conversation is still
 * under it, where the field takes them back.
 */
export function keepDrafts(opts: { store: DraftStore; conversation: Accessor<string | undefined>; held: Accessor<Held>; restore: (held: Held) => void; empty: () => void }): {
  sent: (conversationId: string) => void;
  unsent: (conversationId: string, held: Held) => boolean;
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
  return {
    sent: (id) => keeper.sent(id),
    unsent: (id, held) => {
      if (!gone && under() === id && opts.conversation() === id) return false;
      keeper.unsent(id, held);
      return true;
    },
  };
}
