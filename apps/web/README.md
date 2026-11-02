# Lumia web

The web UI, also the UI of the desktop shell. SolidJS and Vite, bun.

```sh
bun install
bun run dev        # http://localhost:5173, /design is the component gallery
bun run build      # tokens, wasm, typecheck, vite build → dist/
bun run e2e        # the browser suite, against `mix phx.server` in ../instance
bun run build:instance   # the build an instance serves, into ../instance/priv/static/ui
```

`LUMIA_BASE=http://localhost:4000 bun run e2e` runs the suite against a UI the dev instance serves instead of starting vite.

The Rust core is built to wasm by `scripts/wasm.ts` (needs the wasm32 target, the `llvm-tools` component and `wasm-bindgen-cli` 0.2.130; see `../core/README.md`) and runs in a worker. The dev server is a trusted web origin of the dev instance (`config/dev.exs`, `web_origins`), which is what lets the page call its API and link web devices.

The design system (tokens, components, icons, fonts, logo) is vendored from the Lumia design artifact under `src/design/` and `../../assets/`; see `AGENTS.md`.
