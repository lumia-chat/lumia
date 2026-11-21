// Calls that must reach the instance in the order they were made. Two requests sent at once are
// taken by the instance in whichever order they arrive, so a quick second change (a role moved by
// its menu and then dragged) could be applied first and undone by the one before it.

/** Runs each call given to it after the one before has finished, in the order given. */
export function oneAtATime(): <T>(call: () => Promise<T>) => Promise<T> {
  let last: Promise<unknown> = Promise.resolve();
  return <T>(call: () => Promise<T>) => {
    const next = last.then(call, call);
    last = next.catch(() => undefined);
    return next;
  };
}
