// The picture viewer's zoom (specs/media.md 5): how far a picture opened
// from a message zooms, by what steps, about which point, and how far it
// may be moved. The rules are the core's (`lumia_core::viewer`), written
// here as well because every wheel event and every move of a drag needs
// its answer at once and the bridge answers later; both are tested against
// the cases every client agrees on (`proto/vectors/viewer/cases.json`).
//
// A picture opens fitted: shrunk into its room, never grown past its own
// size, a picture pixel to a CSS pixel. A view's `scale` is in times the
// fitted size, and its `x` and `y` are how far the picture's centre is
// from the room's; a point a zoom is about is given the same way.

/** How a picture is zoomed and moved. */
export interface View {
  /** In times the fitted size: 1 is fitted. */
  scale: number;
  /** The picture's centre from the room's, in pixels. */
  x: number;
  y: number;
}

/** Fitted and centred, as a picture opens. */
export const VIEW_FIT: View = { scale: 1, x: 0, y: 0 };

/** One step of the zoom buttons and of the plus and minus keys. */
export const ZOOM_STEP = 1.5;
/** The furthest zoom, in times the picture's own size. */
export const OWN_SIZES = 4;
/** How far from fitted a picture's own size must be for a press to go there; under it, to twice the fitted size. */
const DETAIL_FROM = 1.5;
/** How fast a wheel zooms, a pixel of scrolling: a mouse's notch of 100 is about 1.28 times. */
const WHEEL_RATE = 0.0025;
/** The same for a trackpad's pinch, a wheel with Control held, whose deltas are a few pixels each. */
const PINCH_RATE = 0.01;
/** The most one wheel event zooms, as an exponent: about 1.82 times. */
const WHEEL_MOST = 0.6;

/** A picture fitted into its room. */
export interface Frame {
  /** The fitted picture. */
  width: number;
  height: number;
  /** The room it zooms in, which cuts off what is past it. */
  roomWidth: number;
  roomHeight: number;
  /** The scale of the picture's own size: 1 for a picture that fits as it is. */
  own: number;
}

/** A picture of `ownWidth` by `ownHeight` pixels fitted into a room, `margin` kept clear on each side of it while it is fitted: shrunk to fit, never grown. */
export function viewFrame(ownWidth: number, ownHeight: number, roomWidth: number, roomHeight: number, margin = 0): Frame {
  const w = Math.max(1, ownWidth);
  const h = Math.max(1, ownHeight);
  const rw = Math.max(1, roomWidth);
  const rh = Math.max(1, roomHeight);
  const within = (room: number) => Math.max(1, room - 2 * margin);
  const k = Math.min(1, within(rw) / w, within(rh) / h);
  return { width: w * k, height: h * k, roomWidth: rw, roomHeight: rh, own: 1 / k };
}

/** The furthest scale: four times the picture's own size. */
export function maxScale(frame: Frame): number {
  return frame.own * OWN_SIZES;
}

/** Where a press on a fitted picture zooms to: its own size, or twice the fitted size for a picture whose own size is hardly larger. */
export function detailScale(frame: Frame): number {
  return frame.own >= DETAIL_FROM ? frame.own : 2;
}

/**
 * `view` kept inside the frame, as after a move or when the room changes
 * size: along a side the picture is longer than the room it covers the
 * room, and along one it fits it is centred.
 */
export function held(view: View, frame: Frame): View {
  const scale = Math.min(Math.max(1, maxScale(frame)), Math.max(1, view.scale));
  const axis = (offset: number, size: number, room: number) => {
    const most = Math.max(0, (size * scale - room) / 2);
    // Adding zero makes a negative zero a zero.
    return Math.min(most, Math.max(-most, offset)) + 0;
  };
  return { scale, x: axis(view.x, frame.width, frame.roomWidth), y: axis(view.y, frame.height, frame.roomHeight) };
}

/** Zoomed to `scale` about the point `at`, which stays where it is on screen unless the picture's edges say otherwise. Between fitted and `maxScale`. */
export function zoomAt(view: View, scale: number, at: { x: number; y: number }, frame: Frame): View {
  const next = Math.min(Math.max(1, maxScale(frame)), Math.max(1, scale));
  const k = next / Math.max(view.scale, Number.EPSILON);
  return held({ scale: next, x: at.x - (at.x - view.x) * k, y: at.y - (at.y - view.y) * k }, frame);
}

/** Moved by a drag. */
export function panBy(view: View, dx: number, dy: number, frame: Frame): View {
  return held({ scale: view.scale, x: view.x + dx, y: view.y + dy }, frame);
}

/** `steps` of the zoom buttons or keys, in (above 0) or out, about the room's centre. */
export function zoomStep(view: View, steps: number, frame: Frame): View {
  return zoomAt(view, view.scale * ZOOM_STEP ** steps, { x: 0, y: 0 }, frame);
}

/** A press on the picture at `at`: a fitted picture zooms to `detailScale` about the point, and a zoomed one goes back to fitted. */
export function pressed(view: View, at: { x: number; y: number }, frame: Frame): View {
  return view.scale > 1 ? VIEW_FIT : zoomAt(view, detailScale(frame), at, frame);
}

/** At the picture's own size, about the room's centre. */
export function ownSize(view: View, frame: Frame): View {
  return zoomAt(view, frame.own, { x: 0, y: 0 }, frame);
}

/**
 * What one wheel event multiplies the scale by. `delta` is the pixels it
 * would scroll the page, positive down it (`deltaY`), so up zooms in;
 * `pinch` is a trackpad's pinch, which a browser sends as a wheel with
 * Control held.
 */
export function wheelFactor(delta: number, pinch: boolean): number {
  const by = -delta * (pinch ? PINCH_RATE : WHEEL_RATE);
  return Math.exp(Math.min(WHEEL_MOST, Math.max(-WHEEL_MOST, by)));
}
