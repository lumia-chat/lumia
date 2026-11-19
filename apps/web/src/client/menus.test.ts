import { describe, expect, test } from "bun:test";
import { browsersMenu, firstName, menuKey, offeredWays, onRects, personOffers, placedAt, pressedOn } from "./menus";

/** An element as `closest` sees it: inside whatever `within` lists. */
function inside(...within: string[]) {
  return { closest: (selectors: string) => (selectors.split(", ").some((s) => within.includes(s)) ? {} : null) };
}

describe("the keys that ask for a menu", () => {
  test("Shift+F10 and the context menu key ask", () => {
    expect(menuKey({ key: "F10", shiftKey: true })).toBe(true);
    expect(menuKey({ key: "ContextMenu", shiftKey: false })).toBe(true);
  });
  test("F10 alone and other keys do not", () => {
    expect(menuKey({ key: "F10", shiftKey: false })).toBe(false);
    expect(menuKey({ key: "Enter", shiftKey: true })).toBe(false);
  });
});

describe("a menu at the pointer", () => {
  const size = { width: 220, height: 200 };
  const view = { width: 1000, height: 700 };
  test("has its corner at the pointer where there is room", () => {
    expect(placedAt({ x: 300, y: 200 }, size, view)).toEqual({ left: 300, top: 200 });
  });
  test("goes to the pointer's left at the right edge", () => {
    expect(placedAt({ x: 900, y: 200 }, size, view)).toEqual({ left: 680, top: 200 });
  });
  test("goes over the pointer at the bottom edge", () => {
    expect(placedAt({ x: 300, y: 650 }, size, view)).toEqual({ left: 300, top: 450 });
  });
  test("does both in the corner", () => {
    expect(placedAt({ x: 990, y: 690 }, size, view)).toEqual({ left: 770, top: 490 });
  });
  test("stays 8 inside a window too small to flip in", () => {
    expect(placedAt({ x: 150, y: 150 }, size, { width: 300, height: 260 })).toEqual({ left: 8, top: 8 });
  });
  test("stays 8 inside at the left and top edges", () => {
    expect(placedAt({ x: 0, y: 2 }, size, view)).toEqual({ left: 8, top: 8 });
  });
});

describe("a right-click on a message", () => {
  test("on its text brings the message's menu", () => {
    expect(browsersMenu(pressedOn(inside(), false))).toBe(false);
  });
  test("on a link leaves the browser's", () => {
    expect(pressedOn(inside("a[href]"), false)).toEqual({ link: true, media: false, field: false, selected: false });
    expect(browsersMenu(pressedOn(inside("a[href]"), false))).toBe(true);
  });
  test("on a picture, a video or an audio element leaves the browser's", () => {
    for (const el of ["img", "video", "audio"]) {
      expect(pressedOn(inside(el), false).media).toBe(true);
      expect(browsersMenu(pressedOn(inside(el), false))).toBe(true);
    }
  });
  test("in a field being typed in leaves the browser's", () => {
    for (const el of ["textarea", "input"]) {
      expect(pressedOn(inside(el), false).field).toBe(true);
      expect(browsersMenu(pressedOn(inside(el), false))).toBe(true);
    }
  });
  test("on selected text leaves the browser's, with its Copy", () => {
    expect(browsersMenu(pressedOn(inside(), true))).toBe(true);
  });
  test("a button is none of them", () => {
    expect(browsersMenu(pressedOn(inside("button"), false))).toBe(false);
  });
});

describe("a point on a selection", () => {
  const lines = [
    { left: 100, right: 300, top: 10, bottom: 30 },
    { left: 40, right: 180, top: 30, bottom: 50 },
  ];
  test("is on one of its lines", () => {
    expect(onRects(lines, 150, 20)).toBe(true);
    expect(onRects(lines, 60, 45)).toBe(true);
  });
  test("is not beside or under them", () => {
    expect(onRects(lines, 60, 20)).toBe(false);
    expect(onRects(lines, 250, 45)).toBe(false);
    expect(onRects(lines, 150, 80)).toBe(false);
  });
  test("is on nothing where nothing is selected", () => {
    expect(onRects([], 10, 10)).toBe(false);
  });
});

describe("what a person is offered", () => {
  const open = { canStartDms: true, dmsOff: false };
  test("the first word of the name is who is messaged", () => {
    expect(firstName("Ada Lovelace")).toBe("Ada");
    expect(firstName("  Ada  ")).toBe("Ada");
    expect(firstName("ada")).toBe("ada");
  });
  test("in a server, a server DM and a personal conversation beside it", () => {
    const offers = personOffers({ me: false, name: "Ada Lovelace", server: open });
    expect(offers).toEqual({ action: { way: "serverDm", label: "Message Ada" }, secondary: { way: "private", label: "Message privately" } });
    expect(offeredWays(offers).map((o) => o.label)).toEqual(["Message Ada", "Message privately"]);
  });
  test("someone who turned server DMs off is offered the personal one, and the card says why", () => {
    const offers = personOffers({ me: false, name: "Ada Lovelace", server: { canStartDms: true, dmsOff: true } });
    expect(offers).toEqual({ secondary: { way: "private", label: "Message privately" }, note: "Doesn’t take messages here" });
    expect(offeredWays(offers).map((o) => o.label)).toEqual(["Message privately"]);
  });
  test("an account that may not start server DMs is offered the personal one alone", () => {
    const offers = personOffers({ me: false, name: "Ada Lovelace", server: { canStartDms: false, dmsOff: false } });
    expect(offers.action).toBeUndefined();
    expect(offers.note).toBeUndefined();
    expect(offeredWays(offers)).toEqual([{ way: "private", label: "Message privately" }]);
  });
  test("at Home, the personal conversation by their name", () => {
    const offers = personOffers({ me: false, name: "Ada Lovelace" });
    expect(offers).toEqual({ action: { way: "private", label: "Message Ada" } });
    expect(offeredWays(offers)).toEqual([{ way: "private", label: "Message Ada" }]);
  });
  test("this account itself is offered nothing, anywhere", () => {
    expect(personOffers({ me: true, name: "Ryan", server: open })).toEqual({});
    expect(personOffers({ me: true, name: "Ryan" })).toEqual({});
    expect(offeredWays(personOffers({ me: true, name: "Ryan" }))).toEqual([]);
  });
});
