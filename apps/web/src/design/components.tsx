// The Lumia design system's components, ported to Solid from the design
// artifact's React source on 2026-11-02 (assets/README.md). Same class
// names, same markup, same ARIA; the stylesheet is components.css and every
// value in it comes from tokens.css.
//
// Solid conventions: props are read lazily (never destructured), `class`
// instead of `className`, `for` instead of `htmlFor`, `style` objects with
// CSS property names.

import { For, Show, children, createEffect, createSignal, createUniqueId, on, onCleanup, onMount, splitProps, type JSX } from "solid-js";
import { Dynamic, Portal } from "solid-js/web";
import { ICONS } from "./icons";

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
  icon: IconName;
  label: string;
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
      class={cx("lu-reaction", on() && "is-on", props.class)}
      onClick={() => {
        const next = !on();
        if (!controlled()) setInner(next);
        props.onToggle?.(next);
      }}
    >
      <Icon name={props.icon} size={14} strokeWidth={2} />
      <span class="lu-sr">{props.label},</span>
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
}

/** Message bar with tools, send and a hint row. */
export function Composer(props: ComposerProps) {
  const [local, input] = splitProps(props, ["placeholder", "label", "typing", "hint", "tools", "id", "onSend", "onAttach", "class"]);
  const fid = fieldId(local.id);
  const placeholder = () => local.placeholder ?? "Message";
  const hint = () => local.hint ?? "Return to send";
  const tools = () => local.tools ?? true;
  const typing = () => (Array.isArray(local.typing) ? (local.typing.length ? local.typing : undefined) : local.typing || undefined);
  return (
    <div class={cx("lu-composer", local.class)}>
      <div class={cx("lu-composer__field", !tools() && "is-bare")}>
        <Show when={tools()}>
          <IconButton icon="plus" label="Attach a file" class="lu-composer__attach" onClick={() => local.onAttach?.()} />
        </Show>
        <label for={fid} class="lu-sr">
          {local.label ?? placeholder()}
        </label>
        <input id={fid} type="text" placeholder={placeholder()} autocomplete="off" class="lu-composer__input" {...input} />
        <Show when={tools()}>
          <IconButton glyph="Aa" label="Text formatting" />
          <IconButton icon="emoji" label="Emoji" />
          <IconButton icon="mention" label="Mention someone" strokeWidth={1.9} />
        </Show>
        <IconButton icon="send" variant="send" size={40} label="Send message" class="lu-composer__send" onClick={() => local.onSend?.()} />
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
                          {m.name}
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
  name: string;
  initials: string;
  current?: boolean;
  ink?: "default" | "indigo" | "green" | "amber";
  unread?: boolean;
  mentions?: number;
  onClick?: () => void;
}

export interface ServerRailProps {
  logoSrc: string;
  homeLabel?: string;
  homeUnread?: number;
  /** Home is the current place (no server is). */
  homeCurrent?: boolean;
  servers?: RailServer[];
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

/** The far-left server rail. */
export function ServerRail(props: ServerRailProps) {
  const homeLabel = () => props.homeLabel ?? "Lumia Home — personal messages";
  return (
    <nav aria-label="Home and servers" class={cx("lu-rail", props.class)}>
      <button type="button" class={cx("lu-rail__home", props.homeCurrent && "is-current")} aria-current={props.homeCurrent ? "true" : undefined} onClick={() => props.onHome?.()} aria-label={homeLabel() + (props.homeUnread ? ", " + props.homeUnread + " unread" : "")}>
        <Show when={props.logoSrc}>
          <img src={props.logoSrc} alt="" width={36} height={36} />
        </Show>
        <Show when={props.homeUnread}>
          <Badge count={props.homeUnread} ring="rail" aria-hidden="true" class="lu-rail__badge lu-rail__badge--home" />
        </Show>
      </button>
      <span aria-hidden="true" class="lu-rail__divider"></span>
      <ul class="lu-rail__list">
        <For each={props.servers ?? []}>
          {(s) => {
            const extra = () => (s.mentions ? ", " + s.mentions + (s.mentions === 1 ? " mention" : " mentions") : s.unread ? ", unread messages" : "");
            return (
              <li>
                <button
                  type="button"
                  aria-label={s.name + extra()}
                  aria-current={s.current ? "true" : undefined}
                  onClick={() => s.onClick?.()}
                  class={cx("lu-rail__server", s.current && "is-current", "lu-tile-ink-" + (s.ink ?? "default"))}
                >
                  {s.initials}
                  <Show when={s.mentions} fallback={<Show when={s.unread}><Badge dot ring="rail" class="lu-rail__dot" /></Show>}>
                    <Badge count={s.mentions} ring="rail" aria-hidden="true" class="lu-rail__badge" />
                  </Show>
                </button>
              </li>
            );
          }}
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
    </nav>
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
  items: { icon: IconName; label: string; count?: number; href?: string; onClick?: () => void }[];
  class?: string;
}

/** Threads / Mentions / Saved tiles. */
export function QuickTiles(props: QuickTilesProps) {
  return (
    <div class={cx("lu-quick", props.class)}>
      <For each={props.items}>
        {(it) => (
          <a href={it.href ?? "#"} onClick={(e) => pressed(e, it)} class="lu-quick__tile">
            <Icon name={it.icon} size={18} strokeWidth={1.8} />
            <span>{it.label}</span>
            <Show when={it.count}>
              <Badge count={it.count} size="sm" label={it.count + " new"} class="lu-quick__badge" />
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
}

function RowLead(props: { item: ChannelItem }) {
  return (
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
  );
}

export interface ChannelListProps {
  sections: { title: string; items: ChannelItem[] }[];
  class?: string;
}

/** Sidebar sections of channels and DMs. */
export function ChannelList(props: ChannelListProps) {
  return (
    <div class={cx("lu-chlist", props.class)}>
      <For each={props.sections}>
        {(sec) => (
          <section>
            <h2 class="lu-chlist__title">{sec.title}</h2>
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
  class?: string;
}

/** Inline @mention. */
export function Mention(props: MentionProps) {
  return (
    <a href={props.href ?? "#"} class={cx("lu-mention", props.class)}>
      {props.children}
    </a>
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
  onReact?: () => void;
  onThread?: () => void;
  onMore?: () => void;
  floating?: boolean;
  class?: string;
}

/** Hover toolbar for a message. */
export function MessageActions(props: MessageActionsProps) {
  return (
    <div class={cx("lu-actions", (props.floating ?? true) && "is-floating", props.class)}>
      <IconButton icon="reaction-add" label="Add reaction" size={30} onClick={() => props.onReact?.()} />
      <IconButton icon="thread" label="Open thread" size={30} onClick={() => props.onThread?.()} />
      <IconButton icon="more" label="More actions" size={30} onClick={() => props.onMore?.()} />
    </div>
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
  onAddReaction?: () => void;
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
  class?: string;
}

export interface MessagePart {
  time?: string;
  datetime?: string;
  children?: JSX.Element | string[];
}

/** One message, or one person's run of them as a block. */
export function Message(props: MessageProps) {
  const variant = () => props.variant ?? "default";
  const compact = () => variant() !== "default";
  const avatar = () => <Avatar initials={props.initials} src={props.src} tone={props.tone ?? "blue"} size={compact() ? 28 : 40} />;
  return (
    <article class={cx("lu-msg", "lu-msg--" + variant(), props.highlighted && "is-highlighted", props.class)}>
      <Show when={props.onOpenProfile} fallback={avatar()}>
        {/* The name is the control; the avatar is the same one for a pointer. */}
        <button type="button" class="lu-msg__avatar" tabindex={-1} aria-hidden="true" onClick={(e) => props.onOpenProfile?.(e.currentTarget)}>
          {avatar()}
        </button>
      </Show>
      <div class="lu-msg__main">
        <div class="lu-msg__head">
          <Show when={props.onOpenProfile} fallback={<span class="lu-msg__author">{props.author}</span>}>
            <button type="button" class="lu-msg__author lu-msg__name" aria-haspopup="dialog" onClick={(e) => props.onOpenProfile?.(e.currentTarget)}>
              {props.author}
            </button>
          </Show>
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
        <For each={props.parts ?? []}>
          {(part) => (
            <div class="lu-msg__part">
              <Show when={part.time}>
                <time datetime={part.datetime} class="lu-msg__parttime">
                  <span class="lu-sr">{props.author}, </span>
                  {part.time}
                </time>
              </Show>
              <div class="lu-msg__body">
                <Paras>{part.children}</Paras>
              </div>
            </div>
          )}
        </For>
        <Show when={props.attachment}>{(a) => <Attachment {...a()} />}</Show>
        <Show when={props.thread}>{(t) => <ThreadSummary {...t()} />}</Show>
        <Show when={props.reactions && props.reactions.length}>
          <div class="lu-msg__reactions">
            <For each={props.reactions}>{(r) => <ReactionChip {...r} />}</For>
            <Show when={props.onAddReaction}>
              <button type="button" aria-label="Add reaction" class="lu-reaction-add" onClick={() => props.onAddReaction?.()}>
                <Icon name="reaction-add" size={15} strokeWidth={1.9} />
              </button>
            </Show>
          </div>
        </Show>
      </div>
      <Show when={(props.actions ?? true) && !compact()}>
        <MessageActions onReact={props.onReact} onThread={props.onThread} onMore={props.onMore} />
      </Show>
    </article>
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
      <div class={cx("lu-divider", "lu-divider--new", props.class)}>
        <span aria-hidden="true" class="lu-divider__rule"></span>
        <span class="lu-divider__label">{props.label ?? "New"}</span>
      </div>
    </Show>
  );
}

export interface MessageListProps {
  label?: string;
  children?: JSX.Element;
  class?: string;
}

/** Bottom-anchored scrolling log. */
export function MessageList(props: MessageListProps) {
  return (
    <div class={cx("lu-msglist", props.class)}>
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
    <div class={cx("lu-shell", !!aside() && "has-aside", props.reading && "is-reading", props.asideOpen && "is-aside-open", props.class)} style={props.height !== undefined ? { height: typeof props.height === "number" ? props.height + "px" : props.height } : undefined}>
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
   * the account menu stands by the rail.
   */
  placement?: "below" | "beside";
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
    const items = [...e.currentTarget.querySelectorAll<HTMLElement>('[role^="menuitem"]')];
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
  onSelect?: () => void;
  children?: JSX.Element;
}

/** A command in a menu. */
export function MenuItem(props: MenuItemProps) {
  return (
    <button type="button" role="menuitem" class="lu-menu__item" onClick={() => props.onSelect?.()}>
      <Show when={props.icon}>{(icon) => <Icon name={icon()} size={16} strokeWidth={1.9} />}</Show>
      <span class="lu-menu__text">{props.children}</span>
      <Show when={props.hint}>
        <span class="lu-menu__hint">{props.hint}</span>
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
        <Show when={props.action}>
          {(a) => (
            <div class="lu-pcard__actions">
              <Button variant="primary" size="md" disabled={a().disabled} onClick={() => a().onClick?.()}>
                {a().label}
              </Button>
            </div>
          )}
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

export const icons = Object.keys(ICONS) as IconName[];
