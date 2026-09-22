# 03: PasteTrigger → Used in / Flow detail

**What to build:** Copy then press Ctrl+V or Shift+Insert in another app; select the History card and see Flow detail on the right: "Copied from" (source, time) and "Used in" list (target app, method, time). UI may say Used in; detail must state trigger / insertion not verified. Right-click paste and Win+V are out of scope. Target is the foreground app at trigger time (correct after Alt+Tab).

**Blocked by:** 02 (Copy → History card)

**Status:** done

- [x] Ctrl+V and Shift+Insert attach a PasteTrigger to the current ClipboardRecord — Ctrl+V / Shift+Insert attach
- [x] Flow detail shows Copied from + Used in rows (app, method, time) — detail rows app+method+time
- [x] Card Used-in line summarizes targets (e.g. WeChat · IDEA) or count — card Used-in app list
- [x] Detail honesty: "Ctrl+V detected" / "Insertion not verified" (not claimed insertion) — detail_copy honesty
- [x] Foreground app at trigger time is the Used-in target (Alt+Tab safe) — foreground app at trigger
- [x] Elevated / cross-IL gaps render fallbacks, no crash or invented names — fallbacks, no invented names

## Comments
