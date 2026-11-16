// Lists whose elements stay the same objects while what they hold
// changes. A conversation's messages come from the core as new objects
// every time they are read again; drawn from those, every row would be
// made again each time, and a video playing in it, an embed shown, a body
// opened or an edit being typed would go with it. So what is read is kept
// in a store by id: a message read again is the object it was, with the
// fields that changed written into it, and a list draws its rows from the
// objects themselves.
import { createComputed } from "solid-js";
import { createStore, produce, unwrap } from "solid-js/store";

/** The names of `T`'s fields that hold text. */
type TextKey<T> = { [K in keyof T]-?: T[K] extends string ? K : never }[keyof T] & string;

/**
 * How a list is kept: the field its elements are told apart by, and the
 * lists inside each element that are kept the same way. Every other field
 * is a value: written in whole when what it holds has changed, never
 * changed in place, so what was read from it before stays as it was.
 */
export interface Kept<T> {
  key: TextKey<T>;
  lists?: { [K in keyof T]?: T[K] extends readonly (infer U)[] ? Kept<U> : never };
}

type Rec = Record<string, unknown>;
type AnyKept = { key: string; lists?: Record<string, AnyKept | undefined> };

/** The same elements in the same order. */
export function sameList<T>(a: readonly T[], b: readonly T[]): boolean {
  return a.length === b.length && a.every((v, i) => v === b[i]);
}

/** The same value: what the core hands over (text, numbers, lists and records of them) compared by what it holds. */
export function sameValue(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false;
  if (Array.isArray(a) || Array.isArray(b)) return Array.isArray(a) && Array.isArray(b) && a.length === b.length && a.every((v, i) => sameValue(v, b[i]));
  if (Object.getPrototypeOf(a) !== Object.prototype || Object.getPrototypeOf(b) !== Object.prototype) return false;
  const keys = Object.keys(a);
  return keys.length === Object.keys(b).length && keys.every((k) => Object.hasOwn(b, k) && sameValue((a as Rec)[k], (b as Rec)[k]));
}

/** An element as the list holds it from now on: a copy, with the lists kept inside it copied too, so nothing handed in is ever written to. */
function taken(next: Rec, kept: AnyKept): Rec {
  const copy = { ...next };
  for (const [field, inner] of Object.entries(kept.lists ?? {})) {
    const list = next[field];
    if (inner && Array.isArray(list)) copy[field] = list.map((e) => taken(e as Rec, inner));
  }
  return copy;
}

function keepList(held: Rec[], next: readonly Rec[], kept: AnyKept): void {
  const raw = unwrap(held);
  // Where each key is held now; one held twice is matched in order.
  const at = new Map<unknown, number[]>();
  raw.forEach((e, i) => {
    const places = at.get(e[kept.key]);
    if (places) places.push(i);
    else at.set(e[kept.key], [i]);
  });
  const now = next.map((n) => {
    const i = at.get(n[kept.key])?.shift();
    if (i === undefined) return taken(n, kept);
    const was = held[i] as Rec;
    keepFields(was, n, kept);
    return was;
  });
  for (let i = 0; i < now.length; i++) if (unwrap(now[i]) !== raw[i]) held[i] = now[i] as Rec;
  if (raw.length !== now.length) held.length = now.length;
}

function keepFields(held: Rec, next: Rec, kept: AnyKept): void {
  const raw = unwrap(held);
  for (const field of Object.keys(next)) {
    const inner = kept.lists?.[field];
    const was = raw[field];
    const to = next[field];
    if (inner && Array.isArray(was) && Array.isArray(to)) keepList(held[field] as Rec[], to as Rec[], inner);
    else if (inner && Array.isArray(to)) held[field] = to.map((e) => taken(e as Rec, inner));
    else if (!sameValue(was, to)) held[field] = to;
  }
  for (const field of Object.keys(raw)) if (!Object.hasOwn(next, field)) delete held[field];
}

/**
 * Writes `next` into `held`, a list kept as `kept` says: an element held
 * before under the same key stays the object it was, with the fields that
 * changed written into it; a new one is taken as a copy; one no longer in
 * `next` goes, keeping what it last held. The order is `next`'s.
 */
export function keep<T extends object>(held: T[], next: readonly T[], kept: Kept<T>): void {
  keepList(held as Rec[], next as readonly Rec[], kept as AnyKept);
}

/**
 * `list` kept by id, for the life of the owner it is called under: a
 * store's list of the same elements, each the same object for as long as
 * its key stays in the list, read field by field (a row drawn from one is
 * told of the fields it read, and of no others).
 */
export function keptById<T extends object>(list: () => readonly T[], kept: Kept<T>): T[] {
  const [held, setHeld] = createStore<T[]>([]);
  createComputed(() => {
    // Without the stores' proxies of whatever it was read from: this store holds what they hold.
    const next = unwrap(list().slice());
    setHeld(produce((draft) => keep(draft, next, kept)));
  });
  return held;
}
