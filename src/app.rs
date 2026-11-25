//! Başlatıcı penceresi: durum, klavye/fare, işlemler.

use crate::index::Indexer;
use crate::preview::{self, Preview, PreviewReq};
use crate::recent::{self, State};
use crate::search::{self, Item, Kind, Req, Res};
use crate::config::{self, Config, Hotkey};
use crate::selection::Selection;
use crate::terminal::{self, Term};
use crate::{bin, notes, ops, platform, util};
use eframe::egui::{self, Context, Event, Key, Modifiers, Pos2, TextureHandle, Vec2};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::Arc;
use std::time::{Duration, Instant};

// ---- Ölçüler (nokta) ----
pub const PANEL_W: f32 = 700.0;
pub const PREVIEW_W: f32 = 340.0;
/// Pencere tam olarak kutu boyutundadır: saydam kenar boşluğu yok.
pub const MARGIN: f32 = 0.0;
pub const MARGIN_BOTTOM: f32 = 0.0;
pub const SEARCH_H: f32 = 56.0;
pub const STRIP_H: f32 = 31.0;
pub const ROW_H: f32 = 52.0;
pub const LIST_PAD_T: f32 = 4.0;
pub const LIST_PAD_B: f32 = 2.0;
pub const FOOTER_H: f32 = 34.0;
pub const MAX_ROWS: usize = 8;
/// Ayarlar satırları: kısayol, Windows açılışında başlat, terminal.
pub const SETTINGS_ROWS: usize = 3;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Act {
    Open,
    /// Arama modu: İsim → Uzantı → İçerik → İsim (F1 / mod çipi).
    CycleMode,
    /// Geri Dönüşüm Kutusu: seçili öğeyi özgün yerine geri yükle.
    Restore,
    /// Geri Dönüşüm Kutusu: öğenin özgün klasörünü aç.
    OrigReveal,
    /// Geri Dönüşüm Kutusu: özgün yolu panoya kopyala.
    OrigPath,
    /// Geri Dönüşüm Kutusu'nu Gezgin'de aç.
    TrashExplorer,
    /// Dizini yeniden tara (F5).
    Refresh,
    /// Çoklu seçimde tümünü seç (Ctrl+A).
    SelectAll,
    Reveal,
    Preview,
    Copy,
    Cut,
    Paste,
    Rename,
    Bin,
    CopyPath,
    Dests,
    BinMgr,
    BinRename,
    BinRemove,
    AddPath,
    New,
    Delete,
    ClearClip,
    ExtFilter,
    Settings,
    /// Çoklu seçim modunu aç/kapat.
    Multi,
    /// Çoklu seçimde işaretlenenlerin hepsini aç.
    OpenAll,
    /// Geçerli klasöre not al.
    Note,
    /// Geçerli klasörde terminal aç.
    Terminal,
}

#[derive(Clone)]
pub struct MItem {
    pub key: char,
    pub label: &'static str,
    pub short: &'static str,
    pub act: Act,
}

#[derive(Clone)]
pub enum MEntry {
    Item(MItem),
    Sep,
}

pub struct Menu {
    pub entries: Vec<MEntry>,
    pub sel: usize,
    pub pos: Pos2,
    pub context: bool,
    /// Menü pencereye sığmadığında görünen ilk girdi (yalnızca tam satırlar çizilir).
    pub scroll: usize,
}

impl Menu {
    fn step(&mut self, d: isize) {
        let n = self.entries.len() as isize;
        let mut i = self.sel as isize;
        for _ in 0..n {
            i = (i + d).rem_euclid(n);
            if matches!(self.entries[i as usize], MEntry::Item(_)) {
                self.sel = i as usize;
                return;
            }
        }
    }
    pub fn item(&self, i: usize) -> Option<&MItem> {
        match self.entries.get(i) {
            Some(MEntry::Item(m)) => Some(m),
            _ => None,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum Mode {
    Search,
    /// Son hedefler listesi (son yapıştırılan klasörler).
    Dests,
    /// Bin yöneticisi.
    Bin,
    /// Ayarlar paneli (kısayol, oturum açılışında başlat).
    Settings,
    /// Uzantı süzgeci yazılıyor (arama kutusunda); `prev` = önceki sorgu.
    ExtInput { prev: String },
    /// Not al: yazılan metin `dir` klasörüne .txt olarak kaydedilir.
    Note { dir: String },
    /// Geri Dönüşüm Kutusu (yalnızca bu kullanıcının silinen öğeleri; gezilebilir, aranmaz).
    Trash,
}

/// Esc'nin o anki durumda ne yapacağı (öncelik sırasıyla; ilk eşleşen uygulanır).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EscAction {
    CloseMenu,
    CancelEdit,
    CancelConfirm,
    LeaveCrumbs,
    LeaveNote,
    ClosePreview,
    LeaveMulti,
    ClearMarks,
    LeaveMode,
    LeaveExtInput,
    ContentOff,
    ExtOff,
    LeaveBrowse,
    Hide,
}

/// Esc kararı için durum özeti.
#[derive(Clone, Copy, Default, Debug)]
pub struct EscState {
    pub menu: bool,
    pub edit: bool,
    pub confirm: bool,
    pub crumbs: bool,
    pub note: bool,
    pub preview: bool,
    pub multi: bool,
    pub marked: bool,
    /// Son hedefler / Bin / Ayarlar.
    pub sub_mode: bool,
    pub ext_input: bool,
    pub content: bool,
    pub ext: bool,
    pub browse: bool,
}

/// Esc: önce açık katman/mod kapanır; hiçbir mod yoksa pencere gizlenir.
pub fn esc_action(s: EscState) -> EscAction {
    if s.menu {
        EscAction::CloseMenu
    } else if s.edit {
        EscAction::CancelEdit
    } else if s.confirm {
        EscAction::CancelConfirm
    } else if s.crumbs {
        EscAction::LeaveCrumbs
    } else if s.note {
        EscAction::LeaveNote
    } else if s.ext_input {
        EscAction::LeaveExtInput
    } else if s.preview {
        EscAction::ClosePreview
    } else if s.multi {
        EscAction::LeaveMulti
    } else if s.marked {
        EscAction::ClearMarks
    } else if s.sub_mode {
        EscAction::LeaveMode
    } else if s.content {
        EscAction::ContentOff
    } else if s.ext {
        EscAction::ExtOff
    } else if s.browse {
        EscAction::LeaveBrowse
    } else {
        EscAction::Hide
    }
}

pub enum EditKind {
    Rename { path: String },
    BinAlias { target: String },
    BinRename { old: String },
    /// Yeni dosya/klasör (geçerli klasörde).
    New { dir: String },
}

pub struct Edit {
    pub kind: EditKind,
    pub text: String,
    pub row: usize,
    pub select: Option<(usize, usize)>,
}

pub enum CKind {
    RenameOverwrite { from: String, to: String },
    Transfer { srcs: Vec<String>, dest: String, cut: bool },
    BinRemove { alias: String },
    BinOverwrite { alias: String, target: String },
    AddPath,
    Delete { paths: Vec<String> },
    NewExists { path: String },
}

pub struct Confirm {
    pub title: String,
    pub lines: Vec<String>,
    pub buttons: Vec<&'static str>,
    pub sel: usize,
    pub kind: CKind,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ClipStrip {
    Hidden,
    Shown,
    Pasted(Instant),
}

pub enum Thumb {
    Pending,
    Ready(TextureHandle),
    Failed,
}

struct OpDone {
    msg: String,
    ok: bool,
    dest: Option<String>,
    first: Option<String>,
}

type ThumbMsg = (String, Option<(usize, usize, Vec<u8>)>);

/// Son hedefler / Bin moduna girmeden önceki görünüm (Esc ile geri dönmek için).
struct Saved {
    browse: Option<String>,
    query: String,
    sel: usize,
}

pub struct App {
    pub idx: Arc<Indexer>,
    pub state: State,
    pub query: String,
    last_query: String,
    pub mode: Mode,
    pub browse: Option<String>,
    pub items: Vec<Item>,
    pub sel: usize,
    pub scroll: usize,
    gen_: u64,
    latest: Arc<AtomicU64>,
    stx: Sender<Req>,
    srx: Receiver<Res>,
    waiting: bool,
    reselect: Option<String>,
    shown_query: String,
    idx_version: usize,
    win_recent: (Instant, Vec<(String, i64)>),
    // önizleme
    pub preview_open: bool,
    ptx: Sender<PreviewReq>,
    prx: Receiver<Preview>,
    pub preview: Option<Preview>,
    pub preview_tex: Option<TextureHandle>,
    pkey: u64,
    pub preview_item: Option<Item>,
    pub preview_scroll: f32,
    // küçük resimler
    pub thumbs: HashMap<String, Thumb>,
    ttx: Sender<String>,
    trx: Receiver<ThumbMsg>,
    // katmanlar
    pub menu: Option<Menu>,
    pub confirm: Option<Confirm>,
    pub edit: Option<Edit>,
    pub toast: Option<(String, bool, Instant)>,
    clip: Option<(Vec<String>, bool)>,
    /// Pano şeridi: gizli / "Kopyalandı|Kesildi" / "Yapıştırıldı" (zamanı).
    clip_strip: ClipStrip,
    otx: Sender<OpDone>,
    orx: Receiver<OpDone>,
    busy_op: bool,
    saved: Option<Saved>,
    // pencere
    inited: bool,
    start_hidden: bool,
    was_focused: Option<bool>,
    pub last_size: Vec2,
    pub last_ppp: f32,
    pub wheel_acc: f32,
    pub focus_query: bool,
    pub hover_row: Option<usize>,
    // ayarlar
    pub config: Config,
    /// Geri Dönüşüm Kutusu görünümündeki kayıtlar (en son silinen önce).
    pub trash: Vec<crate::recycle::Entry>,
    /// Masaüstü klasörü (sanal Geri Dönüşüm Kutusu öğesi burada da görünür).
    pub desktop: String,
    pub settings_sel: usize,
    /// Kısayol kaydı: tuş bekleniyor.
    pub recording: bool,
    /// Kayıt sonucu bekleniyor (kısayol iş parçacığı).
    hk_pending: Option<Hotkey>,
    // çoklu seçim (yollar), Shift aralık çapası (yol: liste yenilense de kaymaz) ve Ctrl+Shift tabanı
    pub marked: HashSet<String>,
    anchor_path: Option<String>,
    base: HashSet<String>,
    /// Çoklu seçim modu (Tab menüsünden): Boşluk/tık işaretler, Ctrl+A tümünü seçer.
    pub multi: bool,
    /// Not al metni ve Esc ile çıkılınca saklanan taslak.
    pub note_text: String,
    note_draft: String,
    /// Ayarlar: "Başka bir program" yolu yazılıyor.
    pub settings_edit: Option<String>,
    /// Önceki karedeki fare konumu (üzerine gelme seçimi yalnızca fare gerçekten hareket edince).
    pub last_pointer: Option<Pos2>,
    /// Menüyü kapatan tuştan sonra aynı karede gelen tuşlar: sonraki karede işlenir.
    key_carry: Vec<Event>,
    /// Son görülen gizleme sayacı (ana pencere gizlenince menü kapanır).
    hide_epoch_seen: u64,
    /// Uzantı süzgeci (noktasız, katlanmış), arama çubuğundaki "pdf ✕" çipi.
    pub ext: Option<String>,
    /// İçerik araması açık (">" önekiyle de olur).
    pub content: bool,
    /// Yol şeridi klavye modu: vurgulanan bölüm (0 = "Tümü").
    pub crumb_sel: Option<usize>,
    /// Son gösterilme zamanı (odak kaybıyla gizleme için kısa bekleme).
    shown_at: Instant,
    /// Odak, gösterildikten sonra en az bir kez alındı mı?
    got_focus: bool,
    /// --hidden ile açılışta eframe'in ilk karede gösterdiği pencereyi gizlemek için.
    hide_frames: u8,
    /// Tab menüsünde Shift'in önceki karedeki durumu (basılma anını yakalamak için).
    menu_shift_prev: bool,
}

/// "Bin'den kaldırılsın mı?" penceresi: Kaldır seçili gelir (Enter onaylar, ←/→ değiştirir, Esc vazgeçer).
pub fn bin_remove_confirm(alias: String) -> Confirm {
    Confirm {
        title: "Bin'den kaldırılsın mı?".into(),
        lines: vec![format!("\"{alias}\" kısayolu kaldırılacak."), "Hedef dosyanın kendisi silinmez.".into()],
        buttons: vec!["Kaldır", "Vazgeç"],
        sel: 0,
        kind: CKind::BinRemove { alias },
    }
}

/// Yeni öğe adı: uzantı varsa dosya, yoksa klasör. (".gitignore" gibi nokta dosyaları da dosyadır.)
pub fn new_item_is_file(name: &str) -> bool {
    let n = name.trim();
    match n.rsplit_once('.') {
        Some((stem, ext)) => !ext.is_empty() && !ext.contains(' ') && (!stem.is_empty() || n.starts_with('.')),
        None => false,
    }
}

/// Yol şeridi bölümleri: (etiket, yol). İlk öğe "Tümü" (yol None = genel arama).
pub fn crumb_segments(browse: &str) -> Vec<(String, Option<String>)> {
    let mut segs: Vec<(String, Option<String>)> = Vec::new();
    for a in Path::new(browse).ancestors() {
        let label = a.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| a.to_string_lossy().trim_end_matches(['\\', '/']).to_string());
        let label = if label.is_empty() { a.to_string_lossy().to_string() } else { label };
        segs.push((label, Some(a.to_string_lossy().to_string())));
    }
    segs.push(("Tümü".into(), None));
    segs.reverse();
    segs
}

pub fn file_name(p: &str) -> String {
    Path::new(p).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| p.to_string())
}

pub fn parent_of(p: &str) -> Option<String> {
    Path::new(p).parent().map(|x| x.to_string_lossy().to_string()).filter(|s| !s.is_empty())
}

fn spawn_thumbs(rx: Receiver<String>, tx: Sender<ThumbMsg>) {
    std::thread::spawn(move || {
        while let Ok(p) = rx.recv() {
            let big = std::fs::metadata(&p).map(|m| m.len() > 60 << 20).unwrap_or(true);
            let r = if big {
                None
            } else {
                image::ImageReader::open(&p)
                    .ok()
                    .and_then(|r| r.with_guessed_format().ok())
                    .and_then(|r| r.decode().ok())
                    .map(|img| {
                        let t = img.resize_to_fill(64, 64, image::imageops::FilterType::Triangle).to_rgba8();
                        (t.width() as usize, t.height() as usize, t.into_raw())
                    })
            };
            if tx.send((p, r)).is_err() {
                return;
            }
        }
    });
}

impl App {
    pub fn new(start_hidden: bool, config: Config) -> App {
        let idx = Arc::new(Indexer::new());
        idx.start(true);
        {
            // Dizini arka planda ara sıra tazele (arayüzde hiçbir iz bırakmaz).
            let idx = idx.clone();
            std::thread::spawn(move || {
                loop {
                    std::thread::sleep(Duration::from_secs(25 * 60));
                    idx.start(true);
                }
            });
        }
        let latest = Arc::new(AtomicU64::new(0));
        let (stx, rrx) = channel();
        let (rtx, srx) = channel();
        search::spawn(idx.index.clone(), latest.clone(), rrx, rtx);
        let (ptx, prx_in) = channel();
        let (ptx_out, prx) = channel();
        preview::spawn(prx_in, ptx_out);
        let (ttx, trx_in) = channel();
        let (ttx_out, trx) = channel();
        spawn_thumbs(trx_in, ttx_out);
        let (otx, orx) = channel();
        let mut app = App {
            idx,
            state: State::load(),
            query: String::new(),
            last_query: String::new(),
            mode: Mode::Search,
            browse: None,
            items: Vec::new(),
            sel: 0,
            scroll: 0,
            gen_: 0,
            latest,
            stx,
            srx,
            waiting: false,
            reselect: None,
            shown_query: String::new(),
            idx_version: 0,
            win_recent: (Instant::now() - Duration::from_secs(3600), Vec::new()),
            preview_open: false,
            ptx,
            prx,
            preview: None,
            preview_tex: None,
            pkey: 0,
            preview_item: None,
            preview_scroll: 0.0,
            thumbs: HashMap::new(),
            ttx,
            trx,
            menu: None,
            confirm: None,
            edit: None,
            toast: None,
            clip: None,
            clip_strip: ClipStrip::Hidden,
            otx,
            orx,
            busy_op: false,
            saved: None,
            inited: false,
            start_hidden,
            was_focused: None,
            last_size: Vec2::ZERO,
            last_ppp: 0.0,
            wheel_acc: 0.0,
            focus_query: true,
            hover_row: None,
            config,
            trash: Vec::new(),
            desktop: platform::desktop_dir().unwrap_or_default(),
            settings_sel: 0,
            recording: false,
            hk_pending: None,
            marked: HashSet::new(),
            anchor_path: None,
            base: HashSet::new(),
            multi: false,
            note_text: String::new(),
            note_draft: String::new(),
            settings_edit: None,
            last_pointer: None,
            key_carry: Vec::new(),
            hide_epoch_seen: platform::hide_epoch(),
            ext: None,
            content: false,
            crumb_sel: None,
            shown_at: Instant::now(),
            got_focus: false,
            hide_frames: if start_hidden { 3 } else { 0 },
            menu_shift_prev: false,
        };
        app.research();
        app
    }

    pub fn toast(&mut self, msg: impl Into<String>, ok: bool) {
        self.toast = Some((msg.into(), ok, Instant::now()));
    }

    pub fn selected(&self) -> Option<&Item> {
        self.items.get(self.sel)
    }

    pub fn has_strip(&self) -> bool {
        self.browse.is_some() || !matches!(self.mode, Mode::Search) || self.clip_info().is_some()
    }

    /// Pano şeridi (şerit için): (ilk ad, adet, fiil). Şerit gizliyse None.
    pub fn clip_info(&self) -> Option<(String, usize, &'static str)> {
        let (v, cut) = self.clip.as_ref()?;
        let verb = match self.clip_strip {
            ClipStrip::Hidden => return None,
            ClipStrip::Shown => {
                if *cut {
                    "Kesildi"
                } else {
                    "Kopyalandı"
                }
            }
            ClipStrip::Pasted(_) => "Yapıştırıldı",
        };
        Some((v.first().map(|p| file_name(p)).unwrap_or_default(), v.len(), verb))
    }

    /// Kopyalanan/kesilen kaynaklar artık yoksa (silindi/taşındı) panodan düşür; hiçbiri kalmadıysa şeridi kaldır.
    pub fn prune_clip(&mut self) {
        if let Some((v, _)) = &mut self.clip {
            v.retain(|p| Path::new(p).exists());
            if v.is_empty() {
                self.clip = None;
            }
        }
        if self.clip.is_none() {
            self.clip_strip = ClipStrip::Hidden;
        }
    }

    /// Şeritteki ✕ / Tab→k: panoyu temizle.
    pub fn clear_clip(&mut self) {
        let Some((ours, _)) = self.clip.take() else {
            self.clip_strip = ClipStrip::Hidden;
            self.toast("Pano zaten boş", true);
            return;
        };
        self.clip_strip = ClipStrip::Hidden;
        // Windows panosunda hâlâ bizim dosyalarımız varsa onu da boşalt (başka içerik varsa dokunma)
        if ops::get_clipboard_files().is_some_and(|(v, _)| v == ours) {
            ops::clear_clipboard();
        }
        self.toast("Pano temizlendi", true);
    }

    /// İçerik araması etkin mi ve aranan metin.
    pub fn content_query(&self) -> Option<String> {
        if let Some(rest) = self.query.strip_prefix('>') {
            return Some(rest.trim().to_string());
        }
        self.content.then(|| self.query.trim().to_string())
    }

    /// Satır vurgusu için sorgu (">" öneki olmadan).
    pub fn match_query(&self) -> String {
        self.content_query().unwrap_or_else(|| if matches!(self.mode, Mode::ExtInput { .. }) { String::new() } else { self.query.clone() })
    }

    pub fn visible_rows(&self) -> usize {
        if self.mode == Mode::Settings {
            return SETTINGS_ROWS;
        }
        if matches!(self.mode, Mode::Note { .. }) {
            return 5;
        }
        let mut n = self.items.len().clamp(1, MAX_ROWS);
        if self.preview_open || self.confirm.is_some() {
            n = n.max(5);
        }
        // menü açılınca pencere büyümez: menü sığmazsa kendi içinde kayar (view::menu_rect)
        n
    }

    fn recent_list(&mut self) -> Vec<(String, i64)> {
        if self.win_recent.0.elapsed() > Duration::from_secs(15) {
            self.win_recent = (Instant::now(), recent::windows_recent());
        }
        let mut v = self.state.history.clone();
        v.extend(self.win_recent.1.iter().cloned());
        v
    }

    /// Sorgu ya da mod değişti: aramayı yeniden başlat.
    pub fn research(&mut self) {
        self.last_query = self.query.clone();
        match self.mode.clone() {
            Mode::Dests | Mode::Bin | Mode::Trash => {
                self.local_list();
                return;
            }
            Mode::Settings | Mode::ExtInput { .. } | Mode::Note { .. } => return,
            Mode::Search => {}
        }
        self.gen_ += 1;
        self.latest.store(self.gen_, Ordering::SeqCst);
        let ext = self.ext.clone();
        let (kind, query, recent) = match self.content_query() {
            Some(q) if q.is_empty() => {
                // içerik modu, boş sorgu: liste boş
                self.waiting = false;
                self.items.clear();
                self.marked.clear();
                return;
            }
            Some(q) => (Kind::Content, q, Vec::new()),
            None => {
                let r = if self.query.trim().is_empty() && self.browse.is_none() { self.recent_list() } else { Vec::new() };
                (Kind::Name, self.query.clone(), r)
            }
        };
        let _ = self.stx.send(Req { generation: self.gen_, query, tab: 0, kind, recent, scope: self.browse.clone(), ext });
        self.waiting = true;
    }

    fn local_list(&mut self) {
        let terms = search::terms_of(&self.query);
        if self.mode == Mode::Trash {
            self.trash = crate::recycle::scan();
        }
        let mut v: Vec<Item> = match self.mode {
            Mode::Trash => self
                .trash
                .iter()
                .map(|e| {
                    let name = e.name();
                    Item { cat: util::category(&name, e.is_dir), name, path: e.r_path.clone(), is_dir: e.is_dir, size: e.size, mtime: e.deleted, ..Default::default() }
                })
                .collect(),
            Mode::Dests => self.state.dests.iter().filter_map(|(p, t)| search::item_from_path(p, Some(*t))).filter(|i| i.is_dir).collect(),
            _ => bin::list()
                .into_iter()
                .map(|b| {
                    let md = std::fs::metadata(&b.target).ok();
                    let is_dir = md.as_ref().is_some_and(|m| m.is_dir());
                    Item {
                        name: b.alias,
                        cat: util::category(&b.target, is_dir),
                        path: b.target,
                        is_dir,
                        size: md.as_ref().map(|m| if m.is_dir() { 0 } else { m.len() }).unwrap_or(0),
                        mtime: md.as_ref().map(crate::index::mtime_of).unwrap_or(0),
                        when: None,
                        line: None,
                        rank: 0,
                    }
                })
                .collect(),
        };
        if !terms.is_empty() {
            v.retain(|i| search::score(&util::fold(&i.name), &terms).is_some());
        }
        self.set_items(v, true);
    }

    /// Ana liste (boş arama, son kullanılanlar) en üstte mi? Öyleyse sessiz yenilemede en üstte
    /// kalır; sıralama değişince seçili öğeyi takip edip listenin sonuna zıplamaz.
    fn home_top(&self) -> bool {
        self.mode == Mode::Search && self.browse.is_none() && self.query.trim().is_empty() && self.sel == 0 && self.scroll == 0 && self.edit.is_none()
    }

    fn set_items(&mut self, items: Vec<Item>, new_query: bool) {
        let home_top = self.home_top();
        let keep = self.reselect.take().or_else(|| if new_query || home_top { None } else { self.selected().map(|i| i.path.clone()) });
        self.items = items;
        if !self.marked.is_empty() {
            let have: HashSet<&str> = self.items.iter().map(|i| i.path.as_str()).collect();
            self.marked.retain(|p| have.contains(p.as_str()));
        }
        self.sel = keep.and_then(|p| self.items.iter().position(|i| i.path == p)).unwrap_or(0);
        if new_query {
            self.scroll = 0;
        }
        self.fix_scroll();
    }

    pub fn fix_scroll(&mut self) {
        let rows = self.visible_rows();
        if self.items.is_empty() {
            self.sel = 0;
            self.scroll = 0;
            return;
        }
        self.sel = self.sel.min(self.items.len() - 1);
        if self.sel < self.scroll {
            self.scroll = self.sel;
        }
        if self.sel >= self.scroll + rows {
            self.scroll = self.sel + 1 - rows;
        }
        self.scroll = self.scroll.min(self.items.len().saturating_sub(rows));
    }

    /// Tekerlek: listeyi kaydır. Seçim görünür alanın içinde tutulur (her karede çağrılan
    /// `fix_scroll` seçimi görünür kılmak için kaydırmayı geri almasın diye).
    pub fn scroll_by(&mut self, d: isize) {
        let rows = self.visible_rows();
        let max = self.items.len().saturating_sub(rows) as isize;
        self.scroll = (self.scroll as isize + d).clamp(0, max.max(0)) as usize;
        if self.items.is_empty() || self.edit.is_some() {
            return;
        }
        let last = (self.scroll + rows).min(self.items.len()) - 1;
        let sel = self.sel.clamp(self.scroll, last.max(self.scroll));
        if sel != self.sel {
            self.sel = sel;
            self.request_preview();
        }
    }

    // ---------------- Çoklu seçim ----------------

    /// Uygulama durumundan seçim modeli (yollar → satır indeksleri).
    fn selection(&self) -> Selection {
        let pos = |p: &str| self.items.iter().position(|x| x.path == p);
        let set: std::collections::BTreeSet<usize> = self.items.iter().enumerate().filter(|(_, x)| self.marked.contains(&x.path)).map(|(i, _)| i).collect();
        let base = if set.is_empty() { Default::default() } else { self.items.iter().enumerate().filter(|(_, x)| self.base.contains(&x.path)).map(|(i, _)| i).collect() };
        let anchor = self.anchor_path.as_deref().and_then(pos).unwrap_or(self.sel);
        Selection { cursor: self.sel, anchor, set, base }
    }

    fn store_selection(&mut self, s: Selection) {
        let path_of = |i: usize| self.items.get(i).map(|x| x.path.clone());
        self.marked = s.set.iter().filter_map(|&i| path_of(i)).collect();
        self.base = s.base.iter().filter_map(|&i| path_of(i)).collect();
        self.anchor_path = path_of(s.anchor);
        self.sel = s.cursor.min(self.items.len().saturating_sub(1));
        self.fix_scroll();
    }

    /// Düz tıklama: tek seçim, çapa buraya (Çoklu seçim modunda tıklama işaretler).
    pub fn select_single(&mut self, i: usize) {
        if i >= self.items.len() {
            return;
        }
        if self.multi {
            self.toggle_mark(i);
            return;
        }
        let mut s = self.selection();
        s.plain(i);
        self.store_selection(s);
    }

    /// Ctrl+tık / Boşluk (Çoklu seçim): öğeyi seçime ekle/çıkar, çapa buraya.
    pub fn toggle_mark(&mut self, i: usize) {
        if i >= self.items.len() {
            return;
        }
        let mut s = self.selection();
        s.toggle(i, !self.multi);
        self.store_selection(s);
    }

    /// Shift+tık: çapadan i'ye aralık (her iki yönde).
    pub fn select_range(&mut self, i: usize) {
        if self.items.is_empty() {
            return;
        }
        let mut s = self.selection();
        s.extend_to(i.min(self.items.len() - 1), self.multi);
        self.store_selection(s);
    }

    /// Ctrl+Shift+tık: mevcut seçimi koruyarak çapadan i'ye ekle.
    pub fn select_range_add(&mut self, i: usize) {
        if self.items.is_empty() {
            return;
        }
        let mut s = self.selection();
        s.extend_to(i.min(self.items.len() - 1), true);
        self.store_selection(s);
    }

    /// Oklar (Shift ile aralık).
    pub fn step_sel(&mut self, d: isize, shift: bool) {
        if self.items.is_empty() {
            return;
        }
        let mut s = self.selection();
        s.step(d, self.items.len(), shift, self.multi);
        self.store_selection(s);
    }

    pub fn select_all(&mut self) {
        let mut s = self.selection();
        s.select_all(self.items.len());
        self.store_selection(s);
    }

    pub fn clear_marks(&mut self) {
        self.marked.clear();
        self.base.clear();
    }

    pub fn set_multi(&mut self, on: bool) {
        self.multi = on;
        self.clear_marks();
        self.anchor_path = self.selected().map(|i| i.path.clone());
        if on {
            self.toast("Çoklu seçim: Boşluk ya da tık ile işaretleyin, Ctrl+A ile hepsini seçin", true);
        }
    }

    pub fn is_marked(&self, i: usize) -> bool {
        self.items.get(i).is_some_and(|x| self.marked.contains(&x.path))
    }

    /// İşlemlerin hedefleri: çoklu seçim varsa onlar (liste sırasıyla), yoksa seçili öğe.
    pub fn targets(&self) -> Vec<Item> {
        let real = |x: &Item| x.path != crate::recycle::VIRTUAL_PATH && !x.path.is_empty();
        if !self.marked.is_empty() {
            return self.items.iter().filter(|x| self.marked.contains(&x.path) && real(x)).cloned().collect();
        }
        self.selected().filter(|x| real(x)).cloned().into_iter().collect()
    }

    /// Kanalları oku.
    pub fn poll(&mut self, ctx: &Context) {
        while let Ok(r) = self.srx.try_recv() {
            if r.generation != self.gen_ {
                continue;
            }
            let new_query = self.shown_query != self.query || self.items.is_empty();
            self.shown_query = self.query.clone();
            let mut items = r.items;
            self.add_trash_entry(&mut items);
            self.set_items(items, new_query);
            // yeni öğe adı yazılırken liste yenilendiyse yer tutucu satır başta kalsın
            if matches!(self.edit.as_ref().map(|e| &e.kind), Some(EditKind::New { .. })) && !self.items.first().is_some_and(|i| i.path.is_empty()) {
                self.items.insert(0, Item { name: String::new(), path: String::new(), is_dir: true, ..Default::default() });
                self.sel = 0;
                self.scroll = 0;
            }
            if r.done {
                self.waiting = false;
            }
        }
        // dizin değişti: sessizce yenile (seçim korunur)
        let v = self.idx.version.load(Ordering::SeqCst);
        if v != self.idx_version {
            self.idx_version = v;
            if self.mode == Mode::Search && !(self.browse.is_some() && self.query.trim().is_empty()) {
                self.research_keep();
            }
        }
        while let Ok(p) = self.prx.try_recv() {
            if p.key != self.pkey {
                continue;
            }
            self.preview_tex = p.image.as_ref().map(|im| {
                ctx.load_texture("onizleme", egui::ColorImage::from_rgba_unmultiplied([im.w, im.h], &im.rgba), egui::TextureOptions::LINEAR)
            });
            self.preview_scroll = 0.0;
            self.preview = Some(p);
        }
        while let Ok((path, r)) = self.trx.try_recv() {
            let t = match r {
                Some((w, h, px)) => Thumb::Ready(ctx.load_texture(format!("k:{path}"), egui::ColorImage::from_rgba_unmultiplied([w, h], &px), egui::TextureOptions::LINEAR)),
                None => Thumb::Failed,
            };
            self.thumbs.insert(path, t);
        }
        while let Ok(d) = self.orx.try_recv() {
            self.busy_op = false;
            self.toast(d.msg, d.ok);
            // kopya yapıştırıldı: ~3 sn "Yapıştırıldı", sonra şerit gizlenir (pano korunur).
            // Kesme yapıştırmasında pano zaten temizlendi (şerit hemen kalkar).
            if d.dest.is_some() && self.clip.as_ref().is_some_and(|c| !c.1) && self.clip_strip != ClipStrip::Hidden {
                self.clip_strip = ClipStrip::Pasted(Instant::now());
            }
            self.prune_clip();
            if let Some(dest) = d.dest {
                // yapıştırılan klasör "son hedefler"e; görünümde kal, yapıştırılan öğeyi seç
                self.state.add_dest(&dest);
                self.state.save();
                self.marked.clear();
                if self.browse.as_deref() == Some(dest.as_str()) {
                    self.reselect = d.first;
                    self.research();
                } else {
                    self.research_keep();
                }
            } else {
                self.research_keep();
            }
        }
        if let Some((_, _, t)) = &self.toast
            && t.elapsed() > Duration::from_millis(3800)
        {
            self.toast = None;
        }
        if let ClipStrip::Pasted(t) = self.clip_strip
            && t.elapsed() > Duration::from_secs(3)
        {
            self.clip_strip = ClipStrip::Hidden;
        }
    }

    /// Seçimi koruyarak yeniden ara.
    fn research_keep(&mut self) {
        if let Some(i) = self.selected().filter(|_| !self.home_top()) {
            self.reselect = Some(i.path.clone());
        }
        self.research();
    }

    pub fn busy(&self) -> bool {
        self.waiting || self.busy_op || self.toast.is_some() || self.thumbs.values().any(|t| matches!(t, Thumb::Pending)) || (self.preview_open && self.preview.as_ref().is_none_or(|p| p.key != self.pkey))
    }

    pub fn want_thumb(&mut self, path: &str) {
        if !self.thumbs.contains_key(path) {
            if self.thumbs.len() > 400 {
                self.thumbs.retain(|_, t| matches!(t, Thumb::Pending));
            }
            self.thumbs.insert(path.to_string(), Thumb::Pending);
            let _ = self.ttx.send(path.to_string());
        }
    }

    pub fn request_preview(&mut self) {
        if !self.preview_open {
            return;
        }
        let Some(it) = self.selected().cloned() else {
            self.preview = None;
            self.preview_item = None;
            return;
        };
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            it.path.hash(&mut h);
            it.mtime.hash(&mut h);
            h.finish() | 1
        };
        if key == self.pkey {
            return;
        }
        self.pkey = key;
        self.preview_item = Some(it.clone());
        let needle = self.content_query().filter(|q| !q.is_empty());
        let line = if needle.is_some() { it.line.as_ref().map(|l| l.0) } else { None };
        let _ = self.ptx.send(PreviewReq { key, path: it.path.clone(), is_dir: it.is_dir, line, needle, w: 640, h: 480 });
    }

    /// Pencere yeniden gösterildi: temiz bir başlangıç.
    pub fn on_shown(&mut self) {
        self.menu = None;
        self.confirm = None;
        self.edit = None;
        self.clear_marks();
        self.multi = false;
        self.settings_edit = None;
        if matches!(self.mode, Mode::Note { .. }) {
            self.note_draft = std::mem::take(&mut self.note_text);
        }
        self.crumb_sel = None;
        self.prune_clip();
        self.shown_at = Instant::now();
        self.got_focus = false;
        if self.recording {
            self.recording = false;
            platform::suspend_hotkey(false);
        }
        self.mode = Mode::Search;
        self.saved = None;
        self.browse = None;
        self.query.clear();
        self.preview_open = false;
        self.focus_query = true;
        self.research();
    }

    // ---------------- Eylemler ----------------

    pub fn run(&mut self, a: Act, ctx: &Context) {
        self.menu = None;
        // Geri Dönüşüm Kutusu görünümü: yalnızca güvenli işlemler (kalıcı silme, ad değiştirme,
        // kopyalama/taşıma yok). Yol kopyalama özgün yolu kopyalar.
        let a = match (self.mode == Mode::Trash, a) {
            (true, Act::CopyPath) => Act::OrigPath,
            (true, Act::Reveal) => Act::OrigReveal,
            (true, Act::Open | Act::Preview | Act::Restore | Act::OrigReveal | Act::OrigPath | Act::TrashExplorer | Act::Settings | Act::Dests | Act::BinMgr) => a,
            (true, _) => return,
            // sanal öğe: Alt+Enter / Explorer'da aç → kutuyu Gezgin'de aç
            (false, Act::Reveal) if self.trash_entry_sel() => Act::TrashExplorer,
            _ => a,
        };
        // boş Bin menüsünden Yeni / Not al / Terminal: Bin'den çıkıp önceki klasörde çalış
        if self.mode == Mode::Bin && self.items.is_empty() && matches!(a, Act::New | Act::Note | Act::Terminal) {
            self.leave_mode();
        }
        match a {
            Act::Open => self.activate(self.sel, false, ctx),
            Act::Reveal => self.activate(self.sel, true, ctx),
            Act::Preview => self.toggle_preview(),
            Act::Copy => self.start_copy(false, ctx),
            Act::Cut => self.start_copy(true, ctx),
            Act::Paste => self.paste(),
            Act::Rename | Act::BinRename => self.start_rename(),
            Act::Bin => self.start_bin_add(),
            Act::CopyPath => self.copy_path(ctx),
            Act::Dests => {
                self.enter_mode(Mode::Dests);
            }
            Act::BinMgr => {
                self.enter_mode(Mode::Bin);
            }
            Act::BinRemove => {
                if let Some(it) = self.selected().cloned() {
                    self.confirm = Some(bin_remove_confirm(it.name));
                }
            }
            Act::AddPath => self.ask_path(),
            Act::New => self.start_new(),
            Act::Delete => self.ask_delete(),
            Act::ExtFilter => {
                if self.ext.take().is_some() {
                    self.research();
                } else {
                    let prev = std::mem::take(&mut self.query);
                    self.mode = Mode::ExtInput { prev };
                    self.focus_query = true;
                }
            }
            Act::Settings => self.open_settings(),
            Act::Restore => self.restore_selected(),
            Act::OrigReveal => {
                if let Some(d) = self.trash_sel().map(|e| e.orig_dir()) {
                    if std::path::Path::new(&d).is_dir() {
                        if let Err(e) = ops::open(&d) {
                            self.toast(format!("Klasör açılamadı: {e}"), false);
                        }
                    } else {
                        self.toast(format!("Özgün klasör artık yok: {d}"), false);
                    }
                }
            }
            Act::OrigPath => {
                if let Some(o) = self.trash_sel().map(|e| e.orig.clone()) {
                    let ok = ops::set_clipboard_text(&o);
                    if !cfg!(windows) {
                        ctx.copy_text(o.clone());
                    }
                    self.toast(if ok || !cfg!(windows) { format!("Özgün yol kopyalandı: {o}") } else { "Pano kullanılamadı".into() }, ok || !cfg!(windows));
                }
            }
            Act::TrashExplorer => {
                if let Err(e) = platform::open_recycle_bin_in_explorer() {
                    self.toast(format!("Gezgin açılamadı: {e}"), false);
                } else {
                    platform::hide(ctx);
                }
            }
            Act::Refresh => {
                self.idx.start(true);
                self.research_keep();
                self.toast("Dizin yenileniyor", true);
            }
            Act::SelectAll => {
                if self.mode == Mode::Search {
                    if !self.multi {
                        self.set_multi(true);
                    }
                    self.select_all();
                }
            }
            Act::CycleMode => self.cycle_mode(),
            Act::ClearClip => self.clear_clip(),
            Act::Multi => {
                if self.mode == Mode::Search {
                    let on = !self.multi;
                    self.set_multi(on);
                }
            }
            Act::OpenAll => self.open_all(ctx),
            Act::Note => self.start_note(),
            Act::Terminal => self.open_terminal(),
        }
    }

    /// Çoklu seçimde işaretlenenleri aç (en fazla 20; klasörler Gezgin'de açılır).
    fn open_all(&mut self, ctx: &Context) {
        let ts = self.targets();
        if ts.is_empty() {
            self.toast("Önce bir öğe seçin", false);
            return;
        }
        if ts.len() == 1 {
            let i = self.items.iter().position(|x| x.path == ts[0].path).unwrap_or(self.sel);
            self.activate(i, false, ctx);
            return;
        }
        const MAX_OPEN: usize = 20;
        if ts.len() > MAX_OPEN {
            self.toast(format!("Aynı anda en fazla {MAX_OPEN} öğe açılabilir ({} seçili)", ts.len()), false);
            return;
        }
        let mut err = None;
        for it in &ts {
            match ops::open(&it.path) {
                Ok(()) => {
                    self.state.touch(&it.path);
                    if it.is_dir {
                        self.state.add_dest(&it.path);
                    }
                }
                Err(e) => err = Some(e),
            }
        }
        self.state.save();
        match err {
            None => {
                self.toast(format!("{} öğe açıldı", ts.len()), true);
                platform::hide(ctx);
            }
            Some(e) => self.toast(format!("Bazı öğeler açılamadı: {e}"), false),
        }
    }

    // ---------------- Not al ----------------

    pub fn start_note(&mut self) {
        let Some(dir) = self.current_dir() else {
            self.toast("Önce bir klasöre girin", false);
            return;
        };
        self.menu = None;
        self.edit = None;
        self.multi = false;
        self.clear_marks();
        if self.saved.is_none() {
            self.saved = Some(Saved { browse: self.browse.clone(), query: self.query.clone(), sel: self.sel });
        }
        self.note_text = std::mem::take(&mut self.note_draft);
        self.mode = Mode::Note { dir };
        self.focus_query = false;
    }

    /// Not modundan kaydetmeden çık (yazılan taslak bir sonraki "Not al"da geri gelir).
    pub fn leave_note(&mut self) {
        if matches!(self.mode, Mode::Note { .. }) {
            self.note_draft = std::mem::take(&mut self.note_text);
            self.leave_mode();
            self.focus_query = true;
        }
    }

    /// Enter: notu kaydet.
    pub fn save_note(&mut self) {
        let Mode::Note { dir } = self.mode.clone() else { return };
        if self.note_text.trim().is_empty() {
            self.toast("Not boş · bir şey yazın ya da yapıştırın", false);
            return;
        }
        let now = chrono::Local::now().naive_local();
        match notes::save(Path::new(&dir), &self.note_text, &now) {
            Ok(p) => {
                let name = file_name(&p.to_string_lossy());
                self.note_text.clear();
                self.note_draft.clear();
                let (index, p2) = (self.idx.index.clone(), p.clone());
                std::thread::spawn(move || index.write().unwrap().add_path(&p2));
                self.leave_mode();
                if self.browse.as_deref() == Some(dir.as_str()) {
                    self.reselect = Some(p.to_string_lossy().to_string());
                    self.research();
                }
                self.focus_query = true;
                self.toast(format!("Not kaydedildi: {name}"), true);
            }
            Err(e) => self.toast(format!("Not kaydedilemedi: {e}"), false),
        }
    }

    // ---------------- Terminal ----------------

    pub fn term_choice(&self) -> Term {
        terminal::resolve(&self.config.terminal, &self.config.terminal_custom, &terminal::Env::current())
    }

    /// Ayarlar satırında görünen ad.
    pub fn term_label(&self) -> String {
        match self.term_choice() {
            Term::Custom => {
                let n = file_name(self.config.terminal_custom.trim().trim_matches('"'));
                format!("Başka: {n}")
            }
            t => t.label().to_string(),
        }
    }

    pub fn open_terminal(&mut self) {
        let Some(dir) = self.current_dir() else {
            self.toast("Önce bir klasöre girin", false);
            return;
        };
        let env = terminal::Env::current();
        let t = self.term_choice();
        match terminal::build(t, &self.config.terminal_custom, &dir, &env).and_then(|c| terminal::launch(&c)) {
            Ok(()) => {
                self.state.add_dest(&dir);
                self.state.save();
                self.toast(format!("{} açıldı: {}", t.label(), dir), true);
            }
            Err(e) => self.toast(format!("Terminal açılamadı: {e}"), false),
        }
    }

    /// Ayarlar: terminal seçimini sırayla değiştir (yalnızca bu bilgisayarda bulunanlar + Başka bir program).
    pub fn cycle_terminal(&mut self, d: isize) {
        let env = terminal::Env::current();
        let avail = terminal::available(&env);
        let cur = self.term_choice();
        let i = avail.iter().position(|t| *t == cur).unwrap_or(0) as isize;
        let next = avail[(i + d).rem_euclid(avail.len() as isize) as usize];
        if next == Term::Custom {
            // yol yazılsın (varsa eskisi dolu gelir)
            self.settings_edit = Some(self.config.terminal_custom.clone());
            return;
        }
        self.set_terminal(next, None);
    }

    fn set_terminal(&mut self, t: Term, custom: Option<String>) {
        self.config.terminal = t.id().to_string();
        if let Some(c) = custom {
            self.config.terminal_custom = c;
        }
        self.config.save();
        self.toast(format!("Terminal: {}", self.term_label()), true);
    }

    /// "Başka bir program" yolunu uygula.
    pub fn commit_terminal_path(&mut self) {
        let Some(p) = self.settings_edit.take() else { return };
        let p = p.trim().trim_matches('"').to_string();
        if p.is_empty() {
            self.toast("Programın tam yolunu yazın · vazgeçmek için Esc", false);
            self.settings_edit = Some(p);
            return;
        }
        if !terminal::valid_custom(&p) {
            self.toast("Program bulunamadı · tam yolu yazın (ör. C:\\Programlar\\Terminal\\terminal.exe)", false);
            self.settings_edit = Some(p);
            return;
        }
        self.set_terminal(Term::Custom, Some(p));
    }

    pub fn open_settings(&mut self) {
        self.menu = None;
        self.edit = None;
        self.confirm = None;
        if self.mode != Mode::Settings {
            self.enter_mode(Mode::Settings);
        }
        self.settings_sel = 0;
    }

    /// Ayarlar: kısayol kaydını başlat (genel kısayol geçici olarak bırakılır).
    pub fn start_record(&mut self) {
        self.recording = true;
        self.hk_pending = None;
        platform::suspend_hotkey(true);
    }

    pub fn cancel_record(&mut self) {
        if self.recording {
            self.recording = false;
            platform::suspend_hotkey(false);
        }
    }

    /// Kaydedilen tuş kombinasyonunu uygula (RegisterHotKey ile doğrulanır).
    pub fn apply_hotkey(&mut self, h: Hotkey) {
        self.recording = false;
        if !config::valid_hotkey(h) {
            platform::suspend_hotkey(false);
            self.toast(format!("{} kullanılamaz: Ctrl/Alt ile birlikte ya da F tuşu seçin", h.label()), false);
            return;
        }
        self.hk_pending = Some(h);
        platform::set_hotkey(h);
    }

    pub fn toggle_autostart(&mut self) {
        let on = !self.config.autostart;
        if platform::set_autostart(on, true) {
            self.config.autostart = on;
            self.config.save();
            self.toast(if on { "Windows açılışında başlayacak" } else { "Windows açılışında başlamayacak" }, true);
        } else {
            self.toast("Başlangıç ayarı yazılamadı", false);
        }
    }

    fn enter_mode(&mut self, m: Mode) {
        if self.saved.is_none() {
            self.saved = Some(Saved { browse: self.browse.clone(), query: self.query.clone(), sel: self.sel });
        }
        self.mode = m;
        self.query.clear();
        self.items.clear();
        self.research();
    }

    fn leave_mode(&mut self) {
        self.cancel_record();
        self.mode = Mode::Search;
        if let Some(s) = self.saved.take() {
            self.browse = s.browse;
            self.query = s.query;
            self.reselect = None;
            self.research();
            self.sel = s.sel;
        } else {
            self.query.clear();
            self.research();
        }
    }

    pub fn toggle_preview(&mut self) {
        self.preview_open = !self.preview_open;
        if self.preview_open {
            self.pkey = 0;
            self.request_preview();
        }
        self.fix_scroll();
    }

    pub fn activate(&mut self, idx: usize, alt: bool, ctx: &Context) {
        let Some(it) = self.items.get(idx).cloned() else { return };
        self.sel = idx;
        if it.path == crate::recycle::VIRTUAL_PATH {
            if alt {
                self.run(Act::TrashExplorer, ctx);
            } else {
                self.enter_trash();
            }
            return;
        }
        if self.mode == Mode::Trash {
            if alt {
                self.run(Act::OrigReveal, ctx);
                return;
            }
            // silinmiş öğeyi olduğu yerde aç (klasörler Gezgin'de); kutudan çıkarılmaz
            match ops::open(&it.path) {
                Ok(()) => platform::hide(ctx),
                Err(e) => self.toast(e, false),
            }
            return;
        }
        if alt {
            match ops::reveal(&it.path) {
                Ok(()) => {
                    // Gezgin'de gösterilen öğenin klasörü "Son hedefler"e
                    if let Some(par) = parent_of(&it.path) {
                        self.state.add_dest(&par);
                        self.state.save();
                    }
                    platform::hide(ctx)
                }
                Err(e) => self.toast(format!("Gezgin açılamadı: {e}"), false),
            }
            return;
        }
        match self.mode.clone() {
            Mode::Settings | Mode::ExtInput { .. } | Mode::Note { .. } | Mode::Trash => {}
            Mode::Dests => {
                self.mode = Mode::Search;
                self.saved = None;
                self.enter_browse(&it.path);
            }
            Mode::Bin | Mode::Search => {
                if it.is_dir && self.mode == Mode::Search {
                    self.enter_browse(&it.path);
                } else {
                    match ops::open(&it.path) {
                        Ok(()) => {
                            self.state.touch(&it.path);
                            if it.is_dir {
                                self.state.add_dest(&it.path);
                            }
                            self.state.save();
                            platform::hide(ctx);
                        }
                        Err(e) => self.toast(e, false),
                    }
                }
            }
        }
    }

    pub fn enter_browse(&mut self, dir: &str) {
        self.crumb_sel = None;
        self.clear_marks();
        self.browse = Some(dir.to_string());
        self.query.clear();
        self.sel = 0;
        self.scroll = 0;
        self.items.clear();
        self.state.touch(dir);
        // girilen her klasör "Son hedefler"e (en yeni başta, yakın klasörler tek kayıt)
        self.state.add_dest(dir);
        self.state.save();
        self.research();
    }

    /// Gözatmadan çık, genel aramaya dön.
    pub fn leave_browse(&mut self) {
        self.crumb_sel = None;
        self.marked.clear();
        self.browse = None;
        self.query.clear();
        self.items.clear();
        self.research();
    }

    pub fn busy_waiting(&self) -> bool {
        self.waiting
    }

    /// Ctrl+← / fare geri düğmesi / "Üst klasör": bir üst klasör; Geri Dönüşüm Kutusu'ndan çıkar.
    pub fn browse_up_pub(&mut self) {
        if self.mode == Mode::Trash {
            self.leave_mode();
            self.focus_query = true;
            return;
        }
        if self.browse.is_some() && self.mode == Mode::Search {
            self.browse_up();
        }
    }

    fn browse_up(&mut self) {
        let Some(cur) = self.browse.clone() else { return };
        self.marked.clear();
        self.crumb_sel = None;
        match parent_of(&cur) {
            Some(p) if p != cur => {
                self.reselect = Some(cur);
                self.state.add_dest(&p);
                self.state.save();
                self.browse = Some(p);
                self.query.clear();
                self.research();
            }
            _ => self.leave_browse(),
        }
    }

    fn copy_path(&mut self, ctx: &Context) {
        let ts = self.targets();
        if ts.is_empty() {
            return;
        }
        let text = ts.iter().map(|i| i.path.as_str()).collect::<Vec<_>>().join("\r\n");
        let ok = ops::set_clipboard_text(&text);
        if !cfg!(windows) {
            ctx.copy_text(text.clone());
        }
        let msg = if ts.len() > 1 { format!("{} yol kopyalandı", ts.len()) } else { format!("Yol kopyalandı: {}", ts[0].path) };
        self.toast(if ok || !cfg!(windows) { msg } else { "Pano kullanılamadı".into() }, ok || !cfg!(windows));
    }

    /// Kopyala/Kes: yalnızca panoya al (CF_HDROP + kes/kopyala bilgisi). Klasörden çıkılmaz,
    /// hedef seçici açılmaz; hedefe gidip Ctrl+V / Tab→v ile yapıştırılır.
    fn start_copy(&mut self, cut: bool, ctx: &Context) {
        if self.mode != Mode::Search {
            return;
        }
        let ts = self.targets();
        if ts.is_empty() {
            self.toast("Önce bir öğe seçin", false);
            return;
        }
        let srcs: Vec<String> = ts.iter().map(|i| i.path.clone()).collect();
        self.clip = Some((srcs.clone(), cut));
        self.clip_strip = ClipStrip::Shown;
        let ok = ops::set_clipboard_files(&srcs, cut);
        if !cfg!(windows) {
            ctx.copy_text(srcs.join("\n"));
        }
        let verb = if cut { "Kesildi" } else { "Kopyalandı" };
        let more = if srcs.len() > 1 { format!(" +{}", srcs.len() - 1) } else { String::new() };
        if ok || !cfg!(windows) {
            self.toast(format!("{verb} · {}{more}", ts[0].name), true);
        } else {
            self.toast(format!("{verb} · {}{more} (Windows panosu kullanılamadı; yalnızca Fastplorer içinde yapıştırılabilir)", ts[0].name), false);
        }
    }

    /// Delete / Tab→d: onaydan sonra Geri Dönüşüm Kutusu'na (asla kalıcı silme).
    fn ask_delete(&mut self) {
        if self.mode != Mode::Search {
            return;
        }
        let ts = self.targets();
        if ts.is_empty() {
            self.toast("Önce bir öğe seçin", false);
            return;
        }
        let paths: Vec<String> = ts.iter().map(|i| i.path.clone()).collect();
        let line = if ts.len() == 1 { format!("\"{}\"", ts[0].name) } else { format!("\"{}\" ve {} öğe daha", ts[0].name, ts.len() - 1) };
        self.confirm = Some(Confirm {
            title: format!("{} öğe Geri Dönüşüm Kutusu'na gönderilsin mi?", ts.len()),
            lines: vec![line, "Geri Dönüşüm Kutusu'ndan geri alınabilir.".into()],
            buttons: vec!["Gönder", "Vazgeç"],
            sel: 0,
            kind: CKind::Delete { paths },
        });
    }

    fn do_delete(&mut self, paths: Vec<String>) {
        if self.busy_op {
            self.toast("Önceki işlem sürüyor", false);
            return;
        }
        self.busy_op = true;
        let tx = self.otx.clone();
        let index = self.idx.index.clone();
        // seçimi silinenlerden sonraki öğeye taşı
        let next = self.items.iter().skip(self.sel).find(|i| !paths.contains(&i.path)).or_else(|| self.items.iter().take(self.sel).rev().find(|i| !paths.contains(&i.path))).map(|i| i.path.clone());
        self.marked.clear();
        self.items.retain(|i| !paths.contains(&i.path));
        self.sel = next.and_then(|p| self.items.iter().position(|i| i.path == p)).unwrap_or(0);
        self.fix_scroll();
        std::thread::spawn(move || {
            let r = platform::recycle(&paths);
            let n = paths.len();
            let msg = match &r {
                Ok(()) => {
                    let mut ix = index.write().unwrap();
                    for p in &paths {
                        ix.remove_path(p);
                    }
                    if n == 1 { format!("Geri Dönüşüm Kutusu'na gönderildi: {}", file_name(&paths[0])) } else { format!("{n} öğe Geri Dönüşüm Kutusu'na gönderildi") }
                }
                Err(e) => e.clone(),
            };
            let _ = tx.send(OpDone { msg, ok: r.is_ok(), dest: None, first: None });
        });
    }

    /// Geçerli klasör: gözatılan klasör; değilse seçili öğenin klasörü.
    fn current_dir(&self) -> Option<String> {
        match (&self.browse, self.mode == Mode::Search) {
            (Some(d), true) => Some(d.clone()),
            _ => self.selected().and_then(|i| if i.is_dir { Some(i.path.clone()) } else { parent_of(&i.path) }),
        }
    }

    /// Tab→n: listenin başına satır içi ad düzenleme satırı ekle.
    fn start_new(&mut self) {
        if self.mode != Mode::Search {
            return;
        }
        let Some(dir) = self.current_dir() else {
            self.toast("Önce bir klasöre girin", false);
            return;
        };
        self.marked.clear();
        self.items.insert(0, Item { name: String::new(), path: String::new(), is_dir: true, ..Default::default() });
        self.sel = 0;
        self.scroll = 0;
        self.edit = Some(Edit { kind: EditKind::New { dir }, text: String::new(), row: 0, select: None });
    }

    /// Yeni öğe düzenlemesi iptal: yer tutucu satırı kaldır.
    pub fn cancel_edit(&mut self) {
        if let Some(e) = self.edit.take()
            && matches!(e.kind, EditKind::New { .. })
            && self.items.first().is_some_and(|i| i.path.is_empty())
        {
            self.items.remove(0);
            self.fix_scroll();
        }
    }

    fn create_new(&mut self, dir: &str, name: &str) -> Result<String, String> {
        let p = Path::new(dir).join(name);
        let r = if new_item_is_file(name) {
            std::fs::OpenOptions::new().write(true).create_new(true).open(&p).map(|_| ())
        } else {
            std::fs::create_dir(&p)
        };
        r.map_err(|e| e.to_string())?;
        let s = p.to_string_lossy().to_string();
        let (index, s2) = (self.idx.index.clone(), p.clone());
        std::thread::spawn(move || index.write().unwrap().add_path(&s2));
        Ok(s)
    }

    /// Kopyala/taşı. `policy`: None = sor, Some(true) = üzerine yaz, Some(false) = yeni adla.
    fn transfer(&mut self, srcs: Vec<String>, dest: String, cut: bool, policy: Option<bool>) {
        if self.busy_op {
            self.toast("Önceki işlem sürüyor", false);
            return;
        }
        let dd = PathBuf::from(&dest);
        let conflict = srcs.iter().any(|s| {
            let t = dd.join(file_name(s));
            t.exists() && t != Path::new(s)
        });
        if conflict && policy.is_none() {
            let n = srcs.iter().find(|s| dd.join(file_name(s)).exists()).map(|s| file_name(s)).unwrap_or_default();
            self.confirm = Some(Confirm {
                title: "Aynı adda öğe var".into(),
                lines: vec![format!("\"{n}\" hedefte zaten var:"), dest.clone()],
                buttons: vec!["Üzerine yaz", "Yeni adla", "İptal"],
                sel: 1,
                kind: CKind::Transfer { srcs, dest, cut },
            });
            return;
        }
        if cut {
            self.clip = None;
            self.clip_strip = ClipStrip::Hidden;
        }
        self.busy_op = true;
        let tx = self.otx.clone();
        let index = self.idx.index.clone();
        std::thread::spawn(move || {
            let mut ok = 0;
            let mut err = String::new();
            let mut done: Vec<(String, PathBuf)> = Vec::new();
            for s in &srcs {
                let sp = Path::new(s);
                let mut t = dd.join(file_name(s));
                let mut over = false;
                if t == sp {
                    if cut {
                        err = "Kaynak zaten bu klasörde".into();
                        continue;
                    }
                    t = ops::unique_name(&dd, &file_name(s));
                } else if t.exists() {
                    if policy == Some(true) {
                        over = true;
                    } else {
                        t = ops::unique_name(&dd, &file_name(s));
                    }
                }
                if t.starts_with(sp) {
                    err = "Klasör kendi içine taşınamaz/kopyalanamaz".into();
                    continue;
                }
                match ops::transfer(sp, &t, cut, over) {
                    Ok(()) => {
                        ok += 1;
                        done.push((s.clone(), t));
                    }
                    Err(e) => err = e.to_string(),
                }
            }
            {
                let mut ix = index.write().unwrap();
                for (s, t) in &done {
                    ix.add_path(t);
                    if cut {
                        ix.remove_path(s);
                    }
                }
            }
            let verb = if cut { "Taşındı" } else { "Kopyalandı" };
            let msg = if err.is_empty() {
                let n = done.first().map(|d| file_name(&d.0)).unwrap_or_default();
                format!("{verb}: {n} · hedef: {}", dd.display())
            } else if ok > 0 {
                format!("{ok} öğe {}; hata: {err}", verb.to_lowercase())
            } else {
                format!("Hata: {err}")
            };
            let _ = tx.send(OpDone {
                msg,
                ok: err.is_empty(),
                dest: if ok > 0 { Some(dd.to_string_lossy().to_string()) } else { None },
                first: done.first().map(|d| d.1.to_string_lossy().to_string()),
            });
        });
    }

    fn paste(&mut self) {
        let dest = match (&self.browse, &self.mode) {
            (Some(d), Mode::Search) => Some(d.clone()),
            _ => self.selected().and_then(|i| if i.is_dir { Some(i.path.clone()) } else { parent_of(&i.path) }),
        };
        let Some(dest) = dest else {
            self.toast("Yapıştırmak için bir klasöre girin ya da bir klasör seçin", false);
            return;
        };
        let src = ops::get_clipboard_files().or_else(|| self.clip.clone());
        let Some((paths, cut)) = src else {
            self.toast("Panoda dosya yok (önce Ctrl+C / Ctrl+X)", false);
            return;
        };
        self.transfer(paths, dest, cut, None);
    }

    fn start_rename(&mut self) {
        let Some(it) = self.selected().filter(|i| i.path != crate::recycle::VIRTUAL_PATH).cloned() else {
            self.toast("Önce bir öğe seçin", false);
            return;
        };
        match self.mode {
            Mode::Search => {
                let stem = if it.is_dir { it.name.chars().count() } else { ops::split_ext(&it.name).0.chars().count() };
                self.edit = Some(Edit { kind: EditKind::Rename { path: it.path.clone() }, text: it.name.clone(), row: self.sel, select: Some((0, stem)) });
            }
            Mode::Bin => {
                let n = it.name.chars().count();
                self.edit = Some(Edit { kind: EditKind::BinRename { old: it.name.clone() }, text: it.name.clone(), row: self.sel, select: Some((0, n)) });
            }
            _ => {}
        }
    }

    pub fn commit_edit(&mut self) {
        let Some(e) = self.edit.take() else { return };
        match e.kind {
            EditKind::New { ref dir } => {
                let dir = dir.clone();
                let name = e.text.trim().to_string();
                if name.is_empty() {
                    self.edit = Some(e);
                    self.cancel_edit();
                    return;
                }
                if !util::valid_name(&name) {
                    self.toast("Geçersiz ad (şunlar kullanılamaz: < > : \" / \\ | ? *)", false);
                    self.edit = Some(e);
                    return;
                }
                let target = Path::new(&dir).join(&name);
                if target.exists() {
                    // var olan öğe: onay sor (oraya gidilsin mi?)
                    self.edit = Some(e);
                    self.confirm = Some(Confirm {
                        title: "Bu adda bir öğe zaten var".into(),
                        lines: vec![format!("\"{name}\" bu klasörde zaten var:"), dir.clone(), "Var olan öğe seçilsin mi?".into()],
                        buttons: vec!["Var olanı seç", "Başka ad yaz"],
                        sel: 1,
                        kind: CKind::NewExists { path: target.to_string_lossy().to_string() },
                    });
                    return;
                }
                if self.items.first().is_some_and(|i| i.path.is_empty()) {
                    self.items.remove(0);
                }
                match self.create_new(&dir, &name) {
                    Ok(path) => {
                        let kind = if new_item_is_file(&name) { "Dosya" } else { "Klasör" };
                        self.toast(format!("{kind} oluşturuldu: {name}"), true);
                        self.reselect = Some(path);
                        self.research();
                    }
                    Err(err) => self.toast(format!("Oluşturulamadı: {err}"), false),
                }
            }
            EditKind::Rename { ref path } => {
                let path = path.clone();
                let new = e.text.trim().to_string();
                let old = file_name(&path);
                if new == old {
                    return;
                }
                if !util::valid_name(&new) {
                    self.toast("Geçersiz ad (şunlar kullanılamaz: < > : \" / \\ | ? *)", false);
                    self.edit = Some(e);
                    return;
                }
                let to = Path::new(&path).with_file_name(&new).to_string_lossy().to_string();
                if Path::new(&to).exists() && util::fold(&new) != util::fold(&old) {
                    if Path::new(&to).is_dir() || Path::new(&path).is_dir() {
                        self.toast("Bu adda bir öğe zaten var", false);
                        self.edit = Some(e);
                        return;
                    }
                    self.confirm = Some(Confirm {
                        title: "Üzerine yazılsın mı?".into(),
                        lines: vec![format!("\"{new}\" adında bir dosya zaten var."), "Var olan dosya silinip yerine bu dosya konacak.".into()],
                        buttons: vec!["Üzerine yaz", "İptal"],
                        sel: 1,
                        kind: CKind::RenameOverwrite { from: path, to },
                    });
                    return;
                }
                self.do_rename(&path, &to);
            }
            EditKind::BinAlias { ref target } => {
                let target = target.clone();
                let alias = bin::sanitize_alias(&e.text);
                if alias.is_empty() {
                    self.toast("Geçersiz takma ad", false);
                    self.edit = Some(e);
                    return;
                }
                if bin::exists(&alias) {
                    self.confirm = Some(Confirm {
                        title: "Takma ad kullanımda".into(),
                        lines: vec![format!("\"{alias}\" zaten var. Yeni hedefle değiştirilsin mi?")],
                        buttons: vec!["Değiştir", "İptal"],
                        sel: 1,
                        kind: CKind::BinOverwrite { alias, target },
                    });
                    return;
                }
                self.do_bin_add(&alias, &target);
            }
            EditKind::BinRename { ref old } => {
                let old = old.clone();
                let new = bin::sanitize_alias(&e.text);
                if new.is_empty() || new == old {
                    return;
                }
                if bin::exists(&new) {
                    self.toast("Bu takma ad zaten var", false);
                    self.edit = Some(e);
                    return;
                }
                match bin::rename(&old, &new) {
                    Ok(()) => self.toast(format!("Takma ad değişti: {new} (eski: {old})"), true),
                    Err(err) => self.toast(format!("Değiştirilemedi: {err}"), false),
                }
                self.local_list();
            }
        }
    }

    fn do_rename(&mut self, from: &str, to: &str) {
        match std::fs::rename(from, to) {
            Ok(()) => {
                let new = file_name(to);
                if let Some(it) = self.items.iter_mut().find(|i| i.path == from) {
                    it.path = to.to_string();
                    it.name = new.clone();
                    it.cat = util::category(&new, it.is_dir);
                }
                self.items.retain(|i| i.path != to || i.name == new);
                let (index, fr, tt) = (self.idx.index.clone(), from.to_string(), to.to_string());
                std::thread::spawn(move || {
                    let mut ix = index.write().unwrap();
                    ix.remove_path(&tt);
                    if let Some(i) = ix.find(&fr) {
                        ix.rename(i, &file_name(&tt));
                    }
                });
                for h in self.state.history.iter_mut() {
                    if h.0 == from {
                        h.0 = to.to_string();
                    }
                }
                self.state.save();
                self.toast(format!("Yeni ad: {new}"), true);
                self.pkey = 0;
                self.request_preview();
            }
            Err(e) => self.toast(format!("Ad değiştirilemedi: {e}"), false),
        }
    }

    fn start_bin_add(&mut self) {
        let Some(it) = self.selected().cloned() else {
            self.toast("Önce bir öğe seçin", false);
            return;
        };
        if self.mode != Mode::Search {
            return;
        }
        let a = bin::default_alias(&it.path);
        let n = a.chars().count();
        self.edit = Some(Edit { kind: EditKind::BinAlias { target: it.path }, text: a, row: self.sel, select: Some((0, n)) });
    }

    fn do_bin_add(&mut self, alias: &str, target: &str) {
        match bin::add(alias, target) {
            Ok(_) => {
                self.toast(format!("Bin'e eklendi: terminalde \"{alias}\" yazarak açabilirsiniz"), true);
                if !bin::on_path() {
                    self.ask_path();
                }
            }
            Err(e) => self.toast(format!("Eklenemedi: {e}"), false),
        }
    }

    fn ask_path(&mut self) {
        self.confirm = Some(Confirm {
            title: "Bin klasörü PATH'e eklensin mi?".into(),
            lines: vec![
                bin::bin_dir().to_string_lossy().to_string(),
                "Eklenirse yeni açılan terminallerde takma adlar çalışır.".into(),
            ],
            buttons: vec!["PATH'e ekle", "Şimdi değil"],
            sel: 0,
            kind: CKind::AddPath,
        });
    }

    pub fn confirm_answer(&mut self, btn: Option<usize>) {
        let Some(d) = self.confirm.take() else { return };
        let b = btn.unwrap_or(usize::MAX);
        match d.kind {
            CKind::RenameOverwrite { from, to } => {
                if b == 0 {
                    self.do_rename(&from, &to);
                }
            }
            CKind::Transfer { srcs, dest, cut } => match b {
                0 => self.transfer(srcs, dest, cut, Some(true)),
                1 => self.transfer(srcs, dest, cut, Some(false)),
                _ => self.toast("İptal edildi", true),
            },
            CKind::BinRemove { alias } => {
                if b == 0 {
                    match bin::remove(&alias) {
                        Ok(()) => self.toast(format!("Kaldırıldı: {alias} (dosyanın kendisi silinmedi)"), true),
                        Err(e) => self.toast(format!("Kaldırılamadı: {e}"), false),
                    }
                    self.local_list();
                }
            }
            CKind::BinOverwrite { alias, target } => {
                if b == 0 {
                    self.do_bin_add(&alias, &target);
                }
            }
            CKind::Delete { paths } => {
                if b == 0 {
                    self.do_delete(paths);
                }
            }
            CKind::NewExists { path } => {
                if b == 0 {
                    self.cancel_edit();
                    self.reselect = Some(path);
                    self.research();
                }
                // değilse düzenleme açık kalır: başka ad yazılır
            }
            CKind::AddPath => {
                if b == 0 {
                    match bin::add_to_path() {
                        Ok(()) => self.toast("PATH güncellendi. Yeni terminallerde geçerli.", true),
                        Err(e) => self.toast(e, false),
                    }
                }
            }
        }
    }

    /// Sanal "Geri Dönüşüm Kutusu" öğesi: genel ad aramasında ("çöp", "geri dönüşüm", "recycle",
    /// "bin" …) en üstte; Masaüstü'ne gözatarken (Windows masaüstündeki simge gibi) listenin başında.
    /// Kutunun içeriği aramaya katılmaz.
    fn add_trash_entry(&self, items: &mut Vec<Item>) {
        if self.mode != Mode::Search || self.content_query().is_some() || self.ext.is_some() {
            return;
        }
        let q = self.query.trim();
        let show = match &self.browse {
            None => !q.is_empty() && crate::recycle::query_matches(q),
            Some(b) => self.is_desktop(b) && (q.is_empty() || crate::recycle::query_matches(q)),
        };
        if !show || items.iter().any(|i| i.path == crate::recycle::VIRTUAL_PATH) {
            return;
        }
        items.insert(0, Item { name: crate::recycle::TITLE.into(), path: crate::recycle::VIRTUAL_PATH.into(), is_dir: true, ..Default::default() });
    }

    /// Gözatılan klasör Masaüstü mü?
    pub fn is_desktop(&self, dir: &str) -> bool {
        let norm = |p: &str| util::fold(p.trim_end_matches(['\\', '/']));
        if self.desktop.is_empty() {
            return false;
        }
        norm(dir) == norm(&self.desktop)
    }

    /// Geri Dönüşüm Kutusu görünümünde seçili kayıt.
    pub fn trash_sel(&self) -> Option<&crate::recycle::Entry> {
        if self.mode != Mode::Trash {
            return None;
        }
        let p = &self.selected()?.path;
        self.trash.iter().find(|e| &e.r_path == p)
    }

    /// Seçili öğe sanal "Geri Dönüşüm Kutusu" mu?
    pub fn trash_entry_sel(&self) -> bool {
        self.mode == Mode::Search && self.selected().is_some_and(|i| i.path == crate::recycle::VIRTUAL_PATH)
    }

    /// Geri Dönüşüm Kutusu'na gir (Esc / Ctrl+← / fare geri düğmesi önceki görünüme döner).
    pub fn enter_trash(&mut self) {
        self.menu = None;
        self.edit = None;
        self.multi = false;
        self.clear_marks();
        self.crumb_sel = None;
        self.preview_open = false;
        self.enter_mode(Mode::Trash);
        if crate::recycle::bin_dirs().is_empty() && cfg!(windows) {
            self.toast("Geri Dönüşüm Kutusu okunamadı", false);
        }
    }

    /// Seçili silinmiş öğeyi özgün yerine geri yükle (asla üzerine yazmaz).
    fn restore_selected(&mut self) {
        let Some(e) = self.trash_sel().cloned() else {
            self.toast("Önce geri yüklenecek öğeyi seçin", false);
            return;
        };
        match crate::recycle::restore(&e) {
            Ok(t) => {
                let (index, p) = (self.idx.index.clone(), t.clone());
                std::thread::spawn(move || index.write().unwrap().add_path(Path::new(&p)));
                // seçim bir sonraki öğeye geçsin
                self.reselect = self.items.get(self.sel + 1).or_else(|| self.sel.checked_sub(1).and_then(|i| self.items.get(i))).map(|i| i.path.clone());
                let renamed = file_name(&t) != e.name();
                self.toast(if renamed { format!("Geri yüklendi (ad dolu olduğu için yeni adla): {t}") } else { format!("Geri yüklendi: {t}") }, true);
                self.research();
            }
            Err(x) => self.toast(format!("Geri yüklenemedi: {x}"), false),
        }
    }

    /// Arama modu: İsim → Uzantı → İçerik → İsim (F1 ya da arama kutusunun solundaki çip).
    pub fn cycle_mode(&mut self) {
        match self.mode.clone() {
            Mode::ExtInput { prev } => {
                // Uzantı yazılırken → İçerik
                self.mode = Mode::Search;
                self.query = prev;
                self.ext = None;
                self.content = true;
            }
            Mode::Search if self.content_query().is_some() => {
                if self.query.starts_with('>') {
                    self.query = self.query[1..].trim_start().to_string();
                }
                self.content = false;
                self.ext = None;
            }
            Mode::Search if self.ext.is_some() => {
                self.ext = None;
                self.content = true;
            }
            Mode::Search => {
                let prev = std::mem::take(&mut self.query);
                self.mode = Mode::ExtInput { prev };
                self.focus_query = true;
                return;
            }
            _ => return,
        }
        self.focus_query = true;
        self.items.clear();
        self.research();
    }

    /// Uzantı yazma satırını bitir: `ok` ise yazılan uzantıyı uygula (Enter / "Süz" ipucu).
    pub fn commit_ext_input(&mut self, ok: bool) {
        let Mode::ExtInput { prev } = self.mode.clone() else { return };
        let typed = util::fold(self.query.trim().trim_start_matches('.').trim());
        self.mode = Mode::Search;
        self.query = prev;
        if ok && !typed.is_empty() {
            if typed.chars().all(|c| c.is_alphanumeric()) {
                self.ext = Some(typed);
            } else {
                self.toast("Geçersiz uzantı", false);
            }
        }
        self.focus_query = true;
        self.research();
    }

    /// Geri Dönüşüm Kutusu görünümündeki menü (Tab ve sağ tık aynı; kalıcı silme yok).
    fn trash_menu(&self) -> Vec<MEntry> {
        let it = |key, label, short, act| MEntry::Item(MItem { key, label, short, act });
        if self.trash_sel().is_none() {
            return vec![it('e', "Gezgin'de aç", "", Act::TrashExplorer)];
        }
        vec![
            it('g', "Geri yükle", "", Act::Restore),
            it('a', "Aç", "Enter", Act::Open),
            it('o', "Özgün klasörü aç", "", Act::OrigReveal),
            MEntry::Sep,
            it('p', "Özgün yolu kopyala", "", Act::OrigPath),
            it('e', "Gezgin'de aç", "", Act::TrashExplorer),
        ]
    }

    /// Sanal "Geri Dönüşüm Kutusu" öğesinin menüsü.
    fn trash_entry_menu(&self) -> Vec<MEntry> {
        let it = |key, label, short, act| MEntry::Item(MItem { key, label, short, act });
        vec![it('a', "Aç", "Enter", Act::Open), it('e', "Gezgin'de aç", "Alt+Enter", Act::Reveal)]
    }

    pub fn tab_menu(&self) -> Vec<MEntry> {
        if self.mode == Mode::Trash {
            return self.trash_menu();
        }
        if self.trash_entry_sel() {
            return self.trash_entry_menu();
        }
        let it = |key, label, act| MEntry::Item(MItem { key, label, short: "", act });
        let sc = |key, label, short, act| MEntry::Item(MItem { key, label, short, act });
        if self.mode == Mode::Bin && self.items.is_empty() {
            // boş Bin: öğe işlemleri yok; genel işlemler (önceki klasörde çalışır)
            return vec![it('n', "Yeni", Act::New), MEntry::Sep, it('h', "PATH'e ekle", Act::AddPath), it('g', "Son hedefler", Act::Dests)];
        }
        if self.mode == Mode::Bin {
            return vec![it('r', "Yeniden adlandır", Act::BinRename), it('d', "Kaldır", Act::BinRemove), it('a', "Hedefi aç", Act::Open), it('e', "Gezgin'de göster", Act::Reveal), MEntry::Sep, it('h', "PATH'e ekle", Act::AddPath)];
        }
        let mut v = vec![sc(' ', if self.multi { "Çoklu seçimi kapat" } else { "Çoklu seçim" }, "Ctrl+M", Act::Multi)];
        if self.multi {
            v.push(sc('o', "Seçilenleri aç", "Enter", Act::OpenAll));
            v.push(sc('t', "Tümünü seç", "Ctrl+A", Act::SelectAll));
        }
        v.push(MEntry::Sep);
        v.extend([
            it('c', "Kopyala", Act::Copy),
            it('x', "Kes", Act::Cut),
            it('v', "Yapıştır", Act::Paste),
            it('n', "Yeni", Act::New),
            it('r', "Yeniden adlandır", Act::Rename),
            it('d', "Geri Dönüşüm Kutusu'na gönder", Act::Delete),
            it('b', "Bin'e yolla", Act::Bin),
            it('p', "Yolu kopyala", Act::CopyPath),
            MEntry::Sep,
            it('g', "Son hedefler", Act::Dests),
            it('m', "Bin yöneticisi", Act::BinMgr),
            it('k', "Panoyu temizle", Act::ClearClip),
        ]);
        v
    }

    /// Sağ tık menüsü: ayrı bir menü; Aç / Önizle / Explorer'da aç en üstte, Tab menüsündeki
    /// her işlem de burada (fareyle her şey yapılabilsin).
    pub fn context_menu(&self) -> Vec<MEntry> {
        if self.mode == Mode::Trash {
            return self.trash_menu();
        }
        if self.trash_entry_sel() {
            return self.trash_entry_menu();
        }
        let it = |key, label, short, act| MEntry::Item(MItem { key, label, short, act });
        if self.mode == Mode::Bin && self.items.is_empty() {
            return vec![it('n', "Yeni", "", Act::New), MEntry::Sep, it('h', "PATH'e ekle", "", Act::AddPath), it('g', "Son hedefler", "", Act::Dests)];
        }
        if self.mode == Mode::Bin {
            return vec![
                it('a', "Hedefi aç", "Enter", Act::Open),
                it('e', "Gezgin'de göster", "Alt+Enter", Act::Reveal),
                MEntry::Sep,
                it('r', "Yeniden adlandır", "F2", Act::BinRename),
                it('d', "Kaldır", "Del", Act::BinRemove),
                MEntry::Sep,
                it('h', "PATH'e ekle", "", Act::AddPath),
            ];
        }
        let mut v = vec![
            it('a', "Aç", "Enter", Act::Open),
            it('o', "Önizle", "Shift+Space", Act::Preview),
            it('e', "Explorer'da aç", "Alt+Enter", Act::Reveal),
            MEntry::Sep,
            it(' ', if self.multi { "Çoklu seçimi kapat" } else { "Çoklu seçim" }, "Ctrl+M", Act::Multi),
        ];
        if self.multi {
            v.push(it('s', "Seçilenleri aç", "", Act::OpenAll));
            v.push(it('t', "Tümünü seç", "Ctrl+A", Act::SelectAll));
        }
        v.extend([
            MEntry::Sep,
            it('c', "Kopyala", "Ctrl+C", Act::Copy),
            it('x', "Kes", "Ctrl+X", Act::Cut),
            it('v', "Yapıştır", "Ctrl+V", Act::Paste),
            it('r', "Yeniden adlandır", "F2", Act::Rename),
            it('p', "Yolu kopyala", "Ctrl+Shift+C", Act::CopyPath),
            it('n', "Yeni", "", Act::New),
            it('d', "Geri Dönüşüm Kutusu'na gönder", "Del", Act::Delete),
            it('b', "Bin'e yolla", "", Act::Bin),
            MEntry::Sep,
            it('g', "Son hedefler", "", Act::Dests),
            it('m', "Bin yöneticisi", "", Act::BinMgr),
            it('k', "Panoyu temizle", "", Act::ClearClip),
            it('y', "Yenile", "F5", Act::Refresh),
        ]);
        v
    }

    pub fn open_menu(&mut self, context: bool, pos: Pos2) {
        let entries = if context { self.context_menu() } else { self.tab_menu() };
        // menü açıldığında Shift zaten basılıysa (ör. Shift+F10) mod değiştirme
        self.menu_shift_prev = true;
        let mut m = Menu { entries, sel: 0, pos, context, scroll: 0 };
        if !matches!(m.entries.first(), Some(MEntry::Item(_))) {
            m.step(1);
        }
        // Tab menüsünde en üstteki Çoklu seçim grubu atlanır: seçim ilk dosya işleminde başlar
        // (Tab→Enter eskisi gibi Kopyala).
        if !context && let Some(i) = m.entries.iter().position(|e| matches!(e, MEntry::Item(it) if it.act == Act::Copy)) {
            m.sel = i;
        }
        self.menu = Some(m);
    }

    // ---------------- Klavye ----------------

    /// Tuşları widget'lardan önce işle (tükettiğimiz tuşlar arama kutusuna gitmez).
    pub fn handle_keys(&mut self, ctx: &Context, row_anchor: impl Fn(usize) -> Pos2) {
        let mut acts: Vec<Act> = Vec::new();
        let mut escape = false;
        if !self.key_carry.is_empty() {
            // önceki kareden kalan tuşlar bu karenin tuşlarından önce gelir
            let f = std::mem::take(&mut self.key_carry);
            ctx.input_mut(|i| {
                i.events.splice(0..0, f);
            });
        }

        // Onay penceresi
        if self.confirm.is_some() {
            let (mut mv, mut ok, mut esc) = (0isize, false, false);
            ctx.input_mut(|i| {
                if i.consume_key(Modifiers::NONE, Key::ArrowLeft) || i.consume_key(Modifiers::SHIFT, Key::Tab) {
                    mv = -1;
                }
                if i.consume_key(Modifiers::NONE, Key::ArrowRight) || i.consume_key(Modifiers::NONE, Key::Tab) {
                    mv = 1;
                }
                ok = i.consume_key(Modifiers::NONE, Key::Enter) || i.consume_key(Modifiers::NONE, Key::Space);
                esc = i.consume_key(Modifiers::NONE, Key::Escape);
                i.events.retain(|e| !matches!(e, Event::Text(_) | Event::Key { .. }));
            });
            if let Some(c) = &mut self.confirm {
                let n = c.buttons.len() as isize;
                c.sel = (c.sel as isize + mv).rem_euclid(n) as usize;
            }
            if ok {
                let s = self.confirm.as_ref().map(|c| c.sel);
                self.confirm_answer(s);
            } else if esc {
                self.confirm_answer(None);
            }
            return;
        }

        // Menü — tuşlar geldikleri sırayla işlenir: aynı karede birden çok tuş (ör. hızlı Tab Tab)
        // kaybolmaz; menüyü kapatan/çalıştıran tuştan sonrakiler bir sonraki kareye aktarılır.
        if self.menu.is_some() {
            #[derive(Clone, Copy)]
            enum Op {
                Nav(isize),
                Close,
                Enter,
                Letter(char),
            }
            let mut ops: Vec<Op> = Vec::new();
            let mut carry: Vec<Event> = Vec::new();
            let mut to_crumbs = false;
            let tab_menu = self.menu.as_ref().is_some_and(|m| !m.context);
            let browsing = self.browse.is_some() && self.mode == Mode::Search;
            let keys: Vec<char> = self.menu.as_ref().map(|m| m.entries.iter().filter_map(|e| if let MEntry::Item(it) = e { Some(it.key) } else { None }).collect()).unwrap_or_default();
            ctx.input_mut(|i| {
                // Tab menüsü açıkken Shift'e basmak: yol şeridi moduna geç
                let shift = i.modifiers.shift;
                if tab_menu && browsing && shift && !self.menu_shift_prev {
                    to_crumbs = true;
                }
                self.menu_shift_prev = shift;
                let mut stop = false;
                let mut rest = Vec::new();
                for e in std::mem::take(&mut i.events) {
                    let is_input = matches!(e, Event::Text(_) | Event::Key { pressed: true, .. });
                    if !is_input {
                        rest.push(e);
                        continue;
                    }
                    if stop {
                        carry.push(e);
                        continue;
                    }
                    match &e {
                        Event::Key { key, modifiers, .. } if modifiers.matches_logically(Modifiers::NONE) => match key {
                            Key::ArrowUp => ops.push(Op::Nav(-1)),
                            Key::ArrowDown => ops.push(Op::Nav(1)),
                            Key::Escape | Key::Tab => {
                                ops.push(Op::Close);
                                stop = true;
                            }
                            Key::Enter => {
                                ops.push(Op::Enter);
                                stop = true;
                            }
                            _ => {}
                        },
                        Event::Text(t) => {
                            // harf: doğrudan çalıştır (Tab→v, Tab→n, Tab→e …); eşleşmeyen harfler yok sayılır
                            for c in t.chars() {
                                let c = c.to_lowercase().next().unwrap_or(c);
                                if keys.contains(&c) {
                                    ops.push(Op::Letter(c));
                                    stop = true;
                                    break;
                                }
                            }
                        }
                        _ => {}
                    }
                }
                i.events = rest;
            });
            if to_crumbs {
                self.menu = None;
                if let Some(b) = &self.browse {
                    self.crumb_sel = Some(crumb_segments(b).len() - 1);
                }
                return;
            }
            if !carry.is_empty() {
                self.key_carry.splice(0..0, carry);
                ctx.request_repaint();
            }
            let m = self.menu.as_mut().unwrap();
            let mut run = false;
            for op in ops {
                match op {
                    Op::Nav(dy) => m.step(dy),
                    Op::Close => {
                        self.menu = None;
                        return;
                    }
                    Op::Enter => run = true,
                    Op::Letter(c) => {
                        if let Some(pos) = m.entries.iter().position(|e| matches!(e, MEntry::Item(it) if it.key == c)) {
                            m.sel = pos;
                            run = true;
                        }
                    }
                }
            }
            if run && let Some(a) = m.item(m.sel).map(|i| i.act) {
                self.run(a, ctx);
            }
            return;
        }

        // Ayarlar paneli
        if self.mode == Mode::Settings {
            self.settings_keys(ctx);
            return;
        }

        // Not al: Enter kaydeder, Shift+Enter yeni satır, Esc çıkar
        if matches!(self.mode, Mode::Note { .. }) {
            let (mut ok, mut esc) = (false, false);
            ctx.input_mut(|i| {
                if !i.modifiers.shift && !i.modifiers.ctrl && !i.modifiers.alt && !i.modifiers.command {
                    ok = i.consume_key(Modifiers::NONE, Key::Enter);
                }
                esc = i.consume_key(Modifiers::NONE, Key::Escape);
                i.consume_key(Modifiers::NONE, Key::Tab);
                // Ctrl+V: çok satırlı metni olduğu gibi yapıştır
                if i.consume_key(Modifiers::COMMAND, Key::V)
                    && let Some(t) = ops::get_clipboard_text()
                {
                    i.events.push(Event::Paste(t));
                }
            });
            if ok {
                self.save_note();
            } else if esc {
                self.leave_note();
            }
            return;
        }

        // Uzantı süzgeci yazılıyor
        if matches!(self.mode, Mode::ExtInput { .. }) {
            let (mut ok, mut esc, mut f1) = (false, false, false);
            ctx.input_mut(|i| {
                ok = i.consume_key(Modifiers::NONE, Key::Enter);
                esc = i.consume_key(Modifiers::NONE, Key::Escape);
                i.consume_key(Modifiers::NONE, Key::Tab);
                f1 = i.consume_key(Modifiers::NONE, Key::F1);
            });
            if f1 {
                // Uzantı → İçerik
                self.cycle_mode();
                return;
            }
            if ok || esc {
                self.commit_ext_input(ok);
            }
            return;
        }

        // Satır içi düzenleme
        if self.edit.is_some() {
            let (mut ok, mut esc) = (false, false);
            ctx.input_mut(|i| {
                ok = i.consume_key(Modifiers::NONE, Key::Enter);
                esc = i.consume_key(Modifiers::NONE, Key::Escape);
                i.consume_key(Modifiers::NONE, Key::Tab);
                i.consume_key(Modifiers::NONE, Key::ArrowUp);
                i.consume_key(Modifiers::NONE, Key::ArrowDown);
                // Ctrl+V: metin yapıştır
                if i.consume_key(Modifiers::COMMAND, Key::V)
                    && let Some(t) = ops::get_clipboard_text()
                {
                    i.events.push(Event::Paste(t));
                }
            });
            if ok {
                self.commit_edit();
            } else if esc {
                self.cancel_edit();
            }
            return;
        }

        // Yol şeridi klavye modu (Tab menüsünde Shift ile girilir)
        if let (Some(cs), Some(b)) = (self.crumb_sel, self.browse.clone()) {
            let n = crumb_segments(&b).len();
            let (mut mv, mut go, mut esc, mut leave) = (0isize, false, false, false);
            ctx.input_mut(|i| {
                for m in [Modifiers::NONE, Modifiers::SHIFT] {
                    if i.consume_key(m, Key::ArrowLeft) {
                        mv -= 1;
                    }
                    if i.consume_key(m, Key::ArrowRight) {
                        mv += 1;
                    }
                }
                go = i.consume_key(Modifiers::NONE, Key::Enter);
                esc = i.consume_key(Modifiers::NONE, Key::Escape);
                // başka bir tuş/yazı: moddan çık, tuş normal işlensin
                leave = i.events.iter().any(|e| matches!(e, Event::Text(_) | Event::Paste(_)) || matches!(e, Event::Key { pressed: true, key, .. } if !matches!(key, Key::ArrowLeft | Key::ArrowRight)));
            });
            if esc {
                self.crumb_sel = None;
                self.focus_query = true;
                return;
            }
            if go {
                let segs = crumb_segments(&b);
                self.crumb_sel = None;
                self.focus_query = true;
                match segs.get(cs).and_then(|x| x.1.clone()) {
                    Some(p) if p == b => {}
                    Some(p) => {
                        self.reselect = segs.get(cs + 1).and_then(|x| x.1.clone());
                        let keep = self.reselect.clone();
                        self.enter_browse(&p);
                        self.reselect = keep;
                    }
                    None => self.leave_browse(),
                }
                return;
            }
            if mv != 0 {
                self.crumb_sel = Some((cs as isize + mv).clamp(0, n as isize - 1) as usize);
            }
            if !leave {
                return;
            }
            self.crumb_sel = None;
        }

        let rows = self.visible_rows() as isize;
        let mut mv: isize = 0;
        let mut ext_mv: isize = 0;
        let mut to_end: Option<bool> = None;
        let mut enter: Option<bool> = None;
        let mut back_up = false;
        let mut ctx_menu = false;
        let mut sel_all = false;
        let mut space_toggle = false;
        let multi = self.multi && self.mode == Mode::Search;
        ctx.input_mut(|i| {
            // önce Shift'li: consume_key(NONE) Shift'i yok sayar
            if i.consume_key(Modifiers::SHIFT, Key::ArrowDown) {
                ext_mv += 1;
            }
            if i.consume_key(Modifiers::SHIFT, Key::ArrowUp) {
                ext_mv -= 1;
            }
            // Çoklu seçim: Boşluk işaretler (arama kutusuna boşluk yazılmaz)
            if multi && !i.modifiers.shift && !i.modifiers.command && i.consume_key(Modifiers::NONE, Key::Space) {
                space_toggle = true;
                i.events.retain(|e| !matches!(e, Event::Text(t) if t == " "));
            }
            if i.consume_key(Modifiers::COMMAND, Key::M) {
                acts.push(Act::Multi);
            }
            if i.consume_key(Modifiers::COMMAND, Key::N) {
                acts.push(Act::Note);
            }
            if i.consume_key(Modifiers::COMMAND, Key::T) {
                acts.push(Act::Terminal);
            }
            if i.consume_key(Modifiers::NONE, Key::ArrowDown) {
                mv += 1;
            }
            if i.consume_key(Modifiers::NONE, Key::ArrowUp) {
                mv -= 1;
            }
            // Ctrl+← : üst klasör (arama kutusunda kelime atlama yapmaz)
            if i.consume_key(Modifiers::COMMAND, Key::ArrowLeft) {
                back_up = true;
            }
            // fare geri düğmesi
            if i.pointer.button_pressed(egui::PointerButton::Extra1) {
                back_up = true;
            }
            // Ctrl+A: arama kutusunda yazılan metni seçer (kutu işler); yalnızca
            // Çoklu seçim modunda listedeki tüm öğeleri seçer.
            if multi && i.consume_key(Modifiers::COMMAND, Key::A) {
                sel_all = true;
            }
            if i.consume_key(Modifiers::NONE, Key::PageDown) {
                mv += rows;
            }
            if i.consume_key(Modifiers::NONE, Key::PageUp) {
                mv -= rows;
            }
            if i.consume_key(Modifiers::NONE, Key::Home) {
                to_end = Some(false);
            }
            if i.consume_key(Modifiers::NONE, Key::End) {
                to_end = Some(true);
            }
            if i.consume_key(Modifiers::ALT, Key::Enter) {
                enter = Some(true);
            } else if i.consume_key(Modifiers::NONE, Key::Enter) {
                enter = Some(false);
            }
            if i.consume_key(Modifiers::NONE, Key::Escape) {
                escape = true;
            }
            // aynı karede birden çok Tab: tek sayıda ise menü açık kalır (aç-kapa-aç …)
            if i.count_and_consume_key(Modifiers::NONE, Key::Tab) % 2 == 1 {
                acts.push(Act::Dests); // yer tutucu: Tab menüsü
            }
            if i.consume_key(Modifiers::SHIFT, Key::Space) {
                acts.push(Act::Preview);
                i.events.retain(|e| !matches!(e, Event::Text(t) if t == " "));
            }
            if i.consume_key(Modifiers::NONE, Key::F2) {
                acts.push(Act::Rename);
            }
            if i.consume_key(Modifiers::NONE, Key::F1) {
                acts.push(Act::CycleMode);
            }
            if i.consume_key(Modifiers::COMMAND, Key::S) {
                acts.push(Act::Settings);
            }
            if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::C) {
                acts.push(Act::CopyPath);
            } else if i.consume_key(Modifiers::COMMAND, Key::C) {
                acts.push(Act::Copy);
            }
            if i.consume_key(Modifiers::COMMAND, Key::X) {
                acts.push(Act::Cut);
            }
            if i.consume_key(Modifiers::COMMAND, Key::V) {
                acts.push(Act::Paste);
            }
            if i.consume_key(Modifiers::SHIFT, Key::F10) {
                ctx_menu = true;
            }
            if i.consume_key(Modifiers::NONE, Key::Delete) {
                acts.push(Act::Delete);
            }
            if i.consume_key(Modifiers::NONE, Key::F5) {
                self.idx.start(true);
            }
        });
        if sel_all {
            self.select_all();
        }
        if space_toggle && !self.items.is_empty() {
            self.toggle_mark(self.sel);
        }
        if mv != 0 {
            self.step_sel(mv, false);
        }
        if ext_mv != 0 {
            self.step_sel(ext_mv, true);
        }
        match to_end {
            Some(false) => self.step_sel(-(self.sel as isize), false),
            Some(true) => self.step_sel(self.items.len() as isize - 1 - self.sel as isize, false),
            None => {}
        }
        if let Some(alt) = enter {
            if multi && !alt && self.marked.len() > 1 {
                self.open_all(ctx);
            } else {
                self.activate(self.sel, alt, ctx);
            }
        }
        if back_up {
            self.browse_up_pub();
        }
        if ctx_menu && !self.items.is_empty() {
            let p = row_anchor(self.sel);
            self.open_menu(true, p);
        }
        for a in acts {
            match a {
                Act::Dests => {
                    let p = row_anchor(self.sel);
                    self.open_menu(false, p);
                }
                Act::Paste if self.mode != Mode::Search => {}
                Act::Multi | Act::Note | Act::Terminal | Act::CycleMode | Act::SelectAll if self.mode != Mode::Search => {}
                Act::Paste => {
                    let has_files = self.clip.is_some() || ops::get_clipboard_files().is_some();
                    if has_files {
                        self.paste();
                    } else if let Some(t) = ops::get_clipboard_text() {
                        let t = t.replace(['\r', '\n'], " ");
                        ctx.input_mut(|i| i.events.push(Event::Paste(t)));
                    } else {
                        self.paste();
                    }
                }
                Act::Delete if self.mode == Mode::Bin => self.run(Act::BinRemove, ctx),
                Act::Delete if self.mode != Mode::Search => {}
                Act::Copy | Act::Cut | Act::Rename | Act::Bin if self.mode == Mode::Bin => {
                    if a == Act::Rename {
                        self.start_rename();
                    }
                }
                a => self.run(a, ctx),
            }
        }
        if escape {
            self.escape(ctx);
        }
    }

    /// Ayarlar panelinde klavye.
    fn settings_keys(&mut self, ctx: &Context) {
        if self.recording {
            let mut got: Option<(Modifiers, Key)> = None;
            ctx.input_mut(|i| {
                for e in &i.events {
                    if let Event::Key { key, pressed: true, modifiers, .. } = e
                        && got.is_none()
                    {
                        got = Some((*modifiers, *key));
                    }
                }
                i.events.retain(|e| !matches!(e, Event::Text(_) | Event::Key { .. } | Event::Paste(_)));
            });
            if let Some((m, k)) = got {
                if k == Key::Escape && !m.any() {
                    self.cancel_record();
                    return;
                }
                match config::vk_of(k) {
                    Some(vk) => {
                        let mut mods = 0;
                        if m.ctrl || m.command {
                            mods |= config::MOD_CONTROL;
                        }
                        if m.alt {
                            mods |= config::MOD_ALT;
                        }
                        if m.shift {
                            mods |= config::MOD_SHIFT;
                        }
                        self.apply_hotkey(Hotkey { mods, vk });
                    }
                    None => self.toast("Bu tuş kısayol olarak kullanılamaz", false),
                }
            }
            return;
        }
        // "Başka bir program" yolu yazılıyor
        if self.settings_edit.is_some() {
            let (mut ok, mut esc) = (false, false);
            ctx.input_mut(|i| {
                ok = i.consume_key(Modifiers::NONE, Key::Enter);
                esc = i.consume_key(Modifiers::NONE, Key::Escape);
                i.consume_key(Modifiers::NONE, Key::Tab);
                i.consume_key(Modifiers::NONE, Key::ArrowUp);
                i.consume_key(Modifiers::NONE, Key::ArrowDown);
                if i.consume_key(Modifiers::COMMAND, Key::V)
                    && let Some(t) = ops::get_clipboard_text()
                {
                    i.events.push(Event::Paste(t.replace(['\r', '\n'], "")));
                }
            });
            if ok {
                self.commit_terminal_path();
            } else if esc {
                self.settings_edit = None;
            }
            return;
        }
        let (mut mv, mut ok, mut esc, mut lr) = (0isize, false, false, 0isize);
        ctx.input_mut(|i| {
            if i.consume_key(Modifiers::NONE, Key::ArrowUp) || i.consume_key(Modifiers::SHIFT, Key::Tab) {
                mv -= 1;
            }
            if i.consume_key(Modifiers::NONE, Key::ArrowDown) || i.consume_key(Modifiers::NONE, Key::Tab) {
                mv += 1;
            }
            if i.consume_key(Modifiers::NONE, Key::ArrowLeft) {
                lr -= 1;
            }
            if i.consume_key(Modifiers::NONE, Key::ArrowRight) {
                lr += 1;
            }
            ok = i.consume_key(Modifiers::NONE, Key::Enter) || i.consume_key(Modifiers::NONE, Key::Space);
            // Esc ya da Ctrl+S (ayarları açan kısayol) ayarları kapatır
            esc = i.consume_key(Modifiers::NONE, Key::Escape) || i.consume_key(Modifiers::COMMAND, Key::S);
            i.events.retain(|e| !matches!(e, Event::Text(_) | Event::Paste(_)));
        });
        if mv != 0 {
            self.settings_sel = (self.settings_sel as isize + mv).rem_euclid(SETTINGS_ROWS as isize) as usize;
        }
        if lr != 0 && self.settings_sel == 2 {
            self.cycle_terminal(lr);
        }
        if ok {
            self.settings_activate(self.settings_sel);
        }
        if esc {
            self.leave_mode();
        }
    }

    pub fn settings_activate(&mut self, row: usize) {
        self.settings_sel = row;
        match row {
            0 => self.start_record(),
            1 => self.toggle_autostart(),
            _ => self.cycle_terminal(1),
        }
    }

    pub fn esc_state(&self) -> EscState {
        EscState {
            menu: self.menu.is_some(),
            edit: self.edit.is_some(),
            confirm: self.confirm.is_some(),
            crumbs: self.crumb_sel.is_some(),
            note: matches!(self.mode, Mode::Note { .. }),
            preview: self.preview_open,
            multi: self.multi,
            marked: !self.marked.is_empty(),
            sub_mode: matches!(self.mode, Mode::Dests | Mode::Bin | Mode::Settings | Mode::Trash),
            ext_input: matches!(self.mode, Mode::ExtInput { .. }),
            content: self.mode == Mode::Search && self.content_query().is_some(),
            ext: self.ext.is_some(),
            browse: self.browse.is_some(),
        }
    }

    /// Esc: önce etkin katman/mod kapanır (uzantı süzgeci, içerik araması, çoklu seçim, not …);
    /// hiçbir mod yoksa pencere gizlenir.
    pub fn escape(&mut self, ctx: &Context) {
        match esc_action(self.esc_state()) {
            EscAction::CloseMenu => self.menu = None,
            EscAction::CancelEdit => self.cancel_edit(),
            EscAction::CancelConfirm => self.confirm_answer(None),
            EscAction::LeaveCrumbs => self.crumb_sel = None,
            EscAction::LeaveNote => self.leave_note(),
            EscAction::ClosePreview => {
                self.preview_open = false;
                self.fix_scroll();
            }
            EscAction::LeaveMulti => self.set_multi(false),
            EscAction::ClearMarks => self.clear_marks(),
            EscAction::LeaveMode => self.leave_mode(),
            EscAction::LeaveExtInput => {
                if let Mode::ExtInput { prev } = self.mode.clone() {
                    self.mode = Mode::Search;
                    self.query = prev;
                    self.focus_query = true;
                    self.research();
                }
            }
            EscAction::ContentOff => {
                if self.query.starts_with('>') {
                    self.query = self.query[1..].trim_start().to_string();
                }
                self.content = false;
                self.items.clear();
                self.focus_query = true;
                self.research();
            }
            EscAction::ExtOff => {
                self.ext = None;
                self.focus_query = true;
                self.research();
            }
            EscAction::LeaveBrowse => self.leave_browse(),
            EscAction::Hide => platform::hide(ctx),
        }
    }

    /// Her karede: sorgu değiştiyse ara.
    pub fn after_query_edit(&mut self) {
        if self.query != self.last_query {
            self.research();
        }
    }

    pub fn frame_start(&mut self, ctx: &Context) {
        platform::heartbeat();
        // ana pencere gizlendiyse (F9, odak kaybı, tepsi…) açık menü kapanır
        let ep = platform::hide_epoch();
        if ep != self.hide_epoch_seen {
            self.hide_epoch_seen = ep;
            self.menu = None;
        }
        if !self.inited {
            self.inited = true;
            platform::init(ctx, self.config.hotkey);
            if !self.start_hidden {
                platform::show();
            }
        }
        if self.hide_frames > 0 {
            // eframe ilk çizimden sonra pencereyi görünür yapar; --hidden ile hemen gizle
            self.hide_frames -= 1;
            platform::hide(ctx);
        }
        if platform::take_shown() {
            self.on_shown();
            self.hide_frames = 0;
        }
        if platform::take_open_settings() {
            self.open_settings();
        }
        if let Some(h) = self.hk_pending {
            match platform::hotkey_result() {
                Some(true) => {
                    self.hk_pending = None;
                    self.config.hotkey = h;
                    self.config.save();
                    self.toast(format!("Yeni kısayol: {}", h.label()), true);
                }
                Some(false) => {
                    self.hk_pending = None;
                    self.toast(format!("{} başka bir uygulama tarafından kullanılıyor; {} korunuyor", h.label(), self.config.hotkey.label()), false);
                }
                None => ctx.request_repaint_after(Duration::from_millis(50)),
            }
        }
        self.poll(ctx);
    }

    pub fn frame_end(&mut self, ctx: &Context) {
        let focused = ctx.input(|i| i.viewport().focused);
        if focused == Some(true) {
            self.got_focus = true;
        }
        // Odak kaybında gizle — yalnızca gösterildikten sonra odak gerçekten alındıysa ve kısa
        // bir bekleme geçtiyse (gösterme anındaki eski odak durumu pencereyi hemen gizlemesin).
        if self.was_focused == Some(true)
            && focused == Some(false)
            && self.got_focus
            && self.shown_at.elapsed() > Duration::from_millis(400)
            && !self.recording
            && self.confirm.is_none()
            && std::env::var_os("BUL_NOHIDE").is_none()
        {
            self.menu = None;
            self.got_focus = false;
            platform::hide_if_unfocused(ctx);
        }
        self.was_focused = focused;
        if self.busy() {
            ctx.request_repaint_after(Duration::from_millis(30));
        } else {
            ctx.request_repaint_after(Duration::from_millis(1000));
        }
    }
}

#[cfg(test)]
#[path = "ui_tests.rs"]
mod ui_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn esc_exits_modes_before_hiding() {
        let none = EscState::default();
        assert_eq!(esc_action(none), EscAction::Hide);
        assert_eq!(esc_action(EscState { ext: true, ..none }), EscAction::ExtOff);
        assert_eq!(esc_action(EscState { content: true, ..none }), EscAction::ContentOff);
        assert_eq!(esc_action(EscState { content: true, ext: true, ..none }), EscAction::ContentOff);
        assert_eq!(esc_action(EscState { ext_input: true, ext: true, ..none }), EscAction::LeaveExtInput);
        assert_eq!(esc_action(EscState { multi: true, marked: true, ext: true, ..none }), EscAction::LeaveMulti);
        assert_eq!(esc_action(EscState { marked: true, ..none }), EscAction::ClearMarks);
        assert_eq!(esc_action(EscState { note: true, browse: true, ..none }), EscAction::LeaveNote);
        assert_eq!(esc_action(EscState { confirm: true, multi: true, ..none }), EscAction::CancelConfirm);
        assert_eq!(esc_action(EscState { menu: true, ext: true, ..none }), EscAction::CloseMenu);
        assert_eq!(esc_action(EscState { browse: true, ..none }), EscAction::LeaveBrowse);
        assert_eq!(esc_action(EscState { sub_mode: true, ..none }), EscAction::LeaveMode);
    }
    #[test]
    fn bin_dialog_defaults_to_remove() {
        let c = bin_remove_confirm("rapor".into());
        assert_eq!(c.buttons[c.sel], "Kaldır");
        assert!(matches!(c.kind, CKind::BinRemove { ref alias } if alias == "rapor"));
    }
    #[test]
    fn new_item_name_logic() {
        assert!(new_item_is_file("notlar.txt"));
        assert!(new_item_is_file("rapor.final.pdf"));
        assert!(new_item_is_file(".gitignore"));
        assert!(!new_item_is_file("Yeni klasör"));
        assert!(!new_item_is_file("proje"));
        assert!(!new_item_is_file("sonu nokta."));
        assert!(!new_item_is_file("v1. taslak"));
        assert!(new_item_is_file("  a.md  "));
    }
    #[test]
    fn crumbs() {
        let segs = crumb_segments(if cfg!(windows) { "C:\\Users\\kullanici" } else { "/home/kullanici" });
        assert_eq!(segs[0].0, "Tümü");
        assert!(segs[0].1.is_none());
        assert_eq!(segs.last().unwrap().0, "kullanici");
        assert_eq!(segs.len(), 4);
    }
}
