# 08: Privacy settings + exclude + delete

**What to build:** Privacy is on the Settings home, not buried: Tracking ON/OFF; Auto-delete history (1 day / 7 days / 30 days default / Never); Excluded Apps (never track these exes) with add/remove; context menu "Exclude this app" and "Delete" on a record. Excluded apps stop new tracking immediately. Delete / Clear actually remove content and dependent paste triggers. Local only — no cloud path. Pause-when password field / private browser / RDP is out of scope.

**Blocked by:** 02 (Copy → History card), 05 (Re-copy + InternalClipboardWrite)

**Status:** ready-for-agent

- [ ] Settings home: Tracking toggle + Auto-delete options, default 30 days
- [ ] Retention purge job honors 1/7/30/Never
- [ ] Excluded Apps list + add application
- [ ] Right-click "Exclude this app" adds exclusion; that app is not tracked afterward
- [ ] Right-click Delete removes the record and its paste triggers
- [ ] Already-stored history for a newly excluded app is removed only via Delete/Clear (v0.1 semantics)
- [ ] Storage remains local only

## Comments
