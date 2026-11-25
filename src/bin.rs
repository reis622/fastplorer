//! "Bin" klasörü: dosyalara terminalden adıyla ulaşmak için .cmd başlatıcıları.

use std::path::PathBuf;

pub const MARK: &str = "rem bul-hedef: ";

#[derive(Clone, Debug)]
pub struct BinEntry {
    pub alias: String,
    pub target: String,
}

/// Bin klasörü. Eski (bul) bin klasörü PATH'teyse taşınmaz; yeri `bin-location.txt` içinde tutulur
/// ki terminaldeki takma adlar PATH'e dokunmadan çalışmaya devam etsin.
pub fn bin_dir() -> PathBuf {
    let d = crate::util::data_dir();
    if let Ok(s) = std::fs::read_to_string(d.join("bin-location.txt")) {
        let p = PathBuf::from(s.trim());
        if p.is_dir() {
            return p;
        }
    }
    d.join("bin")
}

/// Bir klasör kullanıcı PATH'inde mi?
pub fn dir_on_path(d: &std::path::Path) -> bool {
    let d = d.to_string_lossy().to_string();
    #[cfg(windows)]
    {
        reg::read_user_path().is_some_and(|(v, _)| path_has(&d, &v))
    }
    #[cfg(not(windows))]
    {
        std::env::var("PATH").is_ok_and(|v| v.split(':').any(|p| p == d))
    }
}

/// Eski "bul" veri klasörünü (dizin önbelleği, durum, ayarlar, bin) Fastplorer'a taşı.
/// Yalnızca yeni klasör henüz yokken bir kez çalışır; hatalar sessizce yok sayılır.
/// Eski bin klasörü PATH'teyse yerinde bırakılır (PATH'e dokunulmaz, takma adlar çalışmaya devam eder).
pub fn migrate_legacy() {
    let old = crate::util::legacy_data_dir();
    let new = crate::util::data_dir();
    if new.exists() || !old.is_dir() {
        return;
    }
    if std::fs::create_dir_all(&new).is_err() {
        return;
    }
    let old_bin = old.join("bin");
    let keep_bin = old_bin.is_dir() && dir_on_path(&old_bin);
    if let Ok(rd) = std::fs::read_dir(&old) {
        for e in rd.flatten() {
            let name = e.file_name();
            if keep_bin && name == "bin" {
                continue;
            }
            let from = e.path();
            let to = new.join(&name);
            if std::fs::rename(&from, &to).is_err() {
                // aynı birimde değilse / kilitliyse: kopyala (eskisi kalır)
                let _ = copy_tree(&from, &to);
            }
        }
    }
    if keep_bin {
        let _ = crate::util::atomic_write(&new.join("bin-location.txt"), old_bin.to_string_lossy().as_bytes());
    }
}

fn copy_tree(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    if from.is_dir() {
        std::fs::create_dir_all(to)?;
        for e in std::fs::read_dir(from)?.flatten() {
            copy_tree(&e.path(), &to.join(e.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(from, to).map(|_| ())
    }
}

/// Takma adı dosya adına uygun hale getir.
pub fn sanitize_alias(s: &str) -> String {
    let t: String = s
        .trim()
        .chars()
        .map(|c| if c.is_whitespace() { '-' } else { c })
        .filter(|c| !"<>:\"/\\|?*%^&!()".contains(*c) && (*c as u32) >= 32)
        .collect();
    t.trim_matches('.').to_string()
}

pub fn default_alias(target: &str) -> String {
    let name = target.trim_end_matches(['\\', '/']).rsplit(['\\', '/']).next().unwrap_or(target);
    let (stem, _) = crate::ops::split_ext(name);
    sanitize_alias(&crate::util::fold(stem))
}

/// .cmd başlatıcı içeriği. Çalıştırılabilirler doğrudan (argümanlarla), diğerleri `start` ile açılır.
pub fn launcher(target: &str) -> String {
    let esc = target.replace('%', "%%");
    let ext = crate::util::ext_of(target);
    let run = if matches!(ext.as_str(), "exe" | "com" | "bat" | "cmd") {
        format!("\"{esc}\" %*")
    } else {
        format!("start \"\" \"{esc}\" %*")
    };
    let mut s = String::from("@echo off\r\n");
    s.push_str(MARK);
    s.push_str(target);
    s.push_str("\r\n");
    if target.is_ascii() {
        s.push_str(&run);
        s.push_str("\r\n");
    } else {
        // ASCII dışı yol: geçici olarak UTF-8 kod sayfasına geç, sonra geri dön.
        s.push_str("setlocal\r\n");
        s.push_str("for /f \"tokens=2 delims=:\" %%a in ('chcp') do set \"_bulcp=%%a\"\r\n");
        s.push_str("chcp 65001 >nul\r\n");
        s.push_str(&run);
        s.push_str("\r\n");
        s.push_str("chcp %_bulcp: =% >nul\r\n");
    }
    s
}

pub fn parse_target(content: &str) -> Option<String> {
    content.lines().find_map(|l| l.strip_prefix(MARK)).map(|s| s.trim_end().to_string())
}

fn file_of(alias: &str) -> PathBuf {
    bin_dir().join(format!("{alias}.cmd"))
}

pub fn exists(alias: &str) -> bool {
    file_of(alias).exists()
}

pub fn add(alias: &str, target: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(bin_dir())?;
    let f = file_of(alias);
    std::fs::write(&f, launcher(target))?;
    Ok(f)
}

/// Bin girdileri: en son eklenen/değiştirilen en üstte (eşitlikte ada göre).
pub fn list() -> Vec<BinEntry> {
    let mut v: Vec<(std::time::SystemTime, BinEntry)> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(bin_dir()) {
        for e in rd.flatten() {
            let p = e.path();
            if !p.extension().is_some_and(|x| x.eq_ignore_ascii_case("cmd")) {
                continue;
            }
            let Ok(c) = std::fs::read_to_string(&p) else { continue };
            if let Some(t) = parse_target(&c) {
                let alias = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                let tm = e.metadata().ok().and_then(|m| m.modified().ok()).unwrap_or(std::time::UNIX_EPOCH);
                v.push((tm, BinEntry { alias, target: t }));
            }
        }
    }
    v.sort_by_cached_key(|(tm, a)| (std::cmp::Reverse(*tm), a.alias.to_lowercase()));
    v.into_iter().map(|(_, a)| a).collect()
}

/// Yalnızca başlatıcıyı siler, hedef dosyaya dokunmaz.
pub fn remove(alias: &str) -> std::io::Result<()> {
    let f = file_of(alias);
    let c = std::fs::read_to_string(&f)?;
    if parse_target(&c).is_none() {
        return Err(std::io::Error::other(t!("Bu dosya Fastplorer tarafından oluşturulmamış", "This file was not created by Fastplorer")));
    }
    std::fs::remove_file(f)
}

pub fn rename(old: &str, new: &str) -> std::io::Result<()> {
    if exists(new) && !old.eq_ignore_ascii_case(new) {
        return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, t!("Bu ad zaten kullanılıyor", "This name is already in use")));
    }
    std::fs::rename(file_of(old), file_of(new))
}

// ---------------- PATH (HKCU\Environment) ----------------

#[cfg_attr(not(windows), allow(dead_code))]
pub fn path_has(dir: &str, path_value: &str) -> bool {
    let norm = |s: &str| crate::util::fold(s.trim().trim_end_matches(['\\', '/']));
    let d = norm(dir);
    path_value.split(';').any(|p| norm(p) == d)
}

#[cfg(windows)]
mod reg {
    use windows_sys::Win32::Foundation::*;
    use windows_sys::Win32::System::Registry::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    pub fn read_user_path() -> Option<(String, u32)> {
        unsafe {
            let mut key: HKEY = std::ptr::null_mut();
            if RegOpenKeyExW(HKEY_CURRENT_USER, w("Environment").as_ptr(), 0, KEY_READ, &mut key) != ERROR_SUCCESS {
                return None;
            }
            let name = w("Path");
            let mut ty = 0u32;
            let mut len = 0u32;
            let r = RegQueryValueExW(key, name.as_ptr(), std::ptr::null(), &mut ty, std::ptr::null_mut(), &mut len);
            if r != ERROR_SUCCESS {
                RegCloseKey(key);
                return Some((String::new(), REG_EXPAND_SZ));
            }
            let mut buf = vec![0u16; (len as usize).div_ceil(2) + 1];
            let r = RegQueryValueExW(key, name.as_ptr(), std::ptr::null(), &mut ty, buf.as_mut_ptr() as *mut u8, &mut len);
            RegCloseKey(key);
            if r != ERROR_SUCCESS {
                return None;
            }
            let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            Some((String::from_utf16_lossy(&buf[..n]), ty))
        }
    }

    pub fn write_user_path(v: &str, ty: u32) -> bool {
        unsafe {
            let mut key: HKEY = std::ptr::null_mut();
            if RegOpenKeyExW(HKEY_CURRENT_USER, w("Environment").as_ptr(), 0, KEY_SET_VALUE, &mut key) != ERROR_SUCCESS {
                return false;
            }
            let data = w(v);
            let ty = if ty == REG_SZ { REG_SZ } else { REG_EXPAND_SZ };
            let r = RegSetValueExW(key, w("Path").as_ptr(), 0, ty, data.as_ptr() as *const u8, (data.len() * 2) as u32);
            RegCloseKey(key);
            if r != ERROR_SUCCESS {
                return false;
            }
            let env = w("Environment");
            let mut res = 0usize;
            SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, 0, env.as_ptr() as isize, SMTO_ABORTIFHUNG, 3000, &mut res);
            true
        }
    }
}

/// Bin klasörü kullanıcı PATH'inde mi?
pub fn on_path() -> bool {
    dir_on_path(&bin_dir())
}

/// Bin klasörünü kullanıcı PATH'ine ekle (yalnızca onaydan sonra çağrılır).
pub fn add_to_path() -> Result<(), String> {
    let d = bin_dir().to_string_lossy().to_string();
    #[cfg(windows)]
    {
        let (cur, ty) = reg::read_user_path().ok_or(t!("PATH okunamadı", "Could not read PATH"))?;
        if path_has(&d, &cur) {
            return Ok(());
        }
        let nv = if cur.trim().is_empty() {
            d
        } else if cur.ends_with(';') {
            format!("{cur}{d}")
        } else {
            format!("{cur};{d}")
        };
        if reg::write_user_path(&nv, ty) { Ok(()) } else { Err(t!("PATH yazılamadı", "Could not write PATH").into()) }
    }
    #[cfg(not(windows))]
    {
        Err(tf!("Bu sistemde PATH'e elle ekleyin: {d}", "On this system add it to PATH manually: {d}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn migrate_moves_data() {
        let base = std::env::temp_dir().join(format!("fp-mig-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("bul/index")).unwrap();
        std::fs::create_dir_all(base.join("bul/bin")).unwrap();
        std::fs::write(base.join("bul/state.json"), "{}").unwrap();
        std::fs::write(base.join("bul/index/index.bin"), "x").unwrap();
        std::fs::write(base.join("bul/bin/a.cmd"), "rem").unwrap();
        // SAFETY: test tek iş parçacığında ortam değişkenini ayarlar
        unsafe {
            std::env::set_var("XDG_DATA_HOME", &base);
            std::env::set_var("LOCALAPPDATA", &base);
        }
        migrate_legacy();
        assert!(base.join("Fastplorer/state.json").exists());
        assert!(base.join("Fastplorer/index/index.bin").exists());
        assert!(base.join("Fastplorer/bin/a.cmd").exists(), "PATH'te değilse bin de taşınır");
        assert_eq!(bin_dir(), base.join("Fastplorer/bin"));
        let _ = std::fs::remove_dir_all(&base);
    }
    #[test]
    fn launcher_gen() {
        let l = launcher(r"C:\Users\PC\Belgeler\rapor 100%.pdf");
        assert!(l.contains("start \"\" \"C:\\Users\\PC\\Belgeler\\rapor 100%%.pdf\" %*"));
        assert_eq!(parse_target(&l).as_deref(), Some(r"C:\Users\PC\Belgeler\rapor 100%.pdf"));
        let e = launcher(r"C:\tools\rg.exe");
        assert!(e.contains("\"C:\\tools\\rg.exe\" %*") && !e.contains("start"));
        let u = launcher(r"C:\Users\PC\Masaüstü\not.txt");
        assert!(u.contains("chcp 65001"));
        assert!(u.ends_with("\r\n"));
    }
    #[test]
    fn alias_and_path() {
        assert_eq!(sanitize_alias(" my file:1 "), "my-file1");
        assert_eq!(default_alias(r"C:\x\Rapor Final.pdf"), "rapor-final");
        assert!(path_has(r"C:\Users\PC\AppData\Local\bul\bin", r"C:\a;c:\users\pc\appdata\local\bul\bin\;D:\b"));
        assert!(!path_has(r"C:\bin", r"C:\bin2;C:\x"));
    }
}
