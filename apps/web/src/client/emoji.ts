// What a server's own emoji need no page for (specs/emoji.md, plan step
// 12): where the composer's `:` list opens and what choosing from it or the
// picker writes, the Emoji section's count, and the instance's refusals in
// the boards' words. The rules of names, matches and refs are the core's
// (`emojiMatches`, `emojiNameFromFile`, `isEmojiName`, the refs found at
// send); nothing here decides one.
import { ClientError } from "./bridge";

/** Where an emoji's picture may come from: at most this many bytes are sent (the instance's bound, as an avatar's). */
export const EMOJI_UPLOAD_MAX = 8 * 1024 * 1024;

/**
 * The `:word` being typed at the caret, for the composer's list: where its
 * colon is and what follows it. Only at the start or after something that
 * is not a letter, a digit or `_`, so a time (`10:30`) or a word with a colon
 * in it opens nothing; the core answers nothing under two characters.
 */
export function colonAt(text: string, caret: number): { start: number; query: string } | null {
  const m = /(^|[^\p{L}\p{N}_]):([A-Za-z0-9_]*)$/u.exec(text.slice(0, caret));
  const query = m?.[2];
  if (query === undefined) return null;
  return { start: caret - query.length - 1, query };
}

/** A field's text with `what` put in place of what runs from `from` to `to`, and where the caret goes after it. */
export function spliced(text: string, from: number, to: number, what: string): { text: string; caret: number } {
  const a = Math.max(0, Math.min(from, text.length));
  const b = Math.max(a, Math.min(to, text.length));
  return { text: text.slice(0, a) + what + text.slice(b), caret: a + what.length };
}

/** What choosing a server's emoji writes into the composer: its name between colons, a space after it. */
export function typedEmoji(name: string): string {
  return `:${name}: `;
}

/** "12 of 50": a server's emoji against its limit; the count alone where the instance names no limit. */
export function emojiCount(count: number, limit: number): string {
  const n = count.toLocaleString("en-US");
  return limit > 0 ? `${n} of ${limit.toLocaleString("en-US")}` : n;
}

/** Whether a server has as many emoji as it may: Upload is off. */
export function emojiFull(count: number, limit: number): boolean {
  return limit > 0 && count >= limit;
}

/** The Emoji section's subtitle: what an emoji is and what a picture may be, or, at the limit, that the server has as many as it may. */
export function emojiSubtitle(server: string, count: number, limit: number): string {
  return emojiFull(count, limit)
    ? fullWords(server, limit)
    : `Used as :name: in ${server}’s channels and server DMs, and as reactions. A PNG, JPEG, WebP or GIF up to 8 MB, made to fit 128 pixels; an animated one plays.`;
}

function fullWords(server: string, limit: number): string {
  return `${server} has as many emoji as it may (${limit.toLocaleString("en-US")}). Remove one to upload another.`;
}

/** The words for a name a name may not be. */
export const NAME_RULE = "A name is 2 to 32 of a to z, 0 to 9 and _, with no spaces.";
/** A picture the instance would refuse for its size, said before anything is sent. */
export const TOO_LARGE = "This picture is too large for an emoji. An animated one can have at most 200 frames and come to 512 KB once made.";

/** "Remove :partyparrot:?" and what removing does. */
export function removeTitle(name: string): string {
  return `Remove :${name}:?`;
}
export const REMOVE_QUESTION = "Messages that used it will show its name instead.";

/** Where a refusal is said: under the name, under the picture, or over the section. */
export interface EmojiRefusal {
  on: "name" | "picture" | "section";
  text: string;
}

/**
 * Why an upload, a rename or a removal was refused, in the boards' words:
 * `server` is the server's name and `name` the name that was sent.
 */
export function emojiRefusal(err: unknown, server: string, name: string): EmojiRefusal {
  const ce = err instanceof ClientError ? err : null;
  const said = (on: EmojiRefusal["on"], text: string): EmojiRefusal => ({ on, text });
  switch (ce?.code) {
    case "ALREADY_EXISTS":
      return said("name", `${server} already has :${name}:. Choose another name.`);
    case "INVALID_ARGUMENT":
      if (ce.details.field === "picture") {
        if (ce.details.reason === "too_large") return said("picture", TOO_LARGE);
        if (ce.details.reason === "unsupported") return said("picture", "Lumia can’t use this file. Choose a PNG, JPEG, WebP or GIF.");
        return said("picture", "This picture couldn’t be read.");
      }
      return said("name", NAME_RULE);
    case "RESOURCE_EXHAUSTED":
      return said("section", fullWords(server, Number(ce.details.limit) || 0));
    case "PERMISSION_DENIED":
      return said("section", `You can’t manage ${server}’s emoji.`);
    case "NOT_FOUND":
      return said("section", "That emoji has been removed.");
    case "FAILED_PRECONDITION":
      return said("section", "This instance does not keep pictures.");
    case "RATE_LIMITED":
      return said("section", "The instance is busy. Try again in a moment.");
  }
  return said("section", err instanceof Error ? err.message : String(err));
}

/** A file's size as the upload form says it: "38 KB", "6.8 MB". */
export function sizeWords(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  const mb = bytes / (1024 * 1024);
  return `${mb < 10 ? mb.toFixed(1).replace(/\.0$/, "") : Math.round(mb)} MB`;
}
