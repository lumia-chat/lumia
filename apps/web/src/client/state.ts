// The page's client state: one bridge for the instance the page is
// signed in to, the account snapshot and a channel's messages as queries
// that the gateway's events invalidate. Screens read these and call the
// bridge; nothing here renders.
import { QueryClient, createQuery, useQueryClient } from "@tanstack/solid-query";
import { createSignal } from "solid-js";
import { Bridge, claimOwnership, type ClientEvent, type Opened } from "./bridge";

export const queryClient = new QueryClient({
  defaultOptions: { queries: { staleTime: Infinity, retry: false } },
});

const INSTANCE_KEY = "lumia:instance";

export function savedInstance(): string | null {
  try {
    return localStorage.getItem(INSTANCE_KEY);
  } catch {
    return null;
  }
}

export function rememberInstance(url: string) {
  try {
    localStorage.setItem(INSTANCE_KEY, url);
  } catch {
    /* ignore */
  }
}

let bridge: Bridge | null = null;
const [opened, setOpened] = createSignal<Opened | null>(null);
const [owner, setOwner] = createSignal<boolean | null>(null);
const [started, setStarted] = createSignal(false);
const [connection, setConnection] = createSignal<string>("idle");

export { opened, owner, started, connection };

/** The bridge, created on first use. */
export function client(): Bridge {
  bridge ??= new Bridge();
  return bridge;
}

/** Elects the owning tab once; false means another tab has the store. */
export async function ensureOwner(): Promise<boolean> {
  if (owner() !== null) return owner() as boolean;
  const ok = await claimOwnership();
  setOwner(ok);
  return ok;
}

/** Opens the client for `baseUrl` and remembers it. */
export async function openInstance(baseUrl: string): Promise<Opened> {
  const result = await client().open(baseUrl);
  rememberInstance(baseUrl);
  setOpened(result);
  return result;
}

/** Starts the gateway loop once and routes its events into the queries. */
export async function startGateway(channelIds: string[]): Promise<void> {
  if (started()) return;
  const b = client();
  b.onEvent((e: ClientEvent) => {
    switch (e.kind) {
      case "welcomed":
        setConnection("connected");
        void queryClient.invalidateQueries({ queryKey: ["snapshot"] });
        break;
      case "applied":
        void queryClient.invalidateQueries({ queryKey: ["messages", e.channelId] });
        break;
      case "accountApplied":
        void queryClient.invalidateQueries({ queryKey: ["snapshot"] });
        break;
      case "snapshotTaken":
        void queryClient.invalidateQueries({ queryKey: ["messages", e.channelId] });
        break;
      case "reconnecting":
        setConnection(`reconnecting (attempt ${e.attempt})`);
        break;
      case "disconnected":
        setConnection(`disconnected: ${e.reason}`);
        break;
      case "closed":
        setConnection(`closed: ${e.reason}`);
        setStarted(false);
        break;
      default:
        break;
    }
  });
  await b.start(channelIds);
  setStarted(true);
  setConnection("connecting");
}

export function snapshotQuery() {
  return createQuery(() => ({
    queryKey: ["snapshot"],
    queryFn: () => client().snapshot(),
  }));
}

export function messagesQuery(channelId: () => string | undefined) {
  return createQuery(() => ({
    queryKey: ["messages", channelId()],
    queryFn: () => client().messages(channelId() as string, 200),
    enabled: !!channelId(),
  }));
}

export { useQueryClient };
