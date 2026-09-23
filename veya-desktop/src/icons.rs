//! Inline SVG icons (Material filled, `fill="currentColor"`).
//! iced `svg` tints them via style color — no emoji, no missing-glyph risk.
//! Includes built-in high-fidelity colored SVG logos for popular desktop apps.

use iced::widget::{container, svg, text};
use iced::{Border, Color, Length};

use crate::theme::{self};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Search,
    Settings,
    History,
    Pause,
    Play,
    Link,
    Code,
    Text,
    File,
    Image,
    Pin,
    Copy,
    Check,
    More,
    List,
    Trash,
    External,
    Terminal,
    QrCode,
    Globe,
    Minimize,
    Maximize,
    Close,
}

impl Icon {
    /// Material-style filled path (24×24 viewBox).
    fn path(self) -> &'static str {
        match self {
            Icon::Search => {
                "M15.5 14h-.79l-.28-.27A6.471 6.471 0 0 0 16 9.5 6.5 6.5 0 1 0 9.5 16c1.61 0 3.09-.59 4.23-1.57l.27.28v.79l5 4.99L20.49 19l-4.99-5zm-6 0C7.01 14 5 11.99 5 9.5S7.01 5 9.5 5 14 7.01 14 9.5 11.99 14 9.5 14z"
            }
            Icon::Settings => {
                "M19.14 12.94c.04-.3.06-.61.06-.94 0-.32-.02-.64-.07-.94l2.03-1.58c.18-.14.23-.41.12-.61l-1.92-3.32c-.12-.22-.37-.29-.59-.22l-2.39.96c-.5-.38-1.03-.7-1.62-.94l-.36-2.54c-.04-.24-.24-.41-.48-.41h-3.84c-.24 0-.43.17-.47.41l-.36 2.54c-.59.24-1.13.57-1.62.94l-2.39-.96c-.22-.08-.47 0-.59.22L2.74 8.87c-.12.21-.08.47.12.61l2.03 1.58c-.05.3-.09.63-.09.94s.02.64.07.94l-2.03 1.58c-.18.14-.23.41-.12.61l1.92 3.32c.12.22.37.29.59.22l2.39-.96c.5.38 1.03.7 1.62.94l.36 2.54c.05.24.24.41.48.41h3.84c.24 0 .44-.17.47-.41l.36-2.54c.59-.24 1.13-.56 1.62-.94l2.39.96c.22.08.47 0 .59-.22l1.92-3.32c.12-.22.07-.47-.12-.61l-2.01-1.58zM12 15.6c-1.98 0-3.6-1.62-3.6-3.6s1.62-3.6 3.6-3.6 3.6 1.62 3.6 3.6-1.62 3.6-3.6 3.6z"
            }
            Icon::History => {
                "M13 3c-4.97 0-9 4.03-9 9H1l3.89 3.89.07.14L9 12H6c0-3.87 3.13-7 7-7s7 3.13 7 7-3.13 7-7 7c-1.93 0-3.68-.79-4.94-2.06l-1.42 1.42C8.27 19.99 10.51 21 13 21c4.97 0 9-4.03 9-9s-4.03-9-9-9zm-1 5v5l4.28 2.54.72-1.21-3.5-2.08V8H12z"
            }
            Icon::Pause => "M6 19h4V5H6v14zm8-14v14h4V5h-4z",
            Icon::Play => "M8 5v14l11-7z",
            Icon::Link => {
                "M3.9 12c0-1.71 1.39-3.1 3.1-3.1h4V7H7c-2.76 0-5 2.24-5 5s2.24 5 5 5h4v-1.9H7c-1.71 0-3.1-1.39-3.1-3.1zM8 13h8v-2H8v2zm9-6h-4v1.9h4c1.71 0 3.1 1.39 3.1 3.1s-1.39 3.1-3.1 3.1h-4V17h4c2.76 0 5-2.24 5-5s-2.24-5-5-5z"
            }
            Icon::Code => {
                "M9.4 16.6L4.8 12l4.6-4.6L8 6l-6 6 6 6 1.4-1.4zm5.2 0l4.6-4.6-4.6-4.6L16 6l6 6-6 6-1.4-1.4z"
            }
            Icon::Text => "M5 4v3h5.5v12h3V7H19V4H5z",
            Icon::File => "M6 2h8l5 5v13c0 1.1-.9 2-2 2H6c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2zm7 2H6v16h11V8h-4V4z",
            Icon::Image => "M21 19V5c0-1.1-.9-2-2-2H5C3.9 3 3 3.9 3 5v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2zM5 5h14v14H5V5zm2 12h10l-3.2-4.2-2.3 2.9-1.7-2.1L7 17z",
            Icon::Pin => "M16 9V4l1-1V2H7v1l1 1v5l-2 3v2h5v8l1 1 1-1v-8h5v-2l-2-3z",
            Icon::Copy => {
                "M16 1H4c-1.1 0-2 .9-2 2v14h2V3h12V1zm3 4H8c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h11c1.1 0 2-.9 2-2V7c0-1.1-.9-2-2-2zm0 16H8V7h11v14z"
            }
            Icon::Check => "M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z",
            Icon::More => {
                "M6 10c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2zm12 0c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2zm-6 0c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2z"
            }
            Icon::List => {
                "M3 13h2v-2H3v2zm0 4h2v-2H3v2zm0-8h2V7H3v2zm4 4h14v-2H7v2zm0 4h14v-2H7v2zM7 7v2h14V7H7z"
            }
            Icon::Trash => {
                "M6 19c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2V7H6v12zM19 4h-3.5l-1-1h-5l-1 1H5v2h14V4z"
            }
            Icon::External => {
                "M19 19H5V5h7V3H5c-1.11 0-2 .9-2 2v14c0 1.1.89 2 2 2h14c1.1 0 2-.9 2-2v-7h-2v7zM14 3v2h3.59l-9.83 9.83 1.41 1.41L19 6.41V10h2V3h-7z"
            }
            Icon::Terminal => {
                "M20 4H4c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V6c0-1.1-.9-2-2-2zm0 14H4V8h16v10zm-2-1h-6v-2h6v2zM7.5 17l-1.41-1.41L8.67 13l-2.59-2.59L7.5 9l4 4-4 4z"
            }
            Icon::QrCode => {
                "M3 3h8v8H3V3zm2 2v4h4V5H5zm8-2h8v8h-8V3zm2 2v4h4V5h-4zM3 13h8v8H3v-8zm2 2v4h4v-4H5zm13-2h3v2h-3v-2zm-5 0h2v3h-2v-3zm2 5h2v3h-2v-3zm3 0h3v3h-3v-3zm0-3h3v2h-3v-2zm-2-2h2v2h-2v-2z"
            }
            Icon::Globe => {
                "M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-1 17.93c-3.95-.49-7-3.85-7-7.93 0-.62.08-1.21.21-1.79L9 15v1c0 1.1.9 2 2 2v1.93zm6.9-2.54c-.26-.81-1-1.39-1.9-1.39h-1v-3c0-.55-.45-1-1-1H8v-2h2c.55 0 1-.45 1-1V7h2c1.1 0 2-.9 2-2v-.41c2.93 1.19 5 4.06 5 7.41 0 2.08-.8 3.97-2.1 5.39z"
            }
            Icon::Minimize => "M6 19h12v2H6z",
            Icon::Maximize => "M3 3h18v18H3V3zm2 2v14h14V5H5z",
            Icon::Close => {
                "M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z"
            }
        }
    }
}

fn svg_handle(path: &str) -> svg::Handle {
    let src = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="currentColor" d="{path}"/></svg>"#
    );
    svg::Handle::from_memory(src.into_bytes())
}

/// Fixed-size tinted icon.
pub fn icon<'a>(name: Icon, color: Color, size: f32) -> svg::Svg<'a, iced::Theme> {
    svg(svg_handle(name.path()))
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .style(move |_t, _s| svg::Style { color: Some(color) })
}

pub fn for_kind<'a>(
    kind: crate::format::ContentKind,
    color: Color,
    size: f32,
) -> svg::Svg<'a, iced::Theme> {
    let name = match kind {
        crate::format::ContentKind::Link => Icon::Link,
        crate::format::ContentKind::Code => Icon::Code,
        crate::format::ContentKind::Text => Icon::Text,
        crate::format::ContentKind::File => Icon::File,
        crate::format::ContentKind::Image => Icon::Image,
    };
    icon(name, color, size)
}

/// Stable pastel tint per executable name (fallback when no file icon).
pub fn app_tint(app: &str) -> Color {
    let mut h: u32 = 0;
    for b in app.bytes() {
        h = h.wrapping_mul(16777619) ^ (b as u32);
    }
    let palette = [
        Color::from_rgb(0.23, 0.51, 0.96),
        Color::from_rgb(0.20, 0.65, 0.55),
        Color::from_rgb(0.75, 0.40, 0.85),
        Color::from_rgb(0.90, 0.45, 0.25),
        Color::from_rgb(0.35, 0.55, 0.95),
        Color::from_rgb(0.55, 0.40, 0.90),
        Color::from_rgb(0.25, 0.70, 0.75),
        Color::from_rgb(0.85, 0.55, 0.30),
    ];
    palette[(h as usize) % palette.len()]
}

fn app_icon_handle(app: &str) -> Option<iced::widget::image::Handle> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    static CACHE: OnceLock<Mutex<HashMap<String, Option<iced::widget::image::Handle>>>> =
        OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    let key = app.trim().to_ascii_lowercase();
    if let Ok(map) = cache.lock() {
        if let Some(hit) = map.get(&key) {
            return hit.clone();
        }
    }

    #[cfg(windows)]
    let loaded = veya_windows::platform::icon::exe_icon_rgba(app)
        .map(|ic| iced::widget::image::Handle::from_rgba(ic.width, ic.height, ic.rgba));
    #[cfg(not(windows))]
    let loaded: Option<iced::widget::image::Handle> = None;

    if let Ok(mut map) = cache.lock() {
        map.insert(key, loaded.clone());
    }
    loaded
}

/// Built-in high-quality colored SVG icons for popular desktop apps
/// to guarantee gorgeous visual rendering even when Windows shell path is unmapped.
fn builtin_app_svg_handle(app: &str) -> Option<svg::Handle> {
    let lower = app.to_ascii_lowercase();
    let name = lower.split('.').next().unwrap_or(&lower);

    let svg_xml = match name {
        "weixin" | "wechat" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48">
                <circle cx="24" cy="24" r="22" fill="#07C160"/>
                <path fill="#FFFFFF" d="M19 10C12.92 10 8 14.18 8 19.33c0 2.87 1.48 5.43 3.82 7.14L10.5 31l4.52-2.26c1.23.33 2.55.52 3.98.52.37 0 .73-.02 1.09-.07A9.03 9.03 0 0 1 19 25c0-5.17 4.54-9.33 10.15-9.33.25 0 .49.01.74.03C28.2 11.96 23.95 10 19 10zm-3.25 5.5a2 2 0 1 1 0 4 2 2 0 0 1 0-4zm6.5 0a2 2 0 1 1 0 4 2 2 0 0 1 0-4z"/>
                <path fill="#FFFFFF" d="M29 17c-4.97 0-9 3.58-9 8s4.03 8 9 8c1.1 0 2.14-.18 3.09-.5l3.91 1.95-1.12-3.37C36.18 29.74 38 27.52 38 25c0-4.42-4.03-8-9-8zm-2.5 4.5a1.6 1.6 0 1 1 0 3.2 1.6 1.6 0 0 1 0-3.2zm5 0a1.6 1.6 0 1 1 0 3.2 1.6 1.6 0 0 1 0-3.2z"/>
            </svg>"##,
        ),
        "chrome" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48">
                <circle cx="24" cy="24" r="22" fill="#FFFFFF"/>
                <circle cx="24" cy="24" r="10" fill="#1A73E8"/>
                <path fill="#EA4335" d="M24 4c7.6 0 14.3 4.3 17.6 10.7L24 24h-8.7L24 4z"/>
                <path fill="#FBBC05" d="M41.6 14.7C44.3 19.3 45 25 43.1 30.7L24 24l10.4-18z"/>
                <path fill="#34A853" d="M24 44c-9.1 0-16.7-6.2-19-14.7L15.3 24 24 44z"/>
                <path fill="#FBBC05" d="M4.9 29.3C3.1 23.6 4.3 17.5 7.9 12.7L24 24l-19.1 5.3z"/>
                <circle cx="24" cy="24" r="8" fill="#FFFFFF"/>
                <circle cx="24" cy="24" r="6" fill="#1A73E8"/>
            </svg>"##,
        ),
        "msedge" | "edge" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48">
                <circle cx="24" cy="24" r="22" fill="#0B5C9C"/>
                <path fill="#00D2FF" d="M24 6c9.9 0 18 8.1 18 18-3.1-4-8.1-6-13-6-9 0-15 6-15 13 0 4 2 8 6 10-9.9 0-18-8.1-18-18C2 13 11.9 6 24 6z"/>
                <path fill="#50E3C2" d="M29 18c6 0 11 4 13 10-2 8-9 14-18 14-4 0-8-1-11-4 3 1 7 0 10-2 4-3 6-7 6-11 0-4-3-7-7-7z"/>
            </svg>"##,
        ),
        "idea64" | "idea" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48">
                <rect width="44" height="44" x="2" y="2" rx="8" fill="#000000"/>
                <path fill="#FC801D" d="M6 6h16v16H6z"/>
                <path fill="#FE2857" d="M22 6h20v16H22z"/>
                <path fill="#087CFA" d="M6 22h16v20H6z"/>
                <path fill="#3BE8B0" d="M22 22h20v20H22z"/>
                <rect width="36" height="36" x="6" y="6" rx="6" fill="#1E1F22"/>
                <path fill="#FFFFFF" d="M12 12h4v24h-4zm8 0h12v4H24v6h6v4h-6v6h8v4H20z"/>
            </svg>"##,
        ),
        "datagrip" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48">
                <rect width="44" height="44" x="2" y="2" rx="8" fill="#000000"/>
                <rect width="36" height="36" x="6" y="6" rx="6" fill="#1B2B34"/>
                <path fill="#21D789" d="M14 14h10c5 0 9 3 9 8s-4 8-9 8h-6v4h-4V14zm4 4v8h6c3 0 5-1.5 5-4s-2-4-5-4h-6z"/>
            </svg>"##,
        ),
        "windowsterminal" | "terminal" | "cmd" | "powershell" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48">
                <rect width="44" height="44" x="2" y="2" rx="8" fill="#1E1E1E"/>
                <path fill="#FFFFFF" d="M12 16l8 8-8 8-2.5-2.5 5.5-5.5-5.5-5.5zm12 14h12v3H24z"/>
            </svg>"##,
        ),
        "snipaste" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48">
                <rect width="44" height="44" x="2" y="2" rx="10" fill="#2A3948"/>
                <circle cx="18" cy="18" r="6" stroke="#26A69A" stroke-width="3" fill="none"/>
                <circle cx="18" cy="30" r="6" stroke="#26A69A" stroke-width="3" fill="none"/>
                <path stroke="#FFA726" stroke-width="3" stroke-linecap="round" d="M22 20l14 14M22 28L36 14"/>
            </svg>"##,
        ),
        "explorer" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48">
                <path fill="#FFA000" d="M40 12H22l-4-4H8c-2.2 0-4 1.8-4 4v24c0 2.2 1.8 4 4 4h32c2.2 0 4-1.8 4-4V16c0-2.2-1.8-4-4-4z"/>
                <path fill="#FFCA28" d="M40 16H8c-2.2 0-4 1.8-4 4v16c0 2.2 1.8 4 4 4h32c2.2 0 4-1.8 4-4V20c0-2.2-1.8-4-4-4z"/>
            </svg>"##,
        ),
        "code" | "vscode" => Some(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48">
                <rect width="44" height="44" x="2" y="2" rx="8" fill="#1E1E1E"/>
                <path fill="#007ACC" d="M33 4l-14 13L9 10l-4 3 6 7-6 7 4 3 10-7 14 13V4z"/>
                <path fill="#1F9CF0" d="M33 4v40l10-5V9L33 4z"/>
            </svg>"##,
        ),
        _ => None,
    };

    svg_xml.map(|xml| svg::Handle::from_memory(xml.as_bytes().to_vec()))
}

/// Real file icon when resolvable; otherwise built-in colorful brand icon; fallback to monogram.
pub fn app_avatar<'a, Message>(
    app: &str,
    size: f32,
) -> container::Container<'a, Message, iced::Theme, iced::Renderer>
where
    Message: 'a,
{
    use iced::widget::image;

    let radius = size * 0.22;
    let border_style = move |_t: &iced::Theme| container::Style {
        background: Some(theme::bg(theme::CHIP_BG)),
        text_color: Some(theme::INK),
        border: Border {
            color: Color::from_rgba(0.24, 0.35, 0.48, 0.25),
            width: 1.0,
            radius: radius.into(),
        },
        ..Default::default()
    };

    // 1) Resolved Windows native icon (extracted with GDI mask/alpha & cropped)
    if let Some(handle) = app_icon_handle(app) {
        return container(
            image(handle)
                .width(Length::Fixed(size))
                .height(Length::Fixed(size)),
        )
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .padding(0)
        .style(border_style);
    }

    // 2) Built-in high quality vector brand icon (e.g. WeChat, Chrome, IDEA, VSCode)
    if let Some(handle) = builtin_app_svg_handle(app) {
        return container(
            svg(handle)
                .width(Length::Fixed(size))
                .height(Length::Fixed(size)),
        )
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .padding(0)
        .style(border_style);
    }

    // 3) Clean monogram fallback with pastel tint
    let letter = app
        .trim_end_matches(".exe")
        .trim_end_matches(".EXE")
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".into());
    let bgc = app_tint(app);
    container(
        text(letter)
            .size((size * 0.42).max(11.0))
            .font(crate::font::name_font()),
    )
    .width(Length::Fixed(size))
    .height(Length::Fixed(size))
    .center_x(Length::Fixed(size))
    .center_y(Length::Fixed(size))
    .style(move |_t: &iced::Theme| container::Style {
        background: Some(theme::bg(bgc)),
        text_color: Some(theme::INK),
        border: Border {
            color: Color::from_rgba(1.0, 1.0, 1.0, 0.15),
            width: 1.0,
            radius: radius.into(),
        },
        ..Default::default()
    })
}

/// Large rounded type chip (list + detail header), matching the product mockup.
pub fn type_chip<'a, Message>(
    kind: crate::format::ContentKind,
    size: f32,
) -> container::Container<'a, Message, iced::Theme, iced::Renderer>
where
    Message: 'a,
{
    let icon_size = size * 0.50;
    let (icon_color, chip_bg) = match kind {
        crate::format::ContentKind::Link => (
            Color::from_rgb(0.35, 0.65, 1.0),
            Color::from_rgb(0.09, 0.16, 0.26),
        ),
        crate::format::ContentKind::Code => (
            Color::from_rgb(0.60, 0.55, 0.95),
            Color::from_rgb(0.14, 0.12, 0.24),
        ),
        crate::format::ContentKind::Text => (
            Color::from_rgb(0.75, 0.80, 0.88),
            Color::from_rgb(0.11, 0.14, 0.19),
        ),
        crate::format::ContentKind::File => (
            Color::from_rgb(0.98, 0.70, 0.30),
            Color::from_rgb(0.23, 0.17, 0.09),
        ),
        crate::format::ContentKind::Image => (
            Color::from_rgb(0.40, 0.80, 0.95),
            Color::from_rgb(0.08, 0.19, 0.25),
        ),
    };

    container(for_kind(kind, icon_color, icon_size))
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .center_x(Length::Fixed(size))
        .center_y(Length::Fixed(size))
        .style(move |_t: &iced::Theme| container::Style {
            background: Some(theme::bg(chip_bg)),
            border: Border {
                color: Color::from_rgba(0.35, 0.50, 0.75, 0.22),
                width: 1.0,
                radius: 10.0.into(),
            },
            ..Default::default()
        })
}

pub fn detail_type_chip<'a, Message>(
    kind: crate::format::ContentKind,
    size: f32,
) -> container::Container<'a, Message, iced::Theme, iced::Renderer>
where
    Message: 'a,
{
    let icon_size = size * 0.52;
    let (icon_color, chip_bg) = match kind {
        crate::format::ContentKind::Link => (
            Color::from_rgb(0.40, 0.70, 1.0),
            Color::from_rgb(0.10, 0.22, 0.38),
        ),
        crate::format::ContentKind::Code => (
            Color::from_rgb(0.70, 0.65, 1.0),
            Color::from_rgb(0.18, 0.16, 0.34),
        ),
        crate::format::ContentKind::Text => (
            Color::from_rgb(0.85, 0.90, 0.98),
            Color::from_rgb(0.14, 0.18, 0.26),
        ),
        crate::format::ContentKind::File => (
            Color::from_rgb(1.0, 0.75, 0.35),
            Color::from_rgb(0.30, 0.21, 0.11),
        ),
        crate::format::ContentKind::Image => (
            Color::from_rgb(0.48, 0.85, 1.0),
            Color::from_rgb(0.09, 0.24, 0.32),
        ),
    };

    container(for_kind(kind, icon_color, icon_size))
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .center_x(Length::Fixed(size))
        .center_y(Length::Fixed(size))
        .style(move |_t: &iced::Theme| container::Style {
            background: Some(theme::bg(chip_bg)),
            border: Border {
                color: Color::from_rgba(0.40, 0.70, 1.0, 0.4),
                width: 1.5,
                radius: (size * 0.5).into(), // Circular hero chip
            },
            ..Default::default()
        })
}
