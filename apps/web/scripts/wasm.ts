// Builds the Rust core for the browser: `lumia-wasm` for wasm32 with the
// size-first `web` profile, then wasm-bindgen into src/client/pkg
// (gitignored), then wasm-opt -Oz (binaryen: the system one, else the
// npm package's). Needs the wasm32 target, the llvm-tools component (its
// llvm-ar archives the C objects of SQLite; see apps/core/README.md) and
// wasm-bindgen-cli at the pinned version. `--debug` skips the profile and
// the optimizer.
import { execSync } from "node:child_process";
import { existsSync, statSync } from "node:fs";
import { resolve } from "node:path";

const web = resolve(import.meta.dir, "..");
const core = resolve(web, "../core");
const out = resolve(web, "src/client/pkg");
const debug = process.argv.includes("--debug");
const profile = debug ? "debug" : "web";

function sh(cmd: string, cwd: string) {
  console.log(`$ ${cmd}`);
  execSync(cmd, { cwd, stdio: "inherit", env: process.env });
}

if (!process.env.AR_wasm32_unknown_unknown) {
  const sysroot = execSync("rustc --print sysroot", { cwd: core }).toString().trim();
  const host = execSync("rustc -vV", { cwd: core }).toString().match(/host: (\S+)/)?.[1] ?? "";
  const ar = [`${sysroot}/lib/rustlib/${host}/bin/llvm-ar`, `${sysroot}/lib/rustlib/${host}/bin/llvm-ar.exe`].find(existsSync);
  if (ar) process.env.AR_wasm32_unknown_unknown = ar;
  else console.warn("llvm-ar not found; run `rustup component add llvm-tools` if the link fails");
}

// SQLite3 Multiple Ciphers brings every cipher; the stores use ChaCha20
// only (lumia-client's `open_on_vfs_keyed`), so the rest stay out of the
// binary. The crate reads these as extra C flags.
process.env.SQLITE_WASM_RS_FLAGS ??=
  "-DHAVE_CIPHER_AES_128_CBC=0 -DHAVE_CIPHER_AES_256_CBC=0 -DHAVE_CIPHER_SQLCIPHER=0 -DHAVE_CIPHER_RC4=0 -DHAVE_CIPHER_ASCON128=0 -DHAVE_CIPHER_AEGIS=0";

sh(`cargo build ${debug ? "" : "--profile web"} -p lumia-wasm --target wasm32-unknown-unknown`, core);
sh(`wasm-bindgen --target web --out-dir "${out}" "${core}/target/wasm32-unknown-unknown/${profile}/lumia_wasm.wasm"`, core);
if (!debug) {
  const bin = resolve(web, "node_modules/.bin/wasm-opt");
  const wasmOpt = existsSync(bin) ? bin : "wasm-opt";
  const file = resolve(out, "lumia_wasm_bg.wasm");
  sh(`"${wasmOpt}" -Oz --enable-bulk-memory --enable-nontrapping-float-to-int --strip-debug --strip-producers -o "${file}" "${file}"`, web);
}
console.log(`wasm: ${out} (${(statSync(resolve(out, "lumia_wasm_bg.wasm")).size / 1_000_000).toFixed(2)} MB)`);
