import { describe, expect, test } from "bun:test";
import type { DeviceSummary } from "./bridge";
import { addedLine, afterPhrase, deviceName, keepWords } from "./restore";

const NOV_12 = new Date(2026, 10, 12, 12).getTime() * 1000;

const device = (over: Partial<DeviceSummary> = {}): DeviceSummary => ({
  deviceId: "d1",
  kind: "native",
  label: "MacBook Pro",
  webOrigin: "",
  addedAtMicros: NOV_12,
  thisDevice: false,
  revoked: false,
  ...over,
});

describe("what follows the phrase", () => {
  test("no devices to stop restores at once", () => {
    expect(afterPhrase([])).toBe("restore");
    expect(afterPhrase([device({ revoked: true })])).toBe("restore");
  });

  test("any device it would stop asks which to keep", () => {
    expect(afterPhrase([device(), device({ deviceId: "d2", kind: "web" })])).toBe("keep");
  });
});

describe("the button", () => {
  test("says what restoring does by how many are ticked", () => {
    expect(keepWords(0)).toBe("Restore and stop them all");
    expect(keepWords(1)).toBe("Restore and keep 1 device");
    expect(keepWords(3)).toBe("Restore and keep 3 devices");
  });
});

describe("a device's row", () => {
  test("is named by its label, or its kind with none", () => {
    expect(deviceName(device())).toBe("MacBook Pro");
    expect(deviceName(device({ label: " " }))).toBe("Computer");
    expect(deviceName(device({ label: "", kind: "web" }))).toBe("Browser");
    expect(deviceName(device({ label: "", kind: "other" }))).toBe("Device");
  });

  test("says when it was added, and where for a web one", () => {
    expect(addedLine(device(), "en-US")).toBe("Added Nov 12");
    expect(addedLine(device({ kind: "web", webOrigin: "https://chat.acme.dev" }), "en-US")).toBe("On chat.acme.dev · added Nov 12");
    expect(addedLine(device({ kind: "web", webOrigin: "chat.acme.dev", addedAtMicros: 0 }), "en-US")).toBe("On chat.acme.dev");
    expect(addedLine(device({ addedAtMicros: 0 }), "en-US")).toBe("");
  });
});
