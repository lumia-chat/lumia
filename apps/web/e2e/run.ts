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
import { crc32, deflateSync } from "node:zlib";

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
  // `LUMIA_E2E_CHANNEL=chromium` runs the full build instead of the headless
  // shell; Chromium's own log goes to stderr, which `DEBUG=pw:browser` shows.
  const options: Parameters<typeof engine.launchPersistentContext>[1] = { viewport: { width: 1440, height: 960 }, timeout: 60_000 };
  if (which === "chromium") {
    options.args = ["--enable-logging=stderr", "--v=0"];
    if (process.env.LUMIA_E2E_CHANNEL) options.channel = process.env.LUMIA_E2E_CHANNEL;
  }
  const launched = engine.launchPersistentContext(profile, options).then(async (ctx) => ({ ctx, page: await ctx.newPage() }));
  const { ctx, page } = await Promise.race([launched, Bun.sleep(90_000).then(() => Promise.reject(new Error(`${which} did not open a page within 90 s`)))]);
  console.log(`  [${which}] browser ${ctx.browser()?.version() ?? "?"} up`);
  live.page = page;
  // WebKit's first navigation of a run on Linux CI takes well over the default 30 s.
  page.setDefaultNavigationTimeout(90_000);
  page.on("pageerror", (e) => console.log(`  [${which}] page error: ${e.message}`));
  page.on("crash", () => console.log(`  [${which}] the page crashed`));
  page.on("worker", (w) => w.on("close", () => console.log(`  [${which}] worker closed: ${w.url().split("/").pop()}`)));
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

// What a failed step leaves: whether the page's main thread, each of its
// workers and the CDP session still answer, a screenshot, and the body's
// text. Each probe has its own bound, so a browser that answers nothing
// names itself instead of hanging the report. (Written for the Linux
// runner's "wedge", which these probes showed was no renderer hang: the
// browser had exited, its DevTools pipe cut by Bun 1.3.x, oven-sh/bun#47956.)
async function diagnose(which: string, page: Page, shot: string) {
  const bounded = async (label: string, work: () => Promise<string>, ms: number) => {
    const outcome = await Promise.race([work().catch((e: unknown) => `failed: ${e instanceof Error ? e.message.split("\n")[0] : String(e)}`), Bun.sleep(ms).then(() => `no answer in ${ms / 1000} s`)]);
    console.log(`  [${which}] ${label}: ${outcome}`);
  };
  console.log(`  [${which}] at ${page.url()}, ${page.isClosed() ? "closed" : "open"}, ${page.workers().length} workers`);
  await bounded("main thread", () => page.evaluate(() => "answers"), 5_000);
  for (const w of page.workers()) await bounded(`worker ${w.url().split("/").pop()}`, () => w.evaluate(() => "answers"), 5_000);
  if (which === "chromium") {
    await bounded(
      "cdp Runtime.evaluate",
      async () => {
        const cdp = await page.context().newCDPSession(page);
        const r = (await cdp.send("Runtime.evaluate", { expression: "1 + 1" })) as { result?: { value?: unknown } };
        await cdp.detach();
        return `answers ${String(r.result?.value)}`;
      },
      5_000,
    );
  }
  await bounded("screenshot", () => page.screenshot({ path: `${out}/${shot}.png`, timeout: 10_000 }).then(() => "ok"), 12_000);
  await bounded("body", async () => (await page.textContent("body", { timeout: 5_000 }))?.replace(/\s+/g, " ").slice(0, 300) ?? "empty", 6_000);
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
  await frameSteps(which, page);
  // Turning notifications on registers the service worker (Chromium:
  // Playwright's WebKit has no service workers; the subscription itself
  // needs a push service, which a headless browser has no account with).
  // It is on the Devices page, reached from the account menu.
  if (which === "chromium") {
    await page.context().grantPermissions(["notifications"], { origin: base });
    await page.getByRole("button", { name: /account menu/ }).click();
    await page.getByRole("menuitem", { name: "Devices" }).click();
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
  // Signing out from the account menu ends the session here: the sign-in
  // card, and the app's address leads back to it.
  phase("sign out");
  await page.getByRole("button", { name: /account menu/ }).click();
  await page.getByRole("menuitem", { name: "Sign out" }).click();
  await page.waitForURL(/\/sign-in/);
  await page.locator(".lu-card").waitFor();
  await goto(page, `${base}/app`);
  await page.waitForURL(/\/sign-in/);
  await page.locator(".lu-card").waitFor();
}

// The frame (the Quiet Split board): the app follows the system's light
// or dark setting, and a channel longer than its pane scrolls inside the
// pane while the page itself stays the size of the window. One person's
// quick messages are one block under one name.
async function frameSteps(which: string, page: Page) {
  phase("theme");
  const ground = () => page.evaluate(() => getComputedStyle(document.body).backgroundColor);
  await page.emulateMedia({ colorScheme: "dark" });
  const dark = await ground();
  await page.emulateMedia({ colorScheme: "light" });
  const light = await ground();
  if (dark !== "rgb(15, 18, 24)" || light !== "rgb(242, 244, 248)") throw new Error(`the ground is ${light} under a light system and ${dark} under a dark one`);
  phase("scroll");
  const composer = page.getByPlaceholder(/^Message #/);
  const stamp = Date.now();
  for (let i = 0; i < 40; i++) {
    await composer.fill(`line ${i} of ${stamp}`);
    await page.keyboard.press("Enter");
  }
  await page.getByText(`line 39 of ${stamp}`).waitFor();
  const sizes = await page.evaluate(() => {
    const list = document.querySelector(".lu-msglist");
    return {
      page: document.documentElement.scrollHeight,
      window: innerHeight,
      pane: list?.clientHeight ?? 0,
      log: list?.scrollHeight ?? 0,
      parts: document.querySelectorAll(".lu-msg__part").length,
    };
  });
  if (sizes.page > sizes.window) throw new Error(`the page is ${sizes.page} tall in a window of ${sizes.window}: it scrolls`);
  if (sizes.log <= sizes.pane) throw new Error(`the message list (${sizes.log}) does not overflow its pane (${sizes.pane})`);
  if (sizes.parts < 39) throw new Error(`40 quick messages drew ${sizes.parts} under the first: not one block`);
  await page.screenshot({ path: `${out}/${which}-scrolled.png` });
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
  // The profile (architecture 6.4): set here, signed by this browser's
  // device key, shown by the rail once the snapshot has it.
  phase("profile");
  await page.getByRole("button", { name: /account menu/ }).click();
  await page.getByRole("menuitem", { name: "View profile" }).click();
  await page.getByLabel("Display name").fill("Linked Browser");
  await page.getByLabel("Pronouns").fill("they/them");
  await page.getByLabel("Bio").fill("Signed in by identity proof.");
  // The signed profile's own Save: the account card over it has one too.
  await page.locator("form", { has: page.getByLabel("Bio") }).getByRole("button", { name: "Save", exact: true }).click();
  await page.getByText("Saved. This instance shows it now.").waitFor();
  await page.getByRole("button", { name: /^Linked Browser, / }).waitFor();
  // The status (protocol 6.7): chosen here, sent with the servers watched.
  await page.getByRole("tab", { name: "Away" }).click();
  await page.getByLabel("Custom status").fill("At lunch");
  await page.getByRole("button", { name: "Save status" }).click();
  await page.getByText("Status saved.").waitFor();
  await page.screenshot({ path: `${out}/${which}-profile.png` });
  // The account menu has the same status, and changes it.
  await page.getByRole("button", { name: /account menu/ }).click();
  await page.getByRole("menuitemradio", { name: "Away", checked: true }).waitFor();
  await page.getByRole("menuitemradio", { name: "Online" }).click();
  await page.getByRole("menuitemradio", { name: "Online", checked: true }).waitFor();
  await page.screenshot({ path: `${out}/${which}-account-menu.png` });
  await page.keyboard.press("Escape");
  await page.getByRole("menu").waitFor({ state: "detached" });
}

// A sign-in handed to another device (the native apps' browser sheet):
// the page signs in for a return URL, mints a one-time token and leaves
// with it; a fresh profile completes it and lands where `next` says.
async function handoff(which: string, handle: string) {
  const { ctx, page } = await open(which, "handoff");
  const t0 = Date.now();
  try {
    phase("sign in for a return");
    await goto(page, `${base}/sign-in?instance=${encodeURIComponent(instance)}&return=${encodeURIComponent(`${base}/design`)}`);
    await page.getByText("run by").waitFor();
    await page.getByLabel("Email or handle").fill(handle);
    await page.getByRole("textbox", { name: "Password" }).fill(password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await page.waitForURL(/\/design\?magic=/);
    const token = new URL(page.url()).searchParams.get("magic") ?? "";
    if (token.length < 32) throw new Error(`no handoff token in ${page.url()}`);
    phase("complete elsewhere");
    const again = await ctx.newPage();
    live.page = again;
    await page.close();
    await goto(again, `${base}/sign-in?instance=${encodeURIComponent(instance)}&magic=${encodeURIComponent(token)}&next=devices`);
    await again.getByText("Everything that holds your identity").waitFor();
    phase("spent");
    const dir = resolve(work, `cli-handoff-${which}`);
    rmSync(dir, { recursive: true, force: true });
    lumia(dir, "instance", "add", instance);
    let refused = "";
    try {
      // As one argument: a token is base64url and may start with a dash, which read as an option on its own.
      lumia(dir, "login", `--magic=${token}`);
    } catch (e) {
      refused = e instanceof Error ? e.message : String(e);
    }
    if (!/UNAUTHENTICATED/.test(refused)) throw new Error(`the spent token signed the CLI in, or failed another way: ${refused || "no error"}`);
  } finally {
    await ctx.close();
  }
  console.log(`  ${which}: signed in for a return URL, took the one-time token to a fresh page, which landed on Devices, and the token was spent, in ${Date.now() - t0} ms`);
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
  let tailed = "";
  // Twenty tries: the macOS runner's WebKit has taken most of a minute
  // over the group alone, and the CLI's store follows the channel log.
  for (let i = 0; i < 20 && !seen.includes(hello); i++) {
    const tail = Bun.spawnSync([cli, "--data-dir", aDir, "dm", "tail", channel], { cwd: web, timeout: 4_000 });
    tailed = tail.stdout.toString() + tail.stderr.toString();
    seen = lumia(aDir, "dm", "messages", channel);
  }
  if (!seen.includes(hello)) throw new Error(`the CLI did not read the browser's message; its last tail said:\n${tailed.slice(-600)}\nand it holds:\n${seen}`);
  const reply = `reply from the cli ${Date.now()}`;
  phase("cli send");
  lumia(aDir, "dm", "send", channel, reply);
  phase("wait for the reply");
  await page.getByText(reply).waitFor();
  // Back from disk: a fresh page in the same profile. (This once wedged
  // the browser on the Linux runner; that was Bun 1.3.x cutting the
  // DevTools pipe, oven-sh/bun#47956, not the page's teardown.)
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

// A small PNG of one colour, for the photo step: the instance decodes
// what it is given, so it has to be a real picture.
function png(size: number, [r, g, b]: [number, number, number]): Buffer {
  const chunk = (type: string, data: Buffer) => {
    const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
    const length = Buffer.alloc(4);
    length.writeUInt32BE(data.length);
    const sum = Buffer.alloc(4);
    sum.writeUInt32BE(crc32(body));
    return Buffer.concat([length, body, sum]);
  };
  const head = Buffer.alloc(13);
  head.writeUInt32BE(size, 0);
  head.writeUInt32BE(size, 4);
  head.set([8, 2, 0, 0, 0], 8); // eight bits a channel, RGB
  const row = Buffer.concat([Buffer.from([0]), Buffer.alloc(size * 3, Buffer.from([r, g, b]))]);
  const rows = Buffer.concat(Array.from({ length: size }, () => row));
  return Buffer.concat([Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), chunk("IHDR", head), chunk("IDAT", deflateSync(rows)), chunk("IEND", Buffer.alloc(0))]);
}

// The account's own handle and display name (`PATCH /account`), from the
// Profile page's Account card, on a browser with no identity (one with an
// identity keeps the name in its signed profile). An account of its own:
// the others are signed in to by handle later.
async function account(which: string, taken: string, stamp: number) {
  const dir = resolve(work, `cli-account-${which}`);
  const handle = `was${which}${stamp}`;
  lumia(dir, "instance", "add", instance);
  lumia(dir, "account", "create", "--handle", handle, "--password", password);
  const { ctx, page } = await open(which, "account");
  const t0 = Date.now();
  try {
    await trustCard(page);
    await page.getByLabel("Email or handle").fill(handle);
    await page.getByRole("textbox", { name: "Password" }).fill(password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await page.getByRole("button", { name: /account menu/ }).click();
    await page.getByRole("menuitem", { name: "View profile" }).click();
    const card = page.locator("form", { has: page.getByLabel("Handle") });
    const save = card.getByRole("button", { name: "Save", exact: true });
    phase("rename");
    await page.getByLabel("Handle").fill(`now${which}${stamp}`);
    await page.getByLabel("Display name").fill("Renamed Person");
    await save.click();
    await card.getByText("Saved.").waitFor();
    await page.getByRole("button", { name: /^Renamed Person, / }).click();
    await page.getByRole("menu").getByText(`@now${which}${stamp}`).waitFor();
    await page.keyboard.press("Escape");
    await page.screenshot({ path: `${out}/${which}-account.png` });
    phase("taken");
    await page.getByLabel("Handle").fill(taken);
    await save.click();
    await card.getByText("That handle is taken.").waitFor();
    phase("not a handle");
    await page.getByLabel("Handle").fill("A b");
    await save.click();
    await card.getByText("A handle is 2 to 32 letters").waitFor();
    // The photo (`PUT /account/avatar`): a file chosen on the same card is
    // the picture on the rail once the instance has made its square of it,
    // and it loads from the instance with no session; what is not a
    // picture is refused in words; Remove brings the initials back.
    phase("photo");
    const photo = page.locator(".app__photo");
    const file = page.getByLabel("Photo");
    await file.setInputFiles({ name: "me.png", mimeType: "image/png", buffer: png(64, [48, 103, 252]) });
    await photo.getByText("Saved.").waitFor();
    const onRail = page.locator(".lu-rail__self img");
    await onRail.waitFor();
    const drawn = await onRail.evaluate(async (img: HTMLImageElement) => {
      await img.decode();
      return { src: img.src, width: img.naturalWidth };
    });
    if (!/\/api\/v1\/avatars\/[0-9a-f]{64}$/.test(drawn.src) || drawn.width === 0) throw new Error(`the rail's picture is ${drawn.src} at ${drawn.width} wide`);
    if (!(await fetch(drawn.src)).ok) throw new Error("the picture does not load without a session");
    await page.screenshot({ path: `${out}/${which}-photo.png` });
    await file.setInputFiles({ name: "notes.txt", mimeType: "text/plain", buffer: Buffer.from("not a picture") });
    await photo.getByText("That file is not a picture Lumia can read.").waitFor();
    await photo.getByRole("button", { name: "Remove" }).click();
    await onRail.waitFor({ state: "detached" });
    await photo.getByRole("button", { name: "Choose a photo…" }).waitFor();
  } finally {
    await ctx.close();
  }
  console.log(`  ${which}: renamed its account, was refused a taken handle and one that is not a handle, set a photo that loads from the instance, was refused a file that is no picture, and removed the photo, in ${Date.now() - t0} ms`);
}

// Preferences, from the account menu: a chosen appearance holds whatever
// the system is in and "System" follows it again; Compact draws the
// small avatar and a block in less room; both are this browser's and are
// still there after a reload. An account of its own with three quick
// messages, so the channel is one block of known size.
async function preferencesStep(which: string, stamp: number) {
  const dir = resolve(work, `cli-preferences-${which}`);
  const handle = `pref${which}${stamp}`;
  lumia(dir, "instance", "add", instance);
  lumia(dir, "account", "create", "--handle", handle, "--password", password);
  const own = lumia(dir, "server-create", "Preferences").match(/[0-9a-f-]{36}/)?.[0];
  if (!own) throw new Error("no server id");
  const channel = lumia(dir, "channel-create", own, "general").match(/[0-9a-f-]{36}/)?.[0];
  if (!channel) throw new Error("no channel id");
  for (const line of ["One", "Two", "Three"]) lumia(dir, "send", channel, `${line} of ${stamp}`);
  const { ctx, page } = await open(which, "preferences");
  const t0 = Date.now();
  try {
    await page.emulateMedia({ colorScheme: "light" });
    await trustCard(page);
    await page.getByLabel("Email or handle").fill(handle);
    await page.getByRole("textbox", { name: "Password" }).fill(password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await page.getByText(`Three of ${stamp}`).waitFor();
    const block = () =>
      page.evaluate(() => {
        const message = document.querySelector(".lu-msglist .lu-msg");
        return { avatar: message?.querySelector(".lu-avatar")?.getBoundingClientRect().width ?? 0, height: message?.getBoundingClientRect().height ?? 0 };
      });
    const comfortable = await block();
    if (comfortable.avatar !== 40) throw new Error(`a comfortable message's avatar is ${comfortable.avatar} wide`);
    phase("the page");
    await page.getByRole("button", { name: /account menu/ }).click();
    await page.getByRole("menuitem", { name: "Preferences" }).click();
    await page.getByRole("heading", { name: "Appearance" }).waitFor();
    await page.screenshot({ path: `${out}/${which}-preferences.png` });
    phase("appearance");
    const ground = () => page.evaluate(() => getComputedStyle(document.body).backgroundColor);
    const DARK = "rgb(15, 18, 24)";
    const LIGHT = "rgb(242, 244, 248)";
    const appearance = page.getByRole("tablist", { name: "Appearance" });
    await appearance.getByRole("tab", { name: "Dark" }).click();
    if ((await ground()) !== DARK) throw new Error(`Dark under a light system draws the ground ${await ground()}`);
    await page.screenshot({ path: `${out}/${which}-preferences-dark.png` });
    await appearance.getByRole("tab", { name: "Light" }).click();
    await page.emulateMedia({ colorScheme: "dark" });
    if ((await ground()) !== LIGHT) throw new Error(`Light under a dark system draws the ground ${await ground()}`);
    await appearance.getByRole("tab", { name: "System" }).click();
    if ((await ground()) !== DARK) throw new Error(`System under a dark system draws the ground ${await ground()}`);
    await page.emulateMedia({ colorScheme: "light" });
    if ((await ground()) !== LIGHT) throw new Error(`System under a light system draws the ground ${await ground()}`);
    phase("density");
    await appearance.getByRole("tab", { name: "Dark" }).click();
    await page.getByRole("tablist", { name: "Message density" }).getByRole("tab", { name: "Compact" }).click();
    // A load from nothing: what is drawn now is what the browser kept.
    await goto(page, `${base}/app`);
    await page.getByText(`Three of ${stamp}`).waitFor();
    if ((await ground()) !== DARK) throw new Error(`after a reload the ground is ${await ground()}: Dark was not kept`);
    const compact = await block();
    if (compact.avatar !== 28) throw new Error(`a compact message's avatar is ${compact.avatar} wide`);
    if (compact.height >= comfortable.height) throw new Error(`a compact block is ${compact.height} tall and a comfortable one ${comfortable.height}`);
    await page.screenshot({ path: `${out}/${which}-compact.png` });
    phase("kept through sign-out");
    await page.getByRole("button", { name: /account menu/ }).click();
    await page.getByRole("menuitem", { name: "Sign out" }).click();
    await page.waitForURL(/\/sign-in/);
    if ((await ground()) !== DARK) throw new Error(`signed out, the ground is ${await ground()}: the appearance went with the account`);
  } finally {
    await ctx.close();
  }
  console.log(`  ${which}: opened Preferences from the account menu, chose each appearance against the system's, drew a block compact, and kept both through a reload and a sign-out, in ${Date.now() - t0} ms`);
}

// A server's settings (architecture 6.7): its owner renames it, keeps its
// messages for 30 days after being told what that deletes, and gives it a
// picture, from the menu under its name; the instance has all three. A
// member is not offered them, sees the new name and picture, and sees a
// rename made elsewhere arrive while the page is open.
async function serverSettings(which: string, stamp: number) {
  const ownerDir = resolve(work, `cli-settings-owner-${which}`);
  const memberDir = resolve(work, `cli-settings-member-${which}`);
  const owner = `own${which}${stamp}`;
  const member = `mem${which}${stamp}`;
  lumia(ownerDir, "instance", "add", instance);
  lumia(ownerDir, "account", "create", "--handle", owner, "--password", password);
  const server = lumia(ownerDir, "server-create", `Settings ${stamp}`).match(/[0-9a-f-]{36}/)?.[0];
  if (!server) throw new Error("no server id");
  lumia(ownerDir, "channel-create", server, "general");
  const code = lumia(ownerDir, "invite-create", server).trim().split(/\s+/)[0];
  if (!code) throw new Error("no invite code");
  lumia(memberDir, "instance", "add", instance);
  lumia(memberDir, "account", "create", "--handle", member, "--password", password, "--invite", code);
  const signIn = async (page: Page, handle: string) => {
    await trustCard(page);
    await page.getByLabel("Email or handle").fill(handle);
    await page.getByRole("textbox", { name: "Password" }).fill(password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await page.getByPlaceholder("Message #general").waitFor();
  };
  const t0 = Date.now();
  {
    const { ctx, page } = await open(which, "settings-owner");
    try {
      await signIn(page, owner);
      phase("the page");
      await page.getByRole("button", { name: /server menu/ }).click();
      await page.getByRole("menuitem", { name: "Server settings" }).click();
      await page.getByRole("heading", { name: "Overview" }).waitFor();
      phase("name");
      const overview = page.locator("section", { has: page.getByRole("heading", { name: "Overview" }) });
      await overview.getByLabel("Name").fill(`Renamed ${stamp}`);
      await overview.getByRole("button", { name: "Save", exact: true }).click();
      await overview.getByText("Saved.").waitFor();
      await page.getByRole("button", { name: new RegExp(`^Renamed ${stamp}`) }).first().waitFor();
      phase("retention");
      const history = page.locator("section", { has: page.getByRole("heading", { name: "Message history" }) });
      await history.getByRole("tab", { name: "30 days" }).click();
      await history.getByText("Messages older than 30 days will be deleted for everyone. This cannot be undone.").waitFor();
      await page.screenshot({ path: `${out}/${which}-server-settings.png` });
      await history.getByRole("button", { name: "Save", exact: true }).click();
      await history.getByText("Saved.").waitFor();
      phase("picture");
      await page.getByLabel("Server picture").setInputFiles({ name: "logo.png", mimeType: "image/png", buffer: png(64, [240, 120, 40]) });
      await overview.getByText("Saved.").waitFor();
      const onRail = page.locator(".lu-rail__server img");
      await onRail.waitFor();
      if (!/\/api\/v1\/avatars\/[0-9a-f]{64}$/.test((await onRail.getAttribute("src")) ?? "")) throw new Error("the rail's picture is not the instance's");
    } finally {
      await ctx.close();
    }
  }
  // The instance has all three, as a member asks for the server.
  const json = { "content-type": "application/json", accept: "application/json" };
  const pair = (await (await fetch(`${instance}/api/v1/auth/sign-in/password`, { method: "POST", headers: json, body: JSON.stringify({ handleOrEmail: member, password }) })).json()) as { accessToken?: string };
  if (!pair.accessToken) throw new Error("the suite could not sign in as the member");
  const held = (await (await fetch(`${instance}/api/v1/servers/${server}`, { headers: { ...json, authorization: `Bearer ${pair.accessToken}` } })).json()) as { name?: string; retentionDays?: number; iconUrl?: string };
  if (held.name !== `Renamed ${stamp}` || held.retentionDays !== 30 || !held.iconUrl) throw new Error(`the instance holds ${JSON.stringify(held)}`);
  {
    const { ctx, page } = await open(which, "settings-member");
    try {
      await signIn(page, member);
      phase("a member");
      await page.getByRole("button", { name: new RegExp(`^Renamed ${stamp}`) }).first().waitFor();
      await page.locator(".lu-rail__server img").waitFor();
      await page.getByRole("button", { name: /server menu/ }).click();
      await page.getByRole("menuitem", { name: "Invite people" }).waitFor();
      if ((await page.getByRole("menuitem", { name: "Server settings" }).count()) !== 0) throw new Error("a member is offered the settings");
      await page.keyboard.press("Escape");
      phase("told");
      lumia(ownerDir, "server-update", server, "--name", `Again ${stamp}`);
      await page.getByRole("button", { name: new RegExp(`^Again ${stamp}`) }).first().waitFor({ timeout: 20_000 });
    } finally {
      await ctx.close();
    }
  }
  console.log(`  ${which}: renamed a server, kept its messages for 30 days after the warning, gave it a picture, a member was not offered the settings and saw a rename arrive, in ${Date.now() - t0} ms`);
}

// The middle button held over a pane and moved up from where it came down
// scrolls the pane up, with a mark where it came down, until it comes up
// (client/autoscroll.ts); a primary drag does none of it. A field being
// typed in draws no focus ring.
async function autoscrollStep(which: string, stamp: number) {
  const dir = resolve(work, `cli-autoscroll-${which}`);
  const handle = `scroll${which}${stamp}`;
  lumia(dir, "instance", "add", instance);
  lumia(dir, "account", "create", "--handle", handle, "--password", password);
  const own = lumia(dir, "server-create", "Scroll").match(/[0-9a-f-]{36}/)?.[0];
  if (!own) throw new Error("no server id");
  const channel = lumia(dir, "channel-create", own, "general").match(/[0-9a-f-]{36}/)?.[0];
  if (!channel) throw new Error("no channel id");
  for (let i = 0; i < 60; i++) lumia(dir, "send", channel, `line ${i} of ${stamp}`);
  const { ctx, page } = await open(which, "autoscroll");
  const t0 = Date.now();
  try {
    await trustCard(page);
    phase("no ring");
    const field = page.getByLabel("Email or handle");
    await field.focus();
    const ring = await field.evaluate((el) => {
      const box = el.closest(".lu-textfield__box");
      return { outline: getComputedStyle(el).outlineStyle, shadow: box ? getComputedStyle(box).boxShadow : "none" };
    });
    if (ring.outline !== "none" || ring.shadow !== "none") throw new Error(`a focused field draws ${JSON.stringify(ring)}`);
    await field.fill(handle);
    await page.getByRole("textbox", { name: "Password" }).fill(password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await page.getByText(`line 59 of ${stamp}`).waitFor();
    const composer = page.getByPlaceholder("Message #general");
    await composer.focus();
    const composerRing = await composer.evaluate((el) => getComputedStyle(el).outlineStyle);
    if (composerRing !== "none") throw new Error(`the focused composer draws a ${composerRing} outline`);
    phase("autoscroll");
    const list = page.locator(".lu-msglist");
    const top = () => list.evaluate((el) => el.scrollTop);
    const box = await list.boundingBox();
    if (!box) throw new Error("no message list");
    const [x, y] = [box.x + box.width / 2, box.y + box.height / 2];
    // A primary drag is a selection's, not this.
    await page.mouse.move(x, y);
    await page.mouse.down();
    await page.mouse.move(x, y - 150, { steps: 5 });
    if ((await page.locator(".lu-autoscroll-mark").count()) !== 0) throw new Error("a primary drag autoscrolls");
    await page.mouse.up();
    await page.mouse.move(x, y);
    const before = await top();
    await page.mouse.down({ button: "middle" });
    await page.locator(".lu-autoscroll-mark").waitFor();
    await page.mouse.move(x, y - 150, { steps: 5 });
    await page.waitForTimeout(700);
    const during = await top();
    if (!(during < before - 100)) throw new Error(`held 150 above, the list went from ${before} to ${during}`);
    await page.screenshot({ path: `${out}/${which}-autoscroll.png` });
    await page.mouse.up({ button: "middle" });
    await page.locator(".lu-autoscroll-mark").waitFor({ state: "detached" });
    const stopped = await top();
    await page.waitForTimeout(300);
    if ((await top()) !== stopped) throw new Error("it went on after the button came up");
  } finally {
    await ctx.close();
  }
  console.log(`  ${which}: drew no ring on a focused field, autoscrolled the message list with the middle button held and stopped when it came up, and did not on a primary drag, in ${Date.now() - t0} ms`);
}

// The aside beside a conversation (the member list): a wide window puts it
// away and brings it back from the header's toggle, and remembers that; a
// narrow one slides it over the conversation's end, and the shade or
// Escape puts it away.
async function asideStep(which: string, stamp: number) {
  const dir = resolve(work, `cli-aside-${which}`);
  const handle = `aside${which}${stamp}`;
  lumia(dir, "instance", "add", instance);
  lumia(dir, "account", "create", "--handle", handle, "--password", password);
  const own = lumia(dir, "server-create", "Aside").match(/[0-9a-f-]{36}/)?.[0];
  if (!own) throw new Error("no server id");
  lumia(dir, "channel-create", own, "general");
  const { ctx, page } = await open(which, "aside");
  const t0 = Date.now();
  try {
    await trustCard(page);
    await page.getByLabel("Email or handle").fill(handle);
    await page.getByRole("textbox", { name: "Password" }).fill(password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await page.getByPlaceholder("Message #general").waitFor();
    const aside = page.locator(".lu-shell__aside");
    phase("wide");
    await aside.waitFor();
    await page.getByRole("button", { name: "Hide members" }).click();
    await aside.waitFor({ state: "hidden" });
    await page.reload();
    await page.getByPlaceholder("Message #general").waitFor();
    if (await aside.isVisible()) throw new Error("put away, then back after a reload");
    await page.getByRole("button", { name: "Show members" }).click();
    await aside.waitFor();
    phase("narrow");
    await page.setViewportSize({ width: 900, height: 800 });
    await aside.waitFor({ state: "hidden" });
    await page.getByRole("button", { name: "Show members" }).click();
    await aside.waitFor();
    // Slid all the way in: at the window's end.
    await page.waitForFunction(() => getComputedStyle(document.querySelector(".lu-shell__aside")!).transform === "none");
    const box = await aside.boundingBox();
    if (!box || Math.abs(box.x + box.width - 900) > 1) throw new Error(`the aside stands at ${JSON.stringify(box)}`);
    await page.screenshot({ path: `${out}/${which}-aside-over.png` });
    await page.getByRole("button", { name: "Close the side panel" }).click({ position: { x: 40, y: 300 } });
    await aside.waitFor({ state: "hidden" });
    await page.getByRole("button", { name: "Show members" }).click();
    await aside.waitFor();
    await page.keyboard.press("Escape");
    await aside.waitFor({ state: "hidden" });
    phase("wide again");
    await page.setViewportSize({ width: 1440, height: 960 });
    await aside.waitFor();
  } finally {
    await ctx.close();
  }
  console.log(`  ${which}: put the member list away and back in a wide window and kept it across a reload, slid it over a narrow one and put it away by the shade and by Escape, in ${Date.now() - t0} ms`);
}

// Being in channels and server DMs (architecture 6.8 and 6.4): a member
// is in the server's default channel only, finds another in Browse,
// reads it before joining with Join where the composer would be, joins
// (and its notice says so), leaves, and from the owner's profile card
// opens a server DM, which says whose it is, and writes in it.
async function membershipStep(which: string, stamp: number) {
  const ownerDir = resolve(work, `cli-members-owner-${which}`);
  const memberDir = resolve(work, `cli-members-member-${which}`);
  const owner = `keeper${which}${stamp}`;
  const member = `joiner${which}${stamp}`;
  lumia(ownerDir, "instance", "add", instance);
  lumia(ownerDir, "account", "create", "--handle", owner, "--password", password);
  const server = lumia(ownerDir, "server-create", `Members ${stamp}`).match(/[0-9a-f-]{36}/)?.[0];
  if (!server) throw new Error("no server id");
  lumia(ownerDir, "channel-create", server, "general");
  const design = lumia(ownerDir, "channel-create", server, "design").match(/[0-9a-f-]{36}/)?.[0];
  if (!design) throw new Error("no channel id");
  lumia(ownerDir, "send", design, "Read me before you join.");
  const code = lumia(ownerDir, "invite-create", server).trim().split(/\s+/)[0];
  if (!code) throw new Error("no invite code");
  lumia(memberDir, "instance", "add", instance);
  lumia(memberDir, "account", "create", "--handle", member, "--password", password);
  lumia(memberDir, "join", code);
  const { ctx, page } = await open(which, "membership");
  const t0 = Date.now();
  try {
    await trustCard(page);
    await page.getByLabel("Email or handle").fill(member);
    await page.getByRole("textbox", { name: "Password" }).fill(password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await page.getByPlaceholder("Message #general").waitFor();
    const sidebar = page.locator(".lu-shell__sidebar");
    if ((await sidebar.getByRole("link", { name: "design" }).count()) > 0) throw new Error("design is listed before joining it");
    phase("browse");
    await page.getByRole("button", { name: "Add channels" }).click();
    await page.getByRole("menuitem", { name: "Browse channels" }).click();
    await page.getByRole("button", { name: "design, 1 member" }).click();
    phase("preview");
    await page.getByText("You’re not in #design").waitFor();
    await page.getByText("Read me before you join.").waitFor();
    if ((await page.getByPlaceholder("Message #design").count()) > 0) throw new Error("a composer before joining");
    await page.screenshot({ path: `${out}/${which}-membership-preview.png` });
    phase("join");
    await page.locator(".lu-joinbar").getByRole("button", { name: "Join" }).click();
    await page.getByPlaceholder("Message #design").waitFor();
    await page.getByText(`${member} joined`).waitFor();
    await sidebar.getByRole("link", { name: "design" }).waitFor();
    await page.screenshot({ path: `${out}/${which}-membership-joined.png` });
    phase("leave");
    await page.getByRole("button", { name: "Leave channel" }).click();
    await sidebar.getByRole("link", { name: "design" }).waitFor({ state: "detached" });
    await page.getByPlaceholder("Message #general").waitFor();
    phase("server DM");
    await page.locator(".lu-shell__aside").getByRole("button", { name: new RegExp(`^${owner}`) }).click();
    await page.getByRole("button", { name: "Message privately" }).waitFor();
    await page.getByRole("button", { name: `Message ${owner}` }).click();
    await page.getByText("Messages here are kept by").waitFor();
    await page.getByPlaceholder(`Message ${owner}`).fill("Just between us and the server.");
    await page.keyboard.press("Enter");
    await page.getByText("Just between us and the server.").waitFor();
    await sidebar.getByRole("link", { name: owner }).waitFor();
    await page.screenshot({ path: `${out}/${which}-membership-dm.png` });
  } finally {
    await ctx.close();
  }
  console.log(`  ${which}: found a channel in Browse, read it before joining, joined with its notice, left it, and wrote in a server DM opened from a profile card, in ${Date.now() - t0} ms`);
}

// A session revoked from elsewhere (protocol 12), as "sign out everywhere"
// or a reused refresh token does it: the gateway is told and its loop
// ends, the page asks for the snapshot, is refused, and goes back to the
// sign-in card, which says what happened. An account of its own, so no
// other step's session goes with it.
async function revoked(which: string, stamp: number) {
  const dir = resolve(work, `cli-revoked-${which}`);
  const handle = `gone${which}${stamp}`;
  lumia(dir, "instance", "add", instance);
  lumia(dir, "account", "create", "--handle", handle, "--password", password);
  const { ctx, page } = await open(which, "revoked");
  const t0 = Date.now();
  try {
    await trustCard(page);
    await page.getByLabel("Email or handle").fill(handle);
    await page.getByRole("textbox", { name: "Password" }).fill(password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    phase("connected");
    await page.locator('.lu-rail__self[title$="connected"]').waitFor();
    phase("revoke");
    const json = { "content-type": "application/json", accept: "application/json" };
    const pair = (await (await fetch(`${instance}/api/v1/auth/sign-in/password`, { method: "POST", headers: json, body: JSON.stringify({ handleOrEmail: handle, password }) })).json()) as { accessToken?: string };
    if (!pair.accessToken) throw new Error("the suite could not sign in beside the browser");
    const mine = { ...json, authorization: `Bearer ${pair.accessToken}` };
    const list = (await (await fetch(`${instance}/api/v1/sessions`, { headers: mine })).json()) as { sessions?: { sessionId: string; current?: boolean }[] };
    const others = (list.sessions ?? []).filter((s) => !s.current);
    if (others.length === 0) throw new Error("the account has no session but the suite's");
    for (const s of others) {
      const answer = await fetch(`${instance}/api/v1/sessions/${s.sessionId}`, { method: "DELETE", headers: mine });
      if (!answer.ok) throw new Error(`revoking a session answered ${answer.status}`);
    }
    phase("sent back");
    await page.waitForURL(/\/sign-in/);
    await page.getByText("You were signed out of this browser").waitFor();
    await page.screenshot({ path: `${out}/${which}-revoked.png` });
  } finally {
    await ctx.close();
  }
  console.log(`  ${which}: its session revoked from elsewhere, the browser went back to sign-in and said so, in ${Date.now() - t0} ms`);
}

// A server and a channel from the app: the rail's "+" makes a server the
// account owns, "New channel" in the menu under its name its first channel.
async function create(which: string, handle: string, stamp: number) {
  // A profile of its own, signed in here: WebKit does not carry a
  // profile's session from one step to the next.
  const { ctx, page } = await open(which, "create");
  const t0 = Date.now();
  try {
    await trustCard(page);
    await page.getByLabel("Email or handle").fill(handle);
    await page.getByRole("textbox", { name: "Password" }).fill(password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await page.getByPlaceholder(/^Message #/).waitFor();
    phase("server");
    await page.getByRole("button", { name: "Create, join or discover servers" }).click();
    await page.getByLabel("Name").fill(`Made ${stamp}`);
    await page.getByRole("button", { name: "Create", exact: true }).click();
    await page.getByText("No channel open").waitFor();
    if ((await page.getByText(`Made ${stamp}`).count()) === 0) throw new Error("the new server is not shown");
    phase("channel");
    await page.getByRole("button", { name: /server menu/ }).click();
    await page.getByRole("menuitem", { name: "New channel" }).click();
    await page.getByLabel("Name", { exact: true }).fill("ops");
    await page.getByRole("button", { name: "Create", exact: true }).click();
    await page.getByPlaceholder("Message #ops").waitFor();
    // Presence (protocol 6.7): the page watches the server on screen and
    // sees itself in the members aside as online.
    phase("presence");
    await page.getByText("Online · 1").waitFor();
    // A member's row opens their profile card; this one is the account's own.
    await page.getByRole("button", { name: /You/ }).first().click();
    await page.getByRole("dialog").getByText("Online", { exact: true }).waitFor();
    await page.screenshot({ path: `${out}/${which}-card.png` });
    await page.getByRole("dialog").getByRole("button", { name: "Close profile" }).click();
    await page.getByRole("dialog").waitFor({ state: "detached" });
    // An invite link from the menu under the server's name, limited to one
    // use, then revoked: the instance stops answering for it.
    phase("invite");
    await page.getByRole("button", { name: /server menu/ }).click();
    await page.getByRole("menuitem", { name: "Invite people" }).click();
    await page.getByRole("radio", { name: "Once" }).click();
    await page.getByRole("button", { name: "Make a link" }).click();
    const link = await page.getByLabel("Invite link").inputValue();
    const code = link.split("/invite/")[1] ?? "";
    if (!code) throw new Error(`no code in the link ${link}`);
    if ((await fetch(`${instance}/api/v1/invites/${code}`)).status !== 200) throw new Error("the instance does not know the link");
    await page.getByText(`${code} · 0/1 uses`).waitFor();
    await page.screenshot({ path: `${out}/${which}-created.png` });
    await page.getByRole("button", { name: "Revoke" }).click();
    await page.getByText(`${code} · 0/1 uses`).waitFor({ state: "detached" });
    if ((await fetch(`${instance}/api/v1/invites/${code}`)).status !== 404) throw new Error("the revoked link still answers");
    // The invite as a mail (architecture 6.2): sent to an address, its
    // link in the dev mailbox, listed with the address, then revoked.
    await page.getByLabel("Email", { exact: true }).fill(`guest${stamp}@example.test`);
    await page.getByRole("button", { name: "Send by email" }).click();
    await page.getByText(/^Sent to guest/).waitFor();
    let mailed = "";
    for (let i = 0; i < 20 && !mailed; i++) {
      const box = (await (await fetch(`${instance}/dev/mailbox/json`)).json()) as { data?: { to?: unknown[]; text_body?: string }[] };
      for (const m of box.data ?? []) {
        const hit = /\/invite\/([A-Za-z0-9_-]+)/.exec(m.text_body ?? "");
        if (hit && JSON.stringify(m.to ?? []).includes(`guest${stamp}@example.test`)) mailed = hit[1] ?? "";
      }
      if (!mailed) await new Promise((r) => setTimeout(r, 250));
    }
    if (!mailed) throw new Error("no invite mail in the dev mailbox");
    if ((await fetch(`${instance}/api/v1/invites/${mailed}`)).status !== 200) throw new Error("the instance does not know the mailed link");
    await page.locator("span", { hasText: `to guest${stamp}@example.test` }).first().waitFor();
  } finally {
    await ctx.close();
  }
  console.log(`  ${which}: made a server and its first channel from the app, saw itself online there, made a one-use invite and revoked it, and emailed one that landed in the mailbox, in ${Date.now() - t0} ms`);
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
    await page.getByRole("button", { name: /account menu/ }).click();
    await page.getByRole("menuitem", { name: "Devices" }).click();
    await page.getByRole("button", { name: "Turn on notifications" }).click();
    phase("subscribed");
    try {
      await page.getByText("This browser gets a notification").waitFor();
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

// Magic-link sign-in (architecture 6.3): an account with an address asks
// for a link from the sign-in page, the mail lands in the dev instance's
// local mailbox, and its link signs this browser in.
async function magicLink(which: string) {
  const { ctx, page } = await open(which, "magic");
  const t0 = Date.now();
  try {
    const dir = resolve(work, `cli-magic-${which}`);
    const handle = `magic${which}${Date.now()}`;
    const email = `${handle}@example.com`;
    lumia(dir, "instance", "add", instance);
    lumia(dir, "account", "create", "--handle", handle, "--password", password, "--email", email);
    phase("ask");
    await trustCard(page);
    await page.getByLabel("Email or handle").fill(email);
    await page.getByRole("button", { name: "Email me a sign-in link" }).click();
    await page.getByText("A link is on its way").waitFor();
    phase("mail");
    let token = "";
    for (let i = 0; i < 20 && !token; i++) {
      const box = (await (await fetch(`${instance}/dev/mailbox/json`)).json()) as { data?: { to?: unknown[]; text_body?: string }[] };
      const mail = (box.data ?? []).find((m) => (m.to ?? []).some((t) => JSON.stringify(t).includes(email)));
      token = mail?.text_body?.split("\n").find((l) => /^[A-Za-z0-9_-]{32}$/.test(l.trim()))?.trim() ?? "";
      if (!token) await Bun.sleep(500);
    }
    if (!token) throw new Error("no magic-link mail in the dev mailbox");
    phase("link");
    await goto(page, `${base}/sign-in?magic=${encodeURIComponent(token)}&instance=${encodeURIComponent(instance)}`);
    await page.getByRole("button", { name: "Create, join or discover servers" }).waitFor();
    await page.screenshot({ path: `${out}/${which}-magic.png` });
  } finally {
    await ctx.close();
  }
  console.log(`  ${which}: asked for a magic link, took its token from the mailbox, and the link signed the browser in, in ${Date.now() - t0} ms`);
}

// Passkeys (architecture 6.3), with Chromium's virtual authenticator over
// the DevTools protocol: signed in with a password, the Devices page makes
// a passkey; the sign-in page then signs in with it.
async function passkey(which: string, handle: string) {
  if (which !== "chromium") {
    console.log(`  ${which}: no virtual authenticator; passkeys are checked in Chromium`);
    return;
  }
  const { ctx, page } = await open(which, "passkey");
  const t0 = Date.now();
  try {
    const cdp = await ctx.newCDPSession(page);
    await cdp.send("WebAuthn.enable");
    await cdp.send("WebAuthn.addVirtualAuthenticator", {
      options: { protocol: "ctap2", transport: "internal", hasResidentKey: true, hasUserVerification: true, isUserVerified: true, automaticPresenceSimulation: true },
    });
    phase("sign in");
    await trustCard(page);
    await page.getByLabel("Email or handle").fill(handle);
    await page.getByRole("textbox", { name: "Password" }).fill(password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await page.getByPlaceholder(/^Message #/).waitFor();
    phase("register");
    await goto(page, `${base}/app?place=home&view=devices`);
    await page.getByLabel("Name for the new passkey").fill("Suite key");
    await page.getByRole("button", { name: "Add a passkey" }).click();
    await page.getByText("Suite key · added").waitFor();
    await page.screenshot({ path: `${out}/${which}-passkey.png` });
    // The passkey as the second factor (protocol 6.1's details.mfa): required
    // from Devices, a password sign-in then asks the authenticator for it.
    phase("require");
    const require = page.getByLabel("Require a passkey when signing in with a password");
    await require.check();
    await goto(page, `${base}/sign-in`);
    await page.locator(".lu-card").waitFor();
    if ((await page.getByLabel("Instance").count()) > 0) {
      await page.getByLabel("Instance").fill(instance);
      await page.getByRole("button", { name: "Continue" }).click();
    }
    await page.getByText("run by").waitFor();
    await page.getByLabel("Email or handle").fill(handle);
    await page.getByRole("textbox", { name: "Password" }).fill(password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await page.getByPlaceholder(/^Message #/).waitFor();
    await goto(page, `${base}/app?place=home&view=devices`);
    await page.getByLabel("Require a passkey when signing in with a password").waitFor();
    await page.getByLabel("Require a passkey when signing in with a password").uncheck();
    phase("assert");
    // Straight to the sign-in page (the root would send a signed-in browser
    // to the app); it reaches the instance once asked, as a fresh visit would.
    await goto(page, `${base}/sign-in`);
    await page.locator(".lu-card").waitFor();
    const field = page.getByLabel("Instance");
    if ((await field.count()) > 0) {
      await field.fill(instance);
      await page.getByRole("button", { name: "Continue" }).click();
    }
    await page.getByText("run by").waitFor();
    await page.getByLabel("Email or handle").fill(handle);
    await page.getByRole("button", { name: "Sign in with a passkey" }).click();
    try {
      await page.getByPlaceholder(/^Message #/).waitFor();
    } catch (e) {
      console.log(`  [${which}] passkey: ${(await page.locator(".auth__error").allTextContents()).join(" | ") || "no error shown"}; ${(await page.textContent("body"))?.replace(/\s+/g, " ").slice(0, 160)}`);
      throw e;
    }
    phase("remove");
    await goto(page, `${base}/app?place=home&view=devices`);
    await page.getByText("Suite key · added").waitFor();
    await page.getByRole("button", { name: "Remove" }).first().click();
    await page.getByText("Suite key · added").waitFor({ state: "detached" });
  } finally {
    await ctx.close();
  }
  console.log(`  ${which}: made a passkey, required it beside the password and signed in that way, signed in with it alone, and removed it in ${Date.now() - t0} ms`);
}

// OIDC sign-in (architecture 6.3), with a provider of the suite's own: the
// discovery document, an authorize page that sends the browser straight
// back with a code, a token endpoint that signs an ID token for that
// code's nonce with the suite's RSA key, the key set and userinfo. The
// step runs when the instance names a provider (LUMIA_OIDC_ISSUER at
// this port, with LUMIA_OIDC_CLIENT_ID=suite), and is skipped otherwise.
const oidcPort = 4920;
const oidcIssuer = `http://127.0.0.1:${oidcPort}`;
const oidcKeys = await crypto.subtle.generateKey({ name: "RSASSA-PKCS1-v1_5", modulusLength: 2048, publicExponent: new Uint8Array([1, 0, 1]), hash: "SHA-256" }, true, ["sign", "verify"]);
const oidcCodes = new Map<string, { nonce: string; clientId: string }>();
const oidcSubject = `suite-${Date.now()}`;
const oidcService = Bun.serve({
  port: oidcPort,
  hostname: "127.0.0.1",
  async fetch(req) {
    const url = new URL(req.url);
    const json = (body: unknown) => new Response(JSON.stringify(body), { headers: { "content-type": "application/json" } });
    if (url.pathname === "/.well-known/openid-configuration") {
      return json({
        issuer: oidcIssuer,
        authorization_endpoint: `${oidcIssuer}/authorize`,
        token_endpoint: `${oidcIssuer}/token`,
        jwks_uri: `${oidcIssuer}/jwks`,
        userinfo_endpoint: `${oidcIssuer}/userinfo`,
        response_types_supported: ["code"],
        subject_types_supported: ["public"],
        id_token_signing_alg_values_supported: ["RS256"],
        token_endpoint_auth_methods_supported: ["client_secret_basic", "client_secret_post"],
        code_challenge_methods_supported: ["S256"],
      });
    }
    if (url.pathname === "/authorize") {
      const code = `code-${Date.now()}`;
      oidcCodes.set(code, { nonce: url.searchParams.get("nonce") ?? "", clientId: url.searchParams.get("client_id") ?? "" });
      const back = new URL(url.searchParams.get("redirect_uri") ?? "");
      back.searchParams.set("code", code);
      back.searchParams.set("state", url.searchParams.get("state") ?? "");
      return Response.redirect(back.toString(), 302);
    }
    if (url.pathname === "/token") {
      const form = new URLSearchParams(await req.text());
      const issued = oidcCodes.get(form.get("code") ?? "");
      if (!issued) return new Response("unknown code", { status: 400 });
      const now = Math.floor(Date.now() / 1000);
      const claims = { iss: oidcIssuer, sub: oidcSubject, aud: issued.clientId, exp: now + 300, iat: now, nonce: issued.nonce, email: `${oidcSubject}@example.com`, preferred_username: "suite.person", name: "Suite Person" };
      const part = (o: unknown) => base64url(new TextEncoder().encode(JSON.stringify(o)));
      const signingInput = `${part({ alg: "RS256", typ: "JWT" })}.${part(claims)}`;
      const signature = await crypto.subtle.sign("RSASSA-PKCS1-v1_5", oidcKeys.privateKey, new TextEncoder().encode(signingInput));
      return json({ access_token: "access", token_type: "Bearer", expires_in: 300, id_token: `${signingInput}.${base64url(signature)}` });
    }
    if (url.pathname === "/jwks") {
      const jwk = await crypto.subtle.exportKey("jwk", oidcKeys.publicKey);
      return json({ keys: [{ kty: jwk.kty, n: jwk.n, e: jwk.e, alg: "RS256", use: "sig" }] });
    }
    if (url.pathname === "/userinfo") return json({ sub: oidcSubject, email: `${oidcSubject}@example.com`, preferred_username: "suite.person", name: "Suite Person" });
    return new Response("not here", { status: 404 });
  },
});

async function oidc(which: string) {
  const info = (await (await fetch(`${instance}/api/v1/instance`)).json()) as { oidcName?: string };
  if (!info.oidcName) {
    console.log(`  ${which}: the instance names no OIDC provider (LUMIA_OIDC_ISSUER=${oidcIssuer} LUMIA_OIDC_CLIENT_ID=suite to try it); skipped`);
    return;
  }
  const { ctx, page } = await open(which, "oidc");
  const t0 = Date.now();
  try {
    phase("provider");
    await trustCard(page);
    await page.getByRole("button", { name: info.oidcName }).click();
    phase("back");
    try {
      await page.getByRole("button", { name: "Create, join or discover servers" }).waitFor();
    } catch (e) {
      console.log(`  [${which}] oidc: at ${page.url()}: ${(await page.textContent("body"))?.replace(/\s+/g, " ").slice(0, 300)}`);
      throw e;
    }
    await page.screenshot({ path: `${out}/${which}-oidc.png` });
  } finally {
    await ctx.close();
  }
  console.log(`  ${which}: went to the provider, came back with a token, and was signed in as its subject in ${Date.now() - t0} ms`);
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
      ["create", () => create(which, `web${stamp}`, stamp)],
      ["revoked", () => revoked(which, stamp)],
      ["account", () => account(which, `web${stamp}`, stamp)],
      ["preferences", () => preferencesStep(which, stamp)],
      ["server settings", () => serverSettings(which, stamp)],
      ["autoscroll", () => autoscrollStep(which, stamp)],
      ["aside", () => asideStep(which, stamp)],
      ["membership", () => membershipStep(which, stamp)],
      ["push", () => webPush(which, `web${stamp}`, passwordDir, server, general, webAccount)],
      ["magic link", () => magicLink(which)],
      ["passkey", () => passkey(which, `web${stamp}`)],
      ["handoff", () => handoff(which, `web${stamp}`)],
      ["oidc", () => oidc(which)],
    ] as const) {
      // A heartbeat while the step runs (CI shows a step's log only once it
      // ends) and a hard bound, so a hang names itself instead of the job
      // timing out.
      // LUMIA_E2E_ONLY names the steps to run (comma-separated), for one
      // step's turnaround while it is written; the rest are skipped.
      const only = (process.env.LUMIA_E2E_ONLY ?? "").split(",").map((s) => s.trim()).filter(Boolean);
      if (only.length && !only.includes(name)) continue;
      const started = Date.now();
      live.phase = "start";
      // Every 30 s: a line, and a screenshot of the page as it is then (a
      // step that closes its own browser in `finally` leaves nothing to
      // shoot once it has failed).
      const beat = setInterval(() => {
        const secs = Math.round((Date.now() - started) / 1000);
        console.log(`  [${which}] ${name}: ${secs} s, ${live.phase}, at ${live.page?.url() ?? "no page"}`);
        void live.page?.screenshot({ path: `${out}/${which}-${name.replace(/\s+/g, "-")}-${secs}s.png`, timeout: 5_000 }).catch(() => {});
      }, 30_000);
      try {
        await Promise.race([step(), Bun.sleep(240_000).then(() => Promise.reject(new Error(`${name} did not finish within 4 minutes`)))]);
      } catch (e) {
        failed++;
        // The whole message: a step's error may carry what it saw.
        const lines = (e instanceof Error ? e.message : String(e)).split("\n");
        console.log(`  ${which}: ${name} FAILED: ${lines[0]}`);
        for (const line of lines.slice(1, 40)) console.log(`      ${line}`);
        if (live.page) await diagnose(which, live.page, `${which}-${name.replace(/\s+/g, "-")}-failed`);
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
  oidcService.stop(true);
  // A browser that went away mid-step leaves calls that never settle;
  // they must not hold the process (and a CI job) open after the report.
  process.exit();
}
