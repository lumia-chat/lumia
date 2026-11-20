import { describe, expect, test } from "bun:test";
import type { MessageSummary, ReactionSummary } from "./bridge";
import { historyAgain, namesLeft } from "./history";

const reaction = (emojiId: string, reaction: string): ReactionSummary => ({ reaction, count: 1, me: false, accountIds: [], emojiId });
const message = (seq: number, reactions: ReactionSummary[] = []): MessageSummary =>
  ({ messageId: `m${seq}`, channelId: "c", seq, body: `${seq}`, reactions }) as unknown as MessageSummary;

// An instance's channel of messages 1..last, answering `GET .../messages?before=&limit=` as the core's `olderMessages` does.
function instance(last: number, label = (seq: number) => `${seq}`) {
  const asked: [number, number][] = [];
  const read = async (before: number, limit: number) => {
    asked.push([before, limit]);
    const top = Math.min(before - 1, last);
    const bottom = Math.max(1, top - limit + 1);
    const messages = top < 1 ? [] : Array.from({ length: top - bottom + 1 }, (_, i) => ({ ...message(bottom + i), body: label(bottom + i) }));
    return { messages, hasMore: bottom > 1 };
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

describe("what scrollback names by a name the page cannot know", () => {
  test("a reaction with an emoji its server no longer lists, not already read again", () => {
    const messages = [message(1, [reaction("e1", ":parrot:"), reaction("", "👍")]), message(2, [reaction("e2", ":dance:"), reaction("e3", ":gone:")])];
    expect(namesLeft(messages, new Set(["e2"]), new Set())).toEqual(["e1", "e3"]);
    expect(namesLeft(messages, new Set(["e2"]), new Set(["e3"]))).toEqual(["e1"]);
    // An emoji listed is drawn by its present name: nothing to read for it.
    expect(namesLeft(messages, new Set(["e1", "e2", "e3"]), new Set())).toEqual([]);
  });
});
