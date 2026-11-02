# Lumia web

The web UI, also the UI of the desktop shell. SolidJS and Vite, bun.

```sh
bun install
bun run dev        # http://localhost:5173, /design is the component gallery
bun run build      # tokens, typecheck, vite build → dist/
```

The design system (tokens, components, icons, fonts, logo) is vendored from the Lumia design artifact under `src/design/` and `../../assets/`; see `AGENTS.md`.
