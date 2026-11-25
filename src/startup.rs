//! Açılış sağlamlığı: çalışma klasörü, açılış günlüğü (veri klasöründe startup.log), panik kaydı.
//! Windows açılışında (Run kaydı) çalışma klasörü genellikle System32'dir; uygulama hiçbir
//! dosyayı çalışma klasörüne göre aramaz, yine de exe'nin klasörüne geçilir.

use std::io::Write;
use std::path::PathBuf;

const MAX_LOG: u64 = 256 * 1024;

pub fn log_path() -> PathBuf {
    crate::util::data_dir().join("startup.log")
}

/// Günlüğe bir satır yaz (hata olursa sessizce geç; açılışı asla engellemez).
pub fn log(msg: impl AsRef<str>) {
    let p = log_path();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if std::fs::metadata(&p).is_ok_and(|m| m.len() > MAX_LOG) {
        let _ = std::fs::rename(&p, p.with_extension("old.log"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&p) {
        let t = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let _ = writeln!(f, "{t} [{}] {}", std::process::id(), msg.as_ref());
    }
}

/// Çalışma klasörünü exe'nin klasörüne al, panikleri günlüğe yaz, açılış bilgisini kaydet.
pub fn prepare(args: &[String]) {
    let exe = std::env::current_exe().ok();
    let cwd_before = std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_else(|e| format!("? ({e})"));
    if let Some(dir) = exe.as_ref().and_then(|e| e.parent()) {
        let _ = std::env::set_current_dir(dir);
    }
    log(format!(
        "açılış: sürüm {} · argümanlar {:?} · exe {} · çalışma klasörü {} → {}",
        env!("CARGO_PKG_VERSION"),
        &args[1..],
        exe.as_ref().map(|e| e.display().to_string()).unwrap_or_default(),
        cwd_before,
        std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default()
    ));
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log(format!("PANİK: {info}"));
        prev(info);
    }));
    unblock_self(exe.as_deref());
}

/// İnternetten indirilen exe'nin "internetten geldi" işareti (Zone.Identifier) açılışta
/// otomatik başlatmayı engelleyebilir. Kullanıcı uygulamayı zaten bir kez kendisi açtığı için
/// (Özellikler → Engellemeyi kaldır ile aynı) işaret kaldırılır.
fn unblock_self(exe: Option<&std::path::Path>) {
    #[cfg(windows)]
    if let Some(e) = exe {
        let ads = format!("{}:Zone.Identifier", e.display());
        if std::fs::metadata(&ads).is_ok() {
            match std::fs::remove_file(&ads) {
                Ok(()) => log("exe'deki \"internetten indirildi\" işareti kaldırıldı (otomatik başlatma engellenmesin)"),
                Err(err) => log(format!("internetten indirildi işareti kaldırılamadı: {err}")),
            }
        }
    }
    #[cfg(not(windows))]
    let _ = exe;
}

/// `--retry=N` argümanı (yeniden başlatma denemesi).
pub fn retry_arg(args: &[String]) -> u32 {
    args.iter().find_map(|a| a.strip_prefix("--retry=")).and_then(|v| v.parse().ok()).unwrap_or(0)
}

/// Arayüz başlatılamadıysa (ör. Windows açılışında ekran sürücüsü henüz hazır değil) kendini
/// biraz sonra yeniden başlat. Kalıcı hata döngüsüne girmemek için en fazla `MAX_RETRY` deneme.
pub const MAX_RETRY: u32 = 5;

pub fn relaunch(hidden: bool, retry: u32) -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    let mut c = std::process::Command::new(exe);
    if hidden {
        c.arg("--hidden");
    }
    c.arg(format!("--retry={retry}"));
    match c.spawn() {
        Ok(_) => {
            log(format!("yeniden başlatılıyor (deneme {retry}/{MAX_RETRY})"));
            true
        }
        Err(e) => {
            log(format!("yeniden başlatılamadı: {e}"));
            false
        }
    }
}

/// Yeniden başlatma denemesinde önce biraz bekle (sistem açılışının oturması için).
pub fn retry_delay(retry: u32) -> std::time::Duration {
    std::time::Duration::from_secs((3 * retry as u64).min(15))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retry_args() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(retry_arg(&a(&["x.exe", "--hidden"])), 0);
        assert_eq!(retry_arg(&a(&["x.exe", "--hidden", "--retry=3"])), 3);
        assert_eq!(retry_arg(&a(&["x.exe", "--retry=abc"])), 0);
        assert_eq!(retry_delay(1).as_secs(), 3);
        assert_eq!(retry_delay(9).as_secs(), 15);
    }
}
