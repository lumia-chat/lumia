//! `cargo run` opens the window. Rune's tooling flags render headless instead, with the same
//! renderer and no window: `-- --snap out.png [--scene NAME] [--theme dark]`, `--tree
//! tree.json`, `--script FILE --video out.mp4`.

use lumia_shell::{app, scenes, tokens, ui};
use rune::{AppSpec, Chrome};

fn main() {
    let spec = AppSpec::<app::State> {
        min_size: Some((880.0, 560.0)),
        fonts: tokens::FONTS,
        families: tokens::FAMILIES,
        setup: Some(app::setup),
        poll: Some(app::poll),
        scenes: scenes::SCENES,
        ..AppSpec::new("Lumia", (1180.0, 760.0), app::init, ui::view)
    };
    rune::App::new(spec)
        .chrome(Chrome {
            full_size_content: true,
            traffic_lights: Some((22.0, 22.0)),
            drag_regions: true,
        })
        .standard_menus()
        .run();
}
