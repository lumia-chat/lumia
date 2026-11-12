//! `cargo run --release` opens the window; Rune's tooling flags render headless instead
//! (`-- --snap out.png`, `--tree tree.json`, `--script scripts/scroll.json --video out.mp4`).

use lumia_rune_spike::{theme, ui};
use rune::{AppSpec, Chrome, TextEdit};

fn main() {
    let spec = AppSpec::<ui::State> {
        min_size: Some((720.0, 480.0)),
        fonts: theme::FONTS,
        families: theme::FAMILIES,
        poll: Some(ui::poll),
        scenes: &[
            ("draft", |s| s.draft = TextEdit::new("Shipping the **Rune** spike today.\nSecond line.")),
            ("design", |s| s.open_channel(1)),
            // Every block of the body subset and a line per script, as the last three messages.
            ("bodies", |s| {
                for (i, body) in ui::BODIES.iter().enumerate() {
                    s.messages.push(ui::Msg { key: format!("scene-{i}"), author: i, time: "now".into(), body: (*body).into() });
                }
            }),
        ],
        ..AppSpec::new("Lumia", (1180.0, 760.0), ui::init, ui::view)
    };
    rune::App::new(spec)
        .chrome(Chrome { full_size_content: true, traffic_lights: Some((22.0, 22.0)), drag_regions: true })
        // The window's first frame is on the GPU: the marker `window-startup.py` times.
        .on_ready(|_| {
            println!("lumia-first-frame");
            if std::env::var_os("LUMIA_SPIKE_QUIT_AFTER_FIRST_FRAME").is_some() {
                std::process::exit(0);
            }
        })
        .run();
}
