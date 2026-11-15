import { describe, expect, test } from "bun:test";
import { endedNotice } from "./ended";

describe("the sign-in card's notice", () => {
  test("a device removed says so, and that its private messages were erased", () => {
    expect(endedNotice(true)).toEqual({ words: "This browser was removed from your account, and its private messages were erased from it.", removed: true });
  });

  test("a session that ended otherwise says to sign in again", () => {
    expect(endedNotice(false)).toEqual({ words: "You were signed out of this browser. Sign in again to carry on.", removed: false });
  });
});
