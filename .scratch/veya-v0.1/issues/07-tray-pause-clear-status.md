# 07: Tray + Pause + Clear + status line

**What to build:** Veya lives in the tray (99% background, 1% open). Tray menu: Open Veya, Pause for 10 minutes, Pause tracking, Clear history, Settings, Exit. Pause shows a distinct tray icon state. Main window bottom status: record count · Local only · retention (e.g. `1,284 records · Local only · 30 day history`). Pause stops new tracking; Clear history erases stored flow data.

**Blocked by:** 02 (Copy → History card)

**Status:** done

- [x] Tray icon + context menu with the six actions above — Shell_NotifyIcon + 6 menu items
- [x] Pause for 10 minutes auto-resumes; Pause tracking lasts until resumed — Pause 10 min timer + Pause tracking
- [x] Tray icon reflects paused vs tracking — paused icon (IDI_WARNING) + tip
- [x] While paused, no new ClipboardRecord / PasteTrigger is stored — tracking=false skips store
- [x] Clear history removes all records and dependent paste triggers — clear cascades paste_trigger
- [x] Status line shows count, Local only, retention window — count · Local only · retention

## Comments
