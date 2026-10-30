use aether_core::ThemeMode;
use iced::widget::{button, container};
use iced::{Background, Border, Color, Shadow, Theme, Vector};

pub fn resolve_theme(_mode: ThemeMode) -> Theme {
    Theme::TokyoNight
}

pub fn top_bar_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb8(24, 24, 28))),
        border: Border {
            color: Color::from_rgb8(38, 38, 44),
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

pub fn sidebar_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb8(22, 22, 26))),
        border: Border {
            color: Color::from_rgb8(40, 40, 48),
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

pub fn canvas_backdrop_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb8(17, 17, 20))),
        ..Default::default()
    }
}

pub fn floating_hud_pill_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgba8(24, 24, 28, 0.88))),
        border: Border {
            color: Color::from_rgba(1.0, 1.0, 1.0, 0.10),
            width: 1.0,
            radius: 10.0.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.45),
            offset: Vector::new(0.0, 4.0),
            blur_radius: 14.0,
        },
        ..Default::default()
    }
}

pub fn card_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb8(30, 31, 38))),
        border: Border {
            color: Color::from_rgb8(48, 50, 60),
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    }
}

pub fn active_card_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb8(36, 54, 88))),
        border: Border {
            color: Color::from_rgb8(96, 165, 250),
            width: 1.5,
            radius: 8.0.into(),
        },
        ..Default::default()
    }
}

pub fn drop_zone_overlay_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgba8(20, 38, 72, 0.88))),
        border: Border {
            color: Color::from_rgb8(96, 165, 250),
            width: 2.5,
            radius: 14.0.into(),
        },
        ..Default::default()
    }
}

pub fn icon_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Some(Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.10))),
        button::Status::Pressed => Some(Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.16))),
        _ => None,
    };

    button::Style {
        background: bg,
        text_color: Color::from_rgb8(235, 238, 245),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 7.0.into(),
        },
        ..Default::default()
    }
}

pub fn active_icon_button_style(_theme: &Theme, _status: button::Status) -> button::Style {
    button::Style {
        background: Some(Background::Color(Color::from_rgb8(45, 105, 220))),
        text_color: Color::WHITE,
        border: Border {
            color: Color::from_rgb8(96, 165, 250),
            width: 1.0,
            radius: 7.0.into(),
        },
        ..Default::default()
    }
}

pub fn pill_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered | button::Status::Pressed => Color::from_rgb8(48, 50, 62),
        _ => Color::from_rgb8(34, 36, 44),
    };

    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::from_rgb8(235, 240, 250),
        border: Border {
            color: Color::from_rgba(1.0, 1.0, 1.0, 0.08),
            width: 1.0,
            radius: 6.0.into(),
        },
        ..Default::default()
    }
}

pub fn accent_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb8(59, 130, 246),
        button::Status::Pressed => Color::from_rgb8(29, 78, 216),
        _ => Color::from_rgb8(37, 99, 235),
    };

    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::WHITE,
        border: Border {
            color: Color::from_rgb8(96, 165, 250),
            width: 1.0,
            radius: 6.0.into(),
        },
        ..Default::default()
    }
}
