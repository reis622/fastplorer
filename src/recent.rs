//! Kalıcı uygulama durumu (açma geçmişi, son hedefler) ve Windows "Recent" klasörü.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct State {
    /// (yol, unix zaman) — uygulamadan açılanlar / kullanılanlar.
    pub history: Vec<(String, i64)>,
    /// Son hedefler: en son gidilen/açılan ve yapıştırılan klasörler (en yeni başta).
    /// Birbirine 1-2 klasör yakın olanlar tek kayıt sayılır (bkz. `dests_related`).
    pub dests: Vec<(String, i64)>,
}

impl State {
    fn file() -> PathBuf {
        crate::util::data_dir().join("state.json")
    }
    pub fn load() -> State {
        crate::util::read_retry(&Self::file()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }
    pub fn save(&self) {
        if let Ok(b) = serde_json::to_vec(self) {
            let _ = crate::util::atomic_write(&Self::file(), &b);
        }
    }
    pub fn touch(&mut self, path: &str) {
        let t = crate::util::now_secs();
        self.history.retain(|(p, _)| !p.eq_ignore_ascii_case(path));
        self.history.insert(0, (path.to_string(), t));
        self.history.truncate(200);
    }
    /// Klasörü "Son hedefler"e kaydet: en yeni başta; aynı yer ve 1-2 klasör yakını olan eski
    /// kayıtlar düşer (ör. bir oyun klasörünün alt klasörlerinde gezerken liste onlarla dolmaz).
    pub fn add_dest(&mut self, path: &str) {
        let home = home_dir();
        record_dest(&mut self.dests, path, crate::util::now_secs(), home.as_deref());
    }
}

pub const MAX_DESTS: usize = 15;

fn home_dir() -> Option<String> {
    std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(|h| h.to_string_lossy().to_string())
}

/// Yol bileşenleri (katlanmış, iki ayırıcı da). "C:\\Oyunlar" → ["c:", "oyunlar"].
fn comps(p: &str) -> Vec<String> {
    p.split(['\\', '/']).filter(|s| !s.is_empty()).map(crate::util::fold).collect()
}

/// Kapsayıcı klasörler: kök, kullanıcı klasörü ve üstü, Program Files gibi genel klasörler.
/// Bunların farklı alt klasörleri birbirinin "yakını" sayılmaz.
fn is_container(c: &[String], home: &[String]) -> bool {
    const NAMES: &[&str] = &["users", "kullanıcılar", "program files", "program files (x86)", "programdata", "appdata", "local", "roaming", "locallow", "windows", "home"];
    if c.len() <= 1 {
        return true;
    }
    if !home.is_empty() && (c == home || (home.len() > 1 && c == &home[..home.len() - 1])) {
        return true;
    }
    c.last().is_some_and(|l| NAMES.contains(&l.as_str()))
}

/// Yeni kayıt `new` gelince eski `old` düşmeli mi?
/// - aynı klasör;
/// - `old`, `new`'in 1-2 üst klasörü;
/// - `old`, `new`'in 1-2 alt klasörü (`new` bir kapsayıcı değilse);
/// - ortak üst klasörleri ikisine de en fazla 2 uzaklıkta ve kapsayıcı değil (kardeş/kuzen klasörler).
pub fn dests_related(new: &str, old: &str, home: Option<&str>) -> bool {
    let a = comps(new);
    let b = comps(old);
    if a.is_empty() || b.is_empty() {
        return a == b;
    }
    let h = home.map(comps).unwrap_or_default();
    let k = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    if k == 0 {
        return false; // farklı sürücü
    }
    let (da, db) = (a.len() - k, b.len() - k);
    match (da, db) {
        (0, 0) => true,
        (_, 0) => da <= 2,
        (0, _) => db <= 2 && !is_container(&a, &h),
        _ => da <= 2 && db <= 2 && !is_container(&a[..k], &h),
    }
}

/// Saf kayıt mantığı (test edilir).
pub fn record_dest(dests: &mut Vec<(String, i64)>, path: &str, t: i64, home: Option<&str>) {
    if path.trim().is_empty() {
        return;
    }
    dests.retain(|(p, _)| !dests_related(path, p, home));
    dests.insert(0, (path.to_string(), t));
    dests.truncate(MAX_DESTS);
}

/// %APPDATA%\Microsoft\Windows\Recent içindeki .lnk hedefleri (yol, lnk değişme zamanı).
pub fn windows_recent() -> Vec<(String, i64)> {
    let Some(appdata) = std::env::var_os("APPDATA") else { return Vec::new() };
    let dir = PathBuf::from(appdata).join("Microsoft").join("Windows").join("Recent");
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return out };
    for de in rd.flatten() {
        let p = de.path();
        if !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("lnk")) {
            continue;
        }
        let Ok(md) = de.metadata() else { continue };
        let t = crate::index::mtime_of(&md);
        if let Some(target) = std::fs::read(&p).ok().and_then(|b| lnk_target(&b))
            && Path::new(&target).exists() {
                out.push((target, t));
            }
    }
    out
}

fn rd_u16(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?))
}
fn rd_u32(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}
fn cstr(b: &[u8], o: usize) -> Option<String> {
    let s = b.get(o..)?;
    let end = s.iter().position(|&c| c == 0)?;
    Some(String::from_utf8_lossy(&s[..end]).to_string())
}
fn wstr(b: &[u8], o: usize) -> Option<String> {
    let mut v = Vec::new();
    let mut i = o;
    loop {
        let c = rd_u16(b, i)?;
        if c == 0 {
            break;
        }
        v.push(c);
        i += 2;
    }
    Some(String::from_utf16_lossy(&v))
}

/// Shell Link (.lnk) dosyasından yerel hedef yolunu çıkar (MS-SHLLINK LinkInfo).
pub fn lnk_target(b: &[u8]) -> Option<String> {
    if rd_u32(b, 0)? != 0x4C {
        return None;
    }
    let flags = rd_u32(b, 0x14)?;
    let mut o = 0x4C;
    if flags & 0x1 != 0 {
        o += 2 + rd_u16(b, o)? as usize;
    }
    if flags & 0x2 == 0 {
        return None;
    }
    let li = o;
    let hdr = rd_u32(b, li + 4)?;
    let liflags = rd_u32(b, li + 8)?;
    if liflags & 1 == 0 {
        return None;
    }
    let base = if hdr >= 0x24 {
        wstr(b, li + rd_u32(b, li + 28)? as usize)?
    } else {
        cstr(b, li + rd_u32(b, li + 16)? as usize)?
    };
    let suffix = if hdr >= 0x24 {
        wstr(b, li + rd_u32(b, li + 32)? as usize).unwrap_or_default()
    } else {
        cstr(b, li + rd_u32(b, li + 24)? as usize).unwrap_or_default()
    };
    if base.is_empty() {
        return None;
    }
    if suffix.is_empty() {
        Some(base)
    } else if base.ends_with('\\') {
        Some(base + &suffix)
    } else {
        Some(format!("{base}\\{suffix}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: Option<&str> = Some("C:\\Users\\kullanici");

    fn names(v: &[(String, i64)]) -> Vec<&str> {
        v.iter().map(|x| x.0.as_str()).collect()
    }

    #[test]
    fn dests_record_visits_newest_first_dedup() {
        let mut d = Vec::new();
        record_dest(&mut d, "C:\\Oyunlar", 1, HOME);
        record_dest(&mut d, "D:\\Filmler", 2, HOME);
        record_dest(&mut d, "C:\\Users\\kullanici\\Documents", 3, HOME);
        assert_eq!(names(&d), vec!["C:\\Users\\kullanici\\Documents", "D:\\Filmler", "C:\\Oyunlar"]);
        // tekrar gidilen en üste çıkar, çift kayıt olmaz (büyük/küçük harf duyarsız)
        record_dest(&mut d, "c:\\oyunlar", 4, HOME);
        assert_eq!(d.len(), 3);
        assert_eq!(d[0].0, "c:\\oyunlar");
        assert_eq!(d[0].1, 4);
    }

    #[test]
    fn dests_diversity_oyunlar() {
        let mut d = Vec::new();
        record_dest(&mut d, "D:\\Filmler", 1, HOME);
        record_dest(&mut d, "C:\\Oyunlar", 2, HOME);
        record_dest(&mut d, "C:\\Oyunlar\\OyunA", 3, HOME);
        record_dest(&mut d, "C:\\Oyunlar\\OyunA\\Game", 4, HOME);
        record_dest(&mut d, "C:\\Oyunlar\\OyunB", 5, HOME);
        record_dest(&mut d, "C:\\Oyunlar\\OyunB\\live", 6, HOME);
        record_dest(&mut d, "C:\\Oyunlar\\Baslatici", 7, HOME);
        // Oyunlar altında gezinti tek kayıt: en son girilen
        assert_eq!(names(&d), vec!["C:\\Oyunlar\\Baslatici", "D:\\Filmler"]);
    }

    #[test]
    fn dests_related_rules() {
        // üst/alt (1-2 seviye)
        assert!(dests_related("C:\\a\\b\\c", "C:\\a\\b", HOME));
        assert!(dests_related("C:\\a\\b\\c", "C:\\a", HOME));
        assert!(!dests_related("C:\\a\\b\\c\\d", "C:\\a", HOME), "3 seviye üst sayılmaz");
        assert!(dests_related("C:\\x\\y", "C:\\x\\y\\z\\w", HOME));
        assert!(!dests_related("C:\\x\\y", "C:\\x\\y\\z\\w\\v", HOME));
        // kardeş/kuzen: ortak üst kapsayıcı değilse
        assert!(dests_related("C:\\Games\\Studyo\\A", "C:\\Games\\Studyo\\B\\C", HOME));
        assert!(!dests_related("C:\\Games", "C:\\Filmler", HOME), "kök altındaki farklı klasörler ayrı");
        assert!(!dests_related("C:\\Program Files\\A", "C:\\Program Files\\B", HOME));
        assert!(!dests_related("C:\\Users\\kullanici\\Desktop", "C:\\Users\\kullanici\\Documents", HOME));
        assert!(!dests_related("C:\\Users\\kullanici\\Desktop\\proje", "C:\\Users\\kullanici\\Downloads", HOME));
        assert!(dests_related("C:\\Users\\kullanici\\Desktop\\p1", "C:\\Users\\kullanici\\Desktop\\p2", HOME));
        // kapsayıcıya gitmek alt klasör kayıtlarını silmez; tersine alt klasöre gitmek üstü düşürür
        assert!(!dests_related("C:\\Users\\kullanici", "C:\\Users\\kullanici\\Desktop", HOME));
        assert!(dests_related("C:\\Users\\kullanici\\Desktop", "C:\\Users\\kullanici", HOME));
        assert!(!dests_related("C:\\", "C:\\Games", HOME));
        // farklı sürücü
        assert!(!dests_related("C:\\a", "D:\\a", HOME));
        // linux yolları da çalışır
        assert!(dests_related("/home/u/p/a", "/home/u/p/b", Some("/home/u")));
        assert!(!dests_related("/home/u/a", "/home/u/b", Some("/home/u")));
    }

    #[test]
    fn dests_cap() {
        let mut d = Vec::new();
        for i in 0..40 {
            record_dest(&mut d, &format!("E:\\arsiv{i}"), i, HOME);
        }
        assert_eq!(d.len(), MAX_DESTS);
        assert_eq!(d[0].0, "E:\\arsiv39");
    }
    #[test]
    fn lnk_minimal() {
        // Başlık (0x4C) + LinkInfo (ANSI yol)
        let mut b = vec![0u8; 0x4C];
        b[0] = 0x4C;
        b[0x14] = 0x2; // HasLinkInfo
        let path = b"C:\\Users\\PC\\a.txt\0";
        let hdr = 0x1Cu32;
        let base_off = hdr;
        let suffix_off = base_off + path.len() as u32;
        let size = suffix_off + 1;
        let mut li = Vec::new();
        li.extend_from_slice(&size.to_le_bytes());
        li.extend_from_slice(&hdr.to_le_bytes());
        li.extend_from_slice(&1u32.to_le_bytes());
        li.extend_from_slice(&0u32.to_le_bytes()); // VolumeID
        li.extend_from_slice(&base_off.to_le_bytes());
        li.extend_from_slice(&0u32.to_le_bytes());
        li.extend_from_slice(&suffix_off.to_le_bytes());
        li.extend_from_slice(path);
        li.push(0);
        b.extend_from_slice(&li);
        assert_eq!(lnk_target(&b).as_deref(), Some("C:\\Users\\PC\\a.txt"));
        assert!(lnk_target(&[0u8; 10]).is_none());
    }
}
