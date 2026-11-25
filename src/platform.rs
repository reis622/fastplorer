//! Platforma özgü işler: tek örnek, genel kısayol (varsayılan F9), tepsi simgesi, pencereyi
//! göster/gizle/konumla, DWM köşe yuvarlatma, oturum açılışında başlatma, Geri Dönüşüm Kutusu.
//! Windows dışında yalnızca egui görünüm komutlarıyla basit karşılıklar vardır (geliştirme/deneme için).

use eframe::egui;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, Ordering};

static CTX: OnceLock<egui::Context> = OnceLock::new();
/// Pencere gösterildi (uygulama durumu sıfırlansın, arama kutusu odaklansın).
static SHOWN: AtomicBool = AtomicBool::new(false);
/// Tepsi menüsünden "Ayarlar".
static OPEN_SETTINGS: AtomicBool = AtomicBool::new(false);
static HOTKEY_OK: AtomicBool = AtomicBool::new(true);
/// Kısayol kaydı sonucu: 0 = bekliyor, 1 = tamam, 2 = başarısız.
static HK_RESULT: AtomicI32 = AtomicI32::new(1);
static HK_MODS: AtomicU32 = AtomicU32::new(0);
static HK_VK: AtomicU32 = AtomicU32::new(crate::config::VK_F9);
/// Arayüz iş parçacığının son kare zamanı (ms) — bekçi için.
static HEARTBEAT: AtomicU64 = AtomicU64::new(0);
/// Son "göster" isteği zamanı (ms); 0 = bekleyen yok.
static SHOW_AT: AtomicU64 = AtomicU64::new(0);
/// Tek doğruluk kaynağı: pencere görünür olmalı mı (göster/gizle bunu ayarlar; bekçi buna uyar).
#[cfg(windows)]
static WANT_VISIBLE: AtomicBool = AtomicBool::new(false);
/// Son gösterme (kısayol/tepsi iş parçacığı saatine göre, ms).
#[cfg(windows)]
static LAST_SHOW_MS: AtomicU64 = AtomicU64::new(0);
/// Son kısayol işleme zamanı (ms) — çift WM_HOTKEY'e karşı 250 ms sıçrama önleme.
#[cfg(windows)]
static LAST_HOTKEY_MS: AtomicU64 = AtomicU64::new(0);
/// Kısayol/tepsi iş parçacığı başlatıldı mı (bir kez).
static STARTED: AtomicBool = AtomicBool::new(false);

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

pub fn take_shown() -> bool {
    SHOWN.swap(false, Ordering::SeqCst)
}

pub fn take_open_settings() -> bool {
    OPEN_SETTINGS.swap(false, Ordering::SeqCst)
}

pub fn hotkey_ok() -> bool {
    HOTKEY_OK.load(Ordering::SeqCst)
}

/// Her karede çağrılır: arayüz canlı.
pub fn heartbeat() {
    HEARTBEAT.store(now_ms(), Ordering::Relaxed);
    SHOW_AT.store(0, Ordering::Relaxed);
}

fn repaint() {
    if let Some(c) = CTX.get() {
        c.request_repaint();
    }
}

pub const WINDOW_TITLE: &str = "Fastplorer";

/// Ana pencere her gizlendiğinde artar (arayüz açık menüyü kapatır).
static HIDE_EPOCH: AtomicU64 = AtomicU64::new(0);

pub fn hide_epoch() -> u64 {
    HIDE_EPOCH.load(Ordering::SeqCst)
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::sync::atomic::AtomicIsize;
    use windows_sys::Win32::Foundation::*;
    use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
    use windows_sys::Win32::Graphics::Gdi::*;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Threading::{AttachThreadInput, CreateMutexW, GetCurrentProcessId, GetCurrentThreadId};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{MOD_NOREPEAT, RegisterHotKey, SetFocus, UnregisterHotKey};
    use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI};
    use windows_sys::Win32::UI::Shell::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    pub const TRAY_CLASS: &str = "fastplorer_tray_window";
    const WM_TRAY: u32 = WM_APP + 1;
    const WM_SHOW_REQ: u32 = WM_APP + 2;
    const WM_SET_HOTKEY: u32 = WM_APP + 3;
    const WM_SUSPEND_HOTKEY: u32 = WM_APP + 4;
    const HOTKEY_ID: i32 = 1;
    const TIMER_WATCHDOG: usize = 7;
    const TIMER_FOREGROUND: usize = 8;
    /// Açılışta kısayol/tepsi yeniden deneme zamanlayıcısı (2 sn).
    const TIMER_RETRY: usize = 9;
    /// Ön plana alma yeniden deneme sayacı (50 ms aralıkla).
    static FG_RETRIES: AtomicI32 = AtomicI32::new(0);
    /// Çapanın bulunduğu monitörün DPI'ı.
    static ANCHOR_DPI: AtomicI32 = AtomicI32::new(96);

    static MAIN: AtomicIsize = AtomicIsize::new(0);
    static TRAY: AtomicIsize = AtomicIsize::new(0);
    static TASKBAR_CREATED: AtomicI32 = AtomicI32::new(0);
    // Pencere çapası (fiziksel piksel): üst-orta nokta ve son boyut
    static ANCHOR_X: AtomicI32 = AtomicI32::new(i32::MIN);
    static ANCHOR_Y: AtomicI32 = AtomicI32::new(0);
    static W: AtomicI32 = AtomicI32::new(0);
    static H: AtomicI32 = AtomicI32::new(0);

    pub fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    /// Tek örnek: zaten çalışıyorsa (gerekirse) ona "göster" der ve false döner. Asla hata göstermez.
    /// `wait`: yeniden başlatma denemesi — önceki örnek kapanırken muteksi bırakması beklenir.
    pub fn single_instance(signal: bool, wait: bool) -> bool {
        unsafe {
            let name = wide("Local\\Fastplorer-single-instance");
            let mut h = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
            if wait {
                // en fazla ~20 sn: önceki örnek çıkınca muteks serbest kalır
                for _ in 0..40 {
                    if h.is_null() || GetLastError() != ERROR_ALREADY_EXISTS {
                        break;
                    }
                    CloseHandle(h);
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    h = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
                }
            }
            if h.is_null() {
                // muteks oluşturulamadı: yine de çalış (tek örnek denetimi olmadan)
                crate::startup::log("tek örnek kilidi oluşturulamadı; denetimsiz devam");
                return true;
            }
            if GetLastError() == ERROR_ALREADY_EXISTS {
                if signal {
                    // ilk örnek henüz tepsi penceresini açmamış olabilir: kısa süre bekle
                    let cls = wide(TRAY_CLASS);
                    for _ in 0..30 {
                        let w = FindWindowW(cls.as_ptr(), std::ptr::null());
                        if !w.is_null() {
                            // ilk örneğin öne gelebilmesi için izin ver
                            let mut pid = 0u32;
                            GetWindowThreadProcessId(w, &mut pid);
                            if pid != 0 {
                                AllowSetForegroundWindow(pid);
                            }
                            PostMessageW(w, WM_SHOW_REQ, 0, 0);
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
                crate::startup::log(if signal { "zaten çalışıyor: açık olana \"göster\" denildi, çıkılıyor" } else { "zaten çalışıyor: çıkılıyor" });
                CloseHandle(h);
                return false;
            }
            // tutamaç işlem boyunca açık kalır
            let _ = h;
        }
        true
    }

    unsafe extern "system" fn enum_cb(h: HWND, lp: LPARAM) -> i32 {
        unsafe {
            let mut pid = 0u32;
            GetWindowThreadProcessId(h, &mut pid);
            if pid != GetCurrentProcessId() {
                return 1;
            }
            let mut cls = [0u16; 64];
            let n = GetClassNameW(h, cls.as_mut_ptr(), 64);
            if String::from_utf16_lossy(&cls[..n.max(0) as usize]) == TRAY_CLASS {
                return 1;
            }
            let mut t = [0u16; 64];
            let n = GetWindowTextW(h, t.as_mut_ptr(), 64);
            if String::from_utf16_lossy(&t[..n.max(0) as usize]) == WINDOW_TITLE {
                *(lp as *mut isize) = h as isize;
                return 0;
            }
            1
        }
    }

    pub fn main_hwnd() -> HWND {
        let cur = MAIN.load(Ordering::SeqCst);
        if cur != 0 && unsafe { IsWindow(cur as HWND) } != 0 {
            return cur as HWND;
        }
        let mut found: isize = 0;
        unsafe {
            EnumWindows(Some(enum_cb), &mut found as *mut isize as LPARAM);
        }
        if found != 0 {
            MAIN.store(found, Ordering::SeqCst);
            round_corners(found as HWND);
        }
        found as HWND
    }

    /// Windows 11: DWM ile yuvarlak köşeler (Windows 10'da çağrı başarısız olur → kare köşeler).
    fn round_corners(h: HWND) {
        const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
        const DWMWCP_ROUND: u32 = 2;
        unsafe {
            let pref = DWMWCP_ROUND;
            let _ = DwmSetWindowAttribute(h, DWMWA_WINDOW_CORNER_PREFERENCE as _, &pref as *const u32 as *const _, 4);
        }
    }

    pub fn is_visible() -> bool {
        let h = main_hwnd();
        !h.is_null() && unsafe { IsWindowVisible(h) != 0 }
    }

    /// Her gösterişte: fare imlecinin bulunduğu monitörün çalışma alanında ortala.
    fn compute_anchor() {
        unsafe {
            let mut pt = POINT { x: 0, y: 0 };
            GetCursorPos(&mut pt);
            let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
            let mut mi: MONITORINFO = std::mem::zeroed();
            mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(mon, &mut mi) != 0 {
                let r = mi.rcWork;
                ANCHOR_X.store((r.left + r.right) / 2, Ordering::SeqCst);
                ANCHOR_Y.store(r.top + (r.bottom - r.top) * 18 / 100, Ordering::SeqCst);
            }
            let (mut dx, mut dy) = (96u32, 96u32);
            if GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) == 0 && dx > 0 {
                ANCHOR_DPI.store(dx as i32, Ordering::SeqCst);
            } else {
                ANCHOR_DPI.store(96, Ordering::SeqCst);
            }
        }
    }

    /// Konumla. Başka iş parçacığından çağrıldığında SWP_ASYNCWINDOWPOS ile asla bloklanmaz.
    fn place(h: HWND, flags: u32) {
        if ANCHOR_X.load(Ordering::SeqCst) == i32::MIN {
            compute_anchor();
        }
        let (mut w, mut hh) = (W.load(Ordering::SeqCst), H.load(Ordering::SeqCst));
        // Boyut pencerenin şu anki DPI'ına göre hesaplandı; hedef monitörün DPI'ı farklıysa ölçekle
        // (DPI değişince arayüz yeni boyutla yeniden konumlar).
        let wdpi = unsafe { GetDpiForWindow(h) } as i32;
        let mdpi = ANCHOR_DPI.load(Ordering::SeqCst);
        if wdpi > 0 && mdpi > 0 && wdpi != mdpi {
            w = w * mdpi / wdpi;
            hh = hh * mdpi / wdpi;
        }
        let x = ANCHOR_X.load(Ordering::SeqCst) - w / 2;
        let y = ANCHOR_Y.load(Ordering::SeqCst);
        unsafe {
            let own = GetWindowThreadProcessId(h, std::ptr::null_mut()) == GetCurrentThreadId();
            let flags = if own { flags } else { flags | SWP_ASYNCWINDOWPOS };
            if w > 0 && hh > 0 {
                SetWindowPos(h, HWND_TOPMOST, x, y, w, hh, flags);
            } else {
                SetWindowPos(h, HWND_TOPMOST, 0, 0, 0, 0, flags | SWP_NOSIZE | SWP_NOMOVE);
            }
        }
    }

    fn foreground(h: HWND) {
        unsafe {
            if GetForegroundWindow() == h {
                return;
            }
            AllowSetForegroundWindow(ASFW_ANY);
            if SetForegroundWindow(h) != 0 && GetForegroundWindow() == h {
                return;
            }
            // Ön plan kilidi: ön plandaki iş parçacığının girişine geçici olarak bağlan.
            let fg = GetForegroundWindow();
            let ft = if fg.is_null() { 0 } else { GetWindowThreadProcessId(fg, std::ptr::null_mut()) };
            let me = GetCurrentThreadId();
            let target = GetWindowThreadProcessId(h, std::ptr::null_mut());
            let attached_fg = ft != 0 && ft != me && AttachThreadInput(me, ft, 1) != 0;
            let attached_tg = target != me && AttachThreadInput(me, target, 1) != 0;
            BringWindowToTop(h);
            SetForegroundWindow(h);
            SetFocus(h);
            if attached_tg {
                AttachThreadInput(me, target, 0);
            }
            if attached_fg {
                AttachThreadInput(me, ft, 0);
            }
            if GetForegroundWindow() != h {
                SwitchToThisWindow(h, 1);
            }
        }
    }

    pub fn show() {
        let h = main_hwnd();
        if h.is_null() {
            return;
        }
        WANT_VISIBLE.store(true, Ordering::SeqCst);
        LAST_SHOW_MS.store(now_ms(), Ordering::SeqCst);
        compute_anchor();
        place(h, SWP_SHOWWINDOW);
        unsafe {
            // eşzamansız: arayüz iş parçacığı meşgulse bile kısayol iş parçacığı bloklanmaz
            ShowWindowAsync(h, SW_SHOW);
            // ön plana almadan önce pencerenin gerçekten görünür olmasını kısa süre bekle
            for _ in 0..20 {
                if IsWindowVisible(h) != 0 {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        foreground(h);
        SHOWN.store(true, Ordering::SeqCst);
        SHOW_AT.store(now_ms().max(1), Ordering::SeqCst);
        // ön plan kilidi ilk denemede reddederse 50 ms aralıkla birkaç kez daha dene
        FG_RETRIES.store(8, Ordering::SeqCst);
        let t = TRAY.load(Ordering::SeqCst);
        if t != 0 {
            unsafe {
                SetTimer(t as HWND, TIMER_FOREGROUND, 50, None);
            }
        }
        repaint();
    }

    pub fn hide() {
        WANT_VISIBLE.store(false, Ordering::SeqCst);
        SHOW_AT.store(0, Ordering::SeqCst);
        FG_RETRIES.store(0, Ordering::SeqCst);
        HIDE_EPOCH.fetch_add(1, Ordering::SeqCst);
        let h = main_hwnd();
        if !h.is_null() {
            unsafe {
                ShowWindowAsync(h, SW_HIDE);
            }
        }
    }

    /// Gerçek durumdan karar ver: görünür VE ön plandaysa gizle, aksi halde göster + odakla.
    pub fn toggle() {
        let h = main_hwnd();
        if h.is_null() {
            return;
        }
        if is_visible() && unsafe { GetForegroundWindow() } == h {
            hide();
        } else {
            show();
        }
    }

    /// Arayüzün "odak kaybı" gizlemesi: yalnızca gerçekten odak kaybedildiyse
    /// (ön planda başka pencere var) ve son göstermeden 500 ms geçtiyse.
    pub fn hide_if_unfocused() {
        let h = main_hwnd();
        if h.is_null() || !WANT_VISIBLE.load(Ordering::SeqCst) {
            return;
        }
        if now_ms().saturating_sub(LAST_SHOW_MS.load(Ordering::SeqCst)) < 500 {
            return;
        }
        let fg = unsafe { GetForegroundWindow() };
        if fg == h {
            return;
        }
        // tepsi menüsü gibi kendi pencerelerimiz ön plandaysa gizleme
        if !fg.is_null() {
            let mut pid = 0u32;
            unsafe { GetWindowThreadProcessId(fg, &mut pid) };
            if pid == unsafe { GetCurrentProcessId() } {
                return;
            }
        }
        hide();
    }

    fn foreground_retry(t: HWND) {
        let h = main_hwnd();
        let left = FG_RETRIES.fetch_sub(1, Ordering::SeqCst);
        if left <= 0 || h.is_null() || !WANT_VISIBLE.load(Ordering::SeqCst) || unsafe { GetForegroundWindow() } == h {
            FG_RETRIES.store(0, Ordering::SeqCst);
            unsafe { KillTimer(t, TIMER_FOREGROUND) };
            return;
        }
        if unsafe { IsWindowVisible(h) } == 0 {
            unsafe { ShowWindowAsync(h, SW_SHOW) };
        }
        foreground(h);
    }

    /// Pencere boyutunu (fiziksel piksel) ayarla; üst-orta çapa sabit kalır.
    pub fn resize(w: i32, h: i32) {
        if W.swap(w, Ordering::SeqCst) == w && H.swap(h, Ordering::SeqCst) == h {
            return;
        }
        let hw = main_hwnd();
        if !hw.is_null() {
            place(hw, SWP_NOACTIVATE);
        }
    }

    fn make_icon() -> HICON {
        // 32x32 BGRA: koyu yuvarlak kare + açık mavi büyüteç
        const S: i32 = 32;
        let mut px = vec![0u8; (S * S * 4) as usize];
        for y in 0..S {
            for x in 0..S {
                let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                // yuvarlak kare maskesi
                let r = 7.0f32;
                let cx = fx.clamp(r, S as f32 - r);
                let cy = fy.clamp(r, S as f32 - r);
                let d = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
                let a = (r + 0.5 - d).clamp(0.0, 1.0);
                let mut c = [0x2a as f32, 0x22 as f32, 0x1c as f32]; // BGR koyu
                // büyüteç halkası
                let dr = ((fx - 13.5).powi(2) + (fy - 13.5).powi(2)).sqrt();
                let ring = (2.2 - (dr - 7.0).abs()).clamp(0.0, 1.0);
                // sap
                let t = ((fx - 18.5) + (fy - 18.5)) / 2.0;
                let along = t.clamp(0.0, 7.0);
                let (qx, qy) = (18.5 + along, 18.5 + along);
                let dh = ((fx - qx).powi(2) + (fy - qy).powi(2)).sqrt();
                let handle = (2.4 - dh).clamp(0.0, 1.0);
                let m = ring.max(handle);
                let acc = [0xe0 as f32, 0xb0 as f32, 0x7e as f32];
                for k in 0..3 {
                    c[k] = c[k] * (1.0 - m) + acc[k] * m;
                }
                let i = ((y * S + x) * 4) as usize;
                px[i] = (c[0] * a) as u8;
                px[i + 1] = (c[1] * a) as u8;
                px[i + 2] = (c[2] * a) as u8;
                px[i + 3] = (a * 255.0) as u8;
            }
        }
        let and = vec![0u8; (S * S / 8) as usize];
        unsafe { CreateIcon(GetModuleHandleW(std::ptr::null()), S, S, 1, 32, and.as_ptr(), px.as_ptr()) }
    }

    /// Tepsi simgesi eklendi mi (Windows açılışında görev çubuğu henüz hazır olmayabilir: yeniden denenir).
    static TRAY_OK: AtomicBool = AtomicBool::new(false);
    /// Kısayol kaydı geçici olarak bırakıldı mı (ayarlarda kayıt sırasında).
    static HK_SUSPENDED: AtomicBool = AtomicBool::new(false);
    /// Açılışta başarısız kısayol/tepsi için yeniden deneme sayacı.
    static RETRY_TICKS: AtomicU32 = AtomicU32::new(0);

    fn add_tray(hwnd: HWND) -> bool {
        unsafe {
            let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = hwnd;
            nid.uID = 1;
            nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
            nid.uCallbackMessage = WM_TRAY;
            nid.hIcon = make_icon();
            let tip = wide(&format!("Fastplorer · {}", hotkey_label()));
            for (i, c) in tip.iter().take(nid.szTip.len() - 1).enumerate() {
                nid.szTip[i] = *c;
            }
            let ok = Shell_NotifyIconW(NIM_ADD, &nid) != 0;
            TRAY_OK.store(ok, Ordering::SeqCst);
            ok
        }
    }

    /// Çıkmadan önce tepsi simgesini kaldır (hayalet simge kalmasın).
    pub fn remove_tray_now() {
        let t = TRAY.load(Ordering::SeqCst);
        if t != 0 {
            remove_tray(t as HWND);
        }
    }

    fn remove_tray(hwnd: HWND) {
        unsafe {
            let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = hwnd;
            nid.uID = 1;
            Shell_NotifyIconW(NIM_DELETE, &nid);
        }
    }

    fn hotkey_label() -> String {
        crate::config::Hotkey { mods: HK_MODS.load(Ordering::SeqCst), vk: HK_VK.load(Ordering::SeqCst) }.label()
    }

    fn tray_menu(hwnd: HWND) {
        unsafe {
            let m = CreatePopupMenu();
            let a = wide(&tf!("Göster\t{}", "Show\t{}", hotkey_label()));
            let s = wide(t!("Ayarlar", "Settings"));
            let b = wide(t!("Çıkış", "Exit"));
            AppendMenuW(m, MF_STRING, 1, a.as_ptr());
            AppendMenuW(m, MF_STRING, 3, s.as_ptr());
            AppendMenuW(m, MF_SEPARATOR, 0, std::ptr::null());
            AppendMenuW(m, MF_STRING, 2, b.as_ptr());
            let mut pt = POINT { x: 0, y: 0 };
            GetCursorPos(&mut pt);
            SetForegroundWindow(hwnd);
            let cmd = TrackPopupMenu(m, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN, pt.x, pt.y, 0, hwnd, std::ptr::null());
            DestroyMenu(m);
            PostMessageW(hwnd, WM_NULL, 0, 0);
            match cmd {
                1 => show(),
                3 => {
                    OPEN_SETTINGS.store(true, Ordering::SeqCst);
                    show();
                }
                2 => {
                    remove_tray(hwnd);
                    std::process::exit(0);
                }
                _ => {}
            }
        }
    }

    fn register(hwnd: HWND, mods: u32, vk: u32) -> bool {
        unsafe { RegisterHotKey(hwnd, HOTKEY_ID, mods | MOD_NOREPEAT, vk) != 0 }
    }

    unsafe extern "system" fn wndproc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        unsafe {
            match msg {
                WM_HOTKEY => {
                    // çift/tekrarlı WM_HOTKEY'e karşı 250 ms sıçrama önleme (MOD_NOREPEAT ile birlikte)
                    let now = now_ms();
                    if now.saturating_sub(LAST_HOTKEY_MS.load(Ordering::SeqCst)) >= 250 {
                        LAST_HOTKEY_MS.store(now, Ordering::SeqCst);
                        toggle();
                    }
                    0
                }
                WM_TIMER if wp == TIMER_FOREGROUND => {
                    foreground_retry(h);
                    0
                }
                WM_SHOW_REQ => {
                    show();
                    0
                }
                WM_SET_HOTKEY => {
                    // yeni kısayolu dene; olmazsa eskisine dön
                    let (mods, vk) = (wp as u32, lp as u32);
                    UnregisterHotKey(h, HOTKEY_ID);
                    if register(h, mods, vk) {
                        HK_MODS.store(mods, Ordering::SeqCst);
                        HK_VK.store(vk, Ordering::SeqCst);
                        HOTKEY_OK.store(true, Ordering::SeqCst);
                        HK_RESULT.store(1, Ordering::SeqCst);
                    } else {
                        let ok = register(h, HK_MODS.load(Ordering::SeqCst), HK_VK.load(Ordering::SeqCst));
                        HOTKEY_OK.store(ok, Ordering::SeqCst);
                        HK_RESULT.store(2, Ordering::SeqCst);
                    }
                    remove_tray(h);
                    add_tray(h);
                    repaint();
                    0
                }
                WM_SUSPEND_HOTKEY => {
                    HK_SUSPENDED.store(wp != 0, Ordering::SeqCst);
                    if wp != 0 {
                        UnregisterHotKey(h, HOTKEY_ID);
                    } else {
                        UnregisterHotKey(h, HOTKEY_ID);
                        let ok = register(h, HK_MODS.load(Ordering::SeqCst), HK_VK.load(Ordering::SeqCst));
                        HOTKEY_OK.store(ok, Ordering::SeqCst);
                    }
                    0
                }
                WM_TIMER if wp == TIMER_WATCHDOG => {
                    watchdog();
                    0
                }
                WM_TRAY => {
                    match (lp as u32) & 0xffff {
                        WM_LBUTTONUP => toggle(),
                        WM_RBUTTONUP | WM_CONTEXTMENU => tray_menu(h),
                        _ => {}
                    }
                    0
                }
                m if m != 0 && m as i32 == TASKBAR_CREATED.load(Ordering::SeqCst) => {
                    add_tray(h);
                    0
                }
                WM_TIMER if wp == TIMER_RETRY => {
                    startup_retry(h);
                    0
                }
                _ => DefWindowProcW(h, msg, wp, lp),
            }
        }
    }

    /// Bekçi: "göster" istendi ama arayüz kare çizmediyse pencereyi yeniden göster,
    /// geçersiz kıl ve egui döngüsünü uyandır. Kısayol iş parçacığı arayüzden bağımsızdır.
    fn watchdog() {
        let at = SHOW_AT.load(Ordering::SeqCst);
        if at == 0 || !WANT_VISIBLE.load(Ordering::SeqCst) {
            return;
        }
        let now = now_ms();
        if now.saturating_sub(at) < 600 || HEARTBEAT.load(Ordering::Relaxed) >= at {
            return;
        }
        let h = main_hwnd();
        if h.is_null() {
            return;
        }
        unsafe {
            if IsWindowVisible(h) == 0 {
                ShowWindowAsync(h, SW_SHOW);
            }
            InvalidateRect(h, std::ptr::null(), 0);
            PostMessageW(h, WM_NULL, 0, 0);
        }
        repaint();
        foreground(h);
        if now.saturating_sub(at) > 10_000 {
            SHOW_AT.store(0, Ordering::SeqCst); // vazgeç
        }
    }

    /// Windows açılışında kısayol (başka bir uygulama henüz bırakmamış olabilir) ya da tepsi
    /// (görev çubuğu henüz hazır değil) başarısız olduysa 2 sn aralıkla ~5 dk yeniden dene.
    fn startup_retry(h: HWND) {
        let n = RETRY_TICKS.fetch_add(1, Ordering::SeqCst);
        let mut pending = false;
        if !HOTKEY_OK.load(Ordering::SeqCst) && !HK_SUSPENDED.load(Ordering::SeqCst) {
            unsafe { UnregisterHotKey(h, HOTKEY_ID) };
            let ok = register(h, HK_MODS.load(Ordering::SeqCst), HK_VK.load(Ordering::SeqCst));
            HOTKEY_OK.store(ok, Ordering::SeqCst);
            if ok {
                crate::startup::log(format!("kısayol {} kaydedildi ({}. denemede)", hotkey_label(), n + 2));
                repaint();
            }
            pending |= !ok;
        }
        if !TRAY_OK.load(Ordering::SeqCst) {
            let ok = add_tray(h);
            if ok {
                crate::startup::log(format!("tepsi simgesi eklendi ({}. denemede)", n + 2));
            }
            pending |= !ok;
        }
        if !pending || n >= 150 {
            if pending {
                crate::startup::log("kısayol/tepsi denemeleri bırakıldı (5 dk)");
            }
            unsafe { KillTimer(h, TIMER_RETRY) };
        }
    }

    /// Kısayol + tepsi iş parçacığı (egui döngüsünden bağımsız ileti döngüsü).
    pub fn start() {
        std::thread::spawn(|| unsafe {
            let hinst = GetModuleHandleW(std::ptr::null());
            let cls = wide(TRAY_CLASS);
            let mut wc: WNDCLASSW = std::mem::zeroed();
            wc.lpfnWndProc = Some(wndproc);
            wc.hInstance = hinst;
            wc.lpszClassName = cls.as_ptr();
            RegisterClassW(&wc);
            let title = wide("Fastplorer tepsi");
            // Görünmez üst düzey pencere: tepsi bildirimleri ve TaskbarCreated yayını için
            // (HWND_MESSAGE pencereleri yayın iletilerini almaz). Kısayol da buna kaydedilir.
            // Windows açılışında pencere oluşturma geçici olarak başarısız olabilir: birkaç kez dene
            let mut hwnd: HWND = std::ptr::null_mut();
            for k in 0..10 {
                hwnd = CreateWindowExW(WS_EX_TOOLWINDOW, cls.as_ptr(), title.as_ptr(), WS_POPUP, 0, 0, 0, 0, std::ptr::null_mut(), std::ptr::null_mut(), hinst, std::ptr::null());
                if !hwnd.is_null() {
                    break;
                }
                crate::startup::log(format!("tepsi penceresi oluşturulamadı (deneme {}), hata {}", k + 1, GetLastError()));
                std::thread::sleep(std::time::Duration::from_millis(1000));
            }
            if hwnd.is_null() {
                HOTKEY_OK.store(false, Ordering::SeqCst);
                return;
            }
            TRAY.store(hwnd as isize, Ordering::SeqCst);
            let tc = wide("TaskbarCreated");
            TASKBAR_CREATED.store(RegisterWindowMessageW(tc.as_ptr()) as i32, Ordering::SeqCst);
            let ok = register(hwnd, HK_MODS.load(Ordering::SeqCst), HK_VK.load(Ordering::SeqCst));
            HOTKEY_OK.store(ok, Ordering::SeqCst);
            let tray = add_tray(hwnd);
            crate::startup::log(format!("kısayol {}: {} · tepsi: {}", hotkey_label(), if ok { "tamam" } else { "kaydedilemedi, yeniden denenecek" }, if tray { "tamam" } else { "eklenemedi, yeniden denenecek" }));
            if !ok || !tray {
                SetTimer(hwnd, TIMER_RETRY, 2000, None);
            }
            SetTimer(hwnd, TIMER_WATCHDOG, 400, None);
            repaint();
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        });
    }

    pub fn post(msg: u32, wp: usize, lp: isize) -> bool {
        let t = TRAY.load(Ordering::SeqCst);
        t != 0 && unsafe { PostMessageW(t as HWND, msg, wp, lp) } != 0
    }

    pub fn set_hotkey(mods: u32, vk: u32) -> bool {
        HK_RESULT.store(0, Ordering::SeqCst);
        post(WM_SET_HOTKEY, mods as usize, vk as isize)
    }

    pub fn suspend_hotkey(on: bool) {
        post(WM_SUSPEND_HOTKEY, on as usize, 0);
    }

    // ---- Oturum açılışında başlat (HKCU Run) ----
    const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    const APPROVED_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run";

    /// Run değeri: tırnak içinde tam exe yolu + --hidden.
    pub fn run_value() -> Option<String> {
        let exe = std::env::current_exe().ok()?;
        Some(super::autostart_command(&exe.to_string_lossy()))
    }

    fn read_value(key: &str, name: &str) -> Option<Vec<u8>> {
        use windows_sys::Win32::System::Registry::*;
        unsafe {
            let (k, n) = (wide(key), wide(name));
            let mut size: u32 = 0;
            if RegGetValueW(HKEY_CURRENT_USER, k.as_ptr(), n.as_ptr(), RRF_RT_ANY, std::ptr::null_mut(), std::ptr::null_mut(), &mut size) != 0 || size == 0 {
                return None;
            }
            let mut buf = vec![0u8; size as usize];
            if RegGetValueW(HKEY_CURRENT_USER, k.as_ptr(), n.as_ptr(), RRF_RT_ANY, std::ptr::null_mut(), buf.as_mut_ptr() as *mut _, &mut size) != 0 {
                return None;
            }
            buf.truncate(size as usize);
            Some(buf)
        }
    }

    fn utf16_bytes_to_string(b: &[u8]) -> String {
        let w: Vec<u16> = b.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).take_while(|&c| c != 0).collect();
        String::from_utf16_lossy(&w)
    }

    /// `explicit`: kullanıcı ayarlardan açtı (Görev Yöneticisi'ndeki "devre dışı" işareti de kaldırılır).
    pub fn set_autostart(on: bool, explicit: bool) -> bool {
        use windows_sys::Win32::System::Registry::*;
        unsafe {
            let key = wide(RUN_KEY);
            let name = wide("Fastplorer");
            let mut hk: HKEY = std::ptr::null_mut();
            if RegCreateKeyExW(HKEY_CURRENT_USER, key.as_ptr(), 0, std::ptr::null(), 0, KEY_SET_VALUE | KEY_QUERY_VALUE, std::ptr::null(), &mut hk, std::ptr::null_mut()) != 0 {
                crate::startup::log("başlangıç kaydı açılamadı (HKCU Run)");
                return false;
            }
            // eski ad ("bul") kaydını her durumda kaldır
            let old = wide("bul");
            RegDeleteValueW(hk, old.as_ptr());
            if on {
                let Some(val) = run_value() else {
                    RegCloseKey(hk);
                    return false;
                };
                let v = wide(&val);
                let ok = RegSetValueExW(hk, name.as_ptr(), 0, REG_SZ, v.as_ptr() as *const u8, (v.len() * 2) as u32) == 0;
                RegCloseKey(hk);
                // geri oku ve doğrula
                let back = read_value(RUN_KEY, "Fastplorer").map(|b| utf16_bytes_to_string(&b));
                let verified = back.as_deref() == Some(val.as_str());
                if explicit {
                    clear_startup_disabled();
                }
                let disabled = startup_disabled();
                crate::startup::log(format!(
                    "başlangıç kaydı: {} · doğrulama: {}{}",
                    val,
                    if verified { "tamam" } else { "UYUŞMUYOR" },
                    if disabled { " · DİKKAT: Görev Yöneticisi > Başlangıç uygulamaları'nda devre dışı" } else { "" }
                ));
                ok && verified
            } else {
                let e = RegDeleteValueW(hk, name.as_ptr());
                RegCloseKey(hk);
                crate::startup::log("başlangıç kaydı kaldırıldı");
                e == 0 || e == ERROR_FILE_NOT_FOUND
            }
        }
    }

    /// Görev Yöneticisi > Başlangıç uygulamaları'nda devre dışı mı? (StartupApproved ilk bayt tek sayı = kapalı)
    pub fn startup_disabled() -> bool {
        read_value(APPROVED_KEY, "Fastplorer").and_then(|b| b.first().copied()).is_some_and(|b| b & 1 == 1)
    }

    fn clear_startup_disabled() {
        use windows_sys::Win32::System::Registry::*;
        if !startup_disabled() {
            return;
        }
        unsafe {
            let key = wide(APPROVED_KEY);
            let name = wide("Fastplorer");
            let mut hk: HKEY = std::ptr::null_mut();
            if RegOpenKeyExW(HKEY_CURRENT_USER, key.as_ptr(), 0, KEY_SET_VALUE, &mut hk) == 0 {
                RegDeleteValueW(hk, name.as_ptr());
                RegCloseKey(hk);
                crate::startup::log("Görev Yöneticisi'ndeki \"devre dışı\" işareti kaldırıldı (kullanıcı ayarlardan açtı)");
            }
        }
    }

    // ---- Geri Dönüşüm Kutusu ----
    /// Yalnızca Geri Dönüşüm Kutusu'na gönderir; kalıcı silme gerektiren sürücülerde reddeder.
    pub fn recycle(paths: &[String]) -> Result<(), String> {
        use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
        for p in paths {
            let root: String = {
                let pb = std::path::Path::new(p);
                match pb.components().next() {
                    Some(std::path::Component::Prefix(pr)) => format!("{}\\", pr.as_os_str().to_string_lossy()),
                    _ => return Err(tf!("Desteklenmeyen yol: {p}", "Unsupported path: {p}")),
                }
            };
            let w = wide(&root);
            // 3 = DRIVE_FIXED: Geri Dönüşüm Kutusu olan tek güvenli tür
            if unsafe { GetDriveTypeW(w.as_ptr()) } != 3 {
                return Err(t!("Bu sürücüde Geri Dönüşüm Kutusu yok; güvenlik için silinmedi", "This drive has no Recycle Bin; not deleted for safety").into());
            }
        }
        // çift sıfır sonlu liste
        let mut from: Vec<u16> = Vec::new();
        for p in paths {
            from.extend(p.encode_utf16());
            from.push(0);
        }
        from.push(0);
        unsafe {
            let mut op: SHFILEOPSTRUCTW = std::mem::zeroed();
            op.wFunc = FO_DELETE;
            op.pFrom = from.as_ptr();
            // FOF_ALLOWUNDO: Geri Dönüşüm Kutusu. FOF_WANTNUKEWARNING: kalıcı silinecekse uyar (onaysız kalıcı silme yok).
            op.fFlags = (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_WANTNUKEWARNING) as u16;
            let r = SHFileOperationW(&mut op);
            if r != 0 {
                return Err(tf!("Geri Dönüşüm Kutusu'na gönderilemedi (kod {r})", "Could not move to the Recycle Bin (code {r})"));
            }
            if op.fAnyOperationsAborted != 0 {
                return Err(t!("İşlem iptal edildi", "Operation cancelled").into());
            }
        }
        Ok(())
    }
}

/// Windows açılışı için Run kaydı değeri: boşluklu yollar için tırnaklı tam exe yolu + --hidden.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn autostart_command(exe: &str) -> String {
    format!("\"{}\" --hidden", exe.trim_matches('"'))
}

/// Kısayol + tepsi iş parçacığını başlat (bir kez). Arayüzden bağımsızdır: `--hidden` ile
/// açılışta pencere hiç çizilmese de F9 ve tepsi simgesi çalışır.
pub fn start_background(hotkey: crate::config::Hotkey) {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    HK_MODS.store(hotkey.mods, Ordering::SeqCst);
    HK_VK.store(hotkey.vk, Ordering::SeqCst);
    #[cfg(windows)]
    win::start();
}

/// egui bağlamını kaydet (arka plan iş parçacıklarından yeniden çizim isteği için).
pub fn set_ctx(ctx: &egui::Context) {
    let _ = CTX.set(ctx.clone());
}

/// İlk karede çağrılır (arka plan zaten başladıysa yalnızca bağlamı kaydeder).
pub fn init(ctx: &egui::Context, hotkey: crate::config::Hotkey) {
    set_ctx(ctx);
    start_background(hotkey);
}

/// Çıkmadan önce (ör. yeniden başlatma) tepsi simgesini kaldır.
pub fn shutdown() {
    #[cfg(windows)]
    win::remove_tray_now();
}

/// `signal`: zaten çalışan örneğe "göster" denilsin mi (--hidden ile açılışta hayır).
/// `wait`: yeniden başlatma denemesi; önceki örneğin kapanması beklenir.
pub fn single_instance(signal: bool, wait: bool) -> bool {
    #[cfg(windows)]
    {
        win::single_instance(signal, wait)
    }
    #[cfg(not(windows))]
    {
        let _ = (signal, wait);
        true
    }
}

pub fn hide(ctx: &egui::Context) {
    #[cfg(windows)]
    {
        let _ = ctx;
        win::hide();
    }
    #[cfg(not(windows))]
    {
        // Linux'ta (yalnızca deneme): BUL_NOHIDE ile gizlemeyi kapat
        if std::env::var_os("BUL_NOHIDE").is_none() {
            HIDE_EPOCH.fetch_add(1, Ordering::SeqCst);
            ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Minimized(true));
        }
    }
}

/// Odak kaybında gizle (Windows'ta gerçek ön plan durumu doğrulanır).
pub fn hide_if_unfocused(ctx: &egui::Context) {
    #[cfg(windows)]
    {
        let _ = ctx;
        win::hide_if_unfocused();
    }
    #[cfg(not(windows))]
    hide(ctx);
}

pub fn show() {
    #[cfg(windows)]
    win::show();
    #[cfg(not(windows))]
    {
        SHOWN.store(true, Ordering::SeqCst);
        repaint();
    }
}

/// Pencere boyutu (nokta cinsinden) değiştiğinde. Pencere tam olarak kutu boyutundadır.
pub fn resize(ctx: &egui::Context, size: egui::Vec2) {
    #[cfg(windows)]
    {
        let ppp = ctx.pixels_per_point();
        win::resize((size.x * ppp).round() as i32, (size.y * ppp).round() as i32);
    }
    #[cfg(not(windows))]
    ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::InnerSize(size));
}

/// Yeni genel kısayolu kaydetmeyi dene (sonuç `hotkey_result` ile okunur).
pub fn set_hotkey(h: crate::config::Hotkey) {
    #[cfg(windows)]
    {
        if !win::set_hotkey(h.mods, h.vk) {
            HK_RESULT.store(2, Ordering::SeqCst);
        }
    }
    #[cfg(not(windows))]
    {
        HK_MODS.store(h.mods, Ordering::SeqCst);
        HK_VK.store(h.vk, Ordering::SeqCst);
        HK_RESULT.store(1, Ordering::SeqCst);
    }
}

/// None = bekliyor, Some(true) = kaydedildi, Some(false) = kullanılamıyor.
pub fn hotkey_result() -> Option<bool> {
    match HK_RESULT.load(Ordering::SeqCst) {
        0 => None,
        1 => Some(true),
        _ => Some(false),
    }
}

/// Kısayol kaydedilirken genel kısayolu geçici olarak bırak (aynı tuşa basılabilsin).
pub fn suspend_hotkey(on: bool) {
    #[cfg(windows)]
    win::suspend_hotkey(on);
    #[cfg(not(windows))]
    let _ = on;
}

/// `explicit`: kullanıcı ayarlardan değiştirdi (her açılıştaki yol tazelemesi değil).
pub fn set_autostart(on: bool, explicit: bool) -> bool {
    #[cfg(windows)]
    {
        win::set_autostart(on, explicit)
    }
    #[cfg(not(windows))]
    {
        let _ = (on, explicit);
        true
    }
}


/// Öğeleri Geri Dönüşüm Kutusu'na gönder (asla kalıcı silme).
pub fn recycle(paths: &[String]) -> Result<(), String> {
    #[cfg(windows)]
    {
        win::recycle(paths)
    }
    #[cfg(not(windows))]
    {
        // Linux (deneme): freedesktop çöp kutusuna taşı
        let base = std::env::var_os("XDG_DATA_HOME").map(std::path::PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/share"))).ok_or(t!("Çöp kutusu bulunamadı", "Trash not found"))?;
        let files = base.join("Trash/files");
        std::fs::create_dir_all(&files).map_err(|e| e.to_string())?;
        for p in paths {
            let name = std::path::Path::new(p).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let t = crate::ops::unique_name(&files, &name);
            std::fs::rename(p, &t).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

/// Yerel sabit sürücü mü (Geri Dönüşüm Kutusu taraması için; ağ/USB/CD atlanır).
#[cfg(windows)]
pub fn is_fixed_drive(letter: char) -> bool {
    use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
    const DRIVE_FIXED: u32 = 3;
    let root: Vec<u16> = format!("{letter}:\\").encode_utf16().chain([0]).collect();
    unsafe { GetDriveTypeW(root.as_ptr()) == DRIVE_FIXED }
}

/// Oturumdaki kullanıcının SID'i ("S-1-5-21-…"). Geri Dönüşüm Kutusu'nda yalnızca bu
/// kullanıcının klasörü (`X:\$Recycle.Bin\<SID>`) okunur.
#[cfg(windows)]
pub fn current_user_sid() -> Option<String> {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree};
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    unsafe {
        let mut tok: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut tok) == 0 {
            return None;
        }
        let mut need = 0u32;
        GetTokenInformation(tok, TokenUser, std::ptr::null_mut(), 0, &mut need);
        // u64 tampon: TOKEN_USER hizalaması için
        let mut buf = vec![0u64; (need as usize).div_ceil(8).max(1)];
        let ok = GetTokenInformation(tok, TokenUser, buf.as_mut_ptr().cast(), (buf.len() * 8) as u32, &mut need);
        CloseHandle(tok);
        if ok == 0 {
            return None;
        }
        let tu = &*(buf.as_ptr() as *const TOKEN_USER);
        let mut w: *mut u16 = std::ptr::null_mut();
        if ConvertSidToStringSidW(tu.User.Sid, &mut w) == 0 || w.is_null() {
            return None;
        }
        let mut n = 0;
        while *w.add(n) != 0 {
            n += 1;
        }
        let s = String::from_utf16_lossy(std::slice::from_raw_parts(w, n));
        LocalFree(w.cast());
        s.starts_with("S-").then_some(s)
    }
}

/// Masaüstü klasörü (OneDrive'a taşınmış ya da yerelleştirilmiş olsa da doğru yol).
pub fn desktop_dir() -> Option<String> {
    if let Some(v) = std::env::var_os("BUL_DESKTOP") {
        return Some(v.to_string_lossy().to_string());
    }
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Com::CoTaskMemFree;
        use windows_sys::Win32::UI::Shell::{FOLDERID_Desktop, SHGetKnownFolderPath};
        let mut w: *mut u16 = std::ptr::null_mut();
        let hr = SHGetKnownFolderPath(&FOLDERID_Desktop, 0, std::ptr::null_mut(), &mut w);
        let out = if hr >= 0 && !w.is_null() {
            let mut n = 0;
            while *w.add(n) != 0 {
                n += 1;
            }
            Some(String::from_utf16_lossy(std::slice::from_raw_parts(w, n)))
        } else {
            None
        };
        if !w.is_null() {
            CoTaskMemFree(w as *const _);
        }
        out.or_else(|| std::env::var_os("USERPROFILE").map(|h| std::path::Path::new(&h).join("Desktop").to_string_lossy().to_string()))
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join("Desktop").to_string_lossy().to_string())
    }
}

/// Taşı ama hedefte bir şey varsa asla üzerine yazma (Windows: MoveFileExW bayraksız).
pub fn move_no_replace(from: &std::path::Path, to: &std::path::Path) -> Result<(), String> {
    if std::fs::symlink_metadata(to).is_ok() {
        return Err(t!("hedefte aynı adlı bir öğe var", "an item with the same name exists at the destination").into());
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Storage::FileSystem::MoveFileExW;
        let w = |p: &std::path::Path| -> Vec<u16> { p.as_os_str().to_string_lossy().encode_utf16().chain([0]).collect() };
        let (a, b) = (w(from), w(to));
        // bayrak 0: MOVEFILE_REPLACE_EXISTING yok → hedef varsa başarısız olur
        if unsafe { MoveFileExW(a.as_ptr(), b.as_ptr(), 0) } == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        std::fs::rename(from, to).map_err(|e| e.to_string())
    }
}

/// Gezgin'de Geri Dönüşüm Kutusu'nu aç.
pub fn open_recycle_bin_in_explorer() -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer.exe").arg("shell:RecycleBinFolder").creation_flags(0x0800_0000).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        Err(t!("Yalnızca Windows'ta", "Windows only").into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn autostart_value_is_quoted() {
        assert_eq!(autostart_command("C:\\Users\\Ali Veli\\Downloads\\Fastplorer.exe"), "\"C:\\Users\\Ali Veli\\Downloads\\Fastplorer.exe\" --hidden");
        assert_eq!(autostart_command("\"C:\\a.exe\""), "\"C:\\a.exe\" --hidden");
    }
}
