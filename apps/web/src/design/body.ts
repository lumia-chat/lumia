// The rules a message body is drawn by that need no page (issue #18): the
// pieces a paragraph is drawn in from the core's runs and links, when a
// long body is drawn collapsed, and what a draft's counter says. The core
// reads the body (`BodyBlock`); nothing here parses Markdown.
import type { BodyLink, BodyRun } from "../client/bridge";

/** A body taller than this many lines is drawn collapsed… */
export const COLLAPSE_OVER = 14;
/** …at the height of its first this many. */
export const COLLAPSED_LINES = 10;

/** How many lines a body is as drawn: its height in lines of its text, a paragraph's spacing not making one. */
export function linesOf(height: number, line: number): number {
  return line > 0 ? Math.round(height / line) : 0;
}

/** The lines a body `lines` tall is drawn at, collapsed: its first ten when it is over fourteen; undefined when it is drawn whole. */
export function collapsedLines(lines: number): number | undefined {
  return lines > COLLAPSE_OVER ? COLLAPSED_LINES : undefined;
}

const GROUPED = new Intl.NumberFormat("en-US", { maximumFractionDigits: 0 });

/** A count with its thousands grouped: "1,204". */
export function grouped(n: number): string {
  return GROUPED.format(n);
}

/** What a draft's counter says: "1,204 characters left", "1 character left", or over the limit "3 characters too long". */
export function roomWords(left: number): string {
  const n = Math.abs(left);
  const what = n === 1 ? "character" : "characters";
  return left < 0 ? `${grouped(n)} ${what} too long` : `${grouped(n)} ${what} left`;
}

/**
 * Whether a key is part of an input method's composition, which Return
 * must not send or save: Safari gives the key that commits one as 229
 * with `isComposing` already false.
 */
export function composing(e: { isComposing: boolean; keyCode: number }): boolean {
  return e.isComposing || e.keyCode === 229;
}

/** A stretch of a paragraph drawn one way: the styles over all of it, and the mention it is. */
export interface Piece {
  text: string;
  emphasis: boolean;
  strong: boolean;
  strike: boolean;
  code: boolean;
  /** Its place in the message's mentions, when it is one. */
  mention?: number;
}

/** A paragraph's pieces in order, those of a link under it, so that a link is one element however it is styled inside. */
export interface Stretch {
  url?: string;
  pieces: Piece[];
}

/**
 * A paragraph's text cut where any run or link begins or ends, each piece
 * with the styles that cover it, the pieces grouped by the link they are
 * in. Ranges are UTF-16 code units, as the core gives them.
 */
export function stretches(text: string, runs: BodyRun[], links: BodyLink[]): Stretch[] {
  const clamp = (n: number) => Math.max(0, Math.min(text.length, n));
  const cuts = new Set([0, text.length]);
  for (const r of [...runs, ...links]) {
    cuts.add(clamp(r.start));
    cuts.add(clamp(r.end));
  }
  const at = [...cuts].sort((a, b) => a - b);
  const out: Stretch[] = [];
  let lastLink = -1;
  for (let i = 0; i + 1 < at.length; i++) {
    const start = at[i] as number;
    const end = at[i + 1] as number;
    if (start >= end) continue;
    const over = runs.filter((r) => r.start <= start && end <= r.end);
    const has = (kind: string) => over.some((r) => r.style.kind === kind);
    const mention = over.find((r) => r.style.kind === "mention")?.style;
    const piece: Piece = { text: text.slice(start, end), emphasis: has("emphasis"), strong: has("strong"), strike: has("strike"), code: has("code") };
    if (mention && "index" in mention) piece.mention = mention.index;
    const link = links.findIndex((l) => l.start <= start && end <= l.end);
    const last = out[out.length - 1];
    if (last && link === lastLink) last.pieces.push(piece);
    else out.push({ url: links[link]?.url, pieces: [piece] });
    lastLink = link;
  }
  return out;
}
