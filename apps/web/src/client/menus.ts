// A menu where it is asked for (issue #25): a message's and a person's,
// opened by a right-click, Shift+F10 or the context menu key. What needs no
// page is here: which keys ask, where a menu at the pointer goes so that it
// stays in the window, where a right-click on a message leaves the browser
// its own menu, and what a person is offered, which their profile card and
// their menu both draw from.

/** Where the pointer asked for a menu, in the window. */
export interface MenuPoint {
  x: number;
  y: number;
}

/** Shift+F10 or the context menu key: the keyboard's way to a menu. */
export function menuKey(e: { key: string; shiftKey: boolean }): boolean {
  return (e.key === "F10" && e.shiftKey) || e.key === "ContextMenu";
}

/** How near the window's edge a menu comes. */
const EDGE = 8;

/**
 * Where a menu of `size` asked for at `at` goes in a window of `view`: its
 * corner at the pointer, on the pointer's other side where it would pass the
 * right or the bottom edge, and never nearer an edge than 8.
 */
export function placedAt(at: MenuPoint, size: { width: number; height: number }, view: { width: number; height: number }): { left: number; top: number } {
  const left = at.x + size.width > view.width - EDGE ? at.x - size.width : at.x;
  const top = at.y + size.height > view.height - EDGE ? at.y - size.height : at.y;
  return {
    left: Math.max(EDGE, Math.min(left, view.width - size.width - EDGE)),
    top: Math.max(EDGE, Math.min(top, view.height - size.height - EDGE)),
  };
}

/** What a right-click on a message landed on, as far as whose menu comes up. */
export interface Pressed {
  /** Inside a link: "Open in new tab" and "Copy link" are the browser's. */
  link: boolean;
  /** A picture, a video or an audio element: so is "Save image". */
  media: boolean;
  /** Inside a field being typed in. */
  field: boolean;
  /** On text that was selected before the press: so is Copy. */
  selected: boolean;
}

/** What was pressed, read off the element under the pointer; `selected` is whether the press was on selected text. */
export function pressedOn(target: { closest(selectors: string): unknown }, selected: boolean): Pressed {
  return { link: !!target.closest("a[href]"), media: !!target.closest("img, video, audio"), field: !!target.closest("input, textarea, select"), selected };
}

/** Whether the browser's own menu stays, and the message's does not come up. */
export function browsersMenu(pressed: Pressed): boolean {
  return pressed.link || pressed.media || pressed.field || pressed.selected;
}

/** Whether a point is on one of a selection's rectangles. */
export function onRects(rects: Iterable<{ left: number; right: number; top: number; bottom: number }>, x: number, y: number): boolean {
  for (const r of rects) if (x >= r.left && x <= r.right && y >= r.top && y <= r.bottom) return true;
  return false;
}

/** A way to write to someone: a server DM in the server on screen, or a personal conversation. */
export type PersonWay = "serverDm" | "private";

export interface PersonOffer {
  way: PersonWay;
  label: string;
}

/** What a person is offered where they are met: the card's two buttons and its note, the menu's rows. */
export interface PersonOffers {
  /** The first way: the card's button. */
  action?: PersonOffer;
  /** A second beside it: "Message privately", in a server. */
  secondary?: PersonOffer;
  /** Why the server's way is not there. */
  note?: string;
}

/** The first word of a name, which "Message Ada" says. */
export function firstName(name: string): string {
  return name.trim().split(/\s+/)[0] || name;
}

/**
 * What someone is offered. In a server, a server DM first (architecture
 * 6.4), where they take them and this account may start them, and a personal
 * conversation beside it; at Home the personal one alone. This account
 * itself is offered nothing.
 */
export function personOffers(who: { me: boolean; name: string; server?: { canStartDms: boolean; dmsOff: boolean } }): PersonOffers {
  if (who.me) return {};
  const message = `Message ${firstName(who.name)}`;
  if (!who.server) return { action: { way: "private", label: message } };
  const secondary: PersonOffer = { way: "private", label: "Message privately" };
  if (who.server.dmsOff) return { secondary, note: "Doesn’t take messages here" };
  return { action: who.server.canStartDms ? { way: "serverDm", label: message } : undefined, secondary };
}

/** The ways offered, in the order a menu lists them. */
export function offeredWays(offers: PersonOffers): PersonOffer[] {
  return [offers.action, offers.secondary].filter((o): o is PersonOffer => !!o);
}
