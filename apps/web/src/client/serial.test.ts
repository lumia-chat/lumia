import { describe, expect, test } from "bun:test";
import { oneAtATime } from "./serial";

describe("calls one at a time", () => {
  test("a second call begins only once the first has ended, whatever their speeds", async () => {
    const queue = oneAtATime();
    const seen: string[] = [];
    const slow = queue(async () => {
      seen.push("first begins");
      await Bun.sleep(30);
      seen.push("first ends");
      return 1;
    });
    const fast = queue(async () => {
      seen.push("second begins");
      seen.push("second ends");
      return 2;
    });
    expect(await Promise.all([slow, fast])).toEqual([1, 2]);
    expect(seen).toEqual(["first begins", "first ends", "second begins", "second ends"]);
  });

  test("a call that fails does not stop the next", async () => {
    const queue = oneAtATime();
    const failed = queue(async () => {
      throw new Error("refused");
    });
    const next = queue(async () => "ran");
    await expect(failed).rejects.toThrow("refused");
    expect(await next).toBe("ran");
  });
});
