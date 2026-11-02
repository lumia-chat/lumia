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

export default defineConfig({
  plugins: [solid(), serviceWorker()],
  publicDir: "../../assets",
  server: { port: 5173 },
  build: { target: "es2022" },
});
