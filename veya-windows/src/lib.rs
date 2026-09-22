//! Win32 capture layer. All unsafe/Win32 stays here.
//!
//! Callbacks emit cheap raw events; `enrich_*` resolves names on the worker
//! and produces `veya-core` events. UI and storage are not referenced.

pub mod hash;
pub mod platform;
pub mod tray;

pub use hash::content_hash;
pub use platform::{enrich_clipboard, enrich_paste, run, write_text, PlatformEvent};
pub use tray::{set_paused as set_tray_paused, TrayEvent};
