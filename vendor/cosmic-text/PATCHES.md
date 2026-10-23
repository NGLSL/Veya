# On-demand font loading

This directory contains the crates.io `cosmic-text` 0.12.1 package's `src`,
normalized Cargo manifest, original manifest, README, changelog, VCS metadata,
and MIT/Apache-2.0 licenses. It is used by the root `[patch.crates-io]`.

Registry source: `registry+https://github.com/rust-lang/crates.io-index`.
Package checksum (the original `Cargo.lock` registry checksum and independently
verified SHA-256 of the cached `cosmic-text-0.12.1.crate` archive):
`59fd57d82eb4bfe7ffa9b1cec0c05e2fd378155b47f255a67983cb4afe0e80c2`.
The local extracted registry source did not contain `.cargo-checksum.json`;
the archive hash records the upstream package identity instead.

Source changes are limited to `src/font/system.rs`: the constructor's existing
monospace font ID clone, `cache_fonts` call, and per-script monospace index
construction run only when `cfg!(feature = "monospace_fallback")` is true.
That feature already controls whether `Font::new` computes the scripts and
codepoints used by the index. Veya does not enable it. Skipping this unused
preload avoids loading every monospace face and starting Rayon's global pool
from that constructor. Enabling the feature retains the original preload and
index construction.

System font discovery, sorted monospace IDs, all on-demand `get_font` loading,
font caching, language and emoji fallback remain unchanged. No global font
system replacement, font database filtering, or Rayon environment changes are
part of this patch.

The normalized manifest omits the explicit `layout` benchmark target because
benchmark sources and their resources are not vendored. All dependencies and
feature definitions are unchanged. Tests, examples, benchmarks, font fixtures,
screenshots, scripts, and registry extraction markers are omitted. All other
retained source files match the registry package. Do not edit the shared Cargo
registry cache.
