//! The Lumia desktop shell (architecture section 13, plan step 6): the web
//! UI from `apps/web` on the system webview, with the Rust core running
//! as WASM inside it, exactly as in the browser. The shell adds nothing
//! of its own yet; the in-process core comes when a measured budget on
//! WebKitGTK says so (architecture section 13's trigger).

/// Runs the app until its last window closes.
pub fn run() {
    if let Err(e) = tauri::Builder::default().run(tauri::generate_context!()) {
        eprintln!("lumia: {e}");
        std::process::exit(1);
    }
}
