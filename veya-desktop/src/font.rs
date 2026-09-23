//! 中文字体：只选 CJK 字族，并把字体文件注册进 iced/cosmic-text。
//!
//! Windows 上 `msyh.ttc` 的族名是 `微软雅黑` / `Microsoft YaHei UI`，
//! 系统消息字体却常常是 `Segoe UI`（无汉字）。若按「消息字体优先」就会
//! 整界面豆腐块。这里白名单 CJK 字族，并 `set_sans_serif_family` 兜底。

use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::OnceLock;

use fontdb::Database;
use iced::font::{Family, Font};

struct UiFont {
    font: Font,
}

static UI: OnceLock<UiFont> = OnceLock::new();

/// 仅这些字族可渲染简体中文。拉丁字体永不入选。
const CJK_FAMILIES: &[&str] = &[
    "Microsoft YaHei UI",
    "Microsoft YaHei",
    "微软雅黑",
    "DengXian",
    "等线",
    "SimHei",
    "黑体",
    "Noto Sans SC",
    "Source Han Sans SC",
    "Source Han Sans CN",
    "Noto Sans CJK SC",
    "PingFang SC",
];

/// 已知的 Windows 中文 UI 字体文件（fontdb 名匹配失败时仍加载字节）。
const CJK_FILES: &[&str] = &[
    r"C:\Windows\Fonts\msyh.ttc",
    r"C:\Windows\Fonts\msyhbd.ttc",
    r"C:\Windows\Fonts\Deng.ttf",
    r"C:\Windows\Fonts\simhei.ttf",
];

/// 应用默认 UI 字体（含中文）。首次调用完成选择并注册字形。
pub fn install() -> Font {
    init().font
}

pub fn ui_font() -> Font {
    init().font
}

/// 应用名/标题：与正文同一字面，仅靠字号区分层级。
///
/// 微软雅黑没有 Medium；即使用 Bold，cosmic-text 也常匹配不到
/// `Microsoft YaHei UI` 粗体字面并回退成豆腐块。这里不再改字重。
pub fn name_font() -> Font {
    ui_font()
}

/// 等宽（纯 ASCII 代码/序列可用）。**不要**用于中文——Consolas 无汉字。
#[allow(dead_code)]
pub fn mono_font() -> Font {
    Font {
        family: Family::Name("Consolas"),
        ..ui_font()
    }
}

fn init() -> &'static UiFont {
    UI.get_or_init(|| {
        let mut db = Database::new();
        db.load_system_fonts();

        let mut family: &str = "Microsoft YaHei UI";
        let mut load_paths: Vec<PathBuf> = Vec::new();

        for name in CJK_FAMILIES {
            let mut matched = false;
            for f in db.faces() {
                if f.families.iter().any(|(n, _)| n.eq_ignore_ascii_case(name)) {
                    matched = true;
                    if let fontdb::Source::File(p) = &f.source {
                        if !load_paths.contains(p) {
                            load_paths.push(p.clone());
                        }
                    }
                }
            }
            if matched {
                family = name;
                break;
            }
        }

        for p in CJK_FILES {
            let path = PathBuf::from(p);
            if path.exists() && !load_paths.contains(&path) {
                load_paths.push(path);
            }
        }

        // 注册进 iced 的 cosmic-text FontSystem：Family::Name 才能命中，
        // 且把 SansSerif 默认族改成中文，避免 Font::DEFAULT 再落到 Fira Sans。
        {
            use iced_graphics::text::font_system;
            let mut fs = font_system().write().expect("iced font system");
            for path in &load_paths {
                if let Ok(bytes) = std::fs::read(path) {
                    fs.load_font(Cow::Owned(bytes));
                }
            }
            let db = fs.raw().db_mut();
            db.set_sans_serif_family(family);
            db.set_serif_family(family);
            // 代码用 Consolas；缺字时 cosmic-text 仍会按脚本回退到 CJK。
            db.set_monospace_family("Consolas");
        }

        let family_static: &'static str = Box::leak(family.to_owned().into_boxed_str());
        let font = Font {
            family: Family::Name(family_static),
            ..Font::DEFAULT
        };
        eprintln!(
            "[veya-font] family={family_static} loaded_files={}",
            load_paths.len()
        );
        UiFont { font }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_font_is_a_cjk_family_not_latin() {
        let font = install();
        let Family::Name(name) = font.family else {
            panic!("expected Family::Name, got {:?}", font.family);
        };
        eprintln!("[test] selected ui family = {name}");
        assert!(!name.is_empty());
        for latin in ["Segoe UI", "Fira Sans", "Fira Mono", "Consolas", "Arial"] {
            assert_ne!(name, latin, "must not pick Latin-only family");
        }
    }

    #[test]
    fn name_font_matches_ui_face_exactly() {
        // 任何字重偏离都可能匹配不到微软雅黑字面 → 豆腐块。
        assert_eq!(name_font().weight, ui_font().weight);
        assert_eq!(name_font().family, ui_font().family);
    }
}
