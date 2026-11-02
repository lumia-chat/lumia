// Generates src/design/tokens.css from assets/design/tokens.json: every
// token as a CSS variable named after it, colours per theme, the font
// faces from the font files, and the families. Run by every build.
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

type Token = { name: string; value: string | Record<string, string>; usage?: string };
type Family = { tokens: Token[] };
type Tokens = {
  color: { themes: { id: string }[]; tokens: Token[] };
  type: {
    fonts: { family: string; file: string; weight: string; style?: string }[];
    families: Record<string, string>;
    groups: { name: string; family: string; styles: Record<string, unknown>[] }[];
  };
  spacing: Family;
  radius: Family;
  size: Family;
  shadow: Family;
};

const root = resolve(import.meta.dir, "..");
const tokens: Tokens = JSON.parse(readFileSync(resolve(root, "../../assets/design/tokens.json"), "utf8"));
const themes = tokens.color.themes.map((t) => t.id);
const first = themes[0] ?? "light";

function value(token: Token, theme: string): string {
  if (typeof token.value === "string") return token.value;
  return token.value[theme] ?? token.value[first] ?? "";
}

const lines: string[] = ["/* Generated from assets/design/tokens.json by scripts/tokens.ts; do not edit. */"];
for (const font of tokens.type.fonts) {
  lines.push(
    `@font-face { font-family: "${font.family}"; src: url("/${font.file}") format("woff2"); font-weight: ${font.weight}; font-style: ${font.style ?? "normal"}; font-display: swap; }`,
  );
}
const flat = [tokens.spacing, tokens.radius, tokens.size, tokens.shadow].flatMap((f) => f.tokens);
lines.push(":root {");
for (const [name, stack] of Object.entries(tokens.type.families)) lines.push(`  --font-${name}: ${stack};`);
for (const token of flat) lines.push(`  --${token.name}: ${value(token, first)};`);
for (const token of tokens.color.tokens) lines.push(`  --${token.name}: ${value(token, first)};`);
lines.push("}");
for (const theme of themes.slice(1)) {
  lines.push(`:root[data-theme="${theme}"] {`);
  for (const token of tokens.color.tokens) lines.push(`  --${token.name}: ${value(token, theme)};`);
  lines.push("}");
}
// Text styles as classes, for the screens that set type outside a component.
for (const group of tokens.type.groups) {
  for (const style of group.styles) {
    const name = String(style.name);
    const decl: string[] = [`font-family: var(--font-${group.family})`];
    for (const [key, css] of [["fontSize", "font-size"], ["lineHeight", "line-height"], ["fontWeight", "font-weight"], ["letterSpacing", "letter-spacing"]] as const) {
      if (style[key] !== undefined) decl.push(`${css}: ${style[key]}`);
    }
    lines.push(`.lu-${name} { ${decl.join("; ")}; }`);
  }
}
writeFileSync(resolve(root, "src/design/tokens.css"), lines.join("\n") + "\n");
console.log(`tokens.css: ${tokens.color.tokens.length} colours in ${themes.length} themes, ${flat.length} other tokens, ${tokens.type.fonts.length} font faces`);
