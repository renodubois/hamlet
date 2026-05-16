pub mod effect;
pub mod message;
pub mod route;
pub mod state;
pub mod update;
pub mod view;
pub mod widget_ids;

pub use effect::AppEffect;
pub use message::{AppMessage, ChannelMoveDirection};
pub use route::Route;
pub use state::{AppState, BootStatus};
pub use update::{boot, reduce};

const WINDOW_ICON: &[u8] = include_bytes!("../../packaging/icons/hamlet-256.png");

pub fn run() -> iced::Result {
    iced::application(update::boot_runtime, update::update_runtime, view::view)
        .title("Hamlet")
        .window(window_settings())
        .subscription(update::subscription_runtime)
        .theme(|_state: &AppState| iced::Theme::Dark)
        .run()
}

fn window_settings() -> iced::window::Settings {
    iced::window::Settings {
        size: iced::Size::new(1200.0, 800.0),
        min_size: Some(iced::Size::new(900.0, 600.0)),
        icon: iced::window::icon::from_file_data(WINDOW_ICON, None).ok(),
        ..iced::window::Settings::default()
    }
}
