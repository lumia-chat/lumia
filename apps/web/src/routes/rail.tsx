// The rail as this account arranged it (architecture 6.9; GitHub #6 and
// #7; the canvas's "Server folders" boards): its servers in an order of
// its own, some in folders, kept at the instance and followed by every
// device. The rules are the core's (`railEdit`): an edit is drawn at once
// and then kept whole with `arrangeRail`; a refusal puts the rail back and
// says why. Whether a folder is open is this browser's (`localStorage`).
// A drag on the rail (components.tsx `ServerRail`) or a tile's menu here
// makes an edit; each one is announced.
import { For, Show, createSignal } from "solid-js";
import { FolderSettings, Menu, MenuItem, MenuLabel, MenuRule, Popover, TONES, type RailDrop, type RailServer, type RailTile, type Tone } from "../design/components";
import { ClientError, type RailEdit, type RailItem, type ServerSummary, type SnapshotSummary } from "../client/bridge";
import { client, queryClient } from "../client/state";

type Folder = Extract<RailItem, { kind: "folder" }>;

/** The rail to draw and edit: the snapshot's, or its servers in order where it has none yet. */
export function railOf(snapshot: SnapshotSummary | undefined): RailItem[] {
  if (!snapshot) return [];
  if (snapshot.rail?.length) return snapshot.rail;
  return snapshot.servers.map((s) => ({ kind: "server", serverId: s.serverId }));
}

const idOf = (item: RailItem) => (item.kind === "server" ? item.serverId : item.folderId);

function toneOf(tone: string): Tone {
  return (TONES as string[]).includes(tone) ? (tone as Tone) : "blue";
}

/* ── Open or closed, this browser's ─────────────────────────── */

const OPEN_KEY = "lumia:rail-open";

function readOpen(): Set<string> {
  try {
    const kept: unknown = JSON.parse(localStorage.getItem(OPEN_KEY) ?? "[]");
    return new Set(Array.isArray(kept) ? kept.filter((x): x is string => typeof x === "string") : []);
  } catch {
    return new Set();
  }
}

const [openFolders, setOpenFolders] = createSignal(readOpen());

/** Opens a closed folder, closes an open one; kept for this browser, and only for folders the rail still has. */
export function toggleFolder(folderId: string) {
  const next = new Set(openFolders());
  if (next.has(folderId)) next.delete(folderId);
  else next.add(folderId);
  const held = new Set(railOf(queryClient.getQueryData<SnapshotSummary>(["snapshot"])).map(idOf));
  for (const id of next) if (!held.has(id)) next.delete(id);
  setOpenFolders(next);
  try {
    localStorage.setItem(OPEN_KEY, JSON.stringify([...next]));
  } catch {
    /* storage may be unavailable; it stays as it is for this page */
  }
}

/* ── How a folder with no name reads, the core's words ─────── */

const [words, setWords] = createSignal(new Map<string, string>());
const asked = new Set<string>();

/** "Acme, Lumia and 2 more" for these names, once the core has said it; their names joined until then. */
export function wordsFor(names: string[]): string {
  const key = JSON.stringify(names);
  const said = words().get(key);
  if (said !== undefined) return said;
  if (!asked.has(key)) {
    asked.add(key);
    void client()
      .railFolderWords(names)
      .then((w) => setWords(new Map(words()).set(key, w)))
      .catch(() => asked.delete(key));
  }
  return names.join(", ");
}

function nameOf(snapshot: SnapshotSummary | undefined, serverId: string): string {
  return snapshot?.servers.find((s) => s.serverId === serverId)?.name ?? "Server";
}

/** A folder's name, or how it reads without one. */
export function folderLabel(snapshot: SnapshotSummary | undefined, folder: Folder): string {
  return folder.name || wordsFor(folder.serverIds.map((id) => nameOf(snapshot, id)));
}

// The tiles drawn last time, by id: one that has not changed is handed back
// as it was, so the rail keeps its elements (and a drag or the focus its
// tile) when the snapshot is read again.
const drawn = new Map<string, { json: string; tile: object }>();
function same<T extends object>(key: string, tile: T): T {
  const json = JSON.stringify(tile);
  const was = drawn.get(key);
  if (was && was.json === json) return was.tile as T;
  drawn.set(key, { json, tile });
  return tile;
}

/** The rail ready for `ServerRail`: each server drawn by `server`, folders with their label and whether they are open. */
export function railTiles(snapshot: SnapshotSummary | undefined, server: (s: ServerSummary) => RailServer): RailTile[] {
  if (!snapshot) return [];
  const byId = new Map(snapshot.servers.map((s) => [s.serverId, s]));
  return railOf(snapshot).flatMap((item): RailTile[] => {
    if (item.kind === "server") {
      const s = byId.get(item.serverId);
      return s ? [same("s" + s.serverId, { kind: "server" as const, ...server(s), id: s.serverId })] : [];
    }
    const inside = item.serverIds.flatMap((id) => byId.get(id) ?? []);
    if (!inside.length) return [];
    return [
      same("f" + item.folderId, {
        kind: "folder" as const,
        id: item.folderId,
        label: folderLabel(snapshot, item),
        tone: toneOf(item.tone),
        open: openFolders().has(item.folderId),
        servers: inside.map((s) => same("i" + s.serverId, { ...server(s), id: s.serverId })),
        onToggle: () => toggleFolder(item.folderId),
      }),
    ];
  });
}

/* ── Edits ───────────────────────────────────────────────────── */

const [said, setSaid] = createSignal("");
const [note, setNote] = createSignal("");
/** What the last move did, for the rail's polite live region. */
export const railSaid = said;
/** Why the last edit was not kept; empty when it was. */
export const railNote = note;
export const dismissRailNote = () => setNote("");

function say(text: string) {
  // Cleared first, so the same words twice are said twice.
  setSaid("");
  setTimeout(() => setSaid(text), 50);
}

function where(items: RailItem[], id: string): { folder?: Folder; at: number } | null {
  const top = items.findIndex((i) => idOf(i) === id);
  if (top >= 0) return { at: top + 1 };
  for (const i of items) {
    if (i.kind !== "folder") continue;
    const k = i.serverIds.indexOf(id);
    if (k >= 0) return { folder: i, at: k + 1 };
  }
  return null;
}

/** A folder's name, or the core's words for its servers. */
async function labelOf(snapshot: SnapshotSummary | undefined, folder: Folder): Promise<string> {
  return folder.name || (await client().railFolderWords(folder.serverIds.map((id) => nameOf(snapshot, id))));
}

/**
 * What a move did, in words: "Acme moved to position 3", "Acme moved into
 * Work", "Acme moved out of Work". A folder is named as it read before the
 * move, as its menu named it.
 */
async function describe(snapshot: SnapshotSummary | undefined, before: RailItem[], after: RailItem[], id: string): Promise<string> {
  const was = where(before, id);
  const now = where(after, id);
  if (!now) return "";
  const self = before.find((i): i is Folder => i.kind === "folder" && i.folderId === id);
  const name = self ? await labelOf(snapshot, self) : nameOf(snapshot, id);
  const into = now.folder && before.find((i): i is Folder => i.kind === "folder" && i.folderId === now.folder?.folderId);
  if (now.folder && now.folder.folderId !== was?.folder?.folderId) {
    if (into) return `${name} moved into ${await labelOf(snapshot, into)}`;
    const others = now.folder.serverIds.filter((s) => s !== id).map((s) => nameOf(snapshot, s));
    return others.length ? `${name} moved into a new folder with ${await client().railFolderWords(others)}` : `${name} moved into a new folder`;
  }
  if (was?.folder && !now.folder) return `${name} moved out of ${await labelOf(snapshot, was.folder)}`;
  if (now.folder) return `${name} moved to position ${now.at} in ${await labelOf(snapshot, now.folder)}`;
  return `${name} moved to position ${now.at}`;
}

function railRefusal(err: unknown): string {
  const ce = err instanceof ClientError ? err : null;
  if (ce?.code === "INVALID_ARGUMENT" && ce.details.field === "name") return "A folder's name is up to 50 characters.";
  if (ce?.code === "INVALID_ARGUMENT" && ce.details.field === "folders") return "There can be at most 100 folders.";
  return err instanceof Error ? err.message : String(err);
}

// Which keep is the latest: an answer to an earlier one is not drawn over a later edit.
let sent = 0;

/**
 * Applies one edit by the core's rules, draws it at once, says what it did
 * (or `words`), and keeps the rail whole at the instance. A refusal puts
 * the rail as it was back and says why.
 */
export async function editRail(edit: RailEdit, subject: string, words?: string) {
  const snap = queryClient.getQueryData<SnapshotSummary>(["snapshot"]);
  if (!snap) return;
  const before = railOf(snap);
  let next: RailItem[];
  try {
    next = await client().railEdit(before, edit);
  } catch (err) {
    setNote(`The rail was not changed: ${railRefusal(err)}`);
    return;
  }
  if (JSON.stringify(next) === JSON.stringify(before)) return;
  const now = queryClient.getQueryData<SnapshotSummary>(["snapshot"]) ?? snap;
  queryClient.setQueryData(["snapshot"], { ...now, rail: next });
  setNote("");
  void (words !== undefined ? Promise.resolve(words) : describe(snap, before, next, subject)).then(say, () => {});
  refocus(subject);
  const seq = ++sent;
  try {
    const kept = await client().arrangeRail(next);
    if (seq === sent) queryClient.setQueryData(["snapshot"], kept);
  } catch (err) {
    if (seq !== sent) return;
    const current = queryClient.getQueryData<SnapshotSummary>(["snapshot"]);
    if (current) queryClient.setQueryData(["snapshot"], { ...current, rail: before });
    setNote(`The rail was not saved: ${railRefusal(err)}`);
    void queryClient.invalidateQueries({ queryKey: ["snapshot"] });
  }
}

// A tile moved by its menu is drawn anew; the keyboard stays on it.
function refocus(id: string) {
  const at = document.activeElement;
  if (at && at !== document.body && !at.closest(".lu-rail")) return;
  setTimeout(() => document.querySelector<HTMLElement>(`.lu-rail [data-rail-tile="${id}"]`)?.focus({ preventScroll: false }), 60);
}

/** A tile let go on the rail: between tiles a move, onto a server a folder of the two, onto a folder into it. */
export async function dropOnRail(id: string, drop: RailDrop) {
  if (drop.kind === "between") return editRail({ op: "moveTo", args: { id, folder: drop.folder, before: drop.before } }, id);
  if (drop.folder) return editRail({ op: "intoFolder", args: { serverId: id, folder: drop.id } }, id);
  const folderId = await client().railNewFolderId();
  return editRail({ op: "merge", args: { dragged: id, onto: drop.id, folderId } }, id);
}

/* ── A tile's menu, and Folder settings ─────────────────────── */

export interface RailMenuProps {
  id: string;
  anchor: HTMLElement;
  snapshot: SnapshotSummary | undefined;
  close: () => void;
  /** Folder settings, over the same tile, or over `anchor`. */
  toSettings: (folderId: string, anchor?: HTMLElement) => void;
}

// The tile of `id` once the rail draws it, for a second at most.
function drawnTile(id: string): Promise<HTMLElement | null> {
  return new Promise((resolve) => {
    let frames = 60;
    const look = () => {
      const tile = document.querySelector<HTMLElement>(`.lu-rail [data-rail-tile="${id}"]`);
      if (tile || --frames <= 0) resolve(tile);
      else requestAnimationFrame(look);
    };
    look();
  });
}

/** A server's menu (Move up, Move down, New folder, Move to folder ›, Remove from folder) or a folder's (Move up, Move down, Folder settings…, Delete folder). */
export function RailMenu(props: RailMenuProps) {
  const rail = () => railOf(props.snapshot);
  const folder = () => rail().find((i): i is Folder => i.kind === "folder" && i.folderId === props.id);
  const home = () => rail().find((i): i is Folder => i.kind === "folder" && i.serverIds.includes(props.id));
  const level = () => home()?.serverIds ?? rail().map(idOf);
  const at = () => level().indexOf(props.id);
  const label = () => {
    const f = folder();
    return f ? folderLabel(props.snapshot, f) : nameOf(props.snapshot, props.id);
  };
  const targets = () => rail().filter((i): i is Folder => i.kind === "folder" && i.folderId !== home()?.folderId);
  const [sub, setSub] = createSignal(false);
  let trigger: HTMLDivElement | undefined;
  const run = (edit: RailEdit, words?: string) => {
    props.close();
    void editRail(edit, props.id, words);
  };
  const openSub = () => {
    setSub(true);
    requestAnimationFrame(() => document.querySelector<HTMLElement>(".lu-menupair [data-submenu] [role='menuitem']")?.focus());
  };
  const keys = (e: KeyboardEvent) => {
    const t = e.target as HTMLElement;
    if (e.key === "ArrowRight" && t.getAttribute("aria-haspopup") === "menu") {
      e.preventDefault();
      openSub();
    } else if (e.key === "ArrowLeft" && t.closest("[data-submenu]")) {
      e.preventDefault();
      setSub(false);
      trigger?.querySelector<HTMLElement>("[aria-haspopup='menu']")?.focus();
    }
  };
  return (
    <Popover anchor={props.anchor} placement="right" onClose={props.close} closeLabel="Close menu">
      <div class="lu-menupair" onKeyDown={keys}>
        <div ref={trigger}>
          <Menu label={label()} class="lu-menu--rail">
            <MenuLabel>{label()}</MenuLabel>
            <MenuItem icon="arrow-up" disabled={at() <= 0} onSelect={() => run({ op: "step", args: { id: props.id, by: -1 } })}>
              Move up
            </MenuItem>
            <MenuItem icon="arrow-down" disabled={at() < 0 || at() >= level().length - 1} onSelect={() => run({ op: "step", args: { id: props.id, by: 1 } })}>
              Move down
            </MenuItem>
            <MenuRule />
            <Show
              when={folder()}
              fallback={
                <>
                  <MenuItem
                    icon="folder-plus"
                    onSelect={() => {
                      props.close();
                      void client()
                        .railNewFolderId()
                        .then((folderId) => {
                          void editRail({ op: "newFolder", args: { serverId: props.id, folderId } }, props.id);
                          // Its Folder settings open, to name it at once (a folder made by a drag opens none).
                          void drawnTile(folderId).then((tile) => tile && props.toSettings(folderId, tile));
                        });
                    }}
                  >
                    New folder
                  </MenuItem>
                  <MenuItem icon="folder-plus" disabled={!targets().length} submenu={{ expanded: sub() }} onSelect={() => (sub() ? setSub(false) : openSub())}>
                    Move to folder
                  </MenuItem>
                  <Show when={home()}>
                    <MenuItem icon="folder-remove" onSelect={() => run({ op: "outOfFolder", args: { serverId: props.id } })}>
                      Remove from folder
                    </MenuItem>
                  </Show>
                </>
              }
            >
              {(f) => (
                <>
                  <MenuItem icon="preferences" onSelect={() => props.toSettings(f().folderId)}>
                    Folder settings…
                  </MenuItem>
                  <MenuRule />
                  <MenuItem icon="trash" danger onSelect={() => run({ op: "deleteFolder", args: { folder: f().folderId } }, `${label()} deleted; its servers are where it was`)}>
                    Delete folder
                  </MenuItem>
                </>
              )}
            </Show>
          </Menu>
        </div>
        <Show when={sub()}>
          <div data-submenu>
            <Menu label="Move to folder" class="lu-menu--rail">
              <For each={targets()}>
                {(f) => (
                  <MenuItem icon="folder" onSelect={() => run({ op: "intoFolder", args: { serverId: props.id, folder: f.folderId } })}>
                    {folderLabel(props.snapshot, f)}
                  </MenuItem>
                )}
              </For>
            </Menu>
          </div>
        </Show>
      </div>
    </Popover>
  );
}

export interface FolderSettingsPopProps {
  folderId: string;
  anchor: HTMLElement;
  snapshot: SnapshotSummary | undefined;
  close: () => void;
}

/** Folder settings over its tile: the name (its servers' words as the placeholder) and the tone, kept on Save. */
export function FolderSettingsPop(props: FolderSettingsPopProps) {
  const folder = rail().find((i): i is Folder => i.kind === "folder" && i.folderId === props.folderId);
  function rail() {
    return railOf(props.snapshot);
  }
  const [name, setName] = createSignal(folder?.name ?? "");
  const [tone, setTone] = createSignal<Tone>(toneOf(folder?.tone ?? ""));
  const words = () => (folder ? wordsFor(folder.serverIds.map((id) => nameOf(props.snapshot, id))) : "");
  const save = () => {
    props.close();
    if (!folder) return;
    const named = name().trim();
    void editRail({ op: "setFolder", args: { folder: props.folderId, name: name(), tone: tone() } }, props.folderId, `Folder ${named || words()} saved`);
  };
  return (
    <Popover anchor={props.anchor} placement="right" onClose={props.close}>
      <FolderSettings name={name()} words={words()} tone={tone()} onName={setName} onTone={setTone} onCancel={props.close} onSave={save} />
    </Popover>
  );
}
