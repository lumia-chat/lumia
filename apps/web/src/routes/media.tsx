// Attachments and link cards in a conversation (specs/media.md 5 and 6):
// what a message carries drawn under its body, the viewer, and the
// composer's side (files chosen, pasted or dropped, made and uploaded by
// the core as soon as they are added; the cards of the links being
// written, in a server's channel only). Every blob is fetched and opened
// by the core; the page turns the bytes into blob URLs, one per blob,
// kept in `blobUrls` so a row drawn again draws the same one.
import { For, Show, createEffect, createMemo, createSignal, mapArray, on, onCleanup, onMount, type Accessor } from "solid-js";
import { Attachment, AudioAttachment, DropZone, LinkCard, MediaGrid, Viewer, type DraftTileProps, type LinkCardProps, type MediaKind, type MediaTileProps } from "../design/components";
import { ClientError, type AttachOptions, type AttachmentDraft, type AttachmentSummary, type BlobSummary, type CardSummary, type Pixels, type UploadLimits } from "../client/bridge";
import { cardImageSrc, client } from "../client/state";
import { MAX_ATTACHMENTS, PREVIEW_PAUSE_MS, attachErrorWords, blobUrls, debounce, fileKind, keptCardUrls, pasteTakesFiles, sizeWords, sniffImage, tooLargeWords, type BlobState } from "../client/media";
import type { Media } from "../client/preferences";

// ---- Blobs drawn ----

/** Whether the system asks for less motion: a GIF is held still until pressed. */
const motionQuery = typeof window !== "undefined" ? window.matchMedia?.("(prefers-reduced-motion: reduce)") : undefined;
const [reducedMotion, setReducedMotion] = createSignal(!!motionQuery?.matches);
motionQuery?.addEventListener?.("change", (e) => setReducedMotion(e.matches));

/** A blob URL held while the calling component lives (and the key stays), loaded once for the page. */
function useBlob(key: () => string | undefined, load: () => Promise<Blob | null>): { url: Accessor<string | undefined>; state: Accessor<BlobState | undefined> } {
  const entry = createMemo(() => {
    const k = key();
    if (!k) return undefined;
    // Drawn as a file row when it fails; why is said here.
    const e = blobUrls.acquire(k, () =>
      load().catch((err) => {
        console.warn("attachment:", k, err);
        throw err;
      }),
    );
    onCleanup(() => blobUrls.release(k));
    return e;
  });
  return { url: () => entry()?.url(), state: () => entry()?.state() };
}

/** A blob held only until `use` has done with its URL (a Save). */
async function withBlobUrl(key: string, load: () => Promise<Blob | null>, use: (url: string) => void): Promise<boolean> {
  const e = blobUrls.acquire(key, load);
  try {
    for (let i = 0; e.state() === "loading" && i < 600; i++) await new Promise((r) => setTimeout(r, 50));
    const url = e.url();
    if (url) use(url);
    return !!url;
  } finally {
    blobUrls.release(key);
  }
}

/** A picture's bytes from the core, drawn only if its header is within the bounds (spec 5). */
async function pictureBlob(blob: BlobSummary, mediaType = ""): Promise<Blob | null> {
  const bytes = await client().attachmentBytes(blob);
  if (!(await client().attachmentDrawable(bytes))) return null;
  return new Blob([bytes as Uint8Array<ArrayBuffer>], { type: sniffImage(bytes) ?? mediaType });
}

/** A file's bytes from the core, as its type. */
async function fileBlob(blob: BlobSummary, mediaType: string): Promise<Blob> {
  const bytes = await client().attachmentBytes(blob);
  return new Blob([bytes as Uint8Array<ArrayBuffer>], { type: mediaType || "application/octet-stream" });
}

/** A placeholder (ThumbHash) as a tiny picture, scaled up where it is drawn. */
async function placeholderBlob(placeholder: string): Promise<Blob | null> {
  const p = await client().placeholderImage(placeholder);
  if (!p || !p.width || !p.height) return null;
  const canvas = document.createElement("canvas");
  canvas.width = p.width;
  canvas.height = p.height;
  canvas.getContext("2d")?.putImageData(new ImageData(new Uint8ClampedArray(p.rgba), p.width, p.height), 0, 0);
  return new Promise((resolve) => canvas.toBlob((b) => resolve(b), "image/png"));
}

const keyOf = (blob: BlobSummary | null | undefined) => (blob ? "blob:" + blob.blobId : undefined);

/** Saves a file under its name: a link with `download`, pressed. */
function saveAs(url: string, name: string) {
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.rel = "noopener";
  document.body.append(a);
  a.click();
  a.remove();
}

function mainLoad(a: AttachmentSummary): () => Promise<Blob | null> {
  return a.kind === "image" || a.kind === "gif" ? () => pictureBlob(a.main, a.mediaType) : () => fileBlob(a.main, a.mediaType);
}

/** Saves an attachment's file, fetching it first where it has not been. */
export function saveAttachment(a: AttachmentSummary): Promise<boolean> {
  return withBlobUrl("blob:" + a.main.blobId, mainLoad(a), (url) => saveAs(url, a.name || "attachment"));
}

// ---- A message's attachments and cards ----

/** What an attachment drawn in a message may be: viewed (pictures, and in link mode videos and audio), or saved. */
const viewable = (a: AttachmentSummary) => a.kind !== "file";

export interface MessageMediaProps {
  attachments: AttachmentSummary[];
  cards: CardSummary[];
  /** Preferences' Images and Embeds. */
  images: Media;
  embeds: Media;
  onView: (a: AttachmentSummary) => void;
  /** While its author edits it: a × on each, and those taken off not drawn. */
  editing?: {
    removedAttachments: ReadonlySet<string>;
    removedCards: ReadonlySet<string>;
    removeAttachment: (id: string) => void;
    removeCard: (url: string) => void;
  };
}

/** One picture, GIF or video of a message, with what it is drawn from loaded for it. */
function tileOf(a: AttachmentSummary, props: MessageMediaProps): MediaTileProps & { failed: () => boolean } {
  const placeholder = useBlob(
    () => (a.placeholder ? "ph:" + a.attachmentId : undefined),
    () => placeholderBlob(a.placeholder),
  );
  const thumb = useBlob(
    () => keyOf(a.thumbnail),
    () => (a.thumbnail ? pictureBlob(a.thumbnail) : Promise.resolve(null)),
  );
  // A GIF plays as itself, unless the system asks for less motion and it was not pressed.
  const [played, setPlayed] = createSignal(false);
  const gif = useBlob(
    () => (a.kind === "gif" && (!reducedMotion() || played()) ? keyOf(a.main) : undefined),
    () => pictureBlob(a.main, "image/gif"),
  );
  const [wanted, setWanted] = createSignal(false);
  const video = useBlob(
    () => (a.kind === "video" && wanted() ? keyOf(a.main) : undefined),
    () => fileBlob(a.main, a.mediaType),
  );
  return {
    kind: a.kind as "image" | "gif" | "video",
    name: a.name,
    alt: a.alt,
    width: a.width,
    height: a.height,
    get src() {
      return gif.url() ?? thumb.url();
    },
    get placeholder() {
      return placeholder.url();
    },
    get still() {
      return a.kind === "gif" && !gif.url() && reducedMotion() && !played();
    },
    get video() {
      return video.url();
    },
    get busy() {
      return (wanted() && video.state() === "loading") || (played() && gif.state() === "loading");
    },
    onOpen: () => {
      if (a.kind === "video") setWanted(true);
      else if (a.kind === "gif" && reducedMotion() && !played()) setPlayed(true);
      else props.onView(a);
    },
    get onRemove() {
      const e = props.editing;
      return e ? () => e.removeAttachment(a.attachmentId) : undefined;
    },
    // A picture whose header is out of bounds, or that is not served, is drawn as a file row (spec 5).
    failed: () => (a.kind !== "video" && thumb.state() === "failed") || (a.kind === "gif" && gif.state() === "failed"),
  };
}

function FileRow(props: { a: AttachmentSummary; open?: () => void; onRemove?: () => void }) {
  const [busy, setBusy] = createSignal(false);
  const [note, setNote] = createSignal<string>();
  const download = async () => {
    setBusy(true);
    setNote("Loading…");
    const ok = await saveAttachment(props.a).catch(() => false);
    setBusy(false);
    setNote(ok ? undefined : "Couldn’t be opened.");
  };
  return (
    <Attachment
      kind={props.a.kind}
      name={props.a.name || "Attachment"}
      size={sizeWords(props.a.sizeBytes)}
      note={note()}
      onOpen={props.open}
      onDownload={() => !busy() && void download()}
      onRemove={props.onRemove}
    />
  );
}

function AudioRow(props: { a: AttachmentSummary; onRemove?: () => void }) {
  const [wanted, setWanted] = createSignal(false);
  const audio = useBlob(
    () => (wanted() ? keyOf(props.a.main) : undefined),
    () => fileBlob(props.a.main, props.a.mediaType),
  );
  return (
    <AudioAttachment
      name={props.a.name || "Audio"}
      size={sizeWords(props.a.sizeBytes)}
      src={audio.url()}
      busy={wanted() && audio.state() === "loading"}
      note={audio.state() === "failed" ? "Couldn’t be opened." : undefined}
      onPlay={() => setWanted(true)}
      href={audio.url()}
      onDownload={() => void saveAttachment(props.a)}
      onRemove={props.onRemove}
    />
  );
}

/** A card under a message: its host from the link, never the page's name; its picture by address (a server's) or as a blob (a personal one's). */
function CardView(props: { card: CardSummary; onRemove?: () => void }) {
  const blob = useBlob(
    () => keyOf(props.card.image),
    () => (props.card.image ? pictureBlob(props.card.image) : Promise.resolve(null)),
  );
  return (
    <LinkCard
      url={props.card.url}
      host={props.card.host}
      title={props.card.title}
      description={props.card.description}
      color={props.card.color}
      image={cardImageSrc(props.card.imageUrl) ?? blob.url()}
      imageWidth={props.card.imageWidth}
      imageHeight={props.card.imageHeight}
      video={props.card.video}
      onRemove={props.onRemove}
    />
  );
}

/** What a message carries, under its body: its pictures in a grid, its audio and files as rows, then its cards. */
export function MessageMedia(props: MessageMediaProps) {
  const kept = createMemo(() => props.attachments.filter((a) => !props.editing?.removedAttachments.has(a.attachmentId)));
  const linked = () => props.images === "link";
  const visual = createMemo(() => (linked() ? [] : kept().filter((a) => a.kind === "image" || a.kind === "gif" || a.kind === "video")));
  const tiles = createMemo(mapArray(visual, (a) => ({ a, tile: tileOf(a, props) })));
  const shownTiles = createMemo(() => tiles().filter((t) => !t.tile.failed()));
  const rows = createMemo(() => {
    const failed = new Set(tiles().filter((t) => t.tile.failed()).map((t) => t.a.attachmentId));
    return kept().filter((a) => linked() || failed.has(a.attachmentId) || a.kind === "audio" || a.kind === "file");
  });
  const cards = createMemo(() => (props.embeds === "link" ? [] : props.cards.filter((c) => !props.editing?.removedCards.has(c.url))));
  const remover = (id: string) => (props.editing ? () => props.editing?.removeAttachment(id) : undefined);
  return (
    <Show when={shownTiles().length || rows().length || cards().length}>
      <div class="lu-mediablock">
        <Show when={shownTiles().length}>
          <MediaGrid items={shownTiles().map((t) => t.tile)} />
        </Show>
        <For each={rows()}>
          {(a) => (
            <Show when={a.kind === "audio" && !linked()} fallback={<FileRow a={a} open={viewable(a) ? () => props.onView(a) : undefined} onRemove={remover(a.attachmentId)} />}>
              <AudioRow a={a} onRemove={remover(a.attachmentId)} />
            </Show>
          )}
        </For>
        <For each={cards()}>{(c) => <CardView card={c} onRemove={props.editing ? () => props.editing?.removeCard(c.url) : undefined} />}</For>
      </div>
    </Show>
  );
}

/** What the viewer shows an attachment as (a file is never opened in it). */
const viewKind = (a: AttachmentSummary): "image" | "gif" | "video" | "audio" => (a.kind === "file" ? "image" : a.kind);

/** The viewer over the window for one attachment: its full picture (or video, or audio) fetched, the thumbnail until then. */
export function MediaViewer(props: { a: AttachmentSummary; onClose: () => void }) {
  const main = useBlob(() => keyOf(props.a.main), mainLoad(props.a));
  const thumb = useBlob(
    () => keyOf(props.a.thumbnail),
    () => (props.a.thumbnail ? pictureBlob(props.a.thumbnail) : Promise.resolve(null)),
  );
  const placeholder = useBlob(
    () => (props.a.placeholder ? "ph:" + props.a.attachmentId : undefined),
    () => placeholderBlob(props.a.placeholder),
  );
  return (
    <Viewer
      name={props.a.name || "Attachment"}
      alt={props.a.alt || undefined}
      kind={viewKind(props.a)}
      src={main.url()}
      preview={thumb.url() ?? placeholder.url()}
      width={props.a.width}
      height={props.a.height}
      saveHref={main.url()}
      failed={main.state() === "failed" ? "This can’t be shown here. Save it to open it." : undefined}
      onClose={props.onClose}
    />
  );
}

// ---- The composer's side ----

/** A file in a composer's strip, from added to sent. */
interface PendingFile {
  key: string;
  channelId: string;
  name: string;
  size: number;
  kind: MediaKind;
  /** A blob URL this owns: the picture itself, a poster, a decoded HEIC, the placeholder. */
  preview?: string;
  state: DraftTileProps["state"];
  error?: string;
  alt: string;
  draft?: AttachmentDraft;
}

// Kept for the page's life by conversation, so a draft's files wait in the channel they were added to.
const [pending, setPending] = createSignal<PendingFile[]>([]);
const outcomes = new Map<string, Promise<AttachmentDraft | null>>();
const gone = new Set<string>();
let fileSeq = 0;

function update(key: string, patch: Partial<PendingFile>) {
  setPending((all) => all.map((p) => (p.key === key ? { ...p, ...patch } : p)));
}
const pendingOf = (key: string) => pending().find((p) => p.key === key);

/** The kind a file will most likely be, before the core says (a tile's icon and whether it takes alt text). */
function guessKind(file: File): MediaKind {
  if (file.type === "image/gif" || /\.gif$/i.test(file.name)) return "gif";
  const k = fileKind(file.name, file.type);
  return k === "still" || k === "heic" ? "image" : k === "video" ? "video" : k === "audio" ? "audio" : "file";
}

function once(el: HTMLMediaElement, events: string[], ms: number): Promise<void> {
  return new Promise((resolve, reject) => {
    const done = (ok: boolean) => {
      clearTimeout(timer);
      for (const e of events) el.removeEventListener(e, yes);
      el.removeEventListener("error", no);
      if (ok) resolve();
      else reject(new Error("media"));
    };
    const yes = () => done(true);
    const no = () => done(false);
    const timer = setTimeout(no, ms);
    for (const e of events) el.addEventListener(e, yes);
    el.addEventListener("error", no);
  });
}

/** Pixels drawn on a canvas, at most `max` on the longer side, and the same as a picture for the strip. */
async function pixelsOf(source: CanvasImageSource, width: number, height: number, max: number): Promise<{ pixels: Pixels; preview?: string }> {
  const scale = Math.min(1, max / Math.max(width, height));
  const w = Math.max(1, Math.round(width * scale));
  const h = Math.max(1, Math.round(height * scale));
  const canvas = document.createElement("canvas");
  canvas.width = w;
  canvas.height = h;
  const g = canvas.getContext("2d");
  if (!g) throw new Error("no canvas");
  g.drawImage(source, 0, 0, w, h);
  const data = g.getImageData(0, 0, w, h);
  const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/jpeg", 0.8));
  return { pixels: { rgba: data.data, width: w, height: h }, preview: blob ? URL.createObjectURL(blob) : undefined };
}

/** A video's poster (a frame near 0.1 s), length and size, from a `<video>` (spec 3.3); what the browser cannot play goes without. */
async function videoFacts(file: File): Promise<{ options: Partial<AttachOptions>; preview?: string }> {
  const url = URL.createObjectURL(file);
  const v = document.createElement("video");
  v.muted = true;
  v.playsInline = true;
  v.preload = "auto";
  v.src = url;
  const options: Partial<AttachOptions> = {};
  try {
    await once(v, ["loadeddata"], 8000);
    if (Number.isFinite(v.duration) && v.duration > 0) options.durationMs = Math.round(v.duration * 1000);
    if (v.videoWidth && v.videoHeight) {
      options.videoWidth = v.videoWidth;
      options.videoHeight = v.videoHeight;
    }
    v.currentTime = Math.min(0.1, Number.isFinite(v.duration) && v.duration > 0 ? v.duration / 2 : 0.1);
    await once(v, ["seeked"], 5000);
    if (!v.videoWidth || !v.videoHeight) return { options };
    const drawn = await pixelsOf(v, v.videoWidth, v.videoHeight, 1280);
    options.poster = drawn.pixels;
    return { options, preview: drawn.preview };
  } catch {
    return { options };
  } finally {
    v.removeAttribute("src");
    v.load();
    URL.revokeObjectURL(url);
  }
}

/** An audio's length, from an `<audio>`. */
async function audioLength(file: File): Promise<number | undefined> {
  const url = URL.createObjectURL(file);
  const a = document.createElement("audio");
  a.preload = "metadata";
  a.src = url;
  try {
    await once(a, ["loadedmetadata"], 5000);
    return Number.isFinite(a.duration) && a.duration > 0 ? Math.round(a.duration * 1000) : undefined;
  } catch {
    return undefined;
  } finally {
    a.removeAttribute("src");
    URL.revokeObjectURL(url);
  }
}

/** HEIC and HEIF decoded by the browser where it can (Safari), at most 2,560 on the longer side as the core keeps them; else the bytes go and the core makes a file of them. */
async function decodedPicture(file: File): Promise<{ pixels: Pixels; preview?: string } | undefined> {
  try {
    const bitmap = await createImageBitmap(file);
    try {
      return await pixelsOf(bitmap, bitmap.width, bitmap.height, 2560);
    } finally {
      bitmap.close();
    }
  } catch {
    return undefined;
  }
}

/** Adds files to a conversation's strip; each is made and uploaded at once. Says why some were not taken, or nothing. */
export function attachFiles(channelId: string, files: File[], limits: UploadLimits): string | undefined {
  const room = MAX_ATTACHMENTS - pending().filter((p) => p.channelId === channelId).length;
  for (const file of files.slice(0, Math.max(0, room))) start(channelId, file, limits);
  if (files.length > room) return `A message can carry ${MAX_ATTACHMENTS} attachments, so ${files.length - Math.max(0, room) === 1 ? "one was" : `${files.length - Math.max(0, room)} were`} left out.`;
  return undefined;
}

function start(channelId: string, file: File, limits: UploadLimits) {
  const key = `f${++fileSeq}`;
  const fk = fileKind(file.name, file.type);
  const tooLarge = tooLargeWords(file.size, fk, limits.fileMaxBytes);
  const shown = file.type.startsWith("image/") && fk !== "heic";
  const preview = shown && !tooLarge ? URL.createObjectURL(file) : undefined;
  setPending((all) => [...all, { key, channelId, name: file.name || "Pasted file", size: file.size, kind: guessKind(file), preview, state: tooLarge ? "failed" : "processing", error: tooLarge, alt: "" }]);
  if (tooLarge) {
    outcomes.set(key, Promise.resolve(null));
    return;
  }
  const own = (url: string | undefined) => {
    if (!url) return;
    if (gone.has(key) || pendingOf(key)?.preview) URL.revokeObjectURL(url);
    else update(key, { preview: url });
  };
  const run = async (): Promise<AttachmentDraft | null> => {
    try {
      const options: AttachOptions = { name: file.name || "file" };
      if (fk === "video") {
        const facts = await videoFacts(file);
        Object.assign(options, facts.options);
        own(facts.preview);
      } else if (fk === "audio") {
        const ms = await audioLength(file);
        if (ms) options.durationMs = ms;
      } else if (fk === "heic") {
        const decoded = await decodedPicture(file);
        if (decoded) {
          options.pixels = decoded.pixels;
          own(decoded.preview);
        }
      }
      if (gone.has(key)) return null;
      update(key, { state: "uploading" });
      const bytes = new Uint8Array(await file.arrayBuffer());
      const draft = await client().attach(channelId, bytes, options);
      if (gone.has(key)) {
        void client().discardAttachment(draft.draftId);
        return null;
      }
      const alt = pendingOf(key)?.alt;
      let made = draft;
      if (alt) made = await client().setAttachmentAlt(draft.draftId, alt).catch(() => draft);
      if (!pendingOf(key)?.preview && draft.placeholder) {
        const b = await placeholderBlob(draft.placeholder).catch(() => null);
        if (b) own(URL.createObjectURL(b));
      }
      update(key, { state: "ready", draft: made, kind: made.kind });
      return made;
    } catch (e) {
      if (!gone.has(key)) update(key, { state: "failed", error: attachErrorWords(e instanceof Error ? e.message : String(e), e instanceof ClientError ? e.code : "", limits.fileMaxBytes) });
      return null;
    }
  };
  outcomes.set(key, run());
}

/** Takes a file out of the strip, and its blobs off the instance. */
export function removeFile(key: string) {
  const p = pendingOf(key);
  gone.add(key);
  outcomes.delete(key);
  if (p?.preview) URL.revokeObjectURL(p.preview);
  if (p?.draft) void client().discardAttachment(p.draft.draftId);
  setPending((all) => all.filter((x) => x.key !== key));
}

function setAlt(key: string, alt: string) {
  const p = pendingOf(key);
  update(key, { alt });
  if (p?.draft)
    client()
      .setAttachmentAlt(p.draft.draftId, alt)
      .then((d) => update(key, { draft: d }))
      .catch((e) => console.warn("alt text:", e));
}

/**
 * The files of a conversation's strip at the moment a send is pressed:
 * waits for them to be made and uploaded, then answers their draft ids,
 * or why the message cannot go.
 */
export async function filesForSend(channelId: string): Promise<{ keys: string[]; ids: string[] } | { error: string }> {
  const files = pending().filter((p) => p.channelId === channelId);
  if (files.some((p) => p.state === "failed")) return { error: "Remove the files that could not be attached, then send." };
  const drafts = await Promise.all(files.map((p) => outcomes.get(p.key) ?? Promise.resolve(null)));
  if (drafts.some((d) => !d)) return { error: "Not sent: a file could not be attached." };
  return { keys: files.map((p) => p.key), ids: drafts.map((d) => (d as AttachmentDraft).draftId) };
}

/** The files a send took: out of the strip, their drafts the message's now. */
export function filesSent(keys: string[]) {
  for (const key of keys) {
    const p = pendingOf(key);
    if (p?.preview) URL.revokeObjectURL(p.preview);
    outcomes.delete(key);
  }
  setPending((all) => all.filter((p) => !keys.includes(p.key)));
}

/** Whether a conversation has files waiting to go. */
export function hasFiles(channelId: string | undefined): boolean {
  return !!channelId && pending().some((p) => p.channelId === channelId);
}

/** A conversation's strip as the design system draws it. */
export function stripOf(channelId: string | undefined): DraftTileProps[] {
  return pending()
    .filter((p) => p.channelId === channelId)
    .map((p) => ({
      id: p.key,
      name: p.name,
      size: sizeWords(p.size),
      kind: p.kind,
      preview: p.preview,
      state: p.state,
      error: p.error,
      alt: p.alt,
      onAlt: (alt: string) => setAlt(p.key, alt),
      onRemove: () => removeFile(p.key),
    }));
}

/**
 * The cards of the links being written (spec 6.4): once typing pauses for
 * 500 ms, the links are read and each kept one's card asked of the
 * instance. A server's channel only; a personal conversation's links go
 * without cards from the web (spec 6.5).
 */
export function createCardPreviews(opts: { channelId: () => string | undefined; text: () => string; enabled: () => boolean }) {
  const [links, setLinks] = createSignal<string[]>([]);
  const [dismissed, setDismissed] = createSignal<ReadonlySet<string>>(new Set());
  const [cards, setCards] = createSignal<ReadonlyMap<string, CardSummary | null>>(new Map());
  const asking = new Set<string>();
  let asked = 0;
  const fetchCard = (channelId: string, url: string) => {
    const k = channelId + " " + url;
    if (cards().has(k) || asking.has(k)) return;
    asking.add(k);
    client()
      .cardPreview(channelId, url)
      .then((c) => setCards((m) => new Map(m).set(k, c)))
      // Over the instance's 30 a minute, or not reached: no card now, asked again later.
      .catch((e) => console.warn("card preview:", e))
      .finally(() => asking.delete(k));
  };
  const look = debounce((text: string) => {
    const n = ++asked;
    const channelId = opts.channelId();
    if (!channelId || !opts.enabled() || !text.trim()) return setLinks([]);
    client()
      .cardLinks(text)
      .then((found) => {
        if (n !== asked) return;
        setLinks(found);
        for (const url of keptCardUrls(found, dismissed())) fetchCard(channelId, url);
      })
      .catch((e) => console.warn("card links:", e));
  }, PREVIEW_PAUSE_MS);
  createEffect(on([opts.text, opts.channelId, opts.enabled], ([text]) => look.call(text)));
  createEffect(on(opts.channelId, () => reset(), { defer: true }));
  onCleanup(() => look.cancel());
  const reset = () => {
    asked++;
    look.cancel();
    setLinks([]);
    setDismissed(new Set<string>());
  };
  const shown = createMemo<LinkCardProps[]>(() => {
    const channelId = opts.channelId();
    if (!channelId || !opts.enabled()) return [];
    return keptCardUrls(links(), dismissed()).flatMap((url) => {
      const c = cards().get(channelId + " " + url);
      return c
        ? [
            {
              url: c.url,
              host: c.host,
              title: c.title,
              description: c.description,
              color: c.color,
              image: cardImageSrc(c.imageUrl),
              imageWidth: c.imageWidth,
              imageHeight: c.imageHeight,
              video: c.video,
              onRemove: () => setDismissed((d) => new Set(d).add(url)),
            },
          ]
        : [];
    });
  });
  return {
    cards: shown,
    /** The links whose cards a send of `body` keeps. */
    async urlsFor(body: string): Promise<string[]> {
      if (!opts.enabled()) return [];
      return keptCardUrls(await client().cardLinks(body), dismissed());
    },
    reset,
  };
}

/** Whether a paste is files, and which. */
export function pastedFiles(e: ClipboardEvent): File[] {
  const data = e.clipboardData;
  if (!data) return [];
  const files = Array.from(data.files ?? []);
  return pasteTakesFiles(data.getData("text/plain"), files.map((f) => f.name)) ? files : [];
}

/**
 * Files dragged over the conversation `within` is in: the drop zone drawn
 * over it while they are, and `onFiles` with what is dropped.
 */
export function DropTarget(props: { within: () => HTMLElement | undefined; label: string; onFiles: (files: File[]) => void }) {
  const [rect, setRect] = createSignal<{ left: number; top: number; width: number; height: number } | null>(null);
  onMount(() => {
    const el = props.within()?.closest<HTMLElement>(".lu-shell__conversation") ?? props.within()?.closest<HTMLElement>(".lu-composer")?.parentElement;
    if (!el) return;
    let depth = 0;
    const hasFiles = (e: DragEvent) => !!e.dataTransfer && Array.from(e.dataTransfer.types).includes("Files");
    const enter = (e: DragEvent) => {
      if (!hasFiles(e)) return;
      e.preventDefault();
      depth++;
      const r = el.getBoundingClientRect();
      setRect({ left: r.left, top: r.top, width: r.width, height: r.height });
    };
    const over = (e: DragEvent) => {
      if (!hasFiles(e)) return;
      e.preventDefault();
      if (e.dataTransfer) e.dataTransfer.dropEffect = "copy";
    };
    const leave = (e: DragEvent) => {
      if (!hasFiles(e)) return;
      depth = Math.max(0, depth - 1);
      if (!depth) setRect(null);
    };
    const drop = (e: DragEvent) => {
      if (!hasFiles(e)) return;
      e.preventDefault();
      depth = 0;
      setRect(null);
      const files = Array.from(e.dataTransfer?.files ?? []);
      if (files.length) props.onFiles(files);
    };
    el.addEventListener("dragenter", enter);
    el.addEventListener("dragover", over);
    el.addEventListener("dragleave", leave);
    el.addEventListener("drop", drop);
    onCleanup(() => {
      el.removeEventListener("dragenter", enter);
      el.removeEventListener("dragover", over);
      el.removeEventListener("dragleave", leave);
      el.removeEventListener("drop", drop);
    });
  });
  return <Show when={rect()}>{(r) => <DropZone label={props.label} rect={r()} />}</Show>;
}
