//! Arayüz dili (Türkçe / English). Tüm arayüz metinleri `t!` / `tf!` ile iki dilde yazılır;
//! böylece eksik çeviri derleme anında yakalanır. Dil değişince bir sonraki karede her şey yeni dilde çizilir.

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Tr,
    En,
}

static LANG: AtomicU8 = AtomicU8::new(0);

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::Tr => "tr",
            Lang::En => "en",
        }
    }
    pub fn from_code(s: &str) -> Option<Lang> {
        match s.trim().to_ascii_lowercase().as_str() {
            "tr" => Some(Lang::Tr),
            "en" => Some(Lang::En),
            _ => None,
        }
    }
    /// Ayarlarda görünen ad (her zaman kendi dilinde).
    pub fn label(self) -> &'static str {
        match self {
            Lang::Tr => "Türkçe",
            Lang::En => "English",
        }
    }
    pub fn other(self) -> Lang {
        match self {
            Lang::Tr => Lang::En,
            Lang::En => Lang::Tr,
        }
    }
}

pub fn set(l: Lang) {
    LANG.store(if l == Lang::En { 1 } else { 0 }, Ordering::Relaxed);
}

pub fn get() -> Lang {
    if LANG.load(Ordering::Relaxed) == 1 { Lang::En } else { Lang::Tr }
}

#[inline]
pub fn en() -> bool {
    LANG.load(Ordering::Relaxed) == 1
}

/// Kayıtlı ayar yoksa: Windows arayüz dili Türkçe ise Türkçe, değilse English.
/// `BUL_LANG` (tr/en) ortam değişkeni bunu geçersiz kılar (testler için).
pub fn system_default() -> Lang {
    if let Some(l) = std::env::var("BUL_LANG").ok().and_then(|s| Lang::from_code(&s)) {
        return l;
    }
    // testler (BUL_LANG verilmediyse) her makinede Türkçe çalışır
    if cfg!(test) {
        return Lang::Tr;
    }
    #[cfg(windows)]
    {
        // LANG_TURKISH = 0x1F; birincil dil kimliği alt 10 bit
        let id = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
        if id & 0x3ff == 0x1f { Lang::Tr } else { Lang::En }
    }
    #[cfg(not(windows))]
    {
        let v = std::env::var("LC_ALL").or_else(|_| std::env::var("LC_MESSAGES")).or_else(|_| std::env::var("LANG")).unwrap_or_default();
        if v.to_ascii_lowercase().starts_with("tr") { Lang::Tr } else { Lang::En }
    }
}

/// `t!("Türkçe", "English")` → geçerli dildeki `&'static str`.
macro_rules! t {
    ($tr:literal, $en:literal $(,)?) => {
        if $crate::i18n::en() { $en } else { $tr }
    };
}

/// `tf!("{n} öğe", "{n} items", ...)` → geçerli dilde `format!`.
macro_rules! tf {
    ($tr:literal, $en:literal $(, $a:expr)* $(,)?) => {
        if $crate::i18n::en() { format!($en $(, $a)*) } else { format!($tr $(, $a)*) }
    };
}

/// Dile göre menü harfi: `k('g', 'h')`.
#[inline]
pub fn k(tr_key: char, en_key: char) -> char {
    if en() { en_key } else { tr_key }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn codes_and_labels() {
        for l in [Lang::Tr, Lang::En] {
            assert_eq!(Lang::from_code(l.code()), Some(l));
            assert_eq!(l.other().other(), l);
        }
        assert_eq!(Lang::from_code(" EN "), Some(Lang::En));
        assert_eq!(Lang::from_code("de"), None);
        assert_eq!((Lang::Tr.label(), Lang::En.label()), ("Türkçe", "English"));
        // testlerde (BUL_LANG yoksa) varsayılan Türkçe; global dil değiştirilmez
        if std::env::var_os("BUL_LANG").is_none() {
            assert_eq!(system_default(), Lang::Tr);
        }
    }
}
