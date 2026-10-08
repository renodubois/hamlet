mod api;
mod chat;
mod runtime;
mod session;
mod storage;
#[cfg(test)]
mod test_support;
mod theme;
mod views;

use gpui_kit::*;

fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(|cx: &mut App| {
            gpui_kit::init(cx);
            theme::init(cx);

            let bounds = Bounds::centered(None, size(px(500.), px(500.0)), cx);

            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    let api = api::HttpTransport::new();
                    #[cfg(not(test))]
                    let (config, persistence) =
                        (storage::load(), Some(storage::Persistence::new()));
                    #[cfg(test)]
                    let (config, persistence) = (storage::Config::default(), None);
                    let execution =
                        runtime::Execution::production(cx.background_executor().clone());
                    views::app_shell::open(window, cx, api, config, persistence, execution)
                },
            )
            .unwrap();
            cx.activate(true);
        });
}
