import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { createRoot, createSignal } from "solid-js";
import type { QuoteSummary } from "./bridge";
import { DraftKeeper, keepDrafts, keptElsewhere, type DraftStore, type Held } from "./drafts";

/** Timers run by hand. */
function fakeTimers() {
  let now = 0;
  let next = 1;
  const due = new Map<number, { at: number; f: () => void }>();
  return {
    set: (f: () => void, ms: number) => {
      const id = next++;
      due.set(id, { at: now + ms, f });
      return id;
    },
    clear: (h: unknown) => void due.delete(h as number),
    advance(ms: number) {
      now += ms;
      for (const [id, t] of [...due].sort((a, b) => a[1].at - b[1].at)) {
        if (t.at > now) continue;
        due.delete(id);
        t.f();
      }
    },
  };
}

/** A store that keeps as the core does (blank words keep nothing) and lists every write. */
function world() {
  const rows = new Map<string, Held>();
  const writes: string[] = [];
  const store: DraftStore = {
    read: async (id) => rows.get(id) ?? null,
    keep: async (id, body, reply) => {
      writes.push(`${id}:${body}`);
      if (body.trim()) rows.set(id, { body, reply });
      else rows.delete(id);
    },
  };
  const timers = fakeTimers();
  return { rows, writes, timers, keeper: new DraftKeeper(store, timers, 400) };
}

const words = (body: string): Held => ({ body, reply: null });
const quote = { messageId: "m1", authorAccountId: "grace", text: "Shipped.", originalDeleted: false, silent: false };

describe("drafts", () => {
  test("a run of keys is one write, once the words rest", () => {
    const w = world();
    w.keeper.changed("a", words("H"));
    w.timers.advance(300);
    w.keeper.changed("a", words("Ha"));
    w.timers.advance(300);
    expect(w.writes).toEqual([]);
    w.timers.advance(100);
    expect(w.writes).toEqual(["a:Ha"]);
    // Nothing more waits.
    w.timers.advance(1000);
    expect(w.writes).toEqual(["a:Ha"]);
  });

  test("a conversation leaving keeps what it held at once, and each keeps its own", async () => {
    const w = world();
    w.keeper.changed("a", { body: "Half a thought", reply: quote });
    w.keeper.flush();
    expect(w.rows.get("a")).toEqual({ body: "Half a thought", reply: quote });
    w.keeper.changed("b", words("Elsewhere"));
    // Another conversation's change does not wait behind the first's rest.
    w.keeper.changed("a", { body: "Half a thought.", reply: quote });
    expect(w.writes).toEqual(["a:Half a thought", "b:Elsewhere"]);
    w.timers.advance(400);
    expect(await w.keeper.read("a")).toEqual({ body: "Half a thought.", reply: quote });
    expect(await w.keeper.read("b")).toEqual(words("Elsewhere"));
  });

  test("a send keeps nothing, and a save that was waiting does not bring the words back", async () => {
    const w = world();
    w.keeper.changed("a", words("About to go"));
    w.timers.advance(400);
    w.keeper.changed("a", words("About to go, now"));
    w.keeper.sent("a");
    w.timers.advance(1000);
    expect(w.writes).toEqual(["a:About to go", "a:"]);
    expect(await w.keeper.read("a")).toBeNull();
    // The field emptied after it changes nothing more.
    w.keeper.changed("a", words(""));
    w.timers.advance(400);
    expect(w.writes.length).toBe(2);
  });

  test("an emptied field clears what was kept, and what is kept already is not written again", async () => {
    const w = world();
    w.rows.set("a", words("From before"));
    expect(await w.keeper.read("a")).toEqual(words("From before"));
    // Read into the field: the same words are no change.
    w.keeper.changed("a", words("From before"));
    w.timers.advance(400);
    expect(w.writes).toEqual([]);
    w.keeper.changed("a", words("  "));
    w.timers.advance(400);
    expect(w.writes).toEqual(["a:  "]);
    expect(w.rows.has("a")).toBe(false);
  });

  test("a store that cannot be read or written costs the draft and nothing else", async () => {
    const timers = fakeTimers();
    const keeper = new DraftKeeper({ read: () => Promise.reject(new Error("no store")), keep: () => Promise.reject(new Error("no store")) }, timers, 400);
    expect(await keeper.read("a")).toBeNull();
    keeper.changed("a", words("Typed anyway"));
    timers.advance(400);
    keeper.flush();
  });

  test("words that did not go after their conversation left are its draft again, unless words typed there since are", async () => {
    const w = world();
    w.keeper.changed("a", words("Did not go"));
    w.keeper.sent("a");
    w.keeper.unsent("a", { body: "Did not go", reply: quote });
    expect(w.rows.get("a")).toEqual({ body: "Did not go", reply: quote });
    // Sent again, then something else typed there and kept: the words that did not go do not take its place.
    w.keeper.sent("a");
    w.keeper.changed("a", words("Typed since"));
    w.keeper.flush();
    w.keeper.unsent("a", words("Did not go"));
    expect(w.rows.get("a")).toEqual(words("Typed since"));
  });

  test("a send that failed elsewhere says why, and where the message is", () => {
    expect(keptElsewhere("Not sent: a file could not be attached.", "#general")).toBe("Not sent: a file could not be attached. Your message is kept in #general.");
    expect(keptElsewhere("transport: offline", "your conversation with Ada")).toBe("transport: offline. Your message is kept in your conversation with Ada.");
  });
});

/** A store that answers a read when the test says, with what it held when asked, or refuses it; and lists every write. */
function slowStore(held: Record<string, Held> = {}) {
  const rows = new Map(Object.entries(held));
  const writes: [string, string][] = [];
  const reads: { id: string; answer: () => void; refuse: () => void }[] = [];
  const store: DraftStore = {
    read: (id) => {
      const was = rows.get(id) ?? null;
      return new Promise((resolve, reject) => reads.push({ id, answer: () => resolve(was), refuse: () => reject(new Error("the store is busy")) }));
    },
    keep: async (id, body, reply) => {
      writes.push([id, body]);
      if (body.trim()) rows.set(id, { body, reply });
      else rows.delete(id);
    },
  };
  /** Answers the read of `id` that has waited longest. */
  const answer = async (id: string, how: "answer" | "refuse" = "answer") => {
    const at = reads.findIndex((r) => r.id === id);
    const [read] = reads.splice(at, 1);
    read?.[how]();
    await settle();
  };
  return { rows, writes, store, answer };
}

/** Lets what a promise's answer set off run. */
const settle = () => new Promise<void>((done) => setTimeout(done, 0));

/** A composer over `store`, with conversation `first` under it. */
function composer(store: DraftStore, first?: string) {
  return createRoot((dispose) => {
    const [conversation, open] = createSignal(first);
    const [body, type] = createSignal("");
    const [reply, answering] = createSignal<QuoteSummary | null>(null);
    const empty = () => {
      type("");
      answering(null);
    };
    const drafts = keepDrafts({
      store,
      conversation,
      held: () => ({ body: body(), reply: reply() }),
      restore: (held) => {
        type(held.body);
        answering(held.reply);
      },
      empty,
    });
    /** As a send does: the field emptied, and nothing kept. */
    const send = (id: string) => {
      empty();
      drafts.sent(id);
    };
    return { open, body, type, reply, answering, send, drafts, dispose };
  });
}

describe("a composer's drafts", () => {
  // The keeper listens for the page being hidden or going; there is no page here.
  const page = globalThis as { document?: unknown; window?: unknown };
  const listens = { addEventListener: () => {}, removeEventListener: () => {} };
  beforeAll(() => {
    page.document = { ...listens, visibilityState: "visible" };
    page.window = listens;
  });
  afterAll(() => {
    delete page.document;
    delete page.window;
  });

  test("a kept draft is put in its conversation's field, and that is no change to keep", async () => {
    const s = slowStore({ a: { body: "From before", reply: quote } });
    const c = composer(s.store, "a");
    expect(c.body()).toBe("");
    await s.answer("a");
    expect(c.body()).toBe("From before");
    expect(c.reply()).toEqual(quote);
    c.dispose();
    expect(s.writes).toEqual([]);
  });

  test("a draft that could not be read stays kept: the empty field it leaves is no change", async () => {
    const s = slowStore({ c: words("Kept, and not read") });
    const c = composer(s.store, "c");
    await s.answer("c", "refuse");
    expect(c.body()).toBe("");
    // Leaving keeps what waits, as the rest would have.
    c.open("d");
    c.dispose();
    expect(s.writes).toEqual([]);
    expect(s.rows.get("c")).toEqual(words("Kept, and not read"));
  });

  test("words typed where the draft could not be read are kept over it", async () => {
    const s = slowStore({ c: words("Kept, and not read") });
    const c = composer(s.store, "c");
    await s.answer("c", "refuse");
    c.type("Typed anyway");
    c.dispose();
    expect(s.writes).toEqual([["c", "Typed anyway"]]);
  });

  test("a field still waiting for its kept draft is not an emptied one", async () => {
    const s = slowStore({ a: words("From before") });
    const c = composer(s.store, "a");
    // The store has not answered when the conversation leaves.
    c.open("b");
    await s.answer("a");
    expect(c.body()).toBe("");
    c.dispose();
    expect(s.writes).toEqual([]);
    expect(s.rows.get("a")).toEqual(words("From before"));
  });

  test("words typed before the kept draft is read are kept when the conversation leaves", async () => {
    const s = slowStore({ a: words("From before") });
    const c = composer(s.store, "a");
    c.type("Typed first");
    c.open("b");
    expect(s.writes).toEqual([["a", "Typed first"]]);
    // The answer that comes now is of what they took the place of, and b's field is b's.
    await s.answer("a");
    expect(c.body()).toBe("");
    c.open("a");
    await s.answer("a");
    expect(c.body()).toBe("Typed first");
    c.dispose();
  });

  test("words typed before the kept draft is read stay when it is, and a reply chosen by then stays over its words", async () => {
    const s = slowStore({ a: words("From before"), b: words("Half a thought") });
    const c = composer(s.store, "a");
    c.type("Typed first");
    await s.answer("a");
    expect(c.body()).toBe("Typed first");
    c.open("b");
    c.answering(quote);
    await s.answer("b");
    expect(c.body()).toBe("Half a thought");
    expect(c.reply()).toEqual(quote);
    c.dispose();
    expect(s.rows.get("a")).toEqual(words("Typed first"));
    expect(s.rows.get("b")).toEqual({ body: "Half a thought", reply: quote });
  });

  test("a send before the kept draft is read does not bring the old one back", async () => {
    const s = slowStore({ a: words("From before") });
    const c = composer(s.store, "a");
    c.type("Sent at once");
    c.send("a");
    await s.answer("a");
    expect(c.body()).toBe("");
    c.dispose();
    expect(s.rows.has("a")).toBe(false);
  });

  test("a send that fails goes back to its own conversation, not the one on screen", async () => {
    const s = slowStore();
    const c = composer(s.store, "a");
    await s.answer("a");
    c.answering(quote);
    c.type("For a");
    c.send("a");
    // Still under the composer: the field takes the words back, and nothing is written for it.
    expect(c.drafts.unsent("a", { body: "For a", reply: quote })).toBe(false);
    expect(s.rows.has("a")).toBe(false);
    // Sent again, and the conversation left while it waits.
    c.open("b");
    await s.answer("b");
    c.type("For b");
    expect(c.drafts.unsent("a", { body: "For a", reply: quote })).toBe(true);
    expect(c.body()).toBe("For b");
    expect(s.rows.get("a")).toEqual({ body: "For a", reply: quote });
    c.open("a");
    await s.answer("a");
    expect(c.body()).toBe("For a");
    expect(c.reply()).toEqual(quote);
    expect(s.rows.get("b")).toEqual(words("For b"));
    c.dispose();
  });

  test("a send that fails after its composer has gone is still its conversation's draft", async () => {
    const s = slowStore();
    const c = composer(s.store, "a");
    await s.answer("a");
    c.type("For a");
    c.send("a");
    c.dispose();
    expect(c.drafts.unsent("a", words("For a"))).toBe(true);
    expect(s.rows.get("a")).toEqual(words("For a"));
  });
});
