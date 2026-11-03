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

async function dispatch(method: string, args: unknown[]): Promise<unknown> {
  await ensureInit();
  if (method === "open") {
    const baseUrl = String(args[0]);
    const shell = args[1] === true;
    const key = `${shell ? "shell" : "web"} ${baseUrl}`;
    if (!client || openedFor !== key) {
      client = await Client.open(baseUrl, await storeKey(), shell);
      openedFor = key;
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
  return await (fn as (...a: unknown[]) => unknown).apply(client, args);
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
