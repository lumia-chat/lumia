//! `cargo run -p uniffi-bindgen -- generate --library <liblumia_ffi> --language swift --out-dir <dir>`.
fn main() {
    uniffi::uniffi_bindgen_main();
}
