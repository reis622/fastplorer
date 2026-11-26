//! Klasör boyutları: arka planda (ayrı iş parçacığında) özyinelemeli toplam.
//! Yalnızca ":si" sıralaması etkinken istenir; iptal edilebilir, yol başına önbelleklenir.
//! Bağlantılar / bağlantı noktaları (reparse point, junction, symlink) izlenmez (döngü olmaz);
//! erişim reddedilen klasörler sessizce atlanır.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Önbellek ömrü: bu süreden eski boyutlar yeniden hesaplanır.
const TTL: Duration = Duration::from_secs(600);

/// `dir` altındaki tüm dosyaların toplam boyutu (bayt). `cancel()` true dönerse None.
pub fn dir_size(dir: &Path, cancel: &dyn Fn() -> bool) -> Option<u64> {
    let mut total = 0u64;
    let mut stack: Vec<PathBuf> = vec![dir.to_path_buf()];
    let mut n = 0u32;
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            n = n.wrapping_add(1);
            if n.is_multiple_of(128) && cancel() {
                return None;
            }
            // file_type / metadata bağlantıları izlemez
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            let Ok(md) = e.metadata() else { continue };
            if is_reparse(&md) {
                continue;
            }
            if ft.is_dir() {
                stack.push(e.path());
            } else {
                total = total.saturating_add(md.len());
            }
        }
    }
    if cancel() { None } else { Some(total) }
}

#[cfg(windows)]
fn is_reparse(md: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    md.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse(_md: &std::fs::Metadata) -> bool {
    false
}

struct Inner {
    cache: Mutex<HashMap<String, (u64, Instant)>>,
    /// İstek kuşağı: yeni istek eskisini iptal eder.
    generation: AtomicU64,
    /// Tamamlanan hesap sayısı (arayüz değişikliği fark etsin diye).
    done: AtomicU64,
    ctx: Mutex<Option<eframe::egui::Context>>,
}

pub struct DirSizes {
    inner: Arc<Inner>,
    tx: Sender<(u64, String)>,
}

impl Default for DirSizes {
    fn default() -> Self {
        Self::new()
    }
}

impl DirSizes {
    pub fn new() -> DirSizes {
        let inner = Arc::new(Inner { cache: Mutex::new(HashMap::new()), generation: AtomicU64::new(0), done: AtomicU64::new(0), ctx: Mutex::new(None) });
        let (tx, rx) = channel::<(u64, String)>();
        let w = inner.clone();
        let _ = std::thread::Builder::new().name("klasor-boyutu".into()).spawn(move || worker(w, rx));
        DirSizes { inner, tx }
    }

    /// Hesap bitince ekranı yenilemek için.
    pub fn set_ctx(&self, ctx: &eframe::egui::Context) {
        if let Ok(mut c) = self.inner.ctx.lock()
            && c.is_none()
        {
            *c = Some(ctx.clone());
        }
    }

    /// Bilinen (taze) boyut.
    pub fn get(&self, path: &str) -> Option<u64> {
        let c = self.inner.cache.lock().ok()?;
        c.get(path).filter(|(_, t)| t.elapsed() < TTL).map(|(s, _)| *s)
    }

    /// Bu klasörlerin boyutlarını sırayla hesapla; önceki bekleyen/süren hesap iptal edilir.
    pub fn request(&self, paths: &[String]) {
        let g = self.inner.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let mut seen = HashSet::new();
        for p in paths {
            if self.get(p).is_none() && seen.insert(p.clone()) {
                let _ = self.tx.send((g, p.clone()));
            }
        }
    }

    /// Bekleyen/süren hesabı iptal et.
    pub fn cancel(&self) {
        self.inner.generation.fetch_add(1, Ordering::SeqCst);
    }

    /// Tamamlanan hesap sayacı.
    pub fn version(&self) -> u64 {
        self.inner.done.load(Ordering::SeqCst)
    }
}

fn worker(inner: Arc<Inner>, rx: Receiver<(u64, String)>) {
    while let Ok((g, path)) = rx.recv() {
        let live = || inner.generation.load(Ordering::SeqCst) == g;
        if !live() {
            continue;
        }
        if inner.cache.lock().ok().is_some_and(|c| c.get(&path).is_some_and(|(_, t)| t.elapsed() < TTL)) {
            continue;
        }
        if let Some(s) = dir_size(Path::new(&path), &|| !live()) {
            if let Ok(mut c) = inner.cache.lock() {
                c.insert(path, (s, Instant::now()));
            }
            inner.done.fetch_add(1, Ordering::SeqCst);
            if let Ok(c) = inner.ctx.lock()
                && let Some(ctx) = c.as_ref()
            {
                ctx.request_repaint();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("fp-dirsize-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn sums_recursively_and_cancels() {
        let d = tmp("sum");
        std::fs::create_dir_all(d.join("a/b/c")).unwrap();
        std::fs::write(d.join("x.bin"), vec![0u8; 1000]).unwrap();
        std::fs::write(d.join("a/y.bin"), vec![0u8; 200]).unwrap();
        std::fs::write(d.join("a/b/c/z.bin"), vec![0u8; 30]).unwrap();
        assert_eq!(dir_size(&d, &|| false), Some(1230));
        assert_eq!(dir_size(&d.join("a"), &|| false), Some(230));
        assert_eq!(dir_size(&d.join("yok"), &|| false), Some(0), "olmayan / erişilemeyen klasör: 0");
        assert_eq!(dir_size(&d, &|| true), None, "iptal");
        #[cfg(unix)]
        {
            // döngü yapan bağlantı izlenmez
            std::os::unix::fs::symlink(&d, d.join("a/b/dongu")).unwrap();
            std::os::unix::fs::symlink(d.join("x.bin"), d.join("a/bag.bin")).unwrap();
            assert_eq!(dir_size(&d, &|| false), Some(1230));
            // erişim reddi: atlanır
            use std::os::unix::fs::PermissionsExt;
            std::fs::create_dir_all(d.join("kapali")).unwrap();
            std::fs::write(d.join("kapali/k.bin"), vec![0u8; 5]).unwrap();
            std::fs::set_permissions(d.join("kapali"), std::fs::Permissions::from_mode(0o000)).unwrap();
            let s = dir_size(&d, &|| false).unwrap();
            assert!(s == 1230 || s == 1235, "erişim reddi hata vermez ({s})");
            std::fs::set_permissions(d.join("kapali"), std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn background_cache() {
        let d = tmp("bg");
        std::fs::create_dir_all(d.join("k1")).unwrap();
        std::fs::create_dir_all(d.join("k2/alt")).unwrap();
        std::fs::write(d.join("k1/a"), vec![0u8; 10]).unwrap();
        std::fs::write(d.join("k2/alt/b"), vec![0u8; 70]).unwrap();
        let ds = DirSizes::new();
        let p1 = d.join("k1").to_string_lossy().to_string();
        let p2 = d.join("k2").to_string_lossy().to_string();
        assert_eq!(ds.get(&p1), None);
        ds.request(&[p1.clone(), p2.clone()]);
        let t = Instant::now();
        while (ds.get(&p1).is_none() || ds.get(&p2).is_none()) && t.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!((ds.get(&p1), ds.get(&p2)), (Some(10), Some(70)));
        assert!(ds.version() >= 2);
        // önbellekte olan yeniden hesaplanmaz
        let v = ds.version();
        ds.request(std::slice::from_ref(&p1));
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(ds.version(), v);
        let _ = std::fs::remove_dir_all(d);
    }
}
