# Shared assets

What every shell and the marketing site draw from, vendored once from the Lumia design system (the Claude design artifact) on 2026-11-02; the repo is the master from here on.

- `brand/`: the Original logo set (see its README). Never recoloured. Not open source: the name, logo and wordmark are the Lumia brand, all rights reserved, with the permitted uses in `brand/LICENSE`; a fork replaces them.
- `icons/`: the 36 stroke glyphs as SVG files drawn in `ink-quiet`, for previews, native shells and documentation; the web UI draws the same paths in `currentColor` from `apps/web/src/design/icons.ts`.
- `fonts/`: Bricolage Grotesque (titles) and Albert Sans (everything else), SIL OFL, licences beside them.
- `design/tokens.json`: the design tokens, the cross-platform source. `apps/web` generates its CSS variables from it; the native shells generate their constants from it when they land.

`apps/web` serves this directory as its public root, so `/fonts/...`, `/brand/...` and `/icons/...` resolve to these files in the web build.
