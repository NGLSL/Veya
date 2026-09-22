# 04: Multi-event correctness + aggregation view

**What to build:** Under rapid churn the flow stays truthful and readable: copy A and copy B never cross-link; many Ctrl+V on one content all attach to that record; a copy with no paste stays visible as "No paste activity"; short-window identical copies (e.g. GameViewer writing hello×5) show as one aggregated card ("Copied 5× …") while the database keeps every raw event. Different Used-in sets still aggregate into one visual object with a combined Used-in view.

**Blocked by:** 03 (PasteTrigger → Used in / Flow detail)

**Status:** done

- [x] Copy A / Copy B remain separate records; pastes never attach to the wrong record — core tests A/B + paste attach
- [x] Repeated paste triggers on one record accumulate (list does not explode) — pastes accumulate
- [x] Copy without paste is kept and labeled "No paste activity" — No paste activity label
- [x] UI aggregates near-identical short-window copies (e.g. "Copied 5× within 4s") without merging raw storage rows — AGGREGATION_WINDOW_MS view-only
- [x] Expand/detail can show raw clipboard events for an aggregated group — raw_sequences toggle
- [x] Aggregated group still shows combined Used-in when flows differ per raw copy — combined Used-in on aggregate

## Comments
