//! Product visual language — charcoal chrome + one blue accent (see UI mockup).
//! Borders follow Kite: 1px `BORDER` on the outer window and every card/panel.

use iced::widget::{button, container, overlay, pick_list, scrollable, text, text_input};
use iced::{Background, Border, Color, Padding, Theme};

// Refined deep navy/charcoal palette aligned with Mockup Fig 1
pub const BG: Color = Color::from_rgb(0.06, 0.08, 0.11); // #0F141C - deep canvas
pub const SIDEBAR: Color = Color::from_rgb(0.05, 0.07, 0.09); // #0C1017 - subtle darker sidebar
pub const SURFACE: Color = Color::from_rgb(0.08, 0.11, 0.15); // #141C26 - card surface
pub const ELEVATED: Color = Color::from_rgb(0.11, 0.15, 0.20); // #1C2633 - hover/raised cards
pub const BORDER: Color = Color::from_rgba(0.24, 0.35, 0.48, 0.28); // delicate subtle border
pub const WINDOW_BORDER: Color = Color::from_rgba(0.35, 0.48, 0.65, 0.35); // outer rim
pub const BORDER_SUBTLE: Color = Color::from_rgba(0.18, 0.26, 0.36, 0.25);
pub const INK: Color = Color::from_rgb(0.95, 0.97, 0.99); // #F2F6FA - crisp primary text
pub const MUTED: Color = Color::from_rgb(0.60, 0.66, 0.75); // #99A8BF - secondary text
pub const FAINT: Color = Color::from_rgb(0.38, 0.44, 0.52); // #617085 - tertiary/hints
pub const ACCENT: Color = Color::from_rgb(0.18, 0.48, 0.96); // #2E7AF5 - vibrant primary blue
pub const ACCENT_SOFT: Color = Color::from_rgb(0.09, 0.18, 0.30); // #172E4D - selected card glow
pub const ACCENT_BORDER: Color = Color::from_rgb(0.24, 0.55, 0.98); // #3D8CFA - selected card border
pub const DANGER: Color = Color::from_rgb(0.97, 0.42, 0.42);
pub const OK: Color = Color::from_rgb(0.18, 0.82, 0.58);
pub const WARN: Color = Color::from_rgb(0.98, 0.74, 0.16);
pub const CHIP_BG: Color = Color::from_rgb(0.07, 0.10, 0.14); // #121924 - inset well

pub fn bg(c: Color) -> Background {
    Background::Color(c)
}

pub fn gradient(start: Color, end: Color) -> Background {
    iced::gradient::Linear::new(iced::Radians(1.2))
        .add_stop(0.0, start)
        .add_stop(1.0, end)
        .into()
}

pub fn stroke(color: Color, width: f32, radius: f32) -> Border {
    Border {
        color,
        width,
        radius: radius.into(),
    }
}

/// Outer window shell: 1px `WINDOW_BORDER`, soft round (mockup window).
pub fn shell_style(_t: &Theme) -> container::Style {
    container::Style {
        background: Some(bg(BG)),
        text_color: Some(INK),
        border: stroke(WINDOW_BORDER, 1.0, 12.0),
        ..Default::default()
    }
}

/// Sidebar: darker charcoal; right edge is a [`vdivider`].
pub fn sidebar_style(_t: &Theme) -> container::Style {
    container::Style {
        background: Some(gradient(Color::from_rgb(0.07, 0.09, 0.12), SIDEBAR)),
        text_color: Some(INK),
        ..Default::default()
    }
}

/// Top chrome under the window edge — bottom edge is a [`hdivider`].
pub fn topbar_style(_t: &Theme) -> container::Style {
    container::Style {
        background: Some(gradient(
            Color::from_rgb(0.07, 0.09, 0.13),
            Color::from_rgb(0.06, 0.08, 0.11),
        )),
        text_color: Some(INK),
        ..Default::default()
    }
}

/// Card / panel: 1px border + radius 12 (mockup cards).
pub fn panel_style(_t: &Theme) -> container::Style {
    container::Style {
        background: Some(gradient(
            Color::from_rgb(0.09, 0.12, 0.17),
            Color::from_rgb(0.07, 0.09, 0.13),
        )),
        text_color: Some(INK),
        border: stroke(BORDER, 1.0, 12.0),
        ..Default::default()
    }
}

pub fn modal_backdrop_style(_t: &Theme) -> container::Style {
    container::Style {
        background: Some(bg(Color::from_rgba(0.01, 0.03, 0.06, 0.62))),
        ..Default::default()
    }
}

pub fn modal_panel_style(_t: &Theme) -> container::Style {
    container::Style {
        background: Some(bg(SURFACE)),
        text_color: Some(INK),
        border: stroke(WINDOW_BORDER, 1.0, 14.0),
        ..Default::default()
    }
}

pub fn modal_content_style(_t: &Theme) -> container::Style {
    container::Style {
        background: Some(bg(CHIP_BG)),
        text_color: Some(INK),
        border: stroke(BORDER_SUBTLE, 1.0, 9.0),
        ..Default::default()
    }
}

/// Inset well for secondary context cards and type chips.
pub fn inset_style(_t: &Theme) -> container::Style {
    container::Style {
        background: Some(bg(CHIP_BG)),
        text_color: Some(INK),
        border: stroke(BORDER, 1.0, 10.0),
        ..Default::default()
    }
}

/// Circular glowing Hero chip in detail header
#[allow(dead_code)]
pub fn hero_chip_style(_t: &Theme) -> container::Style {
    container::Style {
        background: Some(gradient(
            Color::from_rgb(0.12, 0.28, 0.48),
            Color::from_rgb(0.08, 0.18, 0.32),
        )),
        text_color: Some(INK),
        border: stroke(Color::from_rgba(0.24, 0.55, 0.98, 0.35), 1.0, 25.0),
        ..Default::default()
    }
}

/// Search field outer shell (icon + input + keycap).
pub fn search_shell_style(_t: &Theme) -> container::Style {
    container::Style {
        background: Some(bg(Color::from_rgb(0.07, 0.10, 0.14))),
        text_color: Some(INK),
        border: stroke(BORDER, 1.0, 10.0),
        ..Default::default()
    }
}

/// 1px vertical split (sidebar | main).
pub fn vdivider<'a, Message>() -> container::Container<'a, Message, Theme, iced::Renderer>
where
    Message: 'a,
{
    use iced::{widget::Space, Length};
    container(Space::new(Length::Fixed(1.0), Length::Fill))
        .width(Length::Fixed(1.0))
        .height(Length::Fill)
        .style(|_t: &Theme| container::Style {
            background: Some(bg(BORDER_SUBTLE)),
            ..Default::default()
        })
}

/// 1px horizontal split (topbar | body, list rows).
pub fn hdivider<'a, Message>() -> container::Container<'a, Message, Theme, iced::Renderer>
where
    Message: 'a,
{
    use iced::{widget::Space, Length};
    container(Space::new(Length::Fill, Length::Fixed(1.0)))
        .width(Length::Fill)
        .height(Length::Fixed(1.0))
        .style(|_t: &Theme| container::Style {
            background: Some(bg(BORDER_SUBTLE)),
            ..Default::default()
        })
}

pub fn card_button(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style + 'static {
    move |_t: &Theme, status: button::Status| {
        let (bg_start, bg_end, border_c, border_w) = if selected {
            (
                Color::from_rgb(0.10, 0.21, 0.35),
                Color::from_rgb(0.07, 0.14, 0.24),
                ACCENT_BORDER,
                1.5,
            )
        } else {
            match status {
                button::Status::Hovered => (
                    Color::from_rgb(0.10, 0.14, 0.19),
                    Color::from_rgb(0.08, 0.11, 0.16),
                    Color::from_rgba(0.25, 0.35, 0.48, 0.3),
                    1.0,
                ),
                _ => (
                    Color::TRANSPARENT,
                    Color::TRANSPARENT,
                    Color::TRANSPARENT,
                    0.0,
                ),
            }
        };
        button::Style {
            background: Some(if selected || matches!(status, button::Status::Hovered) {
                gradient(bg_start, bg_end)
            } else {
                bg(Color::TRANSPARENT)
            }),
            text_color: INK,
            border: stroke(border_c, border_w, 10.0),
            ..Default::default()
        }
    }
}

#[allow(dead_code)]
pub fn ghost_button(_t: &Theme, status: button::Status) -> button::Style {
    let (bg_c, border_c, text_c) = match status {
        button::Status::Hovered => (ELEVATED, BORDER, INK),
        button::Status::Pressed => (CHIP_BG, BORDER, INK),
        _ => (Color::TRANSPARENT, Color::TRANSPARENT, MUTED),
    };
    button::Style {
        background: Some(bg(bg_c)),
        text_color: text_c,
        border: stroke(border_c, 1.0, 8.0),
        ..Default::default()
    }
}

/// Action button inside toolbar or content card (quick actions).
pub fn action_button(_t: &Theme, status: button::Status) -> button::Style {
    let (bg_c, text_c, border_c) = match status {
        button::Status::Hovered => (
            Color::from_rgb(0.13, 0.18, 0.24),
            INK,
            Color::from_rgba(0.28, 0.40, 0.55, 0.4),
        ),
        button::Status::Pressed => (CHIP_BG, INK, BORDER),
        _ => (
            Color::from_rgb(0.09, 0.12, 0.16),
            MUTED,
            Color::from_rgba(0.20, 0.28, 0.38, 0.3),
        ),
    };
    button::Style {
        background: Some(bg(bg_c)),
        text_color: text_c,
        border: stroke(border_c, 1.0, 6.0),
        ..Default::default()
    }
}

pub fn context_menu_panel(_t: &Theme) -> container::Style {
    container::Style {
        background: Some(bg(ELEVATED)),
        text_color: Some(INK),
        border: stroke(WINDOW_BORDER, 1.0, 9.0),
        shadow: iced::Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.45),
            offset: iced::Vector::new(0.0, 8.0),
            blur_radius: 18.0,
        },
    }
}

pub fn context_menu_item(danger: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_t, status| button::Style {
        background: match status {
            button::Status::Hovered => Some(bg(if danger {
                Color::from_rgb(0.24, 0.14, 0.17)
            } else {
                ACCENT_SOFT
            })),
            button::Status::Pressed => Some(bg(CHIP_BG)),
            _ => None,
        },
        text_color: if danger { DANGER } else { INK },
        border: stroke(Color::TRANSPARENT, 0.0, 6.0),
        ..Default::default()
    }
}

pub fn primary_button(_t: &Theme, status: button::Status) -> button::Style {
    let (bg_c, text_c) = match status {
        button::Status::Hovered => (Color::from_rgb(0.28, 0.56, 1.0), INK),
        button::Status::Pressed => (Color::from_rgb(0.16, 0.42, 0.88), INK),
        _ => (ACCENT, INK),
    };
    button::Style {
        background: Some(bg(bg_c)),
        text_color: text_c,
        border: stroke(ACCENT_BORDER, 1.0, 8.0),
        ..Default::default()
    }
}

pub fn danger_button(_t: &Theme, status: button::Status) -> button::Style {
    let (bg_c, text_c) = match status {
        button::Status::Hovered => (ELEVATED, DANGER),
        _ => (Color::TRANSPARENT, DANGER),
    };
    button::Style {
        background: Some(bg(bg_c)),
        text_color: text_c,
        border: stroke(BORDER, 1.0, 8.0),
        ..Default::default()
    }
}

pub fn nav_button(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style + 'static {
    move |_t: &Theme, status: button::Status| {
        let (bg_c, text_c) = if active {
            (ACCENT_SOFT, INK)
        } else {
            match status {
                button::Status::Hovered => (SURFACE, INK),
                _ => (Color::TRANSPARENT, MUTED),
            }
        };
        button::Style {
            background: Some(if active {
                gradient(
                    Color::from_rgb(0.11, 0.23, 0.38),
                    Color::from_rgb(0.08, 0.16, 0.28),
                )
            } else {
                bg(bg_c)
            }),
            text_color: text_c,
            border: stroke(
                if active {
                    Color::from_rgba(0.24, 0.55, 0.98, 0.5)
                } else {
                    Color::TRANSPARENT
                },
                1.0,
                10.0,
            ),
            ..Default::default()
        }
    }
}

pub fn chip_button(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style + 'static {
    move |_t: &Theme, status: button::Status| {
        let (bg_c, border_c, text_c) = if selected {
            (ACCENT, ACCENT_BORDER, INK)
        } else {
            match status {
                button::Status::Hovered => (ELEVATED, BORDER, INK),
                _ => (CHIP_BG, BORDER, MUTED),
            }
        };
        button::Style {
            background: Some(bg(bg_c)),
            text_color: text_c,
            border: stroke(border_c, 1.0, 8.0),
            ..Default::default()
        }
    }
}

/// History toolbar filter button: active is bright pill matching mockup.
pub fn toolbar_filter_button(
    selected: bool,
) -> impl Fn(&Theme, button::Status) -> button::Style + 'static {
    move |_t: &Theme, status: button::Status| {
        if selected {
            button::Style {
                background: Some(bg(ACCENT)),
                text_color: INK,
                border: stroke(ACCENT_BORDER, 1.0, 8.0),
                ..Default::default()
            }
        } else {
            let color = match status {
                button::Status::Hovered => Color::from_rgb(0.11, 0.15, 0.20),
                button::Status::Pressed => CHIP_BG,
                _ => Color::TRANSPARENT,
            };
            button::Style {
                background: Some(bg(color)),
                text_color: MUTED,
                border: stroke(Color::TRANSPARENT, 1.0, 8.0),
                ..Default::default()
            }
        }
    }
}

pub fn sort_picker(_theme: &Theme, status: pick_list::Status) -> pick_list::Style {
    pick_list::Style {
        text_color: if matches!(
            status,
            pick_list::Status::Hovered | pick_list::Status::Opened
        ) {
            INK
        } else {
            MUTED
        },
        placeholder_color: FAINT,
        handle_color: MUTED,
        background: bg(
            if matches!(
                status,
                pick_list::Status::Hovered | pick_list::Status::Opened
            ) {
                ELEVATED
            } else {
                CHIP_BG
            },
        ),
        border: stroke(BORDER, 1.0, 8.0),
    }
}

pub fn sort_menu(_theme: &Theme) -> overlay::menu::Style {
    overlay::menu::Style {
        background: bg(ELEVATED),
        border: stroke(BORDER, 1.0, 8.0),
        text_color: INK,
        selected_text_color: INK,
        selected_background: bg(ACCENT_SOFT),
    }
}

/// Elevated control (top-bar pause).
pub fn elevated_button(_t: &Theme, status: button::Status) -> button::Style {
    let (bg_c, border_c, text_c) = match status {
        button::Status::Hovered => (ELEVATED, BORDER, INK),
        button::Status::Pressed => (CHIP_BG, BORDER, INK),
        _ => (SURFACE, BORDER, INK),
    };
    button::Style {
        background: Some(bg(bg_c)),
        text_color: text_c,
        border: stroke(border_c, 1.0, 10.0),
        ..Default::default()
    }
}

/// Window control glyph buttons (— □ ×).
pub fn win_button(_t: &Theme, status: button::Status) -> button::Style {
    let (bg_c, text_c) = match status {
        button::Status::Hovered => (ELEVATED, INK),
        button::Status::Pressed => (CHIP_BG, INK),
        _ => (Color::TRANSPARENT, FAINT),
    };
    button::Style {
        background: Some(bg(bg_c)),
        text_color: text_c,
        border: stroke(Color::TRANSPARENT, 1.0, 6.0),
        ..Default::default()
    }
}

pub fn input_style(_t: &Theme, status: text_input::Status) -> text_input::Style {
    let (border_c, bg_c) = match status {
        text_input::Status::Focused => (ACCENT, Color::TRANSPARENT),
        _ => (Color::TRANSPARENT, Color::TRANSPARENT),
    };
    text_input::Style {
        background: bg(bg_c),
        border: stroke(border_c, 1.0, 8.0),
        icon: FAINT,
        placeholder: FAINT,
        value: INK,
        selection: ACCENT_SOFT,
    }
}

pub fn section_label<'a>(s: impl iced::widget::text::IntoFragment<'a>) -> text::Text<'a, Theme> {
    text(s).size(13).color(MUTED).font(crate::font::name_font())
}

/// Container chrome matching a quiet chip (sort dropdown / view toggle).
pub fn chip_container(_t: &Theme) -> container::Style {
    container::Style {
        background: Some(bg(CHIP_BG)),
        text_color: Some(MUTED),
        border: stroke(BORDER, 1.0, 8.0),
        ..Default::default()
    }
}

pub fn scroll_style(_t: &Theme, status: scrollable::Status) -> scrollable::Style {
    let thumb = if matches!(status, scrollable::Status::Active) {
        BORDER
    } else {
        MUTED
    };
    let rail = scrollable::Rail {
        background: None,
        border: Border::default(),
        scroller: scrollable::Scroller {
            color: thumb,
            border: stroke(Color::TRANSPARENT, 0.0, 3.0),
        },
    };
    scrollable::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: None,
    }
}

pub fn meta<'a>(s: impl iced::widget::text::IntoFragment<'a>) -> text::Text<'a, Theme> {
    text(s).size(12).color(MUTED)
}

pub fn body<'a>(s: impl iced::widget::text::IntoFragment<'a>) -> text::Text<'a, Theme> {
    text(s).size(13).color(INK)
}

pub fn pad_narrow() -> Padding {
    Padding {
        top: 8.0,
        right: 12.0,
        bottom: 8.0,
        left: 12.0,
    }
}

#[allow(dead_code)]
pub fn pad_card() -> Padding {
    Padding {
        top: 10.0,
        right: 14.0,
        bottom: 10.0,
        left: 14.0,
    }
}

pub fn pad_panel() -> Padding {
    Padding {
        top: 16.0,
        right: 16.0,
        bottom: 16.0,
        left: 16.0,
    }
}

pub fn pad(top: f32, right: f32, bottom: f32, left: f32) -> Padding {
    Padding {
        top,
        right,
        bottom,
        left,
    }
}

/// Timeline spine segment (vertical line).
pub fn spine_line<'a, Message>(
    height: f32,
) -> container::Container<'a, Message, Theme, iced::Renderer>
where
    Message: 'a,
{
    use iced::{widget::Space, Length};
    container(Space::new(Length::Fixed(2.0), Length::Fixed(height)))
        .width(Length::Fixed(2.0))
        .height(Length::Fixed(height))
        .style(|_t: &Theme| container::Style {
            background: Some(bg(Color::from_rgba(0.24, 0.55, 0.98, 0.4))),
            ..Default::default()
        })
}

#[allow(dead_code)]
pub fn flow_line<'a, Message>(
    height: f32,
) -> container::Container<'a, Message, Theme, iced::Renderer>
where
    Message: 'a,
{
    use iced::{widget::Space, Length};
    container(Space::new(Length::Fixed(2.0), Length::Fixed(height)))
        .width(Length::Fixed(2.0))
        .height(Length::Fixed(height))
        .style(|_t: &Theme| container::Style {
            background: Some(bg(ACCENT)),
            ..Default::default()
        })
}

/// Timeline spine fill (rest of the row).
#[allow(dead_code)]
pub fn spine_fill<'a, Message>() -> container::Container<'a, Message, Theme, iced::Renderer>
where
    Message: 'a,
{
    use iced::{widget::Space, Length};
    container(Space::new(Length::Fixed(2.0), Length::Fixed(24.0)))
        .width(Length::Fixed(2.0))
        .height(Length::Fixed(24.0))
        .style(|_t: &Theme| container::Style {
            background: Some(bg(Color::from_rgba(0.24, 0.55, 0.98, 0.4))),
            ..Default::default()
        })
}

/// Timeline node dot.
pub fn spine_dot<'a, Message>(
    active: bool,
) -> container::Container<'a, Message, Theme, iced::Renderer>
where
    Message: 'a,
{
    use iced::{widget::Space, Length};
    let color = if active { ACCENT } else { BORDER };
    container(Space::new(Length::Fixed(10.0), Length::Fixed(10.0)))
        .width(Length::Fixed(10.0))
        .height(Length::Fixed(10.0))
        .style(move |_t: &Theme| container::Style {
            background: Some(bg(color)),
            border: stroke(Color::from_rgb(0.40, 0.70, 1.0), 1.5, 5.0),
            ..Default::default()
        })
}

/// Small keycap (Ctrl / K).
pub fn keycap<'a, Message>(
    label: &'a str,
) -> container::Container<'a, Message, Theme, iced::Renderer>
where
    Message: 'a,
{
    container(
        text(label)
            .size(11)
            .color(MUTED)
            .font(crate::font::name_font()),
    )
    .padding(pad(3.0, 7.0, 3.0, 7.0))
    .style(|_t: &Theme| container::Style {
        background: Some(bg(ELEVATED)),
        text_color: Some(MUTED),
        border: stroke(BORDER, 1.0, 5.0),
        ..Default::default()
    })
}
