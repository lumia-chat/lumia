import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { OneAtATime, stops, stopsOffScreen, type Playing, type PlayingKind } from "./playback";

function world() {
  const stopped: string[] = [];
  const one = new OneAtATime();
  const make = (id: string, embed: boolean, kind: Playing["kind"]): Playing => ({ id, embed, kind, stop: () => stopped.push(id) });
  return { one, stopped, make };
}

// The cases every client agrees on (lumia_core::playback).
const cases = JSON.parse(readFileSync(resolve(import.meta.dir, "../../../../proto/vectors/playback/cases.json"), "utf8")) as {
  starts: { starting: PlayingKind; other: PlayingKind; stops: boolean; why: string }[];
  off_screen: { playing: PlayingKind; stops: boolean; why: string }[];
};

describe("the shared cases", () => {
  test("what something starting stops", () => {
    expect(cases.starts.length).toBeGreaterThan(0);
    for (const c of cases.starts) expect({ why: c.why, stops: stops(c.starting, c.other) }).toEqual({ why: c.why, stops: c.stops });
  });

  test("what stops when its row scrolls away", () => {
    expect(cases.off_screen.length).toBeGreaterThan(0);
    for (const c of cases.off_screen) expect({ why: c.why, stops: stopsOffScreen(c.playing) }).toEqual({ why: c.why, stops: c.stops });
  });

  test("carried out as each starts", () => {
    for (const c of cases.starts) {
      const { one, stopped, make } = world();
      one.start(make("other", c.other.embed, c.other.kind));
      expect({ why: c.why, stopped: one.start(make("starting", c.starting.embed, c.starting.kind)) }).toEqual({ why: c.why, stopped: c.stops ? ["other"] : [] });
      expect(stopped).toEqual(c.stops ? ["other"] : []);
    }
  });
});

describe("one at a time", () => {
  test("an attached video started pauses the audio playing", () => {
    const { one, stopped, make } = world();
    one.start(make("audio", false, "audio"));
    expect(one.start(make("video", false, "video"))).toEqual(["audio"]);
    expect(stopped).toEqual(["audio"]);
    expect(one.ids()).toEqual(["video"]);
  });

  test("an attachment started puts back a playing embed, but leaves a post being read", () => {
    const { one, stopped, make } = world();
    one.start(make("post", true, "post"));
    one.start(make("spotify", true, "audio"));
    // The second embed put the post back: one embed is shown at a time.
    expect(stopped).toEqual(["post"]);
    one.end("spotify");
    one.start(make("post", true, "post"));
    one.start(make("video", false, "video"));
    expect(one.ids().sort()).toEqual(["post", "video"]);
    one.start(make("youtube", true, "video"));
    expect(stopped).toEqual(["post", "post", "video"]);
    expect(one.start(make("audio", false, "audio"))).toEqual(["youtube"]);
  });

  test("a card pressed puts back any other embed and pauses attachments", () => {
    const { one, stopped, make } = world();
    one.start(make("a", false, "audio"));
    one.start(make("v", false, "video"));
    one.start(make("x", true, "post"));
    expect(stopped).toEqual(["a", "v"]);
    one.start(make("bsky", true, "post"));
    expect(stopped).toEqual(["a", "v", "x"]);
    expect(one.ids()).toEqual(["bsky"]);
  });

  test("starting again what is playing stops nothing", () => {
    const { one, stopped, make } = world();
    const v = make("v", false, "video");
    one.start(v);
    expect(one.start(v)).toEqual([]);
    expect(stopped).toEqual([]);
    expect(one.has("v")).toBe(true);
  });

  test("what ended or went is not stopped again", () => {
    const { one, stopped, make } = world();
    one.start(make("v", false, "video"));
    one.end("v");
    one.start(make("w", false, "video"));
    expect(stopped).toEqual([]);
    expect(one.has("v")).toBe(false);
  });
});
