# 08: Privacy settings + exclude + delete

**What to build:** Privacy is on the Settings home, not buried: Tracking ON/OFF; Auto-delete history (1 day / 7 days / 30 days default / Never); Excluded Apps (never track these exes) with add/remove; context menu "Exclude this app" and "Delete" on a record. Excluded apps stop new tracking immediately. Delete / Clear actually remove content and dependent paste triggers. Local only — no cloud path. Pause-when password field / private browser / RDP is out of scope.

**Blocked by:** 02 (Copy → History card), 05 (Re-copy + InternalClipboardWrite)

**Status:** done

- [x] Settings home: Tracking toggle + Auto-delete options, default 30 days — Settings Tracking + 1/7/30/Never default 30d
- [x] Retention purge job honors 1/7/30/Never — periodic purge + on-change purge
- [x] Excluded Apps list + add application — excluded list + add
- [x] Right-click "Exclude this app" adds exclusion; that app is not tracked afterward — Exclude this app button (detail panel; iced has no native menu)
- [x] Right-click Delete removes the record and its paste triggers — Delete removes record + pastes
- [x] Already-stored history for a newly excluded app is removed only via Delete/Clear (v0.1 semantics) — existing history only via Delete/Clear
- [x] Storage remains local only — local SQLite only

## Comments
