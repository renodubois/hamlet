#![allow(
    dead_code,
    reason = "Keep the full palette available for future UI styling."
)]

use std::rc::Rc;

use gpui_kit::App;
use gpui_kit::component::{Theme, ThemeConfig, ThemeConfigColors, ThemeMode};

// One Dark palette
// https://github.com/atom/one-dark-syntax/blob/master/styles/colors.less

// Surfaces and foregrounds
pub(crate) const BACKGROUND: u32 = 0x21252b;
pub(crate) const SIDEBAR: u32 = 0x282c34;
pub(crate) const SELECTED: u32 = 0x3e4451;
pub(crate) const TEXT: u32 = 0xabb2bf;
pub(crate) const MUTED: u32 = 0x828997;
pub(crate) const COMMENT: u32 = 0x5c6370;

// Accent colors — available for buttons, highlights, and status indicators.
pub(crate) const CYAN: u32 = 0x56b6c2;
pub(crate) const BLUE: u32 = 0x61afef;
pub(crate) const PURPLE: u32 = 0xc678dd;
pub(crate) const GREEN: u32 = 0x98c379;
pub(crate) const RED: u32 = 0xe06c75;
pub(crate) const DARK_RED: u32 = 0xbe5046;
pub(crate) const ORANGE: u32 = 0xd19a66;
pub(crate) const YELLOW: u32 = 0xe5c07b;

/// Install our dark theme. Unspecified settings inherit Kit's dark defaults.
pub(crate) fn init(cx: &mut App) {
    Theme::global_mut(cx).dark_theme = Rc::new(config());
    Theme::change(ThemeMode::Dark, None, cx);
}

fn color(value: u32) -> Option<gpui_kit::SharedString> {
    Some(format!("#{value:06x}").into())
}

/// Initial role mapping: edit these assignments to tune the application's look.
/// Kit derives hover/pressed colors and component tokens from these roles.
fn config() -> ThemeConfig {
    let mut colors = ThemeConfigColors::default();

    // Existing application surfaces and text.
    colors.background = color(BACKGROUND);
    colors.foreground = color(TEXT);
    colors.sidebar = color(SIDEBAR);
    colors.sidebar_foreground = color(TEXT);
    colors.sidebar_accent = color(SELECTED);
    colors.sidebar_accent_foreground = color(TEXT);
    colors.muted = color(SIDEBAR);
    colors.muted_foreground = color(MUTED);
    colors.border = color(SELECTED);
    colors.popover = color(SIDEBAR);
    colors.popover_foreground = color(TEXT);

    // Filled accents use dark text; neutral controls use regular text.
    colors.primary = color(BLUE);
    colors.primary_foreground = color(BACKGROUND);
    colors.secondary = color(GREEN);
    colors.secondary_foreground = color(BACKGROUND);
    colors.accent = color(SELECTED);
    colors.accent_foreground = color(TEXT);
    colors.ring = color(BLUE);
    colors.link = color(BLUE);
    colors.selection = color(SELECTED);
    colors.danger = color(RED);
    colors.danger_foreground = color(BACKGROUND);
    colors.success = color(GREEN);
    colors.success_foreground = color(BACKGROUND);
    colors.warning = color(YELLOW);
    colors.warning_foreground = color(BACKGROUND);
    colors.info = color(CYAN);
    colors.info_foreground = color(BACKGROUND);

    ThemeConfig {
        name: "One Dark".into(),
        mode: ThemeMode::Dark,
        colors,
        ..Default::default()
    }
}

#[cfg(test)]
#[path = "theme/tests/config.rs"]
mod tests;

pub(crate) fn channel_icon() -> gpui_kit::assets::IconName {
    gpui_kit::assets::IconName::Hash
}
