// Autoscroll (Ryan, 2026-11-13): the middle button held over a pane that
// scrolls, and the pointer moved up or down from where it was pressed,
// scrolls the pane continuously, faster the further the pointer is, until
// the button comes up, with a mark where it was pressed. Browsers do this
// themselves only on some systems (not on a Mac, off by default on Linux),
// so the page does it everywhere, and stops the browser's own (and Linux's
// middle-click paste) over a pane. The same numbers as the desktop shell's
// (`apps/desktop/src/app/autoscroll.rs`).

/** Within this many points of the press nothing scrolls. */
export const DEAD_ZONE = 8;
/** Points a second of scrolling for each point past the dead zone. */
export const RATE = 8;
/** The fastest it goes, in points a second. */
export const MAX_SPEED = 4000;

/** Points a second for a pointer `distance` points below the press (above, negative). */
export function speed(distance: number): number {
  const past = Math.abs(distance) - DEAD_ZONE;
  if (past <= 0) return 0;
  return Math.sign(distance) * Math.min(past * RATE, MAX_SPEED);
}

/** The nearest element at or above `target` that scrolls up and down and has somewhere to go. */
function pane(target: EventTarget | null): HTMLElement | null {
  for (let el = target instanceof Element ? target : null; el; el = el.parentElement) {
    if (!(el instanceof HTMLElement)) continue;
    const overflow = getComputedStyle(el).overflowY;
    if ((overflow === "auto" || overflow === "scroll") && el.scrollHeight > el.clientHeight) return el;
  }
  return null;
}

let held: { pane: HTMLElement; originY: number; y: number; at: number; pos: number; mark: HTMLElement; frame: number } | null = null;

function stop() {
  if (!held) return;
  cancelAnimationFrame(held.frame);
  held.mark.remove();
  document.documentElement.style.removeProperty("cursor");
  held = null;
}

function step(now: number) {
  if (!held) return;
  // A frame after a pause (the tab hidden) does not make up for it.
  const dt = Math.min(Math.max((now - held.at) / 1000, 0), 0.1);
  held.at = now;
  const v = speed(held.y - held.originY);
  if (v !== 0) {
    // Something else moved the pane meanwhile (the wheel, older messages
    // above): go on from where it is.
    if (Math.abs(held.pane.scrollTop - held.pos) > 2) held.pos = held.pane.scrollTop;
    held.pos += v * dt;
    held.pane.scrollTop = held.pos;
  }
  held.frame = requestAnimationFrame(step);
}

/** Listens on the whole page, once. */
export function installAutoscroll() {
  const middle = (e: MouseEvent) => e.button === 1;
  // Pointer events carry the gesture: with the middle button down a browser
  // may send no mouse moves at all. The mouse events are cancelled only so
  // the browser does nothing of its own.
  document.addEventListener(
    "pointerdown",
    (e) => {
      if (!middle(e) || e.pointerType !== "mouse") return;
      const scroller = pane(e.target);
      if (!scroller) return;
      e.preventDefault();
      stop();
      const mark = document.createElement("div");
      mark.className = "lu-autoscroll-mark";
      mark.setAttribute("aria-hidden", "true");
      mark.style.left = `${e.clientX}px`;
      mark.style.top = `${e.clientY}px`;
      document.body.append(mark);
      document.documentElement.style.cursor = "ns-resize";
      held = { pane: scroller, originY: e.clientY, y: e.clientY, at: performance.now(), pos: scroller.scrollTop, mark, frame: requestAnimationFrame(step) };
    },
    { capture: true },
  );
  // Heard first, before anything on the page can stop it.
  document.addEventListener(
    "pointermove",
    (e) => {
      if (held) held.y = e.clientY;
    },
    { capture: true },
  );
  document.addEventListener(
    "pointerup",
    (e) => {
      if (held && middle(e)) stop();
    },
    { capture: true },
  );
  document.addEventListener("pointercancel", stop);
  // No native autoscroll, and no paste on Linux, over a pane; and the click a
  // middle press makes there opens nothing.
  for (const type of ["mousedown", "mouseup", "auxclick"] as const) {
    document.addEventListener(
      type,
      (e) => {
        if (middle(e) && pane(e.target)) e.preventDefault();
      },
      { capture: true },
    );
  }
  window.addEventListener("blur", stop);
  document.addEventListener("keydown", (e) => {
    if (held && e.key === "Escape") stop();
  });
}
