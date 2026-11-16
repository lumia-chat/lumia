// The preview on the Preferences page: four sample messages in
// #design-review, the same on every client, and what each one draws
// under the display settings. The page draws them with the conversation's
// own rows (`routes/preferences.tsx`); this holds what needs no page.
import type { Display } from "./preferences";

export type PreviewTone = "mint" | "amber" | "indigo";

export interface PreviewMessage {
  id: string;
  /** Who sent it: the block a message joins is its author's, as in a conversation. */
  authorId: string;
  author: string;
  tone: PreviewTone;
  /** When it was sent, today, in this computer's time. */
  hour: number;
  minute: number;
  text: string;
  /** A link in the text: the words it is and where it goes. */
  link?: { words: string; url: string };
  /** A picture attached: drawn as a tile, or as its file row where Images is As links. */
  picture?: { name: string; bytes: number; alt: string };
  /** A link's card under it, where Embeds is shown. */
  card?: { url: string; host: string; title: string; description: string };
  reactions?: { emoji: string; count: number; mine: boolean }[];
}

const PRIYA = { authorId: "priya", author: "Priya Nair", tone: "mint" } as const;
const TOMAS = { authorId: "tomas", author: "Tomás Reyes", tone: "amber" } as const;
const MEI = { authorId: "mei", author: "Mei Chen", tone: "indigo" } as const;

export const PREVIEW_CHANNEL = "design-review";

export const PREVIEW_MESSAGES: PreviewMessage[] = [
  { id: "p1", ...PRIYA, hour: 10, minute: 42, text: "Pushed the new onboarding flow to staging. Could you look before standup?" },
  { id: "p2", ...PRIYA, hour: 10, minute: 42, text: "The invite step is the one I’m least sure about." },
  {
    id: "p3",
    ...TOMAS,
    hour: 10,
    minute: 44,
    text: "On it. Here’s the first screen on my phone:",
    picture: { name: "onboarding-first-screen.png", bytes: 412_000, alt: "The onboarding flow’s first screen" },
    reactions: [
      { emoji: "👀", count: 2, mine: false },
      { emoji: "🎉", count: 3, mine: true },
    ],
  },
  {
    id: "p4",
    ...MEI,
    hour: 10,
    minute: 51,
    text: "Release notes are up for review too: lumia.chat/blog/0-13",
    link: { words: "lumia.chat/blog/0-13", url: "https://lumia.chat/blog/0-13" },
    card: { url: "https://lumia.chat/blog/0-13", host: "lumia.chat", title: "What’s new in Lumia 0.13", description: "Video and audio play in the conversation, and links open as cards on every client." },
  },
];

/** What one sample message draws under the settings: its picture as a tile or a file row, its card or none, its reactions or none. */
export function previewDrawn(m: PreviewMessage, d: Pick<Display, "images" | "embeds" | "reactions">) {
  return {
    picture: !!m.picture && d.images === "shown",
    fileRow: !!m.picture && d.images === "link",
    card: !!m.card && d.embeds === "shown",
    reactions: d.reactions === "shown" ? (m.reactions ?? []) : [],
  };
}

/** When a sample message was sent, in microseconds, as a conversation's messages say it: today at its hour and minute. */
export function previewAt(m: PreviewMessage, today = new Date()): number {
  return new Date(today.getFullYear(), today.getMonth(), today.getDate(), m.hour, m.minute).getTime() * 1000;
}
