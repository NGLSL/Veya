# Veya v0.1 — Clipboard Flow Tracker

Status: ready-for-agent
Feature: veya-v0.1

---
# Veya v0.1 Spec — Clipboard Flow Tracker

## Problem Statement

I regularly copy things on Windows and later cannot answer the questions that actually matter:

- Where did this text originally come from?
- Which apps did I later paste it into?
- I remember copying something days ago, but I have no idea which window or app I first saw it in.

Existing clipboard managers answer only “what did I copy?” as a flat list of contents. That is not enough. The missing product is a **Clipboard Flow Tracker**: content plus source plus destinations, honest about what was observed, private by default, and fast to search.

One-line product definition:

> **Remember where it came from, and where it went.**

Veya is not another Clipboard Manager. The unique value is the **Used in** flow chain, not merely remembering text.

## Solution

Veya is a Windows-only, local-first background app that silently records text clipboard changes and paste hotkey triggers, then lets the user find any clipboard item and read its flow in seconds.

The main window is a two-column layout:

- **Left**: Search + History list. Each card shows content, source app, and Used-in summary (or “No paste activity”).
- **Right**: Flow detail for the selected record — “Copied from” + compact “Used in” timeline (target app, method, time).

Tray menu handles the always-on lifecycle: Open, Pause (10 minutes / until resumed), Clear history, Settings, Exit.

Privacy is a first-class surface (not advanced settings): app exclusion, pause tracking, auto-delete history (default 30 days), clear history, and local-only storage.

Product honesty is non-negotiable:

- Observed paste events are **PasteTrigger** (Ctrl+V / Shift+Insert while an app is foreground), **not** verified insertion.
- Source attribution that is inferred is labeled with **SourceConfidence** (`Exact` / `Likely` / `Unknown`) and never presented as certain when it is not.
- UI may say “Used in …”; detail and data model say trigger / insertion not verified.

Experience goals (in priority order): **常驻无感、记录可信、搜索很快**. Home surface is only Search + History + Flow; everything else lives in Settings.

## User Stories

1. As a knowledge worker, I want Veya to run silently in the tray and record text clipboard flows without interrupting me, so that I can stay focused and still recover provenance later.
2. As a knowledge worker, I want every history item to show content, source app, and Used-in destinations at a glance, so that I never again stare at a bare text blob wondering where it came from.
3. As a knowledge worker, I want to search clipboard flow globally from one box (content, source app, and target app), so that I can find an item even when I only remember “I pasted it into WeChat” or only the source app name.
4. As a knowledge worker, I want a compact Flow detail for each record (Copied from + Used in list), so that I can understand the full path in about three seconds without a complex graph.
5. As a knowledge worker, I want one clipboard item to accumulate many paste triggers (Ctrl+V to IDEA, WeChat, ChatGPT), so that one copy can answer “where did I use this?” completely.
6. As a knowledge worker, I want copy A and copy B to stay separate clipboard records and never cross-link, so that flow data stays trustworthy even under rapid clipboard churn.
7. As a knowledge worker, I want repeated Ctrl+V on the same content to attach to the same clipboard record, so that the history list does not explode with near-duplicates of the same flow.
8. As a knowledge worker, I want Alt+Tab (or other focus changes) immediately before a paste to still attribute the paste to the actual foreground app at trigger time, so that Used-in targets are correct after task switching.
9. As a knowledge worker, I want Shift+Insert treated as a paste trigger just like Ctrl+V, so that terminal and legacy apps still show up in the flow.
10. As a privacy-conscious user, I want source apps that do not expose clipboard owner (for example QQ / OLE NULL owners) shown with a visible inference mark such as `QQ.exe (fg?)`, so that I can trust the data without being lied to.
11. As a privacy-conscious user, I want a hover or detail explanation when source is inferred (“Source inferred from foreground application. Clipboard owner was unavailable.”), so that I understand why confidence is not Exact.
12. As a developer, I want high-privilege / elevated targets still recorded as paste triggers when possible, and honest gaps when not, so that flow coverage is as complete as Windows allows without false claims.
13. As a user of long or multilingual text, I want the full content stored while the list shows a safe truncated preview, so that I can search the real text without overflowing the UI.
14. As a user of Chinese / multiline / emoji text, I want Unicode text preserved end-to-end, so that flow records remain usable for real-world snippets.
15. As a user whose apps write the clipboard repeatedly (for example GameViewer writing the same text several times in seconds), I want UI-level aggregation (“Copied 5× within 4s”) while the database keeps raw events, so that the list stays readable and audit/debug still has ground truth.
16. As a user with different paste flows for the same text, I want aggregation to keep a combined Used-in view (IDEA · WeChat · Terminal) even when copies were repeated, so that the visual object still tells the whole story.
17. As a user who copied but never pasted, I want that record kept and labeled “No paste activity”, so that “what did I just copy?” remains answerable.
18. As a user re-using a past item, I want double-click or Enter on a history item to re-copy it to the system clipboard, so that I can continue work in another app in one gesture.
19. As a user exploring a flow, I want single-click to select and show Flow detail on the right, so that browsing history and reading flow stay in one window.
20. As a user with a right-click menu, I want Copy, Open source, Exclude this app, and Delete as the only record actions in v0.1, so that the interaction stays simple and predictable.
21. As a user clicking “Open source”, I want best-effort launch (file → Explorer, browser → browser/app, other app → activate/open) without promising exact URL restore in v0.1, so that I can jump back to context without a fake feature.
22. As a user re-copying from Veya, I want Veya’s own clipboard writes suppressed as new history rows only when they are Veya-initiated (InternalClipboardWrite with expected sequence + content hash), so that self-copy does not pollute history while user copies from a Veya console (if any) can still be legitimate flow.
23. As a user who lives in the tray, I want Open Veya / Pause for 10 minutes / Pause tracking / Clear history / Settings / Exit on the tray menu, so that the 99% background, 1% open workflow is fast.
24. As a user in a paused state, I want the tray icon to show a clear pause state, so that I always know whether tracking is on.
25. As a privacy-conscious user, I want Tracking on/off and Auto-delete history (1 day / 7 days / 30 days default / Never) on the Settings home, so that retention is visible and controllable without hunting.
26. As a privacy-conscious user, I want an Excluded Apps list (never track these exes) plus “Exclude this app” from a record’s context menu, so that password managers and noisy tools never enter history.
27. As a privacy-conscious user, I want local-only storage (no cloud sync in v0.1), so that content, source, destinations, and times never leave my machine by product design.
28. As a power user with thousands of records, I want search to stay fast and to match clipboard text, source application, and target application, so that finding one flow among many stays a few keystrokes.
29. As a power user, I want v0.1 search to be one global fuzzy box (not advanced query syntax yet), so that the common path is simple; syntax like `from:chrome` / `to:wechat` / `after:…` can wait.
30. As a Flow reader, I want Used-in rows to show target app, method (Ctrl+V / Shift+Insert), and time in a compact list under “Used in”, so that the detail screen answers the product question without a chart.
31. As a Flow reader, I want the list card Used-in line to summarize apps (WeChat · IDEA · ChatGPT) or “Used in 3 apps”, so that scanning many items stays cheap.
32. As a user of multi-window apps, I want window title captured as secondary context when available (and empty/`-` when not), so that “which WeChat chat / which doc” is hinted without promising chat-object precision.
33. As a user of elevated or cross-IL windows, I want missing titles/exe names rendered as accessible fallbacks instead of crashes or fake names, so that records remain honest when Windows denies access.
34. As a user who only cares about text in v0.1, I want non-text clipboard formats ignored rather than half-recorded, so that the product stays coherent (Text only).
35. As a user cleaning up, I want Delete on a record (and Clear history globally) to remove content and related paste triggers, so that privacy actions actually erase data.
36. As a user with auto-delete enabled, I want records older than the retention window purged on a regular local schedule, so that “30 day history” is true without manual maintenance.
37. As a user adding exclusions later, I want newly excluded apps to stop new tracking immediately; I accept that already-stored history for that app is a separate delete/clear action, so that retention semantics stay simple in v0.1.
38. As a developer extending Veya later, I want core flow types (ClipboardRecord, PasteTrigger, SourceConfidence, aggregation) isolated from Win32 and UI, so that product logic can be tested without hooks.
39. As a developer shipping Windows capture, I want all Win32/unsafe confined to the platform layer (clipboard listener, keyboard hook, process/window resolve), so that the rest of the system stays safe Rust.
40. As a developer persisting history, I want SQLite storage of raw clipboard records and paste triggers with an applications table (exe, display_name, path, icon, excluded), so that app names are resolved once and search/UI stay consistent.
41. As a developer implementing the UI, I want Iced (desktop) depending on core, with storage and windows behind core-facing ports, so that UI never calls Win32 directly.
42. As a product owner, I want PasteTrigger never described as verified insertion in UI detail copy (“Ctrl+V detected / Insertion not verified”), so that the product does not fake certainty.
43. As a product owner, I want SourceConfidence degradation rules fixed (owner HWND → Exact; open/foreground heuristic → Likely; nothing usable → Unknown), so that trust labels are mechanical, not vibes.
44. As a product owner, I want “data not merged, UI aggregated” as a standing rule, so that later audit/stats/debug are not destroyed by list cosmetics.
45. As a product owner, I want privacy controls treated as baseline product surface (exclude, pause, auto-expire, clear, local-only), so that sensitive flow data is never an afterthought.
46. As a user of File Explorer / browsers / IDEs / chat apps (Chrome, IDEA, VS Code, Windows Terminal, WeChat, QQ, Office/WPS), I want the same honest flow model across the app matrix already validated in PoC Round2, so that v0.1 starts from proven capture behavior.
47. As a user, I want the bottom status line to show record count and retention mode (“1,284 records · Local only · 30 day history”), so that trust and scope are always visible.
48. As a user who hates chrome-heavy tools, I want no complex navigation and no graph canvas in v0.1 — only Search, History, Flow detail, Settings, Tray — so that the product stays calm and fast.
49. As a future Kite user, I want Veya to remain a complete standalone product first; Kite integration (search / copy / open_flow only) is explicitly later, so that the core is never a plugin-shaped hole.
50. As a user who might need images, files, right-click paste, Win+V, UI Automation, browser extensions, cloud sync, or exact web URL provenance later, I want those explicitly out of v0.1 so that this release ships a trustworthy Text Clipboard Flow rather than an unfinished everything-app.

## Implementation Decisions

### Product semantics (normative)

- Paste observation is **PasteTrigger** / paste **intent** (hotkey + foreground app). Insertion into the target is **not** claimed. UI may say “Used in”; models, docs, and detail copy must say trigger / insertion not verified.
- Source trust is **SourceConfidence**:
  - `Exact` — GetClipboardOwner gave a usable HWND.
  - `Likely` — owner unavailable (e.g. OLE NULL / QQ); approximated via open-clipboard or foreground HWND. Display like `xxx.exe (fg?)`.
  - `Unknown` — nothing usable to attribute. Display like `unknown` / `xxx.exe (?)`.
  - Inference must never be presented as Exact.
- **Data not merged, UI aggregated.** Raw clipboard writes stay separate records (keyed by clipboard sequence / id). The list groups near-identical consecutive copies (same content, same source, short window) into one visual card with “Copied N× …” and optional raw event list on expand.
- Copy with **no paste** is a valid Flow (“No paste activity”); do not hide it.
- Only `CF_UNICODETEXT` (text) is in scope for v0.1.
- Paste hotkeys tracked in v0.1: **Ctrl+V** and **Shift+Insert** only.

### Domain data model

Agreed shapes (from product design; keep field names stable across crates):

```text
ClipboardRecord
  id
  sequence
  content_type
  content
  content_hash
  source_app
  source_pid
  source_confidence
  created_at

PasteTrigger
  id
  clipboard_record_id
  target_app
  target_pid
  method            # CtrlV | ShiftInsert
  confidence        # hotkey observed; insertion unverified
  triggered_at

Application
  exe
  display_name
  path
  icon
  excluded
```

One ClipboardRecord may have many PasteTrigger rows. Application is normalized so `chrome.exe → Google Chrome` is resolved once.

### InternalClipboardWrite (self-copy suppression)

When the user re-copies from Veya (Enter / Copy action), Veya writes the system clipboard and must not spawn a new history row for **that** write. Do **not** ignore all events with Source == Veya (user-driven copies attributed to a Veya-related window can be legitimate flow).

Use a short-lived internal write token, from the prototype decision:

```text
InternalClipboardWrite {
    expected_sequence,
    hash,
}
```

Suppress only the clipboard event that matches the pending Veya-initiated write (sequence and/or content hash), then clear the token. Any other write remains a normal ClipboardRecord.

### Crate / module split

```text
veya-core      ClipboardRecord, PasteTrigger, SourceConfidence, aggregation, flow use-cases
veya-windows   clipboard, keyboard, process, application resolve (all unsafe/Win32)
veya-storage   SQLite persistence
veya-desktop   Iced UI + tray
```

Dependency direction: **UI → Core → Storage / Windows**. UI never touches Win32. Callbacks stay lightweight (channel to worker); process/window title resolution uses timeout-safe APIs (SendMessageTimeoutW pattern from PoC, not blocking GetWindowTextW).

### Capture behavior retained from PoC

- Clipboard: listener + sequence-keyed records; resolve source **before** OpenClipboard to avoid self-attribution of `veya.exe`.
- Keyboard: WH_KEYBOARD_LL observes Ctrl+V / Shift+Insert; record target as foreground app at trigger time.
- Process/window: exe name + window title with hang-safe timeout; cross-IL may be empty — render `-` / fallback, never crash or invent.
- Confidence and method must survive into storage and UI.

### UI (Iced)

- Two columns: History list (left) + Flow detail (right). Global search field above. Status line: record count · Local only · retention.
- History card: truncated content preview, source (with `?` / `(fg?)` marks when inferred), time, Used-in summary or “No paste activity”.
- Flow detail: “Copied from” (source, time) + “Used in” compact list (app, method, time). Prefer the compact tree over a graph.
- Interactions: single-click = select/show Flow; double-click/Enter = re-copy; right-click = Copy, Open source, Exclude this app, Delete.
- Likely-source tooltip/detail: “Source inferred from foreground application. Clipboard owner was unavailable.”
- Detail may show debug fields (pid, hwnd, sequence, confidence) in a collapsed/debug place — not on the card.
- Tray: Open Veya, Pause for 10 minutes, Pause tracking, Clear history, Settings, Exit. Pause changes tray icon state.
- Settings home: Tracking ON/OFF; Auto-delete (1 / 7 / 30 default / Never); Excluded Apps list + add; never-track from context menu. Pause-when password field / private browser / RDP is **not** v0.1.

### Search

- v0.1: one global fuzzy query box covering clipboard text, source application, and target application.
- No query syntax in v0.1 (`from:`, `to:`, `after:` deferred).

### Storage (SQLite)

- Persist ClipboardRecord, PasteTrigger, Application as specified. Keep raw events; aggregation is a read-model / UI concern.
- Local only. No cloud sync.
- Auto-delete purge job honors retention setting (default 30 days). Delete record and Clear history must actually remove content and dependent paste triggers.

### Open source (best effort)

- File path → Explorer; browser-ish → browser/app; else activate/open application.
- Exact web URL restore is **not** promised without a browser extension (later version).

## Testing Decisions

### What makes a good test here

- Test **external behavior** of the product’s public contracts: given observed platform events and user actions, what records, aggregations, confidence labels, and suppressions result — and what the user-visible list/detail semantics are.
- Do **not** test private helpers, Win32 call order, Iced widget trees, or SQL string formatting as behavior.
- Prefer deterministic injection of events over real clipboard/keyboard hooks in automated tests.

### Seams (confirmed with maintainer)

**Single automated seam: `veya-core` Flow domain API.**  
Inject `ClipboardChange` / `PasteTrigger` (and internal write tokens) into the core flow use-cases; assert external behavior:

- record create / attach pastes to the correct clipboard record (no cross-link between copies A and B)
- many triggers on one record; many raw copies grouped only in the aggregation **view** (library keeps raw events)
- SourceConfidence labeling rules (Exact / Likely / Unknown) as presented to upper layers
- InternalClipboardWrite suppresses only the matching Veya-initiated write
- search/filter behavior on text + source app + target app (core-level query result)
- retention/delete semantics that core owns (record delete removes its triggers)

Do **not** add a `veya-storage` or `veya-windows` / `veya-desktop` automated seam in v0.1. Storage must stay behind core ports (schema correctness is covered by implementation + manual checks if needed). Win32 hooks and Iced UI are not automated test surfaces. PoC Round2 scripts remain the manual/real-OS acceptance checklist for capture; automated tests own domain truth via core only.

### Prior art

- PoC domain types and honesty comments already live in the flow store and event types (sequence-keyed records, paste attach, confidence labels). v0.1 tests should target the extracted core API in the same spirit, without console printing as the oracle.
- Round2 scenario matrix (R2-01…08) is the **acceptance checklist** for real-app capture coverage on a human/manual pass, not a substitute for core unit tests.

## Out of Scope

- Image clipboard, file clipboard, and any non-`CF_UNICODETEXT` capture
- Right-click Paste, Win+V, menu Paste, Paste Special
- UI Automation, chat-object-level attribution, tab/file-level attribution
- Browser extension and exact web URL / page-title provenance
- Cloud sync or any non-local data path
- Advanced search syntax (`from:`, `to:`, `after:`, etc.)
- Pin / favorites (candidate for v0.2)
- Application icons polish beyond basic Application.icon field use (v0.2)
- Pause-when password field / private browser window / Remote Desktop
- Kite plugin (search / copy / open_flow) — only after standalone Veya is stable; runtime/storage/tracking stay in Veya
- Graph visualization of flows (timeline list is enough)
- macOS / Linux / cross-platform

### Explicit later-version roadmap (not this spec)

- **v0.2**: files, images, icons, better search, favorites/Pin
- **v0.3**: Chrome/Edge extension → exact URL + page title in Copied from
- **Later**: UI Automation, right-click Paste, Win+V coverage increases; Kite façade

## Further Notes

- Central experience metrics: **常驻无感、记录可信、搜索很快**. Resist feature gravity; homepage stays Search + History + Flow.
- PoC evidence: Windows-only Rust PoC builds and passed Round2 14/14 sub-scenarios (including one copy → many pastes, no sequence cross-talk, elevated target, QQ confidence boundary, Unicode). Two root-cause fixes to preserve: resolve source before OpenClipboard; never block on GetWindowTextW (use timeout abort).
- Real-noise lesson (GameViewer repeated identical writes) is why UI aggregation + app exclusion + privacy are v0.1-required, not polish.
- Suggested defaults: auto-delete **30 days**; local only; Tracking on at install until user pauses or excludes.
- English product line: “Remember where it came from, and where it went.”
- Spec location in-repo: `docs/specs/veya-v0.1-spec.md`. Publish to the project issue tracker with triage label `ready-for-agent` once `docs/agents/issue-tracker.md` exists (run `/setup-matt-pocock-skills` if missing).

