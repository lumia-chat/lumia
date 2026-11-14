// The Lumia design system's components, ported to Solid from the design
// artifact's React source on 2026-11-02 (assets/README.md). Same class
// names, same markup, same ARIA; the stylesheet is components.css and every
// value in it comes from tokens.css.
//
// Solid conventions: props are read lazily (never destructured), `class`
// instead of `className`, `for` instead of `htmlFor`, `style` objects with
// CSS property names.

import { For, Show, children, createEffect, createMemo, createSignal, createUniqueId, on, onCleanup, onMount, splitProps, type JSX } from "solid-js";
import { Dynamic, Portal } from "solid-js/web";
import { ICONS } from "./icons";
import { emojiMatches, loadEmoji, type EmojiGroup } from "./emoji";

export type Tone = "peach" | "blue" | "purple" | "mint" | "amber" | "sky" | "stone" | "green" | "indigo";
export type Status = "online" | "away" | "dnd";
export type IconName = keyof typeof ICONS;
export interface Person {
  initials: string;
  tone?: Tone;
  /** The person's picture; the initials stand in without one. */
  src?: string;
}

const STATUS_WORD: Record<Status, string> = { online: "online", away: "away", dnd: "do not disturb" };

function cx(...a: (string | false | null | undefined)[]): string {
  return a.filter(Boolean).join(" ");
}

/**
 * A row or a tile that is a link in the boards and a command in the app:
 * with no address of its own the press is the handler's alone, so the
 * page's address and history are left as they were.
 */
function pressed(e: MouseEvent, item: { href?: string; onClick?: () => void }) {
  if (!item.href) e.preventDefault();
  item.onClick?.();
}

function fieldId(id?: string): string {
  const auto = createUniqueId();
  return id ?? "lu-" + auto.replace(/:/g, "");
}

/* ── Foundations ─────────────────────────────────────────── */

export interface IconProps {
  name: IconName;
  size?: number;
  strokeWidth?: number;
  label?: string;
  class?: string;
  style?: JSX.CSSProperties;
}

/** A stroke icon in currentColor. */
export function Icon(props: IconProps) {
  const def = () => ICONS[props.name];
  return (
    <Show when={def()}>
      {(d) => {
        const [sw, filled, els] = d();
        return (
          <svg
            width={props.size ?? 18}
            height={props.size ?? 18}
            viewBox="0 0 24 24"
            ref={(el) => el.setAttribute("focusable", "false")}
            class={cx("lu-icon", props.class)}
            style={props.style}
            fill={filled ? "currentColor" : "none"}
            stroke={filled ? "none" : "currentColor"}
            stroke-width={filled ? undefined : props.strokeWidth ?? sw}
            stroke-linecap={filled ? undefined : "round"}
            stroke-linejoin={filled ? undefined : "round"}
            role={props.label ? "img" : undefined}
            aria-label={props.label}
            aria-hidden={props.label ? undefined : "true"}
          >
            <For each={els}>{([tag, attrs]) => <Dynamic component={tag as "path"} {...attrs} />}</For>
          </svg>
        );
      }}
    </Show>
  );
}

/* ── Actions ─────────────────────────────────────────────── */

export interface ButtonProps extends JSX.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "secondary" | "text";
  size?: "lg" | "md" | "sm";
  icon?: IconName;
  block?: boolean;
}

/** Text button. One primary per view. */
export function Button(props: ButtonProps) {
  const [local, rest] = splitProps(props, ["variant", "size", "icon", "block", "class", "children", "type"]);
  const variant = () => local.variant ?? "secondary";
  const sz = () => (variant() === "text" ? null : local.size ?? (variant() === "primary" ? "lg" : "md"));
  return (
    <button
      type={local.type ?? "button"}
      class={cx("lu-btn", "lu-btn--" + variant(), sz() && "lu-btn--" + sz(), local.block && "lu-btn--block", local.class)}
      {...rest}
    >
      <Show when={local.icon}>{(icon) => <Icon name={icon()} size={17} />}</Show>
      {local.children}
    </button>
  );
}

const ICON_FOR_SIZE: Record<number, number> = { 28: 15, 30: 17, 32: 16, 34: 16, 36: 18, 38: 18, 40: 17, 44: 22 };

export interface IconButtonProps extends JSX.ButtonHTMLAttributes<HTMLButtonElement> {
  icon?: IconName;
  glyph?: string;
  label: string;
  size?: 28 | 30 | 32 | 34 | 36 | 38 | 40 | 44;
  iconSize?: number;
  strokeWidth?: number;
  variant?: "ghost" | "send";
  pressed?: boolean;
}

/** Icon-only button; `label` is its accessible name. */
export function IconButton(props: IconButtonProps) {
  const [local, rest] = splitProps(props, ["icon", "glyph", "label", "size", "iconSize", "strokeWidth", "variant", "pressed", "class", "type"]);
  const size = () => local.size ?? 36;
  const isSend = () => local.variant === "send";
  const iconSize = () => local.iconSize ?? (isSend() && size() === 32 ? 14 : ICON_FOR_SIZE[size()] ?? 18);
  const sw = () => local.strokeWidth ?? (isSend() ? (size() === 32 ? 2.4 : 2.2) : undefined);
  return (
    <button
      type={local.type ?? "button"}
      aria-label={local.label}
      aria-pressed={local.pressed}
      class={cx("lu-iconbtn", "lu-iconbtn--" + (local.variant ?? "ghost"), "lu-iconbtn--" + size(), local.glyph && "lu-iconbtn--glyph", local.class)}
      {...rest}
    >
      <Show when={local.glyph} fallback={<Show when={local.icon}>{(icon) => <Icon name={icon()} size={iconSize()} strokeWidth={sw()} />}</Show>}>
        <span aria-hidden="true">{local.glyph}</span>
      </Show>
    </button>
  );
}

export interface ReactionChipProps {
  /** A board's glyph for the reaction; in the product the reaction is an emoji, drawn as text. */
  icon?: IconName;
  emoji?: string;
  label: string;
  /** Who reacted, as the tooltip and the accessible name: "Ada and Bo reacted with 👍". */
  who?: string;
  count: number;
  selected?: boolean;
  defaultSelected?: boolean;
  onToggle?: (on: boolean) => void;
  class?: string;
}

/** One reaction and its count. */
export function ReactionChip(props: ReactionChipProps) {
  const [inner, setInner] = createSignal(!!props.defaultSelected);
  const controlled = () => props.selected !== undefined;
  const on = () => (controlled() ? !!props.selected : inner());
  const shown = () => (controlled() ? props.count : props.count - (props.defaultSelected ? 1 : 0) + (on() ? 1 : 0));
  return (
    <button
      type="button"
      aria-pressed={on()}
      aria-label={props.who}
      title={props.who}
      class={cx("lu-reaction", on() && "is-on", props.class)}
      onClick={() => {
        const next = !on();
        if (!controlled()) setInner(next);
        props.onToggle?.(next);
      }}
    >
      <Show when={props.emoji} fallback={<Show when={props.icon}>{(icon) => <Icon name={icon()} size={14} strokeWidth={2} />}</Show>}>
        <span class="lu-reaction__emoji" aria-hidden="true">
          {props.emoji}
        </span>
      </Show>
      <Show when={!props.who}>
        <span class="lu-sr">{props.label},</span>
      </Show>
      <span>{shown()}</span>
    </button>
  );
}

/* ── Forms ───────────────────────────────────────────────── */

export interface TextFieldProps extends JSX.InputHTMLAttributes<HTMLInputElement> {
  label: string;
  reveal?: boolean;
  action?: { label: string; href?: string; onClick?: (e: MouseEvent) => void };
}

/** Labelled 46px input with optional action link and password reveal. */
export function TextField(props: TextFieldProps) {
  const [local, input] = splitProps(props, ["label", "id", "type", "reveal", "action", "class"]);
  const fid = fieldId(local.id);
  const [shown, setShown] = createSignal(false);
  const type = () => (local.reveal ? (shown() ? "text" : "password") : local.type ?? "text");
  return (
    <div class={cx("lu-textfield", local.class)}>
      <div class="lu-textfield__top">
        <label for={fid} class="lu-textfield__label">
          {local.label}
        </label>
        <Show when={local.action}>
          {(action) => (
            <a href={action().href ?? "#"} onClick={(e) => action().onClick?.(e)} class="lu-link lu-textfield__action">
              {action().label}
            </a>
          )}
        </Show>
      </div>
      <div class={cx("lu-textfield__box", local.reveal && "has-reveal")}>
        <input id={fid} type={type()} class="lu-textfield__input" {...input} />
        <Show when={local.reveal}>
          <button type="button" class="lu-textfield__reveal" aria-label="Show password" aria-pressed={shown()} onClick={() => setShown(!shown())}>
            <Icon name={shown() ? "eye-off" : "eye"} size={18} />
          </button>
        </Show>
      </div>
    </div>
  );
}

export interface SearchFieldProps extends JSX.InputHTMLAttributes<HTMLInputElement> {
  label?: string;
}

/** Compact search input. */
export function SearchField(props: SearchFieldProps) {
  const [local, input] = splitProps(props, ["label", "placeholder", "id", "class"]);
  const fid = fieldId(local.id);
  const label = () => local.label ?? "Search";
  return (
    <div class={cx("lu-searchfield", local.class)}>
      <label for={fid} class="lu-sr">
        {label()}
      </label>
      <div class="lu-searchfield__box">
        <Icon name="search" size={15} />
        <input id={fid} type="search" placeholder={local.placeholder ?? label()} autocomplete="off" class="lu-searchfield__input" {...input} />
      </div>
    </div>
  );
}

export interface SearchTriggerProps extends JSX.ButtonHTMLAttributes<HTMLButtonElement> {
  label?: string;
  shortcut?: string;
}

/** Button that opens global search. */
export function SearchTrigger(props: SearchTriggerProps) {
  const [local, rest] = splitProps(props, ["label", "shortcut", "class"]);
  const shortcut = () => local.shortcut ?? "⌘K";
  return (
    <button type="button" class={cx("lu-searchtrigger", local.class)} {...rest}>
      <Icon name="search" size={15} />
      <span class="lu-searchtrigger__label">{local.label ?? "Search"}</span>
      <Show when={shortcut()}>
        <kbd class="lu-searchtrigger__kbd">{shortcut()}</kbd>
      </Show>
    </button>
  );
}

export interface CheckboxProps extends JSX.InputHTMLAttributes<HTMLInputElement> {
  label: string;
  size?: "md" | "sm";
}

/** Native checkbox with label. */
export function Checkbox(props: CheckboxProps) {
  const [local, input] = splitProps(props, ["label", "id", "size", "class"]);
  const fid = fieldId(local.id);
  return (
    <div class={cx("lu-check", "lu-check--" + (local.size ?? "md"), local.class)}>
      <input id={fid} type="checkbox" class="lu-check__box" {...input} />
      <label for={fid} class="lu-check__label">
        {local.label}
      </label>
    </div>
  );
}

export interface TypingIndicatorProps {
  /** Who is typing: one name, or each of them. */
  name: string | string[];
  class?: string;
}

/** "Name is typing…" with three dots; two people by name, "Several people" beyond that. */
export function TypingIndicator(props: TypingIndicatorProps) {
  const names = () => (Array.isArray(props.name) ? props.name : [props.name]);
  return (
    <span class={cx("lu-typing", props.class)}>
      <span class="lu-typing__dots" aria-hidden="true">
        <span></span>
        <span></span>
        <span></span>
      </span>
      <span class="lu-typing__text">
        <Show when={names().length <= 2} fallback="Several people are typing…">
          <strong>{names()[0]}</strong>
          <Show when={names().length === 2} fallback=" is typing…">
            {" and "}
            <strong>{names()[1]}</strong> are typing…
          </Show>
        </Show>
      </span>
    </span>
  );
}

export interface ComposerProps extends JSX.InputHTMLAttributes<HTMLInputElement> {
  placeholder?: string;
  label?: string;
  /** Who is typing, named under the field. */
  typing?: string | string[];
  hint?: string;
  /** The attach, formatting, emoji and mention buttons; a screen with nothing behind them leaves them out. */
  tools?: boolean;
  onSend?: () => void;
  onAttach?: () => void;
  /** What stands over the field: a note after sending, the @ list (which floats over the conversation). */
  above?: JSX.Element;
  /** The strip atop the field's own frame (the reply bar), which then has the active border. */
  bar?: JSX.Element;
  inputRef?: (el: HTMLInputElement) => void;
}

/** Message bar with tools, send and a hint row. */
export function Composer(props: ComposerProps) {
  const [local, input] = splitProps(props, ["placeholder", "label", "typing", "hint", "tools", "id", "onSend", "onAttach", "class", "above", "bar", "inputRef"]);
  const fid = fieldId(local.id);
  const placeholder = () => local.placeholder ?? "Message";
  const hint = () => local.hint ?? "Return to send";
  const tools = () => local.tools ?? true;
  const typing = () => (Array.isArray(local.typing) ? (local.typing.length ? local.typing : undefined) : local.typing || undefined);
  // Read once: the frame's class and its contents both ask.
  const bar = children(() => local.bar);
  return (
    <div class={cx("lu-composer", local.class)}>
      {local.above}
      {/* Always the same frame, so the field keeps its focus as a bar comes and goes. */}
      <div class={cx("lu-composer__frame", !!bar() && "has-bar")}>
        {bar()}
        <div class={cx("lu-composer__field", !tools() && "is-bare")}>
          <Show when={tools()}>
            <IconButton icon="plus" label="Attach a file" class="lu-composer__attach" onClick={() => local.onAttach?.()} />
          </Show>
          <label for={fid} class="lu-sr">
            {local.label ?? placeholder()}
          </label>
          <input ref={(el) => local.inputRef?.(el)} id={fid} type="text" placeholder={placeholder()} autocomplete="off" class="lu-composer__input" {...input} />
          <Show when={tools()}>
            <IconButton glyph="Aa" label="Text formatting" />
            <IconButton icon="emoji" label="Emoji" />
            <IconButton icon="mention" label="Mention someone" strokeWidth={1.9} />
          </Show>
          <IconButton icon="send" variant="send" size={40} label="Send message" class="lu-composer__send" onClick={() => local.onSend?.()} />
        </div>
      </div>
      <Show when={typing() || hint()}>
        <div class="lu-composer__hint" aria-live="polite">
          <Show when={typing()} fallback={<span></span>}>
            {(name) => <TypingIndicator name={name()} />}
          </Show>
          <Show when={hint()}>
            <span>{hint()}</span>
          </Show>
        </div>
      </Show>
    </div>
  );
}

/* ── People ──────────────────────────────────────────────── */

const AV_FONT: Record<number, number> = { 15: 6.5, 22: 9, 28: 10, 34: 12.5, 38: 13, 40: 14, 44: 14 };
const AV_DOT: Record<number, number> = { 22: 9, 28: 10, 34: 12, 38: 12, 40: 12, 44: 12 };

/** A person's picture filling its circle, or their initials where there is none or it will not load. */
function Face(props: { initials: string; src?: string }) {
  const [broken, setBroken] = createSignal(false);
  // A new picture gets its own chance.
  createEffect(on(() => props.src, () => setBroken(false), { defer: true }));
  return (
    <Show when={props.src && !broken()} fallback={props.initials}>
      <img class="lu-avatar__photo" src={props.src} alt="" decoding="async" draggable={false} onError={() => setBroken(true)} />
    </Show>
  );
}

export interface AvatarProps {
  initials: string;
  /** The person's picture; the initials stand in without one. */
  src?: string;
  tone?: Tone;
  size?: 22 | 28 | 34 | 38 | 40 | 44;
  status?: Status | "offline";
  ring?: "surface" | "bg" | "rail";
  label?: string;
  class?: string;
  style?: JSX.CSSProperties;
}

/** Initials on a tone, with status. */
export function Avatar(props: AvatarProps) {
  const size = () => props.size ?? 40;
  const dot = () => AV_DOT[size()] ?? 12;
  return (
    <span
      class={cx("lu-avatar", "lu-tone-" + (props.tone ?? "blue"), props.class)}
      style={{ width: size() + "px", height: size() + "px", "font-size": (AV_FONT[size()] ?? Math.round(size() * 0.35)) + "px", ...props.style }}
      role={props.label ? "img" : undefined}
      aria-label={props.label}
      aria-hidden={props.label ? undefined : "true"}
    >
      <Face initials={props.initials} src={props.src} />
      <Show when={props.status}>
        {(status) => <span class={cx("lu-avatar__status", "lu-status-" + status(), "lu-ring-" + (props.ring ?? "surface"))} style={{ width: dot() + "px", height: dot() + "px" }}></span>}
      </Show>
    </span>
  );
}

export interface AvatarStackProps {
  people: Person[];
  size?: number;
  ring?: "surface" | "bg" | "rail";
  class?: string;
}

/** Overlapping small avatars. */
export function AvatarStack(props: AvatarStackProps) {
  const size = () => props.size ?? 22;
  return (
    <span class={cx("lu-avstack", props.class)} aria-hidden="true">
      <For each={props.people}>
        {(p) => (
          <span class={cx("lu-avatar", "lu-tone-" + (p.tone ?? "blue"), "lu-ring-" + (props.ring ?? "surface"))} style={{ width: size() + "px", height: size() + "px", "font-size": (size() <= 22 ? 8 : 10) + "px" }}>
            {p.initials}
          </span>
        )}
      </For>
    </span>
  );
}

export interface BadgeProps extends JSX.HTMLAttributes<HTMLSpanElement> {
  count?: number;
  dot?: boolean;
  size?: "md" | "sm";
  ring?: "surface" | "bg" | "rail";
  label?: string;
}

/** Count pill or unread dot. */
export function Badge(props: BadgeProps) {
  const [local, rest] = splitProps(props, ["count", "dot", "size", "ring", "label", "class"]);
  return (
    <Show
      when={!local.dot}
      fallback={<span aria-hidden="true" class={cx("lu-dot", local.ring && "lu-ring-" + local.ring, local.class)} {...rest}></span>}
    >
      <span aria-label={local.label} class={cx("lu-badge", "lu-badge--" + (local.size ?? "md"), local.ring && "lu-ring-" + local.ring, local.class)} {...rest}>
        {local.count}
      </span>
    </Show>
  );
}

export interface TagProps {
  tone?: "neutral" | "accent";
  size?: "md" | "sm";
  children?: JSX.Element;
  class?: string;
}

/** Role or New label. */
export function Tag(props: TagProps) {
  return <span class={cx("lu-tag", "lu-tag--" + (props.tone ?? "neutral"), "lu-tag--" + (props.size ?? "md"), props.class)}>{props.children}</span>;
}

export interface MemberEntry {
  name: string;
  initials: string;
  src?: string;
  tone?: Tone;
  status?: Status;
  subtitle?: string;
  /** A second line under the subtitle, such as a bio. */
  detail?: string;
  tag?: string;
  /** Whatever names the person to the screen; handed back by `onOpen`. */
  id?: string;
  /** Something to do to them, at the row's end: Remove, drawn as the dark Quiet Split's member-remove button (`label` is its tooltip, "Remove from channel"; `description` names whom). */
  action?: { label: string; description: string; disabled?: boolean; onClick: () => void };
}

export interface MemberListProps {
  /** A section with no title is a list with nothing over it. */
  sections: { title?: string; members: MemberEntry[] }[];
  /** How many there are when the sections show only some: draws "View all". */
  total?: number;
  onViewAll?: () => void;
  /** A row was pressed; the row is what a profile card opens against. */
  onOpen?: (member: MemberEntry, row: HTMLElement) => void;
  /** What to say when no section has anyone. */
  empty?: string;
  class?: string;
}

/** Members grouped by presence. */
export function MemberList(props: MemberListProps) {
  return (
    <div class={cx("lu-members", props.class)}>
      <For each={props.sections}>
        {(sec, i) => (
          <>
            <Show when={sec.title}>
              <h3 class={cx("lu-members__title", i() > 0 && "is-later")}>{sec.title}</h3>
            </Show>
            <ul class="lu-members__list">
              <For each={sec.members}>
                {(m) => (
                  <li>
                    <button type="button" aria-haspopup={props.onOpen ? "dialog" : undefined} onClick={(e) => props.onOpen?.(m, e.currentTarget)} class={cx("lu-member", m.status === "away" && "is-away")}>
                      <Avatar initials={m.initials} src={m.src} tone={m.tone} size={34} status={m.status} ring="surface" />
                      <span class="lu-member__text">
                        <span class="lu-member__name">
                          <span class="lu-member__label">{m.name}</span>
                          <Show when={m.status}>{(s) => <span class="lu-sr">, {STATUS_WORD[s()]}</span>}</Show>
                          <Show when={m.tag}>
                            <Tag tone="accent" size="sm">
                              {m.tag}
                            </Tag>
                          </Show>
                        </span>
                        <Show when={m.subtitle}>
                          <span class="lu-member__sub">{m.subtitle}</span>
                        </Show>
                        <Show when={m.detail}>
                          <span class="lu-member__sub">{m.detail}</span>
                        </Show>
                      </span>
                    </button>
                    <Show when={m.action}>
                      {(a) => (
                        <button type="button" class="lu-member__action" title={a().label} disabled={a().disabled} aria-label={a().description} onClick={() => a().onClick()}>
                          <Icon name="member-remove" size={16} />
                        </button>
                      )}
                    </Show>
                  </li>
                )}
              </For>
            </ul>
          </>
        )}
      </For>
      <Show when={props.empty && props.sections.every((sec) => sec.members.length === 0)}>
        <p class="lu-members__none">{props.empty}</p>
      </Show>
      <Show when={props.total}>
        <Button size="sm" block class="lu-members__all" onClick={() => props.onViewAll?.()}>
          View all {props.total} members
        </Button>
      </Show>
    </div>
  );
}

/* ── Navigation ──────────────────────────────────────────── */

export interface RailServer {
  /** What its menu and a drag name it by; without one it neither drags nor has a menu. */
  id?: string;
  name: string;
  initials: string;
  /** The server's picture; its initials stand in without one, or while it fails to load. */
  src?: string;
  current?: boolean;
  ink?: "default" | "indigo" | "green" | "amber";
  unread?: boolean;
  mentions?: number;
  onClick?: () => void;
}

/** The nine tones, in the order the core hashes an id over (a folder's tone is one of them). */
export const TONES: Tone[] = ["peach", "blue", "purple", "mint", "amber", "sky", "stone", "green", "indigo"];

export interface RailFolder {
  id: string;
  /** Its name, or how it reads without one ("Acme, Lumia and 2 more"). */
  label: string;
  tone: Tone;
  /** Open, it is its glyph and then its servers in a column of its tone. */
  open?: boolean;
  servers: RailServer[];
  onToggle?: () => void;
}

/** A tile between Home and +: a server, or a folder of servers (architecture 6.9). */
export type RailTile = ({ kind: "server" } & RailServer) | ({ kind: "folder" } & RailFolder);

/**
 * Where a dragged tile would land: between tiles, on the rail or in an
 * open folder (`folder`), before `before` or at the end; or onto a tile,
 * a server's or a folder's (`folder`).
 */
export type RailDrop = { kind: "between"; folder?: string; before?: string } | { kind: "onto"; id: string; folder: boolean };

export interface ServerRailProps {
  logoSrc: string;
  homeLabel?: string;
  homeUnread?: number;
  /** Home is the current place (no server is). */
  homeCurrent?: boolean;
  /** Servers alone, in order; `items` draws folders too. */
  servers?: RailServer[];
  items?: RailTile[];
  /** A tile's menu was asked for (right-click, Shift+F10, the context menu key). */
  onMenu?: (id: string, anchor: HTMLElement) => void;
  /** A tile was dragged and let go somewhere it can go; without this nothing drags. */
  onDrop?: (id: string, drop: RailDrop) => void;
  /** What the last move did, said politely to assistive technology. */
  announce?: string;
  /** A drag held still, for the gallery: the tile lifted, where it would land, and its ghost's place in the rail. */
  still?: { id: string; drop: RailDrop | null; ghost: { left: number; top: number } };
  /**
   * The signed-in person at the foot of the rail. `offline` is the dot of
   * someone who is not connected; `menuOpen` says the button's menu is up;
   * `hint` is what hovering it tells.
   */
  self?: { name: string; initials: string; src?: string; tone?: Tone; status?: Status | "offline"; menuOpen?: boolean; hint?: string; onClick?: (button: HTMLElement) => void };
  onHome?: () => void;
  onAddServer?: () => void;
  class?: string;
}

/** ", 3 mentions", ", unread messages" or nothing: what a tile's label adds. */
function railCounts(mentions: number | undefined, unread: boolean | undefined): string {
  return mentions ? ", " + mentions + (mentions === 1 ? " mention" : " mentions") : unread ? ", unread messages" : "";
}

// How far apart the rail's tiles are, halved: where a line between two goes.
const RAIL_HALF_GAP = 6;

type RailDrag = { id: string; kind: "server" | "folder"; x: number; y: number; grabX: number; grabY: number; drop: RailDrop | null; line: number | null };

/** The far-left server rail: Home, the servers and folders as the account arranged them, +, and the account. */
export function ServerRail(props: ServerRailProps) {
  const homeLabel = () => props.homeLabel ?? "Lumia Home — personal messages";
  const items = (): RailTile[] => props.items ?? (props.servers ?? []).map((s) => ({ kind: "server" as const, ...s }));
  let nav: HTMLElement | undefined;
  const [drag, setDrag] = createSignal<RailDrag | null>(null);
  const [stillLine, setStillLine] = createSignal<number | null>(null);

  const tileEl = (id: string) => nav?.querySelector<HTMLElement>(`[data-rail-tile="${id}"]`) ?? null;
  const itemEl = (id: string) => nav?.querySelector<HTMLElement>(`[data-rail-item="${id}"]`) ?? null;
  // Where a line for `drop` goes, from the rail's top; none for a drop onto a tile.
  const lineFor = (drop: RailDrop | null): number | null => {
    if (!nav || !drop || drop.kind !== "between") return null;
    const top = nav.getBoundingClientRect().top;
    if (drop.before) {
      const el = drop.folder ? tileEl(drop.before) : itemEl(drop.before);
      return el ? el.getBoundingClientRect().top - RAIL_HALF_GAP - top : null;
    }
    const level = [...nav.querySelectorAll<HTMLElement>(drop.folder ? `[data-rail-in="${drop.folder}"]` : "[data-rail-item]")];
    const last = level[level.length - 1];
    return last ? last.getBoundingClientRect().bottom + RAIL_HALF_GAP - top : null;
  };
  // Where a tile dragged to (x, y) would land. Over a tile, its top or bottom
  // quarter is between and its middle half onto; a folder goes only between
  // the rail's own items. Over itself, or off the rail, nowhere.
  const dropAt = (x: number, y: number, d: RailDrag): RailDrop | null => {
    if (!nav) return null;
    const r = nav.getBoundingClientRect();
    if (x < r.left - 24 || x > r.right + 24 || y < r.top || y > r.bottom) return null;
    const folderDrag = d.kind === "folder";
    const els = [...nav.querySelectorAll<HTMLElement>(folderDrag ? "[data-rail-item]" : "[data-rail-tile]")];
    const idOf = (el: HTMLElement) => (folderDrag ? el.dataset.railItem : el.dataset.railTile) ?? "";
    const el = els.find((e) => y < e.getBoundingClientRect().bottom + RAIL_HALF_GAP);
    if (!el) return els.length ? { kind: "between" } : null;
    const id = idOf(el);
    if (id === d.id) return null;
    const b = el.getBoundingClientRect();
    const rel = (y - b.top) / b.height;
    const kind = el.dataset.railKind;
    const inFolder = folderDrag ? undefined : el.dataset.railIn;
    if (!folderDrag && rel >= 0.25 && rel <= 0.75) return { kind: "onto", id, folder: kind === "folder" };
    if (folderDrag ? rel < 0.5 : rel < 0.25) return { kind: "between", folder: inFolder, before: id };
    // Under an open folder's glyph: its first place.
    if (!folderDrag && el.dataset.railOpen) {
      const first = nav.querySelector<HTMLElement>(`[data-rail-in="${id}"]`);
      return { kind: "between", folder: id, before: first?.dataset.railTile };
    }
    const level = els.filter((e) => (folderDrag ? true : (e.dataset.railIn ?? "") === (inFolder ?? "")));
    const next = level[level.indexOf(el) + 1];
    return { kind: "between", folder: inFolder, before: next ? idOf(next) : undefined };
  };

  let press: { id: string; kind: "server" | "folder"; x: number; y: number; el: HTMLElement } | null = null;
  // The press that ends a drag is not a click on the tile it began on.
  const swallowClick = () => {
    const swallow = (e: MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
    };
    window.addEventListener("click", swallow, true);
    setTimeout(() => window.removeEventListener("click", swallow, true), 0);
  };
  const onKey = (e: KeyboardEvent) => {
    if (e.key !== "Escape") return;
    e.preventDefault();
    e.stopPropagation();
    end();
    // Cancelled, the button still comes up: over the tile, that is no click either.
    window.addEventListener("pointerup", swallowClick, { capture: true, once: true });
  };
  const onMove = (e: PointerEvent) => {
    if (!press) return;
    let d = drag();
    if (!d) {
      if (Math.hypot(e.clientX - press.x, e.clientY - press.y) < 5) return;
      const b = press.el.getBoundingClientRect();
      d = { id: press.id, kind: press.kind, x: e.clientX, y: e.clientY, grabX: press.x - b.left, grabY: press.y - b.top, drop: null, line: null };
      document.addEventListener("keydown", onKey, true);
    }
    const drop = dropAt(e.clientX, e.clientY, d);
    setDrag({ ...d, x: e.clientX, y: e.clientY, drop, line: lineFor(drop) });
  };
  const onUp = () => {
    const d = drag();
    end();
    if (!d) return;
    swallowClick();
    if (d.drop) props.onDrop?.(d.id, d.drop);
  };
  const end = () => {
    press = null;
    setDrag(null);
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
    window.removeEventListener("pointercancel", end);
    document.removeEventListener("keydown", onKey, true);
  };
  onCleanup(end);
  onMount(() => {
    // The gallery's still drag: its line once the tiles are laid out.
    if (props.still) requestAnimationFrame(() => setStillLine(lineFor(props.still?.drop ?? null)));
  });
  const down = (e: PointerEvent & { currentTarget: HTMLElement }, id: string | undefined, kind: "server" | "folder") => {
    if (!id || !props.onDrop || props.still || e.button !== 0 || e.pointerType === "touch") return;
    press = { id, kind, x: e.clientX, y: e.clientY, el: e.currentTarget };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", end);
  };
  // A tile's menu: right-click, Shift+F10 or the context menu key.
  const menuKeys = (e: KeyboardEvent & { currentTarget: HTMLElement }, id: string | undefined) => {
    if (!id || !props.onMenu || !((e.key === "F10" && e.shiftKey) || e.key === "ContextMenu")) return;
    e.preventDefault();
    props.onMenu(id, e.currentTarget);
  };
  const menuClick = (e: MouseEvent & { currentTarget: HTMLElement }, id: string | undefined) => {
    if (!id || !props.onMenu) return;
    e.preventDefault();
    props.onMenu(id, e.currentTarget);
  };

  const shown = () => (props.still ? { id: props.still.id, drop: props.still.drop } : drag());
  const lifted = (id: string | undefined) => !!id && shown()?.id === id;
  const onto = (id: string | undefined) => {
    const d = shown()?.drop;
    return !!id && d?.kind === "onto" && d.id === id;
  };
  const line = () => (props.still ? stillLine() : drag()?.line ?? null);
  const find = (id: string): RailTile | undefined => {
    for (const t of items()) {
      if (t.id === id) return t;
      if (t.kind === "folder") {
        const s = t.servers.find((x) => x.id === id);
        if (s) return { kind: "server", ...s };
      }
    }
    return undefined;
  };

  const serverTile = (s: RailServer, folder?: string) => (
    <button
      type="button"
      aria-label={s.name + railCounts(s.mentions, s.unread)}
      aria-current={s.current ? "true" : undefined}
      onClick={() => s.onClick?.()}
      onPointerDown={(e) => down(e, s.id, "server")}
      onContextMenu={(e) => menuClick(e, s.id)}
      onKeyDown={(e) => menuKeys(e, s.id)}
      data-rail-tile={s.id}
      data-rail-kind="server"
      data-rail-in={folder}
      class={cx("lu-rail__server", s.current && "is-current", s.src && "has-photo", "lu-tile-ink-" + (s.ink ?? "default"), lifted(s.id) && "is-lifted", onto(s.id) && "is-onto")}
    >
      <Face initials={s.initials} src={s.src} />
      <Show when={s.mentions} fallback={<Show when={s.unread}><Badge dot ring="rail" class="lu-rail__dot" /></Show>}>
        <Badge count={s.mentions} ring="rail" aria-hidden="true" class="lu-rail__badge" />
      </Show>
    </button>
  );
  const minis = (f: RailFolder) => (
    <For each={f.servers.slice(0, 4)}>
      {(s) => (
        <span class={cx("lu-rail__mini", "lu-tile-ink-" + (s.ink ?? "default"))}>
          <Face initials={s.initials} src={s.src} />
        </span>
      )}
    </For>
  );
  const folderTile = (f: RailFolder) => {
    const mentions = () => f.servers.reduce((n, s) => n + (s.mentions ?? 0), 0);
    const unread = () => f.servers.some((s) => s.unread);
    return (
      <button
        type="button"
        aria-label={"Folder, " + f.label + railCounts(mentions(), unread())}
        aria-expanded="false"
        onClick={() => f.onToggle?.()}
        onPointerDown={(e) => down(e, f.id, "folder")}
        onContextMenu={(e) => menuClick(e, f.id)}
        onKeyDown={(e) => menuKeys(e, f.id)}
        data-rail-tile={f.id}
        data-rail-kind="folder"
        class={cx("lu-rail__folder", "lu-tone-" + f.tone, lifted(f.id) && "is-lifted", onto(f.id) && "is-onto")}
      >
        {minis(f)}
        <Show when={mentions()} fallback={<Show when={unread()}><Badge dot ring="rail" class="lu-rail__dot" /></Show>}>
          <Badge count={mentions()} ring="rail" aria-hidden="true" class="lu-rail__badge" />
        </Show>
      </button>
    );
  };
  const openFolder = (f: RailFolder) => (
    <div class={cx("lu-rail__group", "lu-tone-" + f.tone, lifted(f.id) && "is-lifted")}>
      <button
        type="button"
        aria-label={"Folder, " + f.label}
        aria-expanded="true"
        onClick={() => f.onToggle?.()}
        onPointerDown={(e) => down(e, f.id, "folder")}
        onContextMenu={(e) => menuClick(e, f.id)}
        onKeyDown={(e) => menuKeys(e, f.id)}
        data-rail-tile={f.id}
        data-rail-kind="folder"
        data-rail-open="true"
        class={cx("lu-rail__folderhead", onto(f.id) && "is-onto")}
      >
        <Icon name="folder" size={18} strokeWidth={1.9} />
      </button>
      <ul class="lu-rail__inner" aria-label={f.label}>
        <For each={f.servers}>{(s) => <li>{serverTile(s, f.id)}</li>}</For>
      </ul>
    </div>
  );
  const ghost = () => {
    const d = shown();
    if (!d) return null;
    const t = find(d.id);
    if (!t) return null;
    const at = props.still ? props.still.ghost : drag() ? { left: drag()!.x - drag()!.grabX, top: drag()!.y - drag()!.grabY } : null;
    return at ? { t, at } : null;
  };
  const ghostBody = (t: RailTile) =>
    t.kind === "server" ? (
      <span class={cx("lu-rail__server", t.src && "has-photo", "lu-tile-ink-" + (t.ink ?? "default"))}>
        <Face initials={t.initials} src={t.src} />
      </span>
    ) : (
      <span class={cx("lu-rail__folder", "lu-tone-" + t.tone)}>{minis(t)}</span>
    );

  return (
    <nav ref={nav} aria-label="Home and servers" class={cx("lu-rail", drag() && "is-dragging", props.class)}>
      <button type="button" class={cx("lu-rail__home", props.homeCurrent && "is-current")} aria-current={props.homeCurrent ? "true" : undefined} onClick={() => props.onHome?.()} aria-label={homeLabel() + (props.homeUnread ? ", " + props.homeUnread + " unread" : "")}>
        <Show when={props.logoSrc}>
          <img src={props.logoSrc} alt="" width={36} height={36} draggable={false} />
        </Show>
        <Show when={props.homeUnread}>
          <Badge count={props.homeUnread} ring="rail" aria-hidden="true" class="lu-rail__badge lu-rail__badge--home" />
        </Show>
      </button>
      <span aria-hidden="true" class="lu-rail__divider"></span>
      <ul class="lu-rail__list">
        <For each={items()}>
          {(t) => (
            <li data-rail-item={t.id} data-rail-kind={t.kind}>
              {t.kind === "server" ? serverTile(t) : t.open ? openFolder(t) : folderTile(t)}
            </li>
          )}
        </For>
      </ul>
      <button type="button" aria-label="Create, join or discover servers" class="lu-rail__add" onClick={() => props.onAddServer?.()}>
        <Icon name="plus" size={18} strokeWidth={2} />
      </button>
      <span class="lu-rail__spacer"></span>
      <Show when={props.self}>
        {(self) => (
          <button type="button" class="lu-rail__self" onClick={(e) => self().onClick?.(e.currentTarget)} aria-haspopup="menu" aria-expanded={!!self().menuOpen} aria-label={self().name + ", account menu"} title={self().hint}>
            <Avatar initials={self().initials} src={self().src} tone={self().tone ?? "indigo"} size={38} status={self().status} ring="rail" />
          </button>
        )}
      </Show>
      <Show when={line() !== null}>
        <span aria-hidden="true" class="lu-rail__line" style={{ top: line() + "px" }}></span>
      </Show>
      <Show when={ghost()}>
        {(g) =>
          props.still ? (
            <span aria-hidden="true" class="lu-rail__ghost is-still" style={{ left: g().at.left + "px", top: g().at.top + "px" }}>
              {ghostBody(g().t)}
            </span>
          ) : (
            <Portal>
              <span aria-hidden="true" class="lu-rail__ghost" style={{ left: g().at.left + "px", top: g().at.top + "px" }}>
                {ghostBody(g().t)}
              </span>
            </Portal>
          )
        }
      </Show>
      <div class="lu-sr" role="status" aria-live="polite">
        {props.announce}
      </div>
    </nav>
  );
}

export interface RailNoteProps {
  text: string;
  onDismiss?: () => void;
}

/** What went wrong with the rail (a move not kept), beside it until dismissed. */
export function RailNote(props: RailNoteProps) {
  return (
    <Portal>
      <div class="lu-railnote" role="alert">
        <p class="lu-railnote__text">{props.text}</p>
        <IconButton icon="close" label="Dismiss" size={28} strokeWidth={2} onClick={() => props.onDismiss?.()} />
      </div>
    </Portal>
  );
}

export interface ServerHeaderProps {
  name: string;
  /** The name is a button that opens the place's menu; without it the name is a heading. */
  onOpenMenu?: (button: HTMLElement) => void;
  menuOpen?: boolean;
  /** The pencil beside the name; left out where there is nowhere for it to lead. */
  onNewMessage?: () => void;
  class?: string;
}

/** Server name and New message, atop the sidebar. */
export function ServerHeader(props: ServerHeaderProps) {
  return (
    <div class={cx("lu-serverhead", props.class)}>
      <Show
        when={props.onOpenMenu}
        fallback={
          <h1 class="lu-serverhead__name">
            <span class="lu-serverhead__title">{props.name}</span>
          </h1>
        }
      >
        <button type="button" class="lu-serverhead__name" aria-haspopup="menu" aria-expanded={!!props.menuOpen} aria-label={props.name + ", server menu"} onClick={(e) => props.onOpenMenu?.(e.currentTarget)}>
          <span class="lu-serverhead__title">{props.name}</span>
          <Icon name="chevron-down" size={14} />
        </button>
      </Show>
      <Show when={props.onNewMessage}>
        <IconButton icon="compose" label="New message" size={34} onClick={() => props.onNewMessage?.()} />
      </Show>
    </div>
  );
}

export interface QuickTilesProps {
  /** `countLabel` names the count for assistive technology ("2 mentions"); "2 new" without it. `current` is the page open. */
  items: { icon: IconName; label: string; count?: number; countLabel?: string; current?: boolean; href?: string; onClick?: () => void }[];
  class?: string;
}

/** Threads / Mentions / Saved tiles. */
export function QuickTiles(props: QuickTilesProps) {
  return (
    <div class={cx("lu-quick", props.class)}>
      <For each={props.items}>
        {(it) => (
          <a href={it.href ?? "#"} onClick={(e) => pressed(e, it)} aria-current={it.current ? "page" : undefined} class="lu-quick__tile">
            <Icon name={it.icon} size={18} strokeWidth={1.8} />
            <span>{it.label}</span>
            <Show when={it.count}>
              <Badge count={it.count} size="sm" label={it.countLabel ?? it.count + " new"} class="lu-quick__badge" />
            </Show>
          </a>
        )}
      </For>
    </div>
  );
}

export interface ChannelItem {
  name: string;
  kind?: "channel" | "private" | "dm" | "group";
  active?: boolean;
  unread?: boolean;
  mentions?: number;
  count?: number;
  href?: string;
  onClick?: () => void;
  initials?: string;
  src?: string;
  tone?: Tone;
  status?: Status;
  people?: Person[];
  /** A row that is a place rather than a conversation (Mentions) leads with its icon. */
  icon?: IconName;
}

function RowLead(props: { item: ChannelItem }) {
  return (
    <Show when={!props.item.icon} fallback={<Icon name={props.item.icon as IconName} size={15} strokeWidth={2} class="lu-row__glyph lu-row__icon" />}>
      <Show
        when={props.item.kind === "dm"}
        fallback={
          <Show
            when={props.item.kind === "group"}
            fallback={
              <Show when={props.item.kind === "private"} fallback={<span aria-hidden="true" class="lu-row__glyph lu-row__hash">#</span>}>
                <Icon name="lock" size={11} strokeWidth={2.6} class="lu-row__glyph" />
              </Show>
            }
          >
            <span aria-hidden="true" class="lu-row__pair">
              <For each={(props.item.people ?? []).slice(0, 2)}>
                {(p) => (
                  <span class={cx("lu-avatar", "lu-tone-" + (p.tone ?? "blue"))}>
                    <Face initials={p.initials} src={p.src} />
                  </span>
                )}
              </For>
            </span>
          </Show>
        }
      >
        <Avatar initials={props.item.initials ?? ""} src={props.item.src} tone={props.item.tone} size={22} status={props.item.status} ring="bg" />
      </Show>
    </Show>
  );
}

export interface ChannelListProps {
  /** `action` is the plus at the end of a section's title; `before` stands between the title and the rows. */
  sections: { title?: string; items: ChannelItem[]; action?: { label: string; onClick: (anchor: HTMLElement) => void; expanded?: boolean }; before?: JSX.Element; empty?: string }[];
  class?: string;
}

/** Sidebar sections of channels and DMs. */
export function ChannelList(props: ChannelListProps) {
  return (
    <div class={cx("lu-chlist", props.class)}>
      <For each={props.sections}>
        {(sec) => (
          <section>
            <Show when={sec.title}>
              <div class="lu-chlist__head">
                <h2 class="lu-chlist__title">{sec.title}</h2>
                <Show when={sec.action}>{(a) => <IconButton icon="plus" label={a().label} size={28} class="lu-chlist__add" aria-haspopup="menu" aria-expanded={a().expanded ? "true" : "false"} onClick={(e) => a().onClick(e.currentTarget)} />}</Show>
              </div>
            </Show>
            {sec.before}
            <Show when={sec.empty && sec.items.length === 0}>
              <p class="lu-chlist__empty">{sec.empty}</p>
            </Show>
            <ul class="lu-chlist__list">
              <For each={sec.items}>
                {(it) => {
                  const state = () => (it.active ? "active" : it.unread || it.mentions || it.count ? "unread" : "read");
                  const person = () => it.kind === "dm" || it.kind === "group";
                  return (
                    <li>
                      <a href={it.href ?? "#"} onClick={(e) => pressed(e, it)} aria-current={it.active ? "page" : undefined} class={cx("lu-row", "is-" + state(), person() && "lu-row--person")}>
                        <RowLead item={it} />
                        <span class="lu-row__name">
                          {it.name}
                          <Show when={it.kind === "dm" && it.status}>{(s) => <span class="lu-sr">, {STATUS_WORD[s()]}</span>}</Show>
                        </span>
                        <Show when={it.mentions} fallback={<Show when={it.count}><Badge count={it.count} /></Show>}>
                          <Badge count={it.mentions} label={it.mentions + (it.mentions === 1 ? " mention" : " mentions")} />
                        </Show>
                      </a>
                    </li>
                  );
                }}
              </For>
            </ul>
          </section>
        )}
      </For>
    </div>
  );
}

export interface SegmentedTabsProps {
  tabs: { id: string; label: string; count?: number }[];
  value?: string;
  defaultValue?: string;
  onChange?: (id: string) => void;
  label?: string;
  class?: string;
}

/** Equal segments on a track. */
export function SegmentedTabs(props: SegmentedTabsProps) {
  const [inner, setInner] = createSignal(props.defaultValue ?? props.tabs[0]?.id);
  const cur = () => (props.value !== undefined ? props.value : inner());
  return (
    <div role="tablist" aria-label={props.label ?? "Side panel"} class={cx("lu-segtabs", props.class)} style={{ "grid-template-columns": "repeat(" + (props.tabs.length || 1) + ", minmax(0, 1fr))" }}>
      <For each={props.tabs}>
        {(t) => (
          <button
            type="button"
            role="tab"
            aria-selected={cur() === t.id}
            class={cx("lu-segtabs__tab", cur() === t.id && "is-on")}
            onClick={() => {
              if (props.value === undefined) setInner(t.id);
              props.onChange?.(t.id);
            }}
          >
            {t.label}
            <Show when={t.count !== undefined}>
              <span class="lu-segtabs__count">{t.count}</span>
            </Show>
          </button>
        )}
      </For>
    </div>
  );
}

export interface ChannelHeaderProps {
  name: string;
  topic?: string;
  /** `dm` shows the lock (end-to-end encrypted); `page` shows no glyph. */
  kind?: "channel" | "private" | "dm" | "page";
  /** `narrow` shows the action only where the window is too narrow for the aside to stand in the row. */
  actions?: { icon: IconName; label: string; onClick?: () => void; strokeWidth?: number; narrow?: boolean; pressed?: boolean }[];
  /** Back to the list, where the frame is folded (a phone). */
  onBack?: () => void;
  backLabel?: string;
  class?: string;
}

/** Conversation header. */
export function ChannelHeader(props: ChannelHeaderProps) {
  const list = () =>
    props.actions ?? [
      { icon: "voice" as const, label: "Start a voice call" },
      { icon: "pin" as const, label: "Pinned messages" },
      { icon: "search" as const, label: "Search in #" + props.name, strokeWidth: 1.8 },
      { icon: "more" as const, label: "Channel settings" },
    ];
  return (
    <header class={cx("lu-chhead", props.class)}>
      <Show when={props.onBack}>
        <IconButton icon="back" label={props.backLabel ?? "Back"} size={44} class="lu-chhead__back" onClick={() => props.onBack?.()} />
      </Show>
      <div class="lu-chhead__id">
        <Show when={props.kind !== "page"}>
          <Icon name={props.kind === "private" || props.kind === "dm" ? "lock" : "hash"} size={20} strokeWidth={2.2} class="lu-chhead__glyph" />
        </Show>
        <h1 class="lu-chhead__name">{props.name}</h1>
        <Show when={props.topic}>
          <span aria-hidden="true" class="lu-chhead__rule"></span>
          <p class="lu-chhead__topic">{props.topic}</p>
        </Show>
      </div>
      <div class="lu-chhead__actions">
        <For each={list()}>{(a) => <IconButton icon={a.icon} label={a.label} strokeWidth={a.strokeWidth ?? 1.8} pressed={"pressed" in a ? a.pressed : undefined} class={"narrow" in a && a.narrow ? "lu-chhead__narrow" : undefined} onClick={() => a.onClick?.()} />}</For>
      </div>
    </header>
  );
}

/* ── Notice ──────────────────────────────────────────────── */

export interface NoticeProps {
  icon?: IconName;
  tone?: "neutral" | "accent";
  action?: { label: string; onClick?: () => void };
  children?: JSX.Element;
  class?: string;
}

/** An inline strip: what is encrypted, what this device cannot do yet. */
export function Notice(props: NoticeProps) {
  return (
    <div class={cx("lu-notice", "lu-notice--" + (props.tone ?? "neutral"), props.class)} role="note">
      <Show when={props.icon}>{(icon) => <Icon name={icon()} size={18} class="lu-notice__glyph" />}</Show>
      <p class="lu-notice__text">{props.children}</p>
      <Show when={props.action}>{(a) => <Button variant="text" onClick={() => a().onClick?.()}>{a().label}</Button>}</Show>
    </div>
  );
}

/* ── Channel membership (architecture 6.8) ─────────────────── */

export interface NoticeLineProps {
  /** What happened: "Grace joined". */
  children?: JSX.Element;
  time: string;
  datetime?: string;
  class?: string;
}

/** A notice in a conversation: someone came or went. No avatar, nothing to do with it. */
export function NoticeLine(props: NoticeLineProps) {
  return (
    <div class={cx("lu-noticeline", props.class)} role="listitem">
      <Icon name="member-joined" size={14} class="lu-noticeline__glyph" />
      <span class="lu-noticeline__text">{props.children}</span>
      <time class="lu-noticeline__time" datetime={props.datetime}>
        {props.time}
      </time>
    </div>
  );
}

export interface JoinBarProps {
  /** The channel's name, without its hash. */
  channel: string;
  busy?: boolean;
  error?: string;
  onJoin?: () => void;
  class?: string;
}

/** Where the composer would be in a channel read before joining. */
export function JoinBar(props: JoinBarProps) {
  return (
    <div class={cx("lu-joinbar", props.class)}>
      <div class="lu-joinbar__box">
        <p class="lu-joinbar__text">You’re not in #{props.channel}</p>
        <Button variant="primary" size="md" disabled={props.busy} onClick={() => props.onJoin?.()}>
          Join
        </Button>
      </div>
      <Show when={props.error}>
        <p class="lu-joinbar__error" role="alert">
          {props.error}
        </p>
      </Show>
    </div>
  );
}

export interface InvitationRowProps {
  channel: string;
  by: string;
  busy?: boolean;
  onJoin?: () => void;
  onDismiss?: () => void;
  class?: string;
}

/** An invitation into a channel in the sidebar, with the way in and the way to say no. */
export function InvitationRow(props: InvitationRowProps) {
  return (
    <div class={cx("lu-invitation", props.class)} role="group" aria-label={`${props.by} invited you to ${props.channel}`}>
      <p class="lu-invitation__text">
        #{props.channel} · {props.by} invited you
      </p>
      <div class="lu-invitation__actions">
        <Button size="sm" disabled={props.busy} onClick={() => props.onJoin?.()}>
          Join
        </Button>
        <Button variant="text" size="sm" onClick={() => props.onDismiss?.()}>
          Dismiss
        </Button>
      </div>
    </div>
  );
}

export interface BrowseRowProps {
  name: string;
  topic?: string;
  count: number;
  busy?: boolean;
  onOpen?: () => void;
  onJoin?: () => void;
  class?: string;
}

/** A channel in Browse: its name, topic and how many are in it, and Join. */
export function BrowseRow(props: BrowseRowProps) {
  const count = () => (props.count === 1 ? "1 member" : `${props.count} members`);
  return (
    <li class={cx("lu-browse", props.class)}>
      <button type="button" class="lu-browse__open" onClick={() => props.onOpen?.()} aria-label={`${props.name}, ${count()}`}>
        <span class="lu-browse__name">
          <span aria-hidden="true" class="lu-browse__hash">
            #
          </span>
          {props.name}
        </span>
        <span class="lu-browse__meta">{[props.topic, count()].filter(Boolean).join(" · ")}</span>
      </button>
      <Button variant="primary" size="md" disabled={props.busy} onClick={() => props.onJoin?.()}>
        Join
      </Button>
    </li>
  );
}

export interface PickRowProps {
  name: string;
  initials: string;
  src?: string;
  tone?: Tone;
  /** A word at the end: "Will be invited", "Unavailable". */
  note?: string;
  /** Ticked or not, for a picker of several; left out for one that picks one. */
  checked?: boolean;
  disabled?: boolean;
  onPick?: () => void;
  class?: string;
}

/** A person in a picker: "Add people", "Message someone". */
export function PickRow(props: PickRowProps) {
  return (
    <li>
      <button
        type="button"
        class={cx("lu-pick", props.class)}
        role={props.checked !== undefined ? "checkbox" : undefined}
        aria-checked={props.checked !== undefined ? (props.checked ? "true" : "false") : undefined}
        disabled={props.disabled}
        onClick={() => props.onPick?.()}
      >
        <Show when={props.checked !== undefined}>
          <span aria-hidden="true" class={cx("lu-pick__box", props.checked && "is-checked")}>
            <Show when={props.checked}>
              <Icon name="check" size={12} strokeWidth={3} />
            </Show>
          </span>
        </Show>
        <Avatar initials={props.initials} src={props.src} tone={props.tone} size={28} />
        <span class="lu-pick__name">{props.name}</span>
        <Show when={props.note}>
          <Tag tone="accent" size="sm">
            {props.note}
          </Tag>
        </Show>
      </button>
    </li>
  );
}

/* ── Request card ────────────────────────────────────────── */

export interface RequestCardProps {
  name: string;
  handle?: string;
  initials: string;
  src?: string;
  tone?: Tone;
  /** What the two of you share, nothing else (architecture 6.4). */
  meta?: string;
  busy?: boolean;
  onAccept?: () => void;
  onDecline?: () => void;
  onBlock?: () => void;
  class?: string;
}

/** A message request: who is asking and what you share. */
export function RequestCard(props: RequestCardProps) {
  return (
    <article class={cx("lu-request", props.class)} aria-label={"Request from " + props.name}>
      <Avatar initials={props.initials} src={props.src} tone={props.tone} size={40} />
      <div class="lu-request__text">
        <p class="lu-request__name">
          {props.name}
          <Show when={props.handle}>
            <span class="lu-request__handle">@{props.handle}</span>
          </Show>
        </p>
        <Show when={props.meta}>
          <p class="lu-request__meta">{props.meta}</p>
        </Show>
      </div>
      <div class="lu-request__actions">
        <Button variant="primary" size="md" disabled={props.busy} onClick={() => props.onAccept?.()}>
          Accept
        </Button>
        <Button size="md" disabled={props.busy} onClick={() => props.onDecline?.()}>
          Decline
        </Button>
        <Button variant="text" disabled={props.busy} onClick={() => props.onBlock?.()}>
          Block
        </Button>
      </div>
    </article>
  );
}

/* ── Device list ─────────────────────────────────────────── */

export interface DeviceEntry {
  id: string;
  /** `web`, `native`, `mobile`; anything else draws the key. */
  kind: string;
  label: string;
  meta?: string;
  thisDevice?: boolean;
  revoked?: boolean;
}

export interface DeviceListProps {
  devices: DeviceEntry[];
  onRevoke?: (device: DeviceEntry) => void;
  class?: string;
}

function deviceIcon(kind: string): IconName {
  if (kind === "web") return "globe";
  if (kind === "mobile") return "phone";
  if (kind === "native") return "monitor";
  return "key";
}

/** The devices of an identity, with revoke for the others. */
export function DeviceList(props: DeviceListProps) {
  return (
    <ul class={cx("lu-devices", props.class)}>
      <For each={props.devices}>
        {(d) => (
          <li class={cx("lu-device", d.thisDevice && "is-this", d.revoked && "is-revoked")}>
            <span class="lu-device__glyph" aria-hidden="true">
              <Icon name={deviceIcon(d.kind)} size={20} />
            </span>
            <div class="lu-device__text">
              <p class="lu-device__name">
                {d.label}
                <Show when={d.thisDevice}>
                  <Tag tone="accent" size="sm">
                    This device
                  </Tag>
                </Show>
                <Show when={d.revoked}>
                  <Tag size="sm">Revoked</Tag>
                </Show>
              </p>
              <Show when={d.meta}>
                <p class="lu-device__meta">{d.meta}</p>
              </Show>
            </div>
            <Show when={props.onRevoke && !d.thisDevice && !d.revoked}>
              <Button size="sm" onClick={() => props.onRevoke?.(d)}>
                Revoke
              </Button>
            </Show>
          </li>
        )}
      </For>
    </ul>
  );
}

/* ── Empty state ─────────────────────────────────────────── */

export interface EmptyStateProps {
  icon?: IconName;
  title: string;
  children?: JSX.Element;
  action?: { label: string; onClick?: () => void };
  class?: string;
}

/** A quiet centre for a panel with nothing in it yet. */
export function EmptyState(props: EmptyStateProps) {
  return (
    <div class={cx("lu-empty", props.class)}>
      <Show when={props.icon}>{(icon) => <Icon name={icon()} size={28} class="lu-empty__glyph" />}</Show>
      <p class="lu-empty__title">{props.title}</p>
      <Show when={props.children}>
        <p class="lu-empty__text">{props.children}</p>
      </Show>
      <Show when={props.action}>{(a) => <Button size="md" onClick={() => a().onClick?.()}>{a().label}</Button>}</Show>
    </div>
  );
}

/* ── Messages ────────────────────────────────────────────── */

export interface MentionProps {
  children?: JSX.Element;
  href?: string;
  /** The person's card, against the pill; a mention of everyone opens nothing. */
  onOpen?: (pressed: HTMLElement) => void;
  class?: string;
}

/** Inline @mention: the text as written, tinted. */
export function Mention(props: MentionProps) {
  return (
    <Show when={props.onOpen} fallback={<Show when={props.href} fallback={<span class={cx("lu-mention", props.class)}>{props.children}</span>}><a href={props.href} class={cx("lu-mention", props.class)}>{props.children}</a></Show>}>
      <button type="button" class={cx("lu-mention", props.class)} aria-haspopup="dialog" onClick={(e) => props.onOpen?.(e.currentTarget)}>
        {props.children}
      </button>
    </Show>
  );
}

export interface AttachmentProps {
  name: string;
  size?: string;
  kind?: "pdf";
  href?: string;
  onDownload?: () => void;
  class?: string;
}

/** File card in a message. */
export function Attachment(props: AttachmentProps) {
  return (
    <div class={cx("lu-attach", props.class)}>
      <span aria-hidden="true" class={cx("lu-attach__type", "lu-attach__type--" + (props.kind ?? "pdf"))}>
        <Icon name="file" size={17} />
      </span>
      <span class="lu-attach__name">{props.name}</span>
      <Show when={props.size}>
        <span class="lu-attach__size">{props.size}</span>
      </Show>
      <IconButton icon="download" label={"Download " + props.name} size={32} onClick={() => props.onDownload?.()} />
    </div>
  );
}

export interface ThreadSummaryProps {
  count: number;
  lastReply?: string;
  people?: Person[];
  open?: boolean;
  onClick?: () => void;
  class?: string;
}

/** Replies chip under a message. */
export function ThreadSummary(props: ThreadSummaryProps) {
  return (
    <button type="button" onClick={() => props.onClick?.()} class={cx("lu-threadsum", props.open && "is-open", props.class)}>
      <AvatarStack people={props.people ?? []} size={22} ring="surface" />
      <span class="lu-threadsum__count">
        {props.count} {props.count === 1 ? "reply" : "replies"}
      </span>
      <Show when={props.lastReply}>
        <span class="lu-threadsum__last">Last reply {props.lastReply}</span>
      </Show>
    </button>
  );
}

export interface MessageActionsProps {
  /** The picker, against the button pressed. */
  onReact?: (pressed: HTMLElement) => void;
  onThread?: () => void;
  /** The menu of the rest (`MessageMenu`), against the button pressed. */
  onMore?: (pressed: HTMLElement) => void;
  /**
   * The product's toolbar (the "Inline replies" board, architecture 6.6):
   * Add reaction, Reply and More actions. Without it the toolbar is the
   * Quiet Split board's, with its thread button.
   */
  onReply?: () => void;
  /** This message is the one being replied to: Reply is drawn pressed, with its word. */
  replying?: boolean;
  /** The More actions menu is open. */
  moreOpen?: boolean;
  floating?: boolean;
  class?: string;
}

/** Hover toolbar for a message. */
export function MessageActions(props: MessageActionsProps) {
  const board = () => props.onReply === undefined;
  return (
    <div role="toolbar" aria-label="Message actions" class={cx("lu-actions", (props.floating ?? true) && "is-floating", props.replying && "is-replying", props.moreOpen && "is-open", props.class)}>
      <Show
        when={!board()}
        fallback={
          <>
            <IconButton icon="reaction-add" label="Add reaction" size={30} aria-haspopup="dialog" onClick={(e) => props.onReact?.(e.currentTarget)} />
            <IconButton icon="thread" label="Open thread" size={30} onClick={() => props.onThread?.()} />
            <IconButton icon="more" label="More actions" size={30} onClick={(e) => props.onMore?.(e.currentTarget)} />
          </>
        }
      >
        <IconButton icon="emoji" label="Add reaction" size={30} iconSize={16} strokeWidth={1.9} aria-haspopup="dialog" onClick={(e) => props.onReact?.(e.currentTarget)} />
        <Show when={props.replying} fallback={<IconButton icon="reply" label="Reply" size={30} iconSize={16} strokeWidth={1.9} onClick={() => props.onReply?.()} />}>
          <button type="button" class="lu-actions__replying" aria-pressed="true" onClick={() => props.onReply?.()}>
            <Icon name="reply" size={16} strokeWidth={1.9} />
            Reply
          </button>
        </Show>
        <IconButton icon="more" label="More actions" size={30} iconSize={16} aria-haspopup="menu" aria-expanded={!!props.moreOpen} onClick={(e) => props.onMore?.(e.currentTarget)} />
      </Show>
    </div>
  );
}

export interface MessageMenuProps {
  /** Its author's own message: it can be edited. */
  onEdit?: () => void;
  onCopy?: () => void;
  /** Its author's own, or a moderator's to take away; `now` when Shift was held, which skips the question. */
  onDelete?: (now: boolean) => void;
  class?: string;
}

/** What More actions opens over a message: Edit message, Copy text, Delete message. */
export function MessageMenu(props: MessageMenuProps) {
  return (
    <Menu label="Message" compact class={props.class}>
      <Show when={props.onEdit}>
        <MenuItem onSelect={() => props.onEdit?.()}>Edit message</MenuItem>
      </Show>
      <MenuItem onSelect={() => props.onCopy?.()}>Copy text</MenuItem>
      <Show when={props.onDelete}>
        <MenuItem onSelect={(e) => props.onDelete?.(e.shiftKey)}>
          Delete message
        </MenuItem>
      </Show>
    </Menu>
  );
}

export interface ReplyRefProps {
  /**
   * Who wrote what is quoted, and their picture; a personal DM's quote of
   * someone unknown draws neither.
   */
  name?: string;
  initials?: string;
  src?: string;
  tone?: Tone;
  /** What is quoted is this account's: its name is drawn as a mention of it. */
  self?: boolean;
  text: string;
  /** The original was deleted: the line says so and leads nowhere. */
  deleted?: boolean;
  /** Goes to the original. */
  onOpen?: () => void;
  class?: string;
}

/** The connector from the reply's avatar up to what it quotes. */
function ReplyConnector() {
  return (
    <svg class="lu-replyref__line" width="26" height="12" viewBox="0 0 26 12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true">
      <path d="M1 12V7a5 5 0 0 1 5-5h19" />
    </svg>
  );
}

/**
 * What a reply quotes, on one line over its avatar row (the "Inline
 * replies" board): a connector from the avatar, the quoted person's small
 * avatar and name, and the quoted text cut short; pressing it goes to the
 * original.
 */
export function ReplyRef(props: ReplyRefProps) {
  const who = () => (props.self ? "you" : props.name || "a message");
  return (
    <Show
      when={!props.deleted}
      fallback={
        <div class={cx("lu-replyref", "is-deleted", props.class)}>
          <ReplyConnector />
          <span class="lu-replyref__gone" aria-hidden="true">
            <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round">
              <path d="M5 12h14" />
            </svg>
          </span>
          <span class="lu-replyref__deleted">Original message was deleted</span>
        </div>
      }
    >
      <button type="button" class={cx("lu-replyref", props.class)} aria-label={`Replying to ${who()}: ${props.text}. Jump to message`} onClick={() => props.onOpen?.()}>
        <ReplyConnector />
        <Show when={props.name}>
          <span aria-hidden="true" class={cx("lu-avatar", "lu-replyref__avatar", "lu-tone-" + (props.tone ?? "blue"))}>
            <Face initials={props.initials ?? ""} src={props.src} />
          </span>
          <span class={cx("lu-replyref__name", props.self && "is-self")}>{props.self ? "@" + props.name : props.name}</span>
        </Show>
        <span class="lu-replyref__text">{props.text}</span>
      </button>
    </Show>
  );
}

export interface MessageEditProps {
  /** The body as it is, which the field opens with. */
  value: string;
  /** Return: the text as it then is. */
  onSave: (text: string) => void;
  /** Escape. */
  onCancel: () => void;
  class?: string;
}

/** A message's body being edited where it is drawn: Return saves, Escape cancels. */
export function MessageEdit(props: MessageEditProps) {
  let field: HTMLTextAreaElement | undefined;
  const hint = fieldId();
  const fit = () => {
    if (!field) return;
    field.style.height = "auto";
    field.style.height = field.scrollHeight + "px";
  };
  onMount(() => {
    if (!field) return;
    fit();
    field.focus({ preventScroll: true });
    const end = field.value.length;
    field.setSelectionRange(end, end);
  });
  return (
    <div class={cx("lu-msgedit", props.class)}>
      <textarea
        ref={field}
        class="lu-msgedit__field"
        aria-label="Edit message"
        aria-describedby={hint}
        rows={1}
        value={props.value}
        onInput={fit}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            props.onCancel();
          } else if (e.key === "Enter" && !e.isComposing) {
            // As the composer: Return sends, and a line of its own is not something it makes.
            e.preventDefault();
            props.onSave(e.currentTarget.value);
          }
        }}
      />
      <p id={hint} class="lu-msgedit__hint">
        Esc to cancel · Return to save
      </p>
    </div>
  );
}

export interface ConfirmDeleteProps {
  /** The message's first line, quoted under the question. */
  text: string;
  onCancel?: () => void;
  onDelete?: () => void;
  class?: string;
}

/** "Delete this message?", asked before a message is deleted (Shift skips it). */
export function ConfirmDelete(props: ConfirmDeleteProps) {
  return (
    <Pop title="Delete this message?" subtitle={props.text || undefined} class={cx("lu-pop--confirm", props.class)}>
      <div class="lu-pop__actions">
        <Button size="md" onClick={() => props.onCancel?.()}>
          Cancel
        </Button>
        <Button variant="primary" size="md" onClick={() => props.onDelete?.()}>
          Delete
        </Button>
      </div>
    </Pop>
  );
}

function Paras(props: { children?: JSX.Element | string[] }) {
  const value = () => props.children;
  return (
    <Show
      when={typeof value() === "string"}
      fallback={
        <Show when={Array.isArray(value()) && (value() as unknown[]).every((c) => typeof c === "string")} fallback={value() as JSX.Element}>
          <For each={value() as string[]}>{(c) => <p>{c}</p>}</For>
        </Show>
      }
    >
      <p>{value() as string}</p>
    </Show>
  );
}

export interface MessageProps {
  author: string;
  initials: string;
  /** The author's picture. */
  src?: string;
  tone?: Tone;
  tag?: { label: string; tone?: "neutral" | "accent" };
  time?: string;
  datetime?: string;
  children?: JSX.Element | string[];
  reactions?: ReactionChipProps[];
  /** The `lu-reaction-add` after the chips, which opens the picker against itself. */
  onAddReaction?: (pressed: HTMLElement) => void;
  /** What it quotes (`ReplyRef`): a line over the avatar row, the block's whole width. */
  reference?: JSX.Element;
  /** It is the one being replied to: its row keeps the hover's fill and its toolbar stays. */
  replying?: boolean;
  /** The first message's own toolbar (`MessageActions`), shown over it alone on hover; each part has its own. */
  tools?: JSX.Element;
  /** The message's id, which a jump to it finds (`data-message-id`). */
  id?: string;
  /** It names this account or everyone, or quotes it: the row is tinted, with the accent bar. */
  mentioned?: boolean;
  /** Jumped to: the row's highlight, for a moment. */
  flash?: boolean;
  attachment?: AttachmentProps;
  thread?: ThreadSummaryProps;
  highlighted?: boolean;
  actions?: boolean;
  onReact?: () => void;
  onThread?: () => void;
  onMore?: () => void;
  variant?: "default" | "parent" | "reply";
  /** The name and the avatar open the author's profile, against the one pressed. */
  onOpenProfile?: (pressed: HTMLElement) => void;
  /**
   * What the same person sent straight after: drawn under the first as
   * further paragraphs of one block, each with its time in the avatar's
   * column under the pointer.
   */
  parts?: MessagePart[];
  /** Grouped is the block under one name; one line draws each message as `time  name  text`, with no avatar. */
  layout?: MessageLayout;
  /** Large, small, or none and no room kept for one (grouped only). */
  avatars?: MessageAvatars;
  class?: string;
}

export type MessageLayout = "grouped" | "one_line";
export type MessageAvatars = "large" | "small" | "hidden";
export type MessageSpacing = "roomy" | "cozy" | "tight";

export interface MessagePart {
  time?: string;
  datetime?: string;
  children?: JSX.Element | string[];
  id?: string;
  reactions?: ReactionChipProps[];
  onAddReaction?: (pressed: HTMLElement) => void;
  tools?: JSX.Element;
  mentioned?: boolean;
  flash?: boolean;
  replying?: boolean;
}

/** One message's reactions, and the button that adds another. */
function Reactions(props: { reactions?: ReactionChipProps[]; onAdd?: (pressed: HTMLElement) => void }) {
  return (
    <Show when={props.reactions && props.reactions.length}>
      <div class="lu-msg__reactions">
        <For each={props.reactions}>{(r) => <ReactionChip {...r} />}</For>
        <Show when={props.onAdd}>
          <button type="button" aria-label="Add reaction" aria-haspopup="dialog" class="lu-reaction-add" onClick={(e) => props.onAdd?.(e.currentTarget)}>
            <Icon name="reaction-add" size={15} strokeWidth={1.9} />
          </button>
        </Show>
      </div>
    </Show>
  );
}

/** One message, or one person's run of them as a block. */
export function Message(props: MessageProps) {
  const variant = () => props.variant ?? "default";
  const compact = () => variant() !== "default";
  const avatars = () => props.avatars ?? "large";
  const avatar = () => <Avatar initials={props.initials} src={props.src} tone={props.tone ?? "blue"} size={compact() || avatars() === "small" ? 28 : 40} />;
  // Read once: the row's class and its first line both ask.
  const reference = children(() => props.reference);
  const name = (extra?: string) => (
    <Show when={props.onOpenProfile} fallback={<span class={cx("lu-msg__author", extra)}>{props.author}</span>}>
      <button type="button" class={cx("lu-msg__author", "lu-msg__name", extra)} aria-haspopup="dialog" onClick={(e) => props.onOpenProfile?.(e.currentTarget)}>
        {props.author}
      </button>
    </Show>
  );
  // A message as one line: its time, the name again, and its text, which wraps under itself.
  const line = (m: MessagePart, first: boolean, ref?: JSX.Element) => (
    <div class={cx("lu-msg__part", first && "lu-msg__part--first", !!ref && "has-ref", m.mentioned && "is-mentioned", m.flash && "is-flash", m.replying && "is-replying")} data-message-id={m.id}>
      {ref}
      <Show when={m.time}>
        <time datetime={m.datetime} class="lu-line__time">
          {m.time}
        </time>
      </Show>
      {name("lu-line__name")}
      <div class="lu-line__text">
        <div class="lu-msg__body">
          <Paras>{m.children}</Paras>
        </div>
        <Reactions reactions={m.reactions} onAdd={m.onAddReaction} />
      </div>
      {m.tools}
    </div>
  );
  const firstPart: MessagePart = {
    get time() {
      return props.time;
    },
    get datetime() {
      return props.datetime;
    },
    get children() {
      return props.children;
    },
    get id() {
      return props.id;
    },
    get reactions() {
      return props.reactions;
    },
    get onAddReaction() {
      return props.onAddReaction;
    },
    get tools() {
      return props.tools;
    },
    get mentioned() {
      return props.mentioned;
    },
    get flash() {
      return props.flash;
    },
    get replying() {
      return props.replying;
    },
  };
  return (
    <Show
      when={props.layout === "one_line" && !compact()}
      fallback={
        <article class={cx("lu-msg", "lu-msg--" + variant(), !compact() && avatars() !== "large" && "lu-msg--avatars-" + avatars(), props.highlighted && "is-highlighted", !!reference() && "has-ref", props.class)}>
          {reference()}
          <Show when={compact() || avatars() !== "hidden"}>
            <Show when={props.onOpenProfile} fallback={avatar()}>
              {/* The name is the control; the avatar is the same one for a pointer. */}
              <button type="button" class="lu-msg__avatar" tabindex={-1} aria-hidden="true" onClick={(e) => props.onOpenProfile?.(e.currentTarget)}>
                {avatar()}
              </button>
            </Show>
          </Show>
          <div class="lu-msg__main">
            <div class={cx("lu-msg__part", "lu-msg__part--first", props.mentioned && "is-mentioned", props.flash && "is-flash", props.replying && "is-replying")} data-message-id={props.id}>
              <div class="lu-msg__head">
                {name()}
                <Show when={compact() ? undefined : props.tag}>{(tag) => <Tag tone={tag().tone ?? "neutral"}>{tag().label}</Tag>}</Show>
                <Show when={props.time}>
                  <time datetime={props.datetime} class="lu-msg__time">
                    {props.time}
                  </time>
                </Show>
              </div>
              <div class="lu-msg__body">
                <Paras>{props.children}</Paras>
              </div>
              <Show when={!props.parts?.length}>
                <Show when={props.attachment}>{(a) => <Attachment {...a()} />}</Show>
                <Show when={props.thread}>{(t) => <ThreadSummary {...t()} />}</Show>
              </Show>
              <Reactions reactions={props.reactions} onAdd={props.onAddReaction} />
              {props.tools}
            </div>
            <For each={props.parts ?? []}>
              {(part) => (
                <div class={cx("lu-msg__part", part.mentioned && "is-mentioned", part.flash && "is-flash", part.replying && "is-replying")} data-message-id={part.id}>
                  <Show when={part.time}>
                    {/* With no avatar there is no column for it: the time is for assistive technology alone. */}
                    <time datetime={part.datetime} class={avatars() === "hidden" && !compact() ? "lu-sr" : "lu-msg__parttime"}>
                      <span class="lu-sr">{props.author}, </span>
                      {part.time}
                    </time>
                  </Show>
                  <div class="lu-msg__body">
                    <Paras>{part.children}</Paras>
                  </div>
                  <Reactions reactions={part.reactions} onAdd={part.onAddReaction} />
                  {part.tools}
                </div>
              )}
            </For>
            <Show when={props.parts?.length}>
              <Show when={props.attachment}>{(a) => <Attachment {...a()} />}</Show>
              <Show when={props.thread}>{(t) => <ThreadSummary {...t()} />}</Show>
            </Show>
          </div>
          <Show when={(props.actions ?? true) && !compact()}>
            <MessageActions onReact={() => props.onReact?.()} onThread={props.onThread} onMore={props.onMore} />
          </Show>
        </article>
      }
    >
      <article class={cx("lu-msg", "lu-msg--line", props.highlighted && "is-highlighted", props.class)}>
        {line(firstPart, true, reference())}
        <For each={props.parts ?? []}>{(part) => line(part, false)}</For>
        <Show when={props.attachment}>{(a) => <Attachment {...a()} />}</Show>
        <Show when={props.thread}>{(t) => <ThreadSummary {...t()} />}</Show>
      </article>
    </Show>
  );
}

export interface SystemMessageProps {
  icon?: IconName;
  time?: string;
  datetime?: string;
  children?: JSX.Element;
  class?: string;
}

/** One-line event (joined). */
export function SystemMessage(props: SystemMessageProps) {
  return (
    <div class={cx("lu-sysmsg", props.class)}>
      <div aria-hidden="true" class="lu-sysmsg__icon">
        <Icon name={props.icon ?? "member-joined"} size={18} strokeWidth={1.9} />
      </div>
      <p class="lu-sysmsg__text">{props.children}</p>
      <Show when={props.time}>
        <time datetime={props.datetime} class="lu-sysmsg__time">
          {props.time}
        </time>
      </Show>
    </div>
  );
}

export interface DividerProps {
  variant?: "day" | "new" | "or";
  label?: string;
  class?: string;
}

/** Day, New or "or" rule. */
export function Divider(props: DividerProps) {
  const variant = () => props.variant ?? "day";
  return (
    <Show
      when={variant() === "new"}
      fallback={
        <div class={cx("lu-divider", "lu-divider--" + variant(), props.class)}>
          <span aria-hidden="true" class="lu-divider__rule"></span>
          <span class="lu-divider__label">{props.label ?? (variant() === "or" ? "or" : "Today")}</span>
          <span aria-hidden="true" class="lu-divider__rule"></span>
        </div>
      }
    >
      {/* "New" to the eye, "New messages" to assistive technology. */}
      <div class={cx("lu-divider", "lu-divider--new", props.class)} role="separator" aria-label={props.label === undefined || props.label === "New" ? "New messages" : props.label}>
        <span aria-hidden="true" class="lu-divider__rule"></span>
        <span aria-hidden="true" class="lu-divider__label">
          {props.label ?? "New"}
        </span>
      </div>
    </Show>
  );
}

export interface MessageListProps {
  label?: string;
  /** The room between blocks, inside one and around a day's divider (Roomy by default), or between lines in one line. */
  spacing?: MessageSpacing;
  layout?: MessageLayout;
  /** The list's avatars, for the rows it draws that are not messages (a join line's glyph). */
  avatars?: MessageAvatars;
  children?: JSX.Element;
  /** The pane that scrolls, and its scrolling: whether it is at its newest end, where a jump goes. */
  scrollRef?: (el: HTMLDivElement) => void;
  onScroll?: () => void;
  class?: string;
}

/** Bottom-anchored scrolling log. */
export function MessageList(props: MessageListProps) {
  return (
    <div ref={(el) => props.scrollRef?.(el)} onScroll={() => props.onScroll?.()} class={cx("lu-msglist", props.spacing && props.spacing !== "roomy" && "lu-msglist--" + props.spacing, props.layout === "one_line" && "lu-msglist--lines", props.avatars === "hidden" && "lu-msglist--avatars-hidden", props.class)}>
      <div role="log" aria-label={props.label ?? "Messages"} class="lu-msglist__log">
        {props.children}
      </div>
    </div>
  );
}

export interface ThreadEntry {
  author: string;
  initials: string;
  tone?: Tone;
  time?: string;
  datetime?: string;
  text: string;
}

export interface ThreadPanelProps {
  parent?: ThreadEntry;
  earlier?: number;
  replies?: ThreadEntry[];
  channel?: string;
  onShowEarlier?: () => void;
  onSend?: () => void;
  class?: string;
}

/** A thread in the side panel. */
export function ThreadPanel(props: ThreadPanelProps) {
  const rid = fieldId();
  const aid = fieldId();
  return (
    <div class={cx("lu-thread", props.class)}>
      <div class="lu-thread__body">
        <Show when={props.parent}>
          {(p) => (
            <Message {...p()} variant="parent">
              {p().text}
            </Message>
          )}
        </Show>
        <Show when={props.earlier}>
          <Button variant="text" onClick={() => props.onShowEarlier?.()} class="lu-thread__earlier">
            Show {props.earlier} earlier {props.earlier === 1 ? "reply" : "replies"}
          </Button>
        </Show>
        <For each={props.replies ?? []}>
          {(r) => (
            <Message {...r} variant="reply">
              {r.text}
            </Message>
          )}
        </For>
      </div>
      <div class="lu-thread__foot">
        <div class="lu-thread__field">
          <label for={rid} class="lu-sr">
            Reply in thread
          </label>
          <input id={rid} type="text" placeholder="Reply…" autocomplete="off" class="lu-thread__input" />
          <IconButton icon="send" variant="send" size={32} label="Send reply" onClick={() => props.onSend?.()} />
        </div>
        <Show when={props.channel}>
          <Checkbox id={aid} size="sm" label={"Also send to #" + props.channel} />
        </Show>
      </div>
    </div>
  );
}

/* ── Replies, reactions, mentions (architecture 6.6) ─────── */

export interface ReplyBarProps {
  /** Who wrote what is quoted; none where that is not known. */
  name?: string;
  /**
   * Whether the reply notifies them: the "@ On" switch. Left out where it
   * has nothing to say (a reply to oneself, a personal conversation).
   */
  mention?: boolean;
  onMention?: (on: boolean) => void;
  onCancel?: () => void;
  class?: string;
}

/** The strip atop the composer's frame while replying: who is quoted, whether they are told, and the way out. */
export function ReplyBar(props: ReplyBarProps) {
  return (
    <div class={cx("lu-replybar", props.class)} role="status">
      <Icon name="reply" size={15} strokeWidth={1.9} class="lu-replybar__icon" />
      <p class="lu-replybar__who">
        Replying to <Show when={props.name} fallback="a message">{(name) => <strong>{name()}</strong>}</Show>
      </p>
      <Show when={props.mention !== undefined}>
        <button
          type="button"
          class={cx("lu-replybar__mention", props.mention && "is-on")}
          aria-pressed={!!props.mention}
          aria-label={"Mention " + (props.name || "them")}
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => props.onMention?.(!props.mention)}
        >
          <Icon name="mention" size={13} strokeWidth={2.2} />
          {props.mention ? "On" : "Off"}
        </button>
      </Show>
      <IconButton icon="close" label="Cancel reply" size={28} iconSize={14} strokeWidth={2.2} class="lu-replybar__cancel" onMouseDown={(e) => e.preventDefault()} onClick={() => props.onCancel?.()} />
    </div>
  );
}

export interface ComposerNoteProps {
  children?: JSX.Element;
  actions?: { label: string; onClick?: () => void; primary?: boolean; disabled?: boolean }[];
  error?: string;
  class?: string;
}

/** A line over the composer after sending, with what can be done about it: "Ada isn’t in #design, so they won’t be notified." */
export function ComposerNote(props: ComposerNoteProps) {
  return (
    <div class={cx("lu-composernote", props.class)} role="status">
      <p class="lu-composernote__text">{props.children}</p>
      <Show when={props.actions?.length}>
        <div class="lu-composernote__actions">
          <For each={props.actions}>
            {(a) =>
              a.primary ? (
                <Button size="sm" disabled={a.disabled} onClick={() => a.onClick?.()}>
                  {a.label}
                </Button>
              ) : (
                <Button variant="text" disabled={a.disabled} onClick={() => a.onClick?.()}>
                  {a.label}
                </Button>
              )
            }
          </For>
        </div>
      </Show>
      <Show when={props.error}>
        <p class="lu-composernote__error" role="alert">
          {props.error}
        </p>
      </Show>
    </div>
  );
}

export interface MentionOption {
  /** An account id, or `everyone`. */
  id: string;
  name: string;
  handle?: string;
  initials?: string;
  src?: string;
  tone?: Tone;
  /** A muted word at the row's end: "Not in #design", "Notify everyone in #design". */
  note?: string;
  everyone?: boolean;
}

export interface MentionListProps {
  /** The list's id, which the composer's field names as what it controls. */
  id: string;
  options: MentionOption[];
  /** The row the arrow keys are on. */
  active: number;
  onPick?: (option: MentionOption) => void;
  onActive?: (index: number) => void;
  class?: string;
}

/** The @ list over the composer: who a mention can name, those in the channel first. */
export function MentionList(props: MentionListProps) {
  return (
    <ul id={props.id} role="listbox" aria-label="People to mention" class={cx("lu-mentionlist", props.class)}>
      <For each={props.options}>
        {(o, i) => (
          <li
            id={props.id + "-" + i()}
            role="option"
            aria-selected={i() === props.active}
            class={cx("lu-mentionlist__row", i() === props.active && "is-active")}
            // The field keeps the focus; the press picks.
            onMouseDown={(e) => e.preventDefault()}
            onMouseMove={() => i() !== props.active && props.onActive?.(i())}
            onClick={() => props.onPick?.(o)}
          >
            <Show
              when={!o.everyone}
              fallback={
                <span class="lu-mentionlist__all" aria-hidden="true">
                  <Icon name="mention" size={15} strokeWidth={2} />
                </span>
              }
            >
              <Avatar initials={o.initials ?? ""} src={o.src} tone={o.tone} size={28} />
            </Show>
            <span class="lu-mentionlist__name">{o.everyone ? "@everyone" : o.name}</span>
            <Show when={o.handle && !o.everyone}>
              <span class="lu-mentionlist__handle">@{o.handle}</span>
            </Show>
            <Show when={o.note}>
              <span class="lu-mentionlist__note">{o.note}</span>
            </Show>
          </li>
        )}
      </For>
    </ul>
  );
}

export interface EmojiPickerProps {
  /** This device's most used, for "Frequently used". */
  frequent: string[];
  onPick?: (emoji: string) => void;
  class?: string;
}

/** Every emoji by Unicode's groups, searchable, with what this device uses most at the top. */
export function EmojiPicker(props: EmojiPickerProps) {
  const [groups, setGroups] = createSignal<EmojiGroup[] | null>(null);
  const [failed, setFailed] = createSignal(false);
  const [query, setQuery] = createSignal("");
  onMount(() => {
    loadEmoji()
      .then(setGroups)
      .catch(() => setFailed(true));
  });
  const named = createMemo(() => {
    const names = new Map<string, string>();
    for (const g of groups() ?? []) for (const e of g.emoji) names.set(e.emoji, e.name);
    return names;
  });
  const found = createMemo(() => (query().trim() ? (groups() ?? []).flatMap((g) => g.emoji.filter((e) => emojiMatches(e, query()))) : null));
  const cell = (emoji: string, name: string) => (
    <button type="button" class="lu-emojipicker__emoji" aria-label={name} title={name} onClick={() => props.onPick?.(emoji)}>
      {emoji}
    </button>
  );
  return (
    <section role="dialog" aria-label="Emoji" class={cx("lu-emojipicker", props.class)}>
      <div class="lu-emojipicker__search">
        <SearchField label="Search emoji" value={query()} onInput={(e) => setQuery(e.currentTarget.value)} />
      </div>
      <div class="lu-emojipicker__scroll">
        <Show when={!failed()} fallback={<p class="lu-emojipicker__none">The emoji could not be loaded.</p>}>
          <Show
            when={found()}
            fallback={
              <>
                <h3 class="lu-emojipicker__group">Frequently used</h3>
                <div class="lu-emojipicker__grid">
                  <For each={props.frequent}>{(e) => cell(e, named().get(e) ?? e)}</For>
                </div>
                <For each={groups() ?? []}>
                  {(g) => (
                    <>
                      <h3 class="lu-emojipicker__group">{g.name}</h3>
                      <div class="lu-emojipicker__grid">
                        <For each={g.emoji}>{(e) => cell(e.emoji, e.name)}</For>
                      </div>
                    </>
                  )}
                </For>
              </>
            }
          >
            {(list) => (
              <Show when={list().length > 0} fallback={<p class="lu-emojipicker__none">No emoji match that.</p>}>
                <div class="lu-emojipicker__grid">
                  <For each={list()}>{(e) => cell(e.emoji, e.name)}</For>
                </div>
              </Show>
            )}
          </Show>
        </Show>
      </div>
    </section>
  );
}

export interface QuoteReplyButtonProps {
  /** Where the selection ends, in the window. */
  left?: number;
  top?: number;
  /** In the flow instead of over the window (the gallery). */
  inline?: boolean;
  onPress?: () => void;
  class?: string;
}

/** Beside text selected in one message: quotes just that. */
export function QuoteReplyButton(props: QuoteReplyButtonProps) {
  const button = () => (
    <button
      type="button"
      class={cx("lu-quotereply", props.inline && "lu-quotereply--inline", props.class)}
      style={props.inline ? undefined : { left: (props.left ?? 0) + "px", top: (props.top ?? 0) + "px" }}
      // A press must not take the selection away before it is read.
      onMouseDown={(e) => e.preventDefault()}
      onClick={() => props.onPress?.()}
    >
      <Icon name="reply" size={15} strokeWidth={2} />
      Quote reply
    </button>
  );
  return (
    <Show when={!props.inline} fallback={button()}>
      <Portal>{button()}</Portal>
    </Show>
  );
}

export interface MentionEntryProps {
  /** Where it was said: "#design", or "Acme · #design" across servers. */
  where: string;
  onOpen?: () => void;
  /** The message, drawn as in the timeline. */
  children?: JSX.Element;
  class?: string;
}

/** One item of a Mentions page: where, over the message; pressing it goes there. */
export function MentionEntry(props: MentionEntryProps) {
  return (
    <li class={cx("lu-mentionentry", props.class)} onClick={() => props.onOpen?.()}>
      <button
        type="button"
        class="lu-mentionentry__where"
        onClick={(e) => {
          e.stopPropagation();
          props.onOpen?.();
        }}
      >
        {props.where}
      </button>
      {props.children}
    </li>
  );
}

/* ── Surfaces & layout ───────────────────────────────────── */

export interface PanelProps extends JSX.HTMLAttributes<HTMLElement> {
  as?: keyof JSX.IntrinsicElements;
}

/** Rounded surface panel. */
export function Panel(props: PanelProps) {
  const [local, rest] = splitProps(props, ["as", "class", "children"]);
  return (
    <Dynamic component={(local.as ?? "div") as "div"} class={cx("lu-panel", local.class)} {...(rest as JSX.HTMLAttributes<HTMLDivElement>)}>
      {local.children}
    </Dynamic>
  );
}

export interface CardProps {
  title?: string;
  subtitle?: string;
  children?: JSX.Element;
  class?: string;
}

/** Sign-in card. */
export function Card(props: CardProps) {
  const hid = fieldId();
  return (
    <section aria-labelledby={props.title ? hid : undefined} class={cx("lu-card", props.class)}>
      <Show when={props.title}>
        <div class="lu-card__head">
          <h1 id={hid} class="lu-card__title">
            {props.title}
          </h1>
          <Show when={props.subtitle}>
            <p class="lu-card__subtitle">{props.subtitle}</p>
          </Show>
        </div>
      </Show>
      {props.children}
    </section>
  );
}

export interface SidePanelProps {
  tabs: { id: string; label: string; count?: number }[];
  value?: string;
  defaultValue?: string;
  onChange?: (id: string) => void;
  label?: string;
  children?: JSX.Element | ((tab: string) => JSX.Element);
  class?: string;
}

/** Right panel with tabs. */
export function SidePanel(props: SidePanelProps) {
  const [inner, setInner] = createSignal(props.defaultValue ?? props.tabs[0]?.id ?? "");
  const cur = () => (props.value !== undefined ? props.value : inner());
  const tab = () => props.tabs.find((t) => t.id === cur());
  const panel = () => (typeof props.children === "function" ? props.children(cur()) : props.children);
  return (
    <Panel class={cx("lu-sidepanel", props.class)}>
      <div class="lu-sidepanel__head">
        {/* A panel with one thing in it has nothing to switch between: the tab's words are its heading. */}
        <Show
          when={props.tabs.length > 1}
          fallback={
            <h2 class="lu-sidepanel__title">
              {tab()?.label}
              <Show when={tab()?.count !== undefined}>
                <span class="lu-segtabs__count">{tab()?.count}</span>
              </Show>
            </h2>
          }
        >
          <SegmentedTabs
            tabs={props.tabs}
            value={cur()}
            onChange={(id) => {
              if (props.value === undefined) setInner(id);
              props.onChange?.(id);
            }}
          />
        </Show>
      </div>
      <div role="tabpanel" aria-label={tab()?.label ?? props.label ?? "Members and threads"} class="lu-sidepanel__body">
        {panel()}
      </div>
    </Panel>
  );
}

/** The search field at the head of a side panel's body. */
export function SidePanelSearch(props: { children?: JSX.Element }) {
  return <div class="lu-sidepanel__search">{props.children}</div>;
}

/** The part of a side panel's body that scrolls. */
export function SidePanelScroll(props: { children?: JSX.Element }) {
  return <div class="lu-sidepanel__scroll">{props.children}</div>;
}

export interface AppShellProps {
  rail?: JSX.Element;
  sidebar?: JSX.Element;
  sidebarLabel?: string;
  main?: JSX.Element;
  aside?: JSX.Element;
  /** The frame's height; the window's when left out. */
  height?: number | string;
  /** Folded (a phone), the conversation is over the rail and the sidebar; otherwise they are the screen. */
  reading?: boolean;
  /** In a window too narrow for three columns, the aside is over the conversation's end. */
  asideOpen?: boolean;
  /** In a window wide enough for it, the aside is put away from the row. */
  asideHidden?: boolean;
  onAsideClose?: () => void;
  class?: string;
}

/** The Quiet Split layout. */
export function AppShell(props: AppShellProps) {
  // Resolved once: a `when` that evaluated `props.aside` would build the
  // aside's component inside the condition, and the insert would build it
  // again; the one here is what the column shows, and the column shows
  // only when there is something to show.
  const aside = children(() => props.aside);
  return (
    <div class={cx("lu-shell", !!aside() && "has-aside", props.reading && "is-reading", props.asideOpen && "is-aside-open", props.asideHidden && "is-aside-hidden", props.class)} style={props.height !== undefined ? { height: typeof props.height === "number" ? props.height + "px" : props.height } : undefined}>
      <div class="lu-shell__side">
        {props.rail}
        <nav aria-label={props.sidebarLabel ?? "Channels and messages"} class="lu-shell__sidebar">
          {props.sidebar}
        </nav>
      </div>
      <main class="lu-shell__main">
        <Panel class="lu-shell__conversation">{props.main}</Panel>
      </main>
      <Show when={aside()}>
        <button type="button" tabindex={-1} aria-label="Close the side panel" class="lu-shell__scrim" onClick={() => props.onAsideClose?.()}></button>
        <aside aria-label="Members and threads" class="lu-shell__aside">
          {aside()}
        </aside>
      </Show>
    </div>
  );
}

/* ── Over the frame ──────────────────────────────────────── */

export interface PopoverProps {
  /** What opened it: it is placed against this, and focus goes back to it. */
  anchor?: HTMLElement | null;
  /**
   * `below` hangs it under the anchor with their left edges in line (over
   * it when there is no room under, beside it at the window's end);
   * `beside` stands it to the anchor's right with their feet in line, as
   * the account menu stands by the rail; `right` stands it to the anchor's
   * right with their tops in line, as a rail tile's menu.
   */
  placement?: "below" | "beside" | "right";
  closeLabel?: string;
  onClose: () => void;
  children?: JSX.Element;
}

const FOCUSABLE = 'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/** A menu, a card or a small form over the frame: placed against what opened it, closed by Escape or a press outside, with focus kept inside. */
export function Popover(props: PopoverProps) {
  let box: HTMLDivElement | undefined;
  const [at, setAt] = createSignal<{ left: number; top: number } | null>(null);
  // The board's rules for where a card and a menu go, kept 8 inside the window.
  const place = () => {
    if (!box) return;
    const w = box.offsetWidth;
    const h = box.offsetHeight;
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    const r = props.anchor?.isConnected ? props.anchor.getBoundingClientRect() : null;
    let left = (vw - w) / 2;
    let top = (vh - h) / 2;
    if (r && props.placement === "beside") {
      left = r.right + 12;
      top = r.bottom - h;
    } else if (r && props.placement === "right") {
      left = r.right + 12;
      top = r.top;
    } else if (r && r.right + w + 16 > vw && r.left - w - 12 >= 8) {
      left = r.left - w - 12;
      top = r.top - 8;
    } else if (r) {
      left = r.left;
      top = r.bottom + 8;
      if (top + h > vh - 8) top = r.top - h - 8;
    }
    setAt({ left: Math.round(Math.max(8, Math.min(left, vw - w - 8))), top: Math.round(Math.max(8, Math.min(top, vh - h - 8))) });
  };
  const inside = () => (box ? [...box.querySelectorAll<HTMLElement>(FOCUSABLE)] : []);
  // On the document, not on the popover: a browser that does not focus a
  // button it is clicked on (Safari) leaves the focus on the page's body
  // after a press inside, and Escape must still close what is open.
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      props.onClose();
      return;
    }
    if (e.key !== "Tab") return;
    const all = inside();
    const first = all[0];
    const last = all[all.length - 1];
    if (!first || !last) return;
    const at = document.activeElement;
    const within = !!at && !!box?.contains(at);
    if (e.shiftKey && (at === first || !within)) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && (at === last || !within)) {
      e.preventDefault();
      first.focus();
    }
  };
  onMount(() => {
    place();
    inside()[0]?.focus({ preventScroll: true });
    // What it holds can grow after it opens (a list that arrives, a note).
    const watch = new ResizeObserver(place);
    if (box) watch.observe(box);
    window.addEventListener("resize", place);
    document.addEventListener("keydown", onKeyDown, true);
    onCleanup(() => {
      watch.disconnect();
      window.removeEventListener("resize", place);
      document.removeEventListener("keydown", onKeyDown, true);
      if (props.anchor?.isConnected) props.anchor.focus({ preventScroll: true });
    });
  });
  return (
    <Portal>
      <div class="lu-popover">
        <button type="button" tabindex={-1} aria-label={props.closeLabel ?? "Close"} class="lu-popover__scrim" onClick={() => props.onClose()}></button>
        <div ref={box} class="lu-popover__box" style={{ left: (at()?.left ?? 0) + "px", top: (at()?.top ?? 0) + "px", visibility: at() ? "visible" : "hidden" }}>
          {props.children}
        </div>
      </div>
    </Portal>
  );
}

export interface MenuProps {
  label: string;
  /** A short list of commands, narrower than the account menu. */
  compact?: boolean;
  children?: JSX.Element;
  class?: string;
}

/** A menu: the arrow keys, Home and End move through its items. */
export function Menu(props: MenuProps) {
  const onKeyDown = (e: KeyboardEvent & { currentTarget: HTMLDivElement }) => {
    const items = [...e.currentTarget.querySelectorAll<HTMLElement>('[role^="menuitem"]:not(:disabled)')];
    const i = items.indexOf(document.activeElement as HTMLElement);
    const next = e.key === "ArrowDown" ? (i + 1) % items.length : e.key === "ArrowUp" ? (i - 1 + items.length) % items.length : e.key === "Home" ? 0 : e.key === "End" ? items.length - 1 : -1;
    if (next < 0) return;
    e.preventDefault();
    items[next]?.focus();
  };
  return (
    <div role="menu" aria-label={props.label} class={cx("lu-menu", props.compact && "lu-menu--compact", props.class)} onKeyDown={onKeyDown}>
      {props.children}
    </div>
  );
}

/** Who a menu belongs to, at its head. */
export function MenuHead(props: { name: string; sub?: string; initials: string; src?: string; tone?: Tone; status?: Status | "offline" }) {
  return (
    <div class="lu-menu__head">
      <Avatar initials={props.initials} src={props.src} tone={props.tone ?? "indigo"} size={40} status={props.status} ring="surface" />
      <span class="lu-menu__who">
        <span class="lu-menu__name">{props.name}</span>
        <Show when={props.sub}>
          <span class="lu-menu__sub">{props.sub}</span>
        </Show>
      </span>
    </div>
  );
}

export function MenuRule() {
  return <span role="separator" class="lu-menu__rule"></span>;
}

export function MenuLabel(props: { children?: JSX.Element }) {
  return (
    <span role="presentation" class="lu-menu__label">
      {props.children}
    </span>
  );
}

export interface MenuItemProps {
  icon?: IconName;
  /** A key or a word at the row's end. */
  hint?: string;
  /** The press, which says whether Shift was held. */
  onSelect?: (e: MouseEvent) => void;
  disabled?: boolean;
  /** Something that cannot be taken back, in the danger ink: "Delete folder". */
  danger?: boolean;
  /** It opens a menu of its own beside this one (`expanded` while that is open): a chevron at its end. */
  submenu?: { expanded: boolean };
  children?: JSX.Element;
}

/** A command in a menu. */
export function MenuItem(props: MenuItemProps) {
  return (
    <button
      type="button"
      role="menuitem"
      class={cx("lu-menu__item", props.danger && "is-danger")}
      disabled={props.disabled}
      aria-haspopup={props.submenu ? "menu" : undefined}
      aria-expanded={props.submenu ? props.submenu.expanded : undefined}
      onClick={(e) => props.onSelect?.(e)}
    >
      <Show when={props.icon}>{(icon) => <Icon name={icon()} size={16} strokeWidth={1.9} />}</Show>
      <span class="lu-menu__text">{props.children}</span>
      <Show when={props.hint}>
        <span class="lu-menu__hint">{props.hint}</span>
      </Show>
      <Show when={props.submenu}>
        <Icon name="chevron-down" size={14} strokeWidth={2.2} class="lu-menu__more" />
      </Show>
    </button>
  );
}

export interface MenuRadioProps {
  checked: boolean;
  /** The status this row stands for, as its dot. */
  status?: Status;
  onSelect?: () => void;
  children?: JSX.Element;
}

/** One of a menu's either-or rows: a dot, the words, a check on the one chosen. */
export function MenuRadio(props: MenuRadioProps) {
  return (
    <button type="button" role="menuitemradio" aria-checked={props.checked} class="lu-menu__item" onClick={() => props.onSelect?.()}>
      <Show when={props.status}>{(status) => <span aria-hidden="true" class={cx("lu-menu__dot", "lu-status-" + status())}></span>}</Show>
      <span class="lu-menu__text">{props.children}</span>
      <Show when={props.checked}>
        <Icon name="check" size={16} strokeWidth={2.2} class="lu-menu__check" />
      </Show>
    </button>
  );
}

const PRESENCE_WORD: Record<Status, string> = { online: "Online", away: "Away", dnd: "Do not disturb" };

export interface ProfileCardProps {
  name: string;
  initials: string;
  /** The person's picture. */
  src?: string;
  tone?: Tone;
  handle?: string;
  /** A word beside the name: "You", "New". */
  tag?: string;
  /** Where the person is: a status, `null` for not here, left out where nowhere says. */
  status?: Status | null;
  /** What their profile says of them, a line each. */
  details?: { icon: IconName; label: string; text: string }[];
  /** A line over the action: what pressing it led to. */
  note?: string;
  action?: { label: string; onClick?: () => void; disabled?: boolean };
  /** A second way, beside or under the first: "Message privately". */
  secondary?: { label: string; onClick?: () => void; disabled?: boolean };
  onClose?: () => void;
  class?: string;
}

/** Who someone is, opened from their name or their row. */
export function ProfileCard(props: ProfileCardProps) {
  const hid = fieldId();
  const tone = () => "lu-tone-" + (props.tone ?? "blue");
  return (
    <section role="dialog" aria-labelledby={hid} class={cx("lu-pcard", props.class)}>
      <div class={cx("lu-pcard__band", tone())}>
        <IconButton icon="close" label="Close profile" size={32} strokeWidth={2} class="lu-pcard__close" onClick={() => props.onClose?.()} />
      </div>
      <div class="lu-pcard__body">
        <div aria-hidden="true" class={cx("lu-pcard__avatar", tone())}>
          <Face initials={props.initials} src={props.src} />
          <Show when={props.status}>{(status) => <span class={cx("lu-pcard__dot", "lu-status-" + status())}></span>}</Show>
        </div>
        <div class="lu-pcard__who">
          <div class="lu-pcard__named">
            <h2 id={hid} class="lu-pcard__name">
              {props.name}
            </h2>
            <Show when={props.tag}>
              <Tag tone="accent">{props.tag}</Tag>
            </Show>
          </div>
          <Show when={props.handle}>
            <p class="lu-pcard__handle">@{props.handle}</p>
          </Show>
        </div>
        <Show when={props.status !== undefined}>
          <p class="lu-pcard__presence">
            <span aria-hidden="true" class={"lu-status-" + (props.status ?? "offline")}></span>
            {props.status ? PRESENCE_WORD[props.status] : "Offline"}
          </p>
        </Show>
        <Show when={props.details?.length}>
          <dl class="lu-pcard__details">
            <For each={props.details}>
              {(d) => (
                <div class="lu-pcard__detail">
                  <Icon name={d.icon} size={15} strokeWidth={1.9} />
                  <dt class="lu-sr">{d.label}</dt>
                  <dd>{d.text}</dd>
                </div>
              )}
            </For>
          </dl>
        </Show>
        <Show when={props.note}>
          <p class="lu-pcard__note" role="status">
            {props.note}
          </p>
        </Show>
        <Show when={props.action || props.secondary}>
          <div class={cx("lu-pcard__actions", props.action && props.secondary && "lu-pcard__actions--two")}>
            <Show when={props.action}>
              {(a) => (
                <Button variant="primary" size="md" disabled={a().disabled} onClick={() => a().onClick?.()}>
                  {a().label}
                </Button>
              )}
            </Show>
            <Show when={props.secondary}>
              {(a) => (
                <Button variant="secondary" size="md" disabled={a().disabled} onClick={() => a().onClick?.()}>
                  {a().label}
                </Button>
              )}
            </Show>
          </div>
        </Show>
      </div>
    </section>
  );
}

export interface PopProps {
  title: string;
  subtitle?: string;
  children?: JSX.Element;
  class?: string;
}

/** What a popover holds when it asks for something: a title, a line under it, a small form. */
export function Pop(props: PopProps) {
  const hid = fieldId();
  return (
    <section role="dialog" aria-labelledby={hid} class={cx("lu-pop", props.class)}>
      <div class="lu-pop__head">
        <h2 id={hid} class="lu-pop__title">
          {props.title}
        </h2>
        <Show when={props.subtitle}>
          <p class="lu-pop__subtitle">{props.subtitle}</p>
        </Show>
      </div>
      {props.children}
    </section>
  );
}

export interface ChoiceProps<T extends string | number> {
  label: string;
  options: { value: T; label: string }[];
  value: T;
  onChange?: (value: T) => void;
  class?: string;
}

/** One of a few, on a track as the segmented tabs are. */
export function Choice<T extends string | number>(props: ChoiceProps<T>) {
  const lid = fieldId();
  return (
    <div class={cx("lu-choice", props.class)}>
      <span id={lid} class="lu-choice__label">
        {props.label}
      </span>
      <div role="radiogroup" aria-labelledby={lid} class="lu-choice__track">
        <For each={props.options}>
          {(o) => (
            <button type="button" role="radio" aria-checked={props.value === o.value} class="lu-choice__option" onClick={() => props.onChange?.(o.value)}>
              {o.label}
            </button>
          )}
        </For>
      </div>
    </div>
  );
}

const TONE_WORD: Record<Tone, string> = { peach: "Peach", blue: "Blue", purple: "Purple", mint: "Mint", amber: "Amber", sky: "Sky", stone: "Stone", green: "Green", indigo: "Indigo" };

export interface ToneSwatchesProps {
  label: string;
  value: Tone;
  onChange?: (tone: Tone) => void;
  class?: string;
}

/** The nine tones as round swatches, one chosen: a radio group the arrow keys move through. */
export function ToneSwatches(props: ToneSwatchesProps) {
  const pick = (e: KeyboardEvent & { currentTarget: HTMLDivElement }) => {
    const step = e.key === "ArrowRight" || e.key === "ArrowDown" ? 1 : e.key === "ArrowLeft" || e.key === "ArrowUp" ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    const next = TONES[(TONES.indexOf(props.value) + step + TONES.length) % TONES.length] ?? "blue";
    props.onChange?.(next);
    e.currentTarget.querySelector<HTMLElement>(`[data-tone="${next}"]`)?.focus();
  };
  return (
    <div role="radiogroup" aria-label={props.label} class={cx("lu-swatches", props.class)} onKeyDown={pick}>
      <For each={TONES}>
        {(t) => (
          <button type="button" role="radio" aria-checked={props.value === t} aria-label={TONE_WORD[t]} tabindex={props.value === t ? 0 : -1} data-tone={t} class={cx("lu-swatch", "lu-tone-" + t)} onClick={() => props.onChange?.(t)}>
            <Show when={props.value === t}>
              <Icon name="check" size={14} strokeWidth={2.6} />
            </Show>
          </button>
        )}
      </For>
    </div>
  );
}

export interface FolderSettingsProps {
  name: string;
  /** How the folder reads without a name: the field's placeholder. */
  words: string;
  tone: Tone;
  onName?: (name: string) => void;
  onTone?: (tone: Tone) => void;
  onCancel?: () => void;
  onSave?: () => void;
  class?: string;
}

/** A folder's name and tone (architecture 6.9), opened from its menu. */
export function FolderSettings(props: FolderSettingsProps) {
  const hid = fieldId();
  const lid = fieldId();
  return (
    <section role="dialog" aria-labelledby={hid} class={cx("lu-pop", "lu-pop--folder", props.class)}>
      <h2 id={hid} class="lu-pop__title">
        Folder settings
      </h2>
      <form
        class="lu-pop__form lu-folderset"
        onSubmit={(e) => {
          e.preventDefault();
          props.onSave?.();
        }}
      >
        <div class="lu-folderset__name">
          <TextField label="Name" value={props.name} placeholder={props.words} maxLength={50} autocomplete="off" onInput={(e) => props.onName?.(e.currentTarget.value)} />
          <p class="lu-folderset__hint">Without a name it reads as its servers.</p>
        </div>
        <div class="lu-folderset__tone">
          <span id={lid} class="lu-folderset__legend">
            Colour
          </span>
          <ToneSwatches label="Colour" value={props.tone} onChange={(t) => props.onTone?.(t)} />
        </div>
        <div class="lu-pop__actions">
          <Button variant="secondary" size="md" onClick={() => props.onCancel?.()}>
            Cancel
          </Button>
          <Button variant="primary" size="md" type="submit">
            Save
          </Button>
        </div>
      </form>
    </section>
  );
}

export const icons = Object.keys(ICONS) as IconName[];
