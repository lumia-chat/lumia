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

// Runs a CLI call that does not end by itself (`dm tail`) until what it
// printed satisfies `done`, or for `ms` at most; returns what it printed.
async function follow(args: string[], done: (text: string) => boolean, ms: number): Promise<string> {
  const proc = Bun.spawn(args, { cwd: web, stdout: "pipe", stderr: "pipe" });
  const timer = setTimeout(() => proc.kill(), ms);
  const decoder = new TextDecoder();
  let text = "";
  try {
    for await (const chunk of proc.stdout) {
      text += decoder.decode(chunk, { stream: true });
      if (done(text)) break;
    }
  } finally {
    clearTimeout(timer);
    proc.kill();
    await proc.exited;
  }
  return text + (await new Response(proc.stderr).text());
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
  // Every page of the context is watched (and slowed), the ones a step
  // opens itself too.
  const launched = engine.launchPersistentContext(profile, options).then(async (ctx) => {
    ctx.on("page", (p) => watch(which, ctx, p));
    // How often the app's frame has left the document it was put in (`framesLeft`).
    await ctx.addInitScript(() => {
      const w = window as unknown as { lumiaFrameLeft: number };
      w.lumiaFrameLeft = 0;
      new MutationObserver((records) => {
        for (const r of records) for (const n of r.removedNodes) if (n instanceof Element && (n.matches(".lu-shell") || n.querySelector(".lu-shell"))) w.lumiaFrameLeft++;
      }).observe(document, { childList: true, subtree: true });
    });
    const page = await ctx.newPage();
    if (which === "chromium" && slow > 1) await throttle(ctx, page);
    return { ctx, page };
  });
  const { ctx, page } = await Promise.race([launched, Bun.sleep(90_000).then(() => Promise.reject(new Error(`${which} did not open a page within 90 s`)))]);
  console.log(`  [${which}] browser ${ctx.browser()?.version() ?? "?"} up${which === "chromium" && slow > 1 ? `, its CPU slowed ${slow} times` : ""}`);
  live.page = page;
  // WebKit's first navigation of a run on Linux CI takes well over the default 30 s.
  page.setDefaultNavigationTimeout(90_000);
  if (!keep) await clearStores(which, page, base);
  return { ctx, page };
}

// How many times the app's frame (`.lu-shell`) has been taken out of this document since
// it loaded. Once drawn it stays until the app is left: a frame that leaves and comes
// straight back (a query read as it suspends, `settled` in client/state.ts) takes the
// keyboard from whatever had it, and what is typed or pressed just then goes nowhere.
async function framesLeft(page: Page): Promise<number> {
  return page.evaluate(() => (window as unknown as { lumiaFrameLeft?: number }).lumiaFrameLeft ?? 0);
}

// What a page says that a step should show: what it threw, with where
// (the stack, not only the message), and what it logged as an error.
function watch(which: string, ctx: BrowserContext, page: Page) {
  page.on("pageerror", (e) => console.log(`  [${which}] page error: ${e.stack ?? e.message}`));
  page.on("crash", () => console.log(`  [${which}] the page crashed`));
  // A document loaded again in the middle of a step (a reload nobody asked for) names itself.
  page.on("load", () => console.log(`  [${which}] document loaded: ${page.url()}`));
  page.on("worker", (w) => w.on("close", () => console.log(`  [${which}] worker closed: ${w.url().split("/").pop()}`)));
  page.on("console", (m) => {
    if (m.type() === "error" || m.text().startsWith("[vite]")) console.log(`  [${which}] console: ${m.text()}`);
  });
  if (which === "chromium" && slow > 1) void throttle(ctx, page).catch(() => {});
}

// `LUMIA_E2E_SLOW=<rate>` slows Chromium's CPU that many times over on
// every page (CDP's Emulation.setCPUThrottlingRate), so that a slow
// runner's timing can be had on a fast machine and a race only the runner
// loses is lost here too. Chromium only: WebKit has no such switch.
const slow = Number(process.env.LUMIA_E2E_SLOW ?? 0);

async function throttle(ctx: BrowserContext, page: Page) {
  const cdp = await ctx.newCDPSession(page);
  await cdp.send("Emulation.setCPUThrottlingRate", { rate: slow });
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

// A browser with no identity signing in to an account with none is offered
// the recovery phrase first (#20); the steps that are about something else
// put it off, and land where they would have.
async function later(page: Page) {
  await page.getByRole("button", { name: "Set up later" }).click();
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
  await later(page);
  const composer = page.getByPlaceholder(/^Message #/);
  await composer.waitFor();
  const body = `hello from ${which} ${Date.now()}`;
  await composer.fill(body);
  await page.keyboard.press("Enter");
  await page.getByText(body).waitFor();
  const left = await framesLeft(page);
  if (left) throw new Error(`the app's frame left the document ${left} times after it was drawn`);
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
  // Signed out, a prompt can't be asked for (a link session is the
  // account's): the page says to sign in first, or link another way.
  await page.getByRole("button", { name: "Sign in", exact: true }).waitFor();
  await page.getByRole("button", { name: "Link another way" }).click();
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

// Linking by a prompt (identity spec 4.2.1), as far as the suite can take
// it: the CLI has no prompt to answer, so the other side is never there.
// Signed in by password to the account whose identity is the CLI's, the
// browser meets "Private messages need this browser", and "Link from a
// device you use" opens a session and waits, asking again without
// failing; Cancel leaves it, and "Link another way" gives it up for the
// copied request, with Back to a new one. The approval itself is checked
// by hand against a native device.
async function linkPrompt(which: string, handle: string) {
  const { ctx, page } = await open(which, "link-prompt");
  const t0 = Date.now();
  try {
    await trustCard(page);
    await page.getByLabel("Email or handle").fill(handle);
    await page.getByRole("textbox", { name: "Password" }).fill(password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await page.getByPlaceholder(/^Message #/).waitFor();
    phase("needs this browser");
    await goto(page, `${base}/private-messages`);
    await page.getByRole("heading", { name: "Private messages need this browser" }).waitFor();
    await page.getByRole("button", { name: "Link from a device you use" }).click();
    phase("waiting");
    const waiting = page.getByText("Open Lumia on a device that has your private messages. It will ask about this browser.");
    await waiting.waitFor();
    await page.getByText("Waiting for your other device…").waitFor();
    // Two turns of the timer: still waiting, nothing said wrong.
    await Bun.sleep(4_500);
    await waiting.waitFor();
    if ((await page.locator(".auth__error").count()) > 0) throw new Error(`waiting said: ${await page.locator(".auth__error").first().textContent()}`);
    await page.screenshot({ path: `${out}/${which}-link-waiting.png` });
    phase("cancel");
    await page.getByRole("button", { name: "Cancel" }).click();
    await page.waitForURL(/\/app/);
    phase("another way");
    await goto(page, `${base}/link`);
    await waiting.waitFor();
    await page.getByRole("button", { name: "Link another way" }).click();
    await page.getByRole("button", { name: "Start linking" }).click();
    await page.getByLabel("Link request").waitFor();
    await page.getByRole("button", { name: "Back" }).click();
    await waiting.waitFor();
  } finally {
    await ctx.close();
  }
  console.log(`  ${which}: asked to link by a prompt, waited, cancelled, and linked another way and back, in ${Date.now() - t0} ms`);
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
    await later(again);
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
  console.log(`  ${which}: personal DM both ways and back after reload, and its own message edited and deleted, in ${Date.now() - t0} ms`);
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
  // The CLI joined from the welcome. `dm open` returns as soon as the group
  // is ready, which can be before the browser's message was decrypted (its
  // gateway may even have stored it, which the next connection's welcome
  // catches up), and `dm messages` reads the CLI's store without syncing:
  // when the message is not there yet, follow the conversation until the
  // CLI prints it, then read again.
  phase("cli read");
  let seen = lumia(aDir, "dm", "messages", channel);
  let tailed = "";
  if (!seen.includes(hello)) {
    tailed = await follow([cli, "--data-dir", aDir, "dm", "tail", channel], (text) => text.includes(hello), 60_000);
    seen = lumia(aDir, "dm", "messages", channel);
  }
  if (!seen.includes(hello)) throw new Error(`the CLI did not read the browser's message; its open said:\n${opened.slice(-600)}\nits tail said:\n${tailed.slice(-600)}\nand it holds:\n${seen}`);
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
  // Its own message, edited where it is drawn and deleted at once with Shift (architecture 6.6, inside the encryption).
  phase("edit in the dm");
  const part = (text: string) => again.locator(".lu-msg__part", { has: again.locator(".lu-msg__body", { hasText: text }) });
  const edited = `${hello}, edited`;
  await part(hello).hover();
  await part(hello).getByRole("button", { name: "More actions" }).click();
  await again.getByRole("menuitem", { name: "Edit message" }).click();
  const field = again.getByRole("textbox", { name: "Edit message" });
  await field.fill(edited);
  await field.press("Enter");
  await part(edited).locator(".lu-msg__edited").waitFor();
  phase("delete in the dm");
  await part(edited).hover();
  await part(edited).getByRole("button", { name: "More actions" }).click();
  await again.getByRole("menuitem", { name: "Delete message" }).click({ modifiers: ["Shift"] });
  await part(edited).waitFor({ state: "detached" });
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
    // Sign-up's phrase step is the private step's to walk through; put off, it lands in the server.
    await later(page);
    phase("joined");
    await page.getByPlaceholder(/^Message #general/).waitFor();
    if ((await page.getByText("Web suite").count()) === 0) throw new Error("the joined server is not shown");
    await page.screenshot({ path: `${out}/${which}-joined.png` });
  } finally {
    await ctx.close();
  }
  console.log(`  ${which}: opened an invite, created an account from it and landed in the server in ${Date.now() - t0} ms`);
}

// Private messages in a browser (#20). Sign-up's steps 2 and 3: the phrase
// (printing shows only it and its warning; saving asks first, then the file
// holds the words), a wrong check that shows the phrase again, and a right
// one, a word of it taken from the wordlist's completion, which sets up
// this browser as the identity's device. Then an account whose identity is
// on the CLI: signing in asks nothing, Home says private messages need
// this browser, restore is the flagship's, and starting over (the
// password, a new phrase and its check) makes this browser its device.
async function privateStep(which: string, stamp: number) {
  const words = async (page: Page) => {
    await page.locator(".lu-phrase__text").first().waitFor();
    const list = await page.locator(".lu-phrase__text").allTextContents();
    if (list.length !== 24) throw new Error(`the phrase shows ${list.length} words`);
    return list;
  };
  const check = async (page: Page, phrase: string[], right: boolean) => {
    await page.getByRole("button", { name: "I’ve saved them — check three words" }).click();
    await page.getByLabel(/^Word \d+$/).first().waitFor();
    const positions = (await page.locator(".lu-wordfield .lu-textfield__label").allTextContents()).map((t) => Number(t.replace(/\D/g, "")));
    if (positions.length !== 3 || positions.some((p) => !(p >= 1 && p <= 24))) throw new Error(`the check asks for ${positions.join(", ")}`);
    let completed = !right;
    for (const p of positions) {
      const field = page.getByLabel(`Word ${p}`, { exact: true });
      const word = phrase[p - 1] ?? "";
      if (!right) await field.fill("notaword");
      else if (!completed && word.length >= 5) {
        // Four letters name one word of the list; it is offered, and taken.
        await field.fill(word.slice(0, 4));
        await page.getByRole("button", { name: word, exact: true }).click();
        if ((await field.inputValue()) !== word) throw new Error(`the completion left ${await field.inputValue()}`);
        completed = true;
      } else await field.fill(word);
    }
    await page.getByRole("button", { name: "Confirm", exact: true }).click();
  };
  {
    const { ctx, page } = await open(which, "private");
    const t0 = Date.now();
    try {
      phase("sign up");
      await trustCard(page);
      await page.getByRole("button", { name: "Create an account" }).click();
      await page.getByLabel("Handle").fill(`phrase${which}${stamp}`);
      await page.getByRole("textbox", { name: "Password" }).fill(password);
      await page.getByText("Next, you’ll save a recovery phrase.").waitFor();
      await page.getByRole("button", { name: "Create account", exact: true }).click();
      phase("the phrase");
      await page.getByText("Create your account · Step 2 of 3").waitFor();
      const phrase = await words(page);
      await page.screenshot({ path: `${out}/${which}-phrase.png` });
      await page.emulateMedia({ media: "print" });
      const printed = await page.evaluate(() => ({
        words: document.querySelectorAll(".lu-phrase__word").length > 0 && [...document.querySelectorAll(".lu-phrase__word")].every((w) => w.getClientRects().length > 0),
        warning: (document.querySelector(".lu-card .lu-notice--caution")?.getClientRects().length ?? 0) > 0,
        others: [...document.querySelectorAll(".private__phrase button, .private__phrase h1")].filter((e) => e.getClientRects().length > 0).map((e) => e.textContent),
      }));
      await page.emulateMedia({ media: "screen" });
      if (!printed.words || !printed.warning || printed.others.length) throw new Error(`printing shows the words: ${printed.words}, the warning: ${printed.warning}, and also: ${printed.others.join(" | ")}`);
      phase("save");
      await page.getByRole("button", { name: "Save as a file" }).click();
      await page.getByRole("alertdialog", { name: "This file is your identity" }).waitFor();
      const [download] = await Promise.all([page.waitForEvent("download"), page.getByRole("button", { name: "Save anyway" }).click()]);
      const file = await Bun.file(await download.path()).text();
      const missing = phrase.filter((w, i) => !file.includes(`${String(i + 1).padStart(2, " ")}  ${w}`));
      if (missing.length) throw new Error(`the saved file lacks ${missing.join(", ")}`);
      phase("wrong");
      await check(page, phrase, false);
      await page.getByText("Those words didn’t match.").waitFor();
      if ((await words(page)).join(" ") !== phrase.join(" ")) throw new Error("a wrong check showed another phrase");
      phase("right");
      await check(page, phrase, true);
      await page.waitForURL(/\/app/);
      await goto(page, `${base}/app?place=home&view=devices`);
      await page.locator(".lu-device.is-this", { hasText: "This browser" }).waitFor();
      await page.screenshot({ path: `${out}/${which}-phrase-devices.png` });
      // A browser keeps its identity when its account signs out, so the
      // next account signed in here meets one it is not bound to. Binding
      // it is refused, and Try again tries the binding again: a new phrase
      // shown there would restore nothing, since the identity held is the
      // one a binding uses.
      phase("another account");
      const other = resolve(work, `cli-private-other-${which}`);
      const otherHandle = `other${which}${stamp}`;
      lumia(other, "instance", "add", instance);
      lumia(other, "account", "create", "--handle", otherHandle, "--password", password);
      await page.getByRole("button", { name: /account menu/ }).click();
      await page.getByRole("menuitem", { name: "Sign out" }).click();
      await page.waitForURL(/\/sign-in/);
      await trustCard(page);
      await page.getByLabel("Email or handle").fill(otherHandle);
      await page.getByRole("textbox", { name: "Password" }).fill(password);
      await page.getByRole("button", { name: "Sign in", exact: true }).click();
      await page.getByRole("button", { name: "Create, join or discover servers" }).waitFor();
      await goto(page, `${base}/app?place=home`);
      await page.getByRole("button", { name: "Finish setting up" }).click();
      // The reload let the sign-in's reauth go.
      await page.getByLabel("Password", { exact: true }).fill(password);
      await page.getByRole("button", { name: "Continue", exact: true }).click();
      await page.getByText("The identity made in this browser is another account’s on this instance.").waitFor();
      await page.evaluate(() => {
        const titles: string[] = [];
        (window as unknown as { titles: string[] }).titles = titles;
        new MutationObserver(() => {
          const title = document.querySelector(".lu-card__title")?.textContent ?? "";
          if (title && titles.at(-1) !== title) titles.push(title);
        }).observe(document.body, { subtree: true, childList: true, characterData: true });
      });
      // The refused binding spent the reauth, so trying again asks for it.
      await page.getByRole("button", { name: "Try again" }).click();
      await page.getByLabel("Password", { exact: true }).fill(password);
      await page.getByRole("button", { name: "Continue", exact: true }).click();
      await page.getByText("The identity made in this browser is another account’s on this instance.").waitFor();
      const titles = await page.evaluate(() => (window as unknown as { titles: string[] }).titles);
      if (titles.includes("Save your recovery phrase")) throw new Error(`Try again showed a new phrase for the identity this browser holds: ${titles.join(" → ")}`);
    } finally {
      await ctx.close();
    }
    console.log(`  ${which}: signed up through the phrase (printed alone, saved behind its warning), was shown it again after a wrong check, and a right one set up this browser; another account signed in here was refused the identity, and Try again asked again without a new phrase, in ${Date.now() - t0} ms`);
  }
  {
    const dir = resolve(work, `cli-private-${which}`);
    const handle = `over${which}${stamp}`;
    lumia(dir, "instance", "add", instance);
    lumia(dir, "account", "create", "--handle", handle, "--password", password);
    lumia(dir, "identity", "create", "--label", "lost phone");
    lumia(dir, "identity", "link-account", "--password", password);
    const { ctx, page } = await open(which, "private-over");
    const t0 = Date.now();
    try {
      phase("sign in");
      await trustCard(page);
      await page.getByLabel("Email or handle").fill(handle);
      await page.getByRole("textbox", { name: "Password" }).fill(password);
      await page.getByRole("button", { name: "Sign in", exact: true }).click();
      await page.getByRole("button", { name: "Create, join or discover servers" }).waitFor();
      phase("needs this browser");
      await goto(page, `${base}/app?place=home`);
      await page.getByText("Private messages need this browser").waitFor();
      await page.getByRole("button", { name: "Restore with your recovery phrase" }).click();
      await page.getByText("A browser restores your identity only on my.lumia.chat.").waitFor();
      await page.goBack();
      phase("start over");
      await page.getByRole("button", { name: "Start over", exact: true }).click();
      await page.getByText("Private messages from before stay unreadable.").waitFor();
      await page.getByLabel("Confirm with your password").fill(password);
      await page.getByRole("button", { name: "Start over with a new identity" }).click();
      await page.getByText("Start over · Step 1 of 2").waitFor();
      await check(page, await words(page), true);
      await page.waitForURL(/place=home/);
      await page.getByText("Private messages need this browser").waitFor({ state: "detached" });
      await goto(page, `${base}/app?place=home&view=devices`);
      await page.locator(".lu-device.is-this", { hasText: "This browser" }).waitFor();
    } finally {
      await ctx.close();
    }
    console.log(`  ${which}: an account whose identity was elsewhere was told private messages need this browser, offered restore only on the flagship, and started over with the password and a new phrase, in ${Date.now() - t0} ms`);
  }
  {
    const dir = resolve(work, `cli-private-later-${which}`);
    const handle = `later${which}${stamp}`;
    lumia(dir, "instance", "add", instance);
    lumia(dir, "account", "create", "--handle", handle, "--password", password);
    const { ctx, page } = await open(which, "private-later");
    const t0 = Date.now();
    try {
      phase("put off");
      await trustCard(page);
      await page.getByLabel("Email or handle").fill(handle);
      await page.getByRole("textbox", { name: "Password" }).fill(password);
      await page.getByRole("button", { name: "Sign in", exact: true }).click();
      await page.getByText("Set up private messages · Step 1 of 2").waitFor();
      await later(page);
      // A reload lets the sign-in's reauth go: setting up from Devices asks for one first.
      phase("from devices");
      await goto(page, `${base}/app?place=home&view=devices`);
      await page.getByRole("button", { name: "Set up private messages" }).click();
      await page.getByText("Confirm it’s you").waitFor();
      await page.getByLabel("Password", { exact: true }).fill(password);
      await page.getByRole("button", { name: "Continue", exact: true }).click();
      await check(page, await words(page), true);
      await page.waitForURL(/view=devices/);
      await page.locator(".lu-device.is-this", { hasText: "This browser" }).waitFor();
    } finally {
      await ctx.close();
    }
    console.log(`  ${which}: put the phrase off at sign-in, then set up from Devices after confirming it with the password, in ${Date.now() - t0} ms`);
  }
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
    await later(page);
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
// still there after a reload. The page's preview draws its sample
// messages as each choice says (grouped or one line, the avatar's size,
// the spacing, the font size, the picture or its file row, the card, the
// reactions), and each switch's caption says what its state does. An
// account of its own with three quick messages, so the channel is one
// block of known size, the second with a reaction, which Minimal does not
// draw.
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
    await later(page);
    await page.getByText(`Three of ${stamp}`).waitFor();
    const part = (line: string) => page.locator(".lu-msglist .lu-msg__part", { has: page.locator(".lu-msg__body", { hasText: `${line} of ${stamp}` }) });
    await part("Two").hover();
    await part("Two").getByRole("toolbar", { name: "Message actions" }).getByRole("button", { name: "Add reaction" }).click();
    const picker = page.getByRole("dialog", { name: "Emoji" });
    await picker.getByLabel("Search emoji").fill("thumbs up");
    await picker.getByRole("button", { name: "thumbs up", exact: true }).first().click();
    await part("Two").getByRole("button", { name: "You reacted with 👍" }).waitFor();
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
    const appearance = page.getByRole("radiogroup", { name: "Appearance" });
    await appearance.getByRole("radio", { name: "Dark" }).click();
    if ((await ground()) !== DARK) throw new Error(`Dark under a light system draws the ground ${await ground()}`);
    if ((await appearance.getByRole("radio", { name: "Dark" }).getAttribute("aria-checked")) !== "true") throw new Error("Dark chosen is not the tile checked");
    await page.screenshot({ path: `${out}/${which}-preferences-dark.png` });
    await appearance.getByRole("radio", { name: "Light" }).click();
    await page.emulateMedia({ colorScheme: "dark" });
    if ((await ground()) !== LIGHT) throw new Error(`Light under a dark system draws the ground ${await ground()}`);
    await appearance.getByRole("radio", { name: "System" }).click();
    if ((await ground()) !== DARK) throw new Error(`System under a dark system draws the ground ${await ground()}`);
    await page.emulateMedia({ colorScheme: "light" });
    if ((await ground()) !== LIGHT) throw new Error(`System under a light system draws the ground ${await ground()}`);
    phase("the preview");
    // The preview is one picture to assistive technology, drawn by the conversation's rows; nothing in it is pressable.
    await page.getByRole("img", { name: "Preview of messages with these settings" }).waitFor();
    const drawn = () =>
      page.evaluate(() => {
        const p = document.querySelector(".lu-preview");
        const n = (s: string) => p?.querySelectorAll(s).length ?? 0;
        return {
          lines: n(".lu-msg--line .lu-msg__part"),
          blocks: n(".lu-msg:not(.lu-msg--line)"),
          avatar: p?.querySelector(".lu-avatar")?.getBoundingClientRect().width ?? 0,
          picture: n(".lu-preview__shot"),
          file: n(".lu-attach"),
          card: n(".lu-linkcard"),
          reactions: n(".lu-reaction"),
          spacing: p?.querySelector(".lu-msglist")?.className.match(/lu-msglist--(cozy|tight)/)?.[1] ?? "roomy",
          size: getComputedStyle(p?.querySelector(".lu-body") ?? document.body).fontSize,
          inert: !!p?.querySelector("[inert] .lu-msglist"),
        };
      });
    const expectDrawn = async (what: string, want: Partial<Awaited<ReturnType<typeof drawn>>>) => {
      const got = await drawn();
      for (const [k, v] of Object.entries(want)) if (got[k as keyof typeof got] !== v) throw new Error(`the preview ${what}: ${k} is ${JSON.stringify(got[k as keyof typeof got])}, not ${JSON.stringify(v)} (${JSON.stringify(got)})`);
    };
    await expectDrawn("as Comfortable", { lines: 0, blocks: 3, avatar: 40, picture: 1, file: 0, card: 1, reactions: 2, spacing: "roomy", size: "15px", inert: true });
    phase("compact");
    await appearance.getByRole("radio", { name: "Dark" }).click();
    const preset = page.getByRole("radiogroup", { name: "Preset" });
    if ((await preset.getByRole("radio", { name: "Comfortable" }).getAttribute("aria-checked")) !== "true") throw new Error("the preset at first is not Comfortable");
    await preset.getByRole("radio", { name: "Compact" }).click();
    if ((await preset.getByRole("radio", { name: "Compact" }).getAttribute("aria-checked")) !== "true") throw new Error("Compact chosen is not the card lit");
    if ((await page.getByRole("radiogroup", { name: "Avatars" }).getByRole("radio", { name: "Small" }).getAttribute("aria-checked")) !== "true") throw new Error("Compact did not choose small avatars");
    if ((await page.getByRole("radiogroup", { name: "Spacing" }).getByRole("radio", { name: "Cozy" }).getAttribute("aria-checked")) !== "true") throw new Error("Compact did not choose Cozy");
    await expectDrawn("as Compact", { blocks: 3, avatar: 28, spacing: "cozy" });
    // A load from nothing: what is drawn now is what the browser kept.
    await goto(page, `${base}/app`);
    await page.getByText(`Three of ${stamp}`).waitFor();
    if ((await ground()) !== DARK) throw new Error(`after a reload the ground is ${await ground()}: Dark was not kept`);
    await part("Two").getByRole("button", { name: "You reacted with 👍" }).waitFor();
    const compact = await block();
    if (compact.avatar !== 28) throw new Error(`a compact message's avatar is ${compact.avatar} wide`);
    if (compact.height >= comfortable.height) throw new Error(`a compact block is ${compact.height} tall and a comfortable one ${comfortable.height}`);
    await page.screenshot({ path: `${out}/${which}-compact.png` });
    phase("minimal");
    await page.getByRole("button", { name: /account menu/ }).click();
    await page.getByRole("menuitem", { name: "Preferences" }).click();
    await preset.getByRole("radio", { name: "Minimal" }).click();
    if (await page.getByRole("radiogroup", { name: "Avatars" }).isVisible()) throw new Error("one line still offers avatars");
    await expectDrawn("as Minimal", { lines: 4, blocks: 0, avatar: 0, picture: 0, file: 1, card: 0, reactions: 0, spacing: "tight" });
    await page.getByRole("radiogroup", { name: "Font size" }).getByRole("radio", { name: "20 points" }).click();
    await expectDrawn("at 20", { size: "20px" });
    // Font size is no part of a preset: still Minimal.
    if ((await preset.getByRole("radio", { name: "Minimal" }).getAttribute("aria-checked")) !== "true") throw new Error("a font size undid the preset");
    await page.getByRole("radiogroup", { name: "Spacing" }).getByRole("radio", { name: "Roomy" }).click();
    await page.getByText("Custom: these settings match no preset.").waitFor();
    if ((await preset.locator('[aria-checked="true"]').count()) !== 0) throw new Error("Custom still lights a preset");
    await expectDrawn("at Roomy", { spacing: "roomy", lines: 4 });
    await page.getByRole("radiogroup", { name: "Spacing" }).getByRole("radio", { name: "Tight" }).click();
    if ((await preset.getByRole("radio", { name: "Minimal" }).getAttribute("aria-checked")) !== "true") throw new Error("Minimal's values again are not Minimal");
    phase("the switches");
    // Each switch: its caption says what its state does, the preview follows, and a change from Minimal is Custom.
    const toggle = async (name: string, on: boolean, caption: string, want: Partial<Awaited<ReturnType<typeof drawn>>>) => {
      const sw = page.getByRole("switch", { name });
      if ((await sw.getAttribute("aria-checked")) === String(on)) throw new Error(`${name} is already ${on ? "on" : "off"}`);
      await sw.click();
      if ((await sw.getAttribute("aria-checked")) !== String(on)) throw new Error(`${name} did not turn ${on ? "on" : "off"}`);
      await page.getByText(caption, { exact: true }).waitFor();
      await expectDrawn(`with ${name} ${on ? "on" : "off"}`, want);
    };
    const reactionsOff = "Hidden. You can still add one from a message’s toolbar.";
    await page.getByText(reactionsOff, { exact: true }).waitFor();
    await page.getByText("As links: each one a file row that opens the viewer.", { exact: true }).waitFor();
    await page.getByText("As links: no cards.", { exact: true }).waitFor();
    await toggle("Reactions", true, "Shown under each message.", { reactions: 2 });
    await page.getByText("Custom: these settings match no preset.").waitFor();
    await toggle("Images", true, "Shown in their messages.", { picture: 1, file: 0 });
    await toggle("Embeds", true, "A card under each link.", { card: 1 });
    await toggle("Embeds", false, "As links: no cards.", { card: 0 });
    await toggle("Images", false, "As links: each one a file row that opens the viewer.", { picture: 0, file: 1 });
    await toggle("Reactions", false, reactionsOff, { reactions: 0 });
    if ((await preset.getByRole("radio", { name: "Minimal" }).getAttribute("aria-checked")) !== "true") throw new Error("reactions hidden again are not Minimal");
    if (await page.getByText("Custom: these settings match no preset.").isVisible()) throw new Error("Minimal again still says Custom");
    phase("layout");
    const layout = page.getByRole("radiogroup", { name: "Layout" });
    await layout.getByRole("radio", { name: "Grouped" }).click();
    await page.getByRole("radiogroup", { name: "Avatars" }).waitFor();
    await expectDrawn("grouped", { lines: 0, blocks: 3 });
    await page.screenshot({ path: `${out}/${which}-preferences-custom.png` });
    await layout.getByRole("radio", { name: "One line" }).click();
    if (await page.getByRole("radiogroup", { name: "Avatars" }).isVisible()) throw new Error("one line offers avatars again");
    await expectDrawn("one line again", { lines: 4, blocks: 0 });
    if ((await preset.getByRole("radio", { name: "Minimal" }).getAttribute("aria-checked")) !== "true") throw new Error("one line again is not Minimal");
    await goto(page, `${base}/app`);
    await page.getByText(`Three of ${stamp}`).waitFor();
    const lines = await page.evaluate(() =>
      Array.from(document.querySelectorAll(".lu-msglist .lu-msg--line .lu-msg__part")).map((line) => {
        const box = (sel: string) => line.querySelector(sel)?.getBoundingClientRect();
        const text = line.querySelector(".lu-line__text p");
        const t = text?.getBoundingClientRect();
        // The text's first line box, which the time and the name sit on.
        const first = t && text ? { top: t.top, bottom: t.top + parseFloat(getComputedStyle(text).lineHeight) } : undefined;
        const within = (b?: DOMRect) => !!b && !!first && b.top >= first.top - 2 && b.bottom <= first.bottom + 2;
        return { time: within(box(".lu-line__time")), name: within(box(".lu-line__name")), size: text ? getComputedStyle(text).fontSize : "", avatars: document.querySelectorAll(".lu-msglist .lu-avatar").length };
      }),
    );
    if (lines.length !== 3) throw new Error(`Minimal drew ${lines.length} lines for three messages`);
    for (const l of lines) {
      if (l.avatars) throw new Error("one line drew an avatar");
      if (!l.time || !l.name) throw new Error(`a line's time and name are not on its text's first line: ${JSON.stringify(l)}`);
      if (l.size !== "20px") throw new Error(`the text is ${l.size} at font size 20`);
    }
    // Hidden reactions: no chips, no add chip, no room kept for them, and Add reaction still in the toolbar.
    if ((await page.locator(".lu-msglist .lu-reaction, .lu-msglist .lu-reaction-add").count()) !== 0) throw new Error("Minimal drew a reaction");
    const [two, three] = [(await part("Two").boundingBox())?.height, (await part("Three").boundingBox())?.height];
    if (!two || !three || Math.abs(two - three) > 0.5) throw new Error(`the line with a hidden reaction is ${two} tall and the one without ${three}`);
    await part("Two").hover();
    await part("Two").getByRole("toolbar", { name: "Message actions" }).getByRole("button", { name: "Add reaction" }).waitFor();
    await page.screenshot({ path: `${out}/${which}-minimal.png` });
    phase("kept through sign-out");
    await page.getByRole("button", { name: /account menu/ }).click();
    await page.getByRole("menuitem", { name: "Sign out" }).click();
    await page.waitForURL(/\/sign-in/);
    if ((await ground()) !== DARK) throw new Error(`signed out, the ground is ${await ground()}: the appearance went with the account`);
  } finally {
    await ctx.close();
  }
  console.log(`  ${which}: opened Preferences from the account menu, chose each appearance against the system's, followed each choice and switch in the preview, drew a block compact with its reaction and then as Minimal's lines at 20 with none, Custom between, and kept them through a reload and a sign-out, in ${Date.now() - t0} ms`);
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
    await later(page);
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
      phase("show deleted");
      // Off for a new server; saved as it is turned (architecture 6.7).
      const showDeleted = history.getByRole("checkbox", { name: "Show deleted messages" });
      if (await showDeleted.isChecked()) throw new Error("Show deleted messages is on for a new server");
      await showDeleted.check();
      // Disabled while it is saved; on again and still ticked once the instance has it.
      for (let i = 0; i < 100 && ((await showDeleted.isDisabled()) || !(await showDeleted.isChecked())); i++) await Bun.sleep(100);
      if (!(await showDeleted.isChecked())) throw new Error("Show deleted messages did not stay on");
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
  // The instance has all four, as a member asks for the server.
  const json = { "content-type": "application/json", accept: "application/json" };
  const pair = (await (await fetch(`${instance}/api/v1/auth/sign-in/password`, { method: "POST", headers: json, body: JSON.stringify({ handleOrEmail: member, password }) })).json()) as { accessToken?: string };
  if (!pair.accessToken) throw new Error("the suite could not sign in as the member");
  const held = (await (await fetch(`${instance}/api/v1/servers/${server}`, { headers: { ...json, authorization: `Bearer ${pair.accessToken}` } })).json()) as { name?: string; retentionDays?: number; iconUrl?: string; showDeleted?: boolean };
  if (held.name !== `Renamed ${stamp}` || held.retentionDays !== 30 || !held.iconUrl || held.showDeleted !== true) throw new Error(`the instance holds ${JSON.stringify(held)}`);
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

// Roles, titles and tags (architecture 7.5, plan step 11), the owner in one
// browser and a member in another: the settings' sections; a role made,
// given to the member from its editor, and the tag beside the member's
// name in both browsers; a role that is nobody's tag; a role moved by its
// menu and by its handle; Administrator asked first; a name another role
// has refused; a title set in Members and, once the server shows titles,
// drawn as the tag in both browsers with no reload; the member's own title
// from their card; a role deleted after its question; and a kick. The
// member's browser is Chromium's alone: two of Playwright's WebKits open at
// once share one origin-private file system, and the second cannot open
// its store ("file is not a database"). There the member's own title is
// set through the instance instead, and the owner's page still has to
// draw it with no reload.
async function rolesStep(which: string, stamp: number) {
  const ownerDir = resolve(work, `cli-roles-owner-${which}`);
  const memberDir = resolve(work, `cli-roles-member-${which}`);
  const owner = `chief${which}${stamp}`;
  const member = `crew${which}${stamp}`;
  lumia(ownerDir, "instance", "add", instance);
  lumia(ownerDir, "account", "create", "--handle", owner, "--password", password);
  const server = lumia(ownerDir, "server-create", `Roles ${stamp}`).match(/[0-9a-f-]{36}/)?.[0];
  if (!server) throw new Error("no server id");
  const general = lumia(ownerDir, "channel-create", server, "general").match(/[0-9a-f-]{36}/)?.[0];
  if (!general) throw new Error("no channel id");
  const code = lumia(ownerDir, "invite-create", server).trim().split(/\s+/)[0];
  if (!code) throw new Error("no invite code");
  lumia(memberDir, "instance", "add", instance);
  lumia(memberDir, "account", "create", "--handle", member, "--password", password, "--invite", code);
  lumia(memberDir, "send", general, "Reporting for duty.");
  const signIn = async (page: Page, handle: string) => {
    await trustCard(page);
    await page.getByLabel("Email or handle").fill(handle);
    await page.getByRole("textbox", { name: "Password" }).fill(password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await later(page);
    await page.getByPlaceholder("Message #general").waitFor();
  };
  const t0 = Date.now();
  const mine = await open(which, "roles-owner");
  const theirs = which === "chromium" ? await open(which, "roles-member") : null;
  const page = mine.page;
  live.page = page;
  try {
    await signIn(page, owner);
    if (theirs) {
      // A sign-in that fails is the member's to be looked at.
      live.page = theirs.page;
      await signIn(theirs.page, member);
      live.page = page;
    }
    const sections = page.getByRole("navigation", { name: "Server settings" });
    const section = (name: string) => sections.getByRole("button", { name });
    const toSettings = async (name: string) => {
      if ((await sections.count()) === 0) {
        await page.getByRole("button", { name: /server menu/ }).click();
        await page.getByRole("menuitem", { name: "Server settings" }).click();
      }
      await section(name).click();
    };
    const toGeneral = async () => {
      await page.getByRole("link", { name: "general" }).click();
      await page.getByPlaceholder("Message #general").waitFor();
    };
    const order = () => page.locator(".lu-role__name").allTextContents();
    const ordered = async (wanted: string[]) => {
      for (let i = 0; i < 100 && (await order()).join() !== wanted.join(); i++) await Bun.sleep(100);
      if ((await order()).join() !== wanted.join()) throw new Error(`the roles read ${(await order()).join(", ")}, not ${wanted.join(", ")}`);
    };
    const roleRow = (name: string) => page.locator(".lu-role", { has: page.locator(".lu-role__name", { hasText: new RegExp(`^${name}$`) }) });
    // The member's message as each browser draws it, and the tag beside its author's name.
    const tagIn = (p: Page) => p.locator(".lu-msg", { hasText: "Reporting for duty." }).locator(".lu-tag");

    phase("the sections");
    await page.getByRole("button", { name: /server menu/ }).click();
    await page.getByRole("menuitem", { name: "Server settings" }).click();
    for (const name of ["Overview", "Roles", "Members"]) await section(name).waitFor();
    // Their board has no member list beside it: Members is a section.
    if ((await page.locator(".lu-shell__aside .lu-member").count()) > 0) throw new Error("the member list is drawn beside the settings");

    phase("a role");
    await section("Roles").click();
    await ordered(["Everyone"]);
    await page.getByRole("button", { name: "New role" }).click();
    const save = page.getByRole("button", { name: "Save", exact: true });
    if (!(await save.isDisabled())) throw new Error("a role with no name can be saved");
    await page.getByLabel("Name").fill("Crew");
    await page.getByRole("radio", { name: /^sky$/i }).click();
    await save.click();
    // Made, it is the role being edited: who holds it can be added.
    await page.getByRole("heading", { name: /Members with this role/ }).waitFor();
    await page.getByText("Nobody holds this role yet.").waitFor();
    await page.getByRole("button", { name: "Add members" }).click();
    await page.getByRole("checkbox", { name: new RegExp(member) }).click();
    await page.getByRole("button", { name: "Add", exact: true }).click();
    await page.locator(".lu-roleedit__holders").getByText(new RegExp(member)).first().waitFor();
    await page.getByRole("button", { name: "Back to roles" }).click();
    await roleRow("Crew").getByText("1 member", { exact: true }).waitFor();
    await roleRow("Everyone").getByText("2 members", { exact: true }).waitFor();

    phase("the tag");
    if (theirs) await tagIn(theirs.page).filter({ hasText: "Crew" }).waitFor({ timeout: 20_000 });
    await toGeneral();
    await tagIn(page).filter({ hasText: "Crew" }).waitFor();
    if (!(await tagIn(page).getAttribute("class"))?.includes("lu-tone-sky")) throw new Error("the tag is not on its role's tone");
    await page.locator(".lu-shell__aside .lu-member", { hasText: member }).locator(".lu-tag", { hasText: "Crew" }).waitFor();
    // A member who manages nothing is offered no settings.
    if (theirs) {
      await theirs.page.getByRole("button", { name: /server menu/ }).click();
      await theirs.page.getByRole("menuitem", { name: "Invite people" }).waitFor();
      if ((await theirs.page.getByRole("menuitem", { name: "Server settings" }).count()) !== 0) throw new Error("a member is offered the settings");
      await theirs.page.keyboard.press("Escape");
    }

    phase("nobody's tag");
    await toSettings("Roles");
    await page.getByRole("button", { name: "New role" }).click();
    await page.getByLabel("Name").fill("Helpers");
    await page.getByRole("switch", { name: "Show as tag" }).click();
    await page.getByText("This role only carries permissions. It is nobody’s tag.").waitFor();
    await save.click();
    await page.getByRole("heading", { name: /Members with this role/ }).waitFor();
    await page.getByRole("button", { name: "Back to roles" }).click();
    await roleRow("Helpers").getByText("Not shown as tag").waitFor();
    await ordered(["Crew", "Helpers", "Everyone"]);

    phase("moved");
    await roleRow("Helpers").locator(".lu-role__open").click({ button: "right" });
    if (!(await page.getByRole("menuitem", { name: "Move down" }).isDisabled())) throw new Error("the last role above Everyone is offered Move down");
    await page.getByRole("menuitem", { name: "Move up" }).click();
    await ordered(["Helpers", "Crew", "Everyone"]);
    // And by its handle: Crew dragged above Helpers, a line where it would land while it is held.
    const from = await roleRow("Crew").locator(".lu-role__handle").boundingBox();
    const onto = await roleRow("Helpers").boundingBox();
    if (!from || !onto) throw new Error("no rows to drag between");
    await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2);
    await page.mouse.down();
    await page.mouse.move(from.x + from.width / 2, from.y, { steps: 4 });
    await page.mouse.move(onto.x + 40, onto.y + 6, { steps: 6 });
    await page.locator(".lu-roles__line").waitFor();
    await page.locator(".lu-roles__ghost").waitFor();
    await page.mouse.up();
    await ordered(["Crew", "Helpers", "Everyone"]);
    // As the instance has it, read again.
    await section("Members").click();
    await section("Roles").click();
    await ordered(["Crew", "Helpers", "Everyone"]);

    phase("asked first");
    await roleRow("Helpers").locator(".lu-role__open").click();
    const administrator = page.getByRole("switch", { name: "Administrator" });
    await administrator.click();
    const question = page.getByRole("dialog", { name: "Turn on Administrator?" });
    await question.getByText(`Administrator allows everything in Roles ${stamp}, in every channel, whatever a channel’s own permissions say.`).waitFor();
    await question.getByRole("button", { name: "Cancel" }).click();
    if ((await administrator.getAttribute("aria-checked")) !== "false") throw new Error("Administrator is on after Cancel");
    await administrator.click();
    await question.getByRole("button", { name: "Turn on" }).click();
    if ((await administrator.getAttribute("aria-checked")) !== "true") throw new Error("Administrator is off after Turn on");
    await save.click();
    await page.getByText("Saved.").waitFor();

    phase("a name taken");
    await page.getByLabel("Name").fill("crew");
    await save.click();
    await page.getByText(`Another role in Roles ${stamp} has that name.`).waitFor();
    await page.getByRole("button", { name: "Back to roles" }).click();

    phase("a title");
    await section("Members").click();
    const row = page.locator(".lu-roster__item", { hasText: member });
    await row.locator(".lu-roster__row").click();
    if ((await row.getByRole("checkbox", { name: "Crew" }).getAttribute("aria-checked")) !== "true") throw new Error("the member's role is not ticked");
    await row.getByRole("textbox", { name: "Title" }).fill("Navigator");
    await row.getByRole("button", { name: "Save", exact: true }).click();
    await row.getByText("· Navigator").waitFor();
    await page.locator(".lu-roster__item", { hasText: owner }).locator(".lu-tag", { hasText: "Owner" }).waitFor();

    phase("titles as tags");
    await section("Overview").click();
    await page.getByRole("radio", { name: /^Titles/ }).click();
    // In the member's browser with no reload: the title, on the plain chip.
    if (theirs) await tagIn(theirs.page).filter({ hasText: "Navigator" }).waitFor({ timeout: 20_000 });
    await toGeneral();
    await tagIn(page).filter({ hasText: "Navigator" }).waitFor();
    if (!(await tagIn(page).getAttribute("class"))?.includes("lu-tag--neutral")) throw new Error("a title's tag is not the plain chip");

    phase("their own title");
    if (theirs) {
      await theirs.page.locator(".lu-msg", { hasText: "Reporting for duty." }).getByRole("button", { name: new RegExp(member) }).first().click();
      const card = theirs.page.locator(".lu-pcard");
      await card.getByText("Navigator").waitFor();
      await card.locator(".lu-tag", { hasText: "Crew" }).waitFor();
      // Their roles are not theirs to change.
      if ((await card.getByRole("button", { name: "Edit roles" }).count()) !== 0) throw new Error("a member is offered their own roles");
      await card.getByRole("button", { name: "Edit title" }).click();
      await theirs.page.getByRole("textbox", { name: "Title" }).fill("Pilot");
      await theirs.page.getByRole("button", { name: "Save", exact: true }).click();
      await theirs.page.locator(".lu-pcard").getByText("Pilot").waitFor();
      await theirs.page.keyboard.press("Escape");
    } else {
      const json = { "content-type": "application/json", accept: "application/json" };
      const pair = (await (await fetch(`${instance}/api/v1/auth/sign-in/password`, { method: "POST", headers: json, body: JSON.stringify({ handleOrEmail: member, password }) })).json()) as { accessToken?: string };
      if (!pair.accessToken) throw new Error("the suite could not sign in as the member");
      const set = await fetch(`${instance}/api/v1/servers/${server}/membership`, { method: "PATCH", headers: { ...json, authorization: `Bearer ${pair.accessToken}` }, body: JSON.stringify({ title: "Pilot" }) });
      if (!set.ok) throw new Error(`the member's title was not set: ${set.status} ${await set.text()}`);
    }
    // In the owner's browser with no reload, whoever set it and wherever.
    await tagIn(page).filter({ hasText: "Pilot" }).waitFor({ timeout: 20_000 });
    await page.screenshot({ path: `${out}/${which}-roles-titles.png` });

    phase("deleted");
    await toSettings("Roles");
    await roleRow("Helpers").locator(".lu-role__open").click();
    await page.getByRole("button", { name: "Delete role" }).click();
    const asked = page.getByRole("dialog", { name: "Delete Helpers?" });
    await asked.getByText("Nobody holds this role.").waitFor();
    await asked.getByRole("button", { name: "Delete role" }).click();
    await ordered(["Crew", "Everyone"]);
    await page.screenshot({ path: `${out}/${which}-roles.png` });

    phase("kicked");
    await section("Members").click();
    await row.locator(".lu-roster__row").click();
    await row.getByRole("button", { name: `Kick ${member}` }).click();
    await row.waitFor({ state: "detached" });
    await page.getByRole("heading", { name: /^Members/ }).getByText("1", { exact: true }).waitFor();
  } finally {
    await mine.ctx.close();
    await theirs?.ctx.close();
  }
  console.log(`  ${which}: made roles, gave one from its editor and saw its tag${theirs ? " in two browsers" : ""}, moved one by its menu and by its handle, was asked before Administrator, was refused a name taken, set titles and saw them as tags once the server showed titles, deleted a role after its question, and kicked a member, in ${Date.now() - t0} ms`);
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
    await later(page);
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
    await later(page);
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

// The rail's order and folders (architecture 6.9): three servers in the
// order they were made; one moved up by its menu; a folder made by the
// menu (New folder, then Move to folder for another), opened and closed,
// named and given a tone in Folder settings, and kept across a reload with
// whether it is open; the instance's snapshot has it. Then a server taken
// out by the menu, a drag that Escape puts back, and a real drag of one
// server onto another, which makes a folder of the two.
async function railStep(which: string, stamp: number) {
  const dir = resolve(work, `cli-rail-${which}`);
  const handle = `rail${which}${stamp}`;
  lumia(dir, "instance", "add", instance);
  lumia(dir, "account", "create", "--handle", handle, "--password", password);
  for (const name of ["Rail A", "Rail B", "Rail C"]) {
    const id = lumia(dir, "server-create", name).match(/[0-9a-f-]{36}/)?.[0];
    if (!id) throw new Error("no server id");
    lumia(dir, "channel-create", id, "general");
  }
  const { ctx, page } = await open(which, "rail");
  const t0 = Date.now();
  try {
    await trustCard(page);
    await page.getByLabel("Email or handle").fill(handle);
    await page.getByRole("textbox", { name: "Password" }).fill(password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await later(page);
    await page.getByPlaceholder("Message #general").waitFor();
    const rail = page.locator("nav.lu-rail");
    const tile = (name: string) => rail.getByRole("button", { name: new RegExp(`^${name}(,|$)`) });
    const folder = (label: string) => rail.getByRole("button", { name: new RegExp(`^Folder, ${label}(,|$)`) });
    // Every tile's name, in order, without its counts.
    const order = async () => (await rail.locator("[data-rail-tile]").evaluateAll((els) => els.map((e) => (e.getAttribute("aria-label") ?? "").replace(/, (unread messages|\d+ mentions?)$/, "")))).join(" | ");
    const expect = async (want: string, what: string) => {
      for (let i = 0; i < 50 && (await order()) !== want; i++) await Bun.sleep(100);
      const got = await order();
      if (got !== want) throw new Error(`${what}: the rail reads ${got}, not ${want}`);
    };
    const said = async (want: RegExp) => {
      const live = rail.locator("[aria-live=polite]");
      for (let i = 0; i < 50 && !want.test((await live.textContent()) ?? ""); i++) await Bun.sleep(100);
      const got = (await live.textContent()) ?? "";
      if (!want.test(got)) throw new Error(`announced "${got}", not ${want}`);
    };
    phase("order");
    await expect("Rail A | Rail B | Rail C", "as made");
    phase("move up");
    await tile("Rail C").click({ button: "right" });
    await page.getByRole("menu", { name: "Rail C" }).waitFor();
    await page.screenshot({ path: `${out}/${which}-rail-menu.png` });
    await page.getByRole("menuitem", { name: "Move up" }).click();
    await expect("Rail A | Rail C | Rail B", "Rail C moved up");
    await said(/^Rail C moved to position 2$/);
    phase("new folder");
    // The keyboard's way to a tile's menu.
    await tile("Rail B").focus();
    await page.keyboard.press("Shift+F10");
    await page.getByRole("menuitem", { name: "New folder" }).click();
    await folder("Rail B").waitFor();
    // Its Folder settings open, to name it at once; left as it is.
    const made = page.getByRole("dialog", { name: "Folder settings" });
    await made.waitFor();
    await made.getByRole("button", { name: "Cancel" }).click();
    await made.waitFor({ state: "detached" });
    phase("move to folder");
    await tile("Rail A").click({ button: "right" });
    await page.getByRole("menuitem", { name: "Move to folder" }).click();
    await page.getByRole("menu", { name: "Move to folder" }).getByRole("menuitem", { name: "Rail B" }).click();
    await folder("Rail B and Rail A").waitFor();
    await said(/^Rail A moved into Rail B$/);
    phase("open and close");
    await folder("Rail B and Rail A").click();
    await rail.getByRole("list", { name: "Rail B and Rail A" }).waitFor();
    if ((await folder("Rail B and Rail A").getAttribute("aria-expanded")) !== "true") throw new Error("an open folder does not say so");
    await expect("Rail C | Folder, Rail B and Rail A | Rail B | Rail A", "open");
    await page.screenshot({ path: `${out}/${which}-rail-open.png` });
    await folder("Rail B and Rail A").click();
    await rail.getByRole("list", { name: "Rail B and Rail A" }).waitFor({ state: "detached" });
    await expect("Rail C | Folder, Rail B and Rail A", "closed");
    await page.screenshot({ path: `${out}/${which}-rail-closed.png` });
    await folder("Rail B and Rail A").click();
    phase("folder settings");
    await folder("Rail B and Rail A").click({ button: "right" });
    await page.getByRole("menuitem", { name: "Folder settings…" }).click();
    const dialog = page.getByRole("dialog", { name: "Folder settings" });
    await dialog.waitFor();
    if ((await dialog.getByLabel("Name").getAttribute("placeholder")) !== "Rail B and Rail A") throw new Error("the name's placeholder is not the folder's servers");
    await dialog.getByLabel("Name").fill("Work");
    await dialog.getByRole("radio", { name: "Sky" }).click();
    await page.screenshot({ path: `${out}/${which}-rail-settings.png` });
    await dialog.getByRole("button", { name: "Save" }).click();
    await folder("Work").waitFor();
    phase("kept");
    // Kept before the reload: the instance has answered.
    const signIn = async () => {
      const json = { "content-type": "application/json", accept: "application/json" };
      const pair = (await (await fetch(`${instance}/api/v1/auth/sign-in/password`, { method: "POST", headers: json, body: JSON.stringify({ handleOrEmail: handle, password }) })).json()) as { accessToken?: string };
      if (!pair.accessToken) throw new Error("the suite could not sign in");
      return async () => (await (await fetch(`${instance}/api/v1/account/snapshot`, { headers: { ...json, authorization: `Bearer ${pair.accessToken}` } })).json()) as { rail?: { entries?: { serverId?: string; folder?: { name?: string; tone?: string; serverIds?: string[] } }[] } };
    };
    const snapshot = await signIn();
    const folders = async () => (await snapshot()).rail?.entries?.flatMap((e) => (e.folder ? [e.folder] : [])) ?? [];
    for (let i = 0; i < 50 && !(await folders()).some((f) => f.name === "Work"); i++) await Bun.sleep(100);
    const work1 = (await folders()).find((f) => f.name === "Work");
    if (work1?.tone !== "sky" || work1.serverIds?.length !== 2) throw new Error(`the instance holds ${JSON.stringify(await snapshot())}`);
    await page.reload();
    await page.getByPlaceholder("Message #general").waitFor();
    await expect("Rail C | Folder, Work | Rail B | Rail A", "after a reload, open");
    if (!(await rail.locator(".lu-rail__group.lu-tone-sky").count())) throw new Error("the folder is not in its tone");
    phase("out of the folder");
    await tile("Rail A").click({ button: "right" });
    await page.getByRole("menuitem", { name: "Remove from folder" }).click();
    await expect("Rail C | Folder, Work | Rail B | Rail A", "Rail A out of Work");
    await said(/^Rail A moved out of Work$/);
    await folder("Work").click();
    await expect("Rail C | Folder, Work | Rail A", "Work closed");
    phase("drag");
    // Where a tile is; asked again while it is drawn anew.
    const boxOf = async (what: ReturnType<typeof tile>) => {
      for (let i = 0; i < 20; i++) {
        const box = await what.boundingBox().catch(() => null);
        if (box) return box;
        await Bun.sleep(100);
      }
      throw new Error("a tile is not on the rail");
    };
    const middle = async (name: string) => {
      const box = await boxOf(tile(name));
      return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
    };
    const from = await middle("Rail A");
    const to = await middle("Rail C");
    // Escape puts it back.
    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move(from.x, from.y - 20, { steps: 4 });
    await page.mouse.move(to.x, to.y, { steps: 8 });
    await rail.locator(".is-onto").waitFor();
    await page.keyboard.press("Escape");
    await rail.locator(".is-onto").waitFor({ state: "detached" });
    await page.mouse.up();
    await expect("Rail C | Folder, Work | Rail A", "a drag Escape put back");
    // Onto a server: a folder of the two, no name, Folder settings not opened.
    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move(from.x, from.y - 20, { steps: 4 });
    await page.mouse.move(to.x, to.y, { steps: 8 });
    await rail.locator(".is-onto").waitFor();
    await page.screenshot({ path: `${out}/${which}-rail-drag.png` });
    await page.mouse.up();
    await folder("Rail C and Rail A").waitFor();
    await expect("Folder, Rail C and Rail A | Folder, Work", "dragged onto Rail C");
    if (await page.getByRole("dialog").count()) throw new Error("a folder made by a drag opened its settings");
    for (let i = 0; i < 50 && (await folders()).length !== 2; i++) await Bun.sleep(100);
    const kept = await folders();
    if (kept.length !== 2 || kept[0]?.serverIds?.length !== 2 || kept[0]?.name) throw new Error(`the instance holds ${JSON.stringify(await snapshot())}`);
    // Between: Work dragged above the new folder.
    const work = await boxOf(folder("Work"));
    const top = await boxOf(folder("Rail C and Rail A"));
    await page.mouse.move(work.x + work.width / 2, work.y + work.height / 2);
    await page.mouse.down();
    await page.mouse.move(work.x + work.width / 2, work.y - 10, { steps: 4 });
    await page.mouse.move(top.x + top.width / 2, top.y + 4, { steps: 8 });
    await rail.locator(".lu-rail__line").waitFor();
    await page.screenshot({ path: `${out}/${which}-rail-between.png` });
    await page.mouse.up();
    await expect("Folder, Work | Folder, Rail C and Rail A", "Work dragged first");
    await said(/^Work moved to position 1$/);
  } finally {
    await ctx.close();
  }
  console.log(`  ${which}: moved a server by its menu, made a folder by the menu and moved another into it, opened, closed, named and toned it, kept it across a reload as the instance has it, took a server out, dragged one onto another and a folder above it, in ${Date.now() - t0} ms`);
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
    await later(page);
    await page.getByPlaceholder("Message #general").waitFor();
    const sidebar = page.locator(".lu-shell__sidebar");
    if ((await sidebar.getByRole("link", { name: "design" }).count()) > 0) throw new Error("design is listed before joining it");
    // A member who does not manage channels is offered no Remove on anyone's row.
    await page.locator(".lu-shell__aside").getByRole("button", { name: new RegExp(`^${owner}`) }).waitFor();
    if ((await page.locator(".lu-shell__aside .lu-member__action").count()) > 0) throw new Error("Remove offered to a member who does not manage channels");
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
  // The owner, who manages channels, removes the member from #general by the Remove at the end of their row.
  phase("remove");
  const keeper = await open(which, "membership-owner");
  try {
    await trustCard(keeper.page);
    await keeper.page.getByLabel("Email or handle").fill(owner);
    await keeper.page.getByRole("textbox", { name: "Password" }).fill(password);
    await keeper.page.getByRole("button", { name: "Sign in", exact: true }).click();
    await later(keeper.page);
    await keeper.page.getByPlaceholder("Message #general").waitFor();
    const aside = keeper.page.locator(".lu-shell__aside");
    const row = aside.getByRole("button", { name: new RegExp(`^${member}`) });
    await row.waitFor();
    if ((await aside.getByRole("button", { name: `Remove ${owner} from #general` }).count()) > 0) throw new Error("Remove offered on the owner's own row");
    const remove = aside.getByRole("button", { name: `Remove ${member} from #general` });
    await remove.hover();
    await keeper.page.screenshot({ path: `${out}/${which}-membership-remove.png` });
    await remove.click();
    await row.waitFor({ state: "detached" });
  } finally {
    await keeper.ctx.close();
  }
  console.log(`  ${which}: found a channel in Browse, read it before joining, joined with its notice, left it, wrote in a server DM opened from a profile card, and was removed from #general by the owner's Remove, in ${Date.now() - t0} ms`);
}

// Quote replies, reactions, mentions and read state (architecture 6.6):
// Reply on a message puts its bar atop the composer (with "@ On") and
// keeps the message's toolbar up with Reply pressed, and the reply draws
// the line of what it quotes, which jumps back to the original; a
// reaction picked in the picker comes and goes; a mention picked from the
// @ list is drawn as a pill; and the CLI, as the other member, mentions
// the browser's account while the Mentions page is open: the channel's
// row, the Mentions tile and the title count it, the page lists it, and
// opening it from there shows the "New" line and reads it, which takes
// the counts away.
// History from before this browser's store begins: a channel more than
// 1,000 events long is read from its snapshot (the newest 50), so scrolling
// up reads the store's few and then the instance's scrollback, a page at a
// time, back to the channel's first message.
async function historyStep(which: string, stamp: number) {
  const ownerDir = resolve(work, `cli-history-owner-${which}`);
  const readerDir = resolve(work, `cli-history-reader-${which}`);
  const owner = `writer${which}${stamp}`;
  const reader = `reader${which}${stamp}`;
  lumia(ownerDir, "instance", "add", instance);
  lumia(ownerDir, "account", "create", "--handle", owner, "--password", password);
  const server = lumia(ownerDir, "server-create", `History ${stamp}`).match(/[0-9a-f-]{36}/)?.[0];
  if (!server) throw new Error("no server id");
  const general = lumia(ownerDir, "channel-create", server, "general").match(/[0-9a-f-]{36}/)?.[0];
  if (!general) throw new Error("no channel id");
  const code = lumia(ownerDir, "invite-create", server).trim().split(/\s+/)[0];
  if (!code) throw new Error("no invite code");
  lumia(readerDir, "instance", "add", instance);
  lumia(readerDir, "account", "create", "--handle", reader, "--password", password);
  lumia(readerDir, "join", code);
  phase("seeding");
  // One connection, in order: "history 0" is the channel's first message.
  const SENT = 1_050;
  lumia(ownerDir, "send", general, "history {n}", "--count", String(SENT));
  const { ctx, page } = await open(which, "history");
  const t0 = Date.now();
  try {
    await trustCard(page);
    await page.getByLabel("Email or handle").fill(reader);
    await page.getByRole("textbox", { name: "Password" }).fill(password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await later(page);
    await page.getByPlaceholder("Message #general").waitFor();
    phase("scrolling back");
    const first = page.locator(".lu-msg__body", { hasText: /^history 0$/ });
    const list = page.locator(".lu-msglist").first();
    for (let i = 0; i < 120 && (await first.count()) === 0; i++) {
      // To the oldest message shown: the list is bottom-anchored, so its top is the most negative scroll.
      await list.evaluate((el) => el.scrollTo({ top: -el.scrollHeight }));
      await page.waitForTimeout(250);
    }
    await first.waitFor({ timeout: 5_000 });
    await page.screenshot({ path: `${out}/${which}-history.png` });
  } finally {
    await ctx.close();
  }
  console.log(`  ${which}: scrolled a channel of ${SENT} messages back to its first, past the store into the instance's history, in ${Date.now() - t0} ms`);
}

// What is typed and not sent (#23): a reply half written in #general stays
// there, with the reply it answers, when #design is opened and typed in;
// both are back when their channels are, and after the page is loaded
// again; a send and an emptied field keep nothing.
async function draftsStep(which: string, stamp: number) {
  const ownerDir = resolve(work, `cli-drafts-owner-${which}`);
  const owner = `drafter${which}${stamp}`;
  lumia(ownerDir, "instance", "add", instance);
  lumia(ownerDir, "account", "create", "--handle", owner, "--password", password);
  const server = lumia(ownerDir, "server-create", `Drafts ${stamp}`).match(/[0-9a-f-]{36}/)?.[0];
  if (!server) throw new Error("no server id");
  const general = lumia(ownerDir, "channel-create", server, "general").match(/[0-9a-f-]{36}/)?.[0];
  if (!general) throw new Error("no channel id");
  if (!lumia(ownerDir, "channel-create", server, "design").match(/[0-9a-f-]{36}/)) throw new Error("no channel id");
  lumia(ownerDir, "send", general, "Answer me when you can.");
  const { ctx, page } = await open(which, "drafts");
  const t0 = Date.now();
  try {
    await trustCard(page);
    await page.getByLabel("Email or handle").fill(owner);
    await page.getByRole("textbox", { name: "Password" }).fill(password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await later(page);
    const sidebar = page.locator(".lu-shell__sidebar");
    const inGeneral = page.getByPlaceholder("Message #general");
    const inDesign = page.getByPlaceholder("Message #design");
    const replyBar = page.locator(".lu-replybar", { hasText: `Replying to ${owner}` });
    /** Waits until `field` holds `words`: a kept draft arrives a moment after its conversation. */
    const holds = async (field: typeof inGeneral, words: string, what: string) => {
      await field.waitFor();
      const deadline = Date.now() + 5_000;
      while ((await field.inputValue()) !== words) {
        if (Date.now() > deadline) throw new Error(`${what}: the composer holds ${JSON.stringify(await field.inputValue())}, not ${JSON.stringify(words)}`);
        await page.waitForTimeout(50);
      }
    };
    await sidebar.getByRole("link", { name: "general" }).click();
    await inGeneral.waitFor();
    phase("typed in one channel, then another");
    const original = page.locator(".lu-msg__part", { has: page.locator(".lu-msg__body", { hasText: "Answer me when you can." }) });
    await original.hover();
    await original.getByRole("button", { name: "Reply", exact: true }).click();
    await replyBar.waitFor();
    await inGeneral.fill("Half a thought");
    await sidebar.getByRole("link", { name: "design" }).click();
    await holds(inDesign, "", "another channel");
    if ((await page.locator(".lu-replybar").count()) > 0) throw new Error("the reply went to another channel with the reader");
    await inDesign.fill("Elsewhere");
    await sidebar.getByRole("link", { name: "general" }).click();
    await holds(inGeneral, "Half a thought", "back in the first channel");
    await replyBar.waitFor();
    await sidebar.getByRole("link", { name: "design" }).click();
    await holds(inDesign, "Elsewhere", "back in the second channel");
    phase("the page loaded again");
    // More typed, left to rest and not sent: kept with no switch, as a crash would find it.
    await inDesign.fill("Elsewhere, later");
    await page.waitForTimeout(900);
    await page.reload();
    await sidebar.getByRole("link", { name: "design" }).click();
    await holds(inDesign, "Elsewhere, later", "after a reload");
    await sidebar.getByRole("link", { name: "general" }).click();
    await holds(inGeneral, "Half a thought", "after a reload, the first channel");
    await replyBar.waitFor();
    await page.screenshot({ path: `${out}/${which}-drafts.png` });
    phase("sent, and emptied");
    await inGeneral.press("Enter");
    await page.locator(".lu-msg__body", { hasText: /^Half a thought$/ }).waitFor();
    await holds(inGeneral, "", "after the send");
    await sidebar.getByRole("link", { name: "design" }).click();
    await holds(inDesign, "Elsewhere, later", "the other channel after a send");
    await inDesign.fill("");
    await page.waitForTimeout(900);
    await page.reload();
    await sidebar.getByRole("link", { name: "general" }).click();
    await inGeneral.waitFor();
    await page.waitForTimeout(500);
    await holds(inGeneral, "", "a sent draft after a reload");
    if ((await page.locator(".lu-replybar").count()) > 0) throw new Error("the reply of a sent draft came back");
    await sidebar.getByRole("link", { name: "design" }).click();
    await inDesign.waitFor();
    await page.waitForTimeout(500);
    await holds(inDesign, "", "an emptied draft after a reload");
  } finally {
    await ctx.close();
  }
  console.log(`  ${which}: a reply half written kept with its channel through another channel's draft and a reload, a draft left to rest found after a reload, and nothing kept of one sent or emptied, in ${Date.now() - t0} ms`);
}

async function repliesStep(which: string, stamp: number) {
  const ownerDir = resolve(work, `cli-replies-owner-${which}`);
  const readerDir = resolve(work, `cli-replies-reader-${which}`);
  const owner = `quoted${which}${stamp}`;
  const reader = `quoter${which}${stamp}`;
  lumia(ownerDir, "instance", "add", instance);
  lumia(ownerDir, "account", "create", "--handle", owner, "--password", password);
  const server = lumia(ownerDir, "server-create", `Replies ${stamp}`).match(/[0-9a-f-]{36}/)?.[0];
  if (!server) throw new Error("no server id");
  const general = lumia(ownerDir, "channel-create", server, "general").match(/[0-9a-f-]{36}/)?.[0];
  if (!general) throw new Error("no channel id");
  const code = lumia(ownerDir, "invite-create", server).trim().split(/\s+/)[0];
  if (!code) throw new Error("no invite code");
  lumia(readerDir, "instance", "add", instance);
  lumia(readerDir, "account", "create", "--handle", reader, "--password", password);
  lumia(readerDir, "join", code);
  const readerId = lumia(readerDir, "whoami").trim();
  lumia(ownerDir, "send", general, "Quote me, please.");
  const { ctx, page } = await open(which, "replies");
  const t0 = Date.now();
  try {
    await trustCard(page);
    await page.getByLabel("Email or handle").fill(reader);
    await page.getByRole("textbox", { name: "Password" }).fill(password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await later(page);
    const composer = page.getByPlaceholder("Message #general");
    await composer.waitFor();
    phase("quote reply");
    const original = page.locator(".lu-msg__part", { has: page.locator(".lu-msg__body", { hasText: "Quote me, please." }) });
    await original.hover();
    await original.getByRole("button", { name: "Reply", exact: true }).click();
    await page.locator(".lu-replybar", { hasText: `Replying to ${owner}` }).waitFor();
    await page.getByRole("button", { name: `Mention ${owner}`, pressed: true }).waitFor();
    await page.getByText("Return to send · Esc to cancel the reply").waitFor();
    // The message replied to keeps its toolbar, with Reply pressed.
    await original.getByRole("button", { name: "Reply", pressed: true }).waitFor();
    await composer.fill("Quoted it.");
    await page.screenshot({ path: `${out}/${which}-replies-bar.png` });
    await composer.press("Enter");
    // The line of what it quotes is the reply's row's, over its avatar.
    const reply = page.locator("article.lu-msg", { has: page.locator(".lu-msg__body", { hasText: "Quoted it." }) });
    const ref = reply.locator(".lu-replyref");
    await reply.locator(".lu-replyref", { hasText: "Quote me, please." }).waitFor();
    await reply.locator(".lu-replyref", { hasText: owner }).waitFor();
    if ((await page.locator(".lu-replybar").count()) > 0) throw new Error("the reply bar stayed after sending");
    await ref.click();
    await page.locator(".lu-msg__part.is-flash", { has: page.locator(".lu-msg__body", { hasText: "Quote me, please." }) }).waitFor();
    await page.screenshot({ path: `${out}/${which}-replies-quote.png` });
    phase("reaction");
    await original.hover();
    await original.getByRole("toolbar", { name: "Message actions" }).getByRole("button", { name: "Add reaction" }).click();
    const picker = page.getByRole("dialog", { name: "Emoji" });
    await picker.getByLabel("Search emoji").fill("thumbs up");
    await picker.getByRole("button", { name: "thumbs up", exact: true }).first().click();
    const chip = original.getByRole("button", { name: "You reacted with 👍" });
    await chip.waitFor();
    if ((await chip.getAttribute("aria-pressed")) !== "true") throw new Error("this account's reaction is not drawn pressed");
    await page.screenshot({ path: `${out}/${which}-replies-reaction.png` });
    await chip.click();
    await chip.waitFor({ state: "detached" });
    phase("mention");
    await composer.fill(`@${owner.slice(0, 7)}`);
    const option = page.getByRole("option", { name: new RegExp(owner) });
    await option.waitFor();
    await page.screenshot({ path: `${out}/${which}-replies-list.png` });
    await composer.press("Enter");
    if ((await composer.inputValue()) !== `@${owner} `) throw new Error(`picking left ${JSON.stringify(await composer.inputValue())}`);
    await composer.pressSequentially("have a look.");
    await composer.press("Enter");
    await page.locator(".lu-msg__body .lu-mention", { hasText: `@${owner}` }).waitFor();
    phase("unread");
    const sidebar = page.locator(".lu-shell__sidebar");
    await sidebar.getByRole("link", { name: /^Mentions/ }).click();
    await page.getByText("No mentions yet").waitFor();
    lumia(ownerDir, "send", general, `Over to you, @${reader}.`, "--mention", readerId);
    const row = sidebar.getByRole("link", { name: /general/ });
    await row.getByLabel("1 mention").waitFor();
    await sidebar.getByRole("link", { name: /^Mentions/ }).getByLabel("1 mention").waitFor();
    await page.waitForFunction(() => document.title.startsWith("(1)"));
    const entry = page.locator(".lu-mentionentry", { hasText: "Over to you" });
    await entry.waitFor();
    await page.screenshot({ path: `${out}/${which}-replies-unread.png` });
    phase("read");
    await entry.click();
    await page.getByRole("separator", { name: "New messages" }).waitFor();
    await row.getByLabel("1 mention").waitFor({ state: "detached" });
    await page.waitForFunction(() => document.title === "Lumia");
    if (await row.evaluate((el) => el.classList.contains("is-unread"))) throw new Error("the channel is still drawn unread once read");
    await page.screenshot({ path: `${out}/${which}-replies-read.png` });
  } finally {
    await ctx.close();
  }
  console.log(`  ${which}: replied with a quote that jumps back, reacted and took it back, mentioned from the @ list, and a mention's counts went once it was read from the Mentions page, in ${Date.now() - t0} ms`);
}

// Editing and deleting (architecture 6.6): this account's own message is
// edited where it is drawn and then says "(edited)", and what is typed
// stays while the conversation is read again under the field (another
// message arriving, a reaction to the message itself); Up in the empty
// composer takes the newest one up again and Escape puts it down; Delete
// message asks "Delete this message?" first, and Shift skips the
// question; someone else's message offers neither to a member.
async function editingStep(which: string, stamp: number) {
  const ownerDir = resolve(work, `cli-editing-owner-${which}`);
  const writerDir = resolve(work, `cli-editing-writer-${which}`);
  const owner = `keeps${which}${stamp}`;
  const writer = `edits${which}${stamp}`;
  lumia(ownerDir, "instance", "add", instance);
  lumia(ownerDir, "account", "create", "--handle", owner, "--password", password);
  const server = lumia(ownerDir, "server-create", `Editing ${stamp}`).match(/[0-9a-f-]{36}/)?.[0];
  if (!server) throw new Error("no server id");
  const general = lumia(ownerDir, "channel-create", server, "general").match(/[0-9a-f-]{36}/)?.[0];
  if (!general) throw new Error("no channel id");
  const code = lumia(ownerDir, "invite-create", server).trim().split(/\s+/)[0];
  if (!code) throw new Error("no invite code");
  lumia(writerDir, "instance", "add", instance);
  lumia(writerDir, "account", "create", "--handle", writer, "--password", password);
  lumia(writerDir, "join", code);
  lumia(ownerDir, "send", general, "Not yours to change.");
  const { ctx, page } = await open(which, "editing");
  const t0 = Date.now();
  try {
    await trustCard(page);
    await page.getByLabel("Email or handle").fill(writer);
    await page.getByRole("textbox", { name: "Password" }).fill(password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await later(page);
    const composer = page.getByPlaceholder("Message #general");
    await composer.waitFor();
    const part = (text: string) => page.locator(".lu-msg__part", { has: page.locator(".lu-msg__body", { hasText: text }) });
    const more = async (text: string) => {
      await part(text).hover();
      await part(text).getByRole("button", { name: "More actions" }).click();
    };
    phase("someone else's");
    await more("Not yours to change.");
    await page.getByRole("menuitem", { name: "Copy text" }).waitFor();
    if ((await page.getByRole("menuitem", { name: /Edit message|Delete message/ }).count()) > 0) throw new Error("a member is offered to edit or delete someone else's message");
    await page.keyboard.press("Escape");
    await page.getByRole("menu").waitFor({ state: "detached" });
    phase("edit");
    await composer.fill("First draft.");
    await composer.press("Enter");
    await part("First draft.").waitFor();
    const draftId = await part("First draft.").getAttribute("data-message-id");
    if (!draftId) throw new Error("the message sent has no id");
    await more("First draft.");
    await page.getByRole("menuitem", { name: "Edit message" }).click();
    const field = page.getByRole("textbox", { name: "Edit message" });
    await field.waitFor();
    if ((await field.inputValue()) !== "First draft.") throw new Error(`the edit opened with ${JSON.stringify(await field.inputValue())}`);
    await page.getByText("Esc to cancel · Return to save").waitFor();
    await field.fill(`Second draft for @${owner}.`);
    phase("read again while typing");
    // The conversation is read again under the field, for another message and then for a
    // reaction to the very message being edited: the field is the one it was, with what was
    // typed in it and the keyboard.
    await field.evaluate((el) => ((el as HTMLElement).dataset.typedIn = "yes"));
    lumia(ownerDir, "send", general, "While you type.");
    await part("While you type.").waitFor();
    lumia(ownerDir, "react", general, draftId, "👍");
    await page.locator(`[data-message-id="${draftId}"]`).getByRole("button", { name: /reacted with 👍/ }).waitFor();
    if ((await field.inputValue()) !== `Second draft for @${owner}.`) throw new Error(`what was typed did not stay when the conversation was read again: ${JSON.stringify(await field.inputValue())}`);
    if (!(await field.evaluate((el) => (el as HTMLElement).dataset.typedIn === "yes" && el === document.activeElement))) throw new Error("the edit field was made again, or lost the keyboard, when the conversation was read again");
    phase("edit");
    await page.screenshot({ path: `${out}/${which}-editing-field.png` });
    await field.press("Enter");
    await field.waitFor({ state: "detached" });
    await part("Second draft").locator(".lu-msg__edited", { hasText: "(edited)" }).waitFor();
    // An edit carries its mentions again.
    await part("Second draft").locator(".lu-mention", { hasText: `@${owner}` }).waitFor();
    if ((await part("First draft.").count()) > 0) throw new Error("the old body stayed");
    await page.screenshot({ path: `${out}/${which}-editing-edited.png` });
    phase("up arrow");
    await composer.click();
    await composer.press("ArrowUp");
    await field.waitFor();
    if ((await field.inputValue()) !== `Second draft for @${owner}.`) throw new Error(`Up took up ${JSON.stringify(await field.inputValue())}`);
    await field.press("Escape");
    await field.waitFor({ state: "detached" });
    await part("Second draft").waitFor();
    phase("delete");
    await more("Second draft");
    await page.getByRole("menuitem", { name: "Delete message" }).click();
    const ask = page.getByRole("dialog", { name: "Delete this message?" });
    await ask.waitFor();
    await ask.getByText(`Second draft for @${owner}.`).waitFor();
    await page.screenshot({ path: `${out}/${which}-editing-delete.png` });
    await ask.getByRole("button", { name: "Delete", exact: true }).click();
    await ask.waitFor({ state: "detached" });
    await part("Second draft").waitFor({ state: "detached" });
    // The server shows no deleted messages unless it says so (architecture 6.7): gone, no "(deleted)".
    if ((await page.locator(".lu-msg__body", { hasText: /^\(deleted\)$/ }).count()) > 0) throw new Error("a deleted message kept its place in a server that does not show them");
    phase("shift delete");
    await composer.fill("Gone at once.");
    await composer.press("Enter");
    await part("Gone at once.").waitFor();
    await more("Gone at once.");
    await page.getByRole("menuitem", { name: "Delete message" }).click({ modifiers: ["Shift"] });
    await part("Gone at once.").waitFor({ state: "detached" });
    if ((await ask.count()) > 0) throw new Error("Shift did not skip the question");
    await page.screenshot({ path: `${out}/${which}-editing-deleted.png` });
  } finally {
    await ctx.close();
  }
  console.log(`  ${which}: edited its own message in place ("(edited)"), what it typed staying while a message and a reaction arrived, took it up again with Up and put it down with Escape, deleted one after the question and one at once with Shift, and was offered neither on someone else's, in ${Date.now() - t0} ms`);
}

// A right-click's menus (#25). A message's has what its toolbar and More
// actions hold, by their rules: someone else's offers a member neither Edit
// message nor Delete message, this account's own offers both, and Delete
// message asks first unless Shift is held. It opens at the pointer, and
// with Shift+F10 from inside the message, where the arrow keys move through
// it and Escape gives the keyboard back. A link in a message keeps the
// browser's own menu. A person's, on a member's row and on a message's
// author, has View profile and what their card offers; this account's own
// has View profile alone. A right-click elsewhere while one is open moves
// to the other.
async function menusStep(which: string, stamp: number) {
  const ownerDir = resolve(work, `cli-menus-owner-${which}`);
  const memberDir = resolve(work, `cli-menus-member-${which}`);
  const owner = `asked${which}${stamp}`;
  const member = `clicks${which}${stamp}`;
  lumia(ownerDir, "instance", "add", instance);
  lumia(ownerDir, "account", "create", "--handle", owner, "--password", password);
  const server = lumia(ownerDir, "server-create", `Menus ${stamp}`).match(/[0-9a-f-]{36}/)?.[0];
  if (!server) throw new Error("no server id");
  const general = lumia(ownerDir, "channel-create", server, "general").match(/[0-9a-f-]{36}/)?.[0];
  if (!general) throw new Error("no channel id");
  const code = lumia(ownerDir, "invite-create", server).trim().split(/\s+/)[0];
  if (!code) throw new Error("no invite code");
  lumia(memberDir, "instance", "add", instance);
  lumia(memberDir, "account", "create", "--handle", member, "--password", password);
  lumia(memberDir, "join", code);
  lumia(ownerDir, "send", general, "Not yours to change.");
  lumia(ownerDir, "send", general, "The notes are at https://example.com/notes for now.");
  const { ctx, page } = await open(which, "menus");
  if (which === "chromium") await ctx.grantPermissions(["clipboard-read", "clipboard-write"]);
  const t0 = Date.now();
  try {
    await trustCard(page);
    await page.getByLabel("Email or handle").fill(member);
    await page.getByRole("textbox", { name: "Password" }).fill(password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await later(page);
    const composer = page.getByPlaceholder("Message #general");
    await composer.waitFor();
    const part = (text: string) => page.locator(".lu-msg__part", { has: page.locator(".lu-msg__body", { hasText: text }) });
    // A message's words: where its menu is asked for.
    const words = (text: string) => part(text).locator(".lu-body__para", { hasText: text });
    const menu = page.getByRole("menu", { name: "Message" });
    const reads = async (want: string, what: string) => {
      const got = (await page.getByRole("menuitem").allTextContents()).map((s) => s.trim()).join(" | ");
      if (got !== want) throw new Error(`${what}: the menu reads ${got}, not ${want}`);
    };
    const gone = () => page.getByRole("menu").waitFor({ state: "detached" });
    const focusedOn = () =>
      page.evaluate(() => {
        const a = document.activeElement;
        return a ? `${a.getAttribute("role") ?? a.tagName.toLowerCase()}:${a.getAttribute("aria-label") ?? a.getAttribute("placeholder") ?? a.textContent?.trim() ?? ""}` : "nothing";
      });
    await part("The notes are at").waitFor();
    phase("someone else's");
    await composer.click();
    await words("Not yours to change.").click({ button: "right" });
    await menu.waitFor();
    await reads("Reply | Add reaction | Copy text", "someone else's message, to a member");
    // At the pointer, which was on the middle of the words: the menu's corner is there, under it or over it.
    const pressedAt = await words("Not yours to change.").boundingBox();
    const box = await page.locator(".lu-popover__box").boundingBox();
    if (!pressedAt || !box) throw new Error("the menu or the message has no place on the page");
    const [x, y] = [pressedAt.x + pressedAt.width / 2, pressedAt.y + pressedAt.height / 2];
    if (Math.abs(box.x - x) > 2 || (Math.abs(box.y - y) > 2 && Math.abs(box.y + box.height - y) > 2)) throw new Error(`the menu is at ${JSON.stringify(box)}, not at the pointer (${x}, ${y})`);
    if ((await focusedOn()) !== "menuitem:Reply") throw new Error(`the menu opened with the keyboard in ${await focusedOn()}`);
    await page.screenshot({ path: `${out}/${which}-menus-message.png` });
    await page.keyboard.press("Escape");
    await gone();
    // A message takes no keyboard: it goes back to where it was.
    if ((await focusedOn()) !== "textarea:Message #general") throw new Error(`closing the menu left the keyboard in ${await focusedOn()}, not the composer`);
    phase("reply");
    await words("Not yours to change.").click({ button: "right" });
    await page.getByRole("menuitem", { name: "Reply", exact: true }).click();
    await gone();
    await page.locator(".lu-replybar", { hasText: `Replying to ${owner}` }).waitFor();
    await part("Not yours to change.").getByRole("button", { name: "Reply", pressed: true }).waitFor();
    if ((await focusedOn()) !== "textarea:Message #general") throw new Error(`Reply left the keyboard in ${await focusedOn()}, not the composer`);
    await composer.press("Escape");
    await page.locator(".lu-replybar").waitFor({ state: "detached" });
    phase("own");
    await composer.fill("Mine to change.");
    await composer.press("Enter");
    await part("Mine to change.").waitFor();
    await words("Mine to change.").click({ button: "right" });
    await menu.waitFor();
    await reads("Reply | Add reaction | Edit message | Copy text | Delete message", "this account's own message");
    if (!(await page.getByRole("menuitem", { name: "Delete message" }).evaluate((el) => el.classList.contains("is-danger")))) throw new Error("Delete message is not in the danger ink");
    await page.screenshot({ path: `${out}/${which}-menus-own.png` });
    phase("copy");
    await page.getByRole("menuitem", { name: "Copy text" }).click();
    await gone();
    if (which === "chromium") {
      const held = await page.evaluate(() => navigator.clipboard.readText());
      if (held !== "Mine to change.") throw new Error(`Copy text copied ${JSON.stringify(held)}`);
    }
    phase("add reaction");
    await words("Mine to change.").click({ button: "right" });
    await page.getByRole("menuitem", { name: "Add reaction" }).click();
    const picker = page.getByRole("dialog", { name: "Emoji" });
    await picker.getByLabel("Search emoji").fill("thumbs up");
    await picker.getByRole("button", { name: "thumbs up", exact: true }).first().click();
    await part("Mine to change.").getByRole("button", { name: "You reacted with 👍" }).waitFor();
    phase("edit");
    await words("Mine to change.").click({ button: "right" });
    await page.getByRole("menuitem", { name: "Edit message" }).click();
    const field = page.getByRole("textbox", { name: "Edit message" });
    await field.waitFor();
    if ((await field.inputValue()) !== "Mine to change.") throw new Error(`the edit opened with ${JSON.stringify(await field.inputValue())}`);
    // The field being typed in keeps the browser's menu.
    await field.click({ button: "right" });
    await Bun.sleep(300);
    if ((await page.getByRole("menu").count()) > 0) throw new Error("a right-click in the edit field opened the message's menu");
    await field.press("Escape");
    await field.waitFor({ state: "detached" });
    phase("delete");
    await words("Mine to change.").click({ button: "right" });
    await page.getByRole("menuitem", { name: "Delete message" }).click();
    const ask = page.getByRole("dialog", { name: "Delete this message?" });
    await ask.waitFor();
    await ask.getByText("Mine to change.").waitFor();
    await page.screenshot({ path: `${out}/${which}-menus-delete.png` });
    await ask.getByRole("button", { name: "Cancel" }).click();
    await ask.waitFor({ state: "detached" });
    await part("Mine to change.").waitFor();
    await words("Mine to change.").click({ button: "right" });
    await page.getByRole("menuitem", { name: "Delete message" }).click({ modifiers: ["Shift"] });
    await part("Mine to change.").waitFor({ state: "detached" });
    if ((await ask.count()) > 0) throw new Error("Shift did not skip the question");
    phase("link");
    // Whether the page took the last right-click from the browser, asked after the page has had it.
    await page.evaluate(() => window.addEventListener("contextmenu", (e) => ((window as unknown as { lumiaMenuTaken?: boolean }).lumiaMenuTaken = e.defaultPrevented)));
    const taken = () => page.evaluate(() => (window as unknown as { lumiaMenuTaken?: boolean }).lumiaMenuTaken);
    await part("The notes are at").locator("a.lu-body__link").click({ button: "right" });
    await Bun.sleep(300);
    if ((await page.getByRole("menu").count()) > 0) throw new Error("a right-click on a link opened the message's menu");
    if ((await taken()) !== false) throw new Error("a right-click on a link was taken from the browser");
    await page.keyboard.press("Escape");
    phase("a person");
    const aside = page.locator(".lu-shell__aside");
    const row = aside.getByRole("button", { name: new RegExp(`^${owner}`) });
    const theirs = page.getByRole("menu", { name: owner });
    await row.click({ button: "right" });
    await theirs.waitFor();
    await reads(`View profile | Message ${owner} | Message privately`, "a member's row");
    await page.screenshot({ path: `${out}/${which}-menus-person.png` });
    await page.getByRole("menuitem", { name: "View profile" }).click();
    const card = page.getByRole("dialog", { name: owner });
    await card.waitFor();
    await card.getByRole("button", { name: `Message ${owner}` }).waitFor();
    await card.getByRole("button", { name: "Message privately" }).waitFor();
    await page.keyboard.press("Escape");
    await card.waitFor({ state: "detached" });
    if (!(await row.evaluate((el) => el === document.activeElement))) throw new Error(`closing the card left the keyboard in ${await focusedOn()}, not the member's row`);
    // This account itself: its profile, and nothing to write to.
    await aside.getByRole("button", { name: new RegExp(`^${member}`) }).click({ button: "right" });
    await page.getByRole("menu", { name: member }).waitFor();
    await reads("View profile", "this account's own row");
    await page.keyboard.press("Escape");
    await gone();
    // On a message's author the person's menu comes up, not the message's.
    await page.locator(".lu-msg__name", { hasText: owner }).first().click({ button: "right" });
    await theirs.waitFor();
    await reads(`View profile | Message ${owner} | Message privately`, "a message's author");
    if ((await menu.count()) > 0) throw new Error("the message's menu came up on its author's name");
    await page.keyboard.press("Escape");
    await gone();
    phase("keyboard");
    await part("Not yours to change.").hover();
    const replyButton = part("Not yours to change.").getByRole("toolbar", { name: "Message actions" }).getByRole("button", { name: "Reply", exact: true });
    await replyButton.focus();
    await page.keyboard.press("Shift+F10");
    await menu.waitFor();
    await reads("Reply | Add reaction | Copy text", "Shift+F10 in a message's toolbar");
    if ((await focusedOn()) !== "menuitem:Reply") throw new Error(`the menu opened with the keyboard in ${await focusedOn()}`);
    await page.keyboard.press("ArrowDown");
    if ((await focusedOn()) !== "menuitem:Add reaction") throw new Error(`Down moved the keyboard to ${await focusedOn()}`);
    await page.keyboard.press("Escape");
    await gone();
    if (!(await replyButton.evaluate((el) => el === document.activeElement))) throw new Error(`closing the menu left the keyboard in ${await focusedOn()}, not the toolbar's button`);
    await row.focus();
    await page.keyboard.press("Shift+F10");
    await theirs.waitFor();
    await page.keyboard.press("Escape");
    await gone();
    if (!(await row.evaluate((el) => el === document.activeElement))) throw new Error(`closing a person's menu left the keyboard in ${await focusedOn()}, not their row`);
    phase("another right-click");
    // With one open, a right-click elsewhere closes it and opens the menu of what it landed on.
    await words("Not yours to change.").click({ button: "right" });
    await menu.waitFor();
    const there = await row.boundingBox();
    if (!there) throw new Error("the member's row has no place on the page");
    await page.mouse.click(there.x + there.width / 2, there.y + there.height / 2, { button: "right" });
    await theirs.waitFor();
    if ((await page.getByRole("menu").count()) !== 1) throw new Error("two menus are open at once");
    await page.keyboard.press("Escape");
    await gone();
  } finally {
    await ctx.close();
  }
  console.log(`  ${which}: a right-click on someone else's message offered Reply, Add reaction and Copy text, and on its own Edit message and Delete message too; Reply, Copy text, Add reaction, Edit message and Delete message (asked, and at once with Shift) each worked from it; a link kept the browser's menu; a member's row and a message's author opened the person's menu, View profile the card; Shift+F10 opened both, and a right-click elsewhere moved from one to the other, in ${Date.now() - t0} ms`);
}

// A changed name or picture (#15): someone in the browser account's server
// renames themself and sets a picture from the CLI, and with the page open
// and no reload, their message and the member list draw them as they now are.
async function profileStep(which: string, stamp: number) {
  const ownerDir = resolve(work, `cli-profile-owner-${which}`);
  const otherDir = resolve(work, `cli-profile-other-${which}`);
  const owner = `watches${which}${stamp}`;
  const other = `renames${which}${stamp}`;
  lumia(ownerDir, "instance", "add", instance);
  lumia(ownerDir, "account", "create", "--handle", owner, "--password", password);
  const server = lumia(ownerDir, "server-create", `Profile ${stamp}`).match(/[0-9a-f-]{36}/)?.[0];
  if (!server) throw new Error("no server id");
  const general = lumia(ownerDir, "channel-create", server, "general").match(/[0-9a-f-]{36}/)?.[0];
  if (!general) throw new Error("no channel id");
  const code = lumia(ownerDir, "invite-create", server).trim().split(/\s+/)[0];
  if (!code) throw new Error("no invite code");
  lumia(otherDir, "instance", "add", instance);
  lumia(otherDir, "account", "create", "--handle", other, "--password", password);
  lumia(otherDir, "join", code);
  lumia(otherDir, "send", general, "Who am I now?");
  const picture = resolve(work, `profile-${which}.png`);
  await Bun.write(picture, png(64, [252, 103, 48]));
  const { ctx, page } = await open(which, "profile");
  const t0 = Date.now();
  try {
    await trustCard(page);
    await page.getByLabel("Email or handle").fill(owner);
    await page.getByRole("textbox", { name: "Password" }).fill(password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await later(page);
    await page.getByPlaceholder("Message #general").waitFor();
    const message = page.locator("article.lu-msg", { has: page.locator(".lu-msg__body", { hasText: "Who am I now?" }) });
    const member = page.locator(".lu-shell__aside .lu-member", { hasText: other });
    phase("before");
    await message.locator(".lu-msg__author", { hasText: other }).waitFor();
    await member.waitFor();
    const photo = message.locator(".lu-avatar__photo");
    if ((await photo.count()) > 0) throw new Error("a picture is drawn before one was set");
    phase("renamed");
    const name = `Renamed ${which} ${stamp}`;
    lumia(otherDir, "account", "update", "--name", name, "--picture", picture);
    await message.locator(".lu-msg__author", { hasText: name }).waitFor();
    const renamed = page.locator(".lu-shell__aside .lu-member", { hasText: name });
    await renamed.waitFor();
    await photo.waitFor();
    const src = await photo.getAttribute("src");
    if (!src || !/\/api\/v1\/avatars\/[0-9a-f]{64}$/.test(src)) throw new Error(`the message's picture is ${src}`);
    await renamed.locator(`.lu-avatar__photo[src="${src}"]`).waitFor();
    await page.screenshot({ path: `${out}/${which}-profile-renamed.png` });
    phase("again");
    // A second picture is a new address, drawn in place of the first.
    await Bun.write(picture, png(64, [48, 252, 103]));
    lumia(otherDir, "account", "update", "--picture", picture);
    await message.locator(`.lu-avatar__photo:not([src="${src}"])`).waitFor();
    await renamed.locator(`.lu-avatar__photo:not([src="${src}"])`).waitFor();
  } finally {
    await ctx.close();
  }
  console.log(`  ${which}: a member renamed themself and set a picture, then another, and their message and the member list drew each at once, with no reload, in ${Date.now() - t0} ms`);
}

// Attachments (specs/media.md 5): a member sends a picture from the CLI and
// the page draws it, its thumbnail decoded; the page attaches a picture
// through the attach button's file input, gives it alt text, sends it with
// words, draws it from what it sent; the member sends another message and
// reacts to the one with the picture, and each picture is still the element
// it was; the page opens its picture in the viewer, zooms it by a press,
// the wheel and the keys, drags it, and closes it with Escape; the member's
// CLI opens the picture the page sent.
async function mediaStep(which: string, stamp: number) {
  const ownerDir = resolve(work, `cli-media-owner-${which}`);
  const otherDir = resolve(work, `cli-media-other-${which}`);
  const owner = `attaches${which}${stamp}`;
  const other = `sees${which}${stamp}`;
  lumia(ownerDir, "instance", "add", instance);
  lumia(ownerDir, "account", "create", "--handle", owner, "--password", password);
  const server = lumia(ownerDir, "server-create", `Media ${stamp}`).match(/[0-9a-f-]{36}/)?.[0];
  if (!server) throw new Error("no server id");
  const general = lumia(ownerDir, "channel-create", server, "general").match(/[0-9a-f-]{36}/)?.[0];
  if (!general) throw new Error("no channel id");
  const code = lumia(ownerDir, "invite-create", server).trim().split(/\s+/)[0];
  if (!code) throw new Error("no invite code");
  lumia(otherDir, "instance", "add", instance);
  lumia(otherDir, "account", "create", "--handle", other, "--password", password);
  lumia(otherDir, "join", code);
  const theirs = resolve(work, `media-theirs-${which}.png`);
  await Bun.write(theirs, png(320, [40, 120, 220]));
  lumia(otherDir, "send", general, "A picture from the other side", "--attach", theirs);
  const { ctx, page } = await open(which, "media");
  const t0 = Date.now();
  // Whether an <img> has decoded something to draw.
  const drawn = (img: ReturnType<Page["locator"]>) => img.evaluate((el) => (el as HTMLImageElement).complete && (el as HTMLImageElement).naturalWidth > 0);
  try {
    await trustCard(page);
    await page.getByLabel("Email or handle").fill(owner);
    await page.getByRole("textbox", { name: "Password" }).fill(password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await later(page);
    await page.getByPlaceholder("Message #general").waitFor();
    phase("received");
    const received = page.locator(".lu-msg__part", { hasText: "A picture from the other side" }).locator(".lu-media__img");
    await received.waitFor({ timeout: 30_000 });
    for (let i = 0; i < 50 && !(await drawn(received)); i++) await Bun.sleep(100);
    if (!(await drawn(received))) throw new Error("the received picture is not drawn");
    phase("attach");
    const [chooser] = await Promise.all([page.waitForEvent("filechooser"), page.getByRole("button", { name: "Attach a file" }).click()]);
    await chooser.setFiles({ name: "square.png", mimeType: "image/png", buffer: png(240, [30, 180, 90]) });
    const tile = page.locator(".lu-draft", { hasText: "square.png" });
    await tile.waitFor();
    await tile.getByLabel("Alt text for square.png").fill("A green square");
    await tile.getByLabel("Alt text for square.png").press("Tab");
    await tile.locator(".lu-draft__meta", { hasText: /KB|bytes/ }).waitFor({ timeout: 30_000 });
    await page.getByPlaceholder("Message #general").fill("Here is mine");
    await page.getByPlaceholder("Message #general").press("Enter");
    phase("sent");
    await tile.waitFor({ state: "detached", timeout: 30_000 });
    const mine = page.locator(".lu-msg__part", { hasText: "Here is mine" });
    const picture = mine.getByRole("button", { name: "A green square" });
    await picture.waitFor({ timeout: 30_000 });
    const img = picture.locator(".lu-media__img");
    await img.waitFor();
    for (let i = 0; i < 50 && !(await drawn(img)); i++) await Bun.sleep(100);
    if (!(await drawn(img))) throw new Error("the picture sent is not drawn");
    await page.screenshot({ path: `${out}/${which}-media-sent.png` });
    phase("read again");
    // The conversation read again, for another message and then for a reaction to the one
    // that carries the picture: each picture is the element it was (so a video would play
    // on), not one made again.
    const sentId = await mine.getAttribute("data-message-id");
    if (!sentId) throw new Error("the message sent has no id");
    await img.evaluate((el) => ((el as HTMLElement).dataset.drawnOnce = "yes"));
    await received.evaluate((el) => ((el as HTMLElement).dataset.drawnOnce = "yes"));
    lumia(otherDir, "send", general, "And another message");
    await page.locator(".lu-msg__part", { hasText: "And another message" }).waitFor();
    lumia(otherDir, "react", general, sentId, "👍");
    await mine.getByRole("button", { name: /reacted with 👍/ }).waitFor();
    for (const [name, shown] of [["sent", img], ["received", received]] as const) {
      if ((await shown.getAttribute("data-drawn-once")) !== "yes") throw new Error(`the ${name} picture was made again when the conversation was read again`);
      if (!(await drawn(shown))) throw new Error(`the ${name} picture is no longer drawn`);
    }
    phase("viewer");
    await picture.click();
    const viewer = page.getByRole("dialog", { name: "A green square" });
    await viewer.waitFor();
    await viewer.getByRole("img", { name: "A green square" }).waitFor();
    await viewer.getByRole("link", { name: "Save" }).waitFor();
    await page.screenshot({ path: `${out}/${which}-media-viewer.png` });
    // Zoomed by a press on it; a drag leaves it zoomed (the click a drag ends with is no press); the wheel towards the hand takes it back to fitted, where Fit has nothing to do; a key zooms and 0 fits.
    const shown = viewer.getByRole("img", { name: "A green square" });
    const scale = async () => Number(/scale\(([\d.]+)\)/.exec(await shown.evaluate((e) => (e as HTMLElement).style.transform))?.[1]);
    const fit = viewer.getByRole("button", { name: "Fit to window" });
    if (!(await fit.isDisabled())) throw new Error("Fit is offered on a fitted picture");
    const at = await shown.boundingBox();
    if (!at) throw new Error("the viewer's picture has no box");
    const [x, y] = [at.x + at.width / 2, at.y + at.height / 2];
    await page.mouse.click(x, y);
    const pressed = await scale();
    if (!(pressed > 1)) throw new Error(`a press on the picture zoomed to ${pressed}`);
    await page.mouse.move(x, y);
    await page.mouse.down();
    await page.mouse.move(x + 40, y + 12, { steps: 4 });
    await page.mouse.up();
    if ((await scale()) !== pressed) throw new Error(`a drag left the picture at ${await scale()}, from ${pressed}`);
    for (let i = 0; i < 8; i++) await page.mouse.wheel(0, 100);
    await fit.and(page.locator(":disabled")).waitFor();
    if ((await scale()) !== 1) throw new Error(`the wheel left the picture at ${await scale()}`);
    await page.mouse.wheel(0, -100);
    await fit.and(page.locator(":enabled")).waitFor();
    if (!(Math.abs((await scale()) - Math.exp(0.25)) < 0.001)) throw new Error(`a notch of the wheel zoomed to ${await scale()}`);
    await page.keyboard.press("0");
    await page.keyboard.press("+");
    if ((await scale()) !== 1.5) throw new Error(`the plus key zoomed to ${await scale()}`);
    await page.keyboard.press("0");
    await fit.and(page.locator(":disabled")).waitFor();
    await page.keyboard.press("Escape");
    await viewer.waitFor({ state: "detached" });
    phase("opened by the other side");
    const id = await mine.getAttribute("data-message-id");
    if (!id) throw new Error("the message sent has no id");
    const saved = resolve(work, `media-saved-${which}`);
    mkdirSync(saved, { recursive: true });
    let said = "";
    for (let i = 0; i < 20 && !/square/.test(said); i++) {
      said = lumia(otherDir, "attachments", general, id, "--out", saved);
      if (!/square/.test(said)) await Bun.sleep(500);
    }
    if (!/square/.test(said)) throw new Error(`the other side did not open the picture: ${said}`);
  } finally {
    await ctx.close();
  }
  console.log(`  ${which}: a picture from the CLI drawn; one attached here with alt text, sent, drawn, each still the element it was once the conversation was read again, opened in the viewer, zoomed and closed, and opened by the other member, in ${Date.now() - t0} ms`);
}

// An attachment the server lets go while its row is on screen (specs/media.md
// 4.5): the server deletes its oldest attachments when it is full, a member
// sends a file that fills it and then another, and the first one's row, on
// screen all the while, turns to "No longer available" with nothing to
// download. Needs an instance that limits a server's files
// (`LUMIA_SERVER_STORAGE_MAX_BYTES`, small enough that two files fill it);
// against one that does not, it says so and checks nothing.
async function expiredStep(which: string, stamp: number) {
  const ownerDir = resolve(work, `cli-expired-owner-${which}`);
  const otherDir = resolve(work, `cli-expired-other-${which}`);
  const owner = `keepsfiles${which}${stamp}`;
  const other = `sendsfiles${which}${stamp}`;
  lumia(ownerDir, "instance", "add", instance);
  lumia(ownerDir, "account", "create", "--handle", owner, "--password", password);
  const server = lumia(ownerDir, "server-create", `Expired ${stamp}`).match(/[0-9a-f-]{36}/)?.[0];
  if (!server) throw new Error("no server id");
  const general = lumia(ownerDir, "channel-create", server, "general").match(/[0-9a-f-]{36}/)?.[0];
  if (!general) throw new Error("no channel id");
  const code = lumia(ownerDir, "invite-create", server).trim().split(/\s+/)[0];
  if (!code) throw new Error("no invite code");
  lumia(otherDir, "instance", "add", instance);
  lumia(otherDir, "account", "create", "--handle", other, "--password", password);
  lumia(otherDir, "join", code);
  const { ctx, page } = await open(which, "expired");
  const t0 = Date.now();
  try {
    await trustCard(page);
    await page.getByLabel("Email or handle").fill(owner);
    await page.getByRole("textbox", { name: "Password" }).fill(password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await later(page);
    await page.getByPlaceholder("Message #general").waitFor();
    phase("the limit");
    await page.getByRole("button", { name: /server menu/ }).click();
    await page.getByRole("menuitem", { name: "Server settings" }).click();
    const files = page.locator("section", { has: page.getByRole("heading", { name: "Files" }) });
    const usage = files.getByRole("status").filter({ hasText: /used$/ });
    await usage.waitFor();
    // "0 bytes of 2 MB used", in 1,000s; "0 bytes used" where the instance sets no limit.
    const said = /of ([\d.]+) (bytes|KB|MB|GB|TB) used$/.exec((await usage.textContent())?.trim() ?? "");
    const limit = said ? Number(said[1]) * 1000 ** ["bytes", "KB", "MB", "GB", "TB"].indexOf(said[2] ?? "") : 0;
    // Two files of this size: the first fits, both do not, and each is under what one upload may be.
    const size = Math.round(limit * 0.55);
    if (!limit || size > 9_000_000) {
      console.log(`  ${which}: the instance ${limit ? "lets a server's files grow past what two uploads fill" : "sets no limit on a server's files"} (LUMIA_SERVER_STORAGE_MAX_BYTES), so an attachment let go while on screen is not checked`);
      return;
    }
    phase("delete the oldest");
    await files.getByRole("tab", { name: "Delete the oldest attachments" }).click();
    await files.getByRole("button", { name: "Save", exact: true }).click();
    await files.getByText("Saved.").waitFor();
    await page.locator(".lu-shell__sidebar").getByRole("link", { name: /general/ }).click();
    await page.getByPlaceholder("Message #general").waitFor();
    phase("the old file");
    const send = (name: string, text: string) => {
      const path = resolve(work, `expired-${which}-${name}`);
      mkdirSync(path, { recursive: true });
      const file = resolve(path, name);
      return Bun.write(file, new Uint8Array(size).fill(7)).then(() => lumia(otherDir, "send", general, text, "--attach", file));
    };
    await send("old.bin", "The old file");
    const row = page.locator(".lu-msg__part", { hasText: "The old file" }).locator(".lu-attach");
    await row.getByRole("button", { name: "Download old.bin" }).waitFor({ timeout: 30_000 });
    await row.evaluate((el) => ((el as HTMLElement).dataset.drawnOnce = "yes"));
    phase("the new file");
    await send("new.bin", "The new file");
    await page.locator(".lu-msg__part", { hasText: "The new file" }).getByRole("button", { name: "Download new.bin" }).waitFor({ timeout: 30_000 });
    phase("let go");
    try {
      await row.getByText("No longer available").waitFor({ timeout: 15_000 });
    } catch {
      throw new Error(`the old file's row did not say it was let go; it reads ${JSON.stringify((await row.textContent())?.trim())}`);
    }
    if ((await row.getByRole("button", { name: "Download old.bin" }).count()) > 0) throw new Error("a file the server let go is still offered to download");
    if ((await row.getAttribute("data-drawn-once")) !== "yes") throw new Error("the row was made again rather than told");
    await page.screenshot({ path: `${out}/${which}-expired.png` });
    console.log(`  ${which}: a file on screen was let go by a full server making room, and its row said "No longer available" with nothing to download, in ${Date.now() - t0} ms`);
  } finally {
    await ctx.close();
  }
}

// Long messages, code blocks and long pastes (#18): a message over 14 lines
// shows 10 and opens with Show more; a code block keeps its long line on one
// line, scrolling sideways, and copies its text; a pasted draft keeps its
// lines; a draft over the instance's 16,384 bytes is counted and not sent,
// and one at the limit is.
async function longStep(which: string, stamp: number) {
  const ownerDir = resolve(work, `cli-long-owner-${which}`);
  const owner = `pastes${which}${stamp}`;
  const reader = `reads${which}${stamp}`;
  lumia(ownerDir, "instance", "add", instance);
  lumia(ownerDir, "account", "create", "--handle", owner, "--password", password);
  const server = lumia(ownerDir, "server-create", `Long ${stamp}`).match(/[0-9a-f-]{36}/)?.[0];
  if (!server) throw new Error("no server id");
  const general = lumia(ownerDir, "channel-create", server, "general").match(/[0-9a-f-]{36}/)?.[0];
  if (!general) throw new Error("no channel id");
  const code = lumia(ownerDir, "invite-create", server).trim().split(/\s+/)[0];
  if (!code) throw new Error("no invite code");
  const readerDir = resolve(work, `cli-long-reader-${which}`);
  lumia(readerDir, "instance", "add", instance);
  lumia(readerDir, "account", "create", "--handle", reader, "--password", password);
  lumia(readerDir, "join", code);
  const lines = Array.from({ length: 30 }, (_, i) => `Release note ${i + 1} of 30.`).join("\n");
  lumia(ownerDir, "send", general, lines);
  const wide = `let message = format!("{}: ${"the loader could not relocate the section ".repeat(6)}", name);`;
  lumia(ownerDir, "send", general, "The line that fails:\n\n```rust\n" + wide + "\nassert!(message.len() < 120);\n```");
  const { ctx, page } = await open(which, "long");
  if (which === "chromium") await ctx.grantPermissions(["clipboard-read", "clipboard-write"]);
  const t0 = Date.now();
  try {
    await trustCard(page);
    await page.getByLabel("Email or handle").fill(reader);
    await page.getByRole("textbox", { name: "Password" }).fill(password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await later(page);
    const composer = page.getByPlaceholder("Message #general");
    await composer.waitFor();
    const part = (text: string) => page.locator(".lu-msg__part", { has: page.locator(".lu-msg__body", { hasText: text }) });
    phase("collapsed");
    const notes = part("Release note 1 of 30.");
    const more = notes.getByRole("button", { name: "Show more" });
    await more.waitFor();
    if ((await more.getAttribute("aria-expanded")) !== "false") throw new Error("Show more does not say it is collapsed");
    const frame = notes.locator(".lu-body__frame");
    const line = await frame.evaluate((el) => parseFloat(getComputedStyle(el).lineHeight));
    const folded = (await frame.boundingBox())?.height ?? 0;
    if (Math.abs(folded - 10 * line) > 2) throw new Error(`a folded body is ${folded} px, not ten lines of ${line}`);
    await page.screenshot({ path: `${out}/${which}-long-folded.png` });
    await more.click();
    const less = notes.getByRole("button", { name: "Show less" });
    await less.waitFor();
    const opened = (await frame.boundingBox())?.height ?? 0;
    if (opened < 29 * line) throw new Error(`an opened body is ${opened} px, under thirty lines of ${line}`);
    phase("code");
    const block = part("The line that fails:").locator(".lu-body__code");
    const pre = block.locator(".lu-body__pre");
    const box = await pre.evaluate((el) => ({ sw: el.scrollWidth, cw: el.clientWidth, sh: el.scrollHeight, ch: el.clientHeight, text: el.textContent ?? "" }));
    if (box.sw <= box.cw) throw new Error(`the long line wrapped: ${box.sw} wide in ${box.cw}`);
    if (box.sh > box.ch) throw new Error(`a code block scrolls up and down: ${box.sh} in ${box.ch}`);
    if (!box.text.includes(wide)) throw new Error("the code block's line is not as typed");
    await block.hover();
    const copy = block.getByRole("button", { name: "Copy code" });
    await copy.click();
    await copy.getByText("Copied").waitFor();
    if (which === "chromium") {
      const held = await page.evaluate(() => navigator.clipboard.readText());
      if (held !== `${wide}\nassert!(message.len() < 120);`) throw new Error(`Copy wrote ${JSON.stringify(held)}`);
    }
    await page.screenshot({ path: `${out}/${which}-long-code.png` });
    phase("paste");
    await composer.fill("one\ntwo\nthree");
    await composer.press("Enter");
    const pasted = part("one").locator(".lu-body__para").first();
    await pasted.waitFor();
    if ((await pasted.innerText()) !== "one\ntwo\nthree") throw new Error(`a pasted draft lost its lines: ${JSON.stringify(await pasted.innerText())}`);
    phase("limit");
    const room = page.locator(".lu-draftroom");
    const send = page.getByRole("button", { name: "Send message" });
    await composer.fill("x".repeat(16_385));
    await room.filter({ hasText: "1 character too long" }).waitFor();
    if (!(await send.isDisabled())) throw new Error("Send is offered over the limit");
    await page.screenshot({ path: `${out}/${which}-long-over.png` });
    await composer.press("Enter");
    if ((await composer.inputValue()).length !== 16_385) throw new Error("Return over the limit took the draft");
    await composer.fill("y".repeat(16_384));
    await room.filter({ hasText: "0 characters left" }).waitFor();
    await composer.press("Enter");
    await page.waitForFunction(() => (document.querySelector('textarea[placeholder="Message #general"]') as HTMLTextAreaElement | null)?.value === "");
    await part("yyyyyyyyyy").getByRole("button", { name: "Show more" }).waitFor();
  } finally {
    await ctx.close();
  }
  console.log(`  ${which}: folded thirty lines to ten and opened them, kept a code block's long line on one line and copied it, kept a paste's lines, refused a draft a character over the limit and sent one at it, in ${Date.now() - t0} ms`);
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
    await later(page);
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

// This browser removed from the identity by another device (identity spec
// 4.4): linked another way to a CLI identity of its own, then revoked by
// the CLI. The core erases what the browser kept and ends the loop saying
// so; the page goes to the sign-in card with the notice, which dismisses,
// and signing in again works, as a new device.
async function removed(which: string, stamp: number) {
  const dir = resolve(work, `cli-removed-${which}`);
  const handle = `removed${which}${stamp}`;
  lumia(dir, "instance", "add", instance);
  lumia(dir, "account", "create", "--handle", handle, "--password", password);
  lumia(dir, "identity", "create", "--label", "desk");
  lumia(dir, "identity", "link-account", "--password", password);
  const { ctx, page } = await open(which, "removed");
  const t0 = Date.now();
  try {
    await trustCard(page);
    await page.getByRole("button", { name: "Link this browser to your identity" }).click();
    await page.getByRole("button", { name: "Link another way" }).click();
    await page.getByRole("button", { name: "Start linking" }).click();
    const request = await page.getByLabel("Link request").inputValue();
    const code = lumia(dir, "identity", "approve", request).match(/code (\S+)/)?.[1];
    if (!code) throw new Error("the CLI printed no claim code");
    await page.getByLabel("Code from the approving device").fill(code);
    await page.getByRole("button", { name: "Claim and sign in" }).click();
    phase("connected");
    await page.locator('.lu-rail__self[title$="connected"]').waitFor();
    phase("revoke");
    const web = lumia(dir, "identity", "show")
      .split("\n")
      .find((line) => /^\s*device\b/.test(line) && /\bWeb\b/.test(line))
      ?.match(/device (\S+)/)?.[1];
    if (!web) throw new Error("no web device in the CLI's roster");
    lumia(dir, "identity", "revoke", web, "--password", password);
    phase("sent back");
    await page.waitForURL(/\/sign-in/);
    const notice = page.getByText("This browser was removed from your account, and its private messages were erased from it.");
    await notice.waitFor();
    await page.screenshot({ path: `${out}/${which}-removed.png` });
    await page.getByRole("button", { name: "Dismiss" }).click();
    await notice.waitFor({ state: "detached" });
    phase("sign in again");
    await trustCard(page);
    await page.getByLabel("Email or handle").fill(handle);
    await page.getByRole("textbox", { name: "Password" }).fill(password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await page.waitForURL(/\/app/);
  } finally {
    await ctx.close();
  }
  console.log(`  ${which}: removed by the CLI's identity, the browser went back to sign-in and said its private messages were erased, and signed in again, in ${Date.now() - t0} ms`);
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
    await later(page);
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
// `LUMIA_E2E_PUSH_PORT` moves it, for a second suite run beside the first.
const pushPort = Number(process.env.LUMIA_E2E_PUSH_PORT ?? 4919);
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
    await later(page);
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
    await later(page);
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
    await later(page);
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
    await later(page);
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
    await later(page);
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
// `LUMIA_E2E_OIDC_PORT` moves it, as `LUMIA_E2E_PUSH_PORT` does the push service.
const oidcPort = Number(process.env.LUMIA_E2E_OIDC_PORT ?? 4920);
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
      await later(page);
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
      ["link prompt", () => linkPrompt(which, `link${stamp}`)],
      ["link", () => link(which, identityDir)],
      ["dm", () => dm(which, aDir, bAccount, aHandle)],
      ["invite", () => invite(which, passwordDir, server)],
      ["private", () => privateStep(which, stamp)],
      ["create", () => create(which, `web${stamp}`, stamp)],
      ["revoked", () => revoked(which, stamp)],
      ["removed", () => removed(which, stamp)],
      ["account", () => account(which, `web${stamp}`, stamp)],
      ["preferences", () => preferencesStep(which, stamp)],
      ["server settings", () => serverSettings(which, stamp)],
      ["roles", () => rolesStep(which, stamp)],
      ["autoscroll", () => autoscrollStep(which, stamp)],
      ["aside", () => asideStep(which, stamp)],
      ["rail", () => railStep(which, stamp)],
      ["membership", () => membershipStep(which, stamp)],
      ["replies", () => repliesStep(which, stamp)],
      ["drafts", () => draftsStep(which, stamp)],
      ["editing", () => editingStep(which, stamp)],
      ["menus", () => menusStep(which, stamp)],
      ["profile", () => profileStep(which, stamp)],
      ["media", () => mediaStep(which, stamp)],
      ["expired", () => expiredStep(which, stamp)],
      ["long", () => longStep(which, stamp)],
      ["history", () => historyStep(which, stamp)],
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
        // What the page says of itself while a step waits: how long its document has been
        // loaded, whether it has the focus and is visible, and where the keyboard is.
        void live.page
          ?.evaluate(() => {
            const a = document.activeElement;
            const where = a ? `${a.tagName.toLowerCase()}${a.getAttribute("aria-label") ? `[${a.getAttribute("aria-label")}]` : a.getAttribute("placeholder") ? `[${a.getAttribute("placeholder")}]` : ""}` : "nothing";
            return `document loaded ${Math.round(performance.now())} ms ago, ${document.hasFocus() ? "focused" : "not focused"}, ${document.visibilityState}, the keyboard in ${where}`;
          })
          .then((said) => console.log(`  [${which}] ${name}: ${said}`))
          .catch(() => {});
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
} catch (e) {
  // Before any step (the UI not answering, say): said, and a failure, since
  // the exit below would otherwise end the process quietly with 0.
  console.log(e instanceof Error ? e.message : String(e));
  process.exitCode = 1;
} finally {
  vite?.kill();
  pushService.stop(true);
  oidcService.stop(true);
  // A browser that went away mid-step leaves calls that never settle;
  // they must not hold the process (and a CI job) open after the report.
  process.exit();
}
