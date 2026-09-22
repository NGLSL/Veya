# 01: Prefactor — workspace + veya-core Flow seam

**What to build:** Make the change easy, then make the easy change. Split the PoC into a workspace with `veya-core` / `veya-windows` / `veya-storage` / `veya-desktop`, extract the Flow domain types and use-cases into `veya-core`, and stand up the single automated test seam (inject ClipboardChange / PasteTrigger / InternalClipboardWrite). Dependency direction is UI → Core → Storage / Windows. This ticket is demoable as a green `cargo test` on core and a successful workspace release build; no product UI is required yet.

**Blocked by:** None (can start immediately)

**Status:** ready-for-human

- [x] Workspace exposes `veya-core`, `veya-windows`, `veya-storage`, `veya-desktop` with UI → Core → Storage/Windows direction
- [x] Core owns ClipboardRecord, PasteTrigger, SourceConfidence, aggregation view, InternalClipboardWrite, search/delete use-cases
- [x] Core tests run without Win32 or Iced (injected events only) — `veya-core/tests/flow_behavior.rs` 14/14
- [x] Release build of the workspace succeeds
- [x] Spec vocabulary: PasteTrigger is intent, never claimed insertion; SourceConfidence is Exact | Likely | Unknown

## Comments
