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

  constructor(
    private store: DraftStore,
    private timers: { set: (f: () => void, ms: number) => unknown; clear: (handle: unknown) => void } = { set: (f, ms) => setTimeout(f, ms), clear: (h) => clearTimeout(h as ReturnType<typeof setTimeout>) },
    private ms = KEEP_AFTER_MS,
  ) {}

  /** A conversation's kept draft; none when nothing is kept or the store cannot be read. */
  async read(id: string): Promise<Held | null> {
    try {
      const held = await this.store.read(id);
      this.stored.set(id, held ? kept(held) : "");
      return held;
    } catch {
      return null;
    }
  }

  /** The composer of conversation `id` holds `held` now. */
  changed(id: string, held: Held) {
    // Another conversation's words still waiting are kept before these take their place.
    if (this.waiting && this.waiting.id !== id) this.flush();
    this.stop();
    if (this.stored.get(id) === kept(held)) {
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
    this.write(id, NOTHING);
  }

  private stop() {
    if (this.timer !== undefined) this.timers.clear(this.timer);
    this.timer = undefined;
  }

  private write(id: string, held: Held) {
    const next = kept(held);
    if (this.stored.get(id) === next) return;
    this.stored.set(id, next);
    // A store that refuses costs the draft and nothing else.
    void this.store.keep(id, held.body, held.reply).catch(() => this.stored.delete(id));
  }
}

/**
 * Gives a composer its conversation's draft and keeps what is typed in it.
 * `held` is what the composer holds; `restore` puts a kept draft in it and
 * `empty` clears it for another conversation. Answers `sent`, to call when
 * the conversation's words go.
 */
export function keepDrafts(opts: { store: DraftStore; conversation: Accessor<string | undefined>; held: Accessor<Held>; restore: (held: Held) => void; empty: () => void }): { sent: (conversationId: string) => void } {
  const keeper = new DraftKeeper(opts.store);
  // The conversation whose kept draft has been read: until then nothing typed is kept over it.
  const [read, setRead] = createSignal<string>();
  createEffect(
    on(opts.conversation, (id) => {
      // The one that left keeps what it held, and the field is the next one's.
      keeper.flush();
      setRead(undefined);
      opts.empty();
      if (!id) return;
      void keeper.read(id).then((held) => {
        if (opts.conversation() !== id) return;
        // Words typed before the answer came stay: they are this conversation's now.
        if (held && !opts.held().body) opts.restore(held);
        setRead(id);
      });
    }),
  );
  createEffect(() => {
    const id = opts.conversation();
    const held = opts.held();
    if (id && read() === id) keeper.changed(id, held);
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
  });
  return { sent: (id) => keeper.sent(id) };
}
