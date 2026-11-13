// The emoji the picker offers (architecture 6.6): Unicode's list as the
// emojibase-data package carries it, up to Unicode 16 and slimmed by a
// plugin in vite.config.ts, loaded the first time a picker opens (its own
// chunk), in Unicode's order and groups, default skin tone only.
// What this device chose most is kept here too, for "Frequently used".

/** The quick row over a message, and where "Frequently used" starts. */
export const QUICK_REACTIONS = ["👍", "❤️", "😂", "🎉", "😮", "😢"];

export interface EmojiEntry {
  emoji: string;
  /** Unicode's name, lower case: "thumbs up". */
  name: string;
  /** The words a search also finds it by. */
  tags: string[];
}

export interface EmojiGroup {
  name: string;
  emoji: EmojiEntry[];
}

// Unicode's groups, in its order, by emojibase's numbers; 2 (the skin
// tone and hair components) is not offered.
const GROUPS: [number, string][] = [
  [0, "Smileys & emotion"],
  [1, "People & body"],
  [3, "Animals & nature"],
  [4, "Food & drink"],
  [5, "Travel & places"],
  [6, "Activities"],
  [7, "Objects"],
  [8, "Symbols"],
  [9, "Flags"],
];

interface Compact {
  group?: number;
  order?: number;
  label: string;
  tags?: string[];
  unicode: string;
}

/**
 * The fully-qualified form, as Unicode's emoji-test lists it and as every
 * client sends a reaction: emojibase writes a presentation selector after
 * any emoji that has a text form, even one drawn as an emoji by default
 * (thumbs up), and that selector is dropped after a code point that
 * already presents as an emoji, or one reaction would be two.
 */
export function fullyQualified(text: string): string {
  return text.replace(/(\p{Emoji_Presentation})\uFE0F/gu, "$1");
}

let loading: Promise<EmojiGroup[]> | null = null;

/** Every emoji by group, loaded once. */
export function loadEmoji(): Promise<EmojiGroup[]> {
  loading ??= import("emojibase-data/en/data.json").then((m) => {
    const all = (m.default as unknown as Compact[]).filter((e) => e.group !== undefined && e.group !== 2).sort((a, b) => (a.order ?? 0) - (b.order ?? 0));
    return GROUPS.map(([id, name]) => ({
      name,
      emoji: all.filter((e) => e.group === id).map((e) => ({ emoji: fullyQualified(e.unicode), name: e.label.toLowerCase(), tags: (e.tags ?? []).map((t) => t.toLowerCase()) })),
    }));
  });
  return loading;
}

/** Whether a search finds an emoji: a word of its name or one of its keywords starts with what was typed. */
export function emojiMatches(e: EmojiEntry, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return e.name.includes(q) || e.tags.some((t) => t.startsWith(q));
}

const FREQUENT_KEY = "lumia:emoji-frequent";
const FREQUENT_MAX = 16;

/** This device's most used emoji, most first; the quick six until it has used any. */
export function frequentEmoji(): string[] {
  let used: Record<string, number> = {};
  try {
    used = JSON.parse(localStorage.getItem(FREQUENT_KEY) ?? "{}") as Record<string, number>;
  } catch {
    used = {};
  }
  const ranked = Object.entries(used)
    .filter(([, n]) => typeof n === "number" && n > 0)
    .sort((a, b) => b[1] - a[1])
    .map(([e]) => e);
  const list = [...ranked];
  for (const e of QUICK_REACTIONS) if (!list.includes(e)) list.push(e);
  return list.slice(0, FREQUENT_MAX);
}

/** Counts one use of an emoji on this device. */
export function usedEmoji(emoji: string): void {
  try {
    const used = JSON.parse(localStorage.getItem(FREQUENT_KEY) ?? "{}") as Record<string, number>;
    used[emoji] = (typeof used[emoji] === "number" ? used[emoji] : 0) + 1;
    localStorage.setItem(FREQUENT_KEY, JSON.stringify(used));
  } catch {
    /* a browser that keeps nothing keeps no frequency either */
  }
}
