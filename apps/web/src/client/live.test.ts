import { describe, expect, test } from "bun:test";
import { liveItems, sameIds } from "./live";

interface Item {
  id: string;
  body: string;
  reactions: string[];
}

describe("liveItems", () => {
  test("one view per id, reading what the list holds now", () => {
    let map = new Map<string, Item>([["a", { id: "a", body: "hi", reactions: [] }]]);
    const live = liveItems(() => map);
    const a = live("a");
    expect(a.body).toBe("hi");
    // Read again from the core: new objects, the same row.
    map = new Map([["a", { id: "a", body: "hi (edited)", reactions: ["👍"] }]]);
    expect(live("a")).toBe(a);
    expect(a.body).toBe("hi (edited)");
    expect(a.reactions).toEqual(["👍"]);
    expect({ ...a }).toEqual({ id: "a", body: "hi (edited)", reactions: ["👍"] });
  });

  test("a view outlives its id with what it last held; a view of it made again is new", () => {
    let map = new Map<string, Item>([["a", { id: "a", body: "bye", reactions: [] }]]);
    const live = liveItems(() => map);
    const a = live("a");
    map = new Map([["b", { id: "b", body: "", reactions: [] }]]);
    live("b");
    expect(a.body).toBe("bye");
    map = new Map([["a", { id: "a", body: "back", reactions: [] }]]);
    expect(live("a")).not.toBe(a);
  });
});

describe("sameIds", () => {
  test("the same keys in the same order", () => {
    expect(sameIds(["a", "b"], ["a", "b"])).toBe(true);
    expect(sameIds(["a", "b"], ["b", "a"])).toBe(false);
    expect(sameIds(["a"], ["a", "b"])).toBe(false);
    expect(sameIds([], [])).toBe(true);
  });
});
