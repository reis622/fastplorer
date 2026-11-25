//! Dosya dizini: sıkışık bellek düzeni, paralel tarama, disk önbelleği.

use crate::util::{self, SEP};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, RwLock};

pub const NONE: u32 = u32::MAX;
pub const F_DIR: u8 = 1;
pub const F_DEL: u8 = 2;
pub const F_NOISY: u8 = 4;
pub const F_TEXT: u8 = 8;
pub const F_LINK: u8 = 16;

#[derive(Clone, Copy, Debug)]
pub struct Entry {
    pub name_off: u32,
    pub lower_off: u32,
    pub name_len: u16,
    pub lower_len: u16,
    pub parent: u32,
    pub size: u64,
    pub mtime: i64,
    pub cat: u8,
    pub flags: u8,
}

impl Entry {
    #[inline]
    pub fn is_dir(&self) -> bool {
        self.flags & F_DIR != 0
    }
    #[inline]
    pub fn alive(&self) -> bool {
        self.flags & F_DEL == 0
    }
}

#[derive(Default)]
pub struct Index {
    pub names: String,
    pub lower: String,
    pub entries: Vec<Entry>,
}

/// Gürültülü (son kullanılanlar ve içerik aramasında atlanacak) klasör adları.
fn noisy_name(name: &str) -> bool {
    name.starts_with('.')
        || name.starts_with('$')
        || matches!(
            name,
            "node_modules" | "AppData" | "ProgramData" | "Program Files" | "Program Files (x86)" | "Temp" | "tmp"
                | "__pycache__" | "target" | "Windows" | "cache" | "Cache"
        )
}

impl Index {
    #[inline]
    pub fn name(&self, i: u32) -> &str {
        let e = &self.entries[i as usize];
        &self.names[e.name_off as usize..e.name_off as usize + e.name_len as usize]
    }
    #[inline]
    pub fn lower(&self, i: u32) -> &str {
        let e = &self.entries[i as usize];
        &self.lower[e.lower_off as usize..e.lower_off as usize + e.lower_len as usize]
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn path(&self, i: u32) -> String {
        let mut chain = Vec::with_capacity(16);
        let mut c = i;
        while c != NONE && chain.len() < 512 {
            chain.push(c);
            c = self.entries[c as usize].parent;
        }
        let mut s = String::with_capacity(128);
        for &k in chain.iter().rev() {
            if !s.is_empty() && !s.ends_with(SEP) {
                s.push(SEP);
            }
            s.push_str(self.name(k));
        }
        s
    }

    pub fn push(&mut self, name: &str, parent: u32, size: u64, mtime: i64, is_dir: bool, extra: u8) -> u32 {
        let lower = util::fold(name);
        let name_off = self.names.len() as u32;
        let lower_off = self.lower.len() as u32;
        self.names.push_str(name);
        self.lower.push_str(&lower);
        let mut flags = extra;
        if is_dir {
            flags |= F_DIR;
        } else if util::is_textual(name) {
            flags |= F_TEXT;
        }
        self.entries.push(Entry {
            name_off,
            lower_off,
            name_len: name.len().min(u16::MAX as usize) as u16,
            lower_len: lower.len().min(u16::MAX as usize) as u16,
            parent,
            size,
            mtime,
            cat: util::category(name, is_dir),
            flags,
        });
        (self.entries.len() - 1) as u32
    }

    /// Yolu dizinde bul (doğrusal tarama; yalnızca dosya işlemlerinden sonra kullanılır).
    pub fn find(&self, path: &str) -> Option<u32> {
        let target = util::fold(path.trim_end_matches(SEP));
        // Kök adayları
        let mut cur = NONE;
        let mut rest = String::new();
        for (i, e) in self.entries.iter().enumerate() {
            if e.parent != NONE {
                continue;
            }
            let r = util::fold(self.name(i as u32).trim_end_matches(SEP));
            if target == r {
                return Some(i as u32);
            }
            let rr = format!("{r}{SEP}");
            if target.starts_with(&rr) || (r.is_empty() && target.starts_with(SEP)) {
                cur = i as u32;
                rest = target[rr.len().min(target.len())..].to_string();
                break;
            }
        }
        if cur == NONE {
            return None;
        }
        for comp in rest.split(SEP).filter(|c| !c.is_empty()) {
            let mut found = NONE;
            for (i, e) in self.entries.iter().enumerate() {
                if e.parent == cur && e.alive() && self.lower(i as u32) == comp {
                    found = i as u32;
                }
            }
            if found == NONE {
                return None;
            }
            cur = found;
        }
        Some(cur)
    }

    pub fn rename(&mut self, i: u32, new_name: &str) {
        let lower = util::fold(new_name);
        let (no, lo) = (self.names.len() as u32, self.lower.len() as u32);
        self.names.push_str(new_name);
        self.lower.push_str(&lower);
        let is_dir = self.entries[i as usize].is_dir();
        let e = &mut self.entries[i as usize];
        e.name_off = no;
        e.lower_off = lo;
        e.name_len = new_name.len() as u16;
        e.lower_len = lower.len() as u16;
        e.cat = util::category(new_name, is_dir);
    }

    /// Diskteki bir yolu dizine ekle (ebeveyni dizindeyse).
    pub fn add_path(&mut self, p: &Path) {
        let Some(parent) = p.parent() else { return };
        let Some(pi) = self.find(&parent.to_string_lossy()) else { return };
        let Ok(md) = std::fs::symlink_metadata(p) else { return };
        let name = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let noisy = self.entries[pi as usize].flags & F_NOISY;
        if let Some(old) = self.find(&p.to_string_lossy()) {
            self.entries[old as usize].flags |= F_DEL;
        }
        self.push(&name, pi, if md.is_dir() { 0 } else { md.len() }, mtime_of(&md), md.is_dir(), noisy);
    }

    pub fn remove_path(&mut self, p: &str) {
        if let Some(i) = self.find(p) {
            self.entries[i as usize].flags |= F_DEL;
        }
    }

    // ---- Önbellek ----
    const MAGIC: &'static [u8; 8] = b"BULIDX01";

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let mut b: Vec<u8> = Vec::with_capacity(self.names.len() * 2 + self.entries.len() * 34 + 64);
        b.extend_from_slice(Self::MAGIC);
        b.extend_from_slice(&(self.names.len() as u64).to_le_bytes());
        b.extend_from_slice(self.names.as_bytes());
        b.extend_from_slice(&(self.lower.len() as u64).to_le_bytes());
        b.extend_from_slice(self.lower.as_bytes());
        b.extend_from_slice(&(self.entries.len() as u64).to_le_bytes());
        for e in &self.entries {
            b.extend_from_slice(&e.name_off.to_le_bytes());
            b.extend_from_slice(&e.lower_off.to_le_bytes());
            b.extend_from_slice(&e.name_len.to_le_bytes());
            b.extend_from_slice(&e.lower_len.to_le_bytes());
            b.extend_from_slice(&e.parent.to_le_bytes());
            b.extend_from_slice(&e.size.to_le_bytes());
            b.extend_from_slice(&e.mtime.to_le_bytes());
            b.push(e.cat);
            b.push(e.flags);
        }
        crate::util::atomic_write(path, &b)
    }

    pub fn load(path: &Path) -> Option<Index> {
        let b = crate::util::read_retry(path)?;
        let mut p = 0usize;
        let take = |p: &mut usize, n: usize| -> Option<&[u8]> {
            let s = b.get(*p..*p + n)?;
            *p += n;
            Some(s)
        };
        if take(&mut p, 8)? != Self::MAGIC {
            return None;
        }
        let u64v = |p: &mut usize| -> Option<u64> { Some(u64::from_le_bytes(b.get(*p..*p + 8)?.try_into().ok()?)).inspect(|_| *p += 8) };
        let nl = u64v(&mut p)? as usize;
        let names = String::from_utf8(take(&mut p, nl)?.to_vec()).ok()?;
        let ll = u64v(&mut p)? as usize;
        let lower = String::from_utf8(take(&mut p, ll)?.to_vec()).ok()?;
        let n = u64v(&mut p)? as usize;
        const REC: usize = 4 + 4 + 2 + 2 + 4 + 8 + 8 + 1 + 1;
        let data = take(&mut p, n.checked_mul(REC)?)?;
        let mut entries = Vec::with_capacity(n);
        for r in data.as_chunks::<REC>().0 {
            let e = Entry {
                name_off: u32::from_le_bytes(r[0..4].try_into().ok()?),
                lower_off: u32::from_le_bytes(r[4..8].try_into().ok()?),
                name_len: u16::from_le_bytes(r[8..10].try_into().ok()?),
                lower_len: u16::from_le_bytes(r[10..12].try_into().ok()?),
                parent: u32::from_le_bytes(r[12..16].try_into().ok()?),
                size: u64::from_le_bytes(r[16..24].try_into().ok()?),
                mtime: i64::from_le_bytes(r[24..32].try_into().ok()?),
                cat: r[32],
                flags: r[33],
            };
            if e.name_off as usize + e.name_len as usize > names.len()
                || e.lower_off as usize + e.lower_len as usize > lower.len()
                || !names.is_char_boundary(e.name_off as usize)
                || !lower.is_char_boundary(e.lower_off as usize)
            {
                return None;
            }
            entries.push(e);
        }
        Some(Index { names, lower, entries })
    }
}

pub fn mtime_of(md: &std::fs::Metadata) -> i64 {
    md.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Taranacak kökler: Windows'ta tüm sabit sürücüler. BUL_ROOTS (';' ayrılmış) ile değiştirilebilir.
pub fn default_roots() -> Vec<String> {
    if let Ok(r) = std::env::var("BUL_ROOTS") {
        let v: Vec<String> = r.split(';').filter(|s| !s.is_empty()).map(|s| s.to_string()).collect();
        if !v.is_empty() {
            return v;
        }
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};
        let mut v = Vec::new();
        let mask = unsafe { GetLogicalDrives() };
        for i in 0..26u32 {
            if mask & (1 << i) != 0 {
                let root = format!("{}:\\", (b'A' + i as u8) as char);
                let w: Vec<u16> = root.encode_utf16().chain(Some(0)).collect();
                if unsafe { GetDriveTypeW(w.as_ptr()) } == 3 {
                    v.push(root);
                }
            }
        }
        v
    }
    #[cfg(not(windows))]
    {
        vec!["/".to_string()]
    }
}

fn skip_dir(name: &str, depth: u32) -> bool {
    if matches!(name, "$Recycle.Bin" | "System Volume Information" | "$RECYCLE.BIN") {
        return true;
    }
    if depth == 0 {
        #[cfg(windows)]
        {
            return matches!(
                name,
                "Windows" | "$WinREAgent" | "Recovery" | "Config.Msi" | "$SysReset" | "$Windows.~BT" | "$Windows.~WS" | "$GetCurrent"
            );
        }
        #[cfg(not(windows))]
        {
            return matches!(name, "proc" | "sys" | "dev" | "run" | "snap");
        }
    }
    false
}

struct Job {
    path: PathBuf,
    idx: u32,
    noisy: bool,
    depth: u32,
}

/// `target` dizinine paralel tarama. Her klasör için kısa bir yazma kilidi alınır,
/// böylece tarama sürerken arama yapılabilir.
pub fn scan(roots: &[String], target: &RwLock<Index>, progress: &AtomicUsize) {
    let queue: Mutex<(Vec<Job>, usize)> = Mutex::new((Vec::new(), 0));
    let cv = Condvar::new();
    {
        let mut ix = target.write().unwrap();
        let mut q = queue.lock().unwrap();
        for r in roots {
            let p = PathBuf::from(r);
            if !p.is_dir() {
                continue;
            }
            let i = ix.push(r, NONE, 0, 0, true, 0);
            q.0.push(Job { path: p, idx: i, noisy: false, depth: 0 });
        }
    }
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 16) * 2;
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                loop {
                    let job = {
                        let mut q = queue.lock().unwrap();
                        loop {
                            if let Some(j) = q.0.pop() {
                                q.1 += 1;
                                break Some(j);
                            }
                            if q.1 == 0 {
                                break None;
                            }
                            q = cv.wait(q).unwrap();
                        }
                    };
                    let Some(job) = job else {
                        cv.notify_all();
                        return;
                    };
                    let mut items: Vec<(String, u64, i64, bool, bool)> = Vec::new();
                    if let Ok(rd) = std::fs::read_dir(&job.path) {
                        for de in rd.flatten() {
                            let name = de.file_name().to_string_lossy().to_string();
                            let Ok(ft) = de.file_type() else { continue };
                            let link = ft.is_symlink();
                            let is_dir = ft.is_dir() && !link;
                            if is_dir && skip_dir(&name, job.depth) {
                                continue;
                            }
                            let (size, mt) = match de.metadata() {
                                Ok(m) => (if is_dir { 0 } else { m.len() }, mtime_of(&m)),
                                Err(_) => (0, 0),
                            };
                            items.push((name, size, mt, is_dir, link));
                        }
                    }
                    let mut subs = Vec::new();
                    if !items.is_empty() {
                        let mut ix = target.write().unwrap();
                        for (name, size, mt, is_dir, link) in &items {
                            let noisy = job.noisy || (*is_dir && noisy_name(name));
                            let mut fl = if noisy { F_NOISY } else { 0 };
                            if *link {
                                fl |= F_LINK;
                            }
                            let i = ix.push(name, job.idx, *size, *mt, *is_dir, fl);
                            if *is_dir {
                                subs.push(Job { path: job.path.join(name), idx: i, noisy, depth: job.depth + 1 });
                            }
                        }
                    }
                    progress.fetch_add(items.len(), Ordering::Relaxed);
                    let mut q = queue.lock().unwrap();
                    q.0.extend(subs);
                    q.1 -= 1;
                    drop(q);
                    cv.notify_all();
                }
            });
        }
    });
}

pub fn cache_path() -> PathBuf {
    util::data_dir().join("index").join("index.bin")
}

/// Arka plan dizinleme: önbelleği yükle, sonra yeniden tara ve değiştir.
pub struct Indexer {
    pub index: Arc<RwLock<Index>>,
    pub busy: Arc<std::sync::atomic::AtomicBool>,
    pub progress: Arc<AtomicUsize>,
    /// Dizin her değiştirildiğinde artar (önbellek yüklendi, tarama bitti).
    pub version: Arc<AtomicUsize>,
}

impl Indexer {
    pub fn new() -> Self {
        Indexer {
            index: Arc::new(RwLock::new(Index::default())),
            busy: Arc::new(Default::default()),
            progress: Arc::new(AtomicUsize::new(0)),
            version: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn start(&self, use_cache: bool) {
        if self.busy.swap(true, Ordering::SeqCst) {
            return;
        }
        let (index, busy, progress, version) =
            (self.index.clone(), self.busy.clone(), self.progress.clone(), self.version.clone());
        std::thread::spawn(move || {
            let roots = default_roots();
            let cp = cache_path();
            let have = index.read().unwrap().len() > 0;
            let mut cached = have;
            if !have && use_cache
                && let Some(ix) = Index::load(&cp) {
                    *index.write().unwrap() = ix;
                    cached = true;
                    version.fetch_add(1, Ordering::SeqCst);
                }
            progress.store(0, Ordering::SeqCst);
            if cached {
                let fresh = RwLock::new(Index::default());
                scan(&roots, &fresh, &progress);
                *index.write().unwrap() = fresh.into_inner().unwrap();
            } else {
                scan(&roots, &index, &progress);
            }
            version.fetch_add(1, Ordering::SeqCst);
            busy.store(false, Ordering::SeqCst);
            let _ = index.read().unwrap().save(&cp);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scan_and_cache_roundtrip() {
        let d = std::env::temp_dir().join(format!("bul_t_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("proje/src")).unwrap();
        std::fs::create_dir_all(d.join("node_modules/x")).unwrap();
        std::fs::write(d.join("proje/src/fetchUser.ts"), "export function fetchUser() {}\n").unwrap();
        std::fs::write(d.join("İndir.PNG"), [0u8; 10]).unwrap();
        let ix = RwLock::new(Index::default());
        let pr = AtomicUsize::new(0);
        scan(&[d.to_string_lossy().to_string()], &ix, &pr);
        let ix = ix.into_inner().unwrap();
        assert_eq!(ix.len(), 7);
        let f = (0..ix.len() as u32).find(|&i| ix.name(i) == "fetchUser.ts").unwrap();
        assert!(ix.path(f).ends_with(&format!("proje{SEP}src{SEP}fetchUser.ts")));
        assert_eq!(ix.find(&ix.path(f)), Some(f));
        let nm = (0..ix.len() as u32).find(|&i| ix.name(i) == "x").unwrap();
        assert!(ix.entries[nm as usize].flags & F_NOISY != 0);
        let cp = d.join("c.bin");
        ix.save(&cp).unwrap();
        let l = Index::load(&cp).unwrap();
        assert_eq!(l.len(), ix.len());
        assert_eq!(l.path(f), ix.path(f));
        assert_eq!(l.lower(f), "fetchuser.ts");
        let _ = std::fs::remove_dir_all(&d);
    }
}
