import { describe, expect, test } from "bun:test";
import type { EmbedSummary } from "./bridge";
import { EmbedLookup, allowedSrc, frameBox, frameSrc, frameTitle, originOf, pressLabel, pressOf, pressable, reportedHeight } from "./embeds";

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

describe("reportedHeight", () => {
  const frame = { name: "the embed's window" };
  const other = { name: "another window" };
  const xMessage = { "twttr.embed": { id: "1", jsonrpc: "2.0", method: "twttr.private.resize", params: [{ width: 550, height: 612.5 }] } };

  test("X: twttr.private.resize, as an object, as JSON, or as JSON after the filter's name", () => {
    expect(reportedHeight(x, frame, "https://platform.twitter.com", { source: frame, origin: "https://platform.twitter.com", data: xMessage })).toBe(613);
    expect(reportedHeight(x, frame, "https://platform.twitter.com", { source: frame, origin: "https://platform.twitter.com", data: JSON.stringify(xMessage) })).toBe(613);
    expect(reportedHeight(x, frame, "https://platform.twitter.com", { source: frame, origin: "https://platform.twitter.com", data: "twttr.embed" + JSON.stringify(xMessage["twttr.embed"]) })).toBe(613);
    expect(reportedHeight(x, frame, "https://platform.twitter.com", { source: frame, origin: "https://platform.twitter.com", data: { "twttr.embed": { method: "twttr.private.rendered", params: [{ height: 400 }] } } })).toBeUndefined();
  });
  test("Bluesky: {height, id}", () => {
    expect(reportedHeight(bluesky, frame, "https://embed.bsky.app", { source: frame, origin: "https://embed.bsky.app", data: { height: 420, id: null } })).toBe(420);
    expect(reportedHeight(bluesky, frame, "https://embed.bsky.app", { source: frame, origin: "https://embed.bsky.app", data: { height: "lots" } })).toBeUndefined();
  });
  test("Reddit: resize.embed in a JSON string", () => {
    expect(reportedHeight(reddit, frame, "https://embed.reddit.com", { source: frame, origin: "https://embed.reddit.com", data: JSON.stringify({ type: "resize.embed", data: 377 }) })).toBe(377);
    expect(reportedHeight(reddit, frame, "https://embed.reddit.com", { source: frame, origin: "https://embed.reddit.com", data: { type: "resize.embed", data: 377 } })).toBeUndefined();
    expect(reportedHeight(reddit, frame, "https://embed.reddit.com", { source: frame, origin: "https://embed.reddit.com", data: "{not json" })).toBeUndefined();
  });
  test("only from the embed's own window and its origin", () => {
    const data = { height: 420 };
    expect(reportedHeight(bluesky, frame, "https://embed.bsky.app", { source: other, origin: "https://embed.bsky.app", data })).toBeUndefined();
    expect(reportedHeight(bluesky, frame, "https://embed.bsky.app", { source: frame, origin: "https://evil.example", data })).toBeUndefined();
    expect(reportedHeight(bluesky, null, "https://embed.bsky.app", { source: null, origin: "https://embed.bsky.app", data })).toBeUndefined();
    expect(reportedHeight(bluesky, frame, "", { source: frame, origin: "", data })).toBeUndefined();
  });
  test("one site's format is not read for another", () => {
    expect(reportedHeight(x, frame, "https://platform.twitter.com", { source: frame, origin: "https://platform.twitter.com", data: { height: 420 } })).toBeUndefined();
  });
  test("an embed that does not grow is not listened to", () => {
    expect(reportedHeight(instagram, frame, "https://www.instagram.com", { source: frame, origin: "https://www.instagram.com", data: { height: 420 } })).toBeUndefined();
  });
  test("held within bounds", () => {
    const at = (height: number) => reportedHeight(bluesky, frame, "https://embed.bsky.app", { source: frame, origin: "https://embed.bsky.app", data: { height } });
    expect(at(1e9)).toBe(5000);
    expect(at(3)).toBe(50);
    expect(at(0)).toBeUndefined();
    expect(at(-10)).toBeUndefined();
    expect(at(Number.NaN)).toBeUndefined();
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
