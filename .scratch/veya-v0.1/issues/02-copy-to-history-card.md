# 02: Copy → History card

**What to build:** From the user's perspective: copy text in Chrome or Notepad, open Veya, and see a History card on the left with content preview, source application (with confidence marks when inferred), and time. Long text is truncated in the card but stored in full. Unicode (Chinese / multiline / emoji) survives. This is the first complete path: platform capture → core record → storage → list UI.

**Blocked by:** 01 (Prefactor — workspace + veya-core Flow seam)

**Status:** done

- [x] Text clipboard change produces a ClipboardRecord and appears as a History card — SQLite + History card
- [x] Card shows content preview, source app, time — preview/source/time
- [x] SourceConfidence is visible: Exact plain, Likely as `xxx.exe (fg?)` (or similar), Unknown marked; never fake Exact — Exact/Likely(fg?)/Unknown via SourceConfidence
- [x] Likely/Unknown detail copy explains inference ("Source inferred from foreground application…") — confidence_hint in detail
- [x] Long text: full content retained, card truncated; Chinese/multiline/emoji preserved — full content stored, preview truncated
- [x] User-facing honesty: model/docs say PasteTrigger for pastes (this ticket is copy-only) — PasteTrigger wording preserved

## Comments
