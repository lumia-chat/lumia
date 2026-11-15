// Linking this browser by a prompt (identity spec 4.2.1): the rules the
// link page follows that need no page. The core takes each step; the page
// asks it where the session stands when it opens, whenever it moves
// (`linkSessionUpdated`) and every two seconds while it waits, and says so in
// these words. A browser is only ever the new device: it never approves.
import type { LinkStage } from "./bridge";

/** How often the page asks while the session is open. */
export const LINK_POLL_MS = 2000;

/** What the page shows: a stage of the core's, or starting one, or a failure to start or go on. */
export type LinkScreen = LinkStage | "starting" | "failed";

/** Whether the session is open and waiting on the other device: asked again on the timer and whenever it moves. */
export function linkPolls(screen: LinkScreen): boolean {
  return screen === "waiting" || screen === "compare";
}

/** Whether leaving the page should give the session up, so the other devices stop asking. */
export function linkOpen(screen: LinkScreen): boolean {
  return screen === "starting" || linkPolls(screen);
}

/** Whether the session is over without a link: the page offers Try again. */
export function linkEnded(screen: LinkScreen): boolean {
  return screen === "declined" || screen === "expired" || screen === "failed";
}

/**
 * Whether a failure to ask is passing: the instance out of reach or busy,
 * so the page says so and asks again on the next tick. Anything else (a
 * refusal, a session the core no longer holds) ends the session here.
 */
export function linkFailurePasses(code: string, message: string): boolean {
  if (code) return code === "UNAVAILABLE" || code === "RATE_LIMITED";
  return message.startsWith("transport");
}

/**
 * The card's words at each stage. `failed` takes the reason as it was
 * said; a session the core no longer holds (the worker started again)
 * reads as one that ran out.
 */
export function linkWords(screen: LinkScreen, reason = ""): { title: string; text: string } {
  switch (screen) {
    case "starting":
    case "waiting":
    case "linked":
      return { title: "Link this browser", text: "Open Lumia on a device that has your private messages. It will ask about this browser." };
    case "compare":
      return { title: "Compare the number", text: "Check that your other device shows the same number, then approve it there." };
    case "declined":
      return { title: "This browser isn’t linked", text: "It wasn’t approved on your other device." };
    case "expired":
      return { title: "This browser isn’t linked", text: "That took too long." };
    case "failed":
      return { title: "This browser isn’t linked", text: /no link in progress/.test(reason) ? "That took too long." : reason || "Something went wrong." };
  }
}
