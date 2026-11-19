import { describe, expect, test } from "bun:test";
import { createMemo, createRoot, createSignal, mapArray } from "solid-js";
import { createStore, unwrap } from "solid-js/store";
import { keep, keptById, plain, sameList, sameValue, type Kept } from "./live";

interface File {
  fileId: string;
  name: string;
  expired: boolean;
}
interface Item {
  id: string;
  body: string;
  blocks: { kind: string; text: string }[];
  reactions: string[];
  files: File[];
}

const kept: Kept<Item> = { key: "id", lists: { files: { key: "fileId" } } };
const item = (id: string, body: string, more: Partial<Item> = {}): Item => ({ id, body, blocks: [{ kind: "para", text: body }], reactions: [], files: [], ...more });
const file = (fileId: string, more: Partial<File> = {}): File => ({ fileId, name: fileId + ".pdf", expired: false, ...more });

describe("keep", () => {
  test("an element read again is the object it was, with what changed written into it", () => {
    const held: Item[] = [];
    keep(held, [item("a", "hi"), item("b", "there")], kept);
    const [a, b] = held;
    const blocks = a?.blocks;
    // Read again from the core: new objects.
    keep(held, [item("a", "hi", { reactions: ["👍"] }), item("b", "there (edited)")], kept);
    expect(held[0]).toBe(a);
    expect(held[1]).toBe(b);
    expect(a?.reactions).toEqual(["👍"]);
    expect(b?.body).toBe("there (edited)");
    // A value that holds what it held is not written again: what was drawn from it stays.
    expect(a?.blocks).toBe(blocks);
    expect(b?.blocks).toEqual([{ kind: "para", text: "there (edited)" }]);
  });

  test("a new element is a copy: what was handed in is never written to", () => {
    const first = item("a", "hi", { files: [file("f")] });
    const held: Item[] = [];
    keep(held, [first], kept);
    expect(held[0]).not.toBe(first);
    expect(held[0]?.files[0]).not.toBe(first.files[0]);
    keep(held, [item("a", "bye", { files: [file("f", { expired: true })] })], kept);
    expect(held[0]?.body).toBe("bye");
    expect(first.body).toBe("hi");
    expect(first.files[0]?.expired).toBe(false);
  });

  test("the order is the new list's; one that went keeps what it last held, and comes back new", () => {
    const held: Item[] = [];
    keep(held, [item("a", "one"), item("b", "two"), item("c", "three")], kept);
    const [a, b, c] = held;
    keep(held, [item("z", "zero"), item("c", "three"), item("a", "one")], kept);
    expect(held.map((m) => m.id)).toEqual(["z", "c", "a"]);
    expect(held[1]).toBe(c);
    expect(held[2]).toBe(a);
    expect(b?.body).toBe("two");
    keep(held, [item("b", "two")], kept);
    expect(held).toHaveLength(1);
    expect(held[0]).not.toBe(b);
  });

  test("the lists inside an element are kept by their own key", () => {
    const held: Item[] = [];
    keep(held, [item("a", "files", { files: [file("f"), file("g")] })], kept);
    const files = held[0]?.files;
    const [f, g] = files ?? [];
    // The first taken off by an edit, the second let go by the server: each is still itself.
    keep(held, [item("a", "files", { files: [file("g", { expired: true })] })], kept);
    expect(held[0]?.files).toBe(files);
    expect(files).toHaveLength(1);
    expect(files?.[0]).toBe(g);
    expect(g?.expired).toBe(true);
    expect(f?.fileId).toBe("f");
    expect(f?.expired).toBe(false);
  });

  test("a key held twice is matched in order", () => {
    const held: File[] = [];
    keep(held, [file("f", { name: "one" }), file("f", { name: "two" })], { key: "fileId" });
    const [one, two] = held;
    keep(held, [file("f", { name: "one" }), file("f", { name: "two!" })], { key: "fileId" });
    expect(held[0]).toBe(one);
    expect(held[1]).toBe(two);
    expect(two?.name).toBe("two!");
  });
});

describe("keptById", () => {
  test("a row is made once for its id, and told only of the fields it read", () => {
    createRoot((dispose) => {
      const [list, setList] = createSignal([item("a", "hi"), item("b", "there")]);
      const held = keptById(list, kept);
      let made = 0;
      const rows = createMemo(
        mapArray(
          () => held,
          (m) => {
            made++;
            let bodies = 0;
            let reacted = 0;
            const body = createMemo(() => (bodies++, m.body));
            const reactions = createMemo(() => (reacted++, m.reactions.length));
            return { m, body, reactions, counts: () => ({ bodies, reacted }) };
          },
        ),
      );
      const read = () => rows().map((r) => [r.body(), r.reactions()]);
      expect(read()).toEqual([
        ["hi", 0],
        ["there", 0],
      ]);
      const a = held[0];
      // A reaction on one, and another message arriving.
      setList([item("a", "hi", { reactions: ["👍"] }), item("b", "there"), item("c", "new")]);
      expect(read()).toEqual([
        ["hi", 1],
        ["there", 0],
        ["new", 0],
      ]);
      expect(made).toBe(3);
      expect(held[0]).toBe(a);
      expect(rows()[0]?.counts()).toEqual({ bodies: 1, reacted: 2 });
      expect(rows()[1]?.counts()).toEqual({ bodies: 1, reacted: 1 });
      // The same messages read again: nothing is told.
      setList([item("a", "hi", { reactions: ["👍"] }), item("b", "there"), item("c", "new")]);
      read();
      expect(made).toBe(3);
      expect(rows()[0]?.counts()).toEqual({ bodies: 1, reacted: 2 });
      dispose();
    });
  });

  test("an attachment the server lets go is the same row, told so", () => {
    createRoot((dispose) => {
      const [list, setList] = createSignal([item("a", "a file", { files: [file("f")] })]);
      const held = keptById(list, kept);
      let made = 0;
      const rows = createMemo(
        mapArray(
          () => held[0]?.files ?? [],
          (f) => (made++, createMemo(() => (f.expired ? "No longer available" : f.name))),
        ),
      );
      expect(rows().map((r) => r())).toEqual(["f.pdf"]);
      setList([item("a", "a file", { files: [file("f", { expired: true })] })]);
      expect(rows().map((r) => r())).toEqual(["No longer available"]);
      expect(made).toBe(1);
      dispose();
    });
  });

  test("what it is read from is not written to, a store's proxies among it", () => {
    createRoot((dispose) => {
      // As a query holds what it read: in a store, replaced whole each time it is read again.
      const first = item("a", "hi", { files: [file("f")] });
      const [query, setQuery] = createStore<{ data: Item[] }>({ data: [first] });
      const held = keptById(() => query.data, kept);
      expect(held[0]?.body).toBe("hi");
      setQuery("data", [item("a", "bye", { files: [file("f", { expired: true })] })]);
      expect(held[0]?.body).toBe("bye");
      expect(held[0]?.files[0]?.expired).toBe(true);
      expect(first.body).toBe("hi");
      expect(first.files[0]?.expired).toBe(false);
      // And holds none of the query's proxies: what a worker's message can carry.
      expect(() => structuredClone(unwrap(held))).not.toThrow();
      dispose();
    });
  });
});

describe("sameValue", () => {
  test("what the core hands over, by what it holds", () => {
    expect(sameValue({ a: [1, { b: "x" }], c: null }, { a: [1, { b: "x" }], c: null })).toBe(true);
    expect(sameValue({ a: [1, { b: "x" }] }, { a: [1, { b: "y" }] })).toBe(false);
    expect(sameValue([1, 2], [1, 2, 3])).toBe(false);
    expect(sameValue({ a: 1 }, { a: 1, b: undefined })).toBe(false);
    expect(sameValue({ a: undefined }, { b: undefined })).toBe(false);
    expect(sameValue(null, {})).toBe(false);
    expect(sameValue([], {})).toBe(false);
    expect(sameValue("a", "a")).toBe(true);
  });
});

describe("sameList", () => {
  test("the same elements in the same order", () => {
    const a = {};
    const b = {};
    expect(sameList([a, b], [a, b])).toBe(true);
    expect(sameList([a, b], [b, a])).toBe(false);
    expect(sameList([a], [a, b])).toBe(false);
    expect(sameList([a], [{}])).toBe(false);
    expect(sameList([], [])).toBe(true);
  });
});

describe("plain", () => {
  test("what a store holds can be posted once it is plain, objects within objects too", () => {
    const [held] = createStore({ members: [{ accountId: "ada", roleIds: ["eng"], tag: { label: "Engineering", color: "sky" } }] });
    expect(() => structuredClone(held.members)).toThrow();
    expect(structuredClone(plain(held.members))).toEqual([{ accountId: "ada", roleIds: ["eng"], tag: { label: "Engineering", color: "sky" } }]);
  });
  test("what no store holds is handed back as it is", () => {
    const list = [{ a: 1 }];
    expect(plain(list)).toBe(list);
    expect(plain("text")).toBe("text");
    expect(plain(undefined)).toBeUndefined();
  });
});
