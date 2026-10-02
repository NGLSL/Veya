# Local software renderer fixes

This directory contains the crates.io `iced_tiny_skia` 0.13.0 package source and
normalized Cargo manifest. The MIT license is retained from Iced.
Registry checksum: `c625d368284fcc43b0b36b176f76eff1abebe7959dd58bd8ce6897d641962a50`.

Source modifications are limited to `src/engine.rs` and
`src/window/compositor.rs`:

- Zero-width or zero-height quads are valid empty layout regions. Skip them
  before the original assertions and path construction. Quick-panel transitions
  and native minimization can temporarily leave a child with zero available
  width; other invalid dimensions retain the upstream assertions.
- SVGs are rasterized at physical pixel size and position. Apply only rotation
  when presenting that raster; applying viewport DPI scaling again displaces
  and enlarges icons at 125%, 150% and 200%. Keep physical-resolution cache keys
  and the physical rotation center.
- On Windows, when damage is empty, present the existing software pixel buffer.
  Unchanged widget layers do not imply the native window still contains those
  pixels after minimization or exposure. This reuses the buffer without
  rasterizing its contents again or reconfiguring the surface. Other platforms
  retain the upstream empty-damage behavior and buffer-age tracking.

Veya uses this copy through the root `[patch.crates-io]`. Keep these patches until
an upstream renderer upgrade passes the real CPU rendering regressions in
`veya-desktop/src/app/quick.rs`, the SVG pixel/DPI checks in
`veya-desktop/src/icons.rs`, and the Windows minimize/restore checks in
`docs/DEVELOPMENT.md`.

Do not edit the shared Cargo registry cache. All other source files match the
upstream package.
