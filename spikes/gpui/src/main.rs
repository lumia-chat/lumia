//! `cargo run --release` opens the window. GPUI has no headless render of its own to a PNG, so
//! measurements come from the window (`LUMIA_SPIKE_*`, see the README) and from `cargo test`.

use gpui_kit::assets::Assets;
use gpui_kit::{
    AppContext as _, KeyBinding, TitlebarOptions, WindowBounds, WindowOptions, actions, point, px, size,
};
use lumia_gpui_spike::shell::Shell;

actions!(lumia, [Quit]);

fn main() {
    gpui_kit::application().with_assets(Assets).run(|cx| {
        gpui_kit::init(cx);
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(1180.), px(760.)), cx)),
            window_min_size: Some(size(px(720.), px(480.))),
            titlebar: Some(TitlebarOptions {
                title: Some("Lumia".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(22.), px(22.))),
            }),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| {
            let dark = lumia_gpui_spike::dark(window);
            lumia_gpui_spike::init(dark, cx);
            cx.new(|cx| Shell::new(dark, window, cx))
        })
        .expect("the window opens");
        cx.activate(true);
    });
}
