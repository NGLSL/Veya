# Local window rendering lifecycle fixes

This directory contains the crates.io `iced_winit` 0.13.0 package source,
normalized Cargo manifest and README. The MIT license is retained from Iced.
Registry checksum: `f44cd4e1c594b6334f409282937bf972ba14d31fedf03c23aa595d982a2fda28`.

Source modifications are limited to `src/program.rs` and
`src/program/window_manager.rs`. On Windows, the `NewEvents`
`Init | ResumeTimeReached` branch enqueues `ChangeFlow(Wait)` before
requesting a redraw. Hidden Win32 windows do not acknowledge the scheduled paint
normally, so a past `WaitUntil` otherwise fires `ResumeTimeReached` repeatedly
and keeps a UI thread busy. A visible redraw continues to install the next widget
deadline; the existing runner guard retains any future deadline.

The two deprecated `UnboundedReceiver::try_next` calls use `try_recv` instead.
The control queue still stops draining when empty or closed. The event queue
still awaits a new event when empty and exits when closed and drained; receiving
an event still takes the immediate path. This adapts the new `Result<T, _>` API
without changing the previous `Result<Option<T>, _>` behavior.

Veya uses this copy through the root `[patch.crates-io]`. Remove the patch only
after an upstream runtime upgrade passes the hidden CPU regression and visible
caret/reopen checks described in `docs/DEVELOPMENT.md`.

Window surfaces are created lazily at the current non-zero viewport size.
Hidden and minimized windows drop their entire compositor surface and replace
the renderer, releasing softbuffer/GDI pixels, clip masks, damage history and
renderer image/vector/glyph caches while retaining the native window, application,
worker and widget state. Ordinary occlusion does not suspend rendering.
UI updates, layout and widget operations still run while hidden; their backend
caches are released afterwards so background messages cannot retain drawing data.
Show/restore requests a redraw and recreates the surface before presentation;
a fresh surface has no damage history and paints the whole first frame. Existing
Windows unchanged-damage presentation and DPI-sensitive rendering are retained.

Screenshots relayout and draw the current UI and configure a temporary surface
when needed, including while hidden. Hidden/minimized screenshot resources are
released after capturing. A zero-size viewport cancels the screenshot response
without creating a surface or allocating an invalid pixel map.

Do not edit the shared Cargo registry cache. All other source files match the
upstream package.
