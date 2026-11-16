// Attachments and link cards on the page (specs/media.md 5 and 6): the
// rules that need no page, and the one cache of what a picture is drawn
// from. The core makes, seals, uploads, fetches and opens every blob; this
// decides how many tiles a grid draws in a row, how a size is said, which
// links a composer previews and sends, what an edit keeps, what a paste
// is, and how the viewer zooms. `media.test.ts` tests them.
import { createSignal, type Accessor } from "solid-js";

/** A message carries at most ten attachments (spec 2). */
export const MAX_ATTACHMENTS = 10;
/** And at most three cards (spec 6.1). */
export const MAX_CARDS = 3;
/** The pause in typing after which a composer asks for its links' cards (spec 6.4). */
export const PREVIEW_PAUSE_MS = 500;
/** A picture alone is drawn at most this tall (spec 5). */
export const MEDIA_MAX_HEIGHT = 400;

/** A picture's size drawn alone: its own, scaled down to fit `maxHeight` (and `maxWidth`), never up; 4:3 when unknown. */
export function fitSize(width: number, height: number, maxHeight = MEDIA_MAX_HEIGHT, maxWidth = Infinity): { width: number; height: number } {
  if (!(width > 0) || !(height > 0)) return fitSize(320, 240, maxHeight, maxWidth);
  const scale = Math.min(1, maxHeight / height, maxWidth / width);
  return { width: Math.max(1, Math.round(width * scale)), height: Math.max(1, Math.round(height * scale)) };
}

/**
 * How a message's pictures are laid out: one alone, else rows of three
 * with rows of two first where three do not divide them (5 is 2 then 3,
 * 7 is 2, 2, 3). Each tile's span of a six-column grid and its height.
 */
export function mediaGrid(count: number): { span: number; height: number }[] {
  if (count <= 1) return count === 1 ? [{ span: 6, height: MEDIA_MAX_HEIGHT }] : [];
  let threes = Math.floor(count / 3);
  let twos = 0;
  const left = count - threes * 3;
  if (left === 1) {
    threes -= 1;
    twos = 2;
  } else if (left === 2) twos = 1;
  const rows = [...Array<number>(twos).fill(2), ...Array<number>(threes).fill(3)];
  return rows.flatMap((n) => Array.from({ length: n }, () => ({ span: 6 / n, height: n === 2 ? 200 : 150 })));
}

/** A file's size as a row says it: "512 bytes", "48 KB", "2.4 MB" (KB and MB of 1,024). */
export function sizeWords(bytes: number): string {
  if (!(bytes >= 0)) return "";
  if (bytes < 1024) return `${bytes} ${bytes === 1 ? "byte" : "bytes"}`;
  const kb = bytes / 1024;
  if (kb < 1024) return `${kb < 10 ? kb.toFixed(1).replace(/\.0$/, "") : Math.round(kb)} KB`;
  const mb = kb / 1024;
  if (mb < 1024) return `${mb < 10 ? mb.toFixed(1).replace(/\.0$/, "") : Math.round(mb)} MB`;
  return `${(mb / 1024).toFixed(1).replace(/\.0$/, "")} GB`;
}

/** What the page does with a file before the core sees it: a video gets a poster, HEIC is decoded by the browser where it can be. */
export type FileKind = "still" | "heic" | "video" | "audio" | "other";

const EXT_KINDS: Record<string, FileKind> = {
  jpg: "still",
  jpeg: "still",
  png: "still",
  webp: "still",
  heic: "heic",
  heif: "heic",
  mp4: "video",
  m4v: "video",
  mov: "video",
  webm: "video",
  mp3: "audio",
  m4a: "audio",
  aac: "audio",
  ogg: "audio",
  oga: "audio",
  opus: "audio",
  wav: "audio",
  flac: "audio",
};

/** By its declared type, else its name's extension; the core decides what it is from the bytes. */
export function fileKind(name: string, type: string): FileKind {
  const t = type.toLowerCase();
  if (t === "image/heic" || t === "image/heif" || t === "image/heic-sequence" || t === "image/heif-sequence") return "heic";
  if (t === "image/jpeg" || t === "image/png" || t === "image/webp") return "still";
  if (t.startsWith("video/")) return "video";
  if (t.startsWith("audio/")) return "audio";
  const ext = /\.([a-z0-9]+)$/i.exec(name)?.[1]?.toLowerCase() ?? "";
  return EXT_KINDS[ext] ?? "other";
}

/**
 * Why a file will not be attached, said before it is read: anything but a
 * still picture (which the core makes smaller) over the instance's file cap.
 */
export function tooLargeWords(size: number, kind: FileKind, fileMaxBytes: number): string | undefined {
  if (kind === "still" || kind === "heic" || size <= fileMaxBytes) return undefined;
  return `Too large: files can be up to ${sizeWords(fileMaxBytes)} here.`;
}

/** An attachment not made or not uploaded, in words on its tile. */
export function attachErrorWords(message: string, code = "", fileMaxBytes?: number): string {
  const m = message.trim().replace(/^api error [A-Z_]+: /, "");
  if (code === "PERMISSION_DENIED" || /PERMISSION_DENIED/.test(m)) return "You can’t attach files here.";
  if (/does not keep attachments/i.test(m)) return "This instance doesn’t keep attachments.";
  if (/over the limit of/i.test(m) || code === "RESOURCE_EXHAUSTED" || /too_large/i.test(m)) return fileMaxBytes ? `Too large: files can be up to ${sizeWords(fileMaxBytes)} here.` : "Too large to send.";
  if (/too large to send/i.test(m)) return "This picture is too large to send, even made smaller.";
  if (/no thumbnail/i.test(m)) return "This picture can’t be made small enough to preview.";
  if (/fetch|network|transport|timed? ?out/i.test(m)) return "Not uploaded: the connection failed. Remove it and try again.";
  const said = (m.charAt(0).toUpperCase() + m.slice(1)).replace(/\s*details=.*$/, "");
  return said ? (/[.!?]$/.test(said) ? said : said + ".") : "Not uploaded.";
}

/**
 * Whether a paste is files rather than text: files and no text, or the
 * text is only a file's name (a file copied in a file manager). A
 * picture copied from a page or a document with its words is text.
 */
export function pasteTakesFiles(text: string, names: string[]): boolean {
  if (!names.length) return false;
  const t = text.trim();
  return !t || names.includes(t) || t.split(/\r?\n/).every((line) => names.includes(line.trim()));
}

/** The links a composer previews and sends cards for: those the person did not put away, at most three. */
export function keptCardUrls(links: string[], dismissed: ReadonlySet<string>): string[] {
  return links.filter((url) => !dismissed.has(url)).slice(0, MAX_CARDS);
}

/**
 * What an edit says of its cards (spec 6.4, `EditMessage.cards`): null to
 * leave them as they are, when none was put away and no link is new;
 * otherwise the cards kept and the links new to the body.
 */
export function editCardUrls(had: string[], oldLinks: string[], newLinks: string[], removed: ReadonlySet<string>): string[] | null {
  const added = newLinks.filter((url) => !oldLinks.includes(url));
  if (!removed.size && !added.length) return null;
  const kept = had.filter((url) => !removed.has(url) && newLinks.includes(url));
  return [...kept, ...added.filter((url) => !removed.has(url) && !kept.includes(url))].slice(0, MAX_CARDS);
}

/** A picture's type from its first bytes, for the blob it is drawn from and saved as. */
export function sniffImage(bytes: Uint8Array): string | undefined {
  const b = (i: number) => bytes[i] ?? -1;
  if (b(0) === 0x89 && b(1) === 0x50 && b(2) === 0x4e && b(3) === 0x47) return "image/png";
  if (b(0) === 0xff && b(1) === 0xd8 && b(2) === 0xff) return "image/jpeg";
  if (b(0) === 0x47 && b(1) === 0x49 && b(2) === 0x46 && b(3) === 0x38) return "image/gif";
  if (b(0) === 0x52 && b(1) === 0x49 && b(2) === 0x46 && b(3) === 0x46 && b(8) === 0x57 && b(9) === 0x45 && b(10) === 0x42 && b(11) === 0x50) return "image/webp";
  return undefined;
}

/** The viewer's zoom: a scale and the picture's offset from the stage's centre, in pixels. */
export interface View {
  scale: number;
  x: number;
  y: number;
}
export const VIEW_FIT: View = { scale: 1, x: 0, y: 0 };
export const ZOOM_MAX = 4;

/**
 * Zoomed to `scale` about `point` (from the stage's centre): the spot under
 * the pointer stays under it. Between 1 and `ZOOM_MAX`; back at 1, centred.
 */
export function zoomAt(view: View, point: { x: number; y: number }, scale: number): View {
  const next = Math.min(ZOOM_MAX, Math.max(1, scale));
  if (next === 1) return VIEW_FIT;
  const k = next / view.scale;
  return { scale: next, x: point.x - (point.x - view.x) * k, y: point.y - (point.y - view.y) * k };
}

/** Moved by a drag, kept so the picture still covers the stage's centre. */
export function panBy(view: View, dx: number, dy: number, size: { width: number; height: number }): View {
  if (view.scale <= 1) return VIEW_FIT;
  const limX = (size.width * (view.scale - 1)) / 2;
  const limY = (size.height * (view.scale - 1)) / 2;
  const clamp = (v: number, lim: number) => Math.min(lim, Math.max(-lim, v));
  return { scale: view.scale, x: clamp(view.x + dx, limX), y: clamp(view.y + dy, limY) };
}

/**
 * Calls `run` once typing pauses for `ms`: each `call` puts it off again,
 * the last value wins, `cancel` drops it. The timers can be stood in for.
 */
export function debounce<T>(run: (value: T) => void, ms: number, timers: { set: (f: () => void, ms: number) => unknown; clear: (handle: unknown) => void } = { set: (f, ms) => setTimeout(f, ms), clear: (h) => clearTimeout(h as ReturnType<typeof setTimeout>) }) {
  let handle: unknown;
  return {
    call(value: T) {
      if (handle !== undefined) timers.clear(handle);
      handle = timers.set(() => {
        handle = undefined;
        run(value);
      }, ms);
    },
    cancel() {
      if (handle !== undefined) timers.clear(handle);
      handle = undefined;
    },
  };
}

/** Where a picture drawn from a blob stands: loading, drawn from `url`, or not to be drawn (it failed, or its header is out of bounds). */
export type BlobState = "loading" | "ready" | "failed";

export interface BlobEntry {
  url: Accessor<string | undefined>;
  state: Accessor<BlobState>;
}

interface Kept extends BlobEntry {
  refs: number;
  timer?: unknown;
  setUrl: (u: string | undefined) => void;
  setState: (s: BlobState) => void;
}

/**
 * The page's blob URLs, one per attachment's blob (or placeholder, or card
 * picture), by key: made once and handed to every row that draws it, so a
 * row drawn again (the timeline rebuilds its rows when its data changes)
 * finds the same URL rather than loading and making another. A URL no row
 * holds is revoked after `linger` ms, unless one takes it up again first.
 */
export class BlobUrls {
  private kept = new Map<string, Kept>();
  constructor(
    private readonly make: (blob: Blob) => string = (b) => URL.createObjectURL(b),
    private readonly revoke: (url: string) => void = (u) => URL.revokeObjectURL(u),
    private readonly linger = 30_000,
    private readonly timers: { set: (f: () => void, ms: number) => unknown; clear: (h: unknown) => void } = { set: (f, ms) => setTimeout(f, ms), clear: (h) => clearTimeout(h as ReturnType<typeof setTimeout>) },
  ) {}

  /** The entry for `key`, loading it the first time (`load` answering null is "not to be drawn"); held until `release`. */
  acquire(key: string, load: () => Promise<Blob | null>): BlobEntry {
    let k = this.kept.get(key);
    if (!k) {
      const [url, setUrl] = createSignal<string | undefined>();
      const [state, setState] = createSignal<BlobState>("loading");
      const made: Kept = { url, state, setUrl, setState, refs: 0 };
      k = made;
      this.kept.set(key, made);
      load().then(
        (blob) => {
          if (this.kept.get(key) !== made) return;
          if (!blob) return made.setState("failed");
          made.setUrl(this.make(blob));
          made.setState("ready");
        },
        () => {
          if (this.kept.get(key) === made) made.setState("failed");
        },
      );
    }
    if (k.timer !== undefined) {
      this.timers.clear(k.timer);
      k.timer = undefined;
    }
    k.refs++;
    return k;
  }

  /** One holder fewer; with none left, the URL goes after the linger. */
  release(key: string) {
    const k = this.kept.get(key);
    if (!k || k.refs <= 0) return;
    k.refs--;
    if (k.refs > 0) return;
    k.timer = this.timers.set(() => this.drop(key), this.linger);
  }

  /** Whether `key` is held or lingering. */
  has(key: string): boolean {
    return this.kept.has(key);
  }

  private drop(key: string) {
    const k = this.kept.get(key);
    if (!k || k.refs > 0) return;
    this.kept.delete(key);
    const u = k.url();
    if (u) this.revoke(u);
  }

  /** Every URL revoked now (a removed device). */
  clear() {
    for (const [key, k] of this.kept) {
      if (k.timer !== undefined) this.timers.clear(k.timer);
      const u = k.url();
      if (u) this.revoke(u);
      this.kept.delete(key);
    }
  }
}

/** The page's one cache of blob URLs. */
export const blobUrls = new BlobUrls();
