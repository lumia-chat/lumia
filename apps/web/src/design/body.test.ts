import { describe, expect, test } from "bun:test";
import { COLLAPSED_LINES, collapsedLines, grouped, linesOf, roomWords, stretches } from "./body";

describe("a long body collapses", () => {
  test("over fourteen lines it is drawn at its first ten", () => {
    expect(collapsedLines(15)).toBe(COLLAPSED_LINES);
    expect(collapsedLines(400)).toBe(10);
  });

  test("fourteen lines or fewer are drawn whole", () => {
    expect(collapsedLines(14)).toBeUndefined();
    expect(collapsedLines(1)).toBeUndefined();
    expect(collapsedLines(0)).toBeUndefined();
  });

  test("lines are the height drawn in lines of the text, not the newlines typed", () => {
    expect(linesOf(24 * 14, 24)).toBe(14);
    // Fourteen lines and a paragraph's spacing are still fourteen.
    expect(linesOf(24 * 14 + 4, 24)).toBe(14);
    expect(linesOf(24 * 15, 24)).toBe(15);
    expect(linesOf(100, 0)).toBe(0);
  });
});

describe("the counter", () => {
  test("says what is left, a character in the singular", () => {
    expect(roomWords(12)).toBe("12 characters left");
    expect(roomWords(1)).toBe("1 character left");
    expect(roomWords(0)).toBe("0 characters left");
  });

  test("says how much too long a draft is", () => {
    expect(roomWords(-1)).toBe("1 character too long");
    expect(roomWords(-37)).toBe("37 characters too long");
  });

  test("groups thousands", () => {
    expect(grouped(1204)).toBe("1,204");
    expect(grouped(16384)).toBe("16,384");
    expect(grouped(999)).toBe("999");
    expect(roomWords(1204)).toBe("1,204 characters left");
    expect(roomWords(-1204)).toBe("1,204 characters too long");
  });
});

describe("a paragraph's pieces", () => {
  test("plain text is one piece", () => {
    expect(stretches("hello", [], [])).toEqual([{ url: undefined, pieces: [{ text: "hello", emphasis: false, strong: false, strike: false, code: false }] }]);
    expect(stretches("", [], [])).toEqual([]);
  });

  test("overlapping runs cut the text where each begins and ends", () => {
    // "a bold italic b": strong over "bold italic", emphasis over "italic".
    const out = stretches("a bold italic b", [
      { start: 2, end: 13, style: { kind: "strong" } },
      { start: 7, end: 13, style: { kind: "emphasis" } },
    ], []);
    expect(out).toHaveLength(1);
    expect(out[0]?.pieces.map((p) => [p.text, p.strong, p.emphasis])).toEqual([
      ["a ", false, false],
      ["bold ", true, false],
      ["italic", true, true],
      [" b", false, false],
    ]);
  });

  test("a mention is a piece of its own, with the styles over it", () => {
    const out = stretches("hi @ada", [
      { start: 3, end: 7, style: { kind: "mention", index: 2 } },
      { start: 3, end: 7, style: { kind: "strong" } },
    ], []);
    const ada = out[0]?.pieces[1];
    expect(ada?.text).toBe("@ada");
    expect(ada?.mention).toBe(2);
    expect(ada?.strong).toBe(true);
  });

  test("a link's pieces are grouped under it, styled inside", () => {
    const out = stretches("see the docs now", [{ start: 8, end: 12, style: { kind: "code" } }], [{ start: 4, end: 12, url: "https://lumia.chat" }]);
    expect(out.map((s) => [s.url, s.pieces.map((p) => p.text)])).toEqual([
      [undefined, ["see "]],
      ["https://lumia.chat", ["the ", "docs"]],
      [undefined, [" now"]],
    ]);
    expect(out[1]?.pieces[1]?.code).toBe(true);
  });

  test("two links side by side stay two", () => {
    const out = stretches("ab", [], [
      { start: 0, end: 1, url: "https://a.example" },
      { start: 1, end: 2, url: "https://a.example" },
    ]);
    expect(out).toHaveLength(2);
  });

  test("ranges past the text are kept inside it", () => {
    const out = stretches("abc", [{ start: 1, end: 99, style: { kind: "strike" } }], []);
    expect(out[0]?.pieces.map((p) => [p.text, p.strike])).toEqual([
      ["a", false],
      ["bc", true],
    ]);
  });
});
