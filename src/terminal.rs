//! "Terminal aç": seçili terminali geçerli klasörde başlatma.
//! Komut üretimi saf (test edilebilir); başlatma `launch` ile.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Term {
    WindowsTerminal,
    PowerShell,
    Cmd,
    Hyper,
    Custom,
}

impl Term {
    pub const ALL: [Term; 5] = [Term::WindowsTerminal, Term::PowerShell, Term::Cmd, Term::Hyper, Term::Custom];

    /// config.json'daki ad.
    pub fn id(self) -> &'static str {
        match self {
            Term::WindowsTerminal => "wt",
            Term::PowerShell => "powershell",
            Term::Cmd => "cmd",
            Term::Hyper => "hyper",
            Term::Custom => "custom",
        }
    }

    pub fn from_id(s: &str) -> Option<Term> {
        Term::ALL.into_iter().find(|t| t.id() == s)
    }

    pub fn label(self) -> &'static str {
        match self {
            Term::WindowsTerminal => "Windows Terminal",
            Term::PowerShell => "PowerShell",
            Term::Cmd => t!("Komut İstemi (CMD)", "Command Prompt (CMD)"),
            Term::Hyper => "Hyper",
            Term::Custom => t!("Başka bir program", "Another program"),
        }
    }
}

/// Başlatılacak komut.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Launch {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    /// Konsol programı: yeni konsol penceresinde açılsın.
    pub new_console: bool,
}

/// Ortam: algılama için gereken yollar (testte sahte değerler verilebilir).
#[derive(Clone, Debug, Default)]
pub struct Env {
    pub local_appdata: Option<PathBuf>,
    pub path: Option<OsString>,
    pub system_root: Option<PathBuf>,
}

impl Env {
    pub fn current() -> Env {
        Env {
            local_appdata: std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
            path: std::env::var_os("PATH"),
            system_root: std::env::var_os("SystemRoot").or_else(|| std::env::var_os("WINDIR")).map(PathBuf::from),
        }
    }
}

/// Uygulama yürütme takma adları (WindowsApps\wt.exe) yeniden ayrıştırma noktasıdır:
/// hedefi izlemeden var olup olmadığına bak.
fn present(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok()
}

fn on_path(env: &Env, exe: &str) -> Option<PathBuf> {
    let path = env.path.as_ref()?;
    std::env::split_paths(path).map(|d| d.join(exe)).find(|p| present(p))
}

pub fn find_wt(env: &Env) -> Option<PathBuf> {
    if let Some(l) = &env.local_appdata {
        let p = l.join("Microsoft").join("WindowsApps").join("wt.exe");
        if present(&p) {
            return Some(p);
        }
    }
    on_path(env, "wt.exe")
}

pub fn find_hyper(env: &Env) -> Option<PathBuf> {
    if let Some(l) = &env.local_appdata {
        let p = l.join("Programs").join("Hyper").join("Hyper.exe");
        if present(&p) {
            return Some(p);
        }
    }
    on_path(env, "Hyper.exe")
}

fn system_exe(env: &Env, rel: &[&str], name: &str) -> PathBuf {
    if let Some(r) = &env.system_root {
        let mut p = r.join("System32");
        for s in rel {
            p = p.join(s);
        }
        let p = p.join(name);
        if present(&p) {
            return p;
        }
    }
    PathBuf::from(name)
}

/// Bu bilgisayarda bulunan seçenekler (Başka bir program her zaman listede).
pub fn available(env: &Env) -> Vec<Term> {
    let mut v = Vec::new();
    if find_wt(env).is_some() {
        v.push(Term::WindowsTerminal);
    }
    v.push(Term::PowerShell);
    v.push(Term::Cmd);
    if find_hyper(env).is_some() {
        v.push(Term::Hyper);
    }
    v.push(Term::Custom);
    v
}

/// Varsayılan: Windows Terminal varsa o, yoksa PowerShell.
pub fn default_term(env: &Env) -> Term {
    if find_wt(env).is_some() { Term::WindowsTerminal } else { Term::PowerShell }
}

/// Ayardaki seçimi çöz ("auto" ya da bilinmeyen → varsayılan; yüklü olmayan → varsayılan).
pub fn resolve(id: &str, custom: &str, env: &Env) -> Term {
    match Term::from_id(id) {
        Some(Term::WindowsTerminal) if find_wt(env).is_some() => Term::WindowsTerminal,
        Some(Term::Hyper) if find_hyper(env).is_some() => Term::Hyper,
        Some(Term::Custom) if !custom.trim().is_empty() => Term::Custom,
        Some(t @ (Term::PowerShell | Term::Cmd)) => t,
        _ => default_term(env),
    }
}

/// Windows Terminal komut satırında ';' komut ayırıcıdır: klasör adındaki ';' kaçışlanır.
fn wt_dir_arg(dir: &str) -> String {
    let d = dir.replace(';', "\\;");
    // "C:\" gibi sondaki ters bölü, tırnaklama sırasında kapanış tırnağını kaçışlamasın
    if d.ends_with('\\') && d.len() > 3 { d.trim_end_matches('\\').to_string() } else { d }
}

/// Komutu üret. Klasör yolu boşluk içerebilir: her argüman ayrı verilir, tırnaklamayı
/// işletim sistemi katmanı yapar (Rust'ın Windows argüman kaçışlaması).
pub fn build(term: Term, custom: &str, dir: &str, env: &Env) -> Result<Launch, String> {
    let cwd = PathBuf::from(dir);
    let l = |program: PathBuf, args: Vec<OsString>, new_console: bool| Launch { program, args, cwd: cwd.clone(), new_console };
    Ok(match term {
        Term::WindowsTerminal => {
            let p = find_wt(env).unwrap_or_else(|| PathBuf::from("wt.exe"));
            l(p, vec!["-d".into(), wt_dir_arg(dir).into()], false)
        }
        Term::PowerShell => l(system_exe(env, &["WindowsPowerShell", "v1.0"], "powershell.exe"), vec!["-NoExit".into(), "-NoLogo".into()], true),
        Term::Cmd => l(system_exe(env, &[], "cmd.exe"), vec!["/K".into()], true),
        Term::Hyper => {
            let p = find_hyper(env).ok_or(t!("Hyper bulunamadı", "Hyper not found"))?;
            // Hyper'ın kendi "Hyper'ı burada aç" kaydı da klasörü argüman olarak verir
            l(p, vec![dir.into()], false)
        }
        Term::Custom => {
            let c = custom.trim().trim_matches('"');
            if c.is_empty() {
                return Err(t!("Ayarlar'dan terminal programını seçin", "Choose the terminal program in Settings").into());
            }
            l(PathBuf::from(c), Vec::new(), false)
        }
    })
}

/// Komutu başlat.
pub fn launch(cmd: &Launch) -> Result<(), String> {
    let mut c = std::process::Command::new(&cmd.program);
    c.args(&cmd.args).current_dir(&cmd.cwd);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_CONSOLE: u32 = 0x10;
        if cmd.new_console {
            c.creation_flags(CREATE_NEW_CONSOLE);
        }
    }
    c.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    c.spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Özel program yolu geçerli mi? (var olan bir dosya; Windows'ta .exe/.bat/.cmd)
pub fn valid_custom(p: &str) -> bool {
    let p = p.trim().trim_matches('"');
    let path = Path::new(p);
    if !path.is_file() {
        return false;
    }
    if cfg!(windows) {
        let e = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        matches!(e.as_str(), "exe" | "bat" | "cmd" | "com")
    } else {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_env(with_wt: bool, with_hyper: bool) -> (Env, PathBuf) {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("fp-term-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let la = root.join("Local");
        if with_wt {
            std::fs::create_dir_all(la.join("Microsoft/WindowsApps")).unwrap();
            std::fs::write(la.join("Microsoft/WindowsApps/wt.exe"), b"").unwrap();
        }
        if with_hyper {
            std::fs::create_dir_all(la.join("Programs/Hyper")).unwrap();
            std::fs::write(la.join("Programs/Hyper/Hyper.exe"), b"").unwrap();
        }
        std::fs::create_dir_all(&la).unwrap();
        (Env { local_appdata: Some(la), path: Some(OsString::new()), system_root: None }, root)
    }

    #[test]
    fn detection_and_default() {
        let (e, r) = fake_env(true, true);
        assert_eq!(available(&e), vec![Term::WindowsTerminal, Term::PowerShell, Term::Cmd, Term::Hyper, Term::Custom]);
        assert_eq!(default_term(&e), Term::WindowsTerminal);
        assert_eq!(resolve("auto", "", &e), Term::WindowsTerminal);
        assert_eq!(resolve("hyper", "", &e), Term::Hyper);
        let _ = std::fs::remove_dir_all(r);
        let (e, r) = fake_env(false, false);
        assert_eq!(available(&e), vec![Term::PowerShell, Term::Cmd, Term::Custom]);
        assert_eq!(default_term(&e), Term::PowerShell);
        // yüklü olmayan seçim → varsayılan
        assert_eq!(resolve("hyper", "", &e), Term::PowerShell);
        assert_eq!(resolve("wt", "", &e), Term::PowerShell);
        assert_eq!(resolve("custom", "", &e), Term::PowerShell);
        assert_eq!(resolve("custom", "C:\\x\\a.exe", &e), Term::Custom);
        assert_eq!(resolve("cmd", "", &e), Term::Cmd);
        let _ = std::fs::remove_dir_all(r);
        for t in Term::ALL {
            assert_eq!(Term::from_id(t.id()), Some(t));
        }
    }

    #[test]
    fn commands_with_spaces() {
        let (e, r) = fake_env(true, true);
        let dir = "C:\\Program Files (x86)\\Oyunlar";
        let wt = build(Term::WindowsTerminal, "", dir, &e).unwrap();
        assert!(wt.program.ends_with("wt.exe"));
        assert_eq!(wt.args, vec![OsString::from("-d"), OsString::from(dir)]);
        assert_eq!(wt.cwd, PathBuf::from(dir));
        assert!(!wt.new_console);
        // ';' kaçışlanır, sondaki ters bölü atılır (kök hariç)
        let wt2 = build(Term::WindowsTerminal, "", "D:\\a;b\\", &e).unwrap();
        assert_eq!(wt2.args[1], OsString::from("D:\\a\\;b"));
        let wt3 = build(Term::WindowsTerminal, "", "C:\\", &e).unwrap();
        assert_eq!(wt3.args[1], OsString::from("C:\\"));

        let ps = build(Term::PowerShell, "", dir, &e).unwrap();
        assert!(ps.program.to_string_lossy().to_lowercase().ends_with("powershell.exe"));
        assert_eq!(ps.cwd, PathBuf::from(dir));
        assert!(ps.new_console);
        assert!(ps.args.contains(&OsString::from("-NoExit")));

        let cmd = build(Term::Cmd, "", dir, &e).unwrap();
        assert!(cmd.program.to_string_lossy().to_lowercase().ends_with("cmd.exe"));
        assert_eq!(cmd.cwd, PathBuf::from(dir));

        let hy = build(Term::Hyper, "", dir, &e).unwrap();
        assert!(hy.program.ends_with("Programs/Hyper/Hyper.exe") || hy.program.to_string_lossy().ends_with("Hyper.exe"));
        assert_eq!(hy.args, vec![OsString::from(dir)]);
        assert_eq!(hy.cwd, PathBuf::from(dir));

        let cu = build(Term::Custom, "\"C:\\Tools\\My Term\\term.exe\"", dir, &e).unwrap();
        assert_eq!(cu.program, PathBuf::from("C:\\Tools\\My Term\\term.exe"));
        assert!(cu.args.is_empty());
        assert_eq!(cu.cwd, PathBuf::from(dir));
        assert!(build(Term::Custom, "  ", dir, &e).is_err());
        let _ = std::fs::remove_dir_all(r);
        let (e, r) = fake_env(false, false);
        assert!(build(Term::Hyper, "", dir, &e).is_err());
        let _ = std::fs::remove_dir_all(r);
    }
}
