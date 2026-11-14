// The client worker: owns the Rust client (store on OPFS, gateway loop)
// and answers the page's calls. One message in is `{id, method, args}`,
// one out is `{id, ok, result}` or `{id, ok: false, error}`; gateway
// events arrive as `{event}`. Nothing here decides anything; the page's
// bridge types the calls.
import init, { Client } from "./pkg/lumia_wasm.js";
import { storeKey } from "./store-key";

type Call = { id: number; method: string; args: unknown[] };

let client: Client | null = null;
// What the open client was opened for; opening the same again answers
// from it rather than building a second client over the same store.
let openedFor: string | null = null;
let ready: Promise<void> | null = null;

function ensureInit(): Promise<void> {
  ready ??= init().then(() => undefined);
  return ready;
}

// A reload opens the stores while the page before it may still hold
// them: that page's worker goes with it, but the browser frees its OPFS
// access handles a moment later, sooner or later by how busy the worker
// was. Opening waits that moment out instead of failing, which the page
// would take for a signed-out browser.
async function openClient(baseUrl: string): Promise<Client> {
  const key = await storeKey();
  for (let attempt = 0; ; attempt++) {
    try {
      return await Client.open(baseUrl, key);
    } catch (e) {
      if (attempt >= 40 || !/Access Handle|NoModificationAllowedError/i.test(errorText(e))) throw e;
      await new Promise((resolve) => setTimeout(resolve, 250));
    }
  }
}

async function dispatch(method: string, args: unknown[]): Promise<unknown> {
  await ensureInit();
  if (method === "open") {
    const baseUrl = String(args[0]);
    if (!client || openedFor !== baseUrl) {
      client = await openClient(baseUrl);
      openedFor = baseUrl;
    }
    return { host: client.host(), deviceId: client.deviceId(), signedIn: client.signedIn() };
  }
  if (!client) throw new Error("open the client first");
  if (method === "start") {
    client.start(args[0] as string[], (event: unknown) => self.postMessage({ event }));
    return undefined;
  }
  const fn = (client as unknown as Record<string, unknown>)[method];
  if (typeof fn !== "function") throw new Error(`unknown method ${method}`);
  const result = await (fn as (...a: unknown[]) => unknown).apply(client, args);
  const messages = BODIES[method]?.(result);
  if (messages) for (const m of messages) m.blocks = blocksOf(client, m.body, m.mentions ?? []);
  return result;
}

// Every message handed to the page carries its body read into blocks by
// the core (protocol 14), so the page draws them and parses nothing: the
// calls that answer messages, and where in each answer they are.
type Drawn = { body: string; mentions?: { start: number; end: number }[]; blocks?: unknown };
const BODIES: Record<string, (result: unknown) => Drawn[] | undefined> = {
  messages: (r) => r as Drawn[],
  olderMessages: (r) => (r as { messages: Drawn[] }).messages,
  mentions: (r) => (r as { messages: Drawn[] }).messages,
  dmMessages: (r) => (r as { messages: Drawn[] }).messages,
};

// A conversation's messages are read again after every event in it, and
// most of them have not changed: each body is read once, kept by its text
// and its mentions' ranges, the least recently drawn let go first once the
// kept bodies come to READ_KEPT_BYTES.
const read = new Map<string, unknown>();
const READ_KEPT_BYTES = 4 * 1024 * 1024;
let readBytes = 0;

function blocksOf(c: Client, body: string, mentions: { start: number; end: number }[]): unknown {
  const key = mentions.map((m) => `${m.start}-${m.end}`).join(",") + "\u0000" + body;
  let blocks = read.get(key);
  if (blocks === undefined) {
    blocks = c.parseBody(body, mentions);
    readBytes += key.length;
    while (readBytes > READ_KEPT_BYTES && read.size > 0) {
      const oldest = read.keys().next().value as string;
      read.delete(oldest);
      readBytes -= oldest.length;
    }
  } else {
    // Drawn again: to the back of the line.
    read.delete(key);
  }
  read.set(key, blocks);
  return blocks;
}

self.onmessage = async (e: MessageEvent<Call>) => {
  const { id, method, args } = e.data;
  try {
    const result = await dispatch(method, args);
    self.postMessage({ id, ok: true, result });
  } catch (err) {
    self.postMessage({ id, ok: false, error: errorText(err) });
  }
};

function errorText(err: unknown): string {
  if (err instanceof Error) return err.message;
  return String(err);
}
