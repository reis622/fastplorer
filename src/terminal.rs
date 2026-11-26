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
    /// Argümanlardan sonra olduğu gibi (kaçışlamadan) eklenen komut satırı parçası (cmd.exe /S /K "…").
    pub raw: Option<OsString>,
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
    let l = |program: PathBuf, args: Vec<OsString>, new_console: bool| Launch { program, args, cwd: cwd.clone(), new_console, raw: None };
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

/// PowerShell -EncodedCommand değeri: UTF-16LE baytlarının base64'ü. Tırnak, ';', '&' gibi
/// karakterler komut satırında hiç görünmez; kaçışlama sorunu olmaz.
pub fn ps_encoded(cmd: &str) -> String {
    let bytes: Vec<u8> = cmd.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for ch in bytes.chunks(3) {
        let n = (ch[0] as u32) << 16 | (*ch.get(1).unwrap_or(&0) as u32) << 8 | *ch.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= ch.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Komut satırının çalıştırılma planı.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunPlan {
    pub launch: Launch,
    /// Komut zinciri tek satır: "a && b && c" (bildirim ve panoya kopyalama için).
    pub text: String,
    /// Terminal dışarıdan komut almıyor (Hyper, Başka bir program): terminal klasörde açılır,
    /// komut panoya kopyalanır.
    pub copy: bool,
}

/// Windows Terminal komut satırında ';' yeni sekme ayırıcısıdır: komuttaki ';' kaçışlanır.
fn wt_escape(line: &str) -> String {
    line.replace(';', "\\;")
}

/// Komut: `line` komutunu `dir` klasöründe Ayarlar'da seçili terminalde, ayrı pencerede çalıştır;
/// pencere komut bitince açık kalır. Komut kullanıcının yazdığı gibi çalışır (ör. "ipconfig /all").
/// - CMD: cmd.exe /S /K "<komut>" (komut satırı olduğu gibi verilir; /S ile cmd yalnızca en dıştaki
///   tırnak çiftini atar, içerideki tırnaklar korunur). Yeni konsol penceresi.
/// - PowerShell: powershell.exe -NoExit -NoLogo -EncodedCommand <base64> (kaçışlama sorunu yok).
/// - Windows Terminal: wt -d <klasör> cmd.exe /K <komut> (varsayılan profilin penceresinde yeni sekme;
///   komuttaki ';' kaçışlanır).
/// - Hyper / Başka bir program: dışarıdan komut verilemiyor (Hyper'ın komut satırı yalnızca klasör
///   alır); terminal klasörde açılır, komut panoya kopyalanır (`copy`).
///
/// Birden çok komut (`cmds`) sırayla çalışır, biri başarısız olursa durur:
/// CMD ve Windows Terminal'de "a && b && c"; PowerShell'de (5.1'de && yok) her komuttan sonra
/// $? / $LASTEXITCODE denetleyen bir betik; Hyper / Başka programda panoya "a && b && c".
pub fn build_run(term: Term, custom: &str, dir: &str, cmds: &[String], env: &Env) -> Result<RunPlan, String> {
    let cmds: Vec<String> = cmds.iter().map(|c| c.replace(['\r', '\n'], " ").trim().to_string()).filter(|c| !c.is_empty()).collect();
    if cmds.is_empty() {
        return Err(t!("Komut boş", "The command is empty").into());
    }
    let chain = cmds.join(" && ");
    let line = chain.as_str();
    let cwd = PathBuf::from(dir);
    let run = |launch: Launch| Ok(RunPlan { launch, text: chain.clone(), copy: false });
    let ps_cmd = if cmds.len() == 1 { cmds[0].clone() } else { ps_chain_script(&cmds) };
    match term {
        Term::Cmd => run(Launch { program: system_exe(env, &[], "cmd.exe"), args: Vec::new(), cwd, new_console: true, raw: Some(format!("/S /K \"{line}\"").into()) }),
        Term::PowerShell => run(Launch {
            program: system_exe(env, &["WindowsPowerShell", "v1.0"], "powershell.exe"),
            args: vec!["-NoExit".into(), "-NoLogo".into(), "-EncodedCommand".into(), ps_encoded(&ps_cmd).into()],
            cwd,
            new_console: true,
            raw: None,
        }),
        Term::WindowsTerminal => {
            let p = find_wt(env).unwrap_or_else(|| PathBuf::from("wt.exe"));
            run(Launch { program: p, args: vec!["-d".into(), wt_dir_arg(dir).into(), "cmd.exe".into(), "/K".into()], cwd, new_console: false, raw: Some(wt_escape(line).into()) })
        }
        Term::Hyper | Term::Custom => Ok(RunPlan { launch: build(term, custom, dir, env)?, text: chain.clone(), copy: true }),
    }
}

/// PowerShell betiği: komutlar sırayla; her birinden sonra $? ve $LASTEXITCODE denetlenir, hata
/// olursa kırmızı bir satır yazılır ve betik durur (pencere -NoExit ile açık kalır).
pub fn ps_chain_script(cmds: &[String]) -> String {
    let mut s = String::new();
    for (i, c) in cmds.iter().enumerate() {
        let msg = tf!("Durdu: {}. komut başarısız ({})", "Stopped: command {} failed ({})", i + 1, c).replace('\'', "''");
        s.push_str("$global:LASTEXITCODE = 0\n");
        s.push_str(c);
        s.push_str("\n$fpOk = $?\n");
        s.push_str(&format!("if (-not $fpOk -or $LASTEXITCODE -ne 0) {{ Write-Host '{msg}' -ForegroundColor Red; return }}\n"));
    }
    s
}

/// Windows (MSVC) kurallarıyla tek argüman tırnaklama (CommandLineToArgvW ile geri ayrışır).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn quote_arg(a: &str) -> String {
    if !a.is_empty() && !a.contains([' ', '\t', '"', '\n']) {
        return a.to_string();
    }
    let mut s = String::from("\"");
    let mut bs = 0usize;
    for c in a.chars() {
        match c {
            '\\' => bs += 1,
            '"' => {
                s.extend(std::iter::repeat_n('\\', bs * 2 + 1));
                s.push('"');
                bs = 0;
            }
            _ => {
                s.extend(std::iter::repeat_n('\\', bs));
                s.push(c);
                bs = 0;
            }
        }
    }
    s.extend(std::iter::repeat_n('\\', bs * 2));
    s.push('"');
    s
}

/// Başlatılacak tam komut satırı: tırnaklı program, tırnaklanmış argümanlar, sonra olduğu gibi `raw`.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn command_line(l: &Launch) -> String {
    let mut s = format!("\"{}\"", l.program.to_string_lossy());
    for a in &l.args {
        s.push(' ');
        s.push_str(&quote_arg(&a.to_string_lossy()));
    }
    if let Some(r) = &l.raw {
        s.push(' ');
        s.push_str(&r.to_string_lossy());
    }
    s
}

/// Konsol programını yeni konsol penceresinde başlat (Windows). Standart giriş/çıkış yönlendirilmez:
/// konsol kendi klavye/ekran tutamaçlarını kullanır. (std::process::Command her zaman
/// STARTF_USESTDHANDLES verir; Stdio::null ile cmd /K girişte dosya sonu görüp hemen kapanırdı.)
#[cfg(windows)]
fn spawn_console(cmd: &Launch) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{CREATE_NEW_CONSOLE, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, PROCESS_INFORMATION, STARTUPINFOW};
    let mut line: Vec<u16> = std::ffi::OsStr::new(&command_line(cmd)).encode_wide().chain(Some(0)).collect();
    let cwd: Vec<u16> = cmd.cwd.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut si: STARTUPINFOW = unsafe { std::mem::zeroed() };
    si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    let ok = unsafe {
        CreateProcessW(
            std::ptr::null(),
            line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            CREATE_NEW_CONSOLE | CREATE_UNICODE_ENVIRONMENT,
            std::ptr::null(),
            cwd.as_ptr(),
            &si,
            &mut pi,
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    unsafe {
        CloseHandle(pi.hThread);
        CloseHandle(pi.hProcess);
    }
    Ok(())
}

/// Komutu başlat.
pub fn launch(cmd: &Launch) -> Result<(), String> {
    #[cfg(windows)]
    if cmd.new_console {
        return spawn_console(cmd);
    }
    let mut c = std::process::Command::new(&cmd.program);
    c.args(&cmd.args).current_dir(&cmd.cwd);
    #[cfg(windows)]
    if let Some(r) = &cmd.raw {
        use std::os::windows::process::CommandExt;
        c.raw_arg(r);
    }
    #[cfg(not(windows))]
    if let Some(r) = &cmd.raw {
        c.arg(r);
    }
    // pencereli programlar (wt, Hyper, başka program): kendi penceresi var, std akışlar kullanılmaz
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
    fn encoded_command() {
        // PowerShell'in beklediği biçim: UTF-16LE + base64
        assert_eq!(ps_encoded("dir"), "ZABpAHIA");
        assert_eq!(ps_encoded("ipconfig"), "aQBwAGMAbwBuAGYAaQBnAA==");
        assert_eq!(ps_encoded("ş"), "XwE=");
        assert_eq!(ps_encoded(""), "");
    }

    #[test]
    fn run_command_launch() {
        let v = |s: &str| vec![s.to_string()];
        let (e, r) = fake_env(true, true);
        let dir = "C:\\Program Files (x86)\\Oyunlar";
        let line = r#"echo "a b" & dir /s; ipconfig"#;
        // CMD: komut satırı olduğu gibi, yeni konsol
        let p = build_run(Term::Cmd, "", dir, &v(line), &e).unwrap();
        assert!(!p.copy);
        let c = &p.launch;
        assert!(c.program.to_string_lossy().to_lowercase().ends_with("cmd.exe"));
        assert!(c.args.is_empty() && c.new_console);
        assert_eq!(c.cwd, PathBuf::from(dir));
        assert_eq!(command_line(c), r#""cmd.exe" /S /K "echo "a b" & dir /s; ipconfig""#);
        // satır sonları tek satıra iner, baştaki/sondaki boşluk atılır
        assert_eq!(command_line(&build_run(Term::Cmd, "", dir, &v("  ipconfig\r\n/all "), &e).unwrap().launch), "\"cmd.exe\" /S /K \"ipconfig  /all\"");

        // PowerShell: -EncodedCommand (UTF-16LE base64), yeni konsol
        let ps = build_run(Term::PowerShell, "", dir, &v("ipconfig"), &e).unwrap();
        assert!(!ps.copy && ps.launch.new_console && ps.launch.raw.is_none());
        assert_eq!(ps.launch.args, ["-NoExit", "-NoLogo", "-EncodedCommand", "aQBwAGMAbwBuAGYAaQBnAA=="].map(OsString::from).to_vec());
        assert!(command_line(&ps.launch).ends_with("powershell.exe\" -NoExit -NoLogo -EncodedCommand aQBwAGMAbwBuAGYAaQBnAA=="));

        // Windows Terminal: wt -d <klasör> cmd.exe /K <komut>; ';' kaçışlanır; PowerShell'e zorlanmaz
        let wt = build_run(Term::WindowsTerminal, "", "D:\\a;b c\\", &v("dir /s; echo x"), &e).unwrap();
        assert!(!wt.copy && !wt.launch.new_console);
        assert!(wt.launch.program.ends_with("wt.exe"));
        let wl = command_line(&wt.launch);
        assert!(wl.ends_with(r#"wt.exe" -d "D:\a\;b c" cmd.exe /K dir /s\; echo x"#), "{wl}");

        // Hyper / Başka bir program: komut verilemez → klasörde aç + panoya kopyala (CMD'ye sessizce düşmez)
        let hy = build_run(Term::Hyper, "", dir, &v(line), &e).unwrap();
        assert!(hy.copy);
        assert!(hy.launch.program.to_string_lossy().ends_with("Hyper.exe"));
        assert_eq!(hy.launch.args, vec![OsString::from(dir)]);
        let cu = build_run(Term::Custom, "C:\\Tools\\My Term\\term.exe", dir, &v(line), &e).unwrap();
        assert!(cu.copy);
        assert_eq!(cu.launch.program, PathBuf::from("C:\\Tools\\My Term\\term.exe"));
        assert_eq!(cu.launch.cwd, PathBuf::from(dir));
        assert!(build_run(Term::Custom, "", dir, &v(line), &e).is_err(), "özel program seçilmemiş");

        // boş komut
        assert!(build_run(Term::Cmd, "", dir, &v("   "), &e).is_err());
        assert!(build_run(Term::PowerShell, "", dir, &v("\r\n"), &e).is_err());
        let _ = std::fs::remove_dir_all(r);
    }

    #[test]
    fn command_chains() {
        let (e, r) = fake_env(true, true);
        let dir = "C:\\Projeler";
        let cmds: Vec<String> = ["git clone https://github.com/u/p", "cd p", "cargo build --release", "copy target\\release\\p.exe C:\\bin"].map(String::from).to_vec();
        let chain = "git clone https://github.com/u/p && cd p && cargo build --release && copy target\\release\\p.exe C:\\bin";
        // CMD: && ile tek /K içinde
        let c = build_run(Term::Cmd, "", dir, &cmds, &e).unwrap();
        assert_eq!(c.text, chain);
        assert_eq!(command_line(&c.launch), format!("\"cmd.exe\" /S /K \"{chain}\""));
        // Windows Terminal: CMD ile aynı zincir (';' kaçışlı)
        let wt = build_run(Term::WindowsTerminal, "", dir, &cmds, &e).unwrap();
        assert!(command_line(&wt.launch).ends_with(&format!("-d C:\\Projeler cmd.exe /K {chain}")), "{}", command_line(&wt.launch));
        let wt2 = build_run(Term::WindowsTerminal, "", dir, &["echo a;b".to_string(), "dir".to_string()], &e).unwrap();
        assert!(command_line(&wt2.launch).ends_with("cmd.exe /K echo a\\;b && dir"));
        // PowerShell: her komuttan sonra $? / $LASTEXITCODE denetimi, hata olursa durur
        let ps = build_run(Term::PowerShell, "", dir, &cmds, &e).unwrap();
        let script = ps_chain_script(&cmds);
        assert_eq!(ps.launch.args.last(), Some(&OsString::from(ps_encoded(&script))));
        assert_eq!(script.matches("$LASTEXITCODE -ne 0").count(), 4);
        assert_eq!(script.matches("return }").count(), 4);
        assert!(!script.contains("&&"), "PowerShell 5.1'de && yok");
        let lines: Vec<&str> = script.lines().collect();
        assert_eq!(lines[0], "$global:LASTEXITCODE = 0");
        assert_eq!(lines[1], "git clone https://github.com/u/p");
        assert_eq!(lines[2], "$fpOk = $?");
        assert!(lines[3].starts_with("if (-not $fpOk -or $LASTEXITCODE -ne 0) { Write-Host 'Durdu: 1. komut başarısız (git clone https://github.com/u/p)' -ForegroundColor Red; return }"));
        assert_eq!(lines[5], "cd p");
        // tek tırnak betikte kaçışlanır
        let q = ps_chain_script(&["echo 'x'".to_string(), "dir".to_string()]);
        assert!(q.contains("(echo ''x'')"), "{q}");
        // tek komut: eskisi gibi doğrudan
        let one = build_run(Term::PowerShell, "", dir, &cmds[..1], &e).unwrap();
        assert_eq!(one.launch.args.last(), Some(&OsString::from(ps_encoded("git clone https://github.com/u/p"))));
        // Hyper / Başka program: zincir panoya
        let hy = build_run(Term::Hyper, "", dir, &cmds, &e).unwrap();
        assert!(hy.copy);
        assert_eq!(hy.text, chain);
        // boş öğeler atılır
        let c2 = build_run(Term::Cmd, "", dir, &["ipconfig /all".to_string(), "  ".to_string(), "dir".to_string()], &e).unwrap();
        assert_eq!(c2.text, "ipconfig /all && dir");
        let _ = std::fs::remove_dir_all(r);
    }

    #[test]
    fn argument_quoting() {
        assert_eq!(quote_arg("abc"), "abc");
        assert_eq!(quote_arg(""), "\"\"");
        assert_eq!(quote_arg("a b"), "\"a b\"");
        assert_eq!(quote_arg("C:\\a b\\"), "\"C:\\a b\\\\\"");
        assert_eq!(quote_arg("say \"hi\""), "\"say \\\"hi\\\"\"");
        assert_eq!(quote_arg("x\\\"y"), "\"x\\\\\\\"y\"");
        let l = Launch { program: PathBuf::from("C:\\Windows\\System32\\cmd.exe"), args: vec!["/K".into()], cwd: PathBuf::from("C:\\"), new_console: true, raw: None };
        assert_eq!(command_line(&l), "\"C:\\Windows\\System32\\cmd.exe\" /K");
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
