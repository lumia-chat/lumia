// Preferences: how Lumia looks in this browser. Light or dark (or as the
// system is) and how messages are drawn, the choices the desktop shell's
// Preferences page has. They are this browser's, not the account's: kept
// in its storage, never sent to the instance, and still there after
// signing out.
//
// How messages are drawn is six settings, and Comfortable, Compact and
// Minimal are presets that write their values: what is kept is the
// values, and the preset shown is the one they match (plan step 9,
// "Display settings").
//
// The desktop's third choice, when scroll bars show, is not here: a page
// has no say in that which holds in every browser, and a control that
// does nothing is worse than none.
import { createSignal } from "solid-js";

export type Appearance = "light" | "dark" | "system";
export type Layout = "grouped" | "one_line";
export type Spacing = "roomy" | "cozy" | "tight";
export type Avatars = "large" | "small" | "hidden";
/** Shown where they are, or a link in their place. Nothing has either yet: no control until attachments and unfurling exist. */
export type Media = "shown" | "link";
export type Preset = "comfortable" | "compact" | "minimal";

/** The message text's size; every text in a message row, and the composer's, scales with it over 15. */
export const FONT_SIZES = [13, 14, 15, 16, 18, 20] as const;
export type FontSize = (typeof FONT_SIZES)[number];

export interface Display {
  layout: Layout;
  spacing: Spacing;
  avatars: Avatars;
  images: Media;
  embeds: Media;
}

export interface Preferences extends Display {
  appearance: Appearance;
  fontSize: FontSize;
  /** The member list (or a conversation's details) put away from a wide window's row. */
  asideHidden: boolean;
}

/** What each preset writes. Font size is never part of one: it is often a need, and trying Compact should not undo it. */
export const PRESETS: Record<Preset, Display> = {
  comfortable: { layout: "grouped", spacing: "roomy", avatars: "large", images: "shown", embeds: "shown" },
  compact: { layout: "grouped", spacing: "cozy", avatars: "small", images: "shown", embeds: "shown" },
  minimal: { layout: "one_line", spacing: "tight", avatars: "hidden", images: "link", embeds: "link" },
};

/** The preset these values are, or none ("Custom"). One line draws no avatar, so there they are not compared. */
export function presetOf(d: Display): Preset | undefined {
  return (Object.keys(PRESETS) as Preset[]).find((name) => {
    const p = PRESETS[name];
    return p.layout === d.layout && p.spacing === d.spacing && p.images === d.images && p.embeds === d.embeds && (d.layout === "one_line" || p.avatars === d.avatars);
  });
}

const KEY = "lumia:preferences";
const DEFAULTS: Preferences = { appearance: "system", ...PRESETS.comfortable, fontSize: 15, asideHidden: false };

function oneOf<T extends string | number>(value: unknown, allowed: readonly T[], fallback: T): T {
  return allowed.includes(value as T) ? (value as T) : fallback;
}

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
    appearance: oneOf(kept.appearance, ["light", "dark", "system"], DEFAULTS.appearance),
    layout: oneOf(kept.layout, ["grouped", "one_line"], DEFAULTS.layout),
    spacing: oneOf(kept.spacing, ["roomy", "cozy", "tight"], DEFAULTS.spacing),
    avatars: oneOf(kept.avatars, ["large", "small", "hidden"], DEFAULTS.avatars),
    fontSize: oneOf(kept.fontSize, FONT_SIZES, DEFAULTS.fontSize),
    images: oneOf(kept.images, ["shown", "link"], DEFAULTS.images),
    embeds: oneOf(kept.embeds, ["shown", "link"], DEFAULTS.embeds),
    asideHidden: kept.asideHidden === true,
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
 * the page follows the system as it changes. The font size is
 * `--msg-scale` on the root, which the message rows and the composer
 * multiply their sizes by.
 */
export function applyPreferences() {
  if (typeof document === "undefined") return;
  const { appearance, fontSize } = current();
  if (appearance === "system") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = appearance;
  if (fontSize === DEFAULTS.fontSize) document.documentElement.style.removeProperty("--msg-scale");
  else document.documentElement.style.setProperty("--msg-scale", String(fontSize / DEFAULTS.fontSize));
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

/** Sets one or more of the choices; one that is already so changes nothing. */
export function setPreferences(next: Partial<Preferences>) {
  const merged = { ...current(), ...next };
  if ((Object.keys(merged) as (keyof Preferences)[]).some((k) => merged[k] !== current()[k])) change(merged);
}

export function setAppearance(appearance: Appearance) {
  setPreferences({ appearance });
}

export function setAsideHidden(asideHidden: boolean) {
  setPreferences({ asideHidden });
}

/** Writes a preset's values; the font size stays as it is. */
export function choosePreset(preset: Preset) {
  setPreferences(PRESETS[preset]);
}

applyPreferences();
// A choice made in another tab of this browser is this tab's too.
if (typeof window !== "undefined")
  window.addEventListener("storage", (e) => {
    if (e.key !== KEY) return;
    setCurrent(readPreferences(e.newValue));
    applyPreferences();
  });
