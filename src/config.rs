//! Kullanıcı ayarları (config.json): genel kısayol ve oturum açılışında başlatma.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const MOD_ALT: u32 = 0x1;
pub const MOD_CONTROL: u32 = 0x2;
pub const MOD_SHIFT: u32 = 0x4;
pub const MOD_WIN: u32 = 0x8;
pub const VK_F9: u32 = 0x78;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Hotkey {
    pub mods: u32,
    pub vk: u32,
}

impl Default for Hotkey {
    fn default() -> Self {
        Hotkey { mods: 0, vk: VK_F9 }
    }
}

impl Hotkey {
    pub fn label(&self) -> String {
        let mut s = String::new();
        for (m, n) in [(MOD_CONTROL, "Ctrl+"), (MOD_ALT, "Alt+"), (MOD_SHIFT, "Shift+"), (MOD_WIN, "Win+")] {
            if self.mods & m != 0 {
                s.push_str(n);
            }
        }
        s.push_str(&vk_name(self.vk));
        s
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Config {
    pub hotkey: Hotkey,
    pub autostart: bool,
    /// "Terminal aç" ile açılacak program: "auto" (Windows Terminal varsa o, yoksa PowerShell),
    /// "wt", "powershell", "cmd", "hyper", "custom".
    pub terminal: String,
    /// "custom" seçiliyse programın tam yolu.
    pub terminal_custom: String,
}

impl Default for Config {
    fn default() -> Self {
        Config { hotkey: Hotkey::default(), autostart: true, terminal: "auto".into(), terminal_custom: String::new() }
    }
}

impl Config {
    fn file() -> PathBuf {
        crate::util::data_dir().join("config.json")
    }
    /// (ayarlar, ilk çalıştırma mı)
    pub fn load() -> (Config, bool) {
        match crate::util::read_retry(&Self::file()) {
            Some(b) => (serde_json::from_slice(&b).unwrap_or_default(), false),
            None => (Config::default(), true),
        }
    }
    pub fn save(&self) {
        if let Ok(b) = serde_json::to_vec_pretty(self) {
            let _ = crate::util::atomic_write(&Self::file(), &b);
        }
    }
}

pub fn vk_name(vk: u32) -> String {
    match vk {
        0x70..=0x87 => format!("F{}", vk - 0x6f),
        0x30..=0x39 | 0x41..=0x5a => char::from(vk as u8).to_string(),
        0x20 => "Space".into(),
        0x2d => "Insert".into(),
        0x24 => "Home".into(),
        0x23 => "End".into(),
        0x21 => "PageUp".into(),
        0x22 => "PageDown".into(),
        0xc0 => "`".into(),
        _ => format!("0x{vk:02X}"),
    }
}

/// egui tuşu → Windows sanal tuş kodu (kısayol olarak izin verilenler).
pub fn vk_of(key: eframe::egui::Key) -> Option<u32> {
    use eframe::egui::Key as K;
    let name = key.name();
    if let Some(n) = name.strip_prefix('F').and_then(|d| d.parse::<u32>().ok())
        && (1..=24).contains(&n)
    {
        return Some(0x6f + n);
    }
    Some(match key {
        K::Space => 0x20,
        K::Insert => 0x2d,
        K::Home => 0x24,
        K::End => 0x23,
        K::PageUp => 0x21,
        K::PageDown => 0x22,
        K::Backtick => 0xc0,
        _ => {
            let c = name.chars().next()?;
            if name.chars().count() == 1 && (c.is_ascii_uppercase() || c.is_ascii_digit()) {
                c as u32
            } else {
                return None;
            }
        }
    })
}

/// Kısayol makul mü? (tek başına harf/rakam/boşluk olmaz; F tuşları ve gezinme tuşları tek başına olabilir)
pub fn valid_hotkey(h: Hotkey) -> bool {
    let plain_ok = matches!(h.vk, 0x70..=0x87 | 0x2d | 0x24 | 0x23 | 0x21 | 0x22);
    h.vk != 0 && (plain_ok || h.mods & (MOD_CONTROL | MOD_ALT | MOD_WIN) != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::Key;
    #[test]
    fn keys() {
        assert_eq!(Hotkey::default().label(), "F9");
        assert_eq!(vk_of(Key::F9), Some(VK_F9));
        assert_eq!(vk_of(Key::A), Some(0x41));
        assert_eq!(vk_of(Key::Num5), Some(0x35));
        assert_eq!(vk_of(Key::Space), Some(0x20));
        assert_eq!(Hotkey { mods: MOD_CONTROL | MOD_SHIFT, vk: 0x42 }.label(), "Ctrl+Shift+B");
        assert!(valid_hotkey(Hotkey { mods: 0, vk: VK_F9 }));
        assert!(!valid_hotkey(Hotkey { mods: 0, vk: 0x41 }));
        assert!(!valid_hotkey(Hotkey { mods: MOD_SHIFT, vk: 0x41 }));
        assert!(valid_hotkey(Hotkey { mods: MOD_ALT, vk: 0x20 }));
    }
    #[test]
    fn old_config_gets_terminal_default() {
        // v0.4 config.json (terminal alanı yok) sorunsuz okunur
        let c: Config = serde_json::from_str(r#"{"hotkey":{"mods":0,"vk":120},"autostart":true}"#).unwrap();
        assert_eq!(c.terminal, "auto");
        assert!(c.terminal_custom.is_empty());
        assert_eq!(c.hotkey.vk, VK_F9);
        let back: Config = serde_json::from_str(&serde_json::to_string(&Config { terminal: "hyper".into(), ..Default::default() }).unwrap()).unwrap();
        assert_eq!(back.terminal, "hyper");
    }
}
