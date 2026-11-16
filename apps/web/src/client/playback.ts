// One thing playing at a time (specs/media.md 5 and 6.6): an attached
// video or audio, or a site's embed in its card's place. Rules only; the
// page registers what starts and says how to stop it.

/** What is playing or shown: an attached video or audio, or an embed (a player, audio, or a post). */
export interface Playing {
  id: string;
  /** A site's embed in its card's place, rather than an attachment. */
  embed: boolean;
  kind: "video" | "audio" | "post";
  /** Pauses it, or puts an embed back to its card. */
  stop: () => void;
}

/**
 * Whether `starting` stops `other`: a card pressed puts back every other
 * embed (one is shown at a time) and pauses attached video and audio; an
 * attachment started pauses every other attachment and puts back an embed
 * that plays (a player or audio), leaving a post being read where it is.
 */
export function stops(starting: Playing, other: Playing): boolean {
  if (starting.id === other.id) return false;
  if (starting.embed) return true;
  return !other.embed || other.kind !== "post";
}

/** What is playing now, and the rule applied as each starts. */
export class OneAtATime {
  private active = new Map<string, Playing>();

  /** `p` starts: what it stops is stopped and let go, and their ids answered. */
  start(p: Playing): string[] {
    const stopped: string[] = [];
    for (const other of [...this.active.values()]) {
      if (!stops(p, other)) continue;
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
