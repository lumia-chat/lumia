import { describe, expect, test } from "bun:test";
import { OneAtATime, stops, type Playing } from "./playback";

function world() {
  const stopped: string[] = [];
  const one = new OneAtATime();
  const make = (id: string, embed: boolean, kind: Playing["kind"]): Playing => ({ id, embed, kind, stop: () => stopped.push(id) });
  return { one, stopped, make };
}

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

  test("the rule itself", () => {
    const { make } = world();
    expect(stops(make("e", true, "post"), make("p", true, "post"))).toBe(true);
    expect(stops(make("a", false, "audio"), make("p", true, "post"))).toBe(false);
    expect(stops(make("a", false, "audio"), make("s", true, "audio"))).toBe(true);
    expect(stops(make("a", false, "audio"), make("a", false, "audio"))).toBe(false);
  });
});
