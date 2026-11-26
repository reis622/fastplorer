//! Etkileşim testleri: gerçek `App` + `view::draw`, başsız egui bağlamında klavye/fare olaylarıyla.
//! Ortam değişkenleri (veri klasörü, dizin kökleri) diğer testleri etkilemesin diye her senaryo
//! test ikilisinin kendisini ayrı bir süreç olarak çalıştırır.
use super::*;
use egui::{Event, Key, Modifiers, RawInput, Rect, pos2, vec2};
use std::path::{Path, PathBuf};

const CHILD: &str = "FP_UI_CHILD";

fn ctrl() -> Modifiers {
    Modifiers { ctrl: true, command: true, ..Default::default() }
}

struct H {
    ctx: Context,
    app: App,
    root: PathBuf,
}

impl H {
    fn new() -> H {
        let root = PathBuf::from(std::env::var("BUL_ROOTS").unwrap());
        let ctx = Context::default();
        let app = App::new(false, Config::default());
        let mut h = H { ctx, app, root };
        h.frame(vec![]);
        h
    }
    fn frame(&mut self, events: Vec<Event>) -> egui::FullOutput {
        let raw = RawInput { screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(PANEL_W, 520.0))), events, ..Default::default() };
        let app = &mut self.app;
        let mut out = self.ctx.run_ui(raw, |ui| {
            let ctx = ui.ctx().clone();
            app.frame_start(&ctx);
            crate::view::draw(app, ui);
            app.frame_end(&ctx);
        });
        out.textures_delta.clear();
        out
    }
    fn key(&mut self, key: Key, modifiers: Modifiers) -> egui::FullOutput {
        let down = Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers };
        let up = Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers };
        let raw = RawInput { screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(PANEL_W, 520.0))), events: vec![Event::ModifiersChanged(modifiers), down], ..Default::default() };
        let app = &mut self.app;
        let mut out = self.ctx.run_ui(raw, |ui| {
            let ctx = ui.ctx().clone();
            app.frame_start(&ctx);
            crate::view::draw(app, ui);
            app.frame_end(&ctx);
        });
        out.textures_delta.clear();
        self.frame(vec![up, Event::ModifiersChanged(Modifiers::NONE)]);
        out
    }
    fn wait(&mut self, what: &str, cond: impl Fn(&App) -> bool) {
        for _ in 0..1500 {
            self.frame(vec![]);
            if cond(&self.app) {
                return;
            }
            std::thread::sleep(Duration::from_millis(15));
        }
        panic!("beklenen durum oluşmadı: {what} · sorgu={:?} öğeler={:?} gözat={:?}", self.app.query, self.names(), self.app.browse);
    }
    fn names(&self) -> Vec<String> {
        self.app.items.iter().map(|i| i.name.clone()).collect()
    }
    fn dir(&self, rel: &str) -> String {
        self.root.join(rel).to_string_lossy().to_string()
    }
}

fn hid(out: &egui::FullOutput) -> bool {
    out.viewport_output.values().any(|v| v.commands.iter().any(|c| matches!(c, egui::ViewportCommand::Minimized(true) | egui::ViewportCommand::Visible(false))))
}

fn write(p: &Path, s: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, s).unwrap();
}

/// Senaryoyu ayrı süreçte çalıştırır (yalıtılmış veri klasörü + dizin kökü).
fn run_child(name: &str) {
    if std::env::var_os(CHILD).is_some() {
        return;
    }
    let base = std::env::temp_dir().join(format!("fp-ui-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join("kök");
    write(&root.join("Belgeler/rapor.pdf"), "%PDF");
    write(&root.join("Belgeler/2024/fatura.pdf"), "%PDF");
    write(&root.join("Belgeler/2024/Ocak/makbuz.pdf"), "%PDF");
    write(&root.join("Belgeler/notlar.txt"), "merhaba");
    write(&root.join("Belgeler/rapor taslak.docx"), "x");
    write(&root.join("Resimler/tatil.png"), "x");
    write(&root.join("Resimler/kapak.pdf"), "%PDF");
    for i in 0..6 {
        write(&root.join(format!("Liste/dosya{i}.txt")), "x");
    }
    write(&root.join("Masaüstü/kısayol.txt"), "x");
    // sahte Geri Dönüşüm Kutusu (dizin kökünün dışında: aranmaz)
    let kutu = base.join("kutu");
    let now = crate::util::now_secs();
    let rec = |id: &str, orig: &Path, ago: i64, dir: bool| {
        std::fs::create_dir_all(&kutu).unwrap();
        std::fs::write(kutu.join(format!("$I{id}")), crate::recycle::encode_i_v2(&orig.to_string_lossy(), 5, now - ago)).unwrap();
        if dir {
            write(&kutu.join(format!("$R{id}")).join("ic.txt"), "x");
        } else {
            std::fs::write(kutu.join(format!("$R{id}")), format!("silinmiş {id}")).unwrap();
        }
    };
    rec("A1.txt", &root.join("Belgeler/eski.txt"), 3000, false);
    rec("B2.txt", &root.join("Belgeler/notlar.txt"), 60, false);
    rec("C3", &root.join("Silinen/proje"), 600, true);
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &format!("app::ui_tests::{name}_child"), "--include-ignored", "--nocapture", "--test-threads=1"])
        .env(CHILD, "1")
        .env("BUL_ROOTS", &root)
        .env("XDG_DATA_HOME", base.join("veri"))
        .env("LOCALAPPDATA", base.join("veri"))
        .env("HOME", &base)
        .env("BUL_RECYCLE_DIRS", base.join("kutu"))
        .env("BUL_DESKTOP", root.join("Masaüstü"))
        .env_remove("BUL_NOHIDE")
        .env_remove("BUL_LANG")
        .output()
        .unwrap();
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let _ = std::fs::remove_dir_all(&base);
    assert!(out.status.success() && text.contains("1 passed"), "{name} senaryosu başarısız:\n{text}");
}

macro_rules! scenario {
    ($name:ident, $child:ident, $body:expr) => {
        #[test]
        fn $name() {
            run_child(stringify!($name));
        }
        #[test]
        #[ignore = "run_child tarafından ayrı süreçte çalıştırılır"]
        fn $child() {
            if std::env::var_os(CHILD).is_none() {
                return;
            }
            let f: fn(&mut H) = $body;
            let mut h = H::new();
            f(&mut h);
        }
    };
}

scenario!(ctrl_a_in_search_box_selects_text, ctrl_a_in_search_box_selects_text_child, |h| {
    h.frame(vec![Event::Text("pdf".into())]);
    h.wait("arama sonuçları", |a| a.query == "pdf" && a.items.len() >= 3);
    h.key(Key::A, ctrl());
    assert!(h.app.marked.is_empty(), "Ctrl+A arama kutusundayken dosya seçmemeli");
    let st = egui::TextEdit::load_state(&h.ctx, egui::Id::unique("bul-q")).expect("arama kutusu durumu");
    let r = st.cursor.char_range().expect("metin seçimi");
    let (a, b) = (r.primary.index.0.min(r.secondary.index.0), r.primary.index.0.max(r.secondary.index.0));
    assert_eq!((a, b), (0, 3), "Ctrl+A arama metninin tamamını seçmeli");
    assert_eq!(h.app.query, "pdf");
});

scenario!(esc_leaves_ext_and_content_first, esc_leaves_ext_and_content_first_child, |h| {
    h.app.ext = Some("pdf".into());
    h.app.research();
    h.frame(vec![]);
    let out = h.key(Key::Escape, Modifiers::NONE);
    assert!(h.app.ext.is_none(), "Esc uzantı süzgecini kapatmalı");
    assert!(!hid(&out), "uzantı modunda Esc pencereyi gizlememeli");
    h.app.content = true;
    h.app.research();
    h.frame(vec![]);
    let out = h.key(Key::Escape, Modifiers::NONE);
    assert!(!h.app.content, "Esc içerik aramasını kapatmalı");
    assert!(!hid(&out), "içerik modunda Esc pencereyi gizlememeli");
    let out = h.key(Key::Escape, Modifiers::NONE);
    assert!(hid(&out), "mod yokken Esc pencereyi gizler");
});

scenario!(empty_query_ext_lists_scope, empty_query_ext_lists_scope_child, |h| {
    let dir = h.dir("Belgeler");
    h.app.enter_browse(&dir);
    h.app.ext = Some("pdf".into());
    h.app.research();
    h.wait("pdf listesi", |a| a.items.len() == 3);
    let mut n = h.names();
    n.sort();
    assert_eq!(n, ["fatura.pdf", "makbuz.pdf", "rapor.pdf"], "alt klasörler dahil tüm pdf'ler; kapsam dışı (Resimler/kapak.pdf) yok");
});

scenario!(multi_select_mode, multi_select_mode_child, |h| {
    let dir = h.dir("Liste");
    h.app.enter_browse(&dir);
    h.wait("liste", |a| a.items.len() == 6);
    h.key(Key::M, ctrl());
    assert!(h.app.multi, "Ctrl+M çoklu seçimi açar");
    h.key(Key::Space, Modifiers::NONE);
    h.key(Key::ArrowDown, Modifiers::NONE);
    h.key(Key::ArrowDown, Modifiers::NONE);
    h.key(Key::Space, Modifiers::NONE);
    assert_eq!(h.app.marked.len(), 2, "Boşluk işaretler; ok tuşları işaretleri korur");
    h.key(Key::Space, Modifiers::NONE);
    assert_eq!(h.app.marked.len(), 1, "Boşluk tekrar basınca işareti kaldırır");
    h.key(Key::A, ctrl());
    assert_eq!(h.app.marked.len(), 6, "çoklu seçimde Ctrl+A hepsini seçer");
    h.key(Key::Escape, Modifiers::NONE);
    assert!(!h.app.multi && h.app.marked.is_empty(), "Esc çoklu seçimden çıkar");
});

scenario!(shift_arrows_range, shift_arrows_range_child, |h| {
    let dir = h.dir("Liste");
    h.app.enter_browse(&dir);
    h.wait("liste", |a| a.items.len() == 6);
    h.key(Key::ArrowDown, Modifiers::NONE);
    h.key(Key::ArrowDown, Modifiers::NONE);
    assert_eq!(h.app.sel, 2);
    h.key(Key::ArrowDown, Modifiers::SHIFT);
    h.key(Key::ArrowDown, Modifiers::SHIFT);
    assert_eq!(h.app.marked.len(), 3);
    h.key(Key::ArrowUp, Modifiers::SHIFT);
    h.key(Key::ArrowUp, Modifiers::SHIFT);
    h.key(Key::ArrowUp, Modifiers::SHIFT);
    h.key(Key::ArrowUp, Modifiers::SHIFT);
    // çapa (2) korunur: seçim yukarı doğru 0..=2 olur
    assert_eq!(h.app.sel, 0);
    let mut idx: Vec<usize> = h.app.items.iter().enumerate().filter(|(_, i)| h.app.marked.contains(&i.path)).map(|(k, _)| k).collect();
    idx.sort();
    assert_eq!(idx, [0, 1, 2], "Shift+ok ters yönde de çapadan seçer");
});

scenario!(tab_menu_arrows_with_mouse_over, tab_menu_arrows_with_mouse_over_child, |h| {
    // liste dolu olsun: boş listede (yük altında dizin henüz hazır değilken) pencere tek satırdır,
    // menü kendi içinde kayar ve fareyle gezilecek ikinci öğe görünmez
    let d = h.dir("Liste");
    h.app.enter_browse(&d);
    h.wait("liste", |a| a.items.len() == 6 && !a.waiting);
    for _ in 0..3 {
        h.frame(vec![]);
    }
    h.key(Key::Tab, Modifiers::NONE);
    let pos = h.app.menu.as_ref().expect("Tab menüsü").pos;
    let start = h.app.menu.as_ref().unwrap().sel;
    // fareyi menü üzerinde aşağı gezdir, bir öğenin üstüne gelince dur
    let mut over = None;
    h.frame(vec![Event::PointerMoved(pos2(pos.x + 60.0, pos.y + 2.0))]);
    for k in 1..20 {
        h.frame(vec![Event::PointerMoved(pos2(pos.x + 60.0, pos.y + 2.0 + k as f32 * 14.0))]);
        let s = h.app.menu.as_ref().unwrap().sel;
        if s != start {
            over = Some(s);
            break;
        }
    }
    let over = over.unwrap_or_else(|| panic!("fare üzerine gelince seçim değişmeli: pos={pos:?} öğe={} satır={} boyut={:?} menü_kaydırma={} sel={} bekliyor={}", h.app.items.len(), h.app.visible_rows(), h.app.last_size, h.app.menu.as_ref().unwrap().scroll, h.app.menu.as_ref().unwrap().sel, h.app.waiting));
    h.key(Key::ArrowDown, Modifiers::NONE);
    let after = h.app.menu.as_ref().unwrap().sel;
    assert!(after > over, "fare menü üzerindeyken ↓ çalışmalı ({over} → {after})");
    for _ in 0..5 {
        h.frame(vec![]);
    }
    assert_eq!(h.app.menu.as_ref().unwrap().sel, after, "fare kıpırdamadıkça seçim değişmemeli");
    h.key(Key::ArrowUp, Modifiers::NONE);
    assert_eq!(h.app.menu.as_ref().unwrap().sel, over);
    // v0.8: Not al / Terminal aç / Ayarlar / Uzantı / İçerik Tab menüsünde yok; Çoklu seçim en üstte (Boşluk)
    let labels: Vec<&str> = h.app.tab_menu().iter().filter_map(|e| if let MEntry::Item(i) = e { Some(i.label) } else { None }).collect();
    for gone in ["Not al", "Terminal aç", "Ayarlar", "Uzantıya göre ara", "İçerikte ara"] {
        assert!(!labels.contains(&gone), "{gone} Tab menüsünde olmamalı: {labels:?}");
    }
    assert!(matches!(h.app.tab_menu().first(), Some(MEntry::Item(i)) if i.label == "Çoklu seçim" && i.key == ' '));
});

scenario!(bin_dialog_keys, bin_dialog_keys_child, |h| {
    h.app.confirm = Some(bin_remove_confirm("rapor".into()));
    h.frame(vec![]);
    assert_eq!(h.app.confirm.as_ref().unwrap().sel, 0, "Kaldır seçili gelir");
    h.key(Key::ArrowRight, Modifiers::NONE);
    assert_eq!(h.app.confirm.as_ref().unwrap().sel, 1);
    h.key(Key::ArrowLeft, Modifiers::NONE);
    assert_eq!(h.app.confirm.as_ref().unwrap().sel, 0);
    let out = h.key(Key::Escape, Modifiers::NONE);
    assert!(h.app.confirm.is_none(), "Esc vazgeçer");
    assert!(!hid(&out));
});

scenario!(note_mode_saves_file, note_mode_saves_file_child, |h| {
    let dir = h.dir("Belgeler");
    h.app.enter_browse(&dir);
    h.frame(vec![]);
    h.key(Key::N, ctrl());
    assert!(matches!(h.app.mode, Mode::Note { .. }), "Ctrl+N not modunu açar");
    h.app.note_text = "ilk satır\nikinci satır ğüşiöç".into();
    h.key(Key::Enter, Modifiers::NONE);
    assert!(!matches!(h.app.mode, Mode::Note { .. }), "Enter kaydeder ve çıkar");
    let notes: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.starts_with("not-") && n.ends_with(".txt")).collect();
    assert_eq!(notes.len(), 1, "{notes:?}");
    let body = std::fs::read_to_string(Path::new(&dir).join(&notes[0])).unwrap();
    assert!(body.contains("ikinci satır ğüşiöç"));
    // ikinci not aynı dakikada: üzerine yazılmaz
    h.key(Key::N, ctrl());
    h.app.note_text = "başka".into();
    h.key(Key::Enter, Modifiers::NONE);
    let n2 = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).filter(|e| e.file_name().to_string_lossy().starts_with("not-")).count();
    assert_eq!(n2, 2);
    // Esc not modundan çıkar, pencereyi gizlemez
    h.key(Key::N, ctrl());
    let out = h.key(Key::Escape, Modifiers::NONE);
    assert!(!matches!(h.app.mode, Mode::Note { .. }) && !hid(&out));
});

scenario!(navigation_records_dests, navigation_records_dests_child, |h| {
    let a = h.dir("Resimler");
    let b = h.dir("Belgeler/2024/Ocak");
    let c = h.dir("Liste");
    let dests = || recent::State::load().dests.into_iter().map(|(p, _)| p).collect::<Vec<_>>();
    h.app.enter_browse(&a);
    h.frame(vec![]);
    h.app.enter_browse(&b);
    h.frame(vec![]);
    let d = dests();
    assert_eq!(d.first(), Some(&b), "en son gidilen en üstte: {d:?}");
    assert!(d.contains(&a), "uzak klasör korunur: {d:?}");
    // Liste, Resimler'in kardeşi (ortak üst 1 seviye): yakın akraba olduğu için Resimler'in yerini alır
    h.app.enter_browse(&c);
    h.frame(vec![]);
    let d = dests();
    assert_eq!(d.first(), Some(&c), "{d:?}");
    assert!(!d.contains(&a) && d.contains(&b), "1-2 seviye yakın klasörler listeyi doldurmaz: {d:?}");
});

fn inner_sizes(out: &egui::FullOutput) -> Vec<egui::Vec2> {
    out.viewport_output.values().flat_map(|v| v.commands.iter().filter_map(|c| if let egui::ViewportCommand::InnerSize(s) = c { Some(*s) } else { None })).collect()
}

scenario!(tab_menu_never_grows_window, tab_menu_never_grows_window_child, |h| {
    for (dir, n) in [("Resimler", 2usize), ("Belgeler/2024/Ocak", 1)] {
        let d = h.dir(dir);
        h.app.enter_browse(&d);
        h.wait("kısa liste", |a| a.items.len() == n);
        for _ in 0..3 {
            h.frame(vec![]);
        }
        let rows = h.app.visible_rows();
        let size = h.app.last_size;
        let out = h.key(Key::Tab, Modifiers::NONE);
        assert!(h.app.menu.is_some(), "Tab menüsü açılır");
        assert_eq!(h.app.visible_rows(), rows, "menü açılınca liste satırı eklenmez ({dir})");
        let mut sizes = inner_sizes(&out);
        // menüde en alttaki öğeye git (Kopyala → Çoklu seçim → yukarı ok sarar): seçili satır görünür kalmalı → kaydırma
        let out = h.key(Key::ArrowUp, Modifiers::NONE);
        sizes.extend(inner_sizes(&out));
        let out = h.key(Key::ArrowUp, Modifiers::NONE);
        sizes.extend(inner_sizes(&out));
        let m = h.app.menu.as_ref().unwrap();
        assert!(m.scroll > 0, "menü pencereye sığmıyor: son öğe için kayar ({dir})");
        let out = h.key(Key::ArrowDown, Modifiers::NONE);
        sizes.extend(inner_sizes(&out));
        assert_eq!(h.app.menu.as_ref().unwrap().scroll, 0, "ilk öğeye dönünce baştan gösterilir");
        assert!(sizes.iter().all(|s| s.y <= size.y + 0.5), "pencere büyümemeli: {size:?} → {sizes:?} ({dir})");
        assert!((h.app.last_size.y - size.y).abs() < 0.5, "pencere boyutu aynı kalmalı: {size:?} → {:?}", h.app.last_size);
        let out = h.key(Key::Escape, Modifiers::NONE);
        assert!(h.app.menu.is_none());
        assert!(inner_sizes(&out).iter().all(|s| (s.y - size.y).abs() < 0.5));
    }
});

// ---------------- v0.7/v0.7.1: menü açma/kapama, boş Bin, en yeni önce ----------------

fn menu_keys(h: &H) -> Vec<char> {
    h.app.menu.as_ref().unwrap().entries.iter().filter_map(|e| if let MEntry::Item(i) = e { Some(i.key) } else { None }).collect()
}


scenario!(tab_menu_repeated_open_close_no_growth, tab_menu_repeated_open_close_no_growth_child, |h| {
    for dir in ["Resimler", "Belgeler/2024/Ocak", "Liste"] {
        let d = h.dir(dir);
        h.app.enter_browse(&d);
        h.wait("liste", |a| !a.items.is_empty() && !a.waiting);
        for _ in 0..3 {
            h.frame(vec![]);
        }
        let size = h.app.last_size;
        let rows = h.app.visible_rows();
        let mut sizes = Vec::new();
        for k in 0..40 {
            let out = h.key(Key::Tab, Modifiers::NONE);
            sizes.extend(inner_sizes(&out));
            assert!(h.app.menu.is_some(), "Tab #{k} açar ({dir})");
            assert_eq!(h.app.visible_rows(), rows);
            let out = h.key(if k % 2 == 0 { Key::ArrowUp } else { Key::ArrowDown }, Modifiers::NONE);
            sizes.extend(inner_sizes(&out));
            let out = h.key(if k % 3 == 0 { Key::Escape } else { Key::Tab }, Modifiers::NONE);
            sizes.extend(inner_sizes(&out));
            assert!(h.app.menu.is_none(), "Tab/Esc #{k} kapatır ({dir})");
            assert_eq!(h.app.mode, Mode::Search, "Esc menüyü kapatır, moddan çıkmaz");
            assert!(h.app.browse.is_some(), "Esc menüyü kapatırken klasörden çıkmaz");
        }
        assert!(sizes.iter().all(|s| (s.y - size.y).abs() < 0.5), "pencere boyutu değişmemeli: {size:?} → {sizes:?}");
        assert!((h.app.last_size - size).length() < 0.5);
    }
});

scenario!(menu_closes_when_main_hides, menu_closes_when_main_hides_child, |h| {
    h.key(Key::Tab, Modifiers::NONE);
    assert!(h.app.menu.is_some());
    crate::platform::hide(&h.ctx);
    h.frame(vec![]);
    assert!(h.app.menu.is_none(), "ana pencere gizlenince menü kapanır");
    h.key(Key::Tab, Modifiers::NONE);
    assert!(h.app.menu.is_some(), "yeniden açılabilir");
});


scenario!(empty_bin_menu_general_items, empty_bin_menu_general_items_child, |h| {
    let d = h.dir("Belgeler");
    h.app.enter_browse(&d);
    h.wait("klasör", |a| !a.items.is_empty());
    h.app.run(Act::BinMgr, &h.ctx.clone());
    h.wait("boş Bin", |a| a.mode == Mode::Bin && a.items.is_empty());
    h.key(Key::Tab, Modifiers::NONE);
    assert!(h.app.menu.is_some(), "boş Bin'de de Tab menüsü açılır");
    let keys = menu_keys(h);
    assert_eq!(keys, vec!['n', 'h', 'g'], "genel işlemler (Not al / Terminal / Ayarlar alttaki çubukta)");
    let labels: Vec<&str> = h.app.menu.as_ref().unwrap().entries.iter().filter_map(|e| if let MEntry::Item(i) = e { Some(i.label) } else { None }).collect();
    assert!(!labels.iter().any(|l| *l == "Yeniden adlandır" || *l == "Kaldır" || *l == "Hedefi aç"), "öğe işlemleri yok: {labels:?}");
    // Yeni: Bin'den çıkar, önceki klasörde ad satırı açılır (liste yenilense de)
    h.frame(vec![Event::Text("n".into())]);
    assert_eq!(h.app.mode, Mode::Search);
    assert_eq!(h.app.browse.as_deref(), Some(d.as_str()));
    h.wait("liste yenilendi", |a| !a.waiting && a.items.len() > 1);
    assert!(matches!(h.app.edit.as_ref().map(|e| &e.kind), Some(EditKind::New { .. })), "Yeni ad satırı");
    assert!(h.app.items[0].path.is_empty(), "yer tutucu satır en üstte");
    h.key(Key::Escape, Modifiers::NONE);
    // dolu Bin'de öğe işlemleri geri gelir
    crate::bin::add("rapor", &h.dir("Belgeler/rapor.pdf")).unwrap();
    h.app.run(Act::BinMgr, &h.ctx.clone());
    h.wait("dolu Bin", |a| a.mode == Mode::Bin && !a.items.is_empty());
    h.key(Key::Tab, Modifiers::NONE);
    assert!(menu_keys(h).contains(&'r') && menu_keys(h).contains(&'d'));
});

scenario!(folder_lists_newest_first, folder_lists_newest_first_child, |h| {
    let d = h.dir("Liste");
    let base = std::time::SystemTime::now() - Duration::from_secs(3600);
    for i in 0..6 {
        let f = std::fs::File::options().write(true).open(Path::new(&d).join(format!("dosya{i}.txt"))).unwrap();
        // dosya2 en yeni, sonra dosya5, …
        let age = match i { 2 => 0, 5 => 10, _ => 100 + i as u64 * 10 };
        f.set_modified(base - Duration::from_secs(age)).unwrap();
    }
    h.app.enter_browse(&d);
    h.wait("liste", |a| a.items.len() == 6 && !a.waiting);
    assert_eq!(&h.names()[..3], ["dosya2.txt", "dosya5.txt", "dosya0.txt"], "{:?}", h.names());
    // Not al: kaydedilen not hemen en üstte
    h.key(Key::N, ctrl());
    h.app.note_text = "yeni not".into();
    h.key(Key::Enter, Modifiers::NONE);
    h.wait("not en üstte", |a| a.items.len() == 7 && a.items[0].name.starts_with("not-"));
    assert_eq!(h.app.sel, 0, "yeni not seçili");
    // Yeni: oluşturulan dosya en üstte
    h.key(Key::Tab, Modifiers::NONE);
    h.frame(vec![Event::Text("n".into())]);
    h.frame(vec![Event::Text("yeni.txt".into())]);
    if let Some(e) = h.app.edit.as_mut() {
        e.text = "yeni.txt".into();
    }
    h.key(Key::Enter, Modifiers::NONE);
    h.wait("yeni dosya en üstte", |a| a.items.len() == 8 && a.items[0].name == "yeni.txt");
    assert_eq!(h.app.sel, 0);
    // Bin listesi de en yeni önce
    crate::bin::add("eski", &h.dir("Resimler/tatil.png")).unwrap();
    let old = std::fs::read_dir(crate::bin::bin_dir()).unwrap().flatten().next().unwrap().path();
    std::fs::File::options().write(true).open(&old).unwrap().set_modified(base).unwrap();
    crate::bin::add("yeni", &h.dir("Resimler/kapak.pdf")).unwrap();
    let l: Vec<String> = crate::bin::list().into_iter().map(|b| b.alias).collect();
    assert_eq!(l, ["yeni", "eski"]);
});

fn presses(key: Key, n: usize) -> Vec<Event> {
    let mut v = Vec::new();
    for _ in 0..n {
        v.push(Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
        v.push(Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE });
    }
    v
}

scenario!(tab_burst_in_one_frame_keeps_parity, tab_burst_in_one_frame_keeps_parity_child, |h| {
    {
        let d = h.dir("Belgeler");
        h.app.enter_browse(&d);
        h.wait("liste", |a| !a.items.is_empty() && !a.waiting);
        for n in 1..=9usize {
            assert!(h.app.menu.is_none());
            // n Tab aynı karede: tek sayıda → açık, çift → kapalı
            h.frame(presses(Key::Tab, n));
            for _ in 0..n + 2 {
                h.frame(vec![]);
            }
            assert_eq!(h.app.menu.is_some(), n % 2 == 1, "{n} Tab");
            if h.app.menu.is_some() {
                // menü açıkken aynı karede Tab + Aşağı + Tab: kapanır, açılır, bir aşağı
                h.frame([presses(Key::Tab, 1), presses(Key::ArrowDown, 1), presses(Key::Tab, 1)].concat());
                for _ in 0..4 {
                    h.frame(vec![]);
                }
                assert!(h.app.menu.is_some());
                h.frame(presses(Key::ArrowDown, 3));
                h.frame(presses(Key::Escape, 1));
                assert!(h.app.menu.is_none());
                assert_eq!(h.app.browse.as_deref(), Some(d.as_str()), "Esc yalnızca menüyü kapatır");
            }
        }
        // menü açıkken: Aşağı×3 aynı karede → üç adım
        h.key(Key::Tab, Modifiers::NONE);
        h.frame(presses(Key::ArrowDown, 3));
        let m = h.app.menu.as_ref().unwrap();
        assert_eq!(m.item(m.sel).unwrap().key, 'n', "c→x→v→n");
        h.key(Key::Escape, Modifiers::NONE);
    }
});

// ---------------- v0.8 ----------------

fn wheel(pos: egui::Pos2, lines: f32) -> Vec<Event> {
    vec![
        Event::PointerMoved(pos),
        Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta: vec2(0.0, lines), phase: egui::TouchPhase::Move, source: egui::MouseWheelSource::Wheel, modifiers: Modifiers::NONE },
    ]
}

fn labels(v: &[MEntry]) -> Vec<&'static str> {
    v.iter().filter_map(|e| if let MEntry::Item(i) = e { Some(i.label) } else { None }).collect()
}

fn acts(v: &[MEntry]) -> Vec<Act> {
    v.iter().filter_map(|e| if let MEntry::Item(i) = e { Some(i.act) } else { None }).collect()
}

scenario!(wheel_scrolls_home_list, wheel_scrolls_home_list_child, |h| {
    // açılıştaki ana liste (boş arama: son kullanılanlar)
    h.wait("ana liste dolu", |a| a.query.is_empty() && a.browse.is_none() && a.items.len() > MAX_ROWS + 2 && !a.waiting);
    for _ in 0..3 {
        h.frame(vec![]);
    }
    let over_list = pos2(200.0, SEARCH_H + 80.0);
    // dizin yenilenince (sıralama değişse de) ana liste en üstte kalır, sona zıplamaz
    h.app.idx.version.fetch_add(1, Ordering::SeqCst);
    h.wait("sessiz yenileme", |a| !a.waiting);
    assert_eq!((h.app.sel, h.app.scroll), (0, 0), "ana liste en üstte açılır ve orada kalır");
    h.key(Key::Home, Modifiers::NONE);
    assert_eq!((h.app.sel, h.app.scroll), (0, 0));
    for _ in 0..4 {
        h.frame(wheel(over_list, -1.0));
    }
    for _ in 0..5 {
        h.frame(vec![]);
    }
    assert!(h.app.scroll >= 3, "tekerlek ana listeyi aşağı kaydırmalı (kaydırma={})", h.app.scroll);
    assert!(h.app.sel >= h.app.scroll && h.app.sel < h.app.scroll + h.app.visible_rows(), "seçim görünür alanda kalır");
    let s = h.app.scroll;
    h.frame(wheel(over_list, 2.0));
    h.frame(vec![]);
    assert_eq!(h.app.scroll, s - 2, "yukarı da kayar");
});

scenario!(f1_cycles_search_mode, f1_cycles_search_mode_child, |h| {
    h.frame(vec![Event::Text("rapor".into())]);
    h.key(Key::F1, Modifiers::NONE);
    assert!(matches!(h.app.mode, Mode::ExtInput { .. }), "İsim → Uzantı");
    h.frame(vec![Event::Text("pdf".into())]);
    h.key(Key::Enter, Modifiers::NONE);
    assert_eq!(h.app.ext.as_deref(), Some("pdf"));
    assert_eq!(h.app.query, "rapor", "önceki sorgu geri gelir");
    h.key(Key::F1, Modifiers::NONE);
    assert!(h.app.ext.is_none() && h.app.content_query().is_some(), "Uzantı → İçerik");
    h.key(Key::F1, Modifiers::NONE);
    assert!(h.app.ext.is_none() && h.app.content_query().is_none() && h.app.mode == Mode::Search, "İçerik → İsim");
    // Uzantı yazılırken F1 → İçerik
    h.key(Key::F1, Modifiers::NONE);
    h.key(Key::F1, Modifiers::NONE);
    assert!(h.app.mode == Mode::Search && h.app.content_query().is_some());
    h.key(Key::F1, Modifiers::NONE);
    // mod çipine tıklamak da döndürür (fareyle)
    let chip = pos2(60.0, SEARCH_H / 2.0);
    h.frame(vec![Event::PointerMoved(chip)]);
    h.frame(vec![Event::PointerButton { pos: chip, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE }]);
    h.frame(vec![Event::PointerButton { pos: chip, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE }]);
    assert!(matches!(h.app.mode, Mode::ExtInput { .. }), "çipe tıklama: İsim → Uzantı");
    // "Enter Süz" ipucu yerine Esc: önceki sorgu
    h.key(Key::Escape, Modifiers::NONE);
    assert_eq!(h.app.mode, Mode::Search);
});

scenario!(ctrl_s_toggles_settings, ctrl_s_toggles_settings_child, |h| {
    let s = Modifiers { ctrl: true, command: true, ..Default::default() };
    h.key(Key::S, s);
    assert_eq!(h.app.mode, Mode::Settings, "Ctrl+S ayarları açar");
    h.key(Key::S, s);
    assert_eq!(h.app.mode, Mode::Search, "Ctrl+S yeniden basınca kapatır");
});

scenario!(context_menu_has_everything_tab_has, context_menu_has_everything_tab_has_child, |h| {
    let d = h.dir("Liste");
    h.app.enter_browse(&d);
    h.wait("liste", |a| a.items.len() == 6 && !a.waiting);
    for multi in [false, true] {
        h.app.multi = multi;
        let (tab, ctx) = (h.app.tab_menu(), h.app.context_menu());
        let ca = acts(&ctx);
        for a in acts(&tab) {
            assert!(ca.contains(&a), "sağ tık menüsünde {a:?} eksik (çoklu={multi}): {:?}", labels(&ctx));
        }
        assert_eq!(&labels(&ctx)[..3], ["Aç", "Önizle", "Explorer'da aç"], "sağ tıkta ilk üç");
        // her menüde harfler tekil
        for m in [&tab, &ctx] {
            let mut k: Vec<char> = m.iter().filter_map(|e| if let MEntry::Item(i) = e { Some(i.key) } else { None }).collect();
            let n = k.len();
            k.sort();
            k.dedup();
            assert_eq!(k.len(), n, "yinelenen harf: {:?}", labels(m));
        }
    }
    h.app.multi = false;
    assert!(labels(&h.app.tab_menu()).contains(&"Çoklu seçim"));
    assert!(labels(&h.app.tab_menu()).iter().all(|l| *l != "Seçilenleri aç"));
    h.app.multi = true;
    let t = labels(&h.app.tab_menu());
    assert_eq!(&t[..2], ["Çoklu seçimi kapat", "Seçilenleri aç"]);
    h.app.multi = false;
    // Tab → Boşluk: çoklu seçimi açar
    h.key(Key::Tab, Modifiers::NONE);
    assert_eq!(h.app.menu.as_ref().and_then(|m| m.item(m.sel)).map(|i| i.act), Some(Act::Copy), "Tab→Enter eskisi gibi Kopyala");
    h.frame(vec![Event::Key { key: Key::Space, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }, Event::Text(" ".into())]);
    assert!(h.app.multi && h.app.menu.is_none(), "Tab→Boşluk çoklu seçim");
    // Bin: sağ tık menüsü Tab'dakileri de içerir (boş ve dolu)
    h.app.set_multi(false);
    h.app.run(Act::BinMgr, &h.ctx.clone());
    h.wait("boş Bin", |a| a.mode == Mode::Bin && a.items.is_empty());
    let ca = acts(&h.app.context_menu());
    assert!(acts(&h.app.tab_menu()).iter().all(|a| ca.contains(a)), "boş Bin sağ tık");
    crate::bin::add("rapor", &h.dir("Belgeler/rapor.pdf")).unwrap();
    h.app.research();
    h.wait("dolu Bin", |a| !a.items.is_empty());
    let ca = acts(&h.app.context_menu());
    assert!(acts(&h.app.tab_menu()).iter().all(|a| ca.contains(a)), "dolu Bin sağ tık");
});

scenario!(footer_hints_are_clickable, footer_hints_are_clickable_child, |h| {
    let d = h.dir("Belgeler");
    h.app.enter_browse(&d);
    h.wait("liste", |a| !a.items.is_empty() && !a.waiting);
    h.app.toast = None;
    h.frame(vec![]);
    // alttaki ipuçları: yazının üstüne tıkla
    let click = |h: &mut H, p: egui::Pos2| {
        h.frame(vec![Event::PointerMoved(p)]);
        h.frame(vec![Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE }]);
        h.frame(vec![Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE }]);
    };
    // "Tab İşlem" menüyü açar (üçüncü ipucu); x konumu: ipuçlarını sırayla dene
    // (Enter Aç ipucu atlanır: dosyayı gerçekten açardı)
    let mut opened = false;
    for x in (70..520).step_by(10) {
        h.app.toast = None;
        let fy = h.app.last_size.y - FOOTER_H / 2.0;
        click(h, pos2(x as f32, fy));
        if h.app.menu.is_some() {
            opened = true;
            break;
        }
        if matches!(h.app.mode, Mode::Note { .. }) || h.app.preview_open {
            h.app.escape(&h.ctx.clone());
            h.app.toast = None;
        }
    }
    assert!(opened, "Tab İşlem ipucu tıklanınca menü açılır: size={:?} toast={:?} mode={:?} prev={}", h.app.last_size, h.app.toast.as_ref().map(|t| t.0.clone()), h.app.mode, h.app.preview_open);
    h.key(Key::Escape, Modifiers::NONE);
    h.app.toast = None;
    // Ctrl+N Not ipucu
    let mut note = false;
    for x in (70..560).step_by(8) {
        h.app.toast = None;
        let fy = h.app.last_size.y - FOOTER_H / 2.0;
        click(h, pos2(x as f32, fy));
        if matches!(h.app.mode, Mode::Note { .. }) {
            note = true;
            break;
        }
        h.app.menu = None;
        h.app.preview_open = false;
    }
    assert!(note, "Ctrl+N Not ipucu tıklanınca not modu açılır");
    h.key(Key::Escape, Modifiers::NONE);
    // dişli: ayarlar
    h.frame(vec![]);
    let w = h.app.last_size.x;
    let fy = h.app.last_size.y - FOOTER_H / 2.0;
    click(h, pos2(w - 22.0, fy));
    assert_eq!(h.app.mode, Mode::Settings, "dişliye tıklama ayarları açar");
    h.frame(vec![]);
    let fy = h.app.last_size.y - FOOTER_H / 2.0;
    click(h, pos2(w - 22.0, fy));
    assert_eq!(h.app.mode, Mode::Search, "yeniden tıklama kapatır");
});

scenario!(recycle_bin_browse_and_restore, recycle_bin_browse_and_restore_child, |h| {
    // aramada "çöp" → sanal Geri Dönüşüm Kutusu öğesi en üstte; içerik aramaya katılmaz
    h.frame(vec![Event::Text("çöp".into())]);
    h.wait("çöp araması", |a| a.query == "çöp" && !a.waiting && !a.items.is_empty());
    assert_eq!(h.app.items[0].path, crate::recycle::VIRTUAL_PATH);
    h.app.query = "eski".into();
    h.app.research();
    h.wait("eski araması", |a| a.shown_query == "eski" && !a.waiting);
    assert!(h.names().iter().all(|n| n != "eski.txt"), "kutudakiler aramada çıkmaz: {:?}", h.names());
    // Masaüstü'ne gidince listede görünür
    let desk = h.dir("Masaüstü");
    h.app.enter_browse(&desk);
    h.wait("masaüstü", |a| !a.waiting && a.items.len() == 2);
    assert_eq!(h.app.items[0].name, crate::recycle::title());
    // sanal öğede kopyalama/silme/ad değiştirme yok
    assert!(h.app.targets().is_empty());
    h.key(Key::F2, Modifiers::NONE);
    assert!(h.app.edit.is_none());
    // Enter ile içine gir: gerçek adlar, en son silinen üstte
    h.app.sel = 0;
    h.key(Key::Enter, Modifiers::NONE);
    assert_eq!(h.app.mode, Mode::Trash);
    h.wait("kutu", |a| a.items.len() == 3);
    assert_eq!(h.names(), ["notlar.txt", "proje", "eski.txt"]);
    assert!(h.app.items[1].is_dir);
    // menü: Geri yükle, Aç, Özgün klasörü aç, Özgün yolu kopyala; kalıcı silme yok
    let l = labels(&h.app.tab_menu());
    assert_eq!(&l[..4], ["Geri yükle", "Aç", "Özgün klasörü aç", "Özgün yolu kopyala"]);
    assert!(l.iter().all(|x| !x.contains("Sil") && !x.contains("kalıcı")), "{l:?}");
    assert_eq!(labels(&h.app.context_menu()), l, "sağ tık da aynı");
    // Del / F2 / Ctrl+X hiçbir şey yapmaz
    h.key(Key::Delete, Modifiers::NONE);
    h.key(Key::F2, Modifiers::NONE);
    h.key(Key::X, ctrl());
    assert!(h.app.confirm.is_none() && h.app.edit.is_none() && h.app.clip.is_none());
    assert_eq!(h.app.items.len(), 3);
    // notlar.txt'yi geri yükle: özgün yer dolu → "notlar (2).txt", eskisi korunur
    h.app.sel = 0;
    h.key(Key::Tab, Modifiers::NONE);
    h.frame(vec![Event::Text("g".into())]);
    h.wait("geri yüklendi", |a| a.items.len() == 2);
    let b = Path::new(&h.dir("Belgeler")).to_path_buf();
    assert_eq!(std::fs::read_to_string(b.join("notlar.txt")).unwrap(), "merhaba", "var olan dosyanın üzerine yazılmaz");
    assert_eq!(std::fs::read_to_string(b.join("notlar (2).txt")).unwrap(), "silinmiş B2.txt");
    // silinmiş klasör: özgün klasör yoksa yeniden oluşturulur
    let i = h.app.items.iter().position(|x| x.name == "proje").unwrap();
    h.app.sel = i;
    h.app.run(Act::Restore, &h.ctx.clone());
    h.wait("klasör geri yüklendi", |a| a.items.len() == 1);
    assert!(Path::new(&h.dir("Silinen/proje/ic.txt")).is_file());
    // Ctrl+← (fare geri düğmesi de) kutudan çıkar, önceki klasöre döner
    h.key(Key::ArrowLeft, ctrl());
    assert_eq!(h.app.mode, Mode::Search);
    assert_eq!(h.app.browse.as_deref(), Some(desk.as_str()));
});


/// Çizilen tüm metinler (iç içe şekiller dahil).
fn shape_texts(out: &egui::FullOutput) -> Vec<String> {
    fn walk(s: &egui::Shape, v: &mut Vec<String>) {
        match s {
            egui::Shape::Text(t) => v.push(t.galley.job.text.clone()),
            egui::Shape::Vec(xs) => xs.iter().for_each(|x| walk(x, v)),
            _ => {}
        }
    }
    let mut v = Vec::new();
    for c in &out.shapes {
        walk(&c.shape, &mut v);
    }
    v
}

fn menu_keys_unique_ascii(m: &[MEntry], what: &str) {
    let mut k: Vec<char> = m.iter().filter_map(|e| if let MEntry::Item(i) = e { Some(i.key) } else { None }).collect();
    assert!(k.iter().all(|c| *c == ' ' || c.is_ascii_lowercase()), "{what}: harfler a-z olmalı: {k:?}");
    let n = k.len();
    k.sort();
    k.dedup();
    assert_eq!(k.len(), n, "{what}: yinelenen harf: {:?}", labels(m));
}

scenario!(language_setting_switches_immediately, language_setting_switches_immediately_child, |h| {
    assert_eq!(crate::i18n::get(), crate::i18n::Lang::Tr, "testler Türkçe başlar");
    h.key(Key::S, ctrl());
    assert_eq!(h.app.mode, Mode::Settings);
    let t = shape_texts(&h.frame(vec![]));
    assert!(t.iter().any(|s| s == "Dil / Language") && t.iter().any(|s| s == "Türkçe"), "ayarlarda dil satırı: {t:?}");
    for _ in 0..3 {
        h.key(Key::ArrowDown, Modifiers::NONE);
    }
    assert_eq!(h.app.settings_sel, 3);
    h.key(Key::Enter, Modifiers::NONE);
    assert_eq!(crate::i18n::get(), crate::i18n::Lang::En, "Enter dili değiştirir");
    assert_eq!(crate::config::Config::load().0.lang, "en", "dil ayarı kaydedilir");
    let t = shape_texts(&h.frame(vec![]));
    assert!(t.iter().any(|s| s == "English") && t.iter().any(|s| s == "Global shortcut") && t.iter().any(|s| s == "Start with Windows"), "ayarlar hemen İngilizce: {t:?}");
    h.key(Key::Escape, Modifiers::NONE);
    assert_eq!(h.app.mode, Mode::Search);
    // menüler: İngilizce etiketler, a-z harfler tekil, Tab→Enter hâlâ Kopyala
    let d = h.dir("Liste");
    h.app.enter_browse(&d);
    h.wait("liste", |a| a.items.len() == 6 && !a.waiting);
    for multi in [false, true] {
        h.app.multi = multi;
        let (tab, cm) = (h.app.tab_menu(), h.app.context_menu());
        menu_keys_unique_ascii(&tab, "Tab");
        menu_keys_unique_ascii(&cm, "sağ tık");
        let ca = acts(&cm);
        assert!(acts(&tab).iter().all(|a| ca.contains(a)), "sağ tık Tab'ın her şeyini içerir");
        assert_eq!(&labels(&cm)[..3], ["Open", "Preview", "Show in Explorer"]);
        assert!(labels(&tab).contains(&"Copy") && labels(&tab).contains(&"Move to Recycle Bin"));
    }
    h.app.multi = false;
    assert_eq!(labels(&h.app.tab_menu())[0], "Multi-select");
    h.key(Key::Tab, Modifiers::NONE);
    assert_eq!(h.app.menu.as_ref().and_then(|m| m.item(m.sel)).map(|i| i.act), Some(Act::Copy));
    // harfle seçim İngilizce harflerle: f = Recent folders
    h.frame(vec![Event::Text("f".into())]);
    h.frame(vec![]);
    assert_eq!(h.app.mode, Mode::Dests, "İngilizcede f → Recent folders");
    h.key(Key::Escape, Modifiers::NONE);
    // Bin menüleri (boş ve dolu) ve Geri Dönüşüm Kutusu menüsü
    h.app.run(Act::BinMgr, &h.ctx.clone());
    h.wait("boş Bin", |a| a.mode == Mode::Bin && a.items.is_empty());
    menu_keys_unique_ascii(&h.app.tab_menu(), "boş Bin");
    menu_keys_unique_ascii(&h.app.context_menu(), "boş Bin sağ tık");
    crate::bin::add("rapor", &h.dir("Belgeler/rapor.pdf")).unwrap();
    h.app.research();
    h.wait("dolu Bin", |a| !a.items.is_empty());
    menu_keys_unique_ascii(&h.app.tab_menu(), "Bin");
    menu_keys_unique_ascii(&h.app.context_menu(), "Bin sağ tık");
    h.key(Key::Escape, Modifiers::NONE);
    h.app.leave_browse();
    h.frame(vec![Event::Text("recycle".into())]);
    h.wait("sanal öğe", |a| a.items.first().is_some_and(|i| i.path == crate::recycle::VIRTUAL_PATH));
    assert_eq!(h.app.items[0].name, "Recycle Bin");
    h.key(Key::Enter, Modifiers::NONE);
    h.wait("kutu", |a| a.mode == Mode::Trash && a.items.len() == 3);
    let tm = h.app.tab_menu();
    menu_keys_unique_ascii(&tm, "kutu");
    assert_eq!(labels(&tm)[0], "Restore");
    assert!(tm.iter().any(|e| matches!(e, MEntry::Item(i) if i.key == 'r' && i.act == Act::Restore)), "İngilizcede r = Restore");
    h.key(Key::Escape, Modifiers::NONE);
    // geri Türkçe: Sol/Sağ da değiştirir
    h.key(Key::S, ctrl());
    for _ in 0..3 {
        h.key(Key::ArrowDown, Modifiers::NONE);
    }
    h.key(Key::ArrowRight, Modifiers::NONE);
    assert_eq!(crate::i18n::get(), crate::i18n::Lang::Tr);
    assert_eq!(crate::config::Config::load().0.lang, "tr");
    h.key(Key::Escape, Modifiers::NONE);
    assert_eq!(labels(&h.app.context_menu())[0], "Aç", "Türkçe geri geldi");
});

scenario!(english_ui_has_no_turkish_left, english_ui_has_no_turkish_left_child, |h| {
    h.app.toggle_lang();
    assert_eq!(crate::i18n::get(), crate::i18n::Lang::En);
    h.app.toast = None;
    // dosya/klasör adları (test verisi) Türkçe olabilir: onları içeren metinler sayılmaz
    const FIXTURE: [&str; 21] = ["kök", "masaüstü", "kısayol", "belgeler", "notlar", "resimler", "tatil", "kapak", "liste", "dosya", "rapor", "fatura", "makbuz", "ocak", "eski", "proje", "silinen", "ic.txt", "silinmiş", "fp-ui-", "veri"];
    const TR_CHARS: &str = "çğıöşüÇĞİÖŞÜ";
    const TR_WORDS: [&str; 33] = [
        "Kopyala", "Kes", "Yeni", "Kapat", "Geri", "Ayarlar", "Uygula", "Gezgin", "Yenile", "Kaydet", "Sil", "Gir", "Bas", "ara", "ile", "ya", "da", "ve", "yok", "Tab'a", "Oca", "Nis", "Haz", "Tem", "Eyl", "Eki", "Kas", "Ara", "Tümü", "Bugün", "Dün", "önce", "satır",
    ];
    let mut seen = 0;
    let mut check = |h: &mut H, what: &str| {
        let out = h.frame(vec![]);
        for s in shape_texts(&out) {
            let low = s.to_lowercase();
            if FIXTURE.iter().any(|f| low.contains(f)) {
                continue;
            }
            seen += 1;
            assert!(!s.chars().any(|c| TR_CHARS.contains(c)), "{what}: Türkçe metin kalmış: {s:?}");
            for w in s.split(|c: char| !c.is_alphanumeric() && c != '\'') {
                assert!(!TR_WORDS.contains(&w), "{what}: Türkçe sözcük {w:?} kalmış: {s:?}");
            }
        }
    };
    h.wait("ana liste", |a| !a.waiting && !a.items.is_empty());
    check(h, "ana liste");
    h.frame(vec![Event::Text("pdf".into())]);
    h.wait("arama", |a| a.query == "pdf" && !a.items.is_empty() && !a.waiting);
    check(h, "arama");
    h.key(Key::F1, Modifiers::NONE);
    check(h, "uzantı girişi");
    h.key(Key::F1, Modifiers::NONE);
    check(h, "içerik araması");
    h.key(Key::F1, Modifiers::NONE);
    h.key(Key::Escape, Modifiers::NONE);
    let d = h.dir("Belgeler");
    h.app.enter_browse(&d);
    h.wait("klasör", |a| !a.waiting && !a.items.is_empty());
    check(h, "klasör");
    h.key(Key::Space, Modifiers::SHIFT);
    h.wait("önizleme", |a| a.preview.as_ref().is_some_and(|p| p.key == a.pkey));
    check(h, "önizleme");
    h.key(Key::Space, Modifiers::SHIFT);
    h.key(Key::Tab, Modifiers::NONE);
    check(h, "Tab menüsü");
    h.key(Key::Escape, Modifiers::NONE);
    let p = pos2(200.0, 150.0);
    h.app.open_menu(true, p);
    check(h, "sağ tık menüsü");
    h.key(Key::Escape, Modifiers::NONE);
    h.key(Key::M, ctrl());
    check(h, "çoklu seçim");
    h.frame(vec![Event::Key { key: Key::Space, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }, Event::Text(" ".into())]);
    check(h, "işaretli");
    h.key(Key::M, ctrl());
    h.app.clear_marks();
    h.app.toast = None;
    h.key(Key::Delete, Modifiers::NONE);
    assert!(h.app.confirm.is_some());
    check(h, "silme onayı");
    h.key(Key::Escape, Modifiers::NONE);
    h.key(Key::F2, Modifiers::NONE);
    check(h, "yeniden adlandırma");
    h.key(Key::Escape, Modifiers::NONE);
    h.key(Key::N, ctrl());
    check(h, "not");
    h.key(Key::Escape, Modifiers::NONE);
    h.key(Key::S, ctrl());
    check(h, "ayarlar");
    h.key(Key::Escape, Modifiers::NONE);
    h.app.run(Act::Dests, &h.ctx.clone());
    check(h, "son hedefler");
    h.key(Key::Escape, Modifiers::NONE);
    h.app.run(Act::BinMgr, &h.ctx.clone());
    h.wait("Bin", |a| a.mode == Mode::Bin);
    check(h, "boş Bin");
    h.key(Key::Tab, Modifiers::NONE);
    check(h, "boş Bin menüsü");
    h.key(Key::Escape, Modifiers::NONE);
    h.key(Key::Escape, Modifiers::NONE);
    h.app.leave_browse();
    h.frame(vec![Event::Text("trash".into())]);
    h.wait("sanal öğe", |a| a.items.first().is_some_and(|i| i.path == crate::recycle::VIRTUAL_PATH));
    h.app.toast = None;
    check(h, "kutu araması");
    h.key(Key::Enter, Modifiers::NONE);
    h.wait("kutu", |a| a.mode == Mode::Trash && a.items.len() == 3);
    check(h, "Geri Dönüşüm Kutusu");
    h.key(Key::Tab, Modifiers::NONE);
    check(h, "kutu menüsü");
    // dile bağlı biçimler
    assert_eq!(crate::util::fmt_count(1234567), "1,234,567");
    assert_eq!(crate::util::fmt_size(1536), "1.5 KB");
    assert_eq!(crate::util::fmt_when(crate::util::now_secs()), "just now");
    assert!(seen > 40, "yeterince metin denetlenmeli ({seen})");
});

fn alt_ctrl() -> Modifiers {
    Modifiers { ctrl: true, command: true, alt: true, ..Default::default() }
}

fn click_at(h: &mut H, p: egui::Pos2, button: egui::PointerButton) {
    h.frame(vec![Event::PointerMoved(p)]);
    h.frame(vec![Event::PointerButton { pos: p, button, pressed: true, modifiers: Modifiers::NONE }]);
    h.frame(vec![Event::PointerButton { pos: p, button, pressed: false, modifiers: Modifiers::NONE }]);
}

#[test]
fn sort_token_parsing() {
    assert_eq!(parse_sort("the game :si"), ("the game", Some((SORT_SIZE, true))));
    assert_eq!(parse_sort("the game :si-"), ("the game", Some((SORT_SIZE, false))));
    assert_eq!(parse_sort("the game :da"), ("the game", Some((SORT_DATE, true))));
    assert_eq!(parse_sort("the game :da-"), ("the game", Some((SORT_DATE, false))));
    assert_eq!(parse_sort("the game :na"), ("the game", Some((SORT_NAME, false))));
    assert_eq!(parse_sort("the game :NA-"), ("the game", Some((SORT_NAME, true))));
    assert_eq!(parse_sort(":si"), ("", Some((SORT_SIZE, true))));
    assert_eq!(parse_sort("  rapor   :si  "), ("  rapor", Some((SORT_SIZE, true))));
    assert_eq!(parse_sort("şarkı :na"), ("şarkı", Some((SORT_NAME, false))));
    // belirteç değil
    assert_eq!(parse_sort("abc:si"), ("abc:si", None));
    assert_eq!(parse_sort("the game"), ("the game", None));
    assert_eq!(parse_sort("x :zz"), ("x :zz", None));
    assert_eq!(parse_sort("x :si--"), ("x :si--", None));
    assert_eq!(parse_sort(""), ("", None));
    // Ctrl+1/2/3: ekle, değiştir, aynı tuş "-" ekler/kaldırır
    assert_eq!(with_sort_token("the game", SORT_SIZE), "the game :si");
    assert_eq!(with_sort_token("the game :si", SORT_SIZE), "the game :si-");
    assert_eq!(with_sort_token("the game :si-", SORT_SIZE), "the game :si");
    assert_eq!(with_sort_token("the game :si-", SORT_NAME), "the game :na");
    assert_eq!(with_sort_token("the game :na", SORT_NAME), "the game :na-");
    assert_eq!(with_sort_token("the game ", SORT_DATE), "the game :da");
    assert_eq!(with_sort_token("", SORT_DATE), ":da");
    assert_eq!(with_sort_token(":da", SORT_DATE), ":da-");
}

scenario!(relevance_default_exact_first, relevance_default_exact_first_child, |h| {
    for p in ["Oyun/game of the year.txt", "Oyun/the gamer notes.txt", "Oyun/the game guide.pdf", "Oyun/endgame the.txt"] {
        write(&h.root.join(p), "x");
    }
    std::fs::create_dir_all(h.root.join("Arsiv/THE GAME")).unwrap();
    write(&h.root.join("Arsiv/THE GAME/kayit.txt"), "x");
    h.key(Key::F5, Modifiers::NONE); // yeniden dizinle
    h.frame(vec![Event::Text("the game".into())]);
    h.wait("sonuçlar", |a| a.query == "the game" && !a.waiting && a.items.iter().any(|i| i.name == "THE GAME"));
    assert_eq!(h.app.sort_spec(), None, "varsayılan: sıralama yok");
    assert_eq!(h.names()[0], "THE GAME", "birebir eşleşen klasör en üstte: {:?}", h.names());
    let t = shape_texts(&h.frame(vec![]));
    assert!(t.iter().filter(|s| *s == "Sırala").count() == 1 && !t.iter().any(|s| s == "İsim ↑" || s == "Boyut" || s == "Tarih"), "arama kutusunda sıralama çipi yok (alt çubukta yalnızca ipucu): {t:?}");
});

scenario!(sort_tokens_folder_and_not_sticky, sort_tokens_folder_and_not_sticky_child, |h| {
    let d = h.root.join("Sirali");
    let now = std::time::SystemTime::now();
    for (n, size, ago) in [("b-orta.txt", 50usize, 200u64), ("a-kucuk.txt", 5, 100), ("C-buyuk.txt", 500, 300)] {
        write(&d.join(n), &"x".repeat(size));
        let f = std::fs::File::options().write(true).open(d.join(n)).unwrap();
        f.set_modified(now - Duration::from_secs(ago * 3600)).unwrap();
    }
    write(&d.join("klasor/alt/ic.bin"), &"x".repeat(2000));
    let dir = d.to_string_lossy().to_string();
    h.app.enter_browse(&dir);
    h.wait("liste", |a| a.items.len() == 4);
    assert_eq!(h.names()[0], "klasor", "varsayılan: klasörler önce");
    // ":si" yalnız (boş arama + belirteç): klasör listesi boyuta göre; klasör boyutu arka planda
    h.frame(vec![Event::Text(":si".into())]);
    h.wait("klasör boyutu", |a| a.dirs.get(&d.join("klasor").to_string_lossy()).is_some() && a.items.len() == 4 && a.items[0].name == "klasor");
    assert_eq!(h.app.sort_spec(), Some((SORT_SIZE, true)));
    assert_eq!(h.names(), ["klasor", "C-buyuk.txt", "b-orta.txt", "a-kucuk.txt"], ":si klasör ve dosyaları birlikte boyuta göre dizer");
    let t = shape_texts(&h.frame(vec![]));
    assert!(t.iter().any(|s| *s == crate::util::fmt_size(2000)), "klasör satırında hesaplanan boyut: {t:?}");
    // Ctrl+2 aynı belirteçte "-" ekler: küçükten
    h.key(Key::Num2, ctrl());
    h.frame(vec![]);
    assert_eq!(h.app.query, ":si-");
    h.wait("küçükten", |a| a.items.len() == 4 && a.items[0].name == "a-kucuk.txt");
    assert_eq!(h.names(), ["a-kucuk.txt", "b-orta.txt", "C-buyuk.txt", "klasor"]);
    // Ctrl+1: ":na" (A→Z, klasörler önce)
    h.key(Key::Num1, ctrl());
    h.frame(vec![]);
    assert_eq!(h.app.query, ":na");
    h.wait("isim", |a| a.items.len() == 4 && a.items[1].name == "a-kucuk.txt");
    assert_eq!(h.names(), ["klasor", "a-kucuk.txt", "b-orta.txt", "C-buyuk.txt"]);
    // Ctrl+3: ":da" (yeniden)
    h.key(Key::Num3, ctrl());
    h.frame(vec![]);
    assert_eq!(h.app.query, ":da");
    h.wait("tarih", |a| a.items.len() == 4 && a.items[1].name == "a-kucuk.txt" && a.items[3].name == "C-buyuk.txt");
    // AltGr (Ctrl+Alt) ile değil
    h.key(Key::Num1, alt_ctrl());
    assert_eq!(h.app.query, ":da", "Ctrl+Alt+1 belirteç yazmaz");
    // alt çubuk ipucu: belirteci döndürür
    let t = shape_texts(&h.frame(vec![]));
    assert!(t.iter().any(|s| s == "Ctrl+1/2/3") && t.iter().any(|s| s == "Sırala"), "{t:?}");
    h.app.cycle_sort();
    assert_eq!(h.app.query, ":na");
    h.app.cycle_sort();
    assert_eq!(h.app.query, ":si");
    // kalıcı değil: belirteç silinince varsayılan sıra
    h.app.query.clear();
    h.frame(vec![]);
    h.wait("varsayılan", |a| a.items.len() == 4 && a.items[0].name == "klasor" && a.sort_spec().is_none());
    // arama sonuçlarında belirteç: "txt :si" → yalnızca "txt" aranır, boyuta göre
    h.frame(vec![Event::Text("txt :si".into())]);
    let mine = |a: &App| a.items.iter().filter(|i| i.path.contains("Sirali")).map(|i| i.name.clone()).collect::<Vec<_>>();
    h.wait("sonuçlar", |a| a.query == "txt :si" && mine(a).len() == 3);
    assert_eq!(mine(&h.app), ["C-buyuk.txt", "b-orta.txt", "a-kucuk.txt"]);
    assert!(h.app.items.iter().filter(|i| !i.is_dir).collect::<Vec<_>>().windows(2).all(|w| w[0].size >= w[1].size));
    // yeni arama (belirteç yok): sıralama yok
    h.app.query = "txt".into();
    h.frame(vec![]);
    assert_eq!(h.app.sort_spec(), None);
    // başka klasöre girmek metni (ve sıralamayı) temizler
    h.app.query = ":si".into();
    h.frame(vec![]);
    let other = h.dir("Belgeler");
    h.app.enter_browse(&other);
    assert!(h.app.query.is_empty() && h.app.sort_spec().is_none());
    // genel kısayol olarak Ctrl+1/2/3 ve Shift+F1 kabul edilmez
    for hk in [crate::config::Hotkey { mods: crate::config::MOD_CONTROL, vk: 0x32 }, crate::config::Hotkey { mods: crate::config::MOD_SHIFT, vk: crate::config::VK_F1 }] {
        h.app.start_record();
        h.app.apply_hotkey(hk);
        assert!(h.app.hk_pending.is_none(), "{} genel kısayol olamaz", hk.label());
        assert!(h.app.toast.as_ref().is_some_and(|t| t.0.contains(&hk.label()) && t.0.contains("kullanılamaz")), "uyarı: {:?}", h.app.toast);
    }
});

scenario!(slash_command, slash_command_child, |h| {
    let dir = h.dir("Belgeler");
    h.app.enter_browse(&dir);
    h.wait("liste", |a| !a.items.is_empty());
    h.frame(vec![Event::Text("/".into())]);
    h.frame(vec![]);
    assert!(h.app.is_cmd() && h.app.items.is_empty(), "'/' ile başlayan metin komuttur");
    h.key(Key::Enter, Modifiers::NONE);
    assert!(h.app.toast.as_ref().is_some_and(|t| t.0.starts_with("Çalıştırılacak komutu yazın")), "boş komut çalıştırılmaz: {:?}", h.app.toast);
    h.frame(vec![Event::Text("ipconfig /all".into())]);
    h.frame(vec![]);
    assert_eq!(h.app.query, "/ipconfig /all");
    assert_eq!(h.app.cmd_line(), Some("ipconfig /all"));
    assert_eq!(h.app.cmd_dir().as_deref(), Some(dir.as_str()), "komut gözatılan klasörde çalışır");
    assert!(h.app.items.is_empty() && h.app.content_query().is_none() && h.app.sort_spec().is_none(), "komutta arama/sıralama yok");
    h.app.toast = None;
    let t = shape_texts(&h.frame(vec![]));
    assert!(t.iter().any(|s| s == "Komut"), "Komut çipi: {t:?}");
    assert!(t.iter().any(|s| s == "ipconfig /all") && t.iter().any(|s| s.contains("terminalde çalıştır")), "komut kartı: {t:?}");
    assert!(t.iter().any(|s| s == "Terminalde çalıştır"), "alt çubuk: {t:?}");
    // Ctrl+1/2/3 komutta belirteç yazmaz
    h.key(Key::Num2, ctrl());
    assert_eq!(h.app.query, "/ipconfig /all");
    // F1 döngüsünde Komut yok
    assert!(!format!("{:?}", h.app.mode).contains("Cmd"));
    // Esc: komut silinir, pencere gizlenmez, klasör listesi geri gelir
    let out = h.key(Key::Escape, Modifiers::NONE);
    assert!(!hid(&out), "Esc komuttan çıkar, pencereyi gizlemez");
    assert!(h.app.query.is_empty() && !h.app.is_cmd());
    h.wait("liste geri gelir", |a| !a.items.is_empty());
    // "/" ortada: komut değil
    h.frame(vec![Event::Text("a/b".into())]);
    assert!(!h.app.is_cmd());
});

scenario!(shift_f1_cycles_back, shift_f1_cycles_back_child, |h| {
    let shift = Modifiers { shift: true, ..Default::default() };
    let is_name = |a: &App| a.mode == Mode::Search && a.ext.is_none() && a.content_query().is_none();
    h.frame(vec![Event::Text("rapor".into())]);
    assert!(is_name(&h.app));
    h.key(Key::F1, shift);
    assert!(h.app.content_query().is_some(), "İsim ← İçerik (sarar)");
    assert_eq!(h.app.query, "rapor");
    h.key(Key::F1, shift);
    assert!(matches!(h.app.mode, Mode::ExtInput { .. }), "İçerik → Uzantı");
    h.key(Key::F1, shift);
    assert!(is_name(&h.app), "Uzantı → İsim");
    assert_eq!(h.app.query, "rapor", "önceki sorgu geri gelir");
    // uygulanmış uzantı süzgeci: geri → İsim
    h.key(Key::F1, Modifiers::NONE);
    h.frame(vec![Event::Text("pdf".into())]);
    h.key(Key::Enter, Modifiers::NONE);
    assert_eq!(h.app.ext.as_deref(), Some("pdf"));
    h.key(Key::F1, shift);
    assert!(is_name(&h.app), "Uzantı (süzgeç) → İsim");
    // F1 hâlâ ileri (3 mod)
    h.key(Key::F1, Modifiers::NONE);
    assert!(matches!(h.app.mode, Mode::ExtInput { .. }), "F1: İsim → Uzantı");
    h.key(Key::F1, Modifiers::NONE);
    assert!(h.app.content_query().is_some(), "F1: Uzantı → İçerik");
    h.key(Key::F1, Modifiers::NONE);
    assert!(is_name(&h.app), "F1: İçerik → İsim");
    // Shift+F1 menü açmaz; Shift+F10 bağlam menüsü hâlâ çalışır
    h.key(Key::F1, shift);
    assert!(h.app.menu.is_none() && h.app.content_query().is_some());
    h.key(Key::F1, Modifiers::NONE);
    assert!(is_name(&h.app));
    h.app.query.clear();
    h.app.research();
    h.wait("liste", |a| !a.items.is_empty());
    h.key(Key::F10, shift);
    assert!(h.app.menu.as_ref().is_some_and(|m| m.context), "Shift+F10 bağlam menüsü");
    h.key(Key::Escape, Modifiers::NONE);
    assert!(h.app.menu.is_none() && is_name(&h.app));
    // mod çipi: sağ tık geri, sol tık ileri; ipucu
    let chip = pos2(60.0, SEARCH_H / 2.0);
    h.frame(vec![Event::PointerMoved(chip)]);
    let t = shape_texts(&h.frame(vec![]));
    assert!(t.iter().any(|s| s == "F1 / tık: ileri · Shift+F1 / sağ tık: geri"), "mod çipi ipucu: {t:?}");
    click_at(h, chip, egui::PointerButton::Secondary);
    assert!(h.app.content_query().is_some() && h.app.menu.is_none(), "sağ tık: İsim → İçerik");
    click_at(h, chip, egui::PointerButton::Secondary);
    assert!(matches!(h.app.mode, Mode::ExtInput { .. }), "sağ tık: İçerik → Uzantı");
    h.key(Key::Escape, Modifiers::NONE);
    click_at(h, chip, egui::PointerButton::Primary);
    assert!(matches!(h.app.mode, Mode::ExtInput { .. }), "sol tık: İsim → Uzantı");
});

#[test]
fn command_splitting() {
    let s = |x: &str| split_commands(x);
    assert_eq!(
        s(r"git clone https://github.com/u/p //cd p //cargo build --release //copy target\release\p.exe C:\bin"),
        ["git clone https://github.com/u/p", "cd p", "cargo build --release", r"copy target\release\p.exe C:\bin"]
    );
    assert_eq!(s("ipconfig /all //dir"), ["ipconfig /all", "dir"]);
    // boşluksuz "//" komutun parçası (URL'ler)
    assert_eq!(s("curl http://a.b/c //echo file://x/y"), ["curl http://a.b/c", "echo file://x/y"]);
    assert_eq!(s("ipconfig//all"), ["ipconfig//all"]);
    // tek "/" anahtarlar, -/-- bayrakları dokunulmaz
    assert_eq!(s("dir /s /b"), ["dir /s /b"]);
    assert_eq!(s("cargo build --release -j 4 //git status -s"), ["cargo build --release -j 4", "git status -s"]);
    // sondaki / baştaki "//", çok boşluk, boş komutlar
    assert_eq!(s("ipconfig //"), ["ipconfig"]);
    assert_eq!(s("ipconfig   //   //  dir   "), ["ipconfig", "dir"]);
    assert_eq!(s("//dir"), ["dir"]);
    assert_eq!(s("a\t//b"), ["a", "b"]);
    assert_eq!(s(""), Vec::<String>::new());
    assert_eq!(s("  //  "), Vec::<String>::new());
    // Türkçe karakterler
    assert_eq!(s("echo çalış //echo şimdi"), ["echo çalış", "echo şimdi"]);
}

scenario!(slash_command_chain_card, slash_command_chain_card_child, |h| {
    h.frame(vec![Event::Text(r"/git clone https://github.com/u/p //cd p //cargo build --release //copy target\release\p.exe C:\bin".into())]);
    h.frame(vec![]);
    assert!(h.app.is_cmd());
    assert_eq!(h.app.cmd_list().len(), 4);
    assert!(h.app.visible_rows() > 2, "kart çok komutta büyür");
    let t = shape_texts(&h.frame(vec![]));
    assert!(t.iter().any(|s| s == "4 komut · sırayla çalışır, biri başarısız olursa durur"), "{t:?}");
    for (n, c) in [("1.", "git clone https://github.com/u/p"), ("2.", "cd p"), ("3.", "cargo build --release"), ("4.", r"copy target\release\p.exe C:\bin")] {
        assert!(t.iter().any(|s| s == n) && t.iter().any(|s| s == c), "numaralı satır {n} {c}: {t:?}");
    }
    // tek komut + sondaki "//": eski kart
    h.app.query = "/ipconfig /all //".into();
    h.frame(vec![]);
    assert_eq!(h.app.cmd_list(), ["ipconfig /all"]);
    assert_eq!(h.app.visible_rows(), 2);
    let t = shape_texts(&h.frame(vec![]));
    assert!(t.iter().any(|s| s == "ipconfig /all"), "{t:?}");
});
