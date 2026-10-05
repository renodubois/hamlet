use super::*;
use gpui_kit::{Hsla, TestAppContext, rgb};

fn hsla(value: u32) -> Hsla {
    rgb(value).into()
}

#[gpui_kit::test]
fn installs_palette_and_resolves_component_colors(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(init);
    cx.update(|cx| {
        let theme = Theme::global(cx);
        assert!(theme.is_dark());
        assert_eq!(theme.theme_name().as_ref(), "Hamlet One Dark");
        assert_eq!(theme.background, hsla(BACKGROUND));
        assert_eq!(theme.foreground, hsla(TEXT));
        assert_eq!(theme.sidebar, hsla(SIDEBAR));
        assert_eq!(theme.sidebar_accent, hsla(SELECTED));
        assert_eq!(theme.muted_foreground, hsla(MUTED));
        assert_eq!(theme.primary, hsla(BLUE));
        assert_eq!(theme.secondary, hsla(SELECTED));
        assert_eq!(theme.danger, hsla(RED));
        assert_eq!(theme.success, hsla(GREEN));
        assert_eq!(theme.warning, hsla(YELLOW));
        assert_eq!(theme.info, hsla(CYAN));
        assert_eq!(theme.tokens.button_primary, theme.tokens.primary);
        assert_eq!(theme.button_primary_foreground, hsla(BACKGROUND));
        assert_eq!(
            theme.tokens.button_primary_hover,
            theme.tokens.primary_hover
        );
        assert_eq!(
            theme.tokens.button_primary_active,
            theme.tokens.primary_active
        );
        assert_ne!(theme.primary_hover, theme.primary);
        assert_ne!(theme.primary_active, theme.primary);
        assert_eq!(theme.tokens.button_secondary, theme.tokens.secondary);
    });
}

#[gpui_kit::test]
fn switching_back_to_dark_reapplies_the_custom_configuration(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(init);
    cx.update(|cx| {
        Theme::change(ThemeMode::Light, None, cx);
        Theme::change(ThemeMode::Dark, None, cx);
        let theme = Theme::global(cx);
        assert_eq!(theme.background, hsla(BACKGROUND));
        assert_eq!(theme.primary, hsla(BLUE));
        assert_eq!(theme.tokens.button_primary.color, hsla(BLUE));
        assert_eq!(theme.sidebar, hsla(SIDEBAR));
    });
}
