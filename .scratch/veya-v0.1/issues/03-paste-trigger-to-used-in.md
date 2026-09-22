# 03: PasteTrigger → Used in / Flow detail

**What to build:** Copy then press Ctrl+V or Shift+Insert in another app; select the History card and see Flow detail on the right: "Copied from" (source, time) and "Used in" list (target app, method, time). UI may say Used in; detail must state trigger / insertion not verified. Right-click paste and Win+V are out of scope. Target is the foreground app at trigger time (correct after Alt+Tab).

**Blocked by:** 02 (Copy → History card)

**Status:** ready-for-agent

- [ ] Ctrl+V and Shift+Insert attach a PasteTrigger to the current ClipboardRecord
- [ ] Flow detail shows Copied from + Used in rows (app, method, time)
- [ ] Card Used-in line summarizes targets (e.g. WeChat · IDEA) or count
- [ ] Detail honesty: "Ctrl+V detected" / "Insertion not verified" (not claimed insertion)
- [ ] Foreground app at trigger time is the Used-in target (Alt+Tab safe)
- [ ] Elevated / cross-IL gaps render fallbacks, no crash or invented names

## Comments
