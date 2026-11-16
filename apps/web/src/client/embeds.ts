// A link's embed on the page (specs/media.md 6.6): the rules that need no
// page. Which embed a card has is the core's (`embedFor`, from the link);
// here are its address as the page loads it (Twitch's parent), its box, the
// press's words, and the heights an embed reports, taken only from that
// embed's own window and origin.
import type { EmbedSummary } from "./bridge";

/** Players at most this tall, as pictures are (spec 5). */
export const PLAYER_MAX_HEIGHT = 400;
/** A height an embed reports is held within these, whatever it says. */
export const REPORTED_MIN = 50;
export const REPORTED_MAX = 5000;

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
 * The height an embed reports, or undefined: only from the very window
 * the embed was loaded into (`source`) and only from its origin, and only
 * in that site's own format, each read without trusting its shape.
 */
export function reportedHeight(e: EmbedSummary, frameWindow: unknown, origin: string, m: FrameMessage): number | undefined {
  if (!e.resizes || !frameWindow || m.source !== frameWindow || !origin || m.origin !== origin) return undefined;
  const h = heightIn(e.provider, m.data);
  if (h === undefined || !Number.isFinite(h) || h <= 0) return undefined;
  return Math.min(REPORTED_MAX, Math.max(REPORTED_MIN, Math.ceil(h)));
}

function parsed(data: unknown): unknown {
  if (typeof data !== "string") return data;
  try {
    return JSON.parse(data);
  } catch {
    return undefined;
  }
}

const record = (v: unknown): Record<string, unknown> | undefined => (v && typeof v === "object" && !Array.isArray(v) ? (v as Record<string, unknown>) : undefined);
const num = (v: unknown): number | undefined => (typeof v === "number" ? v : typeof v === "string" && /^\d+(\.\d+)?$/.test(v.trim()) ? Number(v) : undefined);

function heightIn(provider: EmbedSummary["provider"], data: unknown): number | undefined {
  switch (provider) {
    case "x": {
      // platform.twitter.com/widgets.js receives JSON-RPC filtered by
      // "twttr.embed": a string that starts with it (the JSON after it), or
      // an object with it as a key; and binds "twttr.private.resize" to a
      // handler reading params[0].height (`a.asInt(e.height)`).
      const key = "twttr.embed";
      const rpc = record(typeof data === "string" && data.startsWith(key) ? parsed(data.slice(key.length)) : parsed(record(parsed(data))?.[key]));
      if (rpc?.method !== "twttr.private.resize" || !Array.isArray(rpc.params)) return undefined;
      return num(record(rpc.params[0])?.height);
    }
    case "bluesky":
      // bluesky-social/social-app, bskyembed/src/components/container.tsx:
      // `window.parent.postMessage({height, id: …get('id')}, …)`, read by
      // embed.bsky.app/static/embed.js as `event.data.height`.
      return num(record(data)?.height);
    case "reddit": {
      // embed.reddit.com/widgets.js: `r = JSON.parse(t.data)` from a string,
      // then `"resize.embed" === r.type` and the height is `r.data`.
      if (typeof data !== "string") return undefined;
      const m = record(parsed(data));
      return m?.type === "resize.embed" ? num(m.data) : undefined;
    }
    default:
      return undefined;
  }
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
