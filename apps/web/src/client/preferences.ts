// Preferences: how Lumia looks in this browser. Light or dark (or as the
// system is) and how much room messages take, the choices the desktop
// shell's Preferences page has. They are this browser's, not the
// account's: kept in its storage, never sent to the instance, and still
// there after signing out.
//
// The desktop's third choice, when scroll bars show, is not here: a page
// has no say in that which holds in every browser, and a control that
// does nothing is worse than none.
import { createSignal } from "solid-js";

export type Appearance = "light" | "dark" | "system";
export type Density = "comfortable" | "compact";

export interface Preferences {
  appearance: Appearance;
  density: Density;
}

const KEY = "lumia:preferences";
const DEFAULTS: Preferences = { appearance: "system", density: "comfortable" };

/** What was stored, with the default for anything missing or not understood. */
export function readPreferences(stored: string | null): Preferences {
  let kept: Partial<Record<keyof Preferences, unknown>> = {};
  try {
    const parsed: unknown = stored ? JSON.parse(stored) : null;
    if (parsed && typeof parsed === "object") kept = parsed;
  } catch {
    /* not JSON: the defaults */
  }
  return {
    appearance: kept.appearance === "light" || kept.appearance === "dark" ? kept.appearance : DEFAULTS.appearance,
    density: kept.density === "compact" ? "compact" : DEFAULTS.density,
  };
}

function load(): Preferences {
  try {
    return readPreferences(localStorage.getItem(KEY));
  } catch {
    return DEFAULTS;
  }
}

const [current, setCurrent] = createSignal<Preferences>(load());

/** The choices as they stand; a view that reads this follows a change. */
export const preferences = current;

// The browser's own chrome (a phone's address bar) takes its colour from
// the page's `theme-color`, which index.html gives per system scheme. A
// chosen appearance names one colour for both.
const metas = typeof document === "undefined" ? [] : Array.from(document.querySelectorAll<HTMLMetaElement>('meta[name="theme-color"]'));
const colours = metas.map((meta) => ({ scheme: /dark/.test(meta.media) ? "dark" : "light", colour: meta.content }));

/**
 * Makes the page wear the choices: a chosen appearance is `data-theme` on
 * the root, which `tokens.css` reads, and "system" is no attribute, so
 * the page follows the system as it changes.
 */
export function applyPreferences() {
  if (typeof document === "undefined") return;
  const { appearance } = current();
  if (appearance === "system") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = appearance;
  metas.forEach((meta, i) => {
    const own = colours[i]?.colour ?? meta.content;
    meta.content = appearance === "system" ? own : (colours.find((c) => c.scheme === appearance)?.colour ?? own);
  });
}

function change(next: Preferences) {
  setCurrent(next);
  applyPreferences();
  try {
    localStorage.setItem(KEY, JSON.stringify(next));
  } catch {
    /* storage may be unavailable: the choice lasts as long as the page */
  }
}

export function setAppearance(appearance: Appearance) {
  if (appearance !== current().appearance) change({ ...current(), appearance });
}

export function setDensity(density: Density) {
  if (density !== current().density) change({ ...current(), density });
}

applyPreferences();
// A choice made in another tab of this browser is this tab's too.
if (typeof window !== "undefined")
  window.addEventListener("storage", (e) => {
    if (e.key !== KEY) return;
    setCurrent(readPreferences(e.newValue));
    applyPreferences();
  });
