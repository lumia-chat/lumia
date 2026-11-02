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
const base = "http://localhost:5173";
const instance = process.env.LUMIA_INSTANCE ?? "http://localhost:4000";
const password = "correct-horse-battery-staple";
const browsers = process.argv.slice(2).filter((a) => a === "chromium" || a === "webkit");
const engines = browsers.length ? browsers : ["chromium", "webkit"];

function run(args: string[], cwd = web): string {
  const r = Bun.spawnSync(args, { cwd });
  const text = r.stdout.toString() + r.stderr.toString();
  if (r.exitCode !== 0) throw new Error(`${args.join(" ")}\n${text}`);
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

async function open(which: string, name: string, keep = false): Promise<{ ctx: BrowserContext; page: Page }> {
  const profile = resolve(work, `profile-${name}-${which}`);
  if (!keep) rmSync(profile, { recursive: true, force: true });
  const engine = which === "webkit" ? webkit : chromium;
  const ctx = await engine.launchPersistentContext(profile, { viewport: { width: 1440, height: 960 } });
  const page = await ctx.newPage();
  page.on("pageerror", (e) => console.log(`  [${which}] page error: ${e.message}`));
  page.on("console", (m) => {
    if (m.type() === "error") console.log(`  [${which}] console: ${m.text()}`);
  });
  if (!keep) {
    // WebKit keeps OPFS outside the profile directory, so a fresh profile
    // still sees the last run's stores; clear them before the worker opens any.
    await page.goto(`${base}/design`);
    await page.evaluate(async () => {
      const root = await navigator.storage.getDirectory();
      for await (const name of (root as unknown as { keys(): AsyncIterable<string> }).keys()) await root.removeEntry(name, { recursive: true });
    });
    await page.evaluate(() => localStorage.clear());
  }
  return { ctx, page };
}

async function trustCard(page: Page) {
  await page.goto(base);
  await page.getByLabel("Instance").fill(instance);
  await page.getByRole("button", { name: "Continue" }).click();
  await page.getByText("run by").waitFor();
}

// Sign in by password, send a message, reload, read it back.
async function signInAndSend(which: string, handle: string) {
  const { ctx, page } = await open(which, "sign-in");
  const t0 = Date.now();
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
  await ctx.close();
  console.log(`  ${which}: signed in, sent, reloaded in ${Date.now() - t0} ms`);
}

// Link this browser as a web device: the CLI approves, the browser claims
// and signs in by identity proof.
async function link(which: string, dataDir: string) {
  const { ctx, page } = await open(which, "link");
  const t0 = Date.now();
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
  await ctx.close();
  const roster = lumia(dataDir, "identity", "show");
  if (!/Web/.test(roster)) throw new Error(`no web device in the roster:\n${roster}`);
  console.log(`  ${which}: linked and signed in by identity proof in ${Date.now() - t0} ms`);
}

// A personal DM between the linked browser (B) and the CLI (A): A asks, B
// accepts in the browser and creates the group, both send, B reloads.
async function dm(which: string, aDir: string, bAccount: string) {
  const { ctx, page } = await open(which, "link", true);
  const t0 = Date.now();
  try {
    await dmSteps(which, page, aDir, bAccount);
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

async function dmSteps(which: string, page: Page, aDir: string, bAccount: string) {
  lumia(aDir, "dm", "request", bAccount);
  await page.goto(`${base}/app?place=home&view=requests`);
  const accept = page.getByRole("button", { name: "Accept" });
  await accept.waitFor();
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
  const seen = lumia(aDir, "dm", "messages", channel);
  if (!seen.includes(hello)) throw new Error(`the CLI did not read the browser's message:\n${seen}`);
  const reply = `reply from the cli ${Date.now()}`;
  lumia(aDir, "dm", "send", channel, reply);
  await page.getByText(reply).waitFor();
  await page.reload();
  await page.getByText(hello).waitFor();
  await page.getByText(reply).waitFor();
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

const vite = Bun.spawn(["bun", "run", "dev"], { cwd: web, stdout: "ignore", stderr: "ignore" });
try {
  await waitFor(base, "vite");
  let failed = 0;
  for (const which of engines) {
    // A fresh CLI identity per browser, so each gets its own request.
    const aDir = resolve(work, `cli-a-${which}`);
    lumia(aDir, "instance", "add", instance);
    lumia(aDir, "account", "create", "--handle", `a${which}${stamp}`, "--password", password);
    lumia(aDir, "identity", "create", "--label", "desk");
    lumia(aDir, "identity", "link-account", "--password", password);
    lumia(aDir, "dm", "setup");
    for (const [name, step] of [
      ["sign in", () => signInAndSend(which, `web${stamp}`)],
      ["link", () => link(which, identityDir)],
      ["dm", () => dm(which, aDir, bAccount)],
    ] as const) {
      try {
        await step();
      } catch (e) {
        failed++;
        console.log(`  ${which}: ${name} FAILED: ${e instanceof Error ? e.message.split("\n")[0] : String(e)}`);
      }
    }
  }
  console.log(failed ? `${failed} failed; screenshots in ${out}` : "ok");
  process.exitCode = failed ? 1 : 0;
} finally {
  vite.kill();
}
