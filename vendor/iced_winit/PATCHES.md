# Local Windows redraw deadline fix

This directory contains the crates.io `iced_winit` 0.13.0 package source,
normalized Cargo manifest and README. The MIT license is retained from Iced.
Registry checksum: `f44cd4e1c594b6334f409282937bf972ba14d31fedf03c23aa595d982a2fda28`.

The sole source modification is in `src/program.rs`, in the `NewEvents`
`Init | ResumeTimeReached` branch. On Windows, enqueue `ChangeFlow(Wait)` before
requesting a redraw. Hidden Win32 windows do not acknowledge the scheduled paint
normally, so a past `WaitUntil` otherwise fires `ResumeTimeReached` repeatedly
and keeps a UI thread busy. A visible redraw continues to install the next widget
deadline; the existing runner guard retains any future deadline.

Veya uses this copy through the root `[patch.crates-io]`. Remove the patch only
after an upstream runtime upgrade passes the hidden CPU regression and visible
caret/reopen checks in `.scratch/veya-code-optimization/validation/`.

Do not edit the shared Cargo registry cache. All other source files match the
upstream package. Upstream's two deprecated `try_next` calls remain unchanged and
produce warnings now that this dependency is built from a local path.
