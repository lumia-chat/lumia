import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

// The shared assets directory is the public root: /fonts, /brand, /icons
// and /design resolve to ../../assets at dev time and are copied into the
// build.
export default defineConfig({
  plugins: [solid()],
  publicDir: "../../assets",
  server: { port: 5173 },
  build: { target: "es2022" },
});
