//! The Lumia desktop shell (architecture section 13, plan step 6): the web
//! UI from `apps/web` on the system webview, with the Rust core running
//! as WASM inside it, exactly as in the browser. The shell adds one thing
//! of its own, updates: a tagged release publishes signed bundles and a
//! `latest.json` on GitHub (the endpoint and key in `tauri.conf.json`),
//! and the shell offers an available one at launch. The in-process core
//! comes when a measured budget on WebKitGTK says so (architecture
//! section 13's trigger).

use tauri::AppHandle;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_updater::UpdaterExt;

/// Runs the app until its last window closes.
pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move { offer_update(handle).await });
            Ok(())
        })
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("lumia: {e}");
        std::process::exit(1);
    }
}

/// Checks the release feed once, at launch, and asks before installing;
/// a failed check is a line on stderr, never a dialog, since the feed is
/// unreachable offline and on a fork that has not published a release.
async fn offer_update(app: AppHandle) {
    let update = match app.updater() {
        Ok(updater) => match updater.check().await {
            Ok(Some(update)) => update,
            Ok(None) => return,
            Err(e) => {
                eprintln!("lumia: update check: {e}");
                return;
            }
        },
        Err(e) => {
            eprintln!("lumia: updater: {e}");
            return;
        }
    };
    let version = update.version.clone();
    let handle = app.clone();
    app.dialog()
        .message(format!(
            "Lumia {version} is available. Install it and restart?"
        ))
        .title("Update available")
        .kind(MessageDialogKind::Info)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Install and restart".into(),
            "Later".into(),
        ))
        .show(move |accepted| {
            if !accepted {
                return;
            }
            tauri::async_runtime::spawn(async move {
                match update.download_and_install(|_, _| {}, || {}).await {
                    Ok(()) => handle.restart(),
                    Err(e) => eprintln!("lumia: update install: {e}"),
                }
            });
        });
}
