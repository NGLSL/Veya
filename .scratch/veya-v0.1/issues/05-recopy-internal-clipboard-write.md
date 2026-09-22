# 05: Re-copy + InternalClipboardWrite

**What to build:** Double-click or Enter on a History item copies its text back to the system clipboard so the user can paste elsewhere — and that Veya-initiated write does **not** create a new History row. Suppression is only for Veya's own pending write (InternalClipboardWrite with expected sequence + content hash), never "ignore everything attributed to Veya.exe", so a legitimate user copy involving a Veya-related window can still be a real Flow.

**Blocked by:** 02 (Copy → History card)

**Status:** ready-for-agent

- [ ] Double-click / Enter (and explicit Copy action) writes clipboard text from the selected record
- [ ] The matching clipboard event is suppressed via InternalClipboardWrite (expected_sequence + hash), not blanket Source==Veya
- [ ] Unrelated or user-driven writes still create normal ClipboardRecords
- [ ] Pending internal token clears after match or timeout; no permanent ignore

## Comments
