import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import type { EmbedSummary } from "./bridge";
import { BLUESKY_UNREACHED, EmbedLookup, allowedSrc, frameBox, frameSrc, frameTitle, originOf, playingOf, postedText, pressLabel, pressOf, pressable, pressed, reportedHeights } from "./embeds";
import { stopsOffScreen } from "./playback";

const base: EmbedSummary = {
  provider: "youtube",
  providerName: "YouTube",
  kind: "player",
  url: "https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ?autoplay=1",
  blueskyHandle: "",
  blueskyRkey: "",
  needsParent: false,
  aspectWidth: 16,
  aspectHeight: 9,
  fixedHeight: 0,
  resizes: false,
  maxWidth: 0,
  label: "Play video from YouTube",
};
const youtube = base;
const short: EmbedSummary = { ...base, url: "https://www.youtube-nocookie.com/embed/abcdefghijk?autoplay=1", aspectWidth: 9, aspectHeight: 16 };
const twitch: EmbedSummary = { ...base, provider: "twitch", providerName: "Twitch", url: "https://player.twitch.tv/?video=123456789", needsParent: true, label: "Play video from Twitch" };
const spotify: EmbedSummary = { ...base, provider: "spotify", providerName: "Spotify", kind: "audio", url: "https://open.spotify.com/embed/track/4uLU6hMCjMI75M1A2tKUQC", aspectWidth: 0, aspectHeight: 0, fixedHeight: 152, label: "Play audio from Spotify" };
const x: EmbedSummary = { ...base, provider: "x", providerName: "X", kind: "post", url: "https://platform.twitter.com/embed/Tweet.html?id=20&dnt=true", aspectWidth: 0, aspectHeight: 0, fixedHeight: 250, resizes: true, maxWidth: 550, label: "Show post from X" };
const bluesky: EmbedSummary = { ...x, provider: "bluesky", providerName: "Bluesky", url: "https://embed.bsky.app/embed/did:plc:z72i7hdynmk6r22z27h6tvur/app.bsky.feed.post/3k2la3rsg5w2y", label: "Show post from Bluesky" };
const blueskyByHandle: EmbedSummary = { ...bluesky, url: "", blueskyHandle: "bsky.app", blueskyRkey: "3k2la3rsg5w2y" };
const reddit: EmbedSummary = { ...x, provider: "reddit", providerName: "Reddit", url: "https://embed.reddit.com/r/rust/comments/abc123/", label: "Show post from Reddit" };
const instagram: EmbedSummary = { ...x, provider: "instagram", providerName: "Instagram", url: "https://www.instagram.com/p/C0abcdEFGhi/embed", fixedHeight: 640, resizes: false, label: "Show post from Instagram" };

describe("which card has a press", () => {
  test("an embed with an address, or a Bluesky handle to resolve", () => {
    expect(pressable(youtube)).toBe(true);
    expect(pressable(blueskyByHandle)).toBe(true);
    expect(pressable(null)).toBe(false);
    expect(pressable(undefined)).toBe(false);
    expect(pressable({ ...bluesky, url: "", blueskyHandle: "", blueskyRkey: "" })).toBe(false);
    expect(pressable({ ...blueskyByHandle, blueskyRkey: "" })).toBe(false);
  });
  test("a player or audio plays on its picture; a post is shown", () => {
    expect(pressOf(youtube)).toBe("play");
    expect(pressOf(spotify)).toBe("play");
    expect(pressOf(x)).toBe("show");
  });
});

describe("what plays", () => {
  test("a site's player is a video to the rules; audio and a post are themselves", () => {
    expect(playingOf(youtube)).toEqual({ embed: true, kind: "video" });
    expect(playingOf(spotify)).toEqual({ embed: true, kind: "audio" });
    expect(playingOf(x)).toEqual({ embed: true, kind: "post" });
    // So scrolled away, only a player goes back to its card.
    expect([youtube, short, twitch, spotify, x, bluesky, reddit, instagram].filter((e) => stopsOffScreen(playingOf(e)))).toEqual([youtube, short, twitch]);
  });
});

describe("words", () => {
  test("the press is the core's label and the card's title after a comma", () => {
    expect(pressLabel(youtube, "Never gonna give you up")).toBe("Play video from YouTube, Never gonna give you up");
    expect(pressLabel(x, "  ")).toBe("Show post from X");
  });
  test("the frame is titled by the card, or the site", () => {
    expect(frameTitle(x, "A post")).toBe("A post");
    expect(frameTitle(x, "")).toBe("X");
  });
});

describe("the address loaded", () => {
  test("Twitch names the page's host as its parent", () => {
    expect(frameSrc(twitch, twitch.url, "lumia.example")).toBe("https://player.twitch.tv/?video=123456789&parent=lumia.example");
    expect(frameSrc({ ...twitch, url: "https://clips.twitch.tv/embed" }, "https://clips.twitch.tv/embed", "localhost")).toBe("https://clips.twitch.tv/embed?parent=localhost");
  });
  test("no other site is changed", () => {
    expect(frameSrc(youtube, youtube.url, "lumia.example")).toBe(youtube.url);
  });
  test("only an https address of the embeds' origins is loaded", () => {
    const origins = ["https://www.youtube-nocookie.com", "https://embed.bsky.app"];
    expect(allowedSrc(youtube.url, origins)).toBe(true);
    expect(allowedSrc("https://evil.example/embed", origins)).toBe(false);
    expect(allowedSrc("https://www.youtube-nocookie.com.evil.example/x", origins)).toBe(false);
    expect(allowedSrc("javascript:alert(1)", origins)).toBe(false);
    expect(allowedSrc("not a url", origins)).toBe(false);
    expect(originOf(bluesky.url)).toBe("https://embed.bsky.app");
  });
});

describe("frameBox", () => {
  test("a player is the column's width at its shape, never over 400 tall", () => {
    expect(frameBox(youtube)).toEqual({ width: "100%", "max-width": "711px", "aspect-ratio": "16 / 9" });
    expect(frameBox(short)).toEqual({ width: "100%", "max-width": "225px", "aspect-ratio": "9 / 16" });
  });
  test("audio is the column's width at the site's height", () => {
    expect(frameBox(spotify)).toEqual({ width: "100%", "max-width": "100%", height: "152px" });
  });
  test("a post is at most 550 wide, at its start height until it reports its own", () => {
    expect(frameBox(x)).toEqual({ width: "100%", "max-width": "550px", height: "250px" });
    expect(frameBox(x, 731)).toEqual({ width: "100%", "max-width": "550px", height: "731px" });
  });
  test("a post that does not grow keeps its height whatever is reported", () => {
    expect(frameBox(instagram, 900)).toEqual({ width: "100%", "max-width": "550px", height: "640px" });
  });
});

describe("postedText", () => {
  const frame = { name: "the embed's window" };
  const other = { name: "another window" };
  const xMessage = { "twttr.embed": { id: "1", jsonrpc: "2.0", method: "twttr.private.resize", params: [{ width: 550, height: 612.5 }] } };

  test("only from the embed's own window and its origin", () => {
    const data = { height: 420 };
    expect(postedText(bluesky, frame, "https://embed.bsky.app", { source: frame, origin: "https://embed.bsky.app", data })).toBe('{"height":420}');
    expect(postedText(bluesky, frame, "https://embed.bsky.app", { source: other, origin: "https://embed.bsky.app", data })).toBeUndefined();
    expect(postedText(bluesky, frame, "https://embed.bsky.app", { source: frame, origin: "https://evil.example", data })).toBeUndefined();
    expect(postedText(bluesky, null, "https://embed.bsky.app", { source: null, origin: "https://embed.bsky.app", data })).toBeUndefined();
    expect(postedText(bluesky, frame, "", { source: frame, origin: "", data })).toBeUndefined();
  });
  test("an embed that does not grow is not listened to", () => {
    expect(postedText(instagram, frame, "https://www.instagram.com", { source: frame, origin: "https://www.instagram.com", data: { height: 420 } })).toBeUndefined();
  });
  test("a string as it was posted, an object as its JSON", () => {
    const xText = "twttr.embed" + JSON.stringify(xMessage["twttr.embed"]);
    expect(postedText(x, frame, "https://platform.twitter.com", { source: frame, origin: "https://platform.twitter.com", data: xText })).toBe(xText);
    expect(postedText(x, frame, "https://platform.twitter.com", { source: frame, origin: "https://platform.twitter.com", data: xMessage })).toBe(JSON.stringify(xMessage));
    // Not read here: text that is not JSON is the core's to refuse.
    expect(postedText(reddit, frame, "https://embed.reddit.com", { source: frame, origin: "https://embed.reddit.com", data: "{not json" })).toBe("{not json");
    expect(postedText(bluesky, frame, "https://embed.bsky.app", { source: frame, origin: "https://embed.bsky.app", data: [420] })).toBe("[420]");
  });
  test("what is neither, or has no JSON, is refused", () => {
    const at = (data: unknown) => postedText(bluesky, frame, "https://embed.bsky.app", { source: frame, origin: "https://embed.bsky.app", data });
    const itself: Record<string, unknown> = { height: 420 };
    itself.self = itself;
    expect(at(itself)).toBeUndefined();
    expect(at({ height: 420n })).toBeUndefined();
    expect(at({ toJSON: () => undefined })).toBeUndefined();
    expect(at(undefined)).toBeUndefined();
    expect(at(null)).toBeUndefined();
    expect(at(420)).toBeUndefined();
    expect(at(true)).toBeUndefined();
  });
});

describe("reportedHeights", () => {
  const frame = { name: "the embed's window" };
  const origin = "https://embed.bsky.app";
  const from = (data: unknown, source: unknown = frame, at = origin) => ({ source, origin: at, data });
  // The core's answers, given when the test says so.
  function core() {
    const asked: { provider: string; message: string; answer: (height: number | null) => void; fail: (err: Error) => void }[] = [];
    const read = (provider: string, message: string) => new Promise<number | null>((answer, fail) => asked.push({ provider, message, answer, fail }));
    return { asked, read };
  }
  const settled = () => new Promise((resolve) => setTimeout(resolve, 0));

  test("the core reads what the embed's own window posted, and its height is drawn", async () => {
    const { asked, read } = core();
    const drawn: number[] = [];
    const posted = reportedHeights(bluesky, origin, read, (h) => drawn.push(h));
    posted(frame, from({ height: 412.2, id: "abc" }));
    expect(asked.map((a) => [a.provider, a.message])).toEqual([["bluesky", '{"height":412.2,"id":"abc"}']]);
    asked[0]?.answer(413);
    await settled();
    expect(drawn).toEqual([413]);
  });

  test("nothing is asked for another window's message, another origin's, or one with no JSON", () => {
    const { asked, read } = core();
    const posted = reportedHeights(bluesky, origin, read, () => {});
    posted(frame, from({ height: 420 }, { name: "another window" }));
    posted(frame, from({ height: 420 }, frame, "https://evil.example"));
    posted(null, from({ height: 420 }, null));
    posted(frame, from(420));
    expect(asked).toEqual([]);
  });

  test("a message that is no height draws nothing, and does not take a height back", async () => {
    const { asked, read } = core();
    const drawn: number[] = [];
    const posted = reportedHeights(bluesky, origin, read, (h) => drawn.push(h));
    posted(frame, from({ height: 700 }));
    posted(frame, from({ ready: true }));
    asked[1]?.answer(null);
    asked[0]?.answer(700);
    await settled();
    expect(drawn).toEqual([700]);
  });

  test("an answer to an earlier message never replaces a later one's", async () => {
    const { asked, read } = core();
    const drawn: number[] = [];
    const posted = reportedHeights(bluesky, origin, read, (h) => drawn.push(h));
    posted(frame, from({ height: 300 }));
    posted(frame, from({ height: 900 }));
    asked[1]?.answer(900);
    asked[0]?.answer(300);
    await settled();
    expect(drawn).toEqual([900]);
  });

  test("the core not answering is said, and the next message is still read", async () => {
    const { asked, read } = core();
    const drawn: number[] = [];
    const posted = reportedHeights(bluesky, origin, read, (h) => drawn.push(h));
    const warn = console.warn;
    const warned: unknown[] = [];
    console.warn = (...args: unknown[]) => warned.push(args);
    try {
      posted(frame, from({ height: 300 }));
      asked[0]?.fail(new Error("the worker went"));
      await settled();
    } finally {
      console.warn = warn;
    }
    expect(warned).toHaveLength(1);
    posted(frame, from({ height: 500 }));
    asked[1]?.answer(500);
    await settled();
    expect(drawn).toEqual([500]);
  });

  // What height a message says is the core's to read (lumia_core::embeds::reported_height), in
  // every form the shared vectors hold. Here: the page hands it each of those messages as the
  // vectors write them, whether the embed posted the text or the object it is the JSON of.
  test("the shared vectors' messages reach the core as they are written", () => {
    const { vectors } = JSON.parse(readFileSync(resolve(import.meta.dir, "../../../../proto/vectors/embeds/heights.json"), "utf8")) as { vectors: { provider: string; message: string; expect: number | null }[] };
    expect(vectors.length).toBeGreaterThan(0);
    const sites: Record<string, EmbedSummary> = { x, bluesky, reddit };
    let objects = 0;
    for (const v of vectors) {
      const e = sites[v.provider] ?? { ...bluesky, provider: v.provider as EmbedSummary["provider"] };
      const at = originOf(e.url);
      const { asked, read } = core();
      const posted = reportedHeights(e, at, read, () => {});
      posted(frame, from(v.message, frame, at));
      expect(asked.map((a) => [a.provider, a.message])).toEqual([[v.provider, v.message]]);
      // A browser hands over an object where the embed posted one: the same JSON value reaches the core.
      let value: unknown;
      try {
        value = JSON.parse(v.message);
      } catch {
        continue;
      }
      if (!value || typeof value !== "object") continue;
      posted(frame, from(value, frame, at));
      // (A number JSON cannot write, as `1e400` is once read, goes as null: no height either.)
      expect(JSON.parse(asked[1]?.message ?? "null")).toEqual(JSON.parse(JSON.stringify(value)));
      objects++;
    }
    expect(objects).toBeGreaterThan(0);
  });
});

describe("EmbedLookup", () => {
  test("asks the core once a link, and again after a failure", async () => {
    let asked = 0;
    let fail = true;
    const lookup = new EmbedLookup(async (url) => {
      asked++;
      if (fail) throw new Error("not ready");
      return url.includes("youtube") ? youtube : null;
    });
    const warn = console.warn;
    console.warn = () => {};
    try {
      expect(await lookup.of("https://youtube.com/watch?v=dQw4w9WgXcQ")).toBeNull();
    } finally {
      console.warn = warn;
    }
    fail = false;
    expect(await lookup.of("https://youtube.com/watch?v=dQw4w9WgXcQ")).toBe(youtube);
    expect(await lookup.of("https://youtube.com/watch?v=dQw4w9WgXcQ")).toBe(youtube);
    expect(await lookup.of("https://example.com")).toBeNull();
    expect(asked).toBe(3);
  });
});

describe("what a card's press comes to", () => {
  const origins = ["https://www.youtube-nocookie.com", "https://embed.bsky.app"];
  const byHandle: EmbedSummary = { ...base, provider: "bluesky", providerName: "Bluesky", kind: "post", url: "", blueskyHandle: "ada.example", blueskyRkey: "3k2la3rsg5w2y" };

  test("an address of an embed origin is loaded", () => {
    expect(pressed(base, base.url, origins, "lumia.example")).toEqual({ src: base.url });
    const post = "https://embed.bsky.app/embed/did:plc:z72i7hdynmk6r22z27h6tvur/app.bsky.feed.post/3k2la3rsg5w2y";
    expect(pressed(byHandle, post, origins, "lumia.example")).toEqual({ src: post });
  });

  test("a handle Bluesky has no account for says so, and the card opens its link from then on", () => {
    expect(pressed(byHandle, "", origins, "lumia.example")).toEqual({ note: "Bluesky couldn’t find @ada.example. The link opens it on Bluesky.", again: false });
  });

  test("a lookup Bluesky did not answer is not word about the post: the next press asks again", () => {
    expect(pressed(byHandle, undefined, origins, "lumia.example")).toEqual({ note: BLUESKY_UNREACHED, again: true });
    expect(BLUESKY_UNREACHED).toBe("Couldn’t reach Bluesky. Try again.");
  });

  test("an address of no embed origin is never loaded", () => {
    expect(pressed(base, "https://evil.example/embed/x", origins, "lumia.example")).toEqual({ note: "Couldn’t show this here. The link opens it on YouTube.", again: false });
    expect(pressed(base, base.url, [], "lumia.example")).toEqual({ note: "Couldn’t show this here. The link opens it on YouTube.", again: false });
  });
});
