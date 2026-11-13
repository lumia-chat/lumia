import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { defineConfig, type Plugin } from "vite";
import solid from "vite-plugin-solid";

// The service worker (src/sw.ts) must live at the origin's root with a
// stable name: served at /sw.js in dev through Vite's own transform,
// emitted as the chunk sw.js in the build.
function serviceWorker(): Plugin {
  const source = resolve(__dirname, "src/sw.ts");
  return {
    name: "lumia-service-worker",
    configureServer(server) {
      server.middlewares.use(async (req, res, next) => {
        if (req.url?.split("?")[0] !== "/sw.js") return next();
        try {
          const result = await server.transformRequest("/src/sw.ts");
          res.setHeader("content-type", "text/javascript");
          res.setHeader("cache-control", "no-cache");
          res.end(result?.code ?? "");
        } catch (e) {
          next(e);
        }
      });
    },
    buildStart() {
      this.emitFile({ type: "chunk", id: source, fileName: "sw.js" });
    },
  };
}

// The picker's emoji (src/design/emoji.ts) are emojibase-data's list less
// what the picker never reads (skin tone variants, shortcodes, emoticons)
// and less what is newer than Unicode 16, which the systems' emoji fonts do
// not all draw yet (the desktop shell's picker stops there too): a third of
// the file, in the chunk loaded the first time a picker opens.
const EMOJI_UP_TO = 16;
function slimEmoji(): Plugin {
  return {
    name: "lumia-slim-emoji",
    enforce: "pre",
    load(id) {
      const path = id.split("?")[0] ?? "";
      if (!path.endsWith("emojibase-data/en/data.json")) return null;
      const all = JSON.parse(readFileSync(path, "utf8")) as { group?: number; order?: number; label: string; tags?: string[]; emoji: string; version: number }[];
      return JSON.stringify(all.filter((e) => e.group !== undefined && e.group !== 2 && e.version <= EMOJI_UP_TO).map((e) => ({ group: e.group, order: e.order, label: e.label, tags: e.tags, unicode: e.emoji })));
    },
  };
}

export default defineConfig({
  plugins: [solid(), serviceWorker(), slimEmoji()],
  publicDir: "../../assets",
  server: { port: 5173 },
  build: { target: "es2022" },
});
