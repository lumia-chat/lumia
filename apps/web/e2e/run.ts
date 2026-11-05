// The browser suite, against a dev instance at http://localhost:4000 (run
// `mix phx.server` in apps/instance first). The CLI is the other device:
// it seeds an account, a server and a channel, and approves this
// browser's link request. Runs in Chromium and WebKit with persistent
// contexts (WebKit has no OPFS in an ephemeral one).
//
//   bun run e2e              # both browsers
//   bun run e2e -- webkit    # one
import { chromium, webkit, type BrowserContext, type Page } from "playwright";
import { existsSync, mkdirSync, rmSync } from "node:fs";
import { resolve } from "node:path";

const web = resolve(import.meta.dir, "..");
const cli = resolve(web, "../core/target/release/lumia-cli");
const work = resolve(import.meta.dir, ".work");
const out = resolve(work, "shots");
// LUMIA_BASE points the suite at a UI an instance serves (`bun run
// build:instance`, then its server); otherwise vite is started here.
const base = process.env.LUMIA_BASE ?? "http://localhost:5173";
const instance = process.env.LUMIA_INSTANCE ?? "http://localhost:4000";
const password = "correct-horse-battery-staple";
const browsers = process.argv.slice(2).filter((a) => a === "chromium" || a === "webkit");
const engines = browsers.length ? browsers : ["chromium", "webkit"];

function run(args: string[], cwd = web): string {
  // A CLI call that waits on the gateway (dm open, dm send) must not hang the suite.
  const r = Bun.spawnSync(args, { cwd, timeout: 120_000 });
  const text = r.stdout.toString() + r.stderr.toString();
  if (r.exitCode !== 0) throw new Error(`${args.join(" ")} (${r.signalCode ? "killed by " + r.signalCode : "exit " + r.exitCode})\n${text}`);
  return text;
}

function lumia(dataDir: string, ...args: string[]): string {
  return run([cli, "--data-dir", dataDir, ...args]);
}

async function waitFor(url: string, what: string) {
  for (let i = 0; i < 60; i++) {
    try {
      if ((await fetch(url, { headers: { accept: "*/*" } })).ok) return;
    } catch {
      /* not yet */
    }
    await Bun.sleep(1000);
  }
  throw new Error(`${what} did not answer at ${url}`);
}

// The page and the phase of the step in progress, for the heartbeat and
// for a screenshot when it fails.
const live: { page: Page | null; phase: string } = { page: null, phase: "" };
const phase = (what: string) => {
  live.phase = what;
};

async function open(which: string, name: string, keep = false): Promise<{ ctx: BrowserContext; page: Page }> {
  const profile = resolve(work, `profile-${name}-${which}`);
  if (!keep) rmSync(profile, { recursive: true, force: true });
  const engine = which === "webkit" ? webkit : chromium;
  // Bounded as a whole: a browser that launches but never serves a page
  // would otherwise hang the suite with nothing printed.
  const launched = engine.launchPersistentContext(profile, { viewport: { width: 1440, height: 960 }, timeout: 60_000 }).then(async (ctx) => ({ ctx, page: await ctx.newPage() }));
  const { ctx, page } = await Promise.race([launched, Bun.sleep(90_000).then(() => Promise.reject(new Error(`${which} did not open a page within 90 s`)))]);
  console.log(`  [${which}] browser ${ctx.browser()?.version() ?? "?"} up`);
  live.page = page;
  // WebKit's first navigation of a run on Linux CI takes well over the default 30 s.
  page.setDefaultNavigationTimeout(90_000);
  page.on("pageerror", (e) => console.log(`  [${which}] page error: ${e.message}`));
  page.on("console", (m) => {
    if (m.type() === "error") console.log(`  [${which}] console: ${m.text()}`);
  });
  if (!keep) await clearStores(which, page, base);
  return { ctx, page };
}

// WebKit keeps OPFS outside the profile directory, so a fresh profile
// still sees the last run's stores for `origin`, under a wrapping key the
// fresh profile no longer has; clear them before the worker opens any.
// Best effort and bounded: an engine without OPFS iteration must not hang
// the suite here, it fails later where the store opens.
async function clearStores(which: string, page: Page, origin: string) {
  await goto(page, `${origin}/design`);
  const cleared = page.evaluate(async () => {
    const root = await navigator.storage.getDirectory();
    for await (const name of (root as unknown as { keys(): AsyncIterable<string> }).keys()) await root.removeEntry(name, { recursive: true });
    localStorage.clear();
    return true;
  });
  const outcome = await Promise.race([cleared.catch((e: unknown) => `failed: ${e instanceof Error ? e.message : String(e)}`), Bun.sleep(15_000).then(() => "timed out")]);
  if (outcome !== true) console.log(`  [${which}] clearing storage at ${origin} ${outcome}`);
}

// WebKit on Linux CI sometimes never finishes a profile's first navigation;
// a second attempt does.
async function goto(page: Page, url: string) {
  try {
    await page.goto(url, { timeout: 45_000 });
  } catch (e) {
    console.log(`  retrying ${url}: ${e instanceof Error ? e.message.split("\n")[0] : String(e)}`);
    await page.goto(url, { timeout: 90_000 });
  }
}

async function trustCard(page: Page) {
  await goto(page, base);
  await page.locator(".lu-card").waitFor();
  // A UI served by its instance has no address to ask for.
  const field = page.getByLabel("Instance");
  if ((await field.count()) > 0) {
    await field.fill(instance);
    await page.getByRole("button", { name: "Continue" }).click();
  }
  await page.getByText("run by").waitFor();
}

// The UI the instance serves (`bun run build:instance`, installed under
// the instance's priv/static/ui): its sign-in page has no address field
// and reaches its own origin. In Chromium, where Playwright can intercept
// the worker's requests, the instance is unreachable at first load, the
// page offers "Try again", and that reaches it once it can; WebKit's
// routing never sees a worker's request, so there the page is only
// checked as served. Skipped when no build is installed.
async function instanceServed(which: string) {
  const probe = await fetch(`${instance}/sign-in`);
  if (!(probe.headers.get("content-type") ?? "").includes("text/html")) {
    console.log(`  [${which}] the instance serves no web build; the served sign-in is not checked`);
    return;
  }
  const { ctx, page } = await open(which, "instance");
  const t0 = Date.now();
  try {
    // The instance origin has its own stores (and, in WebKit, last run's).
    await clearStores(which, page, instance);
    let unreachable = intercepts(which);
    if (intercepts(which)) await ctx.route("**/api/v1/instance", (route) => (unreachable ? route.abort("connectionrefused") : route.continue()));
    await goto(page, `${instance}/sign-in`);
    if (intercepts(which)) {
      const again = page.getByRole("button", { name: "Try again" });
      await again.waitFor();
      if ((await page.getByLabel("Instance").count()) !== 0) throw new Error("the served sign-in asks for an address");
      await page.screenshot({ path: `${out}/${which}-served-unreachable.png` });
      unreachable = false;
      await again.click();
    }
    await page.getByText("run by").waitFor();
    if ((await page.getByLabel("Instance").count()) !== 0) throw new Error("the served sign-in asks for an address");
    const alerts = await page.getByRole("alert").allTextContents();
    if (alerts.length) throw new Error(`an error stayed on screen: ${alerts.join(" | ")}`);
    await page.screenshot({ path: `${out}/${which}-served.png` });
  } finally {
    await ctx.close();
  }
  console.log(`  ${which}: the served sign-in ${intercepts(which) ? "recovers with Try again" : "reaches its instance"} in ${Date.now() - t0} ms`);
}

function intercepts(which: string): boolean {
  return which === "chromium";
}

// Sign in by password, send a message, reload, read it back.
async function signInAndSend(which: string, handle: string) {
  const { ctx, page } = await open(which, "sign-in");
  const t0 = Date.now();
  try {
    await signInSteps(which, page, handle);
  } finally {
    await ctx.close();
  }
  console.log(`  ${which}: signed in, sent, reloaded, stores encrypted at rest in ${Date.now() - t0} ms`);
}

async function signInSteps(which: string, page: Page, handle: string) {
  await trustCard(page);
  await page.screenshot({ path: `${out}/${which}-sign-in.png` });
  await page.getByLabel("Email or handle").fill(handle);
  await page.getByRole("textbox", { name: "Password" }).fill(password);
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  const composer = page.getByPlaceholder(/^Message #/);
  await composer.waitFor();
  const body = `hello from ${which} ${Date.now()}`;
  await composer.fill(body);
  await page.keyboard.press("Enter");
  await page.getByText(body).waitFor();
  await page.reload();
  await page.getByText(body).waitFor();
  await page.screenshot({ path: `${out}/${which}-channel.png` });
  // Turning notifications on registers the service worker (Chromium:
  // Playwright's WebKit has no service workers; the subscription itself
  // needs a push service, which a headless browser has no account with).
  if (which === "chromium") {
    await page.context().grantPermissions(["notifications"], { origin: base });
    await page.getByRole("button", { name: "Turn on notifications" }).click();
    const sw = await page.evaluate(() =>
      Promise.race([
        navigator.serviceWorker.ready.then((r) => r.active?.scriptURL ?? ""),
        new Promise<string>((resolve) => setTimeout(() => resolve(""), 15_000)),
      ]),
    );
    if (!sw.endsWith("/sw.js")) throw new Error(`no service worker (${sw || "none"})`);
  }
  // At rest (e2ee spec 8): no file in OPFS starts a page with SQLite's
  // plaintext header, so the stores are ciphertext without the key.
  const plain = await page.evaluate(async () => {
    const marker = "SQLite format 3";
    const found: string[] = [];
    async function walk(dir: FileSystemDirectoryHandle, path: string) {
      for await (const [name, entry] of (dir as unknown as { entries(): AsyncIterable<[string, FileSystemHandle]> }).entries()) {
        if (entry.kind === "directory") await walk(entry as FileSystemDirectoryHandle, `${path}/${name}`);
        else {
          const file = await (entry as FileSystemFileHandle).getFile();
          const text = new TextDecoder("latin1").decode(await file.slice(0, 65536).arrayBuffer());
          if (file.size > 0 && text.includes(marker)) found.push(`${path}/${name}`);
        }
      }
    }
    await walk(await navigator.storage.getDirectory(), "");
    return found;
  });
  if (plain.length) throw new Error(`plaintext SQLite header in ${plain.join(", ")}`);
}

// Link this browser as a web device: the CLI approves, the browser claims
// and signs in by identity proof.
async function link(which: string, dataDir: string) {
  const { ctx, page } = await open(which, "link");
  const t0 = Date.now();
  try {
    await linkSteps(which, page, dataDir);
  } finally {
    await ctx.close();
  }
  console.log(`  ${which}: linked and signed in by identity proof in ${Date.now() - t0} ms`);
}

async function linkSteps(which: string, page: Page, dataDir: string) {
  await trustCard(page);
  await page.getByRole("button", { name: "Link this browser to your identity" }).click();
  await page.getByRole("button", { name: "Start linking" }).click();
  const request = await page.getByLabel("Link request").inputValue();
  await page.screenshot({ path: `${out}/${which}-link.png` });
  const code = lumia(dataDir, "identity", "approve", request).match(/code (\S+)/)?.[1];
  if (!code) throw new Error("the CLI printed no claim code");
  await page.getByLabel("Code from the approving device").fill(code);
  await page.getByRole("button", { name: "Claim and sign in" }).click();
  await page.getByPlaceholder(/^Message #/).waitFor();
  await page.reload();
  await page.getByPlaceholder(/^Message #/).waitFor();
  const roster = lumia(dataDir, "identity", "show");
  if (!/Web/.test(roster)) throw new Error(`no web device in the roster:\n${roster}`);
}

// A personal DM between the linked browser (B) and the CLI (A): A asks, B
// accepts in the browser and creates the group, both send, B reloads.
async function dm(which: string, aDir: string, bAccount: string, aHandle: string) {
  const { ctx, page } = await open(which, "link", true);
  const t0 = Date.now();
  try {
    await dmSteps(which, page, aDir, bAccount, aHandle);
  } catch (e) {
    // Never let the screenshot's own failure replace the step's.
    try {
      const current = live.page ?? page;
      await current.screenshot({ path: `${out}/${which}-dm-failed.png`, timeout: 10_000 });
      const alerts = await current.getByRole("alert").allTextContents();
      if (alerts.length) console.log(`  [${which}] on screen: ${alerts.join(" | ")}`);
    } catch {
      /* the page may be gone */
    }
    throw e;
  } finally {
    await ctx.close();
  }
  console.log(`  ${which}: personal DM both ways and back after reload in ${Date.now() - t0} ms`);
}

async function dmSteps(which: string, page: Page, aDir: string, bAccount: string, aHandle: string) {
  phase("cli request");
  lumia(aDir, "dm", "request", bAccount);
  phase("open requests");
  await goto(page, `${base}/app?place=home&view=requests`);
  const accept = page.getByRole("button", { name: "Accept" });
  await accept.waitFor();
  // The sender's handle comes from the public profile, visible while the request waits.
  await page.getByText(`@${aHandle}`).waitFor();
  await page.waitForTimeout(1500); // the gateway loop starts after the snapshot
  phase("accept");
  await accept.click();
  phase("wait for the group");
  const composer = page.getByPlaceholder(/^Message /);
  await composer.waitFor();
  await page.screenshot({ path: `${out}/${which}-dm-open.png` });
  const hello = `hello from ${which} ${Date.now()}`;
  phase("send");
  await composer.fill(hello);
  await page.keyboard.press("Enter");
  await page.getByText(hello).waitFor();
  phase("cli open");
  const opened = lumia(aDir, "dm", "open", bAccount);
  const channel = opened.match(/([0-9a-f-]{36})\s+personal DM/)?.[1];
  if (!channel) throw new Error(`no channel from dm open:\n${opened}`);
  // The CLI joined from the welcome; the browser's message lands in its
  // store once its tail has applied the channel log, which takes a moment.
  // `dm messages` reads the CLI's store without syncing, and `dm open`
  // returns as soon as the group is ready, which can be before the
  // browser's message was applied: follow the conversation for a moment,
  // then read, until the message is there.
  phase("cli read");
  let seen = "";
  for (let i = 0; i < 10 && !seen.includes(hello); i++) {
    Bun.spawnSync([cli, "--data-dir", aDir, "dm", "tail", channel], { cwd: web, timeout: 4_000 });
    seen = lumia(aDir, "dm", "messages", channel);
  }
  if (!seen.includes(hello)) throw new Error(`the CLI did not read the browser's message:\n${seen}`);
  const reply = `reply from the cli ${Date.now()}`;
  phase("cli send");
  lumia(aDir, "dm", "send", channel, reply);
  phase("wait for the reply");
  await page.getByText(reply).waitFor();
  // Back from disk: a fresh page in the same profile. Not on the Linux
  // runner: tearing this page down there wedges the whole browser (the
  // plan records it); the sign-in step covers the store coming back from
  // disk on Linux, and this check runs on macOS.
  if (process.platform === "linux") {
    console.log(`  [${which}] the conversation's return from disk is checked on macOS, not here`);
    return;
  }
  phase("reopen");
  const url = page.url();
  const again = await page.context().newPage();
  live.page = again;
  await page.close({ runBeforeUnload: false });
  await goto(again, url);
  await again.getByText(hello).waitFor();
  await again.getByText(reply).waitFor();
  if ((await again.getByText(aHandle).count()) === 0) throw new Error("the CLI's handle is not shown as the sender");
  await again.screenshot({ path: `${out}/${which}-dm.png` });
}

// An invite link (architecture 6.1): the CLI mints one for its server, a
// fresh browser opens it, sees the server and the instance before anything
// is sent, creates an account from it and lands in the server.
async function invite(which: string, dataDir: string, server: string) {
  const { ctx, page } = await open(which, "invite");
  const t0 = Date.now();
  try {
    const code = lumia(dataDir, "invite-create", server).trim().split(/\s+/)[0];
    if (!code) throw new Error("no invite code");
    phase("preview");
    await goto(page, `${base}/invite/${code}?instance=${encodeURIComponent(instance)}`);
    await page.getByText("Web suite").waitFor();
    await page.getByText("run by").waitFor();
    await page.screenshot({ path: `${out}/${which}-invite.png` });
    phase("create");
    await page.getByRole("button", { name: "Create an account" }).click();
    await page.getByText("run by").waitFor();
    await page.getByRole("button", { name: "Create an account" }).click();
    await page.getByLabel("Handle").fill(`joined${which}${Date.now()}`);
    await page.getByRole("textbox", { name: "Password" }).fill(password);
    await page.getByRole("button", { name: "Create account", exact: true }).click();
    phase("joined");
    await page.getByPlaceholder(/^Message #general/).waitFor();
    if ((await page.getByText("Web suite").count()) === 0) throw new Error("the joined server is not shown");
    await page.screenshot({ path: `${out}/${which}-joined.png` });
  } finally {
    await ctx.close();
  }
  console.log(`  ${which}: opened an invite, created an account from it and landed in the server in ${Date.now() - t0} ms`);
}

// A server and a channel from the app: the rail's "+" makes a server the
// account owns, the server sidebar's "New channel" its first channel.
async function create(which: string, stamp: number) {
  const { ctx, page } = await open(which, "sign-in", true);
  const t0 = Date.now();
  try {
    await goto(page, `${base}/app`);
    await page.getByPlaceholder(/^Message #/).waitFor();
    phase("server");
    await page.getByRole("button", { name: "Create, join or discover servers" }).click();
    await page.getByLabel("Name").fill(`Made ${stamp}`);
    await page.getByRole("button", { name: "Create", exact: true }).click();
    await page.getByText("No channel yet").waitFor();
    if ((await page.getByText(`Made ${stamp}`).count()) === 0) throw new Error("the new server is not shown");
    phase("channel");
    await page.getByRole("button", { name: "New channel" }).click();
    await page.getByLabel("Channel name").fill("ops");
    await page.getByRole("button", { name: "Create channel" }).click();
    await page.getByPlaceholder("Message #ops").waitFor();
    await page.screenshot({ path: `${out}/${which}-created.png` });
  } finally {
    await ctx.close();
  }
  console.log(`  ${which}: made a server and its first channel from the app in ${Date.now() - t0} ms`);
}

// Web Push end to end (architecture 15.5), with a push service of the
// suite's own: a headless browser has no account with a real one, so the
// subscription's keys are made here and registered with the instance
// through the page's own client, with this server as the endpoint. The
// browser then goes away, a member mentions its account, and the instance
// posts the notice here, encrypted for those keys (RFC 8291) and signed
// with its VAPID key (RFC 8292); the test decrypts it. The service worker's
// own `push` event cannot be raised from outside, so the content it would
// show is what is checked.
const pushPort = 4919;
type Bytes = Uint8Array<ArrayBuffer>;
const pushes = new Map<string, { resolve: (r: { headers: Headers; body: Bytes }) => void }>();
const pushService = Bun.serve({
  port: pushPort,
  hostname: "127.0.0.1",
  async fetch(req) {
    const id = new URL(req.url).pathname.split("/").pop() ?? "";
    const waiter = pushes.get(id);
    if (!waiter) return new Response("unknown subscription", { status: 404 });
    waiter.resolve({ headers: req.headers, body: new Uint8Array(await req.arrayBuffer()) });
    return new Response(null, { status: 201 });
  },
});

function base64url(bytes: ArrayBuffer | Uint8Array): string {
  return Buffer.from(bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes)).toString("base64url");
}

async function hkdf(salt: Bytes, ikm: Bytes, info: Bytes, length: number): Promise<Bytes> {
  const key = await crypto.subtle.importKey("raw", ikm, "HKDF", false, ["deriveBits"]);
  return new Uint8Array(await crypto.subtle.deriveBits({ name: "HKDF", hash: "SHA-256", salt, info }, key, length * 8));
}

// RFC 8291 for one record: salt(16) | rs(4) | idlen(1) | as_public | ciphertext+tag.
async function decryptPush(body: Bytes, uaPrivate: CryptoKey, uaPublic: Bytes, auth: Bytes): Promise<string> {
  const salt = body.slice(0, 16);
  const idlen = body[20] ?? 0;
  const asPublic = body.slice(21, 21 + idlen);
  const ciphertext = body.slice(21 + idlen);
  const asKey = await crypto.subtle.importKey("raw", asPublic, { name: "ECDH", namedCurve: "P-256" }, false, []);
  const shared = new Uint8Array(await crypto.subtle.deriveBits({ name: "ECDH", public: asKey }, uaPrivate, 256));
  const text = new TextEncoder();
  const ikm = await hkdf(auth, shared, new Uint8Array([...text.encode("WebPush: info\0"), ...uaPublic, ...asPublic]), 32);
  const cek = await hkdf(salt, ikm, text.encode("Content-Encoding: aes128gcm\0"), 16);
  const nonce = await hkdf(salt, ikm, text.encode("Content-Encoding: nonce\0"), 12);
  const aes = await crypto.subtle.importKey("raw", cek, "AES-GCM", false, ["decrypt"]);
  const padded = new Uint8Array(await crypto.subtle.decrypt({ name: "AES-GCM", iv: nonce }, aes, ciphertext));
  let end = padded.length;
  while (end > 0 && padded[end - 1] === 0) end--;
  if (padded[end - 1] !== 2) throw new Error("the record's padding delimiter is missing");
  return new TextDecoder().decode(padded.slice(0, end - 1));
}

async function webPush(which: string, handle: string, inviterDir: string, server: string, channel: string, account: string) {
  if (which !== "chromium") {
    console.log(`  ${which}: Playwright's build has no service workers; push is checked in Chromium`);
    return;
  }
  const t0 = Date.now();
  // This browser's subscription: its own keys, this suite's service as the endpoint.
  const uaKeys = await crypto.subtle.generateKey({ name: "ECDH", namedCurve: "P-256" }, true, ["deriveBits"]);
  const uaPublic = new Uint8Array(await crypto.subtle.exportKey("raw", uaKeys.publicKey));
  const auth = crypto.getRandomValues(new Uint8Array(16));
  const id = `${which}-${Date.now()}`;
  const endpoint = `http://127.0.0.1:${pushPort}/push/${id}`;
  const arrived = new Promise<{ headers: Headers; body: Bytes }>((resolve) => pushes.set(id, { resolve }));
  // A profile of its own: the sign-in step's prompt left a denial behind.
  const { ctx, page: first } = await open(which, "push");
  const warnings: string[] = [];
  try {
    phase("subscribe");
    // The browser's push manager stands in: `subscribe` answers with this
    // suite's endpoint and keys, and everything else is the app's own path
    // (the service worker registered, the subscription sent to the instance).
    await ctx.addInitScript(
      ({ endpoint, p256dh, auth }) => {
        const bytes = (s: string) => {
          const raw = atob(s.replace(/-/g, "+").replace(/_/g, "/") + "=".repeat((4 - (s.length % 4)) % 4));
          const out = new Uint8Array(raw.length);
          for (let i = 0; i < raw.length; i++) out[i] = raw.charCodeAt(i);
          return out.buffer;
        };
        const subscription = { endpoint, getKey: (name: string) => bytes(name === "p256dh" ? p256dh : auth), unsubscribe: async () => true };
        // The headless shell answers every permission with a denial; the
        // grant is this suite's, not the browser's.
        Object.defineProperty(Notification, "permission", { configurable: true, get: () => "granted" });
        Notification.requestPermission = async () => "granted" as NotificationPermission;
        Object.defineProperty(ServiceWorkerRegistration.prototype, "pushManager", {
          configurable: true,
          get: () => ({ getSubscription: async () => null, subscribe: async () => subscription }),
        });
      },
      { endpoint, p256dh: base64url(uaPublic), auth: base64url(auth) },
    );
    await ctx.grantPermissions(["notifications"], { origin: base });
    const page = await ctx.newPage();
    live.page = page;
    page.on("console", (m) => {
      if (m.type() === "warning" || m.type() === "error") warnings.push(m.text());
    });
    await first.close();
    await trustCard(page);
    await page.getByLabel("Email or handle").fill(handle);
    await page.getByRole("textbox", { name: "Password" }).fill(password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    phase("composer");
    await page.getByPlaceholder(/^Message #/).waitFor();
    phase("turn on");
    await page.getByRole("button", { name: "Turn on notifications" }).click();
    phase("subscribed");
    try {
      await page.getByText("notifications on").waitFor();
    } catch (e) {
      await page.screenshot({ path: `${out}/${which}-push-failed.png` });
      const permission = await page.evaluate(() => Notification.permission);
      console.log(`  [${which}] push: permission ${permission}; ${warnings.join(" | ").slice(0, 300)}`);
      throw e;
    }
  } finally {
    await ctx.close();
  }
  // The browser is away now; a member's mention is pushed to it.
  phase("mention");
  const joiner = resolve(work, `cli-push-${which}`);
  const code = lumia(inviterDir, "invite-create", server).trim().split(/\s+/)[0];
  if (!code) throw new Error("no invite code");
  lumia(joiner, "instance", "add", instance);
  lumia(joiner, "account", "create", "--handle", `pusher${which}${Date.now()}`, "--password", password, "--invite", code);
  lumia(joiner, "send", channel, `Are you there, ${which}?`, "--mention", account);
  phase("waiting for the push");
  const push = await Promise.race([arrived, Bun.sleep(60_000).then(() => Promise.reject(new Error("the instance did not post the push within a minute")))]);
  pushes.delete(id);
  if (!(push.headers.get("authorization") ?? "").startsWith("vapid t=")) throw new Error("the push carries no VAPID authorization");
  if (push.headers.get("content-encoding") !== "aes128gcm") throw new Error(`the push is ${push.headers.get("content-encoding")}, not aes128gcm`);
  const notice = JSON.parse(await decryptPush(push.body, uaKeys.privateKey, uaPublic, auth)) as { title?: string; body?: string };
  if (notice.title !== "Lumia" || notice.body !== "New message") throw new Error(`the notice reads ${JSON.stringify(notice)}`);
  console.log(`  ${which}: subscribed, went away, and the mention came as a push the suite's service decrypted in ${Date.now() - t0} ms`);
}

if (!existsSync(cli)) run(["cargo", "build", "--release", "-p", "lumia-cli"], resolve(web, "../core"));
rmSync(work, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
await waitFor(`${instance}/api/v1/instance`, "the instance");

const stamp = Date.now();
const passwordDir = resolve(work, "cli-password");
const identityDir = resolve(work, "cli-identity");
lumia(passwordDir, "instance", "add", instance);
lumia(passwordDir, "account", "create", "--handle", `web${stamp}`, "--password", password);
const server = lumia(passwordDir, "server-create", "Web suite").match(/[0-9a-f-]{36}/)?.[0];
if (!server) throw new Error("no server id");
const general = lumia(passwordDir, "channel-create", server, "general").match(/[0-9a-f-]{36}/)?.[0];
if (!general) throw new Error("no channel id");
const webAccount = lumia(passwordDir, "whoami").trim();
lumia(identityDir, "instance", "add", instance);
lumia(identityDir, "account", "create", "--handle", `link${stamp}`, "--password", password);
lumia(identityDir, "identity", "create", "--label", "laptop");
lumia(identityDir, "identity", "link-account", "--password", password);
const linked = lumia(identityDir, "server-create", "Linked").match(/[0-9a-f-]{36}/)?.[0];
if (!linked) throw new Error("no server id");
lumia(identityDir, "channel-create", linked, "general");
const bAccount = lumia(identityDir, "dm", "setup").match(/account ([0-9a-f-]{36})/)?.[1];
if (!bAccount) throw new Error("no account id from dm setup");
console.log("seeded");

const vite = process.env.LUMIA_BASE ? null : Bun.spawn(["bun", "run", "dev"], { cwd: web, stdout: "ignore", stderr: "ignore" });
try {
  await waitFor(base, vite ? "vite" : "the served UI");
  if (engines.includes("webkit") && process.platform === "linux") {
    // WebKit's first page on a fresh Linux runner takes minutes (its caches
    // are built on first use); later launches take seconds. Pay it once here.
    const t0 = Date.now();
    const warm = await webkit.launchPersistentContext(resolve(work, "profile-warm-webkit"), { timeout: 120_000 });
    try {
      const page = await warm.newPage();
      await page.goto(`${base}/design`, { timeout: 300_000 });
      await page.locator(".lu-shell").waitFor({ timeout: 60_000 });
    } finally {
      await warm.close();
    }
    console.log(`  webkit warmed up in ${Date.now() - t0} ms`);
  }
  let failed = 0;
  for (const which of engines) {
    // A fresh CLI identity per browser, so each gets its own request.
    const aDir = resolve(work, `cli-a-${which}`);
    lumia(aDir, "instance", "add", instance);
    const aHandle = `a${which}${stamp}`;
    lumia(aDir, "account", "create", "--handle", aHandle, "--password", password);
    lumia(aDir, "identity", "create", "--label", "desk");
    lumia(aDir, "identity", "link-account", "--password", password);
    lumia(aDir, "dm", "setup");
    for (const [name, step] of [
      ["sign in", () => signInAndSend(which, `web${stamp}`)],
      ["served", () => instanceServed(which)],
      ["link", () => link(which, identityDir)],
      ["dm", () => dm(which, aDir, bAccount, aHandle)],
      ["invite", () => invite(which, passwordDir, server)],
      ["create", () => create(which, stamp)],
      ["push", () => webPush(which, `web${stamp}`, passwordDir, server, general, webAccount)],
    ] as const) {
      // A heartbeat while the step runs (CI shows a step's log only once it
      // ends) and a hard bound, so a hang names itself instead of the job
      // timing out.
      const started = Date.now();
      live.phase = "start";
      const beat = setInterval(() => console.log(`  [${which}] ${name}: ${Math.round((Date.now() - started) / 1000)} s, ${live.phase}, at ${live.page?.url() ?? "no page"}`), 30_000);
      try {
        await Promise.race([step(), Bun.sleep(240_000).then(() => Promise.reject(new Error(`${name} did not finish within 4 minutes`)))]);
      } catch (e) {
        failed++;
        console.log(`  ${which}: ${name} FAILED: ${e instanceof Error ? e.message.split("\n")[0] : String(e)}`);
        try {
          const page = live.page;
          if (page) {
            await page.screenshot({ path: `${out}/${which}-${name.replace(/\s+/g, "-")}-failed.png`, timeout: 10_000 });
            console.log(`  [${which}] at ${page.url()}: ${(await page.textContent("body", { timeout: 5_000 }))?.replace(/\s+/g, " ").slice(0, 200)}`);
          }
        } catch {
          /* the page may be gone */
        }
      } finally {
        clearInterval(beat);
      }
    }
  }
  console.log(failed ? `${failed} failed; screenshots in ${out}` : "ok");
  process.exitCode = failed ? 1 : 0;
} finally {
  vite?.kill();
  pushService.stop(true);
}
