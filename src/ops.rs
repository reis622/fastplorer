//! Dosya işlemleri: açma, klasörde gösterme, pano, kopyala/taşı/yeniden adlandır.

use std::io;
use std::path::{Path, PathBuf};

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Varsayılan uygulamayla aç.
pub fn open(path: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        let (verb, file) = (wide("open"), wide(path));
        let r = unsafe { ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), file.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL) };
        if (r as isize) <= 32 {
            return Err(format!("Açılamadı (kod {})", r as isize));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// Dosyanın bulunduğu klasörü Gezgin'de dosya seçili olarak aç.
pub fn reveal(path: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer.exe")
            .raw_arg(format!("/select,\"{path}\""))
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        let dir = Path::new(path).parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
        open(&dir)
    }
}

// ---------------- Pano ----------------

#[cfg(windows)]
mod clip {
    use super::wide;
    use windows_sys::Win32::System::DataExchange::*;
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::System::Memory::*;

    const CF_UNICODETEXT: u32 = 13;
    const CF_HDROP: u32 = 15;

    fn open() -> bool {
        for _ in 0..10 {
            if unsafe { OpenClipboard(std::ptr::null_mut()) } != 0 {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        false
    }

    unsafe fn put(fmt: u32, bytes: &[u8]) -> bool {
        unsafe {
            let h = GlobalAlloc(GMEM_MOVEABLE, bytes.len());
            if h.is_null() {
                return false;
            }
            let p = GlobalLock(h) as *mut u8;
            if p.is_null() {
                GlobalFree(h);
                return false;
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
            GlobalUnlock(h);
            if SetClipboardData(fmt, h as _).is_null() {
                GlobalFree(h);
                return false;
            }
            true
        }
    }

    pub fn clear() -> bool {
        if !open() {
            return false;
        }
        unsafe {
            let ok = EmptyClipboard() != 0;
            CloseClipboard();
            ok
        }
    }

    fn u16bytes(v: &[u16]) -> Vec<u8> {
        v.iter().flat_map(|c| c.to_le_bytes()).collect()
    }

    pub fn set_text(s: &str) -> bool {
        if !open() {
            return false;
        }
        let ok = unsafe { EmptyClipboard() != 0 && put(CF_UNICODETEXT, &u16bytes(&wide(s))) };
        unsafe { CloseClipboard() };
        ok
    }

    /// Dosyaları CF_HDROP olarak koy (Gezgin'de yapıştırılabilir). `cut` ise taşıma etkisi.
    pub fn set_files(paths: &[String], cut: bool) -> bool {
        if !open() {
            return false;
        }
        let mut b: Vec<u8> = Vec::new();
        b.extend_from_slice(&20u32.to_le_bytes()); // pFiles
        b.extend_from_slice(&[0u8; 8]); // pt
        b.extend_from_slice(&0u32.to_le_bytes()); // fNC
        b.extend_from_slice(&1u32.to_le_bytes()); // fWide
        for p in paths {
            b.extend(u16bytes(&wide(p)));
        }
        b.extend_from_slice(&[0, 0]);
        let effect: u32 = if cut { 2 } else { 1 };
        let fmt = unsafe { RegisterClipboardFormatW(wide("Preferred DropEffect").as_ptr()) };
        let text = paths.join("\r\n");
        let ok = unsafe {
            EmptyClipboard() != 0
                && put(CF_HDROP, &b)
                && (fmt == 0 || put(fmt, &effect.to_le_bytes()))
                && put(CF_UNICODETEXT, &u16bytes(&wide(&text)))
        };
        unsafe { CloseClipboard() };
        ok
    }

    /// Panodaki metin.
    pub fn get_text() -> Option<String> {
        if !open() {
            return None;
        }
        let mut out = None;
        unsafe {
            let h = GetClipboardData(CF_UNICODETEXT);
            if !h.is_null() {
                let p = GlobalLock(h as _) as *const u16;
                if !p.is_null() {
                    let mut n = 0usize;
                    while *p.add(n) != 0 && n < (1 << 20) {
                        n += 1;
                    }
                    out = Some(String::from_utf16_lossy(std::slice::from_raw_parts(p, n)));
                    GlobalUnlock(h as _);
                }
            }
            CloseClipboard();
        }
        out
    }

    /// Panodaki dosyalar (Gezgin'den kopyalanmış olabilir) ve kesme mi.
    pub fn get_files() -> Option<(Vec<String>, bool)> {
        use windows_sys::Win32::UI::Shell::DragQueryFileW;
        if !open() {
            return None;
        }
        let mut out = Vec::new();
        let mut cut = false;
        unsafe {
            let h = GetClipboardData(CF_HDROP);
            if !h.is_null() {
                let p = GlobalLock(h as _);
                if !p.is_null() {
                    let hd = p as _;
                    let n = DragQueryFileW(hd, u32::MAX, std::ptr::null_mut(), 0);
                    for i in 0..n {
                        let len = DragQueryFileW(hd, i, std::ptr::null_mut(), 0);
                        let mut buf = vec![0u16; len as usize + 1];
                        DragQueryFileW(hd, i, buf.as_mut_ptr(), len + 1);
                        out.push(String::from_utf16_lossy(&buf[..len as usize]));
                    }
                    GlobalUnlock(h as _);
                }
            }
            let fmt = RegisterClipboardFormatW(wide("Preferred DropEffect").as_ptr());
            if fmt != 0 {
                let h = GetClipboardData(fmt);
                if !h.is_null() {
                    let p = GlobalLock(h as _) as *const u32;
                    if !p.is_null() {
                        cut = *p & 2 != 0;
                        GlobalUnlock(h as _);
                    }
                }
            }
            CloseClipboard();
        }
        if out.is_empty() { None } else { Some((out, cut)) }
    }
}

/// OSC 52 ile terminal panosuna yaz (Windows dışı yedek).
#[cfg(not(windows))]
fn osc52(s: &str) -> bool {
    use std::io::Write;
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let b = s.as_bytes();
    let mut enc = String::new();
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            if k <= c.len() {
                enc.push(T[(n >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                enc.push('=');
            }
        }
    }
    let mut o = std::io::stdout();
    write!(o, "\x1b]52;c;{enc}\x07").is_ok() && o.flush().is_ok()
}

pub fn set_clipboard_text(s: &str) -> bool {
    #[cfg(windows)]
    {
        clip::set_text(s)
    }
    #[cfg(not(windows))]
    {
        osc52(s)
    }
}

pub fn set_clipboard_files(paths: &[String], cut: bool) -> bool {
    #[cfg(windows)]
    {
        clip::set_files(paths, cut)
    }
    #[cfg(not(windows))]
    {
        let _ = cut;
        osc52(&paths.join("\n"))
    }
}

/// Windows panosunu boşalt (yalnızca panoda bizim kopyaladığımız dosyalar varken çağrılır).
pub fn clear_clipboard() -> bool {
    #[cfg(windows)]
    {
        clip::clear()
    }
    #[cfg(not(windows))]
    {
        true
    }
}

pub fn get_clipboard_text() -> Option<String> {
    #[cfg(windows)]
    {
        clip::get_text()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

pub fn get_clipboard_files() -> Option<(Vec<String>, bool)> {
    #[cfg(windows)]
    {
        clip::get_files()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

// ---------------- Dosya işlemleri ----------------

/// Hedef klasörde çakışmayan ad: "ad (2).txt".
pub fn unique_name(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    if !p.exists() {
        return p;
    }
    let (stem, ext) = split_ext(name);
    for n in 2.. {
        let c = dir.join(format!("{stem} ({n}){ext}"));
        if !c.exists() {
            return c;
        }
    }
    unreachable!()
}

/// "rapor.final.pdf" -> ("rapor.final", ".pdf"); ".gitignore" -> (".gitignore", "").
pub fn split_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(p) if p > 0 => (&name[..p], &name[p..]),
        _ => (name, ""),
    }
}

fn copy_rec(src: &Path, dst: &Path) -> io::Result<()> {
    let md = std::fs::symlink_metadata(src)?;
    if md.is_dir() {
        if dst.starts_with(src) {
            return Err(io::Error::other("Klasör kendi içine kopyalanamaz"));
        }
        std::fs::create_dir_all(dst)?;
        for e in std::fs::read_dir(src)? {
            let e = e?;
            copy_rec(&e.path(), &dst.join(e.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(src, dst).map(|_| ())
    }
}

pub fn remove_any(p: &Path) -> io::Result<()> {
    if std::fs::symlink_metadata(p)?.is_dir() { std::fs::remove_dir_all(p) } else { std::fs::remove_file(p) }
}

/// `src`'yi `dst` yoluna kopyala ya da taşı. `overwrite` ise var olan hedef önce kaldırılır.
pub fn transfer(src: &Path, dst: &Path, cut: bool, overwrite: bool) -> io::Result<()> {
    if src == dst {
        return Err(io::Error::other("Kaynak ve hedef aynı"));
    }
    if dst.exists() {
        if !overwrite {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, "Hedefte aynı adda öğe var"));
        }
        remove_any(dst)?;
    }
    if cut {
        if std::fs::rename(src, dst).is_ok() {
            return Ok(());
        }
        // farklı sürücü: kopyala, sonra kaynağı kaldır
        copy_rec(src, dst)?;
        remove_any(src)
    } else {
        copy_rec(src, dst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_and_transfer() {
        assert_eq!(split_ext("a.tar.gz"), ("a.tar", ".gz"));
        assert_eq!(split_ext(".env"), (".env", ""));
        let d = std::env::temp_dir().join(format!("bul_ops_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("a/sub")).unwrap();
        std::fs::create_dir_all(d.join("b")).unwrap();
        std::fs::write(d.join("a/sub/x.txt"), "hi").unwrap();
        std::fs::write(d.join("a/y.txt"), "y").unwrap();
        transfer(&d.join("a"), &d.join("b/a"), false, false).unwrap();
        assert!(d.join("b/a/sub/x.txt").exists());
        assert!(transfer(&d.join("a/y.txt"), &d.join("b/a/y.txt"), true, false).is_err());
        assert_eq!(unique_name(&d.join("b/a"), "y.txt"), d.join("b/a/y (2).txt"));
        transfer(&d.join("a/y.txt"), &d.join("b/a/y.txt"), true, true).unwrap();
        assert!(!d.join("a/y.txt").exists());
        assert!(transfer(&d.join("b"), &d.join("b/a/b"), false, false).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }
}
