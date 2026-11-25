//! Geri Dönüşüm Kutusu'nu gezilebilir bir klasör gibi göstermek için okuma ve geri yükleme.
//! İçeriği arama dizinine katılmaz (`$Recycle.Bin` normal dizinlemede atlanır).
//! Windows her silinen öğe için `X:\$Recycle.Bin\<SID>\` altında iki kayıt tutar:
//! `$Rxxxxxx.ext` (asıl veri) ve `$Ixxxxxx.ext` (özgün yol, boyut, silinme zamanı).
//! Burada hiçbir şey kalıcı olarak silinmez.

use std::path::{Path, PathBuf};

/// Listede Geri Dönüşüm Kutusu'nu temsil eden sanal öğenin yolu (gerçek bir yol değildir).
pub const VIRTUAL_PATH: &str = "::geri-donusum-kutusu";
/// Görünen ad (geçerli dilde).
pub fn title() -> &'static str {
    t!("Geri Dönüşüm Kutusu", "Recycle Bin")
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// `$R…` dosyasının yolu (asıl veri)
    pub r_path: String,
    /// `$I…` dosyasının yolu
    pub i_path: String,
    /// Özgün tam yol
    pub orig: String,
    pub size: u64,
    /// Silinme zamanı (unix sn)
    pub deleted: i64,
    pub is_dir: bool,
}

impl Entry {
    pub fn name(&self) -> String {
        self.orig.rsplit(['\\', '/']).next().unwrap_or(&self.orig).to_string()
    }
    pub fn orig_dir(&self) -> String {
        match self.orig.rfind(['\\', '/']) {
            // "C:\a.txt" → "C:\"
            Some(i) if i > 0 && self.orig[..i].ends_with(':') => self.orig[..=i].to_string(),
            Some(i) => self.orig[..i].to_string(),
            None => String::new(),
        }
    }
}

/// Aramada "çöp", "geri dönüşüm", "recycle", "bin" … yazılınca sanal öğe gösterilsin mi?
/// Her sözcük, takma adlardan birinin bir sözcüğünün başı olmalı (Türkçe harfler katlanır).
pub fn query_matches(q: &str) -> bool {
    const ALIASES: [&str; 4] = ["geri dönüşüm kutusu", "çöp kutusu", "recycle bin", "trash"];
    // Türkçe harfler düzleştirilir: "cop", "geri donusum" de bulur
    let plain = |x: &str| -> String {
        crate::util::fold(x).chars().map(|c| match c { 'ç' => 'c', 'ğ' => 'g', 'ö' => 'o', 'ş' => 's', 'ü' => 'u', c => c }).collect()
    };
    let terms: Vec<String> = plain(q).split_whitespace().map(str::to_string).collect();
    if terms.is_empty() || terms.iter().map(|t| t.chars().count()).sum::<usize>() < 3 {
        return false;
    }
    ALIASES.iter().any(|a| {
        let words: Vec<String> = plain(a).split_whitespace().map(str::to_string).collect();
        terms.iter().all(|t| words.iter().any(|w| w.starts_with(t.as_str())))
    })
}

/// `$I` başlığını çöz: (boyut, silinme zamanı, özgün yol).
/// Sürüm 1 (Vista–8.1): 24 bayt başlık + 260 UTF-16 karakter sabit yol.
/// Sürüm 2 (Windows 10/11): 24 bayt başlık + 4 bayt uzunluk + uzunluk×UTF-16.
pub fn parse_i(b: &[u8]) -> Option<(u64, i64, String)> {
    if b.len() < 28 {
        return None;
    }
    let u64_at = |o: usize| u64::from_le_bytes(b[o..o + 8].try_into().unwrap());
    let ver = u64_at(0);
    let size = u64_at(8);
    let ft = u64_at(16);
    let units: Vec<u16> = match ver {
        1 => b.get(24..24 + 520)?.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect(),
        2 => {
            let n = u32::from_le_bytes(b[24..28].try_into().unwrap()) as usize;
            if n == 0 || n > 32768 {
                return None;
            }
            b.get(28..28 + n * 2)?.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect()
        }
        _ => return None,
    };
    let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
    let path = String::from_utf16(&units[..end]).ok()?;
    if path.is_empty() {
        return None;
    }
    // FILETIME (1601'den beri 100 ns) → unix sn
    let deleted = (ft / 10_000_000) as i64 - 11_644_473_600;
    Some((size, deleted, path))
}

/// Taranacak kutu klasörleri: yalnızca bu kullanıcının klasörü (`X:\$Recycle.Bin\<SID>`),
/// yalnızca yerel sabit sürücülerde. SID alınamazsa hiçbir şey okunmaz (başkasınınki asla).
/// Denemelerde `BUL_RECYCLE_DIRS` (`;` ile ayrılmış) ile sahte klasörler verilebilir.
pub fn bin_dirs() -> Vec<PathBuf> {
    if let Some(v) = std::env::var_os("BUL_RECYCLE_DIRS") {
        return v.to_string_lossy().split(';').filter(|s| !s.is_empty()).map(PathBuf::from).collect();
    }
    #[cfg(windows)]
    {
        let Some(sid) = crate::platform::current_user_sid() else { return Vec::new() };
        let mut out = Vec::new();
        for d in b'A'..=b'Z' {
            let d = d as char;
            if !crate::platform::is_fixed_drive(d) {
                continue;
            }
            let p = PathBuf::from(format!("{d}:\\$Recycle.Bin\\{sid}"));
            if p.is_dir() {
                out.push(p);
            }
        }
        out
    }
    #[cfg(not(windows))]
    Vec::new()
}

pub fn scan_dir(dir: &Path) -> Vec<Entry> {
    let mut v = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return v };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let Some(rest) = name.strip_prefix("$I") else { continue };
        let r = dir.join(format!("$R{rest}"));
        let Ok(md) = std::fs::symlink_metadata(&r) else { continue }; // veri yoksa (yarım silme) atla
        let Ok(b) = std::fs::read(e.path()) else { continue };
        if let Some((size, deleted, orig)) = parse_i(&b) {
            v.push(Entry { r_path: r.to_string_lossy().into(), i_path: e.path().to_string_lossy().into(), orig, size, deleted, is_dir: md.is_dir() });
        }
    }
    v
}

pub fn scan() -> Vec<Entry> {
    let mut v: Vec<Entry> = bin_dirs().iter().flat_map(|d| scan_dir(d)).collect();
    v.sort_by(|a, b| b.deleted.cmp(&a.deleted).then_with(|| a.orig.cmp(&b.orig)));
    v
}

/// Özgün yer doluysa "ad (2).ext", "ad (3).ext" … (klasörlerde uzantı ayrılmaz).
pub fn free_target(orig: &Path, is_dir: bool) -> Option<PathBuf> {
    let taken = |p: &Path| std::fs::symlink_metadata(p).is_ok();
    if !taken(orig) {
        return Some(orig.to_path_buf());
    }
    let parent = orig.parent()?;
    let name = orig.file_name()?.to_string_lossy().to_string();
    let (stem, ext) = match name.rfind('.') {
        Some(i) if !is_dir && i > 0 => (name[..i].to_string(), name[i..].to_string()),
        _ => (name.clone(), String::new()),
    };
    (2..10_000).map(|n| parent.join(format!("{stem} ({n}){ext}"))).find(|p| !taken(p))
}

/// Geri yükle: `$R`yi özgün yerine taşı (ad doluysa "ad (2).ext"; asla üzerine yazmaz).
/// Özgün klasör yoksa yeniden oluşturulur. `$I` kaydı yalnızca taşıma başarılı olunca kaldırılır.
pub fn restore(e: &Entry) -> Result<String, String> {
    let r = Path::new(&e.r_path);
    if std::fs::symlink_metadata(r).is_err() {
        return Err(t!("öğe artık Geri Dönüşüm Kutusu'nda değil", "the item is no longer in the Recycle Bin").into());
    }
    let orig = PathBuf::from(&e.orig);
    let parent = orig.parent().filter(|p| !p.as_os_str().is_empty()).ok_or(t!("özgün klasör bilinmiyor", "original folder unknown"))?;
    std::fs::create_dir_all(parent).map_err(|x| tf!("özgün klasör oluşturulamadı: {x}", "could not create the original folder: {x}"))?;
    let target = free_target(&orig, e.is_dir).ok_or(t!("uygun bir ad bulunamadı", "no free name found"))?;
    crate::platform::move_no_replace(r, &target)?;
    let _ = std::fs::remove_file(&e.i_path);
    Ok(target.to_string_lossy().into())
}

/// Denemeler için: Windows 10/11 biçiminde `$I` kaydı (unix zamanı → FILETIME).
#[cfg(test)]
pub fn encode_i_v2(path: &str, size: u64, deleted_unix: i64) -> Vec<u8> {
    let ft = ((deleted_unix + 11_644_473_600).max(0) as u64) * 10_000_000;
    tests::v2(path, size, ft)
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn v2(path: &str, size: u64, ft: u64) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend(2u64.to_le_bytes());
        b.extend(size.to_le_bytes());
        b.extend(ft.to_le_bytes());
        let u: Vec<u16> = path.encode_utf16().chain([0]).collect();
        b.extend((u.len() as u32).to_le_bytes());
        for x in u {
            b.extend(x.to_le_bytes());
        }
        b
    }
    #[test]
    fn parse_v2_and_v1() {
        let ft = (1_764_000_000u64 + 11_644_473_600) * 10_000_000;
        let (s, d, p) = parse_i(&v2("C:\\Users\\kullanici\\Belgeler\\rapor.pdf", 1234, ft)).unwrap();
        assert_eq!((s, d, p.as_str()), (1234, 1_764_000_000, "C:\\Users\\kullanici\\Belgeler\\rapor.pdf"));
        let mut b1 = vec![0u8; 544];
        b1[0] = 1;
        b1[8] = 7;
        for (k, x) in "D:\\a.txt".encode_utf16().enumerate() {
            b1[24 + 2 * k..26 + 2 * k].copy_from_slice(&x.to_le_bytes());
        }
        assert_eq!(parse_i(&b1).unwrap().2, "D:\\a.txt");
        assert!(parse_i(&[0u8; 10]).is_none());
        assert!(parse_i(&v2("", 0, ft)).is_none());
    }
    #[test]
    fn restore_never_overwrites() {
        let d = std::env::temp_dir().join(format!("bul-rc-{}", std::process::id()));
        let bin = d.join("bin");
        let orig_dir = d.join("Belgeler");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&orig_dir).unwrap();
        std::fs::write(orig_dir.join("a.txt"), "mevcut").unwrap();
        std::fs::write(bin.join("$RX1.txt"), "silinmiş").unwrap();
        let orig = orig_dir.join("a.txt").to_string_lossy().to_string();
        std::fs::write(bin.join("$IX1.txt"), v2(&orig, 8, 0)).unwrap();
        let e = scan_dir(&bin).pop().unwrap();
        let t = restore(&e).unwrap();
        assert!(t.ends_with("a (2).txt"));
        assert_eq!(std::fs::read_to_string(orig_dir.join("a.txt")).unwrap(), "mevcut");
        assert_eq!(std::fs::read_to_string(&t).unwrap(), "silinmiş");
        let _ = std::fs::remove_dir_all(&d);
    }
    #[test]
    fn restore_recreates_folder_and_keeps_dirs_whole() {
        let d = std::env::temp_dir().join(format!("bul-rc2-{}", std::process::id()));
        let bin = d.join("bin");
        std::fs::create_dir_all(bin.join("$RD1")).unwrap();
        std::fs::write(bin.join("$RD1").join("ic.txt"), "x").unwrap();
        let orig = d.join("Yok").join("Alt").join("proje.v2");
        std::fs::write(bin.join("$ID1"), v2(&orig.to_string_lossy(), 1, 0)).unwrap();
        let e = scan_dir(&bin).pop().unwrap();
        assert!(e.is_dir);
        let t = restore(&e).unwrap();
        assert_eq!(Path::new(&t), orig.as_path(), "silinmiş klasör yeniden oluşturulan özgün yere döner");
        assert!(orig.join("ic.txt").is_file());
        assert!(!bin.join("$ID1").exists(), "$I yalnızca başarıdan sonra kaldırılır");
        // aynı adlı klasör varken: "proje.v2 (2)" (klasörde uzantı ayrılmaz)
        std::fs::create_dir_all(bin.join("$RD2")).unwrap();
        std::fs::write(bin.join("$ID2"), v2(&orig.to_string_lossy(), 1, 0)).unwrap();
        let e = scan_dir(&bin).pop().unwrap();
        let t = restore(&e).unwrap();
        assert!(t.ends_with("proje.v2 (2)"), "{t}");
        let _ = std::fs::remove_dir_all(&d);
    }
    #[test]
    fn failed_restore_keeps_record() {
        let d = std::env::temp_dir().join(format!("bul-rc3-{}", std::process::id()));
        let bin = d.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("$IZ9.txt"), v2(&d.join("a.txt").to_string_lossy(), 1, 0)).unwrap();
        std::fs::write(bin.join("$RZ9.txt"), "x").unwrap();
        let e = scan_dir(&bin).pop().unwrap();
        std::fs::remove_file(&e.r_path).unwrap();
        assert!(restore(&e).is_err());
        assert!(bin.join("$IZ9.txt").exists(), "başarısız geri yüklemede kayıt silinmez");
        let _ = std::fs::remove_dir_all(&d);
    }
    #[test]
    fn scan_newest_first_and_skips_orphans() {
        let d = std::env::temp_dir().join(format!("bul-rc4-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        for (k, t) in [("A", 100), ("B", 300), ("C", 200)] {
            std::fs::write(d.join(format!("$I{k}.txt")), encode_i_v2(&format!("C:\\x\\{k}.txt"), 1, t)).unwrap();
            std::fs::write(d.join(format!("$R{k}.txt")), "x").unwrap();
        }
        // $R'si olmayan yarım kayıt atlanır
        std::fs::write(d.join("$IQ.txt"), encode_i_v2("C:\\x\\Q.txt", 1, 999)).unwrap();
        let mut v = scan_dir(&d);
        v.sort_by_key(|e| std::cmp::Reverse(e.deleted));
        let names: Vec<String> = v.iter().map(|e| e.name()).collect();
        assert_eq!(names, ["B.txt", "C.txt", "A.txt"]);
        assert_eq!(v[0].orig_dir(), "C:\\x");
        let _ = std::fs::remove_dir_all(&d);
    }
    #[test]
    fn names_and_query_aliases() {
        let e = Entry { r_path: String::new(), i_path: String::new(), orig: "C:\\a.txt".into(), size: 0, deleted: 0, is_dir: false };
        assert_eq!((e.name().as_str(), e.orig_dir().as_str()), ("a.txt", "C:\\"));
        for q in ["çöp", "cop kut", "geri dönüşüm", "geri donusum", "Recycle", "bin", "recycle bin", "GERİ"] {
            assert!(query_matches(q), "{q}");
        }
        for q in ["", "ç", "rapor", "bin2", "geri rapor"] {
            assert!(!query_matches(q), "{q}");
        }
    }
}
