import { describe, expect, test } from "bun:test";
import { PRESETS, presetOf, readPreferences, switchCaption, switchOn, switchValue } from "./preferences";

describe("the switches", () => {
  test("on is Shown; off is As links for Images and Embeds, Hidden for Reactions", () => {
    expect(switchValue("images", true)).toEqual({ images: "shown" });
    expect(switchValue("images", false)).toEqual({ images: "link" });
    expect(switchValue("embeds", false)).toEqual({ embeds: "link" });
    expect(switchValue("reactions", false)).toEqual({ reactions: "hidden" });
    expect(switchValue("reactions", true)).toEqual({ reactions: "shown" });
  });

  test("each caption says what the state does", () => {
    const on = PRESETS.comfortable;
    const off = PRESETS.minimal;
    expect(switchCaption(on, "images")).toBe("Shown in their messages.");
    expect(switchCaption(off, "images")).toBe("As links: each one a file row that opens the viewer.");
    expect(switchCaption(on, "embeds")).toBe("A card under each link.");
    expect(switchCaption(off, "embeds")).toBe("As links: no cards.");
    expect(switchCaption(on, "reactions")).toBe("Shown under each message.");
    expect(switchCaption(off, "reactions")).toBe("Hidden. You can still add one from a message’s toolbar.");
  });

  test("a switch turned writes a value its state reads back", () => {
    for (const setting of ["images", "embeds", "reactions"] as const) {
      for (const on of [true, false]) expect(switchOn({ ...PRESETS.comfortable, ...switchValue(setting, on) }, setting)).toBe(on);
    }
  });

  test("one turned off from Comfortable is Custom", () => {
    expect(presetOf({ ...PRESETS.comfortable, ...switchValue("embeds", false) })).toBeUndefined();
    expect(presetOf({ ...PRESETS.minimal, ...switchValue("reactions", true) })).toBeUndefined();
  });
});

describe("presets", () => {
  test("each preset's values are that preset", () => {
    expect(presetOf(PRESETS.comfortable)).toBe("comfortable");
    expect(presetOf(PRESETS.compact)).toBe("compact");
    expect(presetOf(PRESETS.minimal)).toBe("minimal");
  });

  test("one setting changed is no preset", () => {
    expect(presetOf({ ...PRESETS.comfortable, spacing: "tight" })).toBeUndefined();
    expect(presetOf({ ...PRESETS.compact, avatars: "hidden" })).toBeUndefined();
    expect(presetOf({ ...PRESETS.minimal, layout: "grouped" })).toBeUndefined();
  });

  test("one line draws no avatar, so its avatars are not compared", () => {
    expect(presetOf({ ...PRESETS.minimal, avatars: "large" })).toBe("minimal");
  });

  test("Minimal hides reactions; Comfortable and Compact show them", () => {
    expect(PRESETS.comfortable.reactions).toBe("shown");
    expect(PRESETS.compact.reactions).toBe("shown");
    expect(PRESETS.minimal.reactions).toBe("hidden");
  });

  test("reactions are compared like the rest", () => {
    expect(presetOf({ ...PRESETS.minimal, reactions: "shown" })).toBeUndefined();
    expect(presetOf({ ...PRESETS.comfortable, reactions: "hidden" })).toBeUndefined();
  });

  test("Minimal kept before reactions were a setting reads as Custom: no migration", () => {
    const { reactions: _, ...before } = PRESETS.minimal;
    expect(presetOf(readPreferences(JSON.stringify(before)))).toBeUndefined();
    expect(presetOf(readPreferences(JSON.stringify(PRESETS.minimal)))).toBe("minimal");
  });
});

describe("what was stored", () => {
  test("nothing, or not JSON, is the defaults: Comfortable at 15", () => {
    for (const stored of [null, "", "not json", "[]"]) {
      const p = readPreferences(stored);
      expect(presetOf(p)).toBe("comfortable");
      expect(p.fontSize).toBe(15);
      expect(p.appearance).toBe("system");
      expect(p.reactions).toBe("shown");
    }
  });

  test("each value reads back", () => {
    const p = readPreferences(JSON.stringify({ appearance: "dark", layout: "one_line", spacing: "cozy", avatars: "small", fontSize: 18, reactions: "hidden", images: "link", embeds: "link", asideHidden: true }));
    expect(p).toEqual({ appearance: "dark", layout: "one_line", spacing: "cozy", avatars: "small", fontSize: 18, reactions: "hidden", images: "link", embeds: "link", asideHidden: true });
  });

  test("reactions missing or not understood are shown", () => {
    expect(readPreferences(JSON.stringify({ layout: "one_line" })).reactions).toBe("shown");
    for (const reactions of ["none", "Hidden", true, 0, null]) expect(readPreferences(JSON.stringify({ reactions })).reactions).toBe("shown");
  });

  test("a value not understood is that setting's default, and the others stay", () => {
    const p = readPreferences(JSON.stringify({ layout: "irc", spacing: "tight", fontSize: 17, avatars: 3 }));
    expect(p.layout).toBe("grouped");
    expect(p.spacing).toBe("tight");
    expect(p.fontSize).toBe(15);
    expect(p.avatars).toBe("large");
  });

  test("the old density is not read: no migration", () => {
    expect(presetOf(readPreferences(JSON.stringify({ density: "compact" })))).toBe("comfortable");
  });
});
