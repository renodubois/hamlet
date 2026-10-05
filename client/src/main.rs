mod api;
mod conversation;
mod runtime;
mod session;
mod storage;
#[cfg(test)]
mod test_support;
mod theme;
mod views;

use gpui_kit::component::Root;
use gpui_kit::*;

fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx: &mut App| {
            gpui_kit::init(cx);
            theme::init(cx);

            let bounds = Bounds::centered(None, size(px(500.), px(500.0)), cx);

            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| {
                    let api = api::HttpTransport::new();
                    #[cfg(not(test))]
                    let (config, persistence) =
                        (storage::load(), Some(storage::Persistence::new()));
                    #[cfg(test)]
                    let (config, persistence) = (storage::Config::default(), None);
                    let execution =
                        runtime::Execution::production(cx.background_executor().clone());
                    let view =
                        views::app_shell::open(window, cx, api, config, persistence, execution);
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .unwrap();
            cx.activate(true);
        });
}
