// One thing playing at a time (specs/media.md 5 and 6.6): an attached
// video or audio, or a site's embed in its card's place. Rules only; the
// page registers what starts and says how to stop it. The two rules are
// the core's (`lumia_core::playback`), written here as well because a
// press needs its answer at once and the bridge answers later; both are
// tested against the cases every client agrees on
// (`proto/vectors/playback/cases.json`).

/** What the rules read of something playing or shown: an attached video or audio, or an embed (a player, which is a video; audio; or a post). */
export interface PlayingKind {
  /** A site's embed in its card's place, rather than an attachment. */
  embed: boolean;
  kind: "video" | "audio" | "post";
}

/** What is playing or shown, and how the page stops it. */
export interface Playing extends PlayingKind {
  id: string;
  /** Pauses it, or puts an embed back to its card. */
  stop: () => void;
}

/**
 * Whether `starting` stops `other`, something else already playing or
 * shown: a card pressed puts back every other embed (one is shown at a
 * time) and pauses attached video and audio; an attachment started pauses
 * every other attachment and puts back an embed that plays (a player or
 * audio), leaving a post being read where it is.
 */
export function stops(starting: PlayingKind, other: PlayingKind): boolean {
  return starting.embed || !other.embed || other.kind !== "post";
}

/**
 * Whether `p` stops when its row scrolls away: a video does (an attachment
 * pauses, an embed's player goes back to its card); audio plays on until
 * another starts or the conversation is left, and a post stays shown. The
 * page watches for scrolling away only what this says stops.
 */
export function stopsOffScreen(p: PlayingKind): boolean {
  return p.kind === "video";
}

/** What is playing now, and the rule applied as each starts. */
export class OneAtATime {
  private active = new Map<string, Playing>();

  /** `p` starts: what it stops is stopped and let go, and their ids answered. Starting again what is playing stops nothing. */
  start(p: Playing): string[] {
    const stopped: string[] = [];
    for (const other of [...this.active.values()]) {
      if (other.id === p.id || !stops(p, other)) continue;
      this.active.delete(other.id);
      stopped.push(other.id);
      other.stop();
    }
    this.active.set(p.id, p);
    return stopped;
  }

  /** It paused, ended, went back to its card, or its row went. */
  end(id: string) {
    this.active.delete(id);
  }

  has(id: string): boolean {
    return this.active.has(id);
  }

  ids(): string[] {
    return [...this.active.keys()];
  }
}

/** The page's one. */
export const playback = new OneAtATime();
