import { describe, expect, test } from "bun:test";
import { linkEnded, linkFailurePasses, linkOpen, linkPolls, linkWords, type LinkScreen } from "./link";

const SCREENS: LinkScreen[] = ["starting", "waiting", "compare", "linked", "declined", "expired", "failed"];

describe("when the page asks again", () => {
  test("only while the session waits on the other device", () => {
    expect(SCREENS.filter(linkPolls)).toEqual(["waiting", "compare"]);
  });

  test("leaving gives up a session still being opened or waited on", () => {
    expect(SCREENS.filter(linkOpen)).toEqual(["starting", "waiting", "compare"]);
  });

  test("Try again follows a session that ended without a link", () => {
    expect(SCREENS.filter(linkEnded)).toEqual(["declined", "expired", "failed"]);
  });
});

describe("a failure to ask", () => {
  test("out of reach or busy is asked again", () => {
    expect(linkFailurePasses("", "transport: error sending request")).toBe(true);
    expect(linkFailurePasses("UNAVAILABLE", "api error UNAVAILABLE: down")).toBe(true);
    expect(linkFailurePasses("RATE_LIMITED", "api error RATE_LIMITED: slow down")).toBe(true);
  });

  test("a refusal or a lost session ends it", () => {
    expect(linkFailurePasses("NOT_FOUND", "api error NOT_FOUND: no such bundle")).toBe(false);
    expect(linkFailurePasses("", "client state: no link in progress")).toBe(false);
  });
});

describe("the card's words", () => {
  test("waiting says where to look, for a browser", () => {
    expect(linkWords("waiting").text).toBe("Open Lumia on a device that has your private messages. It will ask about this browser.");
    expect(linkWords("starting")).toEqual(linkWords("waiting"));
  });

  test("compare says to check the number and approve there", () => {
    expect(linkWords("compare").text).toBe("Check that your other device shows the same number, then approve it there.");
  });

  test("declined and expired say why, with typographic apostrophes", () => {
    expect(linkWords("declined").text).toBe("It wasn’t approved on your other device.");
    expect(linkWords("expired").text).toBe("That took too long.");
  });

  test("a failure says its reason, and a session the core lost reads as run out", () => {
    expect(linkWords("failed", "api error UNAVAILABLE: down").text).toBe("api error UNAVAILABLE: down");
    expect(linkWords("failed", "client state: no link in progress").text).toBe("That took too long.");
    expect(linkWords("failed").text).toBe("Something went wrong.");
  });
});
