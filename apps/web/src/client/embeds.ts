// A link's embed on the page (specs/media.md 6.6): the rules that need no
// page. Which embed a card has is the core's (`embedFor`, from the link);
// here are its address as the page loads it (Twitch's parent), its box, the
// press's words, and which of the messages the page receives are an
// embed's own to report its height with: only those from its window and
// origin. What height one says is the core's to read
// (`reportedEmbedHeight`), in each site's own form and held to its bounds.
import type { EmbedSummary } from "./bridge";
import type { PlayingKind } from "./playback";

/** Players at most this tall, as pictures are (spec 5). */
export const PLAYER_MAX_HEIGHT = 400;

/** Whether a card's embed can be pressed: an address, or a Bluesky handle and record key to resolve on the press. */
export function pressable(e: EmbedSummary | null | undefined): e is EmbedSummary {
  return !!e && (!!e.url || (!!e.blueskyHandle && !!e.blueskyRkey));
}

/** What the press is drawn as: a play button on the card's picture (a player or audio), or "Show post". */
export function pressOf(e: EmbedSummary): "play" | "show" {
  return e.kind === "post" ? "show" : "play";
}

/** The press's accessible name: the core's label and the card's title after a comma ("Play video from YouTube, <title>"). */
export function pressLabel(e: EmbedSummary, title: string): string {
  const t = title.trim();
  return t ? `${e.label}, ${t}` : e.label;
}

/** What an embed is to the rules of what plays and what stops (`client/playback.ts`): a site's player is a video. */
export function playingOf(e: EmbedSummary): PlayingKind {
  return { embed: true, kind: e.kind === "player" ? "video" : e.kind };
}

/** The iframe's title: the card's title, or the site's name when there is none. */
export function frameTitle(e: EmbedSummary, title: string): string {
  return title.trim() || e.providerName;
}

/** The address loaded: Twitch names the page holding it (`&parent=`, its host), or it refuses to play. */
export function frameSrc(e: EmbedSummary, url: string, hostname: string): string {
  if (!e.needsParent || !hostname) return url;
  return `${url}${url.includes("?") ? "&" : "?"}parent=${encodeURIComponent(hostname)}`;
}

/** The origin an address loads from; empty when it is not one. */
export function originOf(url: string): string {
  try {
    return new URL(url).origin;
  } catch {
    return "";
  }
}

/** Whether the address is one of the origins the page may frame (the core's list, the instance's `frame-src`). */
export function allowedSrc(url: string, origins: readonly string[]): boolean {
  const o = originOf(url);
  return !!o && url.startsWith("https://") && origins.includes(o);
}

/** A Bluesky lookup that got no answer (no network, a timeout, Bluesky failing): not word about the post, so the card's next press asks again. */
export const BLUESKY_UNREACHED = "Couldn’t reach Bluesky. Try again.";

/** What a card's press came to: the address its frame loads, or why there is none, in words, and whether another press may come to something else (`again`); where it may not, the card opens the link from then on. `address` is the embed's own, a Bluesky handle's once resolved, empty when Bluesky has no account for the handle, and undefined when Bluesky gave no answer. */
export function pressed(e: EmbedSummary, address: string | undefined, origins: readonly string[], hostname: string): { src: string } | { note: string; again: boolean } {
  if (address === undefined) return { note: BLUESKY_UNREACHED, again: true };
  const src = address ? frameSrc(e, address, hostname) : "";
  if (src && allowedSrc(src, origins)) return { src };
  const goes = `The link opens it on ${e.providerName || "the site"}.`;
  return { note: !address && e.blueskyHandle ? `Bluesky couldn’t find @${e.blueskyHandle}. ${goes}` : `Couldn’t show this here. ${goes}`, again: false };
}

/** The embed's box, as CSS: a player at the column's width at its shape and at most 400 tall; audio at the column's width and the site's height; a post at most 550 wide, the height it reported or its own. */
export function frameBox(e: EmbedSummary, reported?: number): Record<string, string> {
  const max = e.maxWidth > 0 ? `${e.maxWidth}px` : "100%";
  if (e.aspectWidth > 0 && e.aspectHeight > 0) {
    // As wide as the column, but no wider than 400 tall allows at its shape.
    const widest = Math.floor((PLAYER_MAX_HEIGHT * e.aspectWidth) / e.aspectHeight);
    return { width: "100%", "max-width": e.maxWidth > 0 ? `${Math.min(e.maxWidth, widest)}px` : `${widest}px`, "aspect-ratio": `${e.aspectWidth} / ${e.aspectHeight}` };
  }
  const height = e.resizes && reported ? reported : e.fixedHeight;
  return { width: "100%", "max-width": max, height: `${height}px` };
}

/** A message an embed's window posted, as the page receives it. */
export interface FrameMessage {
  source: unknown;
  origin: string;
  data: unknown;
}

/**
 * What an embed posted, as the text the core reads its height from, or
 * undefined for a message that is not this embed's to report with: only
 * one from the very window the embed was loaded into (`source`) and from
 * its origin is taken. A string goes as it was posted and an object as
 * its JSON; anything else, and an object that has no JSON, is refused.
 */
export function postedText(e: EmbedSummary, frameWindow: unknown, origin: string, m: FrameMessage): string | undefined {
  if (!e.resizes || !frameWindow || m.source !== frameWindow || !origin || m.origin !== origin) return undefined;
  if (typeof m.data === "string") return m.data;
  if (!m.data || typeof m.data !== "object") return undefined;
  try {
    const text: unknown = JSON.stringify(m.data);
    return typeof text === "string" ? text : undefined;
  } catch {
    // What a message can carry and JSON cannot: an object that holds itself, a BigInt.
    return undefined;
  }
}

/**
 * The heights an embed reports while it is shown: each message that is
 * the embed's own (`postedText`) is read by the core (`read`, which is
 * `reportedEmbedHeight`: the site's own form, held to 50–5000, null for
 * anything else) and its height handed to `draw`. The core answers later,
 * so an answer to an earlier message never replaces a later one's.
 */
export function reportedHeights(e: EmbedSummary, origin: string, read: (provider: string, message: string) => Promise<number | null>, draw: (height: number) => void): (frameWindow: unknown, m: FrameMessage) => void {
  let posted = 0;
  let drawn = 0;
  return (frameWindow, m) => {
    const text = postedText(e, frameWindow, origin, m);
    if (text === undefined) return;
    const n = ++posted;
    read(e.provider, text)
      .then((height) => {
        if (height === null || n < drawn) return;
        drawn = n;
        draw(height);
      })
      .catch((err) => console.warn("embed height:", err));
  };
}

/** Each link's embed asked of the core once for the page: nothing is fetched, so a link's answer never changes. */
export class EmbedLookup {
  private known = new Map<string, Promise<EmbedSummary | null>>();
  constructor(private ask: (url: string) => Promise<EmbedSummary | null>) {}

  of(url: string): Promise<EmbedSummary | null> {
    let p = this.known.get(url);
    if (!p) {
      p = this.ask(url).catch((err) => {
        // Asked again next time: the worker may not have been ready.
        this.known.delete(url);
        console.warn("embed:", url, err);
        return null;
      });
      this.known.set(url, p);
    }
    return p;
  }
}
