import { describe, expect, test } from "bun:test";
import { ClientError } from "./bridge";
import { NAME_RULE, TOO_LARGE, colonAt, emojiCount, emojiFull, emojiListed, emojiRefusal, emojiSubtitle, removeTitle, sizeWords, spliced, typedEmoji } from "./emoji";
import { customKey, frequentEmoji, pictureRetryIn } from "../design/emoji";

describe("the composer's : list", () => {
  test("opens on a colon at the start or after a space, with what follows it", () => {
    expect(colonAt(":pa", 3)).toEqual({ start: 0, query: "pa" });
    expect(colonAt("shipped :party", 14)).toEqual({ start: 8, query: "party" });
    expect(colonAt("(:lg", 4)).toEqual({ start: 1, query: "lg" });
    expect(colonAt("hi :", 4)).toEqual({ start: 3, query: "" });
  });

  test("after another emoji's closing colon too", () => {
    expect(colonAt(":lgtm::pa", 9)).toEqual({ start: 6, query: "pa" });
  });

  test("not in a time or a word with a colon in it", () => {
    expect(colonAt("at 10:30", 8)).toBeNull();
    expect(colonAt("note:pa", 7)).toBeNull();
  });

  test("not once the name is closed or a space is typed after it", () => {
    expect(colonAt(":partyparrot:", 13)).toBeNull();
    expect(colonAt(":party parrot", 13)).toBeNull();
  });

  test("reads what is before the caret only", () => {
    expect(colonAt("x :pa and more", 5)).toEqual({ start: 2, query: "pa" });
  });
});

describe("what is written into the field", () => {
  test("a server's emoji as its name between colons, a space after it", () => {
    expect(typedEmoji("partyparrot")).toBe(":partyparrot: ");
  });

  test("in place of the typed prefix, the caret after it", () => {
    expect(spliced("Shipped :par", 8, 12, typedEmoji("partyparrot"))).toEqual({ text: "Shipped :partyparrot: ", caret: 22 });
  });

  test("at the caret, over what is selected", () => {
    expect(spliced("ab", 1, 1, "🎉")).toEqual({ text: "a🎉b", caret: 3 });
    expect(spliced("a big b", 2, 5, "🎉")).toEqual({ text: "a 🎉 b", caret: 4 });
    expect(spliced("ab", 9, 9, "!")).toEqual({ text: "ab!", caret: 3 });
  });
});

describe("the Emoji section", () => {
  test("counts against the limit", () => {
    expect(emojiCount(12, 50)).toBe("12 of 50");
    expect(emojiCount(1200, 5000)).toBe("1,200 of 5,000");
  });

  test("is full at the limit and over it", () => {
    expect(emojiFull(49, 50)).toBe(false);
    expect(emojiFull(50, 50)).toBe(true);
    expect(emojiFull(52, 50)).toBe(true);
  });

  test("a limit of none is a server with no room, not one with no limit", () => {
    // The instance takes `emoji-limit <server> 0` and then refuses every upload (specs/emoji.md 5).
    expect(emojiCount(0, 0)).toBe("0 of 0");
    expect(emojiCount(3, 0)).toBe("3 of 0");
    expect(emojiFull(0, 0)).toBe(true);
    expect(emojiFull(9, 0)).toBe(true);
    // Removing one lets none in at a limit of 0, so nothing says to; it does where removing one would.
    expect(emojiSubtitle("Acme", 0, 0)).toBe("Acme has as many emoji as it may (0).");
    expect(emojiSubtitle("Acme", 2, 0)).toBe("Acme has as many emoji as it may (0).");
    expect(emojiRefusal(new ClientError("api error RESOURCE_EXHAUSTED: full details=state=emoji,limit=0"), "Acme", "x").text).toBe("Acme has as many emoji as it may (0).");
    expect(emojiSubtitle("Acme", 3, 2)).toBe("Acme has as many emoji as it may (2). Remove one to upload another.");
  });

  test("says what an emoji is, or at the limit that the server has as many as it may", () => {
    expect(emojiSubtitle("Acme", 12, 50)).toBe("Used as :name: in Acme’s channels and server DMs, and as reactions. A PNG, JPEG, WebP or GIF up to 8 MB, made to fit 128 pixels; an animated one plays.");
    expect(emojiSubtitle("Acme", 50, 50)).toBe("Acme has as many emoji as it may (50). Remove one to upload another.");
  });

  test("asks before removing", () => {
    expect(removeTitle("parrotwave")).toBe("Remove :parrotwave:?");
  });

  test("says a file's size", () => {
    expect(sizeWords(38 * 1024)).toBe("38 KB");
    expect(sizeWords(200)).toBe("1 KB");
    expect(sizeWords(6.8 * 1024 * 1024)).toBe("6.8 MB");
    expect(sizeWords(2 * 1024 * 1024)).toBe("2 MB");
    expect(sizeWords(12.4 * 1024 * 1024)).toBe("12 MB");
  });
});

describe("a refusal in the boards' words", () => {
  const refused = (text: string) => new ClientError(text);
  test("a name taken", () => {
    expect(emojiRefusal(refused("api error ALREADY_EXISTS: taken details=existing_id=x"), "Acme", "partyparrot")).toEqual({ on: "name", text: "Acme already has :partyparrot:. Choose another name." });
  });

  test("a name not allowed", () => {
    expect(emojiRefusal(refused("api error INVALID_ARGUMENT: bad details=field=name"), "Acme", "party-time")).toEqual({ on: "name", text: NAME_RULE });
  });

  test("a picture, by why", () => {
    expect(emojiRefusal(refused("api error INVALID_ARGUMENT: big details=field=picture,reason=too_large"), "Acme", "x").text).toBe(TOO_LARGE);
    expect(emojiRefusal(refused("api error INVALID_ARGUMENT: no details=field=picture,reason=unsupported"), "Acme", "x")).toEqual({ on: "picture", text: "Lumia can’t use this file. Choose a PNG, JPEG, WebP or GIF." });
    expect(emojiRefusal(refused("api error INVALID_ARGUMENT: no details=field=picture,reason=invalid"), "Acme", "x")).toEqual({ on: "picture", text: "This picture couldn’t be read." });
  });

  test("the limit, with the instance's number", () => {
    expect(emojiRefusal(refused("api error RESOURCE_EXHAUSTED: full details=state=emoji,limit=50"), "Acme", "x")).toEqual({ on: "section", text: "Acme has as many emoji as it may (50). Remove one to upload another." });
  });

  test("the rest over the section", () => {
    expect(emojiRefusal(refused("api error PERMISSION_DENIED: no"), "Acme", "x").on).toBe("section");
    expect(emojiRefusal(refused("api error NOT_FOUND: gone"), "Acme", "x").text).toBe("That emoji has been removed.");
    expect(emojiRefusal(new Error("offline"), "Acme", "x")).toEqual({ on: "section", text: "offline" });
  });
});

describe("the : list's answer", () => {
  const parrot = { id: "e1", name: "parrot" };
  const dance = { id: "e2", name: "dance" };
  const held = new Map([
    ["e1", parrot],
    ["e2", dance],
  ]);
  const answer = { server: "s1", query: "pa", held, list: [{ emojiId: "e1" }, { emojiId: "e2" }] };

  test("is listed for the server, prefix and emoji it was asked for, each as held now", () => {
    expect(emojiListed(answer, { server: "s1", query: "pa", held })).toEqual([parrot, dance]);
  });

  test("lists nothing while a lookup for what is now typed is on its way", () => {
    expect(emojiListed(answer, { server: "s1", query: "pax", held })).toEqual([]);
    expect(emojiListed(answer, { server: "s1", query: "p", held })).toEqual([]);
  });

  test("lists nothing in another server's conversation", () => {
    expect(emojiListed(answer, { server: "s2", query: "pa", held })).toEqual([]);
  });

  test("lists nothing once the server's emoji have changed, until it is asked again", () => {
    const renamed = new Map([...held, ["e1", { id: "e1", name: "bird" }]]);
    expect(emojiListed(answer, { server: "s1", query: "pa", held: renamed })).toEqual([]);
  });

  test("never offers one the server no longer has", () => {
    const removed = new Map([["e2", dance]]);
    expect(emojiListed({ ...answer, held: removed }, { server: "s1", query: "pa", held: removed })).toEqual([dance]);
  });

  test("nothing before any answer", () => {
    expect(emojiListed(null, { server: "s1", query: "pa", held })).toEqual([]);
  });
});

describe("a picture that could not be had", () => {
  test("is asked for again, later each time, and at least once a minute", () => {
    expect(pictureRetryIn(1)).toBe(2_000);
    expect(pictureRetryIn(2)).toBe(4_000);
    expect(pictureRetryIn(3)).toBe(8_000);
    expect(pictureRetryIn(20)).toBe(60_000);
  });
});

describe("Frequently used", () => {
  test("holds a server's own only where they are held, ahead of the quick six", () => {
    const g = globalThis as { localStorage?: unknown };
    const was = g.localStorage;
    const store = new Map<string, string>();
    g.localStorage = { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => store.set(k, v) };
    try {
      store.set("lumia:emoji-frequent", JSON.stringify({ "🚀": 3, [customKey("e1")]: 5, [customKey("gone")]: 4 }));
      const held = new Map([["e1", { id: "e1", name: "partyparrot" }]]);
      expect(frequentEmoji(held).slice(0, 3)).toEqual([{ id: "e1", name: "partyparrot" }, "🚀", "👍"]);
      // Elsewhere (a personal conversation, another server): Unicode's alone.
      expect(frequentEmoji().slice(0, 2)).toEqual(["🚀", "👍"]);
      expect(frequentEmoji().some((e) => typeof e === "string" && e.startsWith("custom:"))).toBe(false);
    } finally {
      g.localStorage = was;
    }
  });
});
