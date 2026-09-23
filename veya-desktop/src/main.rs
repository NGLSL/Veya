//! Veya desktop: UI → Core → Storage / Windows. No Win32 here.

mod app;
mod capture;
mod font;
mod format;
mod icons;
mod theme;
mod tray;
mod worker;

#[cfg(not(windows))]
fn main() {
    eprintln!("Veya is Windows-only");
}

#[cfg(windows)]
fn main() -> iced::Result {
    match veya_windows::platform::singleton::claim() {
        Ok(()) => {}
        Err(veya_windows::platform::singleton::ClaimError::AlreadyRunning) => return Ok(()),
        Err(veya_windows::platform::singleton::ClaimError::CreateFailed(error)) => {
            eprintln!("Veya single-instance guard failed: {error}");
            std::process::exit(1);
        }
    }
    let (activate_tx, activate_rx) = std::sync::mpsc::channel();
    if let Err(error) =
        veya_windows::platform::singleton::spawn_activation_listener(activate_tx.clone())
    {
        eprintln!("Veya activation listener failed: {error}");
    }
    let ui_font = font::install();
    let placement = veya_windows::platform::window_place::startup_placement(1080.0, 700.0);
    let size = placement
        .map(|p| iced::Size::new(p.logical_size.0, p.logical_size.1))
        .unwrap_or_else(|| iced::Size::new(1080.0, 700.0));
    iced::application("Veya", app::App::update, app::App::view)
        .subscription(app::App::subscription)
        .theme(app::App::theme)
        .default_font(ui_font)
        .window(iced::window::Settings {
            size,
            position: iced::window::Position::Centered,
            visible: false,
            decorations: false,
            icon: window_icon(),
            ..Default::default()
        })
        .run_with(move || {
            app::App::boot(
                placement.map(|p| p.physical_position),
                size,
                activate_rx,
                activate_tx,
            )
        })
}

/// Window / taskbar icon from the multi-size set under `icons/`.
#[cfg(windows)]
fn window_icon() -> Option<iced::window::Icon> {
    const PNG: &[u8] = include_bytes!("../../icons/256x256.png");
    let img = image::load_from_memory(PNG).ok()?.into_rgba8();
    let (w, h) = img.dimensions();
    iced::window::icon::from_rgba(img.into_raw(), w, h).ok()
}
