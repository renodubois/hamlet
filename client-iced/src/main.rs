#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

fn main() -> iced::Result {
    hamlet_client_iced::app::run()
}
