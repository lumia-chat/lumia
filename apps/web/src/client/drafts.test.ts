import { describe, expect, test } from "bun:test";
import { DraftKeeper, type DraftStore, type Held } from "./drafts";

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
});
