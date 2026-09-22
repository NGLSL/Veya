# 07: Tray + Pause + Clear + status line

**What to build:** Veya lives in the tray (99% background, 1% open). Tray menu: Open Veya, Pause for 10 minutes, Pause tracking, Clear history, Settings, Exit. Pause shows a distinct tray icon state. Main window bottom status: record count · Local only · retention (e.g. `1,284 records · Local only · 30 day history`). Pause stops new tracking; Clear history erases stored flow data.

**Blocked by:** 02 (Copy → History card)

**Status:** ready-for-agent

- [ ] Tray icon + context menu with the six actions above
- [ ] Pause for 10 minutes auto-resumes; Pause tracking lasts until resumed
- [ ] Tray icon reflects paused vs tracking
- [ ] While paused, no new ClipboardRecord / PasteTrigger is stored
- [ ] Clear history removes all records and dependent paste triggers
- [ ] Status line shows count, Local only, retention window

## Comments
