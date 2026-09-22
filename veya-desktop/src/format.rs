//! Display helpers.

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
    match confidence {
        SourceConfidence::Exact => app.to_string(),
        SourceConfidence::Likely => {
            if app.ends_with("(fg?)") {
                app.to_string()
            } else {
                format!("{app} (fg?)")
            }
        }
        SourceConfidence::Unknown => {
            if app.ends_with("(?)") || app == "unknown" {
                app.to_string()
            } else {
                format!("{app} (?)")
            }
        }
    }
}

pub fn time_label(ms: i64) -> String {
    // Local display without chrono: show HH:MM from epoch ms (UTC) plus day offset hint.
    let secs = ms.div_euclid(1000);
    let mins = secs.div_euclid(60);
    let hours = mins.div_euclid(24).rem_euclid(24);
    let minutes = mins.rem_euclid(60);
    format!("{hours:02}:{minutes:02}")
}

pub fn confidence_hint(confidence: SourceConfidence) -> &'static str {
    match confidence {
        SourceConfidence::Exact => "",
        SourceConfidence::Likely => {
            "Source inferred from foreground application. Clipboard owner was unavailable."
        }
        SourceConfidence::Unknown => "Source could not be attributed.",
    }
}
