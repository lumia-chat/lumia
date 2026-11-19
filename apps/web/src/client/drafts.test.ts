import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { createRoot, createSignal } from "solid-js";
import type { QuoteSummary } from "./bridge";
import { DraftKeeper, KEEP_AFTER_MS, afterSend, inTurn, keepDrafts, keptElsewhere, type DraftStore, type Held } from "./drafts";

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
    w.keeper.sent("a", "About to go, now");
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

  test("words that went after their conversation left leave its draft, and only they do", async () => {
    const w = world();
    // The draft is what went: nothing is kept of it.
    w.keeper.changed("a", { body: "Went", reply: quote });
    w.keeper.flush();
    w.keeper.went("a", "Went");
    expect(w.rows.has("a")).toBe(false);
    // More was typed after them before the conversation left: that stays, without the reply that went.
    w.keeper.changed("a", { body: "Went and more", reply: quote });
    w.keeper.flush();
    w.keeper.went("a", "Went");
    expect(w.rows.get("a")).toEqual(words("and more"));
    // A draft of other words is left alone, and so is a conversation that keeps none.
    w.keeper.changed("b", { body: "Something else", reply: quote });
    w.keeper.flush();
    w.keeper.went("b", "Went");
    w.keeper.went("c", "Went");
    expect(w.rows.get("b")).toEqual({ body: "Something else", reply: quote });
    expect(w.rows.has("c")).toBe(false);
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
    /** As the routes send what the field holds, `core` being the core's send: answers why it did not go, or nothing. */
    const send = async (core: () => Promise<void>): Promise<string> => {
      const id = conversation();
      if (!id) return "no conversation";
      const typed = body();
      const quoted = reply();
      try {
        await drafts.sending(id, typed, core);
        if (drafts.here(id)) {
          type((now) => afterSend(typed, now));
          answering((now) => (now === quoted ? null : now));
        }
        return "";
      } catch (e) {
        return e instanceof Error ? e.message : String(e);
      }
    };
    return { open, body, type, reply, answering, send, drafts, dispose };
  });
}

/** A core's send that answers, or throws, when the test says. */
function deferred() {
  let go = () => {};
  let fail = (_why: string) => {};
  const answer = new Promise<void>((resolve, reject) => {
    go = resolve;
    fail = (why) => reject(new Error(why));
  });
  return { answer, go, fail };
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
    expect(await c.send(async () => {})).toBe("");
    await s.answer("a");
    expect(c.body()).toBe("");
    c.dispose();
    expect(s.rows.has("a")).toBe(false);
  });

  test("a send lets go of nothing until it has gone: the words stay in the field and in the draft, then leave both", async () => {
    const s = slowStore();
    const c = composer(s.store, "a");
    await s.answer("a");
    c.answering(quote);
    c.type("For a");
    const core = deferred();
    const sent = c.send(() => core.answer);
    await settle();
    // On its way (an upload it waits for, say): kept at once, not once the field has rested, and still in the field.
    expect(s.rows.get("a")).toEqual({ body: "For a", reply: quote });
    expect(c.body()).toBe("For a");
    expect(c.reply()).toEqual(quote);
    core.go();
    expect(await sent).toBe("");
    expect(c.body()).toBe("");
    expect(c.reply()).toBeNull();
    expect(s.rows.has("a")).toBe(false);
    c.dispose();
    expect(s.rows.has("a")).toBe(false);
  });

  test("a send that throws leaves the words in the field and the draft kept", async () => {
    const s = slowStore();
    const c = composer(s.store, "a");
    await s.answer("a");
    c.answering(quote);
    c.type("For a");
    expect(await c.send(() => Promise.reject(new Error("not started")))).toBe("not started");
    expect(c.body()).toBe("For a");
    expect(c.reply()).toEqual(quote);
    expect(c.drafts.here("a")).toBe(true);
    c.dispose();
    expect(s.rows.get("a")).toEqual({ body: "For a", reply: quote });
  });

  test("words typed after the ones on their way stay when those have gone", async () => {
    const s = slowStore();
    const c = composer(s.store, "a");
    await s.answer("a");
    c.type("For a");
    const core = deferred();
    const sent = c.send(() => core.answer);
    c.type("For a and then some");
    core.go();
    await sent;
    expect(c.body()).toBe("and then some");
    c.dispose();
    expect(s.rows.get("a")).toEqual(words("and then some"));
  });

  test("a send that fails after its conversation left is that conversation's draft, and the field on screen is not touched", async () => {
    const s = slowStore();
    const c = composer(s.store, "a");
    await s.answer("a");
    c.answering(quote);
    c.type("For a");
    const core = deferred();
    const sent = c.send(() => core.answer);
    c.open("b");
    await s.answer("b");
    c.type("For b");
    expect(c.drafts.here("a")).toBe(false);
    core.fail("Not sent: a file could not be attached.");
    expect(await sent).toBe("Not sent: a file could not be attached.");
    expect(c.body()).toBe("For b");
    expect(c.reply()).toBeNull();
    expect(s.rows.get("a")).toEqual({ body: "For a", reply: quote });
    c.open("a");
    await s.answer("a");
    expect(c.body()).toBe("For a");
    expect(c.reply()).toEqual(quote);
    expect(s.rows.get("b")).toEqual(words("For b"));
    c.dispose();
  });

  test("a send that goes after its conversation left takes that conversation's draft, and what was typed after it stays", async () => {
    const s = slowStore();
    const c = composer(s.store, "a");
    await s.answer("a");
    c.type("For a");
    const first = deferred();
    const sent = c.send(() => first.answer);
    c.open("b");
    await s.answer("b");
    c.type("For b");
    expect(s.rows.get("a")).toEqual(words("For a"));
    first.go();
    await sent;
    expect(s.rows.has("a")).toBe(false);
    expect(c.body()).toBe("For b");
    // Again, with more typed after the words on their way before the conversation left.
    c.open("a");
    await s.answer("a");
    c.type("Again");
    const second = deferred();
    const again = c.send(() => second.answer);
    c.type("Again, and a thought");
    c.open("b");
    await s.answer("b");
    second.go();
    await again;
    expect(s.rows.get("a")).toEqual(words(", and a thought"));
    c.dispose();
  });

  test("a send that answers after its composer has gone still takes its draft, and one that throws leaves it", async () => {
    const s = slowStore();
    const c = composer(s.store, "a");
    await s.answer("a");
    c.type("For a");
    const core = deferred();
    const sent = c.send(() => core.answer);
    c.dispose();
    expect(s.rows.get("a")).toEqual(words("For a"));
    core.go();
    await sent;
    expect(s.rows.has("a")).toBe(false);

    const d = composer(s.store, "a");
    await s.answer("a");
    d.type("For a, again");
    const failing = deferred();
    const unsent = d.send(() => failing.answer);
    d.dispose();
    failing.fail("offline");
    await unsent;
    expect(s.rows.get("a")).toEqual(words("For a, again"));
  });

  test("words the core let go with their message are not put back by a save that was waiting", async () => {
    const s = slowStore();
    const c = composer(s.store, "a");
    await s.answer("a");
    // Typed and sent before the field has rested: a save of these words waits on its timer.
    c.type("Gone with the message");
    // As the core's send does: the message queued and its draft cleared in the one write.
    await c.send(async () => void s.rows.delete("a"));
    expect(c.body()).toBe("");
    await new Promise((done) => setTimeout(done, KEEP_AFTER_MS + 100));
    expect(s.rows.has("a")).toBe(false);
    expect(s.writes.filter(([, body]) => body === "Gone with the message")).toHaveLength(1);
    c.dispose();
    expect(s.rows.has("a")).toBe(false);
  });

  test("words put in the field in place of those on their way are kept when those have gone", async () => {
    const s = slowStore();
    const c = composer(s.store, "a");
    await s.answer("a");
    c.type("First");
    const core = deferred();
    const sent = c.send(() => core.answer);
    await settle();
    // Typed over before the send answers, and before the field has rested: a save of them waits.
    c.type("Second");
    core.go();
    expect(await sent).toBe("");
    expect(c.body()).toBe("Second");
    await new Promise((done) => setTimeout(done, KEEP_AFTER_MS + 100));
    expect(s.rows.get("a")).toEqual(words("Second"));
    c.dispose();
    expect(s.rows.get("a")).toEqual(words("Second"));
  });

  test("words put in the field in place of those on their way and kept already stay kept when those have gone", async () => {
    const s = slowStore();
    const c = composer(s.store, "a");
    await s.answer("a");
    c.type("First");
    const core = deferred();
    const sent = c.send(() => core.answer);
    await settle();
    // Typed over and rested while the send waits (an upload, say): kept before it answers.
    c.type("Second");
    await new Promise((done) => setTimeout(done, KEEP_AFTER_MS + 100));
    expect(s.rows.get("a")).toEqual(words("Second"));
    core.go();
    expect(await sent).toBe("");
    expect(c.body()).toBe("Second");
    expect(s.rows.get("a")).toEqual(words("Second"));
    c.dispose();
    expect(s.rows.get("a")).toEqual(words("Second"));
  });

  test("a send is asked only once the store has answered the keeps of its conversation, so none lands after the core's clear", async () => {
    // A store whose writes land when the test says, as a worker's calls answer when each finishes.
    const rows = new Map<string, Held>();
    const landing: (() => void)[] = [];
    const store: DraftStore = {
      read: async (id) => rows.get(id) ?? null,
      keep: (id, body, reply) =>
        new Promise<void>((done) =>
          landing.push(() => {
            if (body.trim()) rows.set(id, { body, reply });
            else rows.delete(id);
            done();
          }),
        ),
    };
    const c = composer(store, "a");
    await settle();
    // One keep already on its way once the field rested, and the next one the send makes of what waits.
    c.type("Gone");
    await new Promise((done) => setTimeout(done, KEEP_AFTER_MS + 50));
    c.type("Gone with the message");
    let asked = false;
    const sent = c.send(async () => {
      asked = true;
      // As the core's send does: the message queued and its draft cleared in the one write.
      rows.delete("a");
    });
    await settle();
    expect(landing).toHaveLength(2);
    expect(asked).toBe(false);
    landing.shift()?.();
    await settle();
    expect(asked).toBe(false);
    landing.shift()?.();
    expect(await sent).toBe("");
    expect(asked).toBe(true);
    expect(c.body()).toBe("");
    // Whatever the keeper writes after is a clear, never the words that went.
    for (const land of landing.splice(0)) land();
    await settle();
    expect(rows.has("a")).toBe(false);
    c.dispose();
    for (const land of landing.splice(0)) land();
    expect(rows.has("a")).toBe(false);
  });
});

describe("sending", () => {
  test("the field lets go of what went, and only of that", () => {
    expect(afterSend("Hello", "Hello")).toBe("");
    expect(afterSend("Hello", "Hello and more")).toBe("and more");
    expect(afterSend(" Hello ", " Hello next")).toBe("next");
    // Changed some other way meanwhile: left as it is.
    expect(afterSend("Hello", "Something else")).toBe("Something else");
    expect(afterSend("Hello", "")).toBe("");
    // Files alone were sent: whatever was typed meanwhile stays.
    expect(afterSend("", "A caption too late")).toBe("A caption too late");
  });

  test("one send at a time in a conversation, in order, whatever came of the one before", async () => {
    const turns = inTurn();
    const order: string[] = [];
    const first = deferred();
    const one = turns("a", "first", async () => {
      order.push("first begins");
      await first.answer;
      return true;
    });
    const two = turns("a", "second", async (words) => (order.push(words), true));
    // Another conversation's does not wait behind it.
    await turns("b", "elsewhere", async (words) => (order.push(words), true));
    expect(order).toEqual(["first begins", "elsewhere"]);
    first.fail("refused");
    await one.catch(() => {});
    await two;
    expect(order).toEqual(["first begins", "elsewhere", "second"]);
    // Nothing waits any more: the next goes at once.
    await turns("a", "third", async (words) => (order.push(words), true));
    expect(order).toEqual(["first begins", "elsewhere", "second", "third"]);
  });

  test("each send is of what the field held at Return, less what went before its turn", async () => {
    const turns = inTurn();
    const went: string[] = [];
    const slow = deferred();
    const send = async (words: string) => {
      if (!words.trim()) return false;
      if (!went.length) await slow.answer;
      went.push(words);
      return true;
    };
    // Return twice on the same words, words typed after them, and words put in the field in place of them.
    const all = Promise.all([turns("a", "Hello", send), turns("a", "Hello", send), turns("a", "Hello and more", send), turns("a", "Something else", send)]);
    slow.go();
    await all;
    expect(went).toEqual(["Hello", "and more", "Something else"]);
    // A send that did not go takes nothing from the next: Return again tries the words again.
    const refused: string[] = [];
    await Promise.all([turns("a", "Again", async (words) => (refused.push(words), false)), turns("a", "Again", async (words) => (refused.push(words), true))]);
    expect(refused).toEqual(["Again", "Again"]);
  });
});
