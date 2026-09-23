//! Behavior tests at the pre-agreed seam: `veya-core` FlowEngine public API.

use veya_core::{
    ClipboardChange, ClipboardPayload, FlowEngine, FlowOutcome, InternalClipboardWrite,
    PasteMethod, PasteTrigger, SourceConfidence,
};

fn change(sequence: u32, text: &str, hash: &str, source_exe: &str, t: i64) -> ClipboardChange {
    ClipboardChange {
        sequence,
        payload: ClipboardPayload::Text(text.to_string()),
        content_hash: hash.to_string(),
        source_pid: 1000 + sequence,
        source_exe: source_exe.to_string(),
        source_window: format!("{source_exe} window"),
        source_confidence: SourceConfidence::Exact,
        timestamp_ms: t,
    }
}

fn paste(target_exe: &str, t: i64) -> PasteTrigger {
    PasteTrigger {
        target_pid: 42,
        target_exe: target_exe.to_string(),
        target_window: format!("{target_exe} window"),
        method: PasteMethod::CtrlV,
        timestamp_ms: t,
    }
}

fn paste_method(target_exe: &str, method: PasteMethod, t: i64) -> PasteTrigger {
    PasteTrigger {
        method,
        ..paste(target_exe, t)
    }
}

#[test]
fn clipboard_change_creates_visible_record() {
    let mut flow = FlowEngine::new();
    let outcome = flow.on_clipboard_change(change(1, "hello", "h1", "chrome.exe", 1_000));
    assert_eq!(outcome, FlowOutcome::Recorded { sequence: 1 });
    let rec = flow.record(1).expect("record 1");
    assert_eq!(rec.content, "hello");
    assert_eq!(rec.source_app, "chrome.exe");
    assert_eq!(rec.source_confidence, SourceConfidence::Exact);
    assert!(!rec.has_paste_activity());
}

#[test]
fn paste_trigger_attaches_to_current_record() {
    let mut flow = FlowEngine::new();
    flow.on_clipboard_change(change(1, "hello", "h1", "chrome.exe", 1_000));
    let outcome = flow.on_paste_trigger(paste("notepad.exe", 2_000));
    assert_eq!(outcome, FlowOutcome::PasteAttached { sequence: 1 });
    let rec = flow.record(1).unwrap();
    assert_eq!(rec.pastes.len(), 1);
    assert_eq!(rec.pastes[0].target_app, "notepad.exe");
    assert_eq!(rec.pastes[0].method, PasteMethod::CtrlV);
}

#[test]
fn paste_shift_insert_is_tracked() {
    let mut flow = FlowEngine::new();
    flow.on_clipboard_change(change(1, "x", "h1", "chrome.exe", 1_000));
    flow.on_paste_trigger(paste_method(
        "WindowsTerminal.exe",
        PasteMethod::ShiftInsert,
        2_000,
    ));
    let rec = flow.record(1).unwrap();
    assert_eq!(rec.pastes[0].method, PasteMethod::ShiftInsert);
    assert_eq!(rec.pastes[0].method.label(), "Shift+Insert");
}

#[test]
fn copy_a_and_copy_b_do_not_cross_link() {
    let mut flow = FlowEngine::new();
    flow.on_clipboard_change(change(1, "AAA", "ha", "chrome.exe", 1_000));
    flow.on_clipboard_change(change(2, "BBB", "hb", "code.exe", 2_000));
    flow.on_paste_trigger(paste("notepad.exe", 3_000));
    assert_eq!(flow.record(1).unwrap().pastes.len(), 0);
    assert_eq!(flow.record(2).unwrap().pastes.len(), 1);
    assert_eq!(flow.record(2).unwrap().content, "BBB");
}

#[test]
fn untracked_clipboard_change_breaks_previous_paste_association() {
    let mut flow = FlowEngine::new();
    flow.on_clipboard_change(change(1, "previous", "h1", "chrome.exe", 1_000));

    flow.on_untracked_clipboard_change(2);
    assert_eq!(
        flow.on_paste_trigger(paste("notepad.exe", 2_000)),
        FlowOutcome::PasteWithoutRecord
    );
    assert!(flow.record(1).unwrap().pastes.is_empty());
    assert_eq!(flow.len(), 1);

    flow.on_clipboard_change(change(3, "next", "h3", "code.exe", 3_000));
    assert_eq!(
        flow.on_paste_trigger(paste("notepad.exe", 4_000)),
        FlowOutcome::PasteAttached { sequence: 3 }
    );
}

#[test]
fn many_paste_triggers_accumulate_on_one_record() {
    let mut flow = FlowEngine::new();
    flow.on_clipboard_change(change(1, "hello", "h1", "chrome.exe", 1_000));
    for i in 0..10 {
        flow.on_paste_trigger(paste("notepad.exe", 2_000 + i));
    }
    assert_eq!(flow.record(1).unwrap().pastes.len(), 10);
    assert_eq!(flow.len(), 1);
}

#[test]
fn copy_without_paste_stays_a_valid_record() {
    let mut flow = FlowEngine::new();
    flow.on_clipboard_change(change(7, "SELECT 1", "hs", "datagrip.exe", 1_000));
    let rec = flow.record(7).unwrap();
    assert!(!rec.has_paste_activity());
    assert_eq!(flow.len(), 1);
}

#[test]
fn duplicate_sequence_is_ignored() {
    let mut flow = FlowEngine::new();
    flow.on_clipboard_change(change(1, "hello", "h1", "chrome.exe", 1_000));
    let outcome = flow.on_clipboard_change(change(1, "hello", "h1", "chrome.exe", 1_100));
    assert_eq!(
        outcome,
        FlowOutcome::DuplicateSequenceIgnored { sequence: 1 }
    );
    assert_eq!(flow.len(), 1);
}

#[test]
fn internal_write_suppresses_only_matching_veya_write() {
    let mut flow = FlowEngine::new();
    flow.begin_internal_write(InternalClipboardWrite {
        expected_sequence: Some(9),
        hash: "h-self".to_string(),
    });
    let suppressed =
        flow.on_clipboard_change(change(9, "copied from veya", "h-self", "veya.exe", 5_000));
    assert_eq!(
        suppressed,
        FlowOutcome::SuppressedInternalWrite { sequence: 9 }
    );
    assert!(flow.is_empty());

    // A later user write is a normal record even if source looks like veya.
    let recorded = flow.on_clipboard_change(change(10, "user copy", "h-user", "veya.exe", 6_000));
    assert_eq!(recorded, FlowOutcome::Recorded { sequence: 10 });
    assert_eq!(flow.len(), 1);
}

#[test]
fn internal_write_hash_mismatch_is_not_suppressed() {
    let mut flow = FlowEngine::new();
    flow.begin_internal_write(InternalClipboardWrite {
        expected_sequence: Some(9),
        hash: "h-self".to_string(),
    });
    let outcome = flow.on_clipboard_change(change(9, "other", "h-other", "chrome.exe", 5_000));
    assert_eq!(outcome, FlowOutcome::Recorded { sequence: 9 });
    assert_eq!(flow.len(), 1);
}

#[test]
fn internal_image_write_uses_exact_owner_sequence_when_encoding_changes() {
    let mut flow = FlowEngine::new();
    flow.begin_internal_write(InternalClipboardWrite {
        expected_sequence: None,
        hash: "png-before-replay".into(),
    });
    flow.confirm_internal_write(9, 1234);
    let mut rewritten = change(9, "", "png-after-replay", "veya.exe", 5_000);
    rewritten.source_pid = 1234;
    rewritten.source_confidence = SourceConfidence::Exact;
    assert_eq!(
        flow.on_clipboard_change(rewritten),
        FlowOutcome::SuppressedInternalWrite { sequence: 9 }
    );
    assert!(flow.is_empty());
}

#[test]
fn sequence_only_suppression_requires_the_exact_veya_owner() {
    let mut flow = FlowEngine::new();
    flow.begin_internal_write(InternalClipboardWrite {
        expected_sequence: None,
        hash: "expected".into(),
    });
    flow.confirm_internal_write(9, 1234);
    let mut other = change(9, "other", "different", "other.exe", 5_000);
    other.source_pid = 5678;
    assert_eq!(
        flow.on_clipboard_change(other),
        FlowOutcome::Recorded { sequence: 9 }
    );
}

#[test]
fn unsequenced_internal_write_token_expires_after_the_next_different_event() {
    let mut flow = FlowEngine::new();
    flow.begin_internal_write(InternalClipboardWrite {
        expected_sequence: None,
        hash: "h-self".to_string(),
    });

    let first = flow.on_clipboard_change(change(9, "other", "h-other", "code.exe", 5_000));
    assert_eq!(first, FlowOutcome::Recorded { sequence: 9 });

    let later =
        flow.on_clipboard_change(change(10, "copied from Veya", "h-self", "code.exe", 6_000));
    assert_eq!(later, FlowOutcome::Recorded { sequence: 10 });
    assert_eq!(flow.len(), 2);
}

#[test]
fn history_cards_aggregate_short_window_identical_copies_without_merging_raw() {
    let mut flow = FlowEngine::new();
    flow.on_clipboard_change(change(1, "hello", "h1", "GameViewer.exe", 1_000));
    flow.on_clipboard_change(change(2, "hello", "h1", "GameViewer.exe", 2_000));
    flow.on_clipboard_change(change(3, "hello", "h1", "GameViewer.exe", 5_000));
    // Different content stays separate.
    flow.on_clipboard_change(change(4, "other", "h2", "GameViewer.exe", 6_000));

    assert_eq!(flow.len(), 4, "raw events must not merge");

    let cards = flow.history_cards();
    assert_eq!(cards.len(), 2);
    // 最新优先：B（seq 4）在前，聚合的 A 在后
    assert_eq!(cards[0].copy_count, 1);
    assert_eq!(cards[0].raw_sequences, vec![4]);
    assert_eq!(cards[1].copy_count, 3);
    assert_eq!(cards[1].raw_sequences, vec![1, 2, 3]);
}

#[test]
fn history_cards_combine_used_in_across_aggregated_copies() {
    let mut flow = FlowEngine::new();
    flow.on_clipboard_change(change(1, "hello", "h1", "chrome.exe", 1_000));
    flow.on_paste_trigger(paste("idea64.exe", 1_500));
    flow.on_clipboard_change(change(2, "hello", "h1", "chrome.exe", 2_000));
    flow.on_paste_trigger(paste("weixin.exe", 2_500));

    let cards = flow.history_cards();
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].copy_count, 2);
    assert!(cards[0].has_paste_activity);
    let targets: Vec<_> = cards[0]
        .used_in
        .iter()
        .map(|u| u.target_app.as_str())
        .collect();
    // 使用记录也是最新优先
    assert_eq!(targets, vec!["weixin.exe", "idea64.exe"]);
}

#[test]
fn pinning_a_card_updates_all_raw_copies_without_pinning_future_copies() {
    let mut flow = FlowEngine::new();
    for (sequence, time) in [(1, 1_000), (2, 2_000)] {
        flow.on_clipboard_change(change(sequence, "hello", "h1", "chrome.exe", time));
    }
    let card = &flow.history_cards()[0];
    assert_eq!(card.raw_sequences, vec![1, 2]);
    let sequences = card.raw_sequences.clone();

    assert_eq!(flow.set_pinned(&sequences, true), 2);
    assert!(flow.record(1).unwrap().pinned);
    assert!(flow.record(2).unwrap().pinned);
    flow.on_clipboard_change(change(3, "hello", "h1", "chrome.exe", 3_000));

    let cards = flow.history_cards();
    assert_eq!(cards.len(), 2);
    assert_eq!(cards[0].raw_sequences, vec![3]);
    assert!(!cards[0].pinned);
    assert_eq!(cards[1].raw_sequences, vec![1, 2]);
    assert!(cards[1].pinned);

    assert_eq!(flow.set_pinned(&sequences, false), 2);
    assert!(!flow.record(1).unwrap().pinned);
    assert!(!flow.record(2).unwrap().pinned);
}

#[test]
fn search_matches_content_source_and_target_app() {
    let mut flow = FlowEngine::new();
    flow.on_clipboard_change(change(1, "jwt expired token", "h1", "code.exe", 1_000));
    flow.on_paste_trigger(paste("weixin.exe", 1_500));
    flow.on_clipboard_change(change(2, "hello", "h2", "chrome.exe", 2_000));

    let by_content = flow.search("jwt");
    assert_eq!(by_content.len(), 1);
    assert_eq!(by_content[0].record.sequence, 1);

    let by_source = flow.search("chrome");
    assert_eq!(by_source.len(), 1);
    assert_eq!(by_source[0].record.sequence, 2);

    let by_target = flow.search("weixin");
    assert_eq!(by_target.len(), 1);
    assert_eq!(by_target[0].record.sequence, 1);
}

#[test]
fn delete_record_removes_dependent_paste_triggers() {
    let mut flow = FlowEngine::new();
    flow.on_clipboard_change(change(1, "hello", "h1", "chrome.exe", 1_000));
    flow.on_paste_trigger(paste("notepad.exe", 1_500));
    flow.on_clipboard_change(change(2, "keep", "h2", "code.exe", 2_000));

    assert!(flow.delete_record(1));
    assert!(flow.record(1).is_none());
    assert_eq!(flow.len(), 1);
    assert_eq!(flow.record(2).unwrap().content, "keep");
    // Deleted record's triggers are gone with it (no dangling paste views).
    assert!(flow.search("notepad").is_empty());
}

#[test]
fn source_confidence_labels_are_stable() {
    assert_eq!(SourceConfidence::Exact.as_str(), "exact");
    assert_eq!(SourceConfidence::Likely.as_str(), "likely");
    assert_eq!(SourceConfidence::Unknown.as_str(), "unknown");
}

#[test]
fn payload_hash_separates_types_and_file_boundaries() {
    use veya_core::payload_hash;
    let text = ClipboardPayload::Text("C:\\temp\\a.txt".into());
    let files = ClipboardPayload::Files(vec!["C:\\temp\\a.txt".into()]);
    let other_files = ClipboardPayload::Files(vec!["C:\\temp\\a".into(), ".txt".into()]);
    assert_ne!(payload_hash(&text), payload_hash(&files));
    assert_ne!(payload_hash(&files), payload_hash(&other_files));
}

#[test]
fn file_then_image_paste_is_attached_to_image_only() {
    let mut flow = FlowEngine::new();
    let files = ClipboardPayload::Files(vec![r"C:\temp\a.txt".into()]);
    let mut first = change(1, "", "", "explorer.exe", 1_000);
    first.content_hash = veya_core::payload_hash(&files);
    first.payload = files.clone();
    flow.on_clipboard_change(first);

    let image = ClipboardPayload::Image {
        png: vec![1, 2, 3],
        width: 1,
        height: 1,
    };
    let mut second = change(2, "", "", "snippingtool.exe", 2_000);
    second.content_hash = veya_core::payload_hash(&image);
    second.payload = image.clone();
    flow.on_clipboard_change(second);
    flow.on_paste_trigger(paste("paint.exe", 3_000));

    assert_eq!(flow.record(1).unwrap().payload, files);
    assert!(flow.record(1).unwrap().pastes.is_empty());
    assert_eq!(flow.record(2).unwrap().payload, image);
    assert_eq!(flow.record(2).unwrap().pastes.len(), 1);
}

#[test]
fn restored_or_deleted_history_does_not_claim_new_pastes() {
    let mut flow = FlowEngine::new();
    flow.on_clipboard_change(change(1, "old", "h1", "chrome.exe", 1_000));
    let old = flow.record(1).unwrap().clone();
    flow.clear();
    flow.restore_record(old);
    assert_eq!(
        flow.on_paste_trigger(paste("notepad.exe", 2_000)),
        FlowOutcome::PasteWithoutRecord
    );
    flow.on_clipboard_change(change(2, "new", "h2", "chrome.exe", 3_000));
    flow.delete_record(2);
    assert_eq!(
        flow.on_paste_trigger(paste("notepad.exe", 4_000)),
        FlowOutcome::PasteWithoutRecord
    );
}
