//! "Not al": yazılan metni geçerli klasöre not-YYYY-MM-DD-HHMM.txt olarak kaydet (asla üzerine yazmaz).

use std::io::Write;
use std::path::{Path, PathBuf};

/// Temel ad: not-2026-11-24-1932.txt
pub fn base_name(t: &chrono::NaiveDateTime) -> String {
    format!("not-{}.txt", t.format("%Y-%m-%d-%H%M"))
}

/// k. aday ad (1 = temel ad, sonra -2, -3 …).
pub fn candidate(t: &chrono::NaiveDateTime, k: u32) -> String {
    if k <= 1 { base_name(t) } else { format!("not-{}-{k}.txt", t.format("%Y-%m-%d-%H%M")) }
}

/// Satır sonlarını platforma göre düzenle (Windows'ta CRLF; Not Defteri ile uyumlu).
pub fn normalize(text: &str) -> String {
    let lf = text.replace("\r\n", "\n").replace('\r', "\n");
    if cfg!(windows) { lf.replace('\n', "\r\n") } else { lf }
}

/// Notu kaydet: UTF-8, var olan dosyanın üzerine asla yazmaz (create_new + sonek).
pub fn save(dir: &Path, text: &str, t: &chrono::NaiveDateTime) -> std::io::Result<PathBuf> {
    let body = normalize(text);
    for k in 1..10_000 {
        let p = dir.join(candidate(t, k));
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&p) {
            Ok(mut f) => {
                f.write_all(body.as_bytes())?;
                f.flush()?;
                return Ok(p);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::other(t!("uygun dosya adı bulunamadı", "no free file name found")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn t() -> chrono::NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 24).unwrap().and_hms_opt(19, 32, 5).unwrap()
    }

    #[test]
    fn naming() {
        assert_eq!(base_name(&t()), "not-2026-11-24-1932.txt");
        assert_eq!(candidate(&t(), 1), "not-2026-11-24-1932.txt");
        assert_eq!(candidate(&t(), 2), "not-2026-11-24-1932-2.txt");
        let early = NaiveDate::from_ymd_opt(2027, 1, 5).unwrap().and_hms_opt(7, 4, 0).unwrap();
        assert_eq!(base_name(&early), "not-2027-01-05-0704.txt");
    }

    #[test]
    fn never_overwrites() {
        let d = std::env::temp_dir().join(format!("fp-note-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let a = save(&d, "ilk not\nikinci satır: çğıöşü İ", &t()).unwrap();
        assert_eq!(a.file_name().unwrap(), "not-2026-11-24-1932.txt");
        let b = save(&d, "ikinci not", &t()).unwrap();
        assert_eq!(b.file_name().unwrap(), "not-2026-11-24-1932-2.txt");
        let c = save(&d, "üçüncü", &t()).unwrap();
        assert_eq!(c.file_name().unwrap(), "not-2026-11-24-1932-3.txt");
        // ilk dosya korunur, çok satır ve UTF-8 bozulmaz
        let first = std::fs::read_to_string(&a).unwrap();
        assert_eq!(first.replace("\r\n", "\n"), "ilk not\nikinci satır: çğıöşü İ");
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "ikinci not");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn line_endings() {
        let n = normalize("a\r\nb\nc\rd");
        assert_eq!(n.replace("\r\n", "\n"), "a\nb\nc\nd");
        if cfg!(windows) {
            assert_eq!(n, "a\r\nb\r\nc\r\nd");
        } else {
            assert_eq!(n, "a\nb\nc\nd");
        }
    }
}
