// A conversation's messages as every client draws them since quote
// replies, reactions, mentions, read state, editing and deleting
// (architecture 6.6), shared by a server's channels and server DMs
// (app.tsx) and personal DMs (home.tsx): by day and block, each message
// with the line of what it quotes over it, its mentions as pills named as
// the member list names them, its reactions, its own toolbar (Add
// reaction, Reply, More actions with Edit message, Copy text and Delete
// message), "(edited)", the "New" line, a jump to a quoted message, and
// "Quote reply" beside a selection. A body is the core's blocks
// (`MessageBody`, issue #18), a long one collapsed until it is opened, which
// lasts while the conversation does. A message of this account's is edited
// where it is drawn, and a delete asks first unless Shift is held. The
// composer over a conversation is here too, with the reply bar atop its
// frame, the @ list, the note after a send and the counter near the limit.
import { For, Show, createEffect, createMemo, createSignal, on, onCleanup, onMount, type JSX } from "solid-js";
import {
  ComposerNote,
  Composer,
  ConfirmDelete,
  Divider,
  EmojiPicker,
  Mention,
  MentionList,
  Message,
  MessageActions,
  MessageBody,
  MessageEdit,
  MessageList,
  MessageMenu,
  NoticeLine,
  Popover,
  QuoteReplyButton,
  ReplyBar,
  ReplyRef,
  type MentionOption,
  type ReactionChipProps,
  type Tone,
} from "../design/components";
import { frequentEmoji, usedEmoji } from "../design/emoji";
import { ClientError, type BodyBlock, type MemberSummary, type MentionSummary, type MessageRoom, type QuoteSummary, type ReactionSummary, type ReadStateSummary } from "../client/bridge";
import { avatarSrc, client, markRead } from "../client/state";
import { composing } from "../design/body";
import type { Display } from "../client/preferences";
import { byBlock, byDay, initialsOf, shortTimeOf, timeOf, toneFor } from "./app";

/** One message of a conversation, whichever kind it is. */
export interface TimelineItem {
  id: string;
  seq: number;
  /** Who sent it, as an account id; empty where that is not known. */
  authorId: string;
  /** What a block is one person's by: the account, or in a personal DM the identity. */
  blockKey: string;
  at: number;
  body: string;
  /** The body as the core reads it, which is what is drawn. */
  blocks: BodyBlock[];
  deleted: boolean;
  /** Its author edited it: "(edited)". */
  edited: boolean;
  /** This account wrote it, so it may edit and delete it. */
  mine: boolean;
  /** What a notice says ("Grace joined"); empty for a message. */
  notice: string;
  reply: QuoteSummary | null;
  mentions: MentionSummary[];
  reactions: ReactionSummary[];
  mentionsMe: boolean;
}

/** What is being replied to, over the composer. */
export interface Replying {
  messageId: string;
  authorAccountId: string;
  /** Who wrote it, for the bar; empty where unknown. */
  name: string;
  text: string;
  /**
   * Whether the reply notifies them: the bar's "@ On" switch, on for each
   * reply. Undefined where there is no switch: a reply to oneself, and a
   * personal conversation, whose quote carries no such thing.
   */
  notify?: boolean;
}

/** The quote a reply sends: the text trimmed, at most 2,000 characters, and silent when the switch was turned off. */
export function quoteOf(r: Replying): QuoteSummary {
  return { messageId: r.messageId, authorAccountId: r.authorAccountId, text: Array.from(r.text.trim()).slice(0, 2000).join(""), originalDeleted: false, silent: r.notify === false };
}

/** An edit or a delete the instance refused, said under the message: "Not saved: …". */
export interface Refusal {
  messageId: string;
  text: string;
}

/** Why an edit or a delete was refused, as the line under the message says it. */
export function refusalWords(kind: "edit" | "delete", code: string, message: string): string {
  const head = kind === "edit" ? "Not saved" : "Not deleted";
  if (code === "PERMISSION_DENIED") return `${head}: ${kind === "edit" ? "only its author can edit this message" : "you cannot delete this message"}.`;
  if (code === "NOT_FOUND" || /not_found/i.test(message)) return `${head}: the message is gone.`;
  return `${head}: ${message || code || "something went wrong"}.`.replace(/\.\.$/, ".");
}

/** A refusal thrown by a call (a personal DM's edit or delete), as the line under the message says it. */
export function thrownRefusal(kind: "edit" | "delete", e: unknown): string {
  return refusalWords(kind, e instanceof ClientError ? e.code : "", e instanceof Error ? e.message : String(e));
}

/** A body split at its mentions' UTF-8 byte ranges: the text between, and each mention as written. */
export function bodySegments(body: string, mentions: MentionSummary[]): { text: string; mention?: MentionSummary }[] {
  if (!mentions.length) return [{ text: body }];
  const bytes = new TextEncoder().encode(body);
  const decode = (from: number, to: number) => new TextDecoder().decode(bytes.subarray(from, to));
  const ranges = mentions.filter((m) => m.end > m.start && m.end <= bytes.length).sort((a, b) => a.start - b.start);
  const out: { text: string; mention?: MentionSummary }[] = [];
  let at = 0;
  for (const m of ranges) {
    if (m.start < at) continue;
    if (m.start > at) out.push({ text: decode(at, m.start) });
    out.push({ text: decode(m.start, m.end), mention: m });
    at = m.end;
  }
  if (at < bytes.length) out.push({ text: decode(at, bytes.length) });
  return out;
}

/** How a mention is drawn: `@` and the name the member list shows, or `@everyone`; as written where the person is not known. */
export function mentionWords(written: string, mention: MentionSummary, nameOf: (accountId: string) => string | undefined): string {
  if (mention.everyone || !mention.accountId) return written;
  const name = nameOf(mention.accountId);
  return name ? "@" + name : written;
}

/** A body as it is drawn, mentions by name: what Copy text copies. */
export function drawnText(body: string, mentions: MentionSummary[], nameOf: (accountId: string) => string | undefined): string {
  return bodySegments(body, mentions)
    .map((s) => (s.mention ? mentionWords(s.text, s.mention, nameOf) : s.text))
    .join("");
}

/** "Ada, Bo and 3 others reacted with 👍", "You and Ada reacted with 👍": two names at most, as the desktop shell words it. */
export function reactedWords(r: ReactionSummary, me: string, knownName: (accountId: string) => string | undefined): string {
  const names: string[] = r.me ? ["You"] : [];
  for (const id of r.accountIds) if (id !== me) names.push(knownName(id) ?? "Someone");
  const total = Math.max(r.count, names.length);
  if (!names.length) return `${total === 1 ? "Someone" : total + " people"} reacted with ${r.reaction}`;
  const shown = names.slice(0, 2);
  const others = total - shown.length;
  const who = others > 0 ? `${shown.join(", ")} and ${others} ${others === 1 ? "other" : "others"}` : shown.join(" and ");
  return `${who} reacted with ${r.reaction}`;
}

// Whether the window is in front, for marking read (architecture 6.6).
const [windowFocused, setWindowFocused] = createSignal(typeof document !== "undefined" && document.hasFocus() && document.visibilityState === "visible");
if (typeof window !== "undefined") {
  const update = () => setWindowFocused(document.hasFocus() && document.visibilityState === "visible");
  window.addEventListener("focus", update);
  window.addEventListener("blur", update);
  document.addEventListener("visibilitychange", update);
}

/**
 * Marks the open conversation read whenever its newest message is on
 * screen and the window has focus: at most once a second for it, and only
 * when that moves the position or clears a count.
 */
export function createMarkRead(opts: { channelId: () => string | undefined; newestSeq: () => number | undefined; atEnd: () => boolean; enabled: () => boolean; state: () => ReadStateSummary | undefined }) {
  const last = new Map<string, number>();
  const [tick, setTick] = createSignal(0);
  let timer: ReturnType<typeof setTimeout> | undefined;
  onCleanup(() => clearTimeout(timer));
  createEffect(() => {
    tick();
    const id = opts.channelId();
    const seq = opts.newestSeq();
    if (!id || !seq || !opts.enabled() || !opts.atEnd() || !windowFocused()) return;
    const rs = opts.state();
    if (rs && seq <= rs.readSeq && rs.unread === 0 && rs.mentions === 0) return;
    const since = Date.now() - (last.get(id) ?? 0);
    if (since < 1000) {
      clearTimeout(timer);
      timer = setTimeout(() => setTick((t) => t + 1), 1000 - since);
      return;
    }
    last.set(id, Date.now());
    void markRead(id, seq).catch((e) => console.warn("mark read:", e));
  });
}

/** A draft's room under the instance's limit as the core counts it, asked again as the text changes; the latest answer wins. */
function createRoom(text: () => string): () => MessageRoom | undefined {
  const [room, setRoom] = createSignal<MessageRoom>();
  let asked = 0;
  createEffect(
    on(text, (t) => {
      const n = ++asked;
      client()
        .messageRoom(t)
        .then((r) => n === asked && setRoom(r))
        .catch((e) => console.warn("message room:", e));
    }),
  );
  return room;
}

export interface TimelineProps {
  items: TimelineItem[];
  label: string;
  /** How messages are drawn (Preferences). */
  display: Pick<Display, "layout" | "spacing" | "avatars">;
  me: string;
  authorName: (item: TimelineItem) => string;
  authorPhoto: (item: TimelineItem) => string | undefined;
  tone: (item: TimelineItem) => Tone;
  /** The author's card; none where the author is not known. */
  onOpenProfile?: (item: TimelineItem, anchor: HTMLElement) => void;
  /** Someone's card, from a mention of them. */
  openPerson: (accountId: string, anchor: HTMLElement) => void;
  /** An account's name where it is known (the quote's author, who reacted, who a mention names). */
  knownName: (accountId: string) => string | undefined;
  /** An account's picture, for the line of what a reply quotes. */
  photoOf: (accountId: string) => string | undefined;
  /** The conversation's read state when it opened (null until known): the "New" line goes above the first message after it. */
  readState: () => ReadStateSummary | undefined | null;
  /** False where nothing can be said (a channel read before joining): no toolbars, no chips to press. */
  canAct: boolean;
  onReply: (item: TimelineItem, text: string) => void;
  /** The message being replied to: its row keeps its fill and its toolbar, with Reply pressed, which cancels. */
  replyingTo?: string;
  onCancelReply: () => void;
  onReact: (item: TimelineItem, emoji: string, remove: boolean) => void;
  /** Whether this account may delete a message: its own, or another's as a moderator (the instance decides either way). */
  canDelete: (item: TimelineItem) => boolean;
  /** The message being edited where it is drawn, and the way in and out of it. */
  editing?: string;
  onEditing: (id: string | undefined) => void;
  /** Saves an edit: the new body, trimmed and not empty. */
  onEdit: (item: TimelineItem, body: string) => void;
  onDelete: (item: TimelineItem) => void;
  /** An edit or a delete refused, said under its message. */
  refused?: Refusal | null;
  /** A message to jump to as soon as it can be found (from a Mentions page). */
  jumpTo?: string;
  onJumped?: () => void;
  /** One more page of older messages, for a jump to one not loaded; false when there are none. */
  loadOlder?: () => Promise<boolean>;
  onAtEnd: (atEnd: boolean) => void;
  /** Notices at the top of the log (a personal DM's). */
  before?: JSX.Element;
  after?: JSX.Element;
}

const JUMP_PAGES = 5;
/** How near the oldest message shown, in pixels, the reader gets before the page before it is read. */
const OLDER_WITHIN = 600;

export function Timeline(props: TimelineProps) {
  let scroller: HTMLDivElement | undefined;
  // Where reading starts: the read position when the conversation opened, kept until it is left.
  const [newFrom, setNewFrom] = createSignal<number | undefined | null>(null);
  createEffect(() => {
    if (newFrom() !== null) return;
    const rs = props.readState();
    if (rs === null) return;
    setNewFrom(rs && rs.unread > 0 ? rs.readSeq : undefined);
  });
  // Above the first unread message: after the read position, someone else's, not a notice (as the count counts).
  const firstNew = createMemo(() => {
    const from = newFrom();
    if (from === null || from === undefined) return undefined;
    return props.items.find((m) => m.seq > from && !m.notice && m.authorId !== props.me)?.id;
  });
  const days = createMemo(() =>
    byDay(props.items, (m) => m.at).map((day) => ({
      label: day.label,
      blocks: byBlock(
        day.messages,
        // A notice has no one, so it joins no block and opens none.
        (m) => (m.notice ? "" : m.blockKey),
        (m) => m.at,
        // A quote starts a block with its own name, and so does the first new message, under its line.
        (m) => !!m.reply || m.id === firstNew(),
      ),
    })),
  );

  // A jump: scrolled to, loading older pages where it must, and flashed.
  const [flash, setFlash] = createSignal<string | undefined>();
  let flashTimer: ReturnType<typeof setTimeout> | undefined;
  onCleanup(() => clearTimeout(flashTimer));
  const find = (id: string) => scroller?.querySelector<HTMLElement>(`[data-message-id="${CSS.escape(id)}"]`) ?? null;
  const frame = () => new Promise((resolve) => requestAnimationFrame(() => resolve(undefined)));
  const jump = async (id: string): Promise<boolean> => {
    for (let page = 0; ; page++) {
      const el = find(id);
      if (el) {
        el.scrollIntoView({ block: "center" });
        setFlash(id);
        clearTimeout(flashTimer);
        flashTimer = setTimeout(() => setFlash(undefined), 1000);
        return true;
      }
      if (page >= JUMP_PAGES || !props.loadOlder || !(await props.loadOlder())) return false;
      await frame();
    }
  };
  createEffect(
    on([() => props.jumpTo, () => props.items.length > 0], ([id, loaded]) => {
      if (!id || !loaded) return;
      void frame().then(() => jump(id).finally(() => props.onJumped?.()));
    }),
  );

  // Reading up to the oldest messages shown asks for the page before them: the store's, then the
  // instance's. The list is bottom-anchored, so what comes in above leaves the view where it is.
  let older = false;
  const nearTop = () => {
    if (older || !props.loadOlder || !scroller) return;
    if (scroller.scrollHeight - scroller.clientHeight + scroller.scrollTop > OLDER_WITHIN) return;
    older = true;
    void props.loadOlder().finally(() => (older = false));
  };

  // At its newest end: the list is bottom-anchored (column-reverse), so scrolled to the end is 0.
  const atEnd = () => !scroller || Math.abs(scroller.scrollTop) < 4;
  // Opened with something unread above the fold: at the "New" line, else at the end.
  let positioned = false;
  createEffect(
    on([() => props.items.length > 0, newFrom], ([loaded, from]) => {
      if (positioned || !loaded || from === null) return;
      positioned = true;
      void frame().then(() => {
        const line = scroller?.querySelector<HTMLElement>(".lu-divider--new");
        if (line && scroller && line.getBoundingClientRect().top < scroller.getBoundingClientRect().top) line.scrollIntoView({ block: "start" });
        props.onAtEnd(atEnd());
      });
    }),
  );

  // "Quote reply" beside text selected inside one message's body.
  const byId = createMemo(() => new Map(props.items.map((m) => [m.id, m])));
  const [selected, setSelected] = createSignal<{ left: number; top: number; item: TimelineItem; text: string } | null>(null);
  const onSelection = () => {
    const sel = document.getSelection();
    // Not in a field (an edit): its selection is its own.
    if (!props.canAct || !sel || sel.isCollapsed || !sel.anchorNode || !sel.focusNode || !scroller || document.activeElement instanceof HTMLTextAreaElement) return setSelected(null);
    const bodyOf = (n: Node) => (n instanceof Element ? n : n.parentElement)?.closest(".lu-msg__body") ?? null;
    const body = bodyOf(sel.anchorNode);
    if (!body || body !== bodyOf(sel.focusNode) || !scroller.contains(body)) return setSelected(null);
    const id = body.closest("[data-message-id]")?.getAttribute("data-message-id");
    const item = id ? byId().get(id) : undefined;
    const text = sel.toString().trim();
    if (!item || !text) return setSelected(null);
    // At the end the pointer is at: the focus.
    const at = document.createRange();
    at.setStart(sel.focusNode, sel.focusOffset);
    at.collapse(true);
    const rects = at.getClientRects();
    const r = rects[rects.length - 1] ?? sel.getRangeAt(0).getBoundingClientRect();
    setSelected({ left: Math.min(r.right + 4, window.innerWidth - 140), top: r.bottom + 6, item, text });
  };
  onMount(() => {
    document.addEventListener("selectionchange", onSelection);
    onCleanup(() => document.removeEventListener("selectionchange", onSelection));
  });

  // The picker, against what opened it.
  const [picking, setPicking] = createSignal<{ anchor: HTMLElement; item: TimelineItem } | null>(null);
  const react = (item: TimelineItem, emoji: string, remove: boolean) => {
    if (!remove) usedEmoji(emoji);
    props.onReact(item, emoji, remove);
  };
  // More actions' menu, and the question a delete asks, each against what opened it.
  const [menu, setMenu] = createSignal<{ anchor: HTMLElement; item: TimelineItem } | null>(null);
  const [confirming, setConfirming] = createSignal<{ anchor: HTMLElement | null; item: TimelineItem } | null>(null);
  const drawn = (m: TimelineItem) => drawnText(m.body, m.mentions, props.knownName);
  const askDelete = (item: TimelineItem, anchor: HTMLElement | null, now: boolean) => {
    if (now) props.onDelete(item);
    else setConfirming({ anchor, item });
  };
  const save = async (m: TimelineItem, text: string) => {
    const body = text.trim();
    // Asked of the core for this very text, before the field goes: a counter answered for an
    // earlier text must not let an edit over the limit close and be lost.
    if (body && (await client().messageRoom(body)).left < 0) return;
    props.onEditing(undefined);
    // Nothing left is a delete, which asks first.
    if (!body) return askDelete(m, find(m.id), false);
    if (body !== m.body.trim()) props.onEdit(m, body);
  };
  // The message taken up for editing comes into view (Up in the composer edits one that may be off screen).
  // Its text is counted against the limit as it changes, from the body it opens with.
  const [editText, setEditText] = createSignal("");
  // What is counted is what is saved: the text without its blanks at either end.
  const editRoom = createRoom(() => editText().trim());
  createEffect(
    on(
      () => props.editing,
      (id) => {
        setEditText(id ? (byId().get(id)?.body ?? "") : "");
        if (id) void frame().then(() => find(id)?.scrollIntoView({ block: "nearest" }));
      },
    ),
  );
  // The long bodies opened, while the conversation is open.
  const [expanded, setExpanded] = createSignal<ReadonlySet<string>>(new Set());
  const expand = (id: string, open: boolean) =>
    setExpanded((was) => {
      const now = new Set(was);
      if (open) now.add(id);
      else now.delete(id);
      return now;
    });

  const reference = (m: TimelineItem) => (
    <Show when={m.reply}>
      {(q) => {
        const author = () => q().authorAccountId;
        const name = () => (author() ? props.knownName(author()) : undefined);
        return (
          <ReplyRef
            name={name()}
            initials={initialsOf(name() ?? "")}
            src={author() ? props.photoOf(author()) : undefined}
            tone={toneFor(author())}
            self={!!author() && author() === props.me}
            text={q().text}
            deleted={q().originalDeleted}
            onOpen={() => void jump(q().messageId)}
          />
        );
      }}
    </Show>
  );
  const body = (m: TimelineItem) => (
    <Show
      when={props.editing === m.id && !m.deleted}
      fallback={
        <>
          <Show when={!m.deleted} fallback={<p>(deleted)</p>}>
            <MessageBody
              blocks={m.blocks}
              mention={(index, typed) => {
                const mention = m.mentions[index];
                return (
                  <Show when={mention} fallback={<Mention>{typed}</Mention>}>
                    {(mention) => <Mention onOpen={mention().everyone || !mention().accountId ? undefined : (anchor) => props.openPerson(mention().accountId, anchor)}>{mentionWords(typed, mention(), props.knownName)}</Mention>}
                  </Show>
                );
              }}
              edited={m.edited}
              expanded={expanded().has(m.id)}
              onExpanded={(open) => expand(m.id, open)}
            />
          </Show>
          <Show when={props.refused?.messageId === m.id ? props.refused : undefined}>
            {(r) => (
              <p class="lu-msg__refused" role="alert">
                {r().text}
              </p>
            )}
          </Show>
        </>
      }
    >
      <MessageEdit value={m.body} onSave={(text) => save(m, text)} onCancel={() => props.onEditing(undefined)} onText={setEditText} room={editRoom()} />
    </Show>
  );
  const chips = (m: TimelineItem): ReactionChipProps[] =>
    m.reactions
      .filter((r) => r.count > 0)
      .map((r) => ({
        emoji: r.reaction,
        label: r.reaction,
        count: r.count,
        selected: r.me,
        who: reactedWords(r, props.me, props.knownName),
        onToggle: props.canAct ? () => react(m, r.reaction, r.me) : undefined,
      }));
  const tools = (m: TimelineItem) => (
    <Show when={props.canAct && !m.deleted && props.editing !== m.id}>
      <MessageActions
        onReact={(anchor) => setPicking({ anchor, item: m })}
        onReply={() => (props.replyingTo === m.id ? props.onCancelReply() : props.onReply(m, m.body))}
        replying={props.replyingTo === m.id}
        moreOpen={menu()?.item.id === m.id}
        onMore={(anchor) => setMenu({ anchor, item: m })}
      />
    </Show>
  );
  const adder = (m: TimelineItem) => (props.canAct ? (anchor: HTMLElement) => setPicking({ anchor, item: m }) : undefined);

  return (
    <>
      <MessageList
        label={props.label}
        layout={props.display.layout}
        spacing={props.display.spacing}
        avatars={props.display.avatars}
        scrollRef={(el) => (scroller = el)}
        onScroll={() => {
          props.onAtEnd(atEnd());
          if (selected()) onSelection();
          nearTop();
        }}
      >
        {props.before}
        <For each={days()}>
          {(day) => (
            <>
              <Divider variant="day" label={day.label} />
              <For each={day.blocks}>
                {(block) => {
                  const first = block.first;
                  const t = timeOf(first.at);
                  return (
                    <>
                      <Show when={first.id === firstNew()}>
                        <Divider variant="new" />
                      </Show>
                      <Show
                        when={!first.notice}
                        fallback={
                          <NoticeLine time={t.time} datetime={t.datetime}>
                            {first.notice}
                          </NoticeLine>
                        }
                      >
                        <Message
                          author={props.authorName(first)}
                          initials={initialsOf(props.authorName(first))}
                          src={props.authorPhoto(first)}
                          tone={props.tone(first)}
                          time={t.time}
                          datetime={t.datetime}
                          actions={false}
                          layout={props.display.layout}
                          avatars={props.display.avatars}
                          onOpenProfile={props.onOpenProfile && first.authorId ? (anchor) => props.onOpenProfile?.(first, anchor) : undefined}
                          id={first.id}
                          reference={first.reply ? reference(first) : undefined}
                          reactions={chips(first)}
                          onAddReaction={adder(first)}
                          tools={tools(first)}
                          mentioned={first.mentionsMe}
                          flash={flash() === first.id}
                          replying={props.replyingTo === first.id}
                          parts={block.rest.map((m) => ({
                            // One line shows it whole; grouped, in the avatar's column, without its "AM" or "PM".
                            time: props.display.layout === "one_line" ? timeOf(m.at).time : shortTimeOf(m.at),
                            datetime: timeOf(m.at).datetime,
                            children: body(m),
                            id: m.id,
                            get reactions() {
                              return chips(m);
                            },
                            onAddReaction: adder(m),
                            tools: tools(m),
                            mentioned: m.mentionsMe,
                            get flash() {
                              return flash() === m.id;
                            },
                            get replying() {
                              return props.replyingTo === m.id;
                            },
                          }))}
                        >
                          {body(first)}
                        </Message>
                      </Show>
                    </>
                  );
                }}
              </For>
            </>
          )}
        </For>
        {props.after}
      </MessageList>
      <Show when={selected()}>
        {(s) => (
          <QuoteReplyButton
            left={s().left}
            top={s().top}
            onPress={() => {
              props.onReply(s().item, s().text);
              document.getSelection()?.removeAllRanges();
              setSelected(null);
            }}
          />
        )}
      </Show>
      {/* Keyed: the popover gives the focus back to its anchor as it closes, after the signal has gone. */}
      <Show when={picking()} keyed>
        {(p) => (
          <Popover anchor={p.anchor} onClose={() => setPicking(null)} closeLabel="Close the emoji">
            <EmojiPicker
              frequent={frequentEmoji()}
              onPick={(emoji) => {
                const item = p.item;
                setPicking(null);
                const has = item.reactions.some((r) => r.reaction === emoji && r.me);
                react(item, emoji, has);
              }}
            />
          </Popover>
        )}
      </Show>
      <Show when={menu()} keyed>
        {(open) => (
          <Popover anchor={open.anchor} onClose={() => setMenu(null)} closeLabel="Close the menu">
            <MessageMenu
              onEdit={
                open.item.mine
                  ? () => {
                      setMenu(null);
                      props.onEditing(open.item.id);
                    }
                  : undefined
              }
              onCopy={() => {
                setMenu(null);
                void navigator.clipboard?.writeText(drawn(open.item)).catch(() => {});
              }}
              onDelete={
                props.canDelete(open.item)
                  ? (now) => {
                      setMenu(null);
                      askDelete(open.item, open.anchor, now);
                    }
                  : undefined
              }
            />
          </Popover>
        )}
      </Show>
      <Show when={confirming()} keyed>
        {(c) => (
          <Popover anchor={c.anchor} onClose={() => setConfirming(null)} closeLabel="Keep the message">
            <ConfirmDelete
              text={drawn(c.item).split("\n")[0] ?? ""}
              onCancel={() => setConfirming(null)}
              onDelete={() => {
                setConfirming(null);
                props.onDelete(c.item);
              }}
            />
          </Popover>
        )}
      </Show>
    </>
  );
}

/** Who a mention can name in a server's channel, from the server's members. */
export interface MentionSource {
  members: MemberSummary[];
  /** Who is in the channel. */
  inChannel: Set<string>;
  /** The channel's name, without its hash. */
  channel: string;
  canEveryone: boolean;
}

/** The `@word` being typed at the caret, if any: where it starts and what follows the `@`. */
function mentionAt(text: string, caret: number): { start: number; query: string } | null {
  const m = /(^|\s)@([\p{L}\p{N}_.-]*)$/u.exec(text.slice(0, caret));
  const query = m?.[2];
  if (query === undefined) return null;
  return { start: caret - query.length - 1, query };
}

export interface ComposeProps {
  placeholder: string;
  disabled?: boolean;
  typing?: string[];
  value: string;
  onInput: (text: string) => void;
  onSend: () => void;
  replying: Replying | null;
  onCancelReply: () => void;
  /** The reply bar's "@ On" switch turned. */
  onReplyNotify?: (on: boolean) => void;
  /** Up in the empty field: this account's newest message here, taken up for editing. */
  onEditLast?: () => void;
  /** The @ list's people; none in a DM. */
  mentions?: MentionSource;
  /** What a send left to say, over the field. */
  note?: JSX.Element;
  inputRef?: (el: HTMLTextAreaElement) => void;
}

const MENTION_LIST_MAX = 8;
let listIds = 0;

/**
 * The composer over a conversation: the reply bar atop its frame, the @
 * list over it, and a note after a send. Return sends and Shift+Return
 * adds a line; near the limit the counter shows, and over it nothing is
 * sent and the draft stays.
 */
export function Compose(props: ComposeProps) {
  let input: HTMLTextAreaElement | undefined;
  // What is counted is what is sent: the draft without its blanks at either end.
  const room = createRoom(() => props.value.trim());
  const over = () => (room()?.left ?? 0) < 0;
  // A counter answered for an earlier text can let one over the limit through: the core
  // refuses it then, and the send puts the draft back with why.
  const send = () => {
    if (!over()) props.onSend();
  };
  const listId = `lu-mentions-${++listIds}`;
  const [at, setAt] = createSignal<{ start: number; query: string } | null>(null);
  const [active, setActive] = createSignal(0);
  const look = () => {
    if (!input || !props.mentions) return setAt(null);
    setAt(mentionAt(input.value, input.selectionStart ?? input.value.length));
    setActive(0);
  };
  const options = createMemo<MentionOption[]>(() => {
    const a = at();
    const src = props.mentions;
    if (!a || !src) return [];
    const q = a.query.toLowerCase();
    const starts = (s: string) => !!s && s.toLowerCase().startsWith(q);
    const nameOf = (m: MemberSummary) => m.displayName || m.nickname || m.handle || m.accountId.slice(0, 8);
    const people = src.members
      .filter((m) => starts(m.handle) || starts(m.displayName) || starts(m.nickname))
      .sort((x, y) => Number(src.inChannel.has(y.accountId)) - Number(src.inChannel.has(x.accountId)) || nameOf(x).localeCompare(nameOf(y)))
      .slice(0, MENTION_LIST_MAX)
      .map(
        (m): MentionOption => ({
          id: m.accountId,
          name: nameOf(m),
          handle: m.handle,
          initials: initialsOf(nameOf(m)),
          src: avatarSrc(m.avatarUrl),
          tone: toneFor(m.accountId),
          note: src.inChannel.has(m.accountId) ? undefined : `Not in #${src.channel}`,
        }),
      );
    const everyone = src.canEveryone && "everyone".startsWith(q) ? [{ id: "everyone", name: "everyone", everyone: true, note: `Notify everyone in #${src.channel}` }] : [];
    return [...everyone, ...people];
  });
  const open = () => options().length > 0;
  const pick = (o: MentionOption) => {
    const a = at();
    if (!a || !input) return;
    const caret = input.selectionStart ?? input.value.length;
    const word = `@${o.everyone ? "everyone" : o.handle} `;
    const text = input.value.slice(0, a.start) + word + input.value.slice(caret);
    props.onInput(text);
    setAt(null);
    const to = a.start + word.length;
    queueMicrotask(() => {
      input?.focus();
      input?.setSelectionRange(to, to);
    });
  };
  return (
    <Composer
      placeholder={props.placeholder}
      tools={false}
      disabled={props.disabled}
      typing={props.typing}
      value={props.value}
      room={room()}
      inputRef={(el) => {
        input = el;
        props.inputRef?.(el);
      }}
      aria-controls={open() ? listId : undefined}
      aria-autocomplete={props.mentions ? "list" : undefined}
      aria-activedescendant={open() ? `${listId}-${active()}` : undefined}
      onInput={(e) => {
        props.onInput(e.currentTarget.value);
        look();
      }}
      onClick={look}
      onBlur={() => setAt(null)}
      onKeyDown={(e) => {
        if (open()) {
          if (e.key === "ArrowDown" || e.key === "ArrowUp") {
            e.preventDefault();
            const n = options().length;
            setActive((i) => (i + (e.key === "ArrowDown" ? 1 : n - 1)) % n);
            return;
          }
          if ((e.key === "Enter" || e.key === "Tab") && !composing(e)) {
            e.preventDefault();
            const o = options()[active()];
            if (o) pick(o);
            return;
          }
          if (e.key === "Escape") {
            e.preventDefault();
            setAt(null);
            return;
          }
        }
        if (e.key === "Escape" && props.replying) {
          e.preventDefault();
          props.onCancelReply();
          return;
        }
        if (e.key === "ArrowUp" && !e.currentTarget.value && props.onEditLast && !composing(e)) {
          e.preventDefault();
          props.onEditLast();
          return;
        }
        if (e.key === "Enter" && !e.shiftKey && !composing(e)) {
          e.preventDefault();
          send();
        }
      }}
      onKeyUp={(e) => {
        if (e.key === "ArrowLeft" || e.key === "ArrowRight" || e.key === "Home" || e.key === "End") look();
      }}
      onSend={send}
      hint={props.replying ? "Return to send · Esc to cancel the reply" : undefined}
      above={
        <>
          <Show when={open()}>
            <MentionList id={listId} options={options()} active={active()} onActive={setActive} onPick={pick} />
          </Show>
          {/* A textarea cannot be a combobox, so nothing says the list opened: this does. */}
          <span class="lu-sr" aria-live="polite">
            {open() ? `${options().length} to mention, Up and Down to choose, Return to pick` : ""}
          </span>
          {props.note}
        </>
      }
      bar={
        props.replying ? (
          <ReplyBar
            name={props.replying.name || undefined}
            mention={props.replying.notify}
            onMention={(on) => props.onReplyNotify?.(on)}
            onCancel={() => {
              props.onCancelReply();
              input?.focus();
            }}
          />
        ) : undefined
      }
    />
  );
}

/** Who the note after a send names: "Ada", "Ada and Bo", "Ada, Bo and 2 others". */
function namesWords(names: string[]): string {
  if (names.length <= 1) return names[0] ?? "";
  if (names.length === 2) return `${names[0]} and ${names[1]}`;
  return `${names[0]}, ${names[1]} and ${names.length - 2} ${names.length - 2 === 1 ? "other" : "others"}`;
}

/** "Ada isn’t in #design, so they won’t be notified." */
export function notNotifiedWords(names: string[], channel: string): string {
  return `${namesWords(names)} ${names.length === 1 ? "isn’t" : "aren’t"} in #${channel}, so they won’t be notified.`;
}

/** "Invited Ada to #design.", "Invited 2 people to #design." */
export function invitedWords(names: string[], channel: string): string {
  return names.length === 1 ? `Invited ${names[0]} to #${channel}.` : `Invited ${names.length} people to #${channel}.`;
}

export { ComposerNote };
