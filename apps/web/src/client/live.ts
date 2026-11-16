// Rows that stay while what they draw changes. A conversation's messages
// come from the core as new objects every time they are read again; a row
// keyed by such an object would be made again each time, and a video
// playing in it, an embed shown or a body opened would go with it. A row is
// keyed by its message's id instead, and reads the message through a view
// of what the conversation holds of that id now.

/** The same keys, in the same order. */
export function sameIds(a: readonly string[], b: readonly string[]): boolean {
  return a.length === b.length && a.every((k, i) => k === b[i]);
}

/**
 * A view of each element of `byId` by its id, made once per id: each of
 * its fields reads what `byId` holds now (inside a reactive scope, it is
 * followed), or what it last held once the id has gone, since a row is
 * let go after the list has changed. Views of ids no longer held are
 * forgotten as the list changes.
 */
export function liveItems<T extends object>(byId: () => ReadonlyMap<string, T>): (id: string) => T {
  const views = new Map<string, T>();
  let seen: ReadonlyMap<string, T> | undefined;
  return (id) => {
    const map = byId();
    if (map !== seen) {
      seen = map;
      for (const k of views.keys()) if (!map.has(k)) views.delete(k);
    }
    let view = views.get(id);
    if (view) return view;
    let last = map.get(id);
    if (!last) throw new Error(`no item ${id}`);
    const now = () => {
      const v = byId().get(id);
      if (v) last = v;
      return last as T;
    };
    view = {} as T;
    for (const key of Object.keys(last)) Object.defineProperty(view, key, { get: () => now()[key as keyof T], enumerable: true });
    views.set(id, view);
    return view;
  };
}
