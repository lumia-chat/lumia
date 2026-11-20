// A server's own emoji on screen (specs/emoji.md, plan step 12): each as
// the picture the instance made, through the address its `imageUrl` names,
// an animated one playing except where the system asks for reduced motion,
// where its first frame is drawn instead (made here, once a picture, from
// the GIF itself); and Server settings' Emoji section, for whoever holds
// Manage emoji: the count against the limit, Upload emoji (a picture from
// the system's picker, its name made by the core from the file's, to change
// before Save), the list with Rename in place and Remove, asked first.
import { Show, createEffect, createMemo, createSignal, on, onCleanup, untrack } from "solid-js";
import { Button, ConfirmPop, EmojiUpload, Popover, ServerEmojiList, type CustomEmojiEntry, type ServerEmojiEntry } from "../design/components";
import type { EmojiSummary, MemberSummary, ServerSummary, SnapshotSummary } from "../client/bridge";
import { EMOJI_UPLOAD_MAX, NAME_RULE, REMOVE_QUESTION, TOO_LARGE, emojiCount, emojiFull, emojiRefusal, emojiSubtitle, removeTitle, sizeWords } from "../client/emoji";
import { avatarSrc, client, queryClient } from "../client/state";
import { memberName } from "./roles";
import { pictureRetryIn } from "../design/emoji";

// Whether the system asks for reduced motion, followed as it changes.
const motion = typeof matchMedia === "function" ? matchMedia("(prefers-reduced-motion: reduce)") : null;
const [reduced, setReduced] = createSignal(!!motion?.matches);
motion?.addEventListener("change", () => setReduced(motion.matches));

// The first frames made, by picture address, as Blob URLs; the addresses asked for; and how
// many times each that could not be made has been tried.
const [stills, setStills] = createSignal<ReadonlyMap<string, string>>(new Map());
const making = new Set<string>();
const stillTries = new Map<string, number>();

/**
 * An animated picture's first frame as a still picture, made once an
 * address: the GIF fetched (the instance serves it to this origin) and
 * decoded as a bitmap, which is its first frame, drawn on a canvas.
 * Undefined until it is made, so nothing moves where motion was asked to
 * stop (the emoji is its name meanwhile); one that could not be made is
 * tried again later, each time later than the last.
 */
function stillOf(src: string): string | undefined {
  const made = stills().get(src);
  if (made || making.has(src)) return made;
  making.add(src);
  void (async () => {
    // From the network: the picture the page drew was fetched without CORS, and an instance
    // that names no `Vary: Origin` on it would have the cache answer this with that copy.
    const answer = await fetch(src, { mode: "cors", credentials: "omit", cache: "reload" });
    if (!answer.ok) throw new Error(`${answer.status} for ${src}`);
    const blob = await answer.blob();
    const frame = await createImageBitmap(blob);
    const canvas = document.createElement("canvas");
    canvas.width = frame.width;
    canvas.height = frame.height;
    canvas.getContext("2d")?.drawImage(frame, 0, 0);
    frame.close();
    const png = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/png"));
    if (!png) throw new Error(`no still made of ${src}`);
    if (!making.has(src)) return;
    stillTries.delete(src);
    const url = URL.createObjectURL(png);
    setStills((was) => new Map(was).set(src, url));
  })().catch((e) => {
    console.warn("emoji still:", e);
    const tries = (stillTries.get(src) ?? 0) + 1;
    stillTries.set(src, tries);
    setTimeout(() => {
      // Let go of meanwhile (no server lists it): nothing to ask again.
      if (!making.has(src)) return;
      making.delete(src);
      // Whoever draws it reads `stills` again, and so asks for it again.
      setStills((was) => new Map(was));
    }, pictureRetryIn(tries));
  });
  return undefined;
}

/** Lets go of the first frames made for pictures no server lists any longer (removed emoji): `listed` is every picture address still listed. */
export function forgetStills(listed: ReadonlySet<string>): void {
  for (const src of [...making]) if (!listed.has(src)) making.delete(src);
  for (const src of [...stillTries.keys()]) if (!listed.has(src)) stillTries.delete(src);
  const kept = stills();
  if ([...kept.keys()].every((src) => listed.has(src))) return;
  const next = new Map<string, string>();
  for (const [src, url] of kept) {
    if (listed.has(src)) next.set(src, url);
    else URL.revokeObjectURL(url);
  }
  setStills(next);
}

/** The address a picture is drawn from: the picture itself, or under reduced motion an animated one's first frame. */
function drawnSrc(e: Pick<EmojiSummary, "imageUrl" | "animated">): string | undefined {
  const src = avatarSrc(e.imageUrl);
  if (!src || !e.animated || !reduced()) return src;
  return stillOf(src);
}

/** One of a server's emoji as the components draw it, its picture's address following reduced motion as it changes. */
export function pictureOf(e: EmojiSummary): CustomEmojiEntry {
  return {
    id: e.emojiId,
    name: e.name,
    width: e.width,
    height: e.height,
    animated: e.animated,
    get src() {
      return drawnSrc(e);
    },
  };
}

/** A server's emoji by id, as drawn: what a message's refs and reactions are looked up in. None for a conversation that is no server's. */
export function heldEmoji(list: () => EmojiSummary[] | undefined): () => ReadonlyMap<string, CustomEmojiEntry> {
  return createMemo(() => new Map((list() ?? []).map((e) => [e.emojiId, pictureOf(e)])));
}

/** Server settings' Emoji section (the "Emoji settings" board). */
export function EmojiSection(props: { server: ServerSummary; members: MemberSummary[] | undefined; took: (snap: SnapshotSummary) => void }) {
  const serverId = createMemo(() => props.server.serverId);
  const count = () => props.server.emoji.length;
  const full = () => emojiFull(count(), props.server.emojiLimit);
  const byName = createMemo(() => new Map((props.members ?? []).map((m) => [m.accountId, memberName(m)])));
  const entries = createMemo<ServerEmojiEntry[]>(() =>
    props.server.emoji.map((e) => {
      const p = pictureOf(e);
      return {
        id: p.id,
        name: p.name,
        width: p.width,
        height: p.height,
        animated: p.animated,
        get src() {
          return p.src;
        },
        // Until the members are read, nobody in particular; one who has left since, so.
        by: byName().get(e.createdByAccountId) ?? (props.members ? "a former member" : "someone"),
      };
    }),
  );
  const [error, setError] = createSignal("");

  // The upload form: open once a picture is chosen, over the list.
  let input: HTMLInputElement | undefined;
  interface Upload {
    file: File;
    url: string;
    width: number;
    height: number;
    name: string;
    nameError?: string;
    pictureError?: string;
    busy?: boolean;
  }
  const [upload, setUpload] = createSignal<Upload | null>(null);
  const letGo = (u: Upload | null) => u && URL.revokeObjectURL(u.url);
  onCleanup(() => letGo(untrack(upload)));
  const choose = async (file: File | undefined) => {
    if (!file) return;
    const url = URL.createObjectURL(file);
    const shape = await new Promise<{ width: number; height: number }>((resolve) => {
      const img = new Image();
      img.onload = () => resolve({ width: img.naturalWidth || 1, height: img.naturalHeight || 1 });
      img.onerror = () => resolve({ width: 1, height: 1 });
      img.src = url;
    });
    const name = await client()
      .emojiNameFromFile(file.name)
      .catch(() => "");
    setError("");
    setUpload((was) => {
      letGo(was);
      return { file, url, ...shape, name, pictureError: file.size > EMOJI_UPLOAD_MAX ? TOO_LARGE : undefined };
    });
  };
  const save = async () => {
    const u = upload();
    if (!u || u.busy) return;
    const name = u.name.trim();
    if (u.file.size > EMOJI_UPLOAD_MAX) return setUpload({ ...u, pictureError: TOO_LARGE });
    if (!(await client().isEmojiName(name.toLowerCase()))) return setUpload({ ...u, nameError: NAME_RULE, pictureError: undefined });
    setUpload({ ...u, busy: true, nameError: undefined, pictureError: undefined });
    try {
      const bytes = new Uint8Array(await u.file.arrayBuffer());
      props.took(await client().uploadEmoji(props.server.serverId, name, bytes, u.file.type));
      setUpload(null);
      letGo(u);
    } catch (err) {
      const r = emojiRefusal(err, props.server.name, name.toLowerCase());
      setUpload((now) => (now ? { ...now, busy: false, nameError: r.on === "name" ? r.text : undefined, pictureError: r.on === "picture" ? r.text : undefined } : now));
      if (r.on === "section") setError(r.text);
    }
  };

  // A row renamed in place.
  const [renaming, setRenaming] = createSignal<{ id: string; value: string; error?: string; busy?: boolean } | null>(null);
  const rename = async () => {
    const r = renaming();
    if (!r || r.busy) return;
    const was = props.server.emoji.find((e) => e.emojiId === r.id);
    const name = r.value.trim().toLowerCase();
    if (!was) return setRenaming(null);
    if (name === was.name) return setRenaming(null);
    if (!(await client().isEmojiName(name))) return setRenaming({ ...r, error: NAME_RULE });
    setRenaming({ ...r, busy: true, error: undefined });
    try {
      props.took(await client().renameEmoji(props.server.serverId, r.id, name));
      setRenaming(null);
    } catch (err) {
      setRenaming((now) => (now ? { ...now, busy: false, error: emojiRefusal(err, props.server.name, name).text } : now));
    }
  };

  // Remove, asked first against its button.
  const [asking, setAsking] = createSignal<{ emoji: ServerEmojiEntry; anchor: HTMLElement } | null>(null);
  const remove = async (id: string) => {
    setAsking(null);
    setError("");
    try {
      props.took(await client().removeEmoji(props.server.serverId, id));
    } catch (err) {
      setError(emojiRefusal(err, props.server.name, "").text);
    }
  };

  // Another server is another page; an emoji renamed or removed elsewhere while its row is open closes it.
  createEffect(
    on(
      serverId,
      () => {
        setUpload((was) => {
          letGo(was);
          return null;
        });
        setRenaming(null);
        setAsking(null);
        setError("");
      },
      { defer: true },
    ),
  );
  createEffect(() => {
    const r = renaming();
    if (r && !props.server.emoji.some((e) => e.emojiId === r.id)) setRenaming(null);
  });

  return (
    <>
      <div class="lu-settings__head">
        <div class="lu-settings__headtext">
          <h2 class="lu-settings__title">
            Emoji <span class="lu-settings__count">{emojiCount(count(), props.server.emojiLimit)}</span>
          </h2>
          <p class="lu-settings__subtitle">{emojiSubtitle(props.server.name, count(), props.server.emojiLimit)}</p>
        </div>
        <Button size="sm" icon="plus" disabled={full() || !!upload()?.busy} onClick={() => input?.click()}>
          Upload emoji
        </Button>
      </div>
      <input
        ref={input}
        type="file"
        accept="image/png,image/jpeg,image/webp,image/gif"
        class="lu-sr"
        tabindex={-1}
        aria-label="Emoji picture"
        onChange={(e) => {
          const file = e.currentTarget.files?.[0];
          // The same file chosen again is a choice too.
          e.currentTarget.value = "";
          void choose(file);
        }}
      />
      <Show when={error()}>
        <p class="auth__error" role="alert">
          {error()}
        </p>
      </Show>
      <Show when={upload()}>
        {(u) => (
          <EmojiUpload
            picture={{ name: u().name || "emoji", src: u().url, width: u().width, height: u().height }}
            file={u().file.name}
            size={sizeWords(u().file.size)}
            name={u().name}
            nameError={u().nameError}
            pictureError={u().pictureError}
            busy={u().busy}
            onChoose={() => input?.click()}
            onName={(name) => setUpload((now) => (now ? { ...now, name, nameError: undefined } : now))}
            onCancel={() =>
              setUpload((was) => {
                letGo(was);
                return null;
              })
            }
            onSave={() => void save()}
          />
        )}
      </Show>
      <Show when={entries().length > 0}>
        <ServerEmojiList
          emoji={entries()}
          renaming={renaming() ?? undefined}
          onRename={(e) => {
            setRenaming({ id: e.id, value: e.name });
            // The keyboard into the name, where the row now asks for it.
            requestAnimationFrame(() => document.querySelector<HTMLInputElement>(".lu-emojis__item.is-renaming input")?.focus());
          }}
          onRemove={(emoji, anchor) => setAsking({ emoji, anchor })}
          onRenameInput={(value) => setRenaming((r) => (r ? { ...r, value, error: undefined } : r))}
          onRenameSave={() => void rename()}
          onRenameCancel={() => setRenaming(null)}
        />
      </Show>
      <Show when={asking()}>
        {(a) => {
          // Read once: the keyboard goes back to the row's button after the question has gone.
          const { emoji, anchor } = a();
          return (
            <Popover anchor={anchor} onClose={() => setAsking(null)}>
              <ConfirmPop title={removeTitle(emoji.name)} confirm="Remove" danger onCancel={() => setAsking(null)} onConfirm={() => void remove(emoji.id)}>
                <p>{REMOVE_QUESTION}</p>
              </ConfirmPop>
            </Popover>
          );
        }}
      </Show>
    </>
  );
}
