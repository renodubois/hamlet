//! Lightweight native visual tokens for refreshed Iced surfaces.
//!
//! These helpers are intentionally small: shared colors, spacing, radii, and a
//! few widget style functions used by refreshed views. They are not a separate
//! runtime theming framework.

use iced::widget::{button, container, text_input};
use iced::{Background, Border, Color, Shadow, Theme, Vector, border};

pub mod color {
    use iced::Color;

    pub const APP_BACKGROUND: Color = Color::from_rgb(0.067, 0.078, 0.110);
    pub const SURFACE: Color = Color::from_rgb(0.109, 0.125, 0.169);
    pub const SURFACE_ELEVATED: Color = Color::from_rgb(0.145, 0.165, 0.220);
    pub const SURFACE_MUTED: Color = Color::from_rgb(0.086, 0.098, 0.137);
    pub const BORDER_SUBTLE: Color = Color::from_rgb(0.235, 0.267, 0.345);
    pub const TEXT: Color = Color::from_rgb(0.941, 0.957, 0.980);
    pub const TEXT_MUTED: Color = Color::from_rgb(0.650, 0.686, 0.760);
    pub const ACCENT: Color = Color::from_rgb(0.427, 0.522, 1.000);
    pub const ACCENT_HOVER: Color = Color::from_rgb(0.525, 0.608, 1.000);
    pub const ACCENT_PRESSED: Color = Color::from_rgb(0.337, 0.427, 0.922);
    pub const MAIN_BACKGROUND: Color = Color::from_rgb(0.965, 0.972, 0.984);
    pub const MAIN_SURFACE: Color = Color::from_rgb(1.000, 1.000, 1.000);
    pub const MAIN_SURFACE_ELEVATED: Color = Color::from_rgb(0.941, 0.953, 0.973);
    pub const MAIN_BORDER: Color = Color::from_rgb(0.824, 0.847, 0.890);
    pub const MAIN_TEXT: Color = Color::from_rgb(0.102, 0.122, 0.161);
    pub const MAIN_TEXT_MUTED: Color = Color::from_rgb(0.384, 0.427, 0.506);
    pub const LINK: Color = Color::from_rgb(0.180, 0.415, 0.855);
}

pub mod spacing {
    pub const XS: f32 = 4.0;
    pub const SM: f32 = 8.0;
    pub const MD: f32 = 12.0;
    pub const LG: f32 = 16.0;
    pub const XL: f32 = 24.0;
    pub const XXL: f32 = 32.0;
}

pub mod radius {
    pub const SM: f32 = 6.0;
    pub const MD: f32 = 12.0;
    pub const LG: f32 = 20.0;
}

pub mod container_style {
    use super::*;

    pub fn app_background(_theme: &Theme) -> container::Style {
        container::Style {
            text_color: Some(color::TEXT),
            background: Some(color::APP_BACKGROUND.into()),
            ..container::Style::default()
        }
    }

    pub fn hero_panel(_theme: &Theme) -> container::Style {
        container::Style {
            text_color: Some(color::TEXT),
            background: Some(color::SURFACE.into()),
            border: Border {
                radius: border::Radius::from(radius::LG),
                width: 1.0,
                color: color::BORDER_SUBTLE,
            },
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.35),
                offset: Vector::new(0.0, 18.0),
                blur_radius: 36.0,
            },
            ..container::Style::default()
        }
    }

    pub fn field_group(_theme: &Theme) -> container::Style {
        container::Style {
            text_color: Some(color::TEXT),
            background: Some(color::SURFACE_MUTED.into()),
            border: border::rounded(radius::MD).color(color::BORDER_SUBTLE),
            ..container::Style::default()
        }
    }

    pub fn sidebar(_theme: &Theme) -> container::Style {
        container::Style {
            text_color: Some(color::TEXT),
            background: Some(color::SURFACE.into()),
            border: Border {
                radius: border::Radius::default(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
            ..container::Style::default()
        }
    }

    pub fn main_shell(_theme: &Theme) -> container::Style {
        container::Style {
            text_color: Some(color::MAIN_TEXT),
            background: Some(color::MAIN_BACKGROUND.into()),
            ..container::Style::default()
        }
    }

    pub fn main_surface(_theme: &Theme) -> container::Style {
        container::Style {
            text_color: Some(color::MAIN_TEXT),
            background: Some(color::MAIN_SURFACE.into()),
            border: border::rounded(radius::MD).color(color::MAIN_BORDER),
            ..container::Style::default()
        }
    }

    pub fn composer_bar(_theme: &Theme) -> container::Style {
        container::Style {
            text_color: Some(color::MAIN_TEXT),
            background: Some(color::MAIN_SURFACE.into()),
            border: Border {
                radius: border::Radius::from(radius::MD),
                width: 1.0,
                color: color::MAIN_BORDER,
            },
            shadow: Shadow {
                color: Color::from_rgba(0.102, 0.122, 0.161, 0.10),
                offset: Vector::new(0.0, -4.0),
                blur_radius: 18.0,
            },
            ..container::Style::default()
        }
    }

    pub fn emoji_picker(_theme: &Theme) -> container::Style {
        container::Style {
            text_color: Some(color::MAIN_TEXT),
            background: Some(color::MAIN_SURFACE_ELEVATED.into()),
            border: border::rounded(radius::MD).color(color::MAIN_BORDER),
            ..container::Style::default()
        }
    }

    pub fn message_row(_theme: &Theme) -> container::Style {
        container::Style {
            text_color: Some(color::MAIN_TEXT),
            background: Some(Color::TRANSPARENT.into()),
            ..container::Style::default()
        }
    }

    pub fn embed_card(_theme: &Theme) -> container::Style {
        container::Style {
            text_color: Some(color::MAIN_TEXT),
            background: Some(color::MAIN_SURFACE.into()),
            border: Border {
                radius: border::Radius::from(radius::MD),
                width: 1.0,
                color: color::MAIN_BORDER,
            },
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.08),
                offset: Vector::new(0.0, 4.0),
                blur_radius: 12.0,
            },
            ..container::Style::default()
        }
    }
}

pub mod button_style {
    use super::*;

    pub fn primary(_theme: &Theme, status: button::Status) -> button::Style {
        let background = match status {
            button::Status::Hovered => color::ACCENT_HOVER,
            button::Status::Pressed => color::ACCENT_PRESSED,
            button::Status::Disabled => color::SURFACE_ELEVATED,
            button::Status::Active => color::ACCENT,
        };

        button::Style {
            background: Some(background.into()),
            text_color: if matches!(status, button::Status::Disabled) {
                color::TEXT_MUTED
            } else {
                color::TEXT
            },
            border: border::rounded(radius::SM),
            ..button::Style::default()
        }
    }

    pub fn secondary(_theme: &Theme, status: button::Status) -> button::Style {
        let background = match status {
            button::Status::Hovered => color::SURFACE_ELEVATED,
            button::Status::Pressed => color::SURFACE_MUTED,
            button::Status::Active | button::Status::Disabled => color::SURFACE,
        };

        button::Style {
            background: Some(background.into()),
            text_color: if matches!(status, button::Status::Disabled) {
                color::TEXT_MUTED
            } else {
                color::TEXT
            },
            border: border::rounded(radius::SM).color(color::BORDER_SUBTLE),
            ..button::Style::default()
        }
    }

    pub fn composer_secondary(_theme: &Theme, status: button::Status) -> button::Style {
        let background = match status {
            button::Status::Hovered => color::MAIN_SURFACE,
            button::Status::Pressed => color::MAIN_BORDER,
            button::Status::Active | button::Status::Disabled => color::MAIN_SURFACE_ELEVATED,
        };

        button::Style {
            background: Some(background.into()),
            text_color: color::MAIN_TEXT,
            border: border::rounded(radius::SM).color(color::MAIN_BORDER),
            ..button::Style::default()
        }
    }

    pub fn channel_text(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
        move |_theme: &Theme, status: button::Status| {
            let background = match (selected, status) {
                (true, button::Status::Hovered) => color::ACCENT_HOVER,
                (true, button::Status::Pressed) => color::ACCENT_PRESSED,
                (true, _) => color::ACCENT,
                (false, button::Status::Hovered) => color::SURFACE_ELEVATED,
                (false, button::Status::Pressed) => color::SURFACE_MUTED,
                (false, _) => Color::TRANSPARENT,
            };

            button::Style {
                background: Some(background.into()),
                text_color: if selected {
                    color::TEXT
                } else {
                    color::TEXT_MUTED
                },
                border: border::rounded(radius::SM),
                ..button::Style::default()
            }
        }
    }

    pub fn channel_voice(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
        move |_theme: &Theme, status: button::Status| {
            let background = match (selected, status) {
                (true, button::Status::Hovered) => color::ACCENT_HOVER,
                (true, button::Status::Pressed) => color::ACCENT_PRESSED,
                (true, _) => color::ACCENT,
                (false, button::Status::Hovered) => color::SURFACE_ELEVATED,
                (false, button::Status::Pressed) => color::SURFACE_MUTED,
                (false, _) => color::SURFACE_MUTED,
            };

            button::Style {
                background: Some(background.into()),
                text_color: if selected {
                    color::TEXT
                } else {
                    color::TEXT_MUTED
                },
                border: border::rounded(radius::SM).color(if selected {
                    color::ACCENT_HOVER
                } else {
                    color::BORDER_SUBTLE
                }),
                ..button::Style::default()
            }
        }
    }

    pub fn reorder_control(_theme: &Theme, status: button::Status) -> button::Style {
        let background = match status {
            button::Status::Hovered => color::SURFACE_ELEVATED,
            button::Status::Pressed => color::SURFACE_MUTED,
            button::Status::Active | button::Status::Disabled => Color::TRANSPARENT,
        };

        button::Style {
            background: Some(background.into()),
            text_color: if matches!(status, button::Status::Disabled) {
                Color::from_rgba(
                    color::TEXT_MUTED.r,
                    color::TEXT_MUTED.g,
                    color::TEXT_MUTED.b,
                    0.45,
                )
            } else {
                color::TEXT_MUTED
            },
            border: border::rounded(radius::SM),
            ..button::Style::default()
        }
    }

    pub fn subtle_link(_theme: &Theme, status: button::Status) -> button::Style {
        let text_color = if matches!(status, button::Status::Disabled) {
            color::MAIN_TEXT_MUTED
        } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
            color::ACCENT_HOVER
        } else {
            color::LINK
        };

        button::Style {
            background: Some(Color::TRANSPARENT.into()),
            text_color,
            border: border::rounded(radius::SM).color(Color::TRANSPARENT),
            ..button::Style::default()
        }
    }
}

pub mod text_input_style {
    use super::*;

    pub fn field(_theme: &Theme, status: text_input::Status) -> text_input::Style {
        let border_color = match status {
            text_input::Status::Focused { .. } => color::ACCENT,
            text_input::Status::Hovered => color::TEXT_MUTED,
            text_input::Status::Active | text_input::Status::Disabled => color::BORDER_SUBTLE,
        };

        text_input::Style {
            background: Background::Color(color::SURFACE_ELEVATED),
            border: Border {
                radius: border::Radius::from(radius::SM),
                width: 1.0,
                color: border_color,
            },
            icon: color::TEXT_MUTED,
            placeholder: color::TEXT_MUTED,
            value: if matches!(status, text_input::Status::Disabled) {
                color::TEXT_MUTED
            } else {
                color::TEXT
            },
            selection: color::ACCENT,
        }
    }

    pub fn composer(_theme: &Theme, status: text_input::Status) -> text_input::Style {
        let border_color = match status {
            text_input::Status::Focused { .. } => color::ACCENT,
            text_input::Status::Hovered => color::MAIN_TEXT_MUTED,
            text_input::Status::Active | text_input::Status::Disabled => color::MAIN_BORDER,
        };

        text_input::Style {
            background: Background::Color(color::MAIN_SURFACE_ELEVATED),
            border: Border {
                radius: border::Radius::from(radius::SM),
                width: 1.0,
                color: border_color,
            },
            icon: color::MAIN_TEXT_MUTED,
            placeholder: color::MAIN_TEXT_MUTED,
            value: if matches!(status, text_input::Status::Disabled) {
                color::MAIN_TEXT_MUTED
            } else {
                color::MAIN_TEXT
            },
            selection: color::ACCENT,
        }
    }
}
