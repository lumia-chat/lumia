// Why the sign-in card was come back to, said there once: the instance no
// longer took the session, or another device removed this browser from
// the identity (identity spec 4.4), after which the core has already
// erased what it kept and the browser signs in again as a new device.

export interface Ended {
  words: string;
  /** Removed by another device: said as a notice to dismiss rather than as an error. */
  removed: boolean;
}

/** The sign-in card's words for a session that ended, `removed` when the gateway loop said this device was removed. */
export function endedNotice(removed: boolean): Ended {
  return removed
    ? { words: "This browser was removed from your account, and its private messages were erased from it.", removed: true }
    : { words: "You were signed out of this browser. Sign in again to carry on.", removed: false };
}
