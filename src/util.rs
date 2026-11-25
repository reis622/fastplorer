//! Ortak yardımcılar: harf katlama, kategori, biçimlendirme.
#![allow(dead_code)]

pub const SEP: char = std::path::MAIN_SEPARATOR;

pub const CAT_OTHER: u8 = 0;
pub const CAT_DIR: u8 = 1;
pub const CAT_IMAGE: u8 = 2;
pub const CAT_DOC: u8 = 3;
pub const CAT_CODE: u8 = 4;
pub const CAT_ARCHIVE: u8 = 5;
pub const CAT_VIDEO: u8 = 6;
pub const CAT_AUDIO: u8 = 7;

pub const TABS: [&str; 8] = ["Tümü", "Klasör", "Resim", "Belge", "Kod", "Arşiv", "Video", "Ses"];

/// Tek karakteri küçük harfe katlar (Türkçe I/İ/ı -> i). Karakter sayısı korunur.
#[inline]
pub fn fold_char(c: char) -> char {
    if c.is_ascii() {
        return c.to_ascii_lowercase();
    }
    match c {
        'İ' | 'ı' => 'i',
        _ => c.to_lowercase().next().unwrap_or(c),
    }
}

pub fn fold(s: &str) -> String {
    s.chars().map(fold_char).collect()
}

pub fn ext_of(name: &str) -> String {
    match name.rfind('.') {
        Some(p) if p > 0 && p + 1 < name.len() => fold(&name[p + 1..]),
        _ => String::new(),
    }
}

const IMAGE: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp", "ico", "tif", "tiff", "svg", "heic", "avif", "raw", "psd"];
const DOC: &[&str] = &["pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp", "rtf", "txt", "md", "csv", "log", "epub", "rst", "tex"];
const CODE: &[&str] = &[
    "rs", "go", "ts", "tsx", "js", "jsx", "mjs", "cjs", "py", "c", "h", "cpp", "cc", "hpp", "cs", "java", "kt", "kts", "swift",
    "json", "toml", "yaml", "yml", "html", "htm", "css", "scss", "sass", "less", "vue", "svelte", "sh", "bash", "ps1", "psm1",
    "bat", "cmd", "sql", "lua", "rb", "php", "xml", "ini", "cfg", "conf", "gradle", "dart", "r", "pl", "scala", "zig", "nim",
    "ex", "exs", "hs", "ml", "fs", "vb", "asm", "make", "cmake", "dockerfile", "proto", "graphql", "env", "lock", "gitignore",
];
const TEXT_DOC: &[&str] = &["txt", "md", "csv", "log", "rst", "tex"];
const ARCHIVE: &[&str] = &["zip", "7z", "rar", "tar", "gz", "tgz", "bz2", "xz", "zst", "cab", "iso", "jar", "apk"];
const VIDEO: &[&str] = &["mp4", "mkv", "avi", "mov", "webm", "wmv", "flv", "m4v", "mpg", "mpeg", "3gp", "ts_video"];
const AUDIO: &[&str] = &["mp3", "wav", "flac", "ogg", "m4a", "aac", "wma", "opus", "aiff", "mid", "midi"];

pub fn category(name: &str, is_dir: bool) -> u8 {
    if is_dir {
        return CAT_DIR;
    }
    let e = ext_of(name);
    let e = e.as_str();
    if IMAGE.contains(&e) {
        CAT_IMAGE
    } else if CODE.contains(&e) {
        CAT_CODE
    } else if DOC.contains(&e) {
        CAT_DOC
    } else if ARCHIVE.contains(&e) {
        CAT_ARCHIVE
    } else if VIDEO.contains(&e) {
        CAT_VIDEO
    } else if AUDIO.contains(&e) {
        CAT_AUDIO
    } else {
        CAT_OTHER
    }
}

/// İçerik aramasına uygun düz metin dosyası mı?
pub fn is_textual(name: &str) -> bool {
    let e = ext_of(name);
    CODE.contains(&e.as_str()) || TEXT_DOC.contains(&e.as_str())
}

pub fn type_label(name: &str, is_dir: bool) -> String {
    if is_dir {
        return "Klasör".into();
    }
    let e = ext_of(name);
    let l = match e.as_str() {
        "rs" => "Rust",
        "go" => "Go",
        "ts" | "tsx" => "TypeScript",
        "js" | "jsx" | "mjs" | "cjs" => "JavaScript",
        "py" => "Python",
        "c" | "h" => "C",
        "cpp" | "cc" | "hpp" => "C++",
        "cs" => "C#",
        "java" => "Java",
        "kt" | "kts" => "Kotlin",
        "md" => "Markdown",
        "txt" => "Metin",
        "pdf" => "PDF",
        "doc" | "docx" | "odt" | "rtf" => "Word belgesi",
        "xls" | "xlsx" | "ods" | "csv" => "Tablo",
        "ppt" | "pptx" | "odp" => "Sunum",
        _ => "",
    };
    if !l.is_empty() {
        return l.into();
    }
    if e.is_empty() { "Dosya".into() } else { e.to_uppercase() }
}

/// 4,2 KB biçiminde boyut.
pub fn fmt_size(n: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if n < 1024 {
        return format!("{n} B");
    }
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u < U.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    let s = if v < 100.0 { format!("{v:.1}") } else { format!("{v:.0}") };
    format!("{} {}", s.replace('.', ","), U[u])
}

/// 1.591.409 biçiminde sayı.
pub fn fmt_count(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    out
}

const MONTHS: [&str; 12] = ["Oca", "Şub", "Mar", "Nis", "May", "Haz", "Tem", "Ağu", "Eyl", "Eki", "Kas", "Ara"];

pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// "16 Kas 09:14" (bu yıl) veya "16 Kas 2024".
pub fn fmt_date(t: i64) -> String {
    use chrono::{Datelike, Local, TimeZone, Timelike};
    if t <= 0 {
        return String::new();
    }
    let Some(d) = Local.timestamp_opt(t, 0).single() else { return String::new() };
    let now = Local::now();
    let m = MONTHS[d.month0() as usize];
    if d.year() == now.year() {
        format!("{:>2} {} {:02}:{:02}", d.day(), m, d.hour(), d.minute())
    } else {
        format!("{:>2} {} {}", d.day(), m, d.year())
    }
}

/// Son kullanım zamanı: "az önce", "5 dk önce", "Bugün 14:02", "Dün 09:10", ya da tarih.
pub fn fmt_when(t: i64) -> String {
    use chrono::{Local, TimeZone, Timelike};
    let now = now_secs();
    let diff = now - t;
    if (0..60).contains(&diff) {
        return "az önce".into();
    }
    if (60..3600).contains(&diff) {
        return format!("{} dk önce", diff / 60);
    }
    let (Some(d), n) = (Local.timestamp_opt(t, 0).single(), Local::now()) else { return fmt_date(t) };
    let dd = d.date_naive();
    let nd = n.date_naive();
    if dd == nd {
        return format!("Bugün {:02}:{:02}", d.hour(), d.minute());
    }
    if nd.pred_opt() == Some(dd) {
        return format!("Dün {:02}:{:02}", d.hour(), d.minute());
    }
    fmt_date(t)
}

pub const APP_DIR: &str = "Fastplorer";
/// Eski sürümlerin (bul) veri klasörü adı — taşıma için.
pub const LEGACY_DIR: &str = "bul";

fn data_dir_named(name: &str) -> std::path::PathBuf {
    #[cfg(windows)]
    {
        if let Some(p) = std::env::var_os("LOCALAPPDATA") {
            return std::path::PathBuf::from(p).join(name);
        }
    }
    if let Some(p) = std::env::var_os("XDG_DATA_HOME") {
        return std::path::PathBuf::from(p).join(name);
    }
    if let Some(h) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        return std::path::PathBuf::from(h).join(".local").join("share").join(name);
    }
    std::path::PathBuf::from(format!(".{name}"))
}

/// %LOCALAPPDATA%\Fastplorer
pub fn data_dir() -> std::path::PathBuf {
    data_dir_named(APP_DIR)
}

/// %LOCALAPPDATA%\bul (eski)
pub fn legacy_data_dir() -> std::path::PathBuf {
    data_dir_named(LEGACY_DIR)
}

/// Dosya adında yasak karakter var mı (Windows kuralları)?
pub fn valid_name(s: &str) -> bool {
    let t = s.trim();
    !t.is_empty() && t != "." && t != ".." && !s.chars().any(|c| "<>:\"/\\|?*".contains(c) || (c as u32) < 32)
}

/// Dosyayı atomik yaz: geçici dosya + yeniden adlandırma. Windows'ta hedef o an başka bir
/// süreç/iş parçacığı tarafından okunuyorsa (paylaşım ihlali) kısa aralıklarla yeniden dener.
/// Hata çağırana döner; çağıranlar sessizce yok sayar (başlangıç asla bloklanmaz).
pub fn atomic_write(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    // aynı anda iki yazıcı çakışmasın: süreç + iş parçacığına özgü geçici ad
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = path.with_extension(format!("tmp{}-{n}", std::process::id()));
    std::fs::write(&tmp, data)?;
    let mut last = None;
    for i in 0..8 {
        match std::fs::rename(&tmp, path) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(25 << i.min(4)));
            }
        }
    }
    let _ = std::fs::remove_file(&tmp);
    Err(last.unwrap_or_else(|| std::io::Error::other("yazılamadı")))
}

/// Dosyayı oku; paylaşım ihlalinde birkaç kez dener (Windows'ta std::fs::read okuma+yazma paylaşımıyla açar).
pub fn read_retry(path: &std::path::Path) -> Option<Vec<u8>> {
    for i in 0..4 {
        match std::fs::read(path) {
            Ok(b) => return Some(b),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(20 << i)),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn folding() {
        assert_eq!(fold("İSTANBUL Iğdır"), "istanbul iğdir");
        assert_eq!(fold("FetchUser.TS").chars().count(), "FetchUser.TS".chars().count());
    }
    #[test]
    fn cats() {
        assert_eq!(category("a.PNG", false), CAT_IMAGE);
        assert_eq!(category("a.ts", false), CAT_CODE);
        assert_eq!(category("a.zip", false), CAT_ARCHIVE);
        assert_eq!(category("x", true), CAT_DIR);
        assert!(is_textual("notes.md"));
        assert!(!is_textual("a.pdf"));
    }
    #[test]
    fn fmt() {
        assert_eq!(fmt_size(4300), "4,2 KB");
        assert_eq!(fmt_count(1591409), "1.591.409");
        assert!(valid_name("a b.txt"));
        assert!(!valid_name("a:b"));
    }
}
