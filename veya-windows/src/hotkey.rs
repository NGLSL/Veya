//! User-selectable global window activation shortcuts.

pub const MOD_ALT: u32 = 0x1;
pub const MOD_CONTROL: u32 = 0x2;
pub const MOD_SHIFT: u32 = 0x4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hotkey {
    Disabled,
    Keys { modifiers: u32, key: u32 },
}

impl Default for Hotkey {
    fn default() -> Self {
        Self::ALT_V
    }
}

impl Hotkey {
    pub const ALT_V: Self = Self::Keys {
        modifiers: MOD_ALT,
        key: b'V' as u32,
    };

    /// Supported combinations need Ctrl or Alt and a non-reserved main key.
    pub fn new(modifiers: u32, key: u32) -> Option<Self> {
        let valid_modifiers = modifiers != 0
            && modifiers & !(MOD_ALT | MOD_CONTROL | MOD_SHIFT) == 0
            && modifiers & (MOD_ALT | MOD_CONTROL) != 0;
        let valid_key = key == 0x20
            || (b'A' as u32..=b'Z' as u32).contains(&key)
            || (b'0' as u32..=b'9' as u32).contains(&key)
            || (0x70..=0x7A).contains(&key);
        let reserved = modifiers == MOD_ALT && key == 0x73; // Alt+F4
        (valid_modifiers && valid_key && !reserved).then_some(Self::Keys { modifiers, key })
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "disabled" | "关闭快捷键" => return Some(Self::Disabled),
            "alt_v" => return Some(Self::ALT_V),
            "ctrl_alt_v" => return Self::new(MOD_CONTROL | MOD_ALT, b'V' as u32),
            "alt_shift_v" => return Self::new(MOD_ALT | MOD_SHIFT, b'V' as u32),
            _ => {}
        }

        let mut modifiers = 0;
        let mut key = None;
        for part in value.split('+') {
            let part = part.trim().to_ascii_lowercase();
            match part.as_str() {
                "alt" => modifiers |= MOD_ALT,
                "ctrl" | "control" => modifiers |= MOD_CONTROL,
                "shift" => modifiers |= MOD_SHIFT,
                "space" if key.is_none() => key = Some(0x20),
                _ if key.is_none() => {
                    if part.len() == 1 {
                        let ch = part.as_bytes()[0];
                        if ch.is_ascii_alphanumeric() {
                            key = Some(ch.to_ascii_uppercase() as u32);
                        }
                    } else if let Some(number) = part.strip_prefix('f') {
                        let n: u32 = number.parse().ok()?;
                        if (1..=11).contains(&n) {
                            key = Some(0x70 + n - 1);
                        }
                    }
                    if key.is_none() {
                        return None;
                    }
                }
                _ => return None,
            }
        }
        Self::new(modifiers, key?)
    }

    pub fn from_key(value: &str) -> Self {
        Self::parse(value).unwrap_or_default()
    }

    pub fn as_key(self) -> String {
        match self {
            Self::Disabled => "disabled".into(),
            _ => self.to_string(),
        }
    }

    /// Pass a validated combination through the message window's WPARAM.
    pub const fn code(self) -> usize {
        match self {
            Self::Disabled => 0,
            Self::Keys { modifiers, key } => ((modifiers as usize) << 16) | key as usize,
        }
    }

    pub fn from_code(code: usize) -> Option<Self> {
        if code == 0 {
            Some(Self::Disabled)
        } else {
            Self::new((code >> 16) as u32, (code & 0xFFFF) as u32)
        }
    }

    pub const fn registration(self) -> Option<(u32, u32)> {
        match self {
            Self::Disabled => None,
            Self::Keys { modifiers, key } => Some((modifiers, key)),
        }
    }
}

impl std::fmt::Display for Hotkey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self::Keys { modifiers, key } = self else {
            return f.write_str("关闭快捷键");
        };
        if modifiers & MOD_CONTROL != 0 {
            f.write_str("Ctrl+")?;
        }
        if modifiers & MOD_ALT != 0 {
            f.write_str("Alt+")?;
        }
        if modifiers & MOD_SHIFT != 0 {
            f.write_str("Shift+")?;
        }
        match key {
            0x20 => f.write_str("Space"),
            0x70..=0x7A => write!(f, "F{}", key - 0x70 + 1),
            _ => write!(f, "{}", char::from_u32(*key).unwrap_or('?')),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_shortcuts_round_trip_through_storage_and_window_message() {
        for shortcut in [
            "Alt+V",
            "Ctrl+Shift+K",
            "Ctrl+Alt+7",
            "Alt+F9",
            "Ctrl+Space",
        ] {
            let hotkey = Hotkey::parse(shortcut).expect(shortcut);
            assert_eq!(Hotkey::from_key(&hotkey.as_key()), hotkey);
            assert_eq!(Hotkey::from_code(hotkey.code()), Some(hotkey));
        }
        assert_eq!(Hotkey::from_key("alt_v"), Hotkey::ALT_V);
        assert_eq!(Hotkey::from_key("disabled"), Hotkey::Disabled);
        assert_eq!(
            Hotkey::from_key(&Hotkey::Disabled.as_key()),
            Hotkey::Disabled
        );
        assert_eq!(Hotkey::from_key("关闭快捷键"), Hotkey::Disabled);
    }

    #[test]
    fn rejects_unintended_single_keys_and_reserved_f12_or_alt_f4() {
        for shortcut in [
            "V", "Shift+V", "Alt+F4", "Ctrl+F12", "Win+V", "Alt+?", "Alt+",
        ] {
            assert_eq!(Hotkey::parse(shortcut), None, "{shortcut}");
        }
        assert_eq!(Hotkey::from_code(99), None);
    }
}
