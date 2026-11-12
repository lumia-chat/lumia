//! `cargo run` opens the window. Rune's tooling flags render headless instead, with the same
//! renderer and no window: `-- --snap out.png [--scene NAME] [--theme dark]`, `--tree
//! tree.json`, `--script FILE --video out.mp4`.
//!
//! Around the window, `platform` makes it an app: one running copy per data directory, the
//! `lumia:` links the system hands it, the Account menu, the frame remembered between
//! launches, notifications and updates. `scripts/bundle-macos.sh` makes `Lumia.app` of the
//! binary, which is what gives it an icon, the URL scheme, the right to post notifications
//! and the updater.

// A release build on Windows is a GUI program: no console window opens beside it. The
// price is that the headless flags print nothing there; a debug build keeps its console.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use lumia_shell::platform::instance::{self, Claim, Launch};
use lumia_shell::{app, platform, scenes, settings, tokens, ui};
use rune::{AppSpec, Chrome};

fn main() {
    // First: a relaunch behind another app stays behind it (rune-mac).
    rune_mac::launch();
    let headless = rune::is_headless();
    let links = instance::launch_links();
    let launch = Launch {
        token: ["XDG_ACTIVATION_TOKEN", "DESKTOP_STARTUP_ID"]
            .into_iter()
            .find_map(|name| std::env::var(name).ok())
            .unwrap_or_default(),
        links: links.clone(),
    };
    let mut hold = match instance::claim(&settings::data_dir(), &launch, headless) {
        Claim::First(hold) => hold,
        // The copy already running took this launch's links and came forward.
        Claim::Forwarded => return,
        Claim::Refused(why) => {
            eprintln!("lumia: {why}");
            std::process::exit(1);
        }
    };

    let spec = AppSpec::<app::State> {
        min_size: Some(platform::MIN_SIZE),
        fonts: tokens::FONTS,
        families: tokens::FAMILIES,
        setup: Some(app::setup),
        poll: Some(platform::poll),
        scenes: scenes::SCENES,
        ..AppSpec::new(
            "Lumia",
            platform::window_size(headless),
            app::init,
            ui::view,
        )
    };
    let app = platform::install(
        rune::App::new(spec)
            .chrome(Chrome {
                full_size_content: true,
                traffic_lights: Some(ui::TRAFFIC_LIGHTS),
                drag_regions: true,
            })
            .standard_menus(),
    );
    platform::attach(app.handle(), &mut hold, links, headless);
    app.run();
}
