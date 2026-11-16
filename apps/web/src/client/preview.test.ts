import { describe, expect, test } from "bun:test";
import { PRESETS } from "./preferences";
import { PREVIEW_MESSAGES, previewAt, previewDrawn, type PreviewMessage } from "./preview";

const [priya1, priya2, tomas, mei] = PREVIEW_MESSAGES as [PreviewMessage, PreviewMessage, PreviewMessage, PreviewMessage];

describe("the preview's sample messages", () => {
  test("four, the spec's, in order", () => {
    expect(PREVIEW_MESSAGES.map((m) => [m.author, `${m.hour}:${m.minute}`])).toEqual([
      ["Priya Nair", "10:42"],
      ["Priya Nair", "10:42"],
      ["Tomás Reyes", "10:44"],
      ["Mei Chen", "10:51"],
    ]);
    expect(mei.link?.words).toBe("lumia.chat/blog/0-13");
    expect(mei.text.endsWith("lumia.chat/blog/0-13")).toBe(true);
  });

  test("sent today at their time", () => {
    const today = new Date(2026, 10, 16, 18, 5);
    expect(new Date(previewAt(priya1, today) / 1000)).toEqual(new Date(2026, 10, 16, 10, 42));
  });
});

describe("what each draws", () => {
  test("Comfortable: the picture, the card and the reactions, the 🎉 one this account's", () => {
    const d = previewDrawn(tomas, PRESETS.comfortable);
    expect(d.picture).toBe(true);
    expect(d.fileRow).toBe(false);
    expect(d.reactions.map((r) => [r.emoji, r.count, r.mine])).toEqual([
      ["👀", 2, false],
      ["🎉", 3, true],
    ]);
    expect(previewDrawn(mei, PRESETS.comfortable).card).toBe(true);
  });

  test("Images as links: the file row, not the picture", () => {
    const d = previewDrawn(tomas, { ...PRESETS.comfortable, images: "link" });
    expect(d.picture).toBe(false);
    expect(d.fileRow).toBe(true);
  });

  test("Embeds as links: no card", () => {
    expect(previewDrawn(mei, { ...PRESETS.comfortable, embeds: "link" }).card).toBe(false);
  });

  test("Reactions hidden: none", () => {
    expect(previewDrawn(tomas, { ...PRESETS.comfortable, reactions: "hidden" }).reactions).toEqual([]);
  });

  test("Minimal: the file row, no card, no reactions", () => {
    expect(previewDrawn(tomas, PRESETS.minimal)).toEqual({ picture: false, fileRow: true, card: false, reactions: [] });
    expect(previewDrawn(mei, PRESETS.minimal).card).toBe(false);
  });

  test("a message with nothing under it draws nothing under it", () => {
    expect(previewDrawn(priya2, PRESETS.comfortable)).toEqual({ picture: false, fileRow: false, card: false, reactions: [] });
  });
});
