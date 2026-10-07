use super::*;
use gpui_kit::TestAppContext;

#[gpui_kit::test]
fn installs_custom_theme_and_wires_button_tokens(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(init);
    cx.update(|cx| {
        let theme = Theme::global(cx);
        assert!(theme.is_dark());
        assert_eq!(theme.theme_name(), &config().name);
        assert_eq!(theme.tokens.button_primary, theme.tokens.primary);
        assert_eq!(
            theme.tokens.button_primary_hover,
            theme.tokens.primary_hover
        );
        assert_eq!(
            theme.tokens.button_primary_active,
            theme.tokens.primary_active
        );
        assert_eq!(theme.tokens.button_secondary, theme.tokens.secondary);
    });
}

#[gpui_kit::test]
fn switching_back_to_dark_reapplies_the_custom_configuration(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(init);
    cx.update(|cx| {
        let theme = Theme::global(cx);
        let installed_name = theme.theme_name().clone();
        let installed_background = theme.background;
        let installed_primary = theme.tokens.button_primary;

        Theme::change(ThemeMode::Light, None, cx);
        assert!(!Theme::global(cx).is_dark());
        Theme::change(ThemeMode::Dark, None, cx);

        let theme = Theme::global(cx);
        assert!(theme.is_dark());
        assert_eq!(theme.theme_name(), &installed_name);
        assert_eq!(theme.background, installed_background);
        assert_eq!(theme.tokens.button_primary, installed_primary);
    });
}
