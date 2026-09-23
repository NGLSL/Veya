# Veya

Windows-only Rust clipboard flow tracker. Veya records copied text, local file
lists, and DIB images, plus observed application context around later paste shortcuts.

## Start here

- Product scope and current limitations: [README.md](README.md)
- Data flow and ownership: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- Build, test, and Windows runtime checks: [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)
- Local issue workflow: [docs/agents/issue-tracker.md](docs/agents/issue-tracker.md)
- Issue triage roles: [docs/agents/triage-labels.md](docs/agents/triage-labels.md)
- Domain terminology and ADR discovery: [docs/agents/domain.md](docs/agents/domain.md)

## Module map

- `veya-core`: clipboard events, source/target confidence, `FlowEngine`, history aggregation, and search semantics. It does not access Iced, SQLite, or Win32.
- `veya-storage`: SQLite schema, migrations, record/settings/retention persistence. It does not decide event meaning or render UI.
- `veya-windows`: Win32 capture and system effects, process/window metadata, tray, icons, and single-instance activation. It does not sort or aggregate history cards.
- `veya-desktop/src/worker.rs`: owns the running core and store, translates platform events and UI commands, and publishes UI snapshots.
- `veya-desktop/src/app.rs` and `veya-desktop/src/app/`: Iced state, messages, and views. Keep Win32 calls and SQLite queries behind the worker or Windows crate.
- `veya-desktop/src/main.rs`: application and window assembly, font setup, and single-instance startup.

The dependency direction is `veya-desktop` → `veya-core`, `veya-storage`, and
`veya-windows`. Keep the lower crates independent of Iced and keep core
semantics out of view code.

## Invariants

- Raw clipboard records remain distinct from the aggregated history-card view.
- An observed paste shortcut is evidence of a paste attempt, not proof that the target application inserted the text.
- Clipboard payloads are typed as text, existing local file paths, or PNG-backed images. Pinned is a retention attribute on raw records, not a payload type. Preserve the type and pin state through capture, storage, replay, and history aggregation.
- UI changes must preserve source confidence and the distinction between exact, inferred, and unknown context.
- System actions belong in `veya-windows`; a view should not perform repeated Win32 or shell queries during rendering.

## Change loop

1. Inspect `git status` and the relevant call chain before editing; keep unrelated worktree changes intact.
2. Change the smallest responsible crate and keep public interfaces narrow.
3. Run the checks from [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md). Pure logic and storage changes need automated tests; Windows capture, tray, singleton, window, clipboard, and shell behavior need a real Windows check.
4. Update the architecture or development docs when ownership, commands, or user-visible behavior changes.
