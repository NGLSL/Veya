//! Veya desktop: UI → Core → Storage / Windows. No Win32 here.

mod app;
mod capture;
mod format;
mod tray;
mod worker;

#[cfg(not(windows))]
fn main() {
    eprintln!("Veya is Windows-only");
}

#[cfg(windows)]
fn main() -> iced::Result {
    iced::application("Veya", app::App::update, app::App::view)
        .subscription(app::App::subscription)
        .theme(app::App::theme)
        .window_size((1040.0, 680.0))
        .run_with(app::App::boot)
}
