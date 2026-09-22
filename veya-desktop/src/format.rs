//! Display helpers. Confidence marks come from `SourceConfidence` — never stored in exe names.

use veya_core::SourceConfidence;

pub fn preview_line(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        let ch = match ch {
            '\r' | '\n' | '\t' => ' ',
            c if (c as u32) < 0x20 => ' ',
            c => c,
        };
        out.push(ch);
        if out.chars().count() >= 80 {
            out.push('…');
            break;
        }
    }
    out
}

pub fn source_label(app: &str, confidence: SourceConfidence) -> String {
    confidence.display_source(app)
}

pub fn time_label(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let mins = secs.div_euclid(60);
    let hours = mins.div_euclid(24).rem_euclid(24);
    let minutes = mins.rem_euclid(60);
    format!("{hours:02}:{minutes:02}")
}

pub fn confidence_hint(confidence: SourceConfidence) -> &'static str {
    confidence.hint()
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
