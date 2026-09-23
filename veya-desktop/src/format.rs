//! Display helpers. Confidence marks come from `SourceConfidence` — never stored in exe names.

use veya_core::{ClipboardPayload, SourceConfidence};

/// Coarse clipboard content class for list icons and filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentKind {
    Link,
    Code,
    Text,
    File,
    Image,
}

impl ContentKind {
    pub fn label(self) -> &'static str {
        match self {
            ContentKind::Link => "URL",
            ContentKind::Code => "代码",
            ContentKind::Text => "文本",
            ContentKind::File => "文件",
            ContentKind::Image => "图片",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            ContentKind::Link => "看起来是网页链接",
            ContentKind::Code => "看起来像代码片段",
            ContentKind::Text => "纯文本内容",
            ContentKind::File => "本地文件与文件夹列表",
            ContentKind::Image => "剪贴板图像像素",
        }
    }
}

/// Classify from the captured format. Only actual text content uses heuristics.
pub fn payload_kind(payload: &ClipboardPayload) -> ContentKind {
    match payload {
        ClipboardPayload::Text(text) => content_kind(text),
        ClipboardPayload::Files(_) => ContentKind::File,
        ClipboardPayload::Image { .. } => ContentKind::Image,
    }
}

pub fn content_kind(text: &str) -> ContentKind {
    let t = text.trim();
    if t.starts_with("http://") || t.starts_with("https://") || t.starts_with("www.") {
        return ContentKind::Link;
    }
    if t.contains("://") && t.split_whitespace().count() <= 3 {
        return ContentKind::Link;
    }
    let codey = [
        "SELECT ",
        "INSERT ",
        "UPDATE ",
        "DELETE ",
        "function ",
        "=>",
        "const ",
        "let ",
        "var ",
        "def ",
        "class ",
        "import ",
        "#include",
        "public ",
        "private ",
        "fn ",
        "struct ",
        "impl ",
    ];
    if codey.iter().any(|p| t.contains(p)) || t.contains('{') && t.contains(';') {
        return ContentKind::Code;
    }
    ContentKind::Text
}

/// Short list preview that preserves payload type and avoids showing full paths.
pub fn payload_preview(payload: &ClipboardPayload, text_preview: &str) -> String {
    match payload {
        ClipboardPayload::Text(_) => preview_line(text_preview),
        ClipboardPayload::Files(paths) => {
            let first = paths
                .first()
                .map(|path| path.rsplit(['\\', '/']).next().unwrap_or(path.as_str()))
                .unwrap_or("文件列表");
            if paths.len() > 1 {
                format!("{first} · 共 {} 项", paths.len())
            } else {
                first.to_string()
            }
        }
        ClipboardPayload::Image { width, height, .. } => format!("图片 · {width} × {height}"),
    }
}

pub fn byte_size_label(bytes: usize) -> String {
    const KB: usize = 1024;
    const MB: usize = 1024 * KB;
    if bytes < KB {
        format!("{bytes} B")
    } else if bytes < MB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    }
}

pub fn preview_line(text: &str) -> String {
    let mut out = String::new();
    for (index, ch) in text.chars().enumerate() {
        if index >= 40 {
            out.push('…');
            break;
        }
        let ch = match ch {
            '\r' | '\n' | '\t' => ' ',
            c if (c as u32) < 0x20 => ' ',
            c => c,
        };
        out.push(ch);
    }
    out
}

pub fn source_label(app: &str, confidence: SourceConfidence) -> String {
    confidence.display_source(app)
}

pub fn time_label(ms: i64) -> String {
    veya_windows::platform::time::local_hhmm(ms)
}

/// `刚刚` / `N 分钟前` / `N 小时前` / `N 天前`.
pub fn relative_time(ms: i64, now_ms: i64) -> String {
    let diff = (now_ms - ms).max(0);
    if diff < 60_000 {
        "刚刚".into()
    } else if diff < 3_600_000 {
        format!("{} 分钟前", diff / 60_000)
    } else if diff < 86_400_000 {
        format!("{} 小时前", diff / 3_600_000)
    } else {
        format!("{} 天前", diff / 86_400_000)
    }
}

/// `2024-01-22 16:32:18` (local).
pub fn full_time_label(ms: i64) -> String {
    veya_windows::platform::time::local_datetime(ms)
}

/// `14:28 – 14:32` when a card aggregates several copies; single copy → empty.
pub fn time_range_label(first_ms: i64, last_ms: i64) -> String {
    if last_ms - first_ms < 1_000 {
        return String::new();
    }
    format!("{} – {}", time_label(first_ms), time_label(last_ms))
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

#[cfg(test)]
mod tests {
    use super::{payload_kind, ContentKind};
    use veya_core::ClipboardPayload;

    #[test]
    fn payload_kind_uses_real_clipboard_format_and_does_not_guess_from_paths() {
        assert_eq!(
            payload_kind(&ClipboardPayload::Text(
                r"C:\Users\admin\Pictures\photo.png".into()
            )),
            ContentKind::Text
        );
        assert_eq!(
            payload_kind(&ClipboardPayload::Files(vec![
                r"C:\Users\admin\Pictures\photo.png".into()
            ])),
            ContentKind::File
        );
        assert_eq!(
            payload_kind(&ClipboardPayload::Image {
                png: vec![137, 80, 78, 71],
                width: 10,
                height: 20,
            }),
            ContentKind::Image
        );
    }
}
