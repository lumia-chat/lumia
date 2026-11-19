// Draws boards of the gallery (`/design`) as artboards for the design canvas,
// where Ryan looks at a design and edits it. Each part of a board (a window,
// a figure with its caption, a phone) becomes one `.dc.html`, light and
// dark, whose elements carry what the stylesheets gave them as inline styles
// with the tokens resolved: the canvas's panel edits inline styles, and an
// artboard has no stylesheet of ours.
//
//   bun run scripts/canvas.ts scripts/canvas-roles.json <out>
//
// Writes <out>/project/<Stem><Theme>.dc.html and <out>/parts.json (file,
// title, size, caption and the board's lead for each), and checks every
// artboard: drawn as plain HTML, each element must sit where the gallery has
// it. Placing the frames in the canvas's index and publishing them is the
// session's, with the Artifact tool, on the index as it is then.
//
// What it does not carry: hover and focus styles (no element is in either
// here), transitions, and pictures other than the rail's logo, which is the
// canvas's own upload.
import { spawn } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { chromium } from "playwright";

type Spec = {
  /** The parts of a board, in the order the gallery draws them. One inside another is not a part. */
  parts: string;
  boards: { tab: string; parts: [stem: string, title: string][] }[];
};
type Tokens = { bg: string; accent: string; muted: string; link: string };
type Part = { kind: string; w: number; h: number; pad: number; base: string; html: string; rects: number[][]; caption: string };

const [specPath, out] = process.argv.slice(2);
if (!specPath || !out) throw new Error("usage: canvas.ts <spec.json> <out>");
const spec = JSON.parse(readFileSync(specPath, "utf8")) as Spec;
mkdirSync(`${out}/project`, { recursive: true });
const port = 5197;

/** The rail's logo as the canvas holds it. */
const LOGO = "/_blob/3cf5d4b87ae32b8eac7378eb5020dc37";
const FONTS = "https://fonts.googleapis.com/css2?family=Albert+Sans:wght@400;500;600;700;800&amp;family=Bricolage+Grotesque:wght@600;700;800&amp;display=swap";

// Runs in the page: each part's markup with its styles inline.
function inPage(arg: { logo: string; parts: string }) {
  type Decl = { prop: string; value: string; important: boolean };

  const split = (text: string, sep: string): string[] => {
    const found: string[] = [];
    let depth = 0;
    let quote = "";
    let cur = "";
    for (const ch of text) {
      if (quote) {
        if (ch === quote) quote = "";
      } else if (ch === '"' || ch === "'") quote = ch;
      else if (ch === "(" || ch === "[") depth++;
      else if (ch === ")" || ch === "]") depth--;
      if (ch === sep && depth === 0 && !quote) {
        found.push(cur);
        cur = "";
      } else cur += ch;
    }
    if (cur.trim()) found.push(cur);
    return found;
  };

  const declarations = (cssText: string): Decl[] => {
    const found: Decl[] = [];
    for (const d of split(cssText, ";")) {
      const at = d.indexOf(":");
      const prop = at < 0 ? "" : d.slice(0, at).trim();
      if (!prop) continue;
      const value = d.slice(at + 1).trim();
      const important = /!important\s*$/.test(value);
      found.push({ prop, value: important ? value.replace(/\s*!important\s*$/, "") : value, important });
    }
    return found;
  };

  // The browser spells `font: inherit` as its dozen longhands: the one word goes back.
  const FONT = /^font-(style|variant|variant-[\w-]+|weight|stretch|width|size|optical-sizing|size-adjust|kerning|feature-settings|variation-settings|language-override|palette|synthesis[\w-]*)$|^line-height$/;
  const short = (decls: Decl[]): Decl[] => {
    if (!["font-style", "font-weight", "font-stretch"].every((p) => decls.some((d) => d.prop === p && d.value === "inherit"))) return decls;
    const kept: Decl[] = [];
    let put = false;
    for (const d of decls) {
      if (d.value === "inherit" && FONT.test(d.prop)) {
        if (!put) kept.push({ prop: "font", value: "inherit", important: d.important });
        put = true;
      } else kept.push(d);
    }
    return kept;
  };

  const closing = (s: string, open: number): number => {
    let depth = 0;
    for (let i = open; i < s.length; i++) {
      if (s[i] === "(") depth++;
      else if (s[i] === ")" && --depth === 0) return i;
    }
    return s.length - 1;
  };

  const specificity = (selector: string): number => {
    let s = selector;
    // :where() counts for nothing.
    for (let at = s.indexOf(":where("); at >= 0; at = s.indexOf(":where(")) s = s.slice(0, at) + s.slice(closing(s, at + 6) + 1);
    s = s.replace(/:(not|is|has)\(/g, " (");
    let b = 0;
    s = s.replace(/\[[^\]]*\]/g, () => (b++, " "));
    s = s.replace(/::[\w-]+/g, " ");
    const a = (s.match(/#[\w-]+/g) ?? []).length;
    s = s.replace(/#[\w-]+/g, " ");
    b += (s.match(/\.[\w-]+/g) ?? []).length;
    s = s.replace(/\.[\w-]+/g, " ");
    b += (s.match(/:[\w-]+(\([^)]*\))?/g) ?? []).length;
    s = s.replace(/:[\w-]+(\([^)]*\))?/g, " ");
    const c = (s.match(/[a-zA-Z][\w-]*/g) ?? []).length;
    return a * 10000 + b * 100 + c;
  };

  // A rule inside a container query holds for an element whose container is that wide.
  type Within = { min: number; max: number } | null;
  type Rule = { selector: string; pseudo: string; decls: Decl[]; spec: number; order: number; within: Within };
  const rules: Rule[] = [];
  const walk = (list: CSSRuleList, within: Within) => {
    for (const r of Array.from(list)) {
      if (r instanceof CSSStyleRule) {
        const decls = short(declarations(r.style.cssText));
        for (const sel of split(r.selectorText, ",")) {
          const full = sel.trim();
          const m = full.match(/::([\w-]+)$/);
          const base = m ? full.slice(0, m.index).trim() || "*" : full;
          if (base.includes("::")) continue;
          rules.push({ selector: base, pseudo: m?.[1] ?? "", decls, spec: specificity(full), order: rules.length, within });
        }
      } else if (r instanceof CSSMediaRule) {
        if (matchMedia(r.conditionText).matches) walk(r.cssRules, within);
      } else if (r.constructor.name === "CSSContainerRule") {
        const text = (r as CSSConditionRule).conditionText;
        const min = text.match(/min-width:\s*([\d.]+)px/);
        const max = text.match(/max-width:\s*([\d.]+)px/);
        if (!min && !max) throw new Error(`a container query this does not read: ${text}`);
        walk((r as CSSGroupingRule).cssRules, { min: Number(min?.[1] ?? 0), max: Number(max?.[1] ?? Infinity) });
      } else if (!(r instanceof CSSKeyframesRule) && (r as CSSGroupingRule).cssRules) walk((r as CSSGroupingRule).cssRules, within);
    }
  };
  for (const sheet of Array.from(document.styleSheets)) {
    try {
      walk(sheet.cssRules, null);
    } catch {
      // A sheet from elsewhere: none of the boards' rules.
    }
  }

  const resolve = (value: string, cs: CSSStyleDeclaration): string => {
    let v = value;
    for (let guard = 0; guard < 50; guard++) {
      const at = v.indexOf("var(");
      if (at < 0) break;
      const end = closing(v, at + 3);
      const inner = v.slice(at + 4, end);
      const comma = inner.indexOf(",");
      const name = (comma < 0 ? inner : inner.slice(0, comma)).trim();
      const fallback = comma < 0 ? "" : inner.slice(comma + 1).trim();
      v = v.slice(0, at) + (cs.getPropertyValue(name).trim() || fallback) + v.slice(end + 1);
    }
    return v;
  };

  const DROP = /^(--|transition|animation|will-change|cursor$|user-select$|-webkit-user-select$|-webkit-tap-highlight-color$|-webkit-font-smoothing$|-moz-osx-font-smoothing$)/;

  /** What the cascade gives an element (or its `::before`, `::after`), then its own `style`, as one inline style. */
  const styleOf = (el: Element, pseudo: string, own: string | null): string => {
    const cs = getComputedStyle(el, pseudo ? `::${pseudo}` : undefined);
    let held: number | null | undefined;
    const holder = (): number | null => {
      if (held !== undefined) return held;
      held = null;
      for (let a = el.parentElement; a; a = a.parentElement) {
        const s = getComputedStyle(a);
        if (s.containerType && s.containerType !== "normal") {
          held = a.clientWidth - parseFloat(s.paddingLeft) - parseFloat(s.paddingRight);
          break;
        }
      }
      return held;
    };
    const found: (Decl & { spec: number; order: number })[] = [];
    for (const r of rules) {
      if (r.pseudo !== pseudo) continue;
      if (r.within) {
        const w = holder();
        if (w === null || w < r.within.min || w > r.within.max) continue;
      }
      let ok = false;
      try {
        ok = el.matches(r.selector);
      } catch {
        ok = false;
      }
      if (ok) for (const d of r.decls) found.push({ ...d, spec: r.spec, order: r.order });
    }
    found.sort((x, y) => x.spec - y.spec || x.order - y.order);
    const inline = own ? short(declarations(own)) : [];
    // In the order a later one wins: so a shorthand and a longhand of it stay right written one after the other.
    const all: Decl[] = [...found.filter((d) => !d.important), ...inline.filter((d) => !d.important), ...found.filter((d) => d.important), ...inline.filter((d) => d.important)];
    const map = new Map<string, string>();
    for (const d of all) {
      if (DROP.test(d.prop)) continue;
      const value = resolve(d.value, cs).replace(/"/g, "'");
      if (!value) continue;
      map.delete(d.prop);
      map.set(d.prop, value);
    }
    // An artboard's sheet makes every box a border box; one that is not says so.
    if (!pseudo && cs.boxSizing === "content-box" && el instanceof HTMLElement) map.set("box-sizing", "content-box");
    else if (map.get("box-sizing") === "border-box") map.delete("box-sizing");
    return Array.from(map, ([p, v]) => `${p}: ${v}`).join("; ");
  };

  const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/\{\{/g, "{&#8203;{");
  const attr = (s: string) => s.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;");
  const VOID = new Set(["img", "input", "br", "hr", "wbr"]);
  const SKIP = /^(class|style|data-|on|draggable$|decoding$|loading$|autocomplete$|spellcheck$|autocapitalize$|autocorrect$|enterkeyhint$|tabindex$|maxlength$|inputmode$)/;
  const problems: string[] = [];

  /** A `::before` or `::after` with content, as the element it is drawn like. */
  const pseudoSpan = (el: Element, which: "before" | "after"): string => {
    const content = getComputedStyle(el, `::${which}`).content;
    if (!content || content === "none" || content === "normal") return "";
    const style = styleOf(el, which, null)
      .split("; ")
      .filter((d) => !d.startsWith("content:"))
      .join("; ");
    return `<span aria-hidden="true" style="${attr(style)}">${esc(content.replace(/^"(.*)"$/, "$1"))}</span>`;
  };

  /** Whether a line may be broken before an element without a space showing. */
  const blockish = (el: Element): boolean => {
    if (!(el instanceof HTMLElement)) return false;
    const parent = el.parentElement ? getComputedStyle(el.parentElement) : null;
    if (parent && /^pre/.test(parent.whiteSpace)) return false;
    if (parent && /flex|grid/.test(parent.display)) return true;
    const d = getComputedStyle(el).display;
    return !/^inline/.test(d) && d !== "contents";
  };

  // Where each element drawn sits in its part: what the artboard is checked against.
  let rects: number[][] = [];
  let origin = { x: 0, y: 0 };
  const draw = (el: Element, over?: string): string => {
    const svg = el instanceof SVGElement;
    const tag = svg ? el.localName : el.tagName.toLowerCase();
    if (el instanceof HTMLElement && getComputedStyle(el).display === "none") return "";
    if (tag === "script" || tag === "style") return "";
    let attrs = "";
    if (el instanceof HTMLElement) {
      const r = el.getBoundingClientRect();
      attrs += ` data-g="${rects.length}"`;
      rects.push([r.x - origin.x, r.y - origin.y, r.width, r.height].map((n) => Math.round(n * 10) / 10));
    }
    const field = el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement;
    for (const a of Array.from(el.attributes)) {
      if (SKIP.test(a.name)) continue;
      if (field && (a.name === "value" || a.name === "checked")) continue;
      let value = a.value;
      if (tag === "img" && a.name === "src") {
        if (/logo|icon/.test(value)) value = arg.logo;
        else problems.push(`a picture the canvas has no upload for: ${value.slice(0, 60)}`);
      }
      if (tag === "a" && a.name === "href" && !/^(#|https?:)/.test(value)) value = "#";
      attrs += ` ${a.name}="${attr(value)}"`;
    }
    if (el instanceof HTMLInputElement) {
      if (el.type === "checkbox" || el.type === "radio") attrs += el.checked ? ` checked=""` : "";
      else if (el.value) attrs += ` value="${attr(el.value)}"`;
    }
    const style = styleOf(el, "", [el.getAttribute("style"), over].filter(Boolean).join("; ") || null);
    if (style) attrs += ` style="${attr(style)}"`;
    const lead = blockish(el) ? "\n" : "";
    if (VOID.has(tag)) return `${lead}<${tag}${attrs}>`;
    let inner = svg ? "" : pseudoSpan(el, "before");
    if (el instanceof HTMLTextAreaElement) inner += esc(el.value);
    else
      for (const node of Array.from(el.childNodes)) {
        if (node.nodeType === Node.TEXT_NODE) inner += esc(node.textContent ?? "");
        else if (node.nodeType === Node.ELEMENT_NODE) inner += draw(node as Element);
      }
    if (!svg) inner += pseudoSpan(el, "after");
    return `${lead}<${tag}${attrs}>${inner}${inner.includes("\n") && lead ? "\n" : ""}</${tag}>`;
  };

  const board = document.querySelector(".gallery__board");
  if (!board) throw new Error("no board is drawn");
  const parts = Array.from(board.querySelectorAll<HTMLElement>(arg.parts)).filter((el) => !el.parentElement?.closest(arg.parts));
  const root = getComputedStyle(document.documentElement);
  const tokens = { bg: root.getPropertyValue("--bg").trim(), accent: root.getPropertyValue("--accent").trim(), muted: root.getPropertyValue("--ink-muted").trim(), link: root.getPropertyValue("--link").trim() };
  const leads = Array.from(board.querySelectorAll(".gallery-roles__lead"), (p) => p.textContent ?? "");
  const drawn = parts.map((el) => {
    // What a part inherits from the page around it, said on the artboard's root. The line height is a ratio, as the page's is.
    const around = getComputedStyle(el.parentElement ?? document.body);
    const base = `color: ${around.color}; font-family: ${getComputedStyle(document.body).fontFamily.replace(/"/g, "'")}; font-size: ${around.fontSize}; line-height: ${Math.round((parseFloat(around.lineHeight) / parseFloat(around.fontSize)) * 1000) / 1000}`;
    rects = [];
    const phone = el.querySelector<HTMLElement>(":scope > .phone");
    if (phone) {
      // A phone is its screen, at the canvas's 390 by 844: the frame around it is the canvas's.
      const r = phone.getBoundingClientRect();
      origin = { x: r.x + 1, y: r.y + 1 };
      const html = draw(phone, "box-sizing: border-box; width: 390px; height: 844px; border: 0; border-radius: 0; box-shadow: none");
      return { kind: "phone", w: 390, h: 844, pad: 0, base, html, rects, caption: el.querySelector("figcaption")?.textContent ?? "" };
    }
    const r = el.getBoundingClientRect();
    origin = { x: r.x, y: r.y };
    const figure = el.tagName === "FIGURE";
    const pad = figure ? 24 : 0;
    // A figure keeps the width it has, whatever in the gallery gave it that.
    const html = draw(el, figure ? `width: ${Math.ceil(r.width)}px; max-width: none` : undefined);
    return { kind: figure ? "figure" : "window", w: Math.ceil(r.width) + pad * 2, h: Math.ceil(r.height) + pad * 2, pad, base, html, rects, caption: el.querySelector(":scope > figcaption")?.textContent ?? "" };
  });
  return { drawn, tokens, leads, problems };
}

function artboard(title: string, theme: string, tokens: Tokens, part: Part): string {
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Lumia — ${title.replace(/&/g, "&amp;")}</title>
<script src="./support.js"></script>
</head>
<body>
<x-dc>
<helmet>
<link rel="stylesheet" href="${FONTS}">
<style>
body{margin:0;background:${tokens.bg}}
*{box-sizing:border-box}
a{color:${tokens.link}}a:hover{color:${tokens.link}}
::placeholder{color:${tokens.muted};opacity:1}
button:focus-visible,input:focus-visible,a:focus-visible{outline:2px solid ${tokens.accent};outline-offset:2px}
</style>
</helmet>
<div style="width: ${part.w}px; height: ${part.h}px; box-sizing: border-box; padding: ${part.pad}px; background: ${tokens.bg}; color-scheme: ${theme}; ${part.base}">${part.html}
</div>
</x-dc>
<script type="text/x-dc" data-dc-script data-props='{"$preview":{"width":${part.w},"height":${part.h}}}'>
class Component extends DCLogic {
  renderVals() {
    return {};
  }
}
</script>
</body>
</html>
`;
}

// Runs in the check's page: the elements that are not where the gallery has them.
function moved(arg: { rects: number[][]; pad: number }): string[] {
  const bad: string[] = [];
  for (const el of Array.from(document.querySelectorAll<HTMLElement>("[data-g]"))) {
    const want = arg.rects[Number(el.dataset.g)] ?? [];
    const r = el.getBoundingClientRect();
    const got = [r.x - arg.pad, r.y - arg.pad, r.width, r.height];
    // Nothing of an element with no box is drawn, wherever it is said to be.
    if (!r.width && !r.height && !want[2] && !want[3]) continue;
    if (Math.max(...got.map((n, i) => Math.abs(n - (want[i] ?? 0)))) > 1.5) bad.push(`${el.tagName.toLowerCase()} "${(el.textContent ?? "").trim().slice(0, 24)}" at ${got.map((n) => Math.round(n)).join(",")}, in the gallery ${want.join(",")}`);
  }
  return bad;
}

const server = spawn("node_modules/.bin/vite", ["--port", String(port), "--strictPort"], { stdio: "ignore" });
const browser = await chromium.launch();
try {
  for (let i = 0; ; i++) {
    const up = await fetch(`http://localhost:${port}/design`).then(
      (r) => r.ok,
      () => false,
    );
    if (up) break;
    if (i > 100) throw new Error("the gallery did not start");
    await new Promise((r) => setTimeout(r, 200));
  }
  const gallery = await browser.newPage({ viewport: { width: 1440, height: 1000 }, deviceScaleFactor: 1 });
  const check = await browser.newPage({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1 });
  await gallery.goto(`http://localhost:${port}/design`, { waitUntil: "networkidle" });
  const index: unknown[] = [];
  let wrong = 0;
  for (const board of spec.boards) {
    await gallery.locator(".gallery__tab", { hasText: new RegExp(`^${board.tab}$`) }).click();
    for (const theme of ["Light", "Dark"]) {
      await gallery.locator(".gallery__tab", { hasText: new RegExp(`^${theme}$`) }).click();
      await gallery.evaluate(() => document.fonts.ready);
      await gallery.waitForTimeout(250);
      const got = await gallery.evaluate(inPage, { logo: LOGO, parts: spec.parts });
      if (got.drawn.length !== board.parts.length) throw new Error(`${board.tab}: ${got.drawn.length} parts drawn, ${board.parts.length} named`);
      for (const problem of got.problems) console.log(`${board.tab} · ${theme}: ${problem}`);
      wrong += got.problems.length;
      for (const [i, part] of got.drawn.entries()) {
        const [stem, name] = board.parts[i] ?? ["", ""];
        const file = `${stem}${theme}.dc.html`;
        const title = `${name} · ${theme}`;
        const html = artboard(title, theme.toLowerCase(), got.tokens, part);
        // The check: the artboard's own markup under its own sheet, the canvas's runtime aside.
        const helmet = html.match(/<helmet>([\s\S]*?)<\/helmet>/)?.[1] ?? "";
        const body = html.match(/<\/helmet>([\s\S]*?)<\/x-dc>/)?.[1] ?? "";
        await check.setContent(`<!doctype html><html lang="en"><head><meta charset="utf-8">${helmet}</head><body>${body}</body></html>`, { waitUntil: "networkidle" });
        await check.evaluate(() => document.fonts.ready);
        const bad = (await check.evaluate(moved, { rects: part.rects, pad: part.pad })).filter((line) => part.kind !== "phone" || !line.includes("in the gallery -1,-1,392,846"));
        for (const line of bad) console.log(`${file}: ${line}`);
        wrong += bad.length;
        writeFileSync(`${out}/project/${file}`, html.replace(/ data-g="\d+"/g, ""));
        index.push({ board: board.tab, theme, file, title, kind: part.kind, w: part.w, h: part.h, caption: part.caption, leads: got.leads });
      }
    }
  }
  writeFileSync(`${out}/parts.json`, JSON.stringify(index, null, 2));
  console.log(`${index.length} artboards in ${out}/project${wrong ? `, ${wrong} things to look at` : ", each element where the gallery has it"}`);
  if (wrong) process.exitCode = 1;
} finally {
  await browser.close();
  server.kill("SIGTERM");
}
