This is the Lumia web UI (architecture section 13): SolidJS 1.9 with Vite, TanStack Solid Router, Solid Query and Solid Virtual, bun as package manager and script runner. It is the UI for the browser and for the Tauri shell in `apps/desktop`. Read `../../docs/architecture.md` and `../../docs/plan.md` (step 6) before changing anything here.

## Layout

- `src/design/`: the design system. `components.tsx` is the port of the 36 components from the design artifact (same class names, markup and ARIA), `components.css` their stylesheet, `icons.ts` the 26 glyphs, and `tokens.css` is GENERATED from `../../assets/design/tokens.json` by `scripts/tokens.ts` on every build; never edit it, edit the tokens.
- `src/routes/`: the route tree (code-based TanStack router). `/design` is the gallery that rebuilds the design canvas's boards from the components with their sample data; it is how a change to the system is checked, in both themes.
- `../../assets` is the public root (`publicDir`): `/fonts`, `/brand`, `/icons`, `/design` resolve to the shared assets.
- Logic lives in the Rust core (`apps/core`, built to wasm and run in a worker); this app holds the UI and a thin bridge to that worker. Nothing here decides a protocol or storage question.

## Rules

- Solid, not React: props are read lazily (never destructured), `class` not `className`, `for` not `htmlFor`, `style` objects use CSS property names, lists are `<For>`, conditions `<Show>`.
- The design system's components are the vocabulary; a screen that needs a new element adds it to `components.tsx` with its CSS beside the others, named `lu-*`, and shows it in the gallery.
- Colours, spacing, radii, shadows and sizes come from tokens (`var(--…)`); no literal values in screen CSS.
- Sentence case everywhere, typographic punctuation, no emoji in chrome; the brand book in the design artifact and `assets/README.md` is the reference.
- `bun run check` (tokens, typecheck) and `bun run build` must pass; CI runs them.
