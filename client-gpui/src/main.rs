mod api;
mod conversation;
mod runtime;
mod session;
mod storage;
mod theme;
mod views;

// Temporary import compatibility, not parallel implementations. See MIGRATION-48.md.
use api as http;
use storage as persistence;

use gpui_kit::component::Root;
use gpui_kit::*;
use std::sync::Arc;

fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx: &mut App| {
            gpui_kit::init(cx);

            let bounds = Bounds::centered(None, size(px(500.), px(500.0)), cx);

            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| {
                    let api = Arc::new(api::HttpAuth::new());
                    #[cfg(not(test))]
                    let (config, persistence) =
                        (storage::load(), Some(storage::Persistence::new()));
                    #[cfg(test)]
                    let (config, persistence) = (storage::Config::default(), None);
                    let view = views::app_shell::open(window, cx, api, config, persistence);
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .unwrap();
            cx.activate(true);
        });
}
