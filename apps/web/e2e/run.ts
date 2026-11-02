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

// The page of the step in progress, for a screenshot when it fails.
const live: { page: Page | null } = { page: null };

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
  if (!keep) {
    // WebKit keeps OPFS outside the profile directory, so a fresh profile
    // still sees the last run's stores; clear them before the worker opens
    // any. Best effort and bounded: an engine without OPFS iteration must
    // not hang the suite here, it fails later where the store opens.
    await page.goto(`${base}/design`);
    const cleared = page.evaluate(async () => {
      const root = await navigator.storage.getDirectory();
      for await (const name of (root as unknown as { keys(): AsyncIterable<string> }).keys()) await root.removeEntry(name, { recursive: true });
      localStorage.clear();
      return true;
    });
    const outcome = await Promise.race([cleared.catch((e: unknown) => `failed: ${e instanceof Error ? e.message : String(e)}`), Bun.sleep(15_000).then(() => "timed out")]);
    if (outcome !== true) console.log(`  [${which}] clearing storage ${outcome}`);
  }
  return { ctx, page };
}

async function trustCard(page: Page) {
  await page.goto(base);
  await page.locator(".lu-card").waitFor();
  // A UI served by its instance has no address to ask for.
  const field = page.getByLabel("Instance");
  if ((await field.count()) > 0) {
    await field.fill(instance);
    await page.getByRole("button", { name: "Continue" }).click();
  }
  await page.getByText("run by").waitFor();
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
    await page.context().grantPermissions(["notifications"]);
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
    await page.screenshot({ path: `${out}/${which}-dm-failed.png` });
    const alerts = await page.getByRole("alert").allTextContents();
    if (alerts.length) console.log(`  [${which}] on screen: ${alerts.join(" | ")}`);
    throw e;
  } finally {
    await ctx.close();
  }
  console.log(`  ${which}: personal DM both ways and back after reload in ${Date.now() - t0} ms`);
}

async function dmSteps(which: string, page: Page, aDir: string, bAccount: string, aHandle: string) {
  lumia(aDir, "dm", "request", bAccount);
  await page.goto(`${base}/app?place=home&view=requests`);
  const accept = page.getByRole("button", { name: "Accept" });
  await accept.waitFor();
  // The sender's handle comes from the public profile, visible while the request waits.
  await page.getByText(`@${aHandle}`).waitFor();
  await page.waitForTimeout(1500); // the gateway loop starts after the snapshot
  await accept.click();
  const composer = page.getByPlaceholder(/^Message /);
  await composer.waitFor();
  await page.screenshot({ path: `${out}/${which}-dm-open.png` });
  const hello = `hello from ${which} ${Date.now()}`;
  await composer.fill(hello);
  await page.keyboard.press("Enter");
  await page.getByText(hello).waitFor();
  const opened = lumia(aDir, "dm", "open", bAccount);
  const channel = opened.match(/([0-9a-f-]{36})\s+personal DM/)?.[1];
  if (!channel) throw new Error(`no channel from dm open:\n${opened}`);
  // The CLI joined from the welcome; the browser's message lands in its
  // store once its tail has applied the channel log, which takes a moment.
  let seen = "";
  for (let i = 0; i < 20 && !seen.includes(hello); i++) {
    if (i > 0) await Bun.sleep(1_500);
    seen = lumia(aDir, "dm", "messages", channel);
  }
  if (!seen.includes(hello)) throw new Error(`the CLI did not read the browser's message:\n${seen}`);
  const reply = `reply from the cli ${Date.now()}`;
  lumia(aDir, "dm", "send", channel, reply);
  await page.getByText(reply).waitFor();
  await page.reload();
  await page.getByText(hello).waitFor();
  await page.getByText(reply).waitFor();
  if ((await page.getByText(aHandle).count()) === 0) throw new Error("the CLI's handle is not shown as the sender");
  await page.screenshot({ path: `${out}/${which}-dm.png` });
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
lumia(passwordDir, "channel-create", server, "general");
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
      ["link", () => link(which, identityDir)],
      ["dm", () => dm(which, aDir, bAccount, aHandle)],
    ] as const) {
      // A heartbeat while the step runs (CI shows a step's log only once it
      // ends) and a hard bound, so a hang names itself instead of the job
      // timing out.
      const started = Date.now();
      const beat = setInterval(() => console.log(`  [${which}] ${name}: ${Math.round((Date.now() - started) / 1000)} s, at ${live.page?.url() ?? "no page"}`), 30_000);
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
}
