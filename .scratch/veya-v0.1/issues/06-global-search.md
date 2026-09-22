# 06: Global search

**What to build:** One search box on the main window, fuzzy, covering clipboard text, source application, and target application. Searching `wechat` finds items copied from WeChat and items later pasted into WeChat. No query syntax in v0.1 (`from:` / `to:` / `after:` deferred). Search stays fast with thousands of records.

**Blocked by:** 03 (PasteTrigger → Used in / Flow detail)

**Status:** done

- [x] Single global search input filters History — global search box
- [x] Matches clipboard text content — content match via match_field
- [x] Matches source application name — source app match
- [x] Matches target (Used-in) application name — target app match
- [x] Empty query shows full History; clearing search restores list — empty query full list
- [x] No advanced syntax required or implied in v0.1 — no query syntax

## Comments
