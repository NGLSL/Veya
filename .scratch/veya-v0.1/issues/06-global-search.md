# 06: Global search

**What to build:** One search box on the main window, fuzzy, covering clipboard text, source application, and target application. Searching `wechat` finds items copied from WeChat and items later pasted into WeChat. No query syntax in v0.1 (`from:` / `to:` / `after:` deferred). Search stays fast with thousands of records.

**Blocked by:** 03 (PasteTrigger → Used in / Flow detail)

**Status:** ready-for-agent

- [ ] Single global search input filters History
- [ ] Matches clipboard text content
- [ ] Matches source application name
- [ ] Matches target (Used-in) application name
- [ ] Empty query shows full History; clearing search restores list
- [ ] No advanced syntax required or implied in v0.1

## Comments
