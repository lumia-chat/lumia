import { describe, expect, test } from "bun:test";
import type { MessageSummary, ReactionSummary } from "./bridge";
import { Rereads, ask, followed, historyAgain, joined, namesLeft, oneAgain, touched, withMessage } from "./history";

const reaction = (emojiId: string, reaction: string): ReactionSummary => ({ reaction, count: 1, me: false, accountIds: [], emojiId });
const message = (seq: number, reactions: ReactionSummary[] = []): MessageSummary =>
  ({ messageId: `m${seq}`, channelId: "c", seq, body: `${seq}`, reactions }) as unknown as MessageSummary;

// An instance's channel of messages 1..last, answering `GET .../messages?before=&limit=` as the core's `olderMessages` does:
// `gone` are deleted and left out, the read going on below them.
function instance(last: number, label = (seq: number) => `${seq}`, gone: ReadonlySet<number> = new Set()) {
  const asked: [number, number][] = [];
  const read = async (before: number, limit: number) => {
    asked.push([before, limit]);
    const messages: MessageSummary[] = [];
    let seq = Math.min(before - 1, last);
    for (; seq >= 1 && messages.length < limit; seq--) if (!gone.has(seq)) messages.unshift({ ...message(seq), body: label(seq) });
    return { messages, hasMore: seq >= 1 };
  };
  return { read, asked };
}

describe("the instance's scrollback held, read again", () => {
  test("over the span it holds, a page at a time from its newest down to its oldest", async () => {
    const kept = { messages: Array.from({ length: 450 }, (_, i) => message(301 + i)), more: true };
    const { read, asked } = instance(1000, (seq) => `again ${seq}`);
    const again = await historyAgain(kept, read, 200);
    expect(asked).toEqual([
      [751, 200],
      [551, 200],
      [351, 200],
    ]);
    // What the last page brought from below the oldest held is kept too, and whether more remains.
    expect(again.messages.map((m) => m.seq)).toEqual(Array.from({ length: 600 }, (_, i) => 151 + i));
    expect(again.messages.every((m) => m.body === `again ${m.seq}`)).toBe(true);
    expect(again.more).toBe(true);
  });

  test("down to the channel's first message", async () => {
    const kept = { messages: [message(1), message(2), message(3)], more: false };
    const { read } = instance(10);
    const again = await historyAgain(kept, read, 200);
    expect(again.messages.map((m) => m.seq)).toEqual([1, 2, 3]);
    expect(again.more).toBe(false);
  });

  test("nothing held, nothing asked", async () => {
    const { read, asked } = instance(10);
    const kept = { messages: [], more: true };
    expect(await historyAgain(kept, read, 200)).toBe(kept);
    expect(asked).toEqual([]);
  });
});

const event = (kind: string, over: { seq?: number; messageId?: string; beforeSeq?: number } = {}) => ({ kind, seq: 99, messageId: "", beforeSeq: 0, ...over });

describe("what of the messages held an event changed", () => {
  const held = [message(10), message(11), message(12)];

  test("each message an event names, where it is held", () => {
    const a = touched([event("reaction_added", { messageId: "m11" }), event("message_edited", { messageId: "m12" }), event("message_deleted", { messageId: "m500" }), event("reaction_removed", { messageId: "m11" })], held);
    expect([...a.ids]).toEqual(["m11", "m12"]);
    expect(a.below).toBe(0);
    expect(a.whole).toBe(false);
  });

  test("a new message is never one held, and an event about no message names none", () => {
    expect(touched([event("message_created", { messageId: "m11" }), event("member_joined"), event("acl_changed")], held).ids.size).toBe(0);
  });

  test("a redaction by the seq of the message it took the payload of", () => {
    expect([...touched([event("redacted", { seq: 10 }), event("redacted", { seq: 700 })], held).ids]).toEqual(["m10"]);
  });

  test("an expiry or a purge below a seq, where something held is below it", () => {
    expect(touched([event("attachments_expired", { beforeSeq: 12 }), event("payloads_purged", { beforeSeq: 11 })], held).below).toBe(12);
    expect(touched([event("attachments_expired", { beforeSeq: 10 })], held).below).toBe(0);
  });

  test("nothing held, nothing touched", () => {
    expect(touched([event("message_edited", { messageId: "m11" })], []).ids.size).toBe(0);
  });

  test("two asks as one", () => {
    const a = joined(ask({ ids: new Set(["a"]), below: 5 }), ask({ ids: new Set(["b"]), below: 3, names: new Set(["e"]) }));
    expect([[...a.ids], a.below, a.whole, [...a.names]]).toEqual([["a", "b"], 5, false, ["e"]]);
    expect(joined(undefined, a)).toBe(a);
    expect(joined(a, ask({ whole: true })).whole).toBe(true);
  });
});

describe("a message read again on its own", () => {
  test("by its seq, one message asked for", async () => {
    const { read, asked } = instance(50, (seq) => `now ${seq}`);
    expect((await oneAgain("m20", 20, read))?.body).toBe("now 20");
    expect(asked).toEqual([[21, 1]]);
  });

  test("gone where the instance leaves it out (deleted), whatever comes from below it", async () => {
    const { read } = instance(50, undefined, new Set([20]));
    expect(await oneAgain("m20", 20, read)).toBeNull();
    const first = instance(50, undefined, new Set([1]));
    expect(await oneAgain("m1", 1, first.read)).toBeNull();
  });

  test("written in where it is held, or taken out", () => {
    const list = [message(1), message(2), message(3)];
    const now = { ...message(2), body: "edited" };
    expect(withMessage(list, "m2", now).map((m) => m.body)).toEqual(["1", "edited", "3"]);
    expect(withMessage(list, "m2", null).map((m) => m.seq)).toEqual([1, 3]);
    expect(withMessage(list, "m9", now)).toBe(list);
  });
});

describe("the history held after an ask", () => {
  const kept = { messages: Array.from({ length: 300 }, (_, i) => message(101 + i)), more: true };

  test("each message named read on its own, the rest as they were", async () => {
    const { read, asked } = instance(1000, (seq) => `again ${seq}`, new Set([150]));
    const h = await followed(kept, ask({ ids: new Set(["m200", "m150", "m999"]) }), read, 200);
    // m999 is not held: nothing asked for it.
    expect(asked).toEqual([
      [201, 1],
      [151, 1],
    ]);
    expect(h.messages.find((m) => m.seq === 200)?.body).toBe("again 200");
    expect(h.messages.some((m) => m.seq === 150)).toBe(false);
    expect(h.messages.find((m) => m.seq === 201)?.body).toBe("201");
    expect(h.messages.length).toBe(299);
    expect(h.more).toBe(true);
  });

  test("the span below a seq read again, the newer kept, and a message named in it not read twice", async () => {
    const { read, asked } = instance(1000, (seq) => `again ${seq}`);
    const h = await followed(kept, ask({ below: 151, ids: new Set(["m120", "m300"]) }), read, 200);
    expect(asked).toEqual([
      [151, 200],
      [301, 1],
    ]);
    expect(h.messages.map((m) => m.seq)).toEqual([...Array.from({ length: 150 }, (_, i) => 1 + i), ...Array.from({ length: 250 }, (_, i) => 151 + i)]);
    expect(h.messages.filter((m) => m.seq <= 150).every((m) => m.body === `again ${m.seq}`)).toBe(true);
    expect(h.messages.find((m) => m.seq === 300)?.body).toBe("again 300");
    expect(h.messages.find((m) => m.seq === 151)?.body).toBe("151");
    expect(h.more).toBe(false);
  });

  test("all of it, after events skipped or an emoji removed", async () => {
    const { read, asked } = instance(1000, (seq) => `again ${seq}`);
    const h = await followed(kept, ask({ whole: true, ids: new Set(["m200"]) }), read, 200);
    expect(asked).toEqual([
      [401, 200],
      [201, 200],
    ]);
    expect(h.messages.every((m) => m.body === `again ${m.seq}`)).toBe(true);
  });

  test("nothing asked, nothing read", async () => {
    const { read, asked } = instance(1000);
    expect(await followed(kept, ask(), read, 200)).toBe(kept);
    expect(asked).toEqual([]);
  });
});

describe("reads again, coalesced", () => {
  // A read that waits until it is let go, recording what it was asked.
  function reads() {
    const began: [string, string[], number, boolean][] = [];
    const waiting: (() => void)[] = [];
    const read = (key: string, a: ReturnType<typeof ask>) => {
      began.push([key, [...a.ids], a.below, a.whole]);
      return new Promise<void>((done) => waiting.push(done));
    };
    const finish = async () => {
      waiting.shift()?.();
      // The read's `finally`, and the next one it starts.
      for (let i = 0; i < 5; i++) await Promise.resolve();
    };
    return { began, read, finish };
  }
  const ids = (...m: string[]) => ask({ ids: new Set(m) });

  test("asks made while one is on its way are taken together by one more after it", async () => {
    const { began, read, finish } = reads();
    const r = new Rereads(read, () => true);
    r.ask("c", ids("m1"));
    r.ask("c", ids("m2"));
    r.ask("c", ask({ below: 7 }));
    r.ask("c", ids("m3", "m2"));
    expect(began).toEqual([["c", ["m1"], 0, false]]);
    await finish();
    expect(began).toEqual([
      ["c", ["m1"], 0, false],
      ["c", ["m2", "m3"], 7, false],
    ]);
    await finish();
    expect(began.length).toBe(2);
  });

  test("an ask for nothing starts nothing, and each channel goes on its own", () => {
    const { began, read } = reads();
    const r = new Rereads(read, () => true);
    r.ask("a", ask());
    expect(began).toEqual([]);
    r.ask("a", ids("m1"));
    r.ask("b", ids("m2"));
    expect(began.map(([k]) => k)).toEqual(["a", "b"]);
  });

  test("one asked while its channel is not on screen waits for it", async () => {
    const { began, read, finish } = reads();
    let shown = false;
    const r = new Rereads(read, () => shown);
    r.ask("c", ids("m1"));
    r.ask("c", ask({ whole: true }));
    r.run("c");
    expect(began).toEqual([]);
    shown = true;
    r.run("c");
    r.run("c");
    expect(began).toEqual([["c", ["m1"], 0, true]]);
    await finish();
    expect(began.length).toBe(1);
  });

  test("a read that fails is not asked again until the next ask, and forget lets go of what was not begun", async () => {
    const began: string[] = [];
    const r = new Rereads(
      async (key) => {
        began.push(key);
        throw new Error("unreachable");
      },
      () => true,
    );
    r.ask("c", ids("m1"));
    for (let i = 0; i < 5; i++) await Promise.resolve();
    expect(began).toEqual(["c"]);
    let shown = false;
    const later = new Rereads(
      async (key) => void began.push(`later ${key}`),
      () => shown,
    );
    later.ask("c", ids("m1"));
    later.forget();
    shown = true;
    later.run("c");
    expect(began).toEqual(["c"]);
  });
});

describe("what scrollback names by a name the page cannot know", () => {
  test("a reaction with an emoji its server no longer lists, not already read again", () => {
    const messages = [message(1, [reaction("e1", ":parrot:"), reaction("", "👍")]), message(2, [reaction("e2", ":dance:"), reaction("e3", ":gone:")])];
    expect(namesLeft(messages, new Set(["e2"]), new Set())).toEqual(["e1", "e3"]);
    expect(namesLeft(messages, new Set(["e2"]), new Set(["e3"]))).toEqual(["e1"]);
    // An emoji listed is drawn by its present name: nothing to read for it.
    expect(namesLeft(messages, new Set(["e1", "e2", "e3"]), new Set())).toEqual([]);
  });
});
