//! Başlatıcının çizimi ve fare etkileşimi (maketlerdeki görünüm).

use crate::app::*;
use crate::search::{self, Item};
use crate::theme::{self as t, Line, LineIcon};
use crate::util::{self, CAT_ARCHIVE, CAT_AUDIO, CAT_CODE, CAT_IMAGE, CAT_VIDEO};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, CursorIcon, Event, FontFamily, FontId, Id, LayerId, Order, Painter, PointerButton, Pos2, Rect, RichText, Shape, Stroke, StrokeKind,
    TextEdit, TextFormat, Ui, Vec2, pos2, text::LayoutJob, vec2,
};

pub fn semi() -> FontFamily {
    FontFamily::Name("semi".into())
}
fn reg(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}
fn sb(size: f32) -> FontId {
    FontId::new(size, semi())
}

/// "12 Kas 2026"
pub fn fmt_day(ts: i64) -> String {
    use chrono::{Datelike, Local, TimeZone};
    const M: [&str; 12] = ["Oca", "Şub", "Mar", "Nis", "May", "Haz", "Tem", "Ağu", "Eyl", "Eki", "Kas", "Ara"];
    const M_EN: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let months = if crate::i18n::en() { M_EN } else { M };
    if ts <= 0 {
        return String::new();
    }
    match Local.timestamp_opt(ts, 0).single() {
        Some(d) => format!("{} {} {}", d.day(), months[d.month0() as usize], d.year()),
        None => String::new(),
    }
}

struct Layout {
    panel: Rect,
    rows: usize,
    list_top: f32,
    list_rect: Rect,
    footer: Rect,
    strip: Option<Rect>,
    preview: Option<Rect>,
}

fn layout(app: &App) -> Layout {
    let rows = app.visible_rows();
    let pw = PANEL_W + if app.preview_open { PREVIEW_W } else { 0.0 };
    let strip_h = if app.has_strip() { STRIP_H } else { 0.0 };
    let list_h = LIST_PAD_T + rows as f32 * ROW_H + LIST_PAD_B;
    let ph = SEARCH_H + 1.0 + strip_h + list_h + FOOTER_H;
    let panel = Rect::from_min_size(pos2(MARGIN, MARGIN), vec2(pw, ph));
    let list_top = panel.top() + SEARCH_H + 1.0 + strip_h;
    let list_rect = Rect::from_min_size(pos2(panel.left(), list_top), vec2(PANEL_W, list_h));
    let footer = Rect::from_min_size(pos2(panel.left(), list_rect.bottom()), vec2(PANEL_W, FOOTER_H));
    let strip = (strip_h > 0.0).then(|| Rect::from_min_size(pos2(panel.left(), panel.top() + SEARCH_H + 1.0), vec2(pw, STRIP_H)));
    let preview = app.preview_open.then(|| Rect::from_min_max(pos2(panel.left() + PANEL_W, list_top), panel.max));
    Layout { panel, rows, list_top, list_rect, footer, strip, preview }
}

fn row_rect(l: &Layout, vi: usize) -> Rect {
    Rect::from_min_size(pos2(l.panel.left(), l.list_top + LIST_PAD_T + vi as f32 * ROW_H), vec2(PANEL_W, ROW_H))
}

/// Menü kartı (D): sıkı satırlar, solda vurgu şeridi, harfler küçük tuş kutularında.
const MENU_ITEM_H_D: f32 = 20.0;
const MENU_SEP_H: f32 = 5.0;
const MENU_PAD: f32 = 6.0;
const MENU_W: f32 = 240.0;
/// Kartta aynı anda görünen en fazla satır (fazlası kartın içinde kayar).
const MENU_MAX_ROWS: f32 = 12.0;

fn menu_entry_h(e: &MEntry) -> f32 {
    match e {
        MEntry::Item(_) => MENU_ITEM_H_D,
        MEntry::Sep => MENU_SEP_H,
    }
}

/// Menünün kenar boşlukları.
fn menu_chrome_h(_m: &Menu) -> f32 {
    MENU_PAD
}

/// Menü girdilerine pencere içinde ayrılabilecek en fazla yükseklik (pencere asla büyümez).
fn menu_avail_h(m: &Menu, l: &Layout) -> f32 {
    let full = l.panel.bottom() - 6.0 - (l.panel.top() + SEARCH_H + 4.0) - menu_chrome_h(m);
    let cap = MENU_ITEM_H_D * MENU_MAX_ROWS + MENU_SEP_H * 2.0;
    full.min(cap).max(MENU_ITEM_H_D)
}

/// `first`ten başlayarak tam sığan girdilerin sonu (hariç).
fn menu_visible_end(m: &Menu, first: usize, avail: f32) -> usize {
    let mut h = 0.0;
    let mut end = first;
    while end < m.entries.len() {
        let eh = menu_entry_h(&m.entries[end]);
        if h + eh > avail + 0.5 {
            break;
        }
        h += eh;
        end += 1;
    }
    end.max((first + 1).min(m.entries.len()))
}

/// Kaydırmanın en fazla değeri: sondaki girdiler alanı tam doldurur.
fn menu_max_scroll(m: &Menu, avail: f32) -> usize {
    let mut first = m.entries.len();
    let mut h = 0.0;
    while first > 0 {
        let eh = menu_entry_h(&m.entries[first - 1]);
        if h + eh > avail + 0.5 {
            break;
        }
        h += eh;
        first -= 1;
    }
    first
}

/// Seçili satır her zaman görünsün; boşluk kalmasın.
pub(crate) fn menu_fix_scroll(m: &mut Menu, avail: f32) {
    let max = menu_max_scroll(m, avail);
    if m.sel < m.scroll {
        m.scroll = m.sel;
        // grubun başındaki ayracı da göster
        if m.scroll > 0 && matches!(m.entries[m.scroll - 1], MEntry::Sep) {
            m.scroll -= 1;
        }
    }
    while m.scroll < max && m.sel >= menu_visible_end(m, m.scroll, avail) {
        m.scroll += 1;
    }
    m.scroll = m.scroll.min(max);
}

fn menu_size(m: &Menu, l: &Layout) -> Vec2 {
    let total: f32 = m.entries.iter().map(menu_entry_h).sum();
    let avail = menu_avail_h(m, l);
    let body = if total <= avail + 0.5 {
        total
    } else {
        let end = menu_visible_end(m, m.scroll, avail);
        m.entries[m.scroll..end].iter().map(menu_entry_h).sum()
    };
    vec2(MENU_W, body + menu_chrome_h(m))
}

fn paint_gear(p: &Painter, c: Pos2, col: Color32) {
    for k in 0..8 {
        let a = k as f32 * std::f32::consts::TAU / 8.0;
        let d = vec2(a.cos(), a.sin());
        p.line_segment([c + d * 4.0, c + d * 6.6], Stroke::new(2.0, col));
    }
    p.circle_stroke(c, 4.2, Stroke::new(1.5, col));
    p.circle_filled(c, 1.4, col);
}

/// Dünya simgesi (dil ayarı): daire, meridyen ve enlemler.
fn paint_globe(p: &Painter, c: Pos2, col: Color32) {
    let k = Stroke::new(1.3, col);
    p.circle_stroke(c, 7.0, k);
    p.line_segment([pos2(c.x - 7.0, c.y), pos2(c.x + 7.0, c.y)], k);
    p.line_segment([pos2(c.x - 6.0, c.y - 3.6), pos2(c.x + 6.0, c.y - 3.6)], Stroke::new(1.0, col));
    p.line_segment([pos2(c.x - 6.0, c.y + 3.6), pos2(c.x + 6.0, c.y + 3.6)], Stroke::new(1.0, col));
    let pts: Vec<Pos2> = (0..=24).map(|i| {
        let a = i as f32 / 24.0 * std::f32::consts::TAU;
        pos2(c.x + 3.2 * a.cos(), c.y + 7.0 * a.sin())
    }).collect();
    p.add(egui::Shape::line(pts, k));
}

/// Terminal simgesi: çerçeve içinde ">_".
fn paint_terminal(p: &Painter, c: Pos2, col: Color32) {
    let r = Rect::from_center_size(c, vec2(15.0, 12.0));
    p.rect_stroke(r, CornerRadius::same(2), Stroke::new(1.3, col), StrokeKind::Inside);
    p.line_segment([pos2(r.left() + 3.0, r.top() + 3.5), pos2(r.left() + 5.8, r.center().y)], Stroke::new(1.3, col));
    p.line_segment([pos2(r.left() + 5.8, r.center().y), pos2(r.left() + 3.0, r.bottom() - 3.5)], Stroke::new(1.3, col));
    p.line_segment([pos2(r.left() + 7.5, r.bottom() - 3.2), pos2(r.right() - 3.0, r.bottom() - 3.2)], Stroke::new(1.3, col));
}

/// Çöp kutusu simgesi (çizgi): gövde, kapak, sap ve iki dikey çizgi. `s` ≈ yarı yükseklik.
pub fn paint_trash(p: &Painter, c: Pos2, s: f32, col: Color32) {
    let k = Stroke::new((s * 0.2).clamp(1.0, 1.6), col);
    let w = s * 1.25;
    let body = Rect::from_min_max(pos2(c.x - w * 0.42, c.y - s * 0.45), pos2(c.x + w * 0.42, c.y + s));
    p.rect_stroke(body, CornerRadius::same((s * 0.25) as u8), k, StrokeKind::Inside);
    p.line_segment([pos2(c.x - w * 0.6, c.y - s * 0.62), pos2(c.x + w * 0.6, c.y - s * 0.62)], k);
    p.line_segment([pos2(c.x - w * 0.2, c.y - s * 0.62), pos2(c.x - w * 0.2, c.y - s * 0.95)], k);
    p.line_segment([pos2(c.x - w * 0.2, c.y - s * 0.95), pos2(c.x + w * 0.2, c.y - s * 0.95)], k);
    p.line_segment([pos2(c.x + w * 0.2, c.y - s * 0.95), pos2(c.x + w * 0.2, c.y - s * 0.62)], k);
    if s >= 5.0 {
        for dx in [-0.16, 0.16] {
            p.line_segment([pos2(c.x + w * dx, c.y - s * 0.15), pos2(c.x + w * dx, c.y + s * 0.7)], k);
        }
    }
}

fn paint_x(p: &Painter, c: Pos2, s: f32, col: Color32) {
    p.line_segment([pos2(c.x - s, c.y - s), pos2(c.x + s, c.y + s)], Stroke::new(1.4, col));
    p.line_segment([pos2(c.x - s, c.y + s), pos2(c.x + s, c.y - s)], Stroke::new(1.4, col));
}

fn menu_rect(m: &Menu, l: &Layout) -> Rect {
    let s = menu_size(m, l);
    let mut x = m.pos.x;
    if x + s.x > l.panel.right() - 8.0 {
        x = l.panel.right() - 8.0 - s.x;
    }
    x = x.max(l.panel.left() + 8.0);
    // pencere = kutu: menü kutunun içinde kalsın
    // aşağıda yer yoksa satırın üstünde aç; yine de sığmazsa kaydır — asla kutu dışına taşma
    let mut y = m.pos.y;
    if y + s.y > l.panel.bottom() - 6.0 {
        y = m.pos.y - 30.0 - s.y;
    }
    let y = y.min(l.panel.bottom() - 6.0 - s.y).max(l.panel.top() + SEARCH_H + 4.0);
    Rect::from_min_size(pos2(x, y), s)
}

fn job(text: &str, font: FontId, color: Color32, max_w: f32) -> LayoutJob {
    let mut j = LayoutJob::simple_singleline(text.to_string(), font, color);
    j.wrap.max_width = max_w;
    j.wrap.max_rows = 1;
    j.wrap.break_anywhere = true;
    j.wrap.overflow_character = Some('…');
    j
}

fn text_w(p: &Painter, s: &str, f: FontId) -> f32 {
    p.layout_no_wrap(s.to_string(), f, Color32::WHITE).size().x
}

/// Ad: eşleşen kısımlar vurgulu (arka plan kutuları ayrıca çizilir).
fn name_job(name: &str, ranges: &[(usize, usize)], selected: bool, searching: bool, max_w: f32) -> LayoutJob {
    let base = if searching { Color32::from_rgb(0xc4, 0xc8, 0xcf) } else { t::FG };
    let mut j = LayoutJob::default();
    let chars: Vec<char> = name.chars().collect();
    let mut i = 0;
    let fmt_n = TextFormat { font_id: sb(14.0), color: base, ..Default::default() };
    let fmt_h = TextFormat { font_id: sb(14.0), color: if selected { t::HIT_FG_SEL } else { t::HIT_FG }, ..Default::default() };
    for &(a, b) in ranges {
        let (a, b) = (a.min(chars.len()), b.min(chars.len()));
        if a > i {
            j.append(&chars[i..a].iter().collect::<String>(), 0.0, fmt_n.clone());
        }
        j.append(&chars[a..b].iter().collect::<String>(), 0.0, fmt_h.clone());
        i = b;
    }
    if i < chars.len() {
        j.append(&chars[i..].iter().collect::<String>(), 0.0, fmt_n);
    }
    j.wrap.max_width = max_w;
    j.wrap.max_rows = 1;
    j.wrap.break_anywhere = true;
    j.wrap.overflow_character = Some('…');
    j
}

/// Ad satırı: vurgu kutuları + metin.
fn paint_name(p: &Painter, origin: Pos2, name: &str, q: &str, selected: bool, max_w: f32) {
    let searching = !q.trim().is_empty();
    let ranges = if searching { search::match_ranges(name, q) } else { Vec::new() };
    let fuzzy = ranges.len() > search::terms_of(q).len();
    let g = p.layout_job(name_job(name, &ranges, selected, searching, max_w));
    if !fuzzy {
        let h = g.size().y;
        for &(a, b) in &ranges {
            let ra = g.pos_from_cursor(egui::text::CCursor::new(a));
            let rb = g.pos_from_cursor(egui::text::CCursor::new(b));
            if rb.min.x <= ra.min.x {
                continue;
            }
            let top = origin.y + (h - 17.0) / 2.0;
            let r = Rect::from_min_max(pos2(origin.x + ra.min.x - 1.5, top), pos2(origin.x + rb.min.x + 1.5, top + 17.0));
            p.rect_filled(r, CornerRadius::same(3), t::hit_bg(selected));
        }
    }
    p.galley(origin, g, t::FG);
}

// ---------------- Simgeler ----------------

fn file_badge(it: &Item) -> (Color32, &'static str) {
    let e = util::ext_of(&it.name);
    match e.as_str() {
        "doc" | "docx" | "odt" | "rtf" => (Color32::from_rgb(0x2b, 0x57, 0x9a), "W"),
        "xls" | "xlsx" | "ods" | "csv" => (Color32::from_rgb(0x1f, 0x72, 0x44), "X"),
        "ppt" | "pptx" | "odp" => (Color32::from_rgb(0xc4, 0x48, 0x2a), "P"),
        "pdf" => (Color32::from_rgb(0xd1, 0x34, 0x38), "P"),
        "md" => (Color32::from_rgb(0x4b, 0x52, 0x60), "M"),
        "txt" | "log" => (Color32::from_rgb(0x5b, 0x62, 0x70), "T"),
        "exe" | "msi" | "bat" | "cmd" => (Color32::from_rgb(0x2f, 0x7d, 0x8c), "E"),
        "lnk" | "url" => (Color32::from_rgb(0x4b, 0x6a, 0x9a), "L"),
        _ => match it.cat {
            CAT_CODE => (Color32::from_rgb(0x3d, 0x5f, 0x8f), "</>"),
            CAT_ARCHIVE => (Color32::from_rgb(0x8a, 0x6d, 0x3b), "Z"),
            CAT_VIDEO => (Color32::from_rgb(0x6e, 0x52, 0xb8), ""),
            CAT_AUDIO => (Color32::from_rgb(0xb2, 0x4a, 0x80), "♪"),
            CAT_IMAGE => (Color32::from_rgb(0x2a, 0x8a, 0x9a), ""),
            _ => (Color32::from_rgb(0x52, 0x58, 0x64), ""),
        },
    }
}

pub fn paint_folder(p: &Painter, c: Pos2, s: f32) {
    let k = s / 32.0;
    let r = |x0: f32, y0: f32, x1: f32, y1: f32| Rect::from_min_max(pos2(c.x + x0 * k, c.y + y0 * k), pos2(c.x + x1 * k, c.y + y1 * k));
    p.rect_filled(r(-14.0, -10.5, -3.0, -4.0), CornerRadius::same(2), Color32::from_rgb(0xd9, 0xa2, 0x35));
    p.rect_filled(r(-14.0, -7.5, 14.0, 10.0), CornerRadius::same(3), Color32::from_rgb(0xe6, 0xb0, 0x45));
    p.rect_filled(r(-14.0, -3.5, 14.0, 10.0), CornerRadius::same(3), Color32::from_rgb(0xf7, 0xcf, 0x63));
    p.line_segment([pos2(c.x - 13.0 * k, c.y - 3.0 * k), pos2(c.x + 13.0 * k, c.y - 3.0 * k)], Stroke::new(1.0, Color32::from_rgba_premultiplied(60, 54, 30, 60)));
}

/// Küçük dosya simgesi (önizleme listesi): sonuç listesindeki sayfa simgesinin küçüğü, aynı renkler.
pub fn paint_file_small(p: &Painter, c: Pos2, name: &str) {
    let it = Item { name: name.to_string(), cat: util::category(name, false), ..Default::default() };
    let (col, _) = file_badge(&it);
    let page = Rect::from_center_size(c, vec2(11.0, 14.0));
    p.rect_filled(page, CornerRadius::same(2), col);
    let (x1, y0) = (page.right(), page.top());
    p.add(Shape::convex_polygon(vec![pos2(x1 - 4.0, y0), pos2(x1, y0 + 4.0), pos2(x1 - 4.0, y0 + 4.0)], col.gamma_multiply(0.55).additive().to_opaque(), Stroke::NONE));
    p.add(Shape::convex_polygon(vec![pos2(x1 - 4.0, y0), pos2(x1, y0), pos2(x1, y0 + 4.0)], Color32::from_rgb(0x14, 0x16, 0x1b), Stroke::NONE));
    // iki ince "metin" çizgisi
    for k in 0..2 {
        let y = page.top() + 7.0 + k as f32 * 3.0;
        p.line_segment([pos2(page.left() + 2.5, y), pos2(page.right() - 2.5, y)], Stroke::new(1.0, Color32::from_white_alpha(120)));
    }
}

/// Onay kutusu (Çoklu seçim).
fn paint_check(p: &Painter, c: Pos2, on: bool) {
    let r = Rect::from_center_size(c, vec2(16.0, 16.0));
    if on {
        p.rect_filled(r, CornerRadius::same(4), t::ACCENT);
        let col = Color32::from_rgb(0x10, 0x16, 0x1e);
        p.line_segment([pos2(c.x - 4.2, c.y + 0.2), pos2(c.x - 1.2, c.y + 3.2)], Stroke::new(2.0, col));
        p.line_segment([pos2(c.x - 1.2, c.y + 3.2), pos2(c.x + 4.4, c.y - 3.4)], Stroke::new(2.0, col));
    } else {
        p.rect_stroke(r, CornerRadius::same(4), Stroke::new(1.4, Color32::from_rgb(0x6a, 0x70, 0x78)), StrokeKind::Inside);
    }
}

fn paint_icon(ui: &Ui, p: &Painter, c: Pos2, it: &Item, thumb: Option<&egui::TextureHandle>) {
    if it.path == crate::recycle::VIRTUAL_PATH {
        // sanal Geri Dönüşüm Kutusu: çöp kutusu simgesi
        p.rect_filled(Rect::from_center_size(c, vec2(30.0, 30.0)), CornerRadius::same(7), Color32::from_rgb(0x2a, 0x30, 0x3a));
        paint_trash(p, c + vec2(0.0, 1.0), 8.5, Color32::from_rgb(0xa8, 0xcf, 0xf0));
        return;
    }
    if it.is_dir {
        paint_folder(p, c, 32.0);
        return;
    }
    if let Some(tex) = thumb {
        let r = Rect::from_center_size(c, vec2(32.0, 32.0));
        egui::Image::from_texture(egui::load::SizedTexture::from_handle(tex)).corner_radius(6).paint_at(ui, r);
        p.rect_stroke(r, CornerRadius::same(6), Stroke::new(1.0, Color32::from_rgba_premultiplied(20, 20, 20, 20)), StrokeKind::Inside);
        return;
    }
    let (col, letter) = file_badge(it);
    let page = Rect::from_center_size(c, vec2(22.0, 26.0));
    p.rect_filled(page, CornerRadius::same(3), col);
    // köşe kıvrımı
    let (x1, y0) = (page.right(), page.top());
    p.add(Shape::convex_polygon(vec![pos2(x1 - 7.0, y0), pos2(x1, y0 + 7.0), pos2(x1 - 7.0, y0 + 7.0)], col.gamma_multiply(0.55).additive().to_opaque(), Stroke::NONE));
    p.add(Shape::convex_polygon(vec![pos2(x1 - 7.0, y0), pos2(x1, y0), pos2(x1, y0 + 7.0)], Color32::from_rgb(0x1a, 0x1c, 0x21), Stroke::NONE));
    match it.cat {
        CAT_VIDEO if letter.is_empty() => {
            p.add(Shape::convex_polygon(vec![pos2(c.x - 3.5, c.y - 4.5), pos2(c.x + 5.0, c.y + 1.0), pos2(c.x - 3.5, c.y + 6.0)], Color32::WHITE, Stroke::NONE));
        }
        CAT_IMAGE if letter.is_empty() => {
            p.add(Shape::convex_polygon(vec![pos2(c.x - 8.0, c.y + 9.0), pos2(c.x - 2.0, c.y + 1.0), pos2(c.x + 3.0, c.y + 9.0)], Color32::from_white_alpha(230), Stroke::NONE));
            p.add(Shape::convex_polygon(vec![pos2(c.x, c.y + 9.0), pos2(c.x + 4.0, c.y + 4.0), pos2(c.x + 8.0, c.y + 9.0)], Color32::from_white_alpha(170), Stroke::NONE));
            p.circle_filled(pos2(c.x + 3.5, c.y - 3.0), 2.2, Color32::from_rgb(0xff, 0xe0, 0x8a));
        }
        _ => {
            if !letter.is_empty() {
                let f = if letter.chars().count() > 1 { sb(8.5) } else { sb(10.5) };
                p.text(pos2(c.x, c.y + 2.0), Align2::CENTER_CENTER, letter, f, Color32::WHITE);
            }
        }
    }
}

fn paint_magnifier(p: &Painter, c: Pos2, col: Color32) {
    p.circle_stroke(pos2(c.x - 1.5, c.y - 1.5), 5.6, Stroke::new(1.6, col));
    p.line_segment([pos2(c.x + 2.7, c.y + 2.7), pos2(c.x + 6.5, c.y + 6.5)], Stroke::new(1.8, col));
}

fn paint_keycap(p: &Painter, c: Pos2, col: Color32) {
    let r = Rect::from_center_size(c, vec2(11.0, 8.0));
    p.rect_stroke(r, CornerRadius::same(2), Stroke::new(1.0, col), StrokeKind::Inside);
    p.line_segment([pos2(r.left() + 3.0, r.bottom() - 2.5), pos2(r.right() - 3.0, r.bottom() - 2.5)], Stroke::new(1.0, col));
}

// ---------------- Ana çizim ----------------

pub fn draw(app: &mut App, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    // tuşlar (menü konumu için ön düzen)
    let l0 = layout(app);
    let scroll0 = app.scroll;
    let lt = l0.list_top;
    let pl = l0.panel.left();
    app.handle_keys(&ctx, move |i| pos2(pl + 330.0, lt + LIST_PAD_T + i.saturating_sub(scroll0) as f32 * ROW_H + 30.0));
    app.fix_scroll();
    app.request_preview();
    let mut l = layout(app);
    if matches!(app.mode, Mode::Note { .. }) {
        // Not al: liste alanı yazma alanına ayrılır; satır çizilmez, tıklanmaz
        l.rows = 0;
    }

    // pencere boyutu
    // pencere tam olarak kutu boyutunda (saydam kenar yok)
    let size = vec2(l.panel.width() + 2.0 * MARGIN, (l.panel.bottom() + MARGIN_BOTTOM).ceil());
    // DPI (ölçek) değişince de yeniden boyutlandır/konumla
    let ppp = ctx.pixels_per_point();
    if size != app.last_size || ppp != app.last_ppp {
        app.last_size = size;
        app.last_ppp = ppp;
        crate::platform::resize(&ctx, size);
    }

    let painter = ui.painter().clone();
    let panel = l.panel;
    // panel: pencerenin tamamı (köşe yuvarlatmayı Windows 11'de DWM yapar)
    painter.rect_filled(ui.ctx().content_rect(), CornerRadius::ZERO, t::PANEL_BOTTOM);
    painter.rect_filled(panel, CornerRadius::ZERO, t::PANEL_BOTTOM);
    painter.rect_filled(Rect::from_min_size(panel.min, vec2(panel.width(), SEARCH_H)), CornerRadius::ZERO, t::PANEL_TOP);
    painter.rect_filled(l.footer, CornerRadius::ZERO, t::FOOTER);

    // arama satırı
    paint_magnifier(&painter, pos2(panel.left() + 28.0, panel.top() + SEARCH_H / 2.0), Color32::from_rgb(0x8b, 0x91, 0x9a));
    // sağdaki çip: uzantı süzgeci "pdf ✕"
    let mut chip_x = panel.right() - 16.0;
    let mut ext_chip_x: Option<Rect> = None;
    let mut mode_chip: Option<Rect> = None;
    let cy = panel.top() + SEARCH_H / 2.0;
    if let Some(e) = &app.ext {
        let tw = text_w(&painter, e, sb(12.0));
        let r = Rect::from_min_max(pos2(chip_x - tw - 38.0, cy - 12.0), pos2(chip_x, cy + 12.0));
        painter.rect(r, CornerRadius::same(12), t::accent_dim(), Stroke::new(1.0, t::ACCENT.gamma_multiply(0.6)), StrokeKind::Inside);
        painter.text(pos2(r.left() + 12.0, cy), Align2::LEFT_CENTER, e, sb(12.0), Color32::from_rgb(0xcf, 0xe4, 0xf7));
        let xr = Rect::from_center_size(pos2(r.right() - 13.0, cy), vec2(18.0, 18.0));
        paint_x(&painter, xr.center(), 3.5, Color32::from_rgb(0xa8, 0xcf, 0xf0));
        ext_chip_x = Some(xr.expand(2.0));
        chip_x = r.left() - 8.0;
    }
    // solda arama modu çipi (F1 ya da tıklama: İsim → Uzantı → İçerik → Komut; Shift+F1 ya da sağ tık: geri)
    let mut te_left = panel.left() + 50.0;
    let mode_lab = match app.mode {
        Mode::Search if app.is_cmd() => Some((3, t!("Komut", "Command"))),
        Mode::ExtInput { .. } => Some((1, t!("Uzantı", "Extension"))),
        Mode::Search if app.content_query().is_some() => Some((2, t!("İçerik", "Content"))),
        Mode::Search if app.ext.is_some() => Some((1, t!("Uzantı", "Extension"))),
        Mode::Search => Some((0, t!("İsim", "Name"))),
        _ => None,
    };
    if let Some((kind, lab)) = mode_lab {
        let (bg, bd, fgc) = match kind {
            3 => (Color32::from_rgb(0x36, 0x2c, 0x1c), Color32::from_rgb(0xe8, 0xb0, 0x60).gamma_multiply(0.6), Color32::from_rgb(0xf3, 0xd2, 0x9a)),
            2 => (Color32::from_rgb(0x2a, 0x2f, 0x24), t::OK.gamma_multiply(0.6), Color32::from_rgb(0xc8, 0xe6, 0xb0)),
            1 => (t::accent_dim(), t::ACCENT.gamma_multiply(0.6), Color32::from_rgb(0xcf, 0xe4, 0xf7)),
            _ => (Color32::from_rgb(0x2a, 0x2d, 0x34), Color32::from_rgb(0x3a, 0x3e, 0x46), t::KEY),
        };
        let tw = text_w(&painter, lab, sb(11.5));
        let fw = text_w(&painter, "F1", reg(9.5));
        let r = Rect::from_min_size(pos2(panel.left() + 44.0, cy - 11.0), vec2(tw + fw + 26.0, 22.0));
        let hov = ctx.input(|i| i.pointer.hover_pos()).is_some_and(|p| r.contains(p));
        let bg = if hov { bg.gamma_multiply(1.35) } else { bg };
        painter.rect(r, CornerRadius::same(11), bg, Stroke::new(1.0, bd), StrokeKind::Inside);
        painter.text(pos2(r.left() + 10.0, cy), Align2::LEFT_CENTER, lab, sb(11.5), fgc);
        painter.text(pos2(r.right() - 9.0, cy + 0.5), Align2::RIGHT_CENTER, "F1", reg(9.5), fgc.gamma_multiply(0.55));
        te_left = r.right() + 10.0;
        mode_chip = Some(r);
        if hov {
            // ipucu: ileri/geri (en üst katmanda)
            let tp = ctx.layer_painter(LayerId::new(Order::Tooltip, Id::unique("fp-mode-tip")));
            let l1 = t!("F1 / tık: ileri · Shift+F1 / sağ tık: geri", "F1 / click: next · Shift+F1 / right-click: back");
            let l2 = t!("İsim · Uzantı · İçerik  ·  /komut yazınca: terminalde çalıştır", "Name · Extension · Content  ·  type /command: run in terminal");
            let w = text_w(&tp, l1, reg(11.5)).max(text_w(&tp, l2, reg(11.0))) + 24.0;
            let tr = Rect::from_min_size(pos2(r.left(), r.bottom() + 8.0), vec2(w, 46.0));
            tp.rect_filled(tr.translate(vec2(0.0, 3.0)), CornerRadius::same(8), Color32::from_black_alpha(90));
            tp.rect(tr, CornerRadius::same(8), Color32::from_rgb(0x25, 0x28, 0x30), Stroke::new(1.0, Color32::from_rgb(0x3a, 0x3e, 0x46)), StrokeKind::Inside);
            tp.text(pos2(tr.left() + 12.0, tr.top() + 15.0), Align2::LEFT_CENTER, l1, reg(11.5), t::FG);
            tp.text(pos2(tr.left() + 12.0, tr.top() + 32.0), Align2::LEFT_CENTER, l2, reg(11.0), t::TEXT3);
        }
    }
    let te_rect = Rect::from_min_max(pos2(te_left, panel.top() + 14.0), pos2(chip_x - 6.0, panel.top() + 42.0));
    let hint: String = match &app.mode {
        Mode::Dests => t!("Son hedeflerde ara", "Search recent folders").into(),
        Mode::Bin => t!("Bin'de ara", "Search Bin").into(),
        Mode::Trash => t!("Geri Dönüşüm Kutusu'nda ara", "Search the Recycle Bin").into(),
        Mode::Settings => t!("Ayarlar", "Settings").into(),
        Mode::ExtInput { .. } => t!("Uzantı yazın (ör. pdf) ve Enter", "Type an extension (e.g. pdf) and press Enter").into(),
        Mode::Note { .. } => t!("Not alınıyor · aşağıya yazın", "Taking a note · type below").into(),
        Mode::Search if app.content_query().is_some() => match &app.browse {
            Some(b) => tf!("{} içindeki dosyaların içeriğinde ara", "Search inside files in {}", crate::app::file_name(b)),
            None => t!("Dosyaların içeriğinde ara", "Search inside files").into(),
        },
        Mode::Search => match &app.browse {
            Some(b) => tf!("{} içinde ara", "Search in {}", crate::app::file_name(b)),
            None => t!("Aramak için buraya yazın", "Type here to search").into(),
        },
    };
    let resp = ui.put(
        te_rect,
        TextEdit::singleline(&mut app.query)
            .id(Id::unique("bul-q"))
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::ZERO)
            .font(reg(17.0))
            .text_color(t::FG)
            .hint_text(RichText::new(hint).color(t::TEXT3).font(reg(17.0)))
            .desired_width(te_rect.width())
            .align(Align2::LEFT_CENTER),
    );
    let win_focused = ctx.input(|i| i.viewport().focused) != Some(false);
    let note_mode = matches!(app.mode, Mode::Note { .. });
    if app.edit.is_none() && app.mode != Mode::Settings && !note_mode && win_focused && (app.focus_query || !resp.has_focus()) {
        resp.request_focus();
        app.focus_query = false;
    }
    if app.query_cursor_end {
        // Ctrl+1/2/3 belirteci yazdı: imleç sona
        app.query_cursor_end = false;
        if let Some(mut st) = egui::TextEdit::load_state(&ctx, resp.id) {
            let n = app.query.chars().count();
            st.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(n))));
            st.store(&ctx, resp.id);
        }
    }
    app.after_query_edit();
    painter.line_segment([pos2(panel.left(), panel.top() + SEARCH_H + 0.5), pos2(panel.right(), panel.top() + SEARCH_H + 0.5)], Stroke::new(1.0, t::line()));

    // bağlam şeridi
    let mut crumbs: Vec<(Rect, Option<String>)> = Vec::new();
    let mut strip_click: Option<Rect> = None;
    let mut clip_x: Option<Rect> = None;
    let pointer_now = ctx.input(|i| i.pointer.hover_pos());
    if let Some(sr) = l.strip {
        painter.line_segment([pos2(sr.left(), sr.bottom() - 0.5), pos2(sr.right(), sr.bottom() - 0.5)], Stroke::new(1.0, t::line()));
        let y = sr.center().y;
        let mut x = sr.left() + 18.0;
        let f = reg(12.0);
        // sağda: panodaki öğeler ("Kopyalandı · ad +N")
        let mut right_limit = sr.right() - 14.0;
        if let Some((name, n, verb)) = app.clip_info()
            && app.mode == Mode::Search
        {
            // ✕: şeridi/panoyu temizle
            let xc = pos2(right_limit - 6.0, y);
            let xr = Rect::from_center_size(xc, vec2(18.0, 18.0));
            let xh = pointer_now.is_some_and(|p| xr.contains(p));
            if xh {
                painter.rect_filled(xr, CornerRadius::same(4), Color32::from_rgb(0x2c, 0x30, 0x38));
            }
            paint_x(&painter, xc, 3.5, if xh { t::FG } else { t::TEXT3 });
            clip_x = Some(xr);
            right_limit = xr.left() - 8.0;
            let cut = verb == t!("Kesildi", "Cut");
            let more = if n > 1 { format!(" +{}", n - 1) } else { String::new() };
            let g = painter.layout_job(job(&format!("{name}{more}"), sb(12.0), Color32::from_rgb(0xd5, 0xd8, 0xde), 200.0));
            let gw = g.size().x;
            let gx = right_limit - gw;
            painter.galley(pos2(gx, y - g.size().y / 2.0), g, t::FG);
            let r = painter.text(pos2(gx - 8.0, y), Align2::RIGHT_CENTER, format!("{verb} ·"), f.clone(), if cut { Color32::from_rgb(0xe8, 0xb0, 0x60) } else { t::OK });
            right_limit = r.left() - 16.0;
        }
        match &app.mode {
            Mode::Dests => {
                painter.text(pos2(x, y), Align2::LEFT_CENTER, t!("Son hedefler · en son gidilen klasörler", "Recent folders · most recently visited"), sb(12.0), Color32::from_rgb(0xd5, 0xd8, 0xde));
            }
            Mode::Settings => {
                painter.text(pos2(x, y), Align2::LEFT_CENTER, t!("Ayarlar", "Settings"), sb(12.0), Color32::from_rgb(0xd5, 0xd8, 0xde));
            }
            Mode::Trash => {
                paint_trash(&painter, pos2(x + 5.0, y), 5.5, Color32::from_rgb(0xa8, 0xcf, 0xf0));
                x += 16.0;
                x += painter.text(pos2(x, y), Align2::LEFT_CENTER, crate::recycle::title(), sb(12.0), Color32::from_rgb(0xd5, 0xd8, 0xde)).width() + 8.0;
                x += painter.text(pos2(x, y), Align2::LEFT_CENTER, "·", f.clone(), t::TEXT3).width() + 8.0;
                let n = app.trash.len();
                let g = painter.layout_job(job(&tf!("{n} öğe · en son silinen üstte · geri yüklemek için Tab ya da sağ tık", "{n} items · most recently deleted first · Tab or right-click to restore"), f.clone(), t::TEXT2, (right_limit - x).max(40.0)));
                painter.galley(pos2(x, y - g.size().y / 2.0), g, t::TEXT2);
            }
            Mode::ExtInput { .. } => {
                painter.text(pos2(x, y), Align2::LEFT_CENTER, t!("Uzantıya göre ara · tüm aramalar bu uzantıyla süzülür", "Search by extension · all searches are filtered by this extension"), f.clone(), t::TEXT2);
            }
            Mode::Note { dir } => {
                x += painter.text(pos2(x, y), Align2::LEFT_CENTER, t!("Not al", "Take a note"), sb(12.0), Color32::from_rgb(0xd5, 0xd8, 0xde)).width() + 8.0;
                x += painter.text(pos2(x, y), Align2::LEFT_CENTER, "·", f.clone(), t::TEXT3).width() + 8.0;
                let g = painter.layout_job(job(&tf!("{dir} klasörüne kaydedilecek", "Will be saved to {dir}"), f.clone(), t::TEXT2, (right_limit - x).max(40.0)));
                painter.galley(pos2(x, y - g.size().y / 2.0), g, t::TEXT2);
            }
            Mode::Bin => {
                x += painter.text(pos2(x, y), Align2::LEFT_CENTER, "Bin", sb(12.0), Color32::from_rgb(0xd5, 0xd8, 0xde)).width() + 8.0;
                x += painter.text(pos2(x, y), Align2::LEFT_CENTER, "·", f.clone(), t::TEXT3).width() + 8.0;
                let d = crate::bin::bin_dir().to_string_lossy().to_string();
                x += painter.text(pos2(x, y), Align2::LEFT_CENTER, &d, f.clone(), t::TEXT2).width() + 12.0;
                if !crate::bin::on_path() {
                    let r = painter.text(pos2(x, y), Align2::LEFT_CENTER, t!("PATH'te değil · eklemek için tıklayın", "Not on PATH · click to add"), f.clone(), t::ACCENT);
                    strip_click = Some(r.expand(3.0));
                }
            }
            Mode::Search => {
                if app.multi {
                    // "Çoklu seçim · 3 seçili" etiketi (sağda)
                    let lab = tf!("Çoklu seçim · {} seçili", "Multi-select · {} selected", app.marked.len());
                    let tw = text_w(&painter, &lab, sb(11.5));
                    let r = Rect::from_min_max(pos2(right_limit - tw - 34.0, y - 10.0), pos2(right_limit, y + 10.0));
                    painter.rect_filled(r, CornerRadius::same(10), t::accent_dim());
                    paint_check(&painter, pos2(r.left() + 13.0, y), true);
                    painter.text(pos2(r.left() + 26.0, y), Align2::LEFT_CENTER, &lab, sb(11.5), Color32::from_rgb(0xcf, 0xe4, 0xf7));
                    right_limit = r.left() - 12.0;
                    if app.browse.is_none() {
                        painter.text(pos2(x, y), Align2::LEFT_CENTER, t!("Boşluk ya da tık ile işaretleyin · Ctrl+A ile hepsini seçin", "Mark with Space or click · Ctrl+A selects all"), f.clone(), t::TEXT2);
                    }
                }
                if let Some(b) = &app.browse {
                    // "Tümü ›  C:\ › Users › …"
                    let segs = crate::app::crumb_segments(b);
                    let n = segs.len();
                    let hl = app.crumb_sel;
                    // sığmazsa baştan kısalt (Tümü hep görünür; vurgulanan bölüm de görünür kalsın)
                    let avail = right_limit - x - 40.0;
                    let total = |s: &[(String, Option<String>)]| s.iter().map(|x| text_w(&painter, &x.0, f.clone()) + 18.0).sum::<f32>();
                    let mut skip = 1;
                    while skip + 1 < n && total(&segs[skip..]) + text_w(&painter, t!("Tümü", "All"), f.clone()) > avail && hl.is_none_or(|h| h > skip) {
                        skip += 1;
                    }
                    for (k, (label, path)) in segs.into_iter().enumerate() {
                        if k > 0 && k < skip {
                            continue;
                        }
                        if k > 0 {
                            if k == skip && skip > 1 {
                                x += painter.text(pos2(x, y), Align2::LEFT_CENTER, "› …", f.clone(), Color32::from_rgb(0x5c, 0x61, 0x68)).width() + 6.0;
                            }
                            x += painter.text(pos2(x, y), Align2::LEFT_CENTER, "›", f.clone(), Color32::from_rgb(0x5c, 0x61, 0x68)).width() + 6.0;
                        }
                        let last = k + 1 == n;
                        let lit = hl == Some(k);
                        let (font, col) = if lit {
                            (sb(12.0), Color32::from_rgb(0x10, 0x16, 0x1e))
                        } else if last {
                            (sb(12.0), Color32::from_rgb(0xd0, 0xd4, 0xda))
                        } else {
                            (f.clone(), t::TEXT2)
                        };
                        if lit {
                            let tw = text_w(&painter, &label, font.clone());
                            painter.rect_filled(Rect::from_min_size(pos2(x - 6.0, y - 10.0), vec2(tw + 12.0, 20.0)), CornerRadius::same(6), t::ACCENT);
                        }
                        let r = painter.text(pos2(x, y), Align2::LEFT_CENTER, &label, font, col);
                        crumbs.push((r.expand2(vec2(4.0, 6.0)), path));
                        x += r.width() + 6.0;
                    }
                }
            }
        }
    }

    // liste
    let searching = !app.query.trim().is_empty();
    let pointer = ctx.input(|i| i.pointer.hover_pos());
    // Fare gerçekten hareket etti mi? (menü/onay fare üzerindeyken açılsa da oklar çalışsın:
    // üzerine gelme seçimi yalnızca fare hareket edince değişir)
    let pointer_moved = pointer.is_some() && app.last_pointer.is_some() && pointer != app.last_pointer;
    app.last_pointer = pointer;
    let overlay = app.menu.is_some() || app.confirm.is_some();
    app.hover_row = None;
    if !overlay
        && let Some(pp) = pointer
        && l.list_rect.contains(pp)
    {
        let vi = ((pp.y - l.list_top - LIST_PAD_T) / ROW_H).floor();
        if vi >= 0.0 {
            let i = app.scroll + vi as usize;
            if i < app.items.len() && (vi as usize) < l.rows {
                app.hover_row = Some(i);
            }
        }
    }
    let mut want: Vec<String> = Vec::new();
    let list_clip = painter.with_clip_rect(l.list_rect);
    if app.items.is_empty() {
        let msg = match (&app.mode, app.browse.is_some(), searching) {
            (_, _, true) => t!("Eşleşen öğe yok", "No matching items"),
            (Mode::Dests, _, _) => t!("Henüz hedef yok", "No recent folders yet"),
            (Mode::Bin, _, _) => t!("Bin boş · bir öğede Tab, sonra b ile ekleyin", "Bin is empty · press Tab on an item, then b to add it"),
            (Mode::Trash, _, _) => t!("Geri Dönüşüm Kutusu boş", "The Recycle Bin is empty"),
            (Mode::Search, _, _) if app.content_query().is_some_and(|q| q.is_empty()) => t!("İçerikte aranacak metni yazın", "Type the text to search for inside files"),
            (Mode::Settings | Mode::ExtInput { .. } | Mode::Note { .. }, _, _) => "",
            (Mode::Search, true, _) if app.ext.is_some() => t!("Bu klasörde bu uzantıda dosya yok", "No files with this extension in this folder"),
            (Mode::Search, false, _) if app.ext.is_some() => t!("Bu uzantıda dosya bulunamadı", "No files with this extension found"),
            (_, true, _) => t!("Bu klasör boş", "This folder is empty"),
            _ => "",
        };
        if app.cmd_line().is_some() {
            // "/komut" kartı
            let one = app.cmd_list().into_iter().next().unwrap_or_default();
            let q = one.as_str();
            let dir = app.cmd_dir().unwrap_or_else(|| t!("kullanıcı klasörü", "user folder").into());
            let r = row_rect(&l, 0);
            let cmds = app.cmd_list();
            let enter_hint = if matches!(app.term_choice(), crate::terminal::Term::Hyper | crate::terminal::Term::Custom) { t!("terminali bu klasörde aç · komut panoya kopyalanır", "open the terminal here · the command is copied") } else { t!("terminalde çalıştır (ayrı pencere, açık kalır)", "run in terminal (separate window, stays open)") };
            if cmds.len() > 1 {
                // birden çok komut: numaralı satırlar
                let card = Rect::from_min_size(pos2(r.left() + 12.0, r.top() + 6.0), vec2(r.width() - 24.0, cmd_card_height(cmds.len())));
                let painter = painter.with_clip_rect(card.intersect(painter.clip_rect()).intersect(l.list_rect));
                painter.rect(card, CornerRadius::same(10), Color32::from_rgb(0x1b, 0x1e, 0x24), Stroke::new(1.0, Color32::from_rgb(0x34, 0x38, 0x40)), StrokeKind::Inside);
                painter.rect_filled(Rect::from_min_max(pos2(card.left(), card.top() + 12.0), pos2(card.left() + 3.0, card.bottom() - 12.0)), CornerRadius::same(2), Color32::from_rgb(0xe8, 0xb0, 0x60));
                paint_terminal(&painter, pos2(card.left() + 28.0, card.top() + 26.0), Color32::from_rgb(0xf3, 0xd2, 0x9a));
                let x = card.left() + 48.0;
                let pr = painter.text(pos2(x, card.top() + 26.0), Align2::LEFT_CENTER, "›", sb(16.0), Color32::from_rgb(0xe8, 0xb0, 0x60));
                painter.text(pos2(pr.right() + 8.0, card.top() + 26.0), Align2::LEFT_CENTER, tf!("{} komut · sırayla çalışır, biri başarısız olursa durur", "{} commands · run in order, stop if one fails", cmds.len()), reg(12.0), t::TEXT2);
                let mut y = card.top() + 50.0;
                for (i, c) in cmds.iter().enumerate() {
                    let nr = painter.text(pos2(x + 18.0, y), Align2::RIGHT_CENTER, format!("{}.", i + 1), FontId::monospace(13.5), t::TEXT3);
                    painter.text(pos2(nr.right() + 8.0, y), Align2::LEFT_CENTER, c, FontId::monospace(13.5), t::FG);
                    y += 20.0;
                }
                let g = painter.layout_job(job(&tf!("Klasör: {dir}", "Folder: {dir}"), reg(11.5), t::TEXT2, card.width() - 64.0));
                painter.galley(pos2(x, y + 2.0), g, t::TEXT2);
                let kx = painter.text(pos2(x, y + 30.0), Align2::LEFT_CENTER, "Enter", sb(11.5), t::KEY);
                painter.text(pos2(kx.right() + 6.0, y + 30.0), Align2::LEFT_CENTER, enter_hint, reg(11.5), t::TEXT2);
            } else if q.is_empty() {
                painter.text(pos2(r.left() + 20.0, r.center().y), Align2::LEFT_CENTER, t!("Komutu '/' işaretinden sonra yazın (ör. /ipconfig) · Enter ile terminalde çalışır", "Type the command after '/' (e.g. /ipconfig) · Enter runs it in a terminal"), reg(13.0), t::TEXT3);
            } else {
                let card = Rect::from_min_size(pos2(r.left() + 12.0, r.top() + 6.0), vec2(r.width() - 24.0, 92.0));
                let painter = painter.with_clip_rect(card.intersect(painter.clip_rect()));
                painter.rect(card, CornerRadius::same(10), Color32::from_rgb(0x1b, 0x1e, 0x24), Stroke::new(1.0, Color32::from_rgb(0x34, 0x38, 0x40)), StrokeKind::Inside);
                painter.rect_filled(Rect::from_min_max(pos2(card.left(), card.top() + 12.0), pos2(card.left() + 3.0, card.bottom() - 12.0)), CornerRadius::same(2), Color32::from_rgb(0xe8, 0xb0, 0x60));
                paint_terminal(&painter, pos2(card.left() + 28.0, card.top() + 26.0), Color32::from_rgb(0xf3, 0xd2, 0x9a));
                let x = card.left() + 48.0;
                let pr = painter.text(pos2(x, card.top() + 26.0), Align2::LEFT_CENTER, "›", sb(16.0), Color32::from_rgb(0xe8, 0xb0, 0x60));
                painter.text(pos2(pr.right() + 8.0, card.top() + 26.0), Align2::LEFT_CENTER, q, FontId::monospace(15.0), t::FG);
                let g = painter.layout_job(job(&tf!("Klasör: {dir}", "Folder: {dir}"), reg(11.5), t::TEXT2, card.width() - 64.0));
                painter.galley(pos2(x, card.top() + 44.0), g, t::TEXT2);
                let kx = painter.text(pos2(x, card.top() + 72.0), Align2::LEFT_CENTER, "Enter", sb(11.5), t::KEY);
                painter.text(pos2(kx.right() + 6.0, card.top() + 72.0), Align2::LEFT_CENTER, enter_hint, reg(11.5), t::TEXT2);
            }
        } else if !app.busy_waiting() && app.mode != Mode::Settings && !note_mode {
            let r = row_rect(&l, 0);
            painter.text(pos2(r.left() + 20.0, r.center().y), Align2::LEFT_CENTER, msg, reg(13.0), t::TEXT3);
        }
    }
    let edit_row = app.edit.as_ref().map(|e| e.row);
    let mq = app.match_query();
    let content_mode = app.content_query().is_some();
    let mut settings_hits: Vec<(Rect, usize)> = Vec::new();
    if app.mode == Mode::Settings {
        let editing_term = app.settings_edit.is_some();
        let rows: [(String, String, String); SETTINGS_ROWS] = [
            (
                t!("Genel kısayol", "Global shortcut").into(),
                if app.recording { t!("Yeni kombinasyona basın… (Esc iptal)", "Press the new combination… (Esc cancels)").into() } else { t!("Pencereyi her yerden aç/kapat · değiştirmek için Enter", "Show/hide the window from anywhere · Enter to change").into() },
                if app.recording { "…".into() } else if crate::platform::hotkey_ok() { app.config.hotkey.label() } else { tf!("{} (kullanılamıyor)", "{} (unavailable)", app.config.hotkey.label()) },
            ),
            (t!("Windows açılışında başlat", "Start with Windows").into(), t!("Bilgisayar açılınca arka planda hazır bekler", "Waits ready in the background after sign-in").into(), if app.config.autostart { t!("Açık", "On").into() } else { t!("Kapalı", "Off").into() }),
            (
                "Terminal".into(),
                if editing_term { String::new() } else { t!("\"Terminal aç\" bununla açılır · Enter ya da Sol/Sağ ile değiştir", "\"Open terminal\" uses this · change with Enter or Left/Right").into() },
                if editing_term { t!("Kaydet", "Save").into() } else { app.term_label() },
            ),
            ("Dil / Language".into(), t!("Arayüz dili · Enter ya da Sol/Sağ ile değiştir", "Interface language · change with Enter or Left/Right").into(), crate::i18n::get().label().into()),
        ];
        for (k, (title, sub, val)) in rows.iter().enumerate() {
            let r = row_rect(&l, k);
            let sel = app.settings_sel == k;
            if sel {
                list_clip.rect_filled(r, CornerRadius::ZERO, t::accent_dim());
                list_clip.rect_filled(Rect::from_min_max(pos2(r.left(), r.top() + 10.0), pos2(r.left() + 3.0, r.bottom() - 10.0)), CornerRadius { nw: 0, sw: 0, ne: 2, se: 2 }, t::ACCENT);
            }
            let ic = pos2(r.left() + 28.0, r.center().y);
            match k {
                0 => paint_keycap(&list_clip, ic, Color32::from_rgb(0xa8, 0xcf, 0xf0)),
                1 => paint_gear(&list_clip, ic, Color32::from_rgb(0xa8, 0xcf, 0xf0)),
                3 => paint_globe(&list_clip, ic, Color32::from_rgb(0xa8, 0xcf, 0xf0)),
                _ => paint_terminal(&list_clip, ic, Color32::from_rgb(0xa8, 0xcf, 0xf0)),
            }
            list_clip.text(pos2(r.left() + 54.0, r.top() + 9.0), Align2::LEFT_TOP, title, sb(14.0), t::FG);
            list_clip.text(pos2(r.left() + 54.0, r.top() + 29.0), Align2::LEFT_TOP, sub, reg(11.5), t::TEXT2);
            // değer düğmesi
            let on = k == 1 && app.config.autostart;
            let tw = text_w(&list_clip, val, sb(12.5));
            let br = Rect::from_min_max(pos2(r.right() - 24.0 - tw - 24.0, r.center().y - 14.0), pos2(r.right() - 24.0, r.center().y + 14.0));
            let (bg, fgc) = if (app.recording && k == 0) || (editing_term && k == 2) {
                (t::ACCENT, Color32::from_rgb(0x10, 0x16, 0x1e))
            } else if on {
                (Color32::from_rgb(0x2a, 0x4a, 0x2e), Color32::from_rgb(0xc8, 0xe6, 0xb0))
            } else {
                (Color32::from_rgb(0x2c, 0x30, 0x38), if k == 0 && !crate::platform::hotkey_ok() { t::ERR } else { t::FG })
            };
            list_clip.rect(br, CornerRadius::same(8), bg, Stroke::NONE, StrokeKind::Inside);
            list_clip.text(br.center(), Align2::CENTER_CENTER, val, sb(12.5), fgc);
            settings_hits.push((r, k));
        }
        // "Başka bir program": yol yazma alanı (terminal satırında)
        if editing_term {
            let r = row_rect(&l, 2);
            let fr = Rect::from_min_max(pos2(r.left() + 50.0, r.top() + 27.0), pos2(r.right() - 140.0, r.top() + 47.0));
            list_clip.rect(fr, CornerRadius::same(5), Color32::from_rgb(0x1f, 0x22, 0x28), Stroke::new(1.0, t::ACCENT), StrokeKind::Inside);
            let inner = fr.shrink2(vec2(6.0, 1.0));
            let id = Id::unique("bul-term-path");
            if let Some(text) = app.settings_edit.as_mut() {
                let resp = ui.put(
                    inner,
                    TextEdit::singleline(text)
                        .id(id)
                        .frame(egui::Frame::NONE)
                        .margin(egui::Margin::ZERO)
                        .font(reg(12.0))
                        .text_color(t::FG)
                        .hint_text(RichText::new(t!("Programın tam yolu (ör. C:\\Araçlar\\terminal.exe)", "Full path of the program (e.g. C:\\Tools\\terminal.exe)")).color(t::TEXT3).font(reg(12.0)))
                        .desired_width(inner.width())
                        .align(Align2::LEFT_CENTER),
                );
                if !resp.has_focus() && ui.ctx().input(|i| i.viewport().focused) != Some(false) {
                    resp.request_focus();
                }
            }
        }
    }
    // Not al: çok satırlı yazma alanı (liste alanında)
    if note_mode {
        let area = l.list_rect.shrink2(vec2(16.0, 8.0));
        painter.rect(area, CornerRadius::same(8), Color32::from_rgb(0x1a, 0x1d, 0x23), Stroke::new(1.0, t::ACCENT.gamma_multiply(0.7)), StrokeKind::Inside);
        let inner = area.shrink2(vec2(10.0, 8.0));
        let id = Id::unique("bul-note");
        let mut ui2 = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::top_down(egui::Align::Min)));
        ui2.set_clip_rect(inner);
        let resp = egui::ScrollArea::vertical()
            .id_salt("bul-note-scroll")
            .max_height(inner.height())
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(&mut ui2, |ui| {
                ui.add_sized(
                    vec2(inner.width() - 12.0, inner.height()),
                    TextEdit::multiline(&mut app.note_text)
                        .id(id)
                        .frame(egui::Frame::NONE)
                        .margin(egui::Margin::ZERO)
                        .font(reg(14.0))
                        .text_color(t::FG)
                        .hint_text(RichText::new(t!("Notunuzu yazın ya da yapıştırın · Enter ile kaydedilir (Shift+Enter yeni satır)", "Type or paste your note · Enter saves (Shift+Enter new line)")).color(t::TEXT3).font(reg(14.0)))
                        .desired_width(inner.width() - 12.0)
                        .lock_focus(true),
                )
            })
            .inner;
        if !resp.has_focus() && win_focused && app.confirm.is_none() && app.menu.is_none() {
            resp.request_focus();
        }
    }
    for vi in 0..l.rows {
        if app.mode == Mode::Settings {
            break;
        }
        let i = app.scroll + vi;
        if i >= app.items.len() {
            break;
        }
        let r = row_rect(&l, vi);
        let mut it = app.items[i].clone();
        if edit_row == Some(i)
            && let Some(Edit { kind: EditKind::New { .. }, text, .. }) = &app.edit
        {
            // yeni öğe: yazılan ada göre dosya/klasör simgesi
            it.name = text.clone();
            it.is_dir = !crate::app::new_item_is_file(text);
            it.cat = util::category(text, it.is_dir);
        }
        let selected = i == app.sel;
        let marked = app.is_marked(i);
        if marked && !selected {
            list_clip.rect_filled(r, CornerRadius::ZERO, Color32::from_rgb(0x1f, 0x33, 0x47));
            list_clip.rect_filled(Rect::from_min_max(pos2(r.left(), r.top() + 10.0), pos2(r.left() + 3.0, r.bottom() - 10.0)), CornerRadius { nw: 0, sw: 0, ne: 2, se: 2 }, t::ACCENT.gamma_multiply(0.7));
        }
        if selected {
            list_clip.rect_filled(r, CornerRadius::ZERO, t::accent_dim());
            list_clip.rect_filled(Rect::from_min_max(pos2(r.left(), r.top() + 10.0), pos2(r.left() + 3.0, r.bottom() - 10.0)), CornerRadius { nw: 0, sw: 0, ne: 2, se: 2 }, t::ACCENT);
        } else if app.hover_row == Some(i) {
            list_clip.rect_filled(r, CornerRadius::ZERO, Color32::from_rgba_premultiplied(10, 10, 10, 10));
        }
        // Çoklu seçim: satır başında onay kutusu, simge ve metin sağa kayar
        let shift_x = if app.multi && app.mode == Mode::Search && !it.path.is_empty() {
            paint_check(&list_clip, pos2(r.left() + 20.0, r.center().y), marked);
            24.0
        } else {
            0.0
        };
        let ic = pos2(r.left() + 4.0 + 24.0 + shift_x, r.center().y);
        let thumb = if it.cat == CAT_IMAGE && !it.is_dir {
            match app.thumbs.get(&it.path) {
                Some(Thumb::Ready(t)) => Some(t.clone()),
                None => {
                    want.push(it.path.clone());
                    None
                }
                _ => None,
            }
        } else {
            None
        };
        paint_icon(ui, &list_clip, ic, &it, thumb.as_ref());
        let bx = r.left() + 54.0 + shift_x;
        let meta_right = r.right() - 20.0;
        let body_w = meta_right - 96.0 - 16.0 - bx;
        if edit_row == Some(i) {
            paint_row_edit(app, ui, &painter, Rect::from_min_size(pos2(bx - 4.0, r.top() + 5.0), vec2(body_w + 40.0, 24.0)));
            let hint = match app.edit.as_ref().map(|e| &e.kind) {
                Some(EditKind::BinAlias { .. }) => t!("Bin takma adı · Enter ekle · Esc iptal", "Bin alias · Enter adds · Esc cancels"),
                Some(EditKind::New { .. }) => t!("Yeni · uzantılı ad = dosya, uzantısız = klasör · Enter oluştur · Esc iptal", "New · name with extension = file, without = folder · Enter creates · Esc cancels"),
                _ => t!("Enter uygula · Esc iptal", "Enter applies · Esc cancels"),
            };
            list_clip.text(pos2(bx, r.top() + 35.0), Align2::LEFT_TOP, hint, reg(11.5), t::TEXT3);
        } else {
            if content_mode && let Some((ln, line)) = &it.line {
                // içerik eşleşmesi: ad + klasör, altında eşleşen satır (vurgulu)
                let nw = text_w(&list_clip, &it.name, sb(14.0)).min(body_w * 0.6);
                list_clip.galley(pos2(bx, r.top() + 8.0), list_clip.layout_job(job(&it.name, sb(14.0), t::FG, body_w * 0.6)), t::FG);
                let par = crate::app::parent_of(&it.path).unwrap_or_default();
                list_clip.galley(pos2(bx + nw + 10.0, r.top() + 10.5), list_clip.layout_job(job(&par, reg(11.5), t::TEXT3, (body_w - nw - 10.0).max(10.0))), t::TEXT3);
                let pre = format!("{ln}: ");
                let pw = text_w(&list_clip, &pre, reg(11.5));
                list_clip.text(pos2(bx, r.top() + 27.0), Align2::LEFT_TOP, &pre, reg(11.5), t::TEXT3);
                let trimmed = line.trim_start();
                // eşleşmeyi görünür tut: uzun satırda eşleşmeden biraz önce başla
                let folded: Vec<char> = trimmed.chars().map(util::fold_char).collect();
                let qc: Vec<char> = util::fold(&mq).chars().collect();
                let at = (0..folded.len().saturating_sub(qc.len().saturating_sub(1))).find(|&k| !qc.is_empty() && folded[k..].starts_with(&qc)).unwrap_or(0);
                let start = at.saturating_sub(24);
                let shown: String = if start > 0 { format!("…{}", trimmed.chars().skip(start).collect::<String>()) } else { trimmed.to_string() };
                let lr = search::match_ranges(&shown, &mq);
                let mut j = LayoutJob::default();
                let chars: Vec<char> = shown.chars().collect();
                let fn_ = TextFormat { font_id: reg(11.5), color: t::TEXT2, ..Default::default() };
                let fh = TextFormat { font_id: sb(11.5), color: t::HIT_FG, background: t::hit_bg(selected), ..Default::default() };
                let mut k0 = 0;
                for (a, b) in lr.into_iter().filter(|(a, b)| b - a > 1 || qc.len() <= 1) {
                    if a > k0 {
                        j.append(&chars[k0..a].iter().collect::<String>(), 0.0, fn_.clone());
                    }
                    j.append(&chars[a..b].iter().collect::<String>(), 0.0, fh.clone());
                    k0 = b;
                }
                if k0 < chars.len() {
                    j.append(&chars[k0..].iter().collect::<String>(), 0.0, fn_);
                }
                j.wrap.max_width = body_w - pw;
                j.wrap.max_rows = 1;
                j.wrap.break_anywhere = true;
                j.wrap.overflow_character = Some('…');
                list_clip.galley(pos2(bx + pw, r.top() + 27.0), list_clip.layout_job(j), t::TEXT2);
            } else {
            paint_name(&list_clip, pos2(bx, r.top() + 8.0), &it.name, &mq, selected, body_w);
            let trash_view = app.mode == Mode::Trash;
            let sub = if trash_view {
                // özgün klasör (kayıt yoksa $R yolu)
                app.trash.iter().find(|e| e.r_path == it.path).map(|e| e.orig_dir()).unwrap_or_else(|| it.path.clone())
            } else if it.path == crate::recycle::VIRTUAL_PATH {
                t!("Silinen öğeler · içine girmek için Enter ya da çift tık", "Deleted items · Enter or double-click to open").to_string()
            } else if app.browse.is_some() && !searching && app.mode == Mode::Search {
                util::type_label(&it.name, it.is_dir)
            } else if app.mode == Mode::Bin {
                it.path.clone()
            } else {
                crate::app::parent_of(&it.path).unwrap_or_default()
            };
            if trash_view {
                // küçük çöp kutusu simgesi + özgün klasör
                let col = Color32::from_rgb(0x8c, 0x93, 0x9c);
                paint_trash(&list_clip, pos2(bx + 5.0, r.top() + 34.5), 4.6, col);
                let g = list_clip.layout_job(job(&sub, reg(11.5), t::TEXT2, body_w - 16.0));
                list_clip.galley(pos2(bx + 16.0, r.top() + 27.0), g, t::TEXT2);
            } else {
                let g = list_clip.layout_job(job(&sub, reg(11.5), t::TEXT2, body_w));
                list_clip.galley(pos2(bx, r.top() + 27.0), g, t::TEXT2);
            }
            }
        }
        if it.path.is_empty() {
            continue; // yeni öğe yer tutucusu: boyut/tarih yok
        }
        let (size_s, muted) = if it.is_dir && !it.path.is_empty() && let Some(sz) = app.dirs.get(&it.path) { (util::fmt_size(sz), true) } else if it.is_dir { (t!("Klasör", "Folder").to_string(), true) } else { (util::fmt_size(it.size), false) };
        list_clip.text(pos2(meta_right, r.top() + 10.0), Align2::RIGHT_TOP, size_s, if muted { reg(11.5) } else { FontId::new(11.5, semi()) }, if muted { t::TEXT3 } else { t::META_SIZE });
        list_clip.text(pos2(meta_right, r.top() + 26.0), Align2::RIGHT_TOP, fmt_day(it.mtime), reg(11.5), t::TEXT2);
    }
    for w in want {
        app.want_thumb(&w);
    }
    // kaydırma göstergesi
    if l.rows > 0 && app.items.len() > l.rows {
        let track = l.list_rect.shrink2(vec2(0.0, 6.0));
        let frac = l.rows as f32 / app.items.len() as f32;
        let h = (track.height() * frac).max(18.0);
        let top = track.top() + (track.height() - h) * (app.scroll as f32 / (app.items.len() - l.rows).max(1) as f32);
        painter.rect_filled(Rect::from_min_size(pos2(l.list_rect.right() - 5.0, top), vec2(3.0, h)), CornerRadius::same(2), Color32::from_rgba_premultiplied(30, 30, 30, 30));
    }

    // önizleme
    let mut preview_img_rect: Option<Rect> = None;
    if let Some(pr) = l.preview {
        preview_img_rect = paint_preview(app, ui, &painter, pr);
    }

    // alt satır
    painter.line_segment([pos2(l.footer.left(), l.footer.top() + 0.5), pos2(l.footer.right(), l.footer.top() + 0.5)], Stroke::new(1.0, t::line()));
    let hints = footer_hints(app);
    let mut hint_rects: Vec<(Rect, &'static str)> = Vec::new();
    let fy = l.footer.center().y;
    if let Some((msg, ok, _)) = &app.toast {
        let col = if *ok { t::OK } else { t::ERR };
        painter.circle_filled(pos2(l.footer.left() + 22.0, fy), 3.0, col);
        let g = painter.layout_job(job(msg, reg(11.5), t::FG, PANEL_W - 160.0));
        painter.galley(pos2(l.footer.left() + 32.0, fy - g.size().y / 2.0), g, t::FG);
    }
    // sağ alt: ayarlar dişlisi + genel kısayol
    let gear_c = pos2(l.footer.right() - 22.0, fy);
    let gear_rect = Rect::from_center_size(gear_c, vec2(24.0, 24.0));
    let gear_hover = pointer.is_some_and(|p| gear_rect.contains(p));
    paint_gear(&painter, gear_c, if gear_hover { t::FG } else { Color32::from_rgb(0x7a, 0x80, 0x88) });
    let hk_label = app.config.hotkey.label();
    let hk = if crate::platform::hotkey_ok() { hk_label } else { tf!("{hk_label} kullanılamıyor", "{hk_label} unavailable") };
    let hcol = if crate::platform::hotkey_ok() { Color32::from_rgb(0x7a, 0x80, 0x88) } else { t::ERR };
    let rs = painter.text(pos2(gear_rect.left() - 4.0, fy), Align2::RIGHT_CENTER, "Ctrl+S", sb(10.5), t::KEY.gamma_multiply(0.8));
    painter.line_segment([pos2(rs.left() - 9.0, fy - 6.0), pos2(rs.left() - 9.0, fy + 6.0)], Stroke::new(1.0, t::line()));
    let r = painter.text(pos2(rs.left() - 18.0, fy), Align2::RIGHT_CENTER, &hk, reg(11.0), hcol);
    paint_keycap(&painter, pos2(r.left() - 11.0, fy), hcol.gamma_multiply(0.8));
    let hk_rect = r.expand2(vec2(18.0, 8.0));
    // Ctrl+S yazısı da dişli gibi tıklanır
    let gear_rect = gear_rect.union(rs.expand2(vec2(4.0, 8.0)));
    if app.toast.is_none() {
        // ipuçları: her biri tıklanınca işini yapar (yalnızca fareyle kullanım); sağdaki kısayol alanına taşmaz
        let limit = r.left() - 30.0;
        let mut x = l.footer.left() + 16.0;
        for (k, lab) in hints {
            let w = text_w(&painter, k, sb(10.5)) + 4.0 + text_w(&painter, lab, reg(10.5));
            if x + w > limit {
                break;
            }
            let r1 = painter.text(pos2(x, fy), Align2::LEFT_CENTER, k, sb(10.5), t::KEY);
            let r2 = painter.text(pos2(r1.right() + 4.0, fy), Align2::LEFT_CENTER, lab, reg(10.5), t::TEXT3);
            let hr = r1.union(r2).expand2(vec2(4.0, 8.0));
            if pointer.is_some_and(|p| hr.contains(p)) {
                painter.rect_filled(hr.shrink2(vec2(0.0, 4.0)), CornerRadius::same(5), Color32::from_white_alpha(8));
            }
            hint_rects.push((hr, k));
            x = r2.right() + 12.0;
        }
    }

    // panel kenarı
    painter.rect_stroke(panel, CornerRadius::ZERO, Stroke::new(1.0, Color32::from_rgba_premultiplied(20, 20, 20, 20)), StrokeKind::Inside);

    // katmanlar
    let fg = ctx.layer_painter(LayerId::new(Order::Foreground, Id::unique("bul-ov")));
    let mut menu_hit: Option<(Rect, Vec<(Rect, usize)>)> = None;
    if let Some(m) = &mut app.menu {
        let avail = menu_avail_h(m, &l);
        menu_fix_scroll(m, avail);
    }
    if let Some(m) = &app.menu {
        let mr = menu_rect(m, &l);
        // D kartı: seçili satırın yanında yüzen kart, solda vurgu şeridi, harfler küçük tuş kutularında
        let rad = 7;
        let sh = egui::epaint::Shadow { offset: [0, 4], blur: 14, spread: 0, color: Color32::from_black_alpha(90) };
        fg.add(sh.as_shape(mr, CornerRadius::same(rad)));
        fg.rect(mr, CornerRadius::same(rad), Color32::from_rgb(0x25, 0x28, 0x30), Stroke::new(1.0, Color32::from_rgba_premultiplied(34, 36, 42, 60)), StrokeKind::Inside);
        fg.rect_filled(Rect::from_min_size(pos2(mr.left() + 1.0, mr.top() + 8.0), vec2(2.0, mr.height() - 16.0)), CornerRadius::same(1), t::ACCENT.gamma_multiply(0.8));
        let pad = MENU_PAD / 2.0;
        let mut y = mr.top() + pad;
        let mut hits = Vec::new();
        let avail = menu_avail_h(m, &l);
        let total: f32 = m.entries.iter().map(menu_entry_h).sum();
        let (first, end) = if total <= avail + 0.5 { (0, m.entries.len()) } else { (m.scroll, menu_visible_end(m, m.scroll, avail)) };
        if first > 0 || end < m.entries.len() {
            // kaydırma göstergesi (kart içinde kayar)
            let track = Rect::from_min_max(pos2(mr.right() - 4.5, mr.top() + 7.0), pos2(mr.right() - 2.0, mr.bottom() - 7.0));
            let frac = (end - first) as f32 / m.entries.len() as f32;
            let h = (track.height() * frac).max(10.0);
            let top = track.top() + (track.height() - h) * (first as f32 / (m.entries.len() - (end - first)).max(1) as f32);
            fg.rect_filled(Rect::from_min_size(pos2(track.left(), top), vec2(2.5, h)), CornerRadius::same(2), Color32::from_rgba_premultiplied(40, 40, 40, 40));
        }
        let ih = MENU_ITEM_H_D;
        let inset = 6.0;
        for (k, e) in m.entries.iter().enumerate().take(end).skip(first) {
            match e {
                MEntry::Sep => {
                    fg.line_segment([pos2(mr.left() + 12.0, y + MENU_SEP_H / 2.0), pos2(mr.right() - 12.0, y + MENU_SEP_H / 2.0)], Stroke::new(1.0, t::line().gamma_multiply(0.7)));
                    y += MENU_SEP_H;
                }
                MEntry::Item(it) => {
                    let ir = Rect::from_min_size(pos2(mr.left() + inset, y), vec2(mr.width() - inset * 2.0, ih));
                    let sel = k == m.sel;
                    let cy = ir.center().y;
                    if sel {
                        fg.rect_filled(ir, CornerRadius::same(5), Color32::from_white_alpha(14));
                    }
                    let kb = Rect::from_center_size(pos2(ir.left() + 13.0, cy), vec2(16.0, 15.0));
                    fg.rect(kb, CornerRadius::same(4), if sel { t::ACCENT } else { Color32::from_rgb(0x30, 0x34, 0x3c) }, Stroke::NONE, StrokeKind::Inside);
                    let kcol = if sel { Color32::WHITE } else { t::MENU_KEY };
                    if it.key == ' ' {
                        // boşluk tuşu simgesi ⎵
                        let y0 = kb.center().y + 2.0;
                        let st = Stroke::new(1.3, kcol);
                        fg.line_segment([pos2(kb.left() + 4.0, y0 - 3.0), pos2(kb.left() + 4.0, y0)], st);
                        fg.line_segment([pos2(kb.left() + 4.0, y0), pos2(kb.right() - 4.0, y0)], st);
                        fg.line_segment([pos2(kb.right() - 4.0, y0), pos2(kb.right() - 4.0, y0 - 3.0)], st);
                    } else {
                        fg.text(kb.center(), Align2::CENTER_CENTER, it.key.to_string(), sb(10.0), kcol);
                    }
                    let short_w = if it.short.is_empty() { 0.0 } else { text_w(&fg, it.short, reg(9.5)) + 10.0 };
                    let g = fg.layout_job(job(it.label, reg(11.5), if sel { Color32::WHITE } else { t::FG.gamma_multiply(0.9) }, ir.width() - 30.0 - short_w));
                    fg.galley(pos2(ir.left() + 28.0, cy - g.size().y / 2.0), g, t::FG);
                    if !it.short.is_empty() {
                        fg.text(pos2(ir.right() - 7.0, cy), Align2::RIGHT_CENTER, it.short, reg(9.5), t::TEXT3.gamma_multiply(0.7));
                    }
                    hits.push((ir, k));
                    y += ih;
                }
            }
        }
        menu_hit = Some((mr, hits));
    }
    let mut confirm_hit: Option<(Rect, Vec<(Rect, usize)>)> = None;
    if let Some(c) = &app.confirm {
        let area = Rect::from_min_max(pos2(panel.left(), panel.top() + SEARCH_H + 1.0), panel.max);
        fg.rect_filled(area, CornerRadius::ZERO, Color32::from_black_alpha(110));
        let w = 440.0;
        let line_jobs: Vec<_> = c
            .lines
            .iter()
            .map(|s| {
                let mut j = LayoutJob::simple(s.clone(), reg(12.5), t::TEXT2, w - 40.0);
                j.wrap.break_anywhere = true;
                fg.layout_job(j)
            })
            .collect();
        let h = 18.0 + 22.0 + line_jobs.iter().map(|g| g.size().y + 3.0).sum::<f32>() + 16.0 + 32.0 + 16.0;
        let cr = Rect::from_center_size(area.center(), vec2(w, h));
        let sh = egui::epaint::Shadow { offset: [0, 10], blur: 26, spread: 0, color: Color32::from_black_alpha(140) };
        fg.add(sh.as_shape(cr, CornerRadius::same(12)));
        fg.rect(cr, CornerRadius::same(12), t::MENU_BG, Stroke::new(1.0, Color32::from_rgba_premultiplied(26, 26, 26, 26)), StrokeKind::Inside);
        let mut y = cr.top() + 18.0;
        fg.text(pos2(cr.left() + 20.0, y), Align2::LEFT_TOP, &c.title, sb(14.5), t::FG);
        y += 26.0;
        for g in line_jobs {
            let hgt = g.size().y;
            fg.galley(pos2(cr.left() + 20.0, y), g, t::TEXT2);
            y += hgt + 3.0;
        }
        let mut bx = cr.right() - 16.0;
        let by = cr.bottom() - 16.0 - 30.0;
        let mut hits = Vec::new();
        for (k, b) in c.buttons.iter().enumerate().rev() {
            let tw = text_w(&fg, b, sb(12.5));
            let br = Rect::from_min_size(pos2(bx - tw - 28.0, by), vec2(tw + 28.0, 30.0));
            let sel = k == c.sel;
            fg.rect(br, CornerRadius::same(8), if sel { t::ACCENT } else { Color32::from_rgb(0x2c, 0x30, 0x38) }, Stroke::NONE, StrokeKind::Inside);
            fg.text(br.center(), Align2::CENTER_CENTER, *b, sb(12.5), if sel { Color32::from_rgb(0x10, 0x16, 0x1e) } else { t::FG });
            hits.push((br, k));
            bx = br.left() - 8.0;
        }
        confirm_hit = Some((cr, hits));
    }

    // ---------------- Fare ----------------
    let (clicked, dbl, sec, alt, wheel) = ctx.input(|i| {
        let mut wheel = 0.0f32;
        for e in &i.events {
            if let Event::MouseWheel { unit, delta, .. } = e {
                wheel += match unit {
                    egui::MouseWheelUnit::Point => delta.y / 50.0,
                    egui::MouseWheelUnit::Line => delta.y,
                    egui::MouseWheelUnit::Page => delta.y * 5.0,
                };
            }
        }
        (i.pointer.primary_clicked(), i.pointer.button_double_clicked(PointerButton::Primary), i.pointer.secondary_clicked(), i.modifiers.alt, wheel)
    });
    let (mod_ctrl, mod_shift) = ctx.input(|i| (i.modifiers.command, i.modifiers.shift));
    let pos = ctx.input(|i| i.pointer.interact_pos());
    let mut pointing = false;

    if let Some((cr, hits)) = confirm_hit {
        if let Some(p) = pointer {
            for (r, k) in &hits {
                if r.contains(p) {
                    pointing = true;
                    if pointer_moved && let Some(c) = &mut app.confirm {
                        c.sel = *k;
                    }
                }
            }
        }
        if clicked && let Some(p) = pos {
            if let Some((_, k)) = hits.iter().find(|(r, _)| r.contains(p)) {
                app.confirm_answer(Some(*k));
            } else if let Some((_, k)) = hint_rects.iter().find(|(r, _)| r.contains(p)) {
                hint_action(app, k, &ctx, &l);
            } else if !cr.contains(p) && !panel.contains(p) {
                app.confirm_answer(None);
            }
        }
    } else if let Some((mr, hits)) = menu_hit {
        // tekerlek: menü pencereye sığmıyorsa kaydırır; seçim görünür alanda kalır
        if wheel != 0.0 && pointer.is_some_and(|p| mr.contains(p)) && let Some(m) = &mut app.menu {
            let avail = menu_avail_h(m, &l);
            let max = menu_max_scroll(m, avail);
            app.wheel_acc += wheel;
            while app.wheel_acc >= 1.0 {
                m.scroll = m.scroll.saturating_sub(1);
                app.wheel_acc -= 1.0;
            }
            while app.wheel_acc <= -1.0 {
                m.scroll = (m.scroll + 1).min(max);
                app.wheel_acc += 1.0;
            }
            let end = menu_visible_end(m, m.scroll, avail);
            if m.sel < m.scroll || m.sel >= end {
                let inside: Vec<usize> = (m.scroll..end).filter(|&k| matches!(m.entries[k], MEntry::Item(_))).collect();
                if let Some(&k) = if m.sel < m.scroll { inside.first() } else { inside.last() } {
                    m.sel = k;
                }
            }
        }
        if let Some(p) = pointer {
            for (r, k) in &hits {
                if r.contains(p) {
                    pointing = true;
                    if pointer_moved && let Some(m) = &mut app.menu {
                        m.sel = *k;
                    }
                }
            }
        }
        if (clicked || sec) && let Some(p) = pos {
            if let Some((_, k)) = hits.iter().find(|(r, _)| r.contains(p)) {
                let a = app.menu.as_ref().and_then(|m| m.item(*k)).map(|i| i.act);
                if let Some(a) = a {
                    app.run(a, &ctx);
                }
            } else if let Some((_, k)) = hint_rects.iter().find(|(r, _)| r.contains(p)).filter(|_| clicked) {
                // alttaki ipuçları menü açıkken de tıklanabilir (Enter Seç, Shift Yol, Esc Kapat)
                hint_action(app, k, &ctx, &l);
            } else if !mr.contains(p) {
                app.menu = None;
            }
        }
        if pointer.is_some_and(|p| hint_rects.iter().any(|(r, _)| r.contains(p))) {
            pointing = true;
        }
    } else {
        // tekerlek
        if wheel != 0.0 && l.rows > 0 && pointer.is_some_and(|p| l.list_rect.contains(p)) {
            app.wheel_acc += wheel;
            while app.wheel_acc >= 1.0 {
                app.scroll_by(-1);
                app.wheel_acc -= 1.0;
            }
            while app.wheel_acc <= -1.0 {
                app.scroll_by(1);
                app.wheel_acc += 1.0;
            }
        } else if wheel != 0.0 && let (Some(pr), Some(p)) = (l.preview, pointer)
            && pr.contains(p)
        {
            app.preview_scroll = (app.preview_scroll - wheel * 3.0).max(0.0);
        }
        if let Some(p) = pointer
            && (crumbs.iter().any(|(r, _)| r.contains(p))
                || hint_rects.iter().any(|(r, _)| r.contains(p))
                || strip_click.is_some_and(|r| r.contains(p))
                || preview_img_rect.is_some_and(|r| r.contains(p))
                || gear_rect.contains(p)
                || clip_x.is_some_and(|r| r.contains(p))
                || hk_rect.contains(p)
                || ext_chip_x.is_some_and(|r| r.contains(p))
                || mode_chip.is_some_and(|r| r.contains(p))
                || settings_hits.iter().any(|(r, _)| r.contains(p)))
        {
            pointing = true;
        }
        if let Some(p) = pos {
            let row_hit = app.hover_row.filter(|_| l.list_rect.contains(p));
            if clicked {
                if clip_x.is_some_and(|r| r.contains(p)) {
                    app.clear_clip();
                } else if gear_rect.contains(p) || hk_rect.contains(p) {
                    if app.mode == Mode::Settings {
                        app.escape(&ctx);
                    } else {
                        app.open_settings();
                    }
                } else if ext_chip_x.is_some_and(|r| r.contains(p)) {
                    app.run(Act::ExtFilter, &ctx);
                } else if mode_chip.is_some_and(|r| r.contains(p)) {
                    app.run(Act::CycleMode, &ctx);
                } else if let Some((_, k)) = settings_hits.iter().find(|(r, _)| r.contains(p)) {
                    app.settings_activate(*k);
                } else if let Some((_, path)) = crumbs.iter().find(|(r, _)| r.contains(p)) {
                    app.crumb_sel = None;
                    match path {
                        Some(pth) => app.enter_browse(&pth.clone()),
                        None => app.leave_browse(),
                    }
                } else if let Some((_, k)) = hint_rects.iter().find(|(r, _)| r.contains(p)) {
                    hint_action(app, k, &ctx, &l);
                } else if strip_click.is_some_and(|r| r.contains(p)) {
                    app.run(Act::AddPath, &ctx);
                } else if preview_img_rect.is_some_and(|r| r.contains(p)) {
                    if dbl {
                        app.run(Act::Open, &ctx);
                    }
                } else if let Some(i) = row_hit {
                    if app.edit.as_ref().is_some_and(|e| e.row != i) {
                        app.cancel_edit();
                    }
                    if app.edit.is_none() && i < app.items.len() {
                        app.crumb_sel = None;
                        let dbl_open = dbl && !mod_ctrl && !mod_shift && !alt;
                        if mod_ctrl && mod_shift && !alt {
                            app.select_range_add(i);
                        } else if mod_ctrl && !alt {
                            app.toggle_mark(i);
                        } else if mod_shift && !alt {
                            app.select_range(i);
                        } else if !(dbl_open && app.multi) {
                            // Çoklu seçimde çift tık işareti bozmadan açar
                            app.select_single(i);
                        }
                        app.request_preview();
                        if alt {
                            app.activate(i, true, &ctx);
                        } else if dbl_open {
                            app.activate(i, false, &ctx);
                        }
                    }
                } else if !panel.contains(p) {
                    crate::platform::hide(&ctx);
                }
            }
            if sec && mode_chip.is_some_and(|r| r.contains(p)) {
                app.run(Act::CycleModeBack, &ctx);
            }
            if sec && let Some(i) = row_hit {
                app.cancel_edit();
                if !app.is_marked(i) && !app.multi {
                    app.select_single(i);
                }
                app.sel = i;
                app.open_menu(true, pos2(p.x + 2.0, p.y + 2.0));
            }
        }
    }
    if pointing {
        ctx.set_cursor_icon(CursorIcon::PointingHand);
    }
}

fn paint_row_edit(app: &mut App, ui: &mut Ui, painter: &Painter, r: Rect) {
    let Some(e) = app.edit.as_mut() else { return };
    painter.rect(r, CornerRadius::same(6), Color32::from_rgb(0x1f, 0x22, 0x28), Stroke::new(1.0, t::ACCENT), StrokeKind::Inside);
    let id = Id::unique("bul-edit");
    if let Some((a, b)) = e.select.take() {
        let mut st = egui::text_edit::TextEditState::load(ui.ctx(), id).unwrap_or_default();
        st.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(a), egui::text::CCursor::new(b))));
        st.store(ui.ctx(), id);
        ui.ctx().memory_mut(|m| m.request_focus(id));
    }
    let inner = r.shrink2(vec2(6.0, 2.0));
    let resp = ui.put(
        inner,
        TextEdit::singleline(&mut e.text).id(id).frame(egui::Frame::NONE).margin(egui::Margin::ZERO).font(sb(14.0)).text_color(t::FG).desired_width(inner.width()).align(Align2::LEFT_CENTER),
    );
    if !resp.has_focus() && ui.ctx().input(|i| i.viewport().focused) != Some(false) {
        resp.request_focus();
    }
}

fn footer_hints(app: &App) -> Vec<(&'static str, &'static str)> {
    if app.confirm.is_some() {
        return vec![(t!("Sol/Sağ", "Left/Right"), t!("Seç", "Select")), ("Enter", t!("Onayla", "Confirm")), ("Esc", t!("Vazgeç", "Cancel"))];
    }
    if app.menu.as_ref().is_some_and(|m| !m.context) {
        return vec![("Enter", t!("Seç", "Select")), ("Shift", t!("Yol", "Path")), ("Esc", t!("Kapat", "Close"))];
    }
    if app.menu.is_some() {
        return vec![("Enter", t!("Seç", "Select")), ("Esc", t!("Kapat", "Close"))];
    }
    if app.edit.is_some() {
        return vec![("Enter", t!("Uygula", "Apply")), ("Esc", t!("İptal", "Cancel"))];
    }
    if app.crumb_sel.is_some() {
        return vec![(t!("Sol/Sağ", "Left/Right"), t!("Bölüm", "Part")), ("Enter", t!("Git", "Go")), ("Esc", t!("Çık", "Exit"))];
    }
    match app.mode {
        Mode::Note { .. } => vec![("Enter", t!("Kaydet", "Save")), ("Shift+Enter", t!("Yeni satır", "New line")), ("Esc", t!("Vazgeç", "Cancel"))],
        Mode::Settings if app.settings_edit.is_some() => vec![("Enter", t!("Kaydet", "Save")), ("Esc", t!("Vazgeç", "Cancel"))],
        Mode::Dests => vec![("Enter", t!("Gir", "Open")), ("Esc", t!("Geri", "Back"))],
        Mode::Bin => vec![("Enter", t!("Aç", "Open")), ("F2", t!("Ad değiştir", "Rename")), ("Del", t!("Kaldır", "Remove")), ("Esc", t!("Geri", "Back"))],
        Mode::Settings if app.recording => vec![(t!("Kombinasyon", "Combination"), t!("Bas", "Press")), ("Esc", t!("İptal", "Cancel"))],
        Mode::Settings => vec![("Enter", t!("Değiştir", "Change")), ("Ctrl+S", t!("Kapat", "Close")), ("Esc", t!("Geri", "Back"))],
        Mode::Trash if app.items.is_empty() => vec![("Tab", t!("İşlem", "Actions")), ("Esc", t!("Geri", "Back"))],
        Mode::Trash => vec![("Enter", t!("Aç", "Open")), ("Shift+Space", t!("Önizle", "Preview")), ("Tab", t!("İşlem", "Actions")), ("Alt+Enter", t!("Özgün klasör", "Original folder")), ("Esc", t!("Geri", "Back"))],
        Mode::ExtInput { .. } => vec![("Enter", t!("Süz", "Filter")), ("Esc", t!("İptal", "Cancel"))],
        Mode::Search => {
            if app.multi {
                return vec![(t!("Boşluk", "Space"), t!("İşaretle", "Mark")), ("Ctrl+A", t!("Tümü", "All")), ("Enter", t!("Aç", "Open")), ("Tab", t!("İşlem", "Actions")), ("Esc", t!("Çık", "Exit"))];
            }
            if !app.marked.is_empty() {
                return vec![("Ctrl+C", t!("Kopyala", "Copy")), ("Ctrl+X", t!("Kes", "Cut")), ("Del", t!("Sil", "Delete")), ("Tab", t!("İşlem", "Actions")), ("Esc", t!("Seçimi bırak", "Clear selection"))];
            }
            // sığmayan sondan düşer (Gezgin en sonda: sağ tık menüsünde de var)
            if app.is_cmd() {
                return vec![("Enter", t!("Terminalde çalıştır", "Run in terminal")), ("Esc", t!("Çık", "Exit"))];
            }
            vec![("Enter", t!("Aç", "Open")), ("Shift+Space", t!("Önizle", "Preview")), ("Tab", t!("İşlem", "Actions")), ("Ctrl+1/2/3", t!("Sırala", "Sort")), ("Ctrl+N", t!("Not", "Note")), ("Ctrl+T", "Terminal"), ("Alt+Enter", t!("Gezgin", "Explorer"))]
        }
    }
}

fn hint_action(app: &mut App, k: &str, ctx: &egui::Context, l: &Layout) {
    match k {
        "Enter" => match (&app.confirm, &app.menu, &app.edit) {
            (Some(c), _, _) => {
                let s = c.sel;
                app.confirm_answer(Some(s));
            }
            (_, Some(m), _) => {
                if let Some(a) = m.item(m.sel).map(|i| i.act) {
                    app.run(a, ctx);
                }
            }
            (_, _, Some(_)) => app.commit_edit(),
            _ if matches!(app.mode, Mode::Note { .. }) => app.save_note(),
            _ if app.mode == Mode::Settings && app.settings_edit.is_some() => app.commit_terminal_path(),
            _ if app.mode == Mode::Settings => {
                let k = app.settings_sel;
                app.settings_activate(k);
            }
            _ if matches!(app.mode, Mode::ExtInput { .. }) => app.commit_ext_input(true),
            _ if app.is_cmd() => app.run_command(),
            _ if app.multi && app.marked.len() > 1 => app.run(Act::OpenAll, ctx),
            _ => app.run(Act::Open, ctx),
        },
        "Boşluk" | "Space" => {
            let i = app.sel;
            app.toggle_mark(i);
        }
        "Shift+Space" => app.run(Act::Preview, ctx),
        "Tab" => {
            let vi = app.sel.saturating_sub(app.scroll);
            let p = pos2(l.panel.left() + 330.0, l.list_top + LIST_PAD_T + vi as f32 * ROW_H + 30.0);
            app.open_menu(false, p);
        }
        "Alt+Enter" => app.run(Act::Reveal, ctx),
        "Ctrl+Sol" | "Ctrl+Left" => app.browse_up_pub(),
        "Ctrl+N" => app.run(Act::Note, ctx),
        "Ctrl+T" => app.run(Act::Terminal, ctx),
        "Ctrl+1/2/3" => app.cycle_sort(),
        "F1" => app.run(Act::CycleMode, ctx),
        "Ctrl+A" if app.mode == Mode::Search => app.run(Act::SelectAll, ctx),
        "Ctrl+S" => {
            if app.mode == Mode::Settings {
                app.escape(ctx);
            } else {
                app.open_settings();
            }
        }
        "Shift" => {
            // Tab menüsü açıkken "Shift Yol": yol şeridinde bölüm seçimi (yalnızca klasörde)
            app.menu = None;
            if let Some(b) = app.browse.clone() {
                app.crumb_sel = Some(crate::app::crumb_segments(&b).len() - 1);
            }
        }
        "Shift+Enter" => app.note_text.push('\n'),
        "Sol/Sağ" | "Left/Right" => {
            if let Some(c) = &mut app.confirm {
                c.sel = (c.sel + 1) % c.buttons.len().max(1);
            }
        }
        "F2" => app.run(Act::BinRename, ctx),
        "Del" if app.mode == Mode::Bin => app.run(Act::BinRemove, ctx),
        "Del" => app.run(Act::Delete, ctx),
        "Ctrl+C" => app.run(Act::Copy, ctx),
        "Ctrl+X" => app.run(Act::Cut, ctx),
        "Esc" => app.escape(ctx),
        _ => {}
    }
}

fn paint_preview(app: &mut App, ui: &Ui, painter: &Painter, pr: Rect) -> Option<Rect> {
    painter.rect_filled(pr, CornerRadius::ZERO, Color32::from_black_alpha(56));
    painter.line_segment([pos2(pr.left() + 0.5, pr.top()), pos2(pr.left() + 0.5, pr.bottom())], Stroke::new(1.0, t::line()));
    let inner = pr.shrink2(vec2(16.0, 0.0));
    let info_h = 62.0;
    let shot = Rect::from_min_max(pos2(inner.left(), pr.top() + 16.0), pos2(inner.right(), pr.bottom() - 14.0 - info_h));
    painter.rect_filled(shot, CornerRadius::same(10), Color32::from_rgb(0x0e, 0x10, 0x14));
    let it = app.preview_item.clone();
    let ready = app.preview.as_ref().is_some_and(|p| Some(p.key) == app.preview.as_ref().map(|x| x.key));
    let mut img_rect = None;
    if let (Some(p), true) = (&app.preview, ready) {
        if let Some(tex) = &app.preview_tex {
            let ts = tex.size_vec2();
            let scale = (shot.width() / ts.x).min(shot.height() / ts.y).min(4.0);
            let r = Rect::from_center_size(shot.center(), ts * scale);
            egui::Image::from_texture(egui::load::SizedTexture::from_handle(tex)).corner_radius(if (r.width() - shot.width()).abs() < 1.0 { 10 } else { 4 }).paint_at(ui, r);
            img_rect = Some(r);
        } else {
            let clip = painter.with_clip_rect(shot.shrink(1.0));
            let mono = it.as_ref().is_some_and(|i| util::is_textual(&i.name) && !i.is_dir);
            let has_icons = p.lines.iter().any(|l| l.icon.is_some());
            let lh = if mono { 15.0 } else if has_icons { 20.0 } else { 18.0 };
            // yalnızca tamamen sığan satırlar çizilir: son satır kenarda yarım kalmaz
            let top = shot.top() + 10.0;
            let rows_fit = (((shot.bottom() - 8.0) - top) / lh).floor().max(1.0) as usize;
            let overflow = p.lines.len() > rows_fit;
            let max_scroll = p.lines.len().saturating_sub(rows_fit) as f32;
            app.preview_scroll = app.preview_scroll.min(max_scroll);
            let start = app.preview_scroll as usize;
            let text_w = shot.width() - 20.0 - if overflow { 8.0 } else { 0.0 };
            let hover = ui.ctx().input(|i| i.pointer.hover_pos()).filter(|h| shot.contains(*h));
            let mut tip: Option<(Pos2, String)> = None;
            for (k, line) in p.lines.iter().skip(start).take(rows_fit).enumerate() {
                let y = top + k as f32 * lh;
                let row = Rect::from_min_size(pos2(shot.left() + 4.0, y - 1.0), vec2(shot.width() - 8.0, lh));
                if line.icon.is_some() && hover.is_some_and(|h| row.contains(h)) {
                    clip.rect_filled(row, CornerRadius::same(4), Color32::from_rgba_premultiplied(8, 9, 10, 10));
                }
                let truncated = paint_rich_line(&clip, line, pos2(shot.left() + 10.0, y), text_w, mono, lh);
                if truncated && let Some(h) = hover.filter(|h| row.contains(*h)) {
                    tip = Some((h, line.spans.iter().map(|s| s.text.as_str()).collect()));
                }
            }
            // kaydırma göstergesi
            if overflow {
                let track = Rect::from_min_max(pos2(shot.right() - 7.0, shot.top() + 8.0), pos2(shot.right() - 4.0, shot.bottom() - 8.0));
                let frac = rows_fit as f32 / p.lines.len() as f32;
                let h = (track.height() * frac).max(16.0);
                let y0 = track.top() + (track.height() - h) * (app.preview_scroll / max_scroll.max(1.0));
                painter.rect_filled(Rect::from_min_size(pos2(track.left(), y0), vec2(3.0, h)), CornerRadius::same(2), Color32::from_rgba_premultiplied(40, 40, 40, 40));
            }
            // uzun ad: üzerine gelince tam ad
            if let Some((h, full)) = tip {
                let fg = ui.ctx().layer_painter(LayerId::new(Order::Tooltip, Id::unique("bul-prev-tip")));
                let mut j = LayoutJob::simple(full, reg(12.0), t::FG, 300.0);
                j.wrap.break_anywhere = true;
                let g = fg.layout_job(j);
                let content = ui.ctx().content_rect();
                let size = g.size() + vec2(16.0, 10.0);
                let mut r = Rect::from_min_size(pos2(h.x + 12.0, h.y + 16.0), size);
                if r.right() > content.right() - 4.0 {
                    r = r.translate(vec2(content.right() - 4.0 - r.right(), 0.0));
                }
                if r.bottom() > content.bottom() - 4.0 {
                    r = r.translate(vec2(0.0, h.y - 8.0 - size.y - r.top()));
                }
                fg.rect(r, CornerRadius::same(6), t::MENU_BG, Stroke::new(1.0, Color32::from_rgba_premultiplied(40, 40, 40, 40)), StrokeKind::Inside);
                fg.galley(r.min + vec2(8.0, 5.0), g, t::FG);
            }
        }
    }
    // bilgi
    let mut y = shot.bottom() + 12.0;
    if let Some(it) = &it {
        let g = painter.layout_job(job(&it.name, sb(13.0), t::FG, inner.width()));
        painter.galley(pos2(inner.left(), y), g, t::FG);
        y += 19.0;
        let mut parts: Vec<String> = Vec::new();
        if let Some(p) = app.preview.as_ref().filter(|_| ready) {
            if let Some(im) = &p.image {
                parts.push(format!("{} × {}", im.orig.0, im.orig.1));
                parts.push(util::fmt_size(it.size));
            } else if !p.info.is_empty() {
                parts.push(p.info.replace("  ·  ", " · "));
            }
        }
        parts.push(fmt_day(it.mtime));
        let sub = parts.into_iter().filter(|s| !s.trim().is_empty()).collect::<Vec<_>>().join(" · ");
        let g = painter.layout_job(job(&sub, reg(11.5), t::TEXT2, inner.width()));
        painter.galley(pos2(inner.left(), y), g, t::TEXT2);
        y += 21.0;
        let mut x = inner.left();
        for (k, lab) in [("Shift+Space", t!("kapat ·", "close ·")), ("Enter", if it.is_dir { t!("gir", "open") } else { t!("aç", "open") })] {
            let r = painter.text(pos2(x, y), Align2::LEFT_TOP, k, sb(11.0), t::KEY);
            let r2 = painter.text(pos2(r.right() + 4.0, y), Align2::LEFT_TOP, lab, reg(11.0), t::TEXT3);
            x = r2.right() + 5.0;
        }
    }
    img_rect
}

/// Önizleme satırı. Döner: ad kısaltıldı mı (üzerine gelince tam ad gösterilir).
fn paint_rich_line(p: &Painter, line: &Line, pos: Pos2, max_w: f32, mono: bool, lh: f32) -> bool {
    let size = if mono { 11.5 } else { 12.0 };
    let font = if mono { FontId::monospace(size) } else { reg(size) };
    if let Some(icon) = line.icon {
        // klasör/dosya girdisi: çizilmiş simge + ad (… ile) + sağa yaslı bilgi
        let x0 = pos.x + line.indent as f32 * 12.0;
        let cy = pos.y + lh / 2.0 - 1.0;
        let name: String = line.spans.iter().map(|s| s.text.as_str()).collect();
        match icon {
            LineIcon::Folder => paint_folder(p, pos2(x0 + 8.0, cy), 18.0),
            LineIcon::File => paint_file_small(p, pos2(x0 + 8.0, cy), &name),
        }
        let mut right_w = 0.0;
        if let Some(r) = &line.right {
            let g = p.layout_no_wrap(r.clone(), reg(11.0), t::TEXT2);
            right_w = g.size().x + 10.0;
            p.galley(pos2(pos.x + max_w - g.size().x, cy - g.size().y / 2.0), g, t::TEXT2);
        }
        let nx = x0 + 22.0;
        let avail = (pos.x + max_w - right_w - nx).max(20.0);
        let st = line.spans.first().map(|s| s.style).unwrap_or_default();
        let f = if st.bold { sb(12.0) } else { reg(12.0) };
        let full_w = p.layout_no_wrap(name.clone(), f.clone(), t::FG).size().x;
        let g = p.layout_job(job(&name, f, st.fg.unwrap_or(t::FG), avail));
        p.galley(pos2(nx, cy - g.size().y / 2.0), g, t::FG);
        return full_w > avail + 0.5;
    }
    if let Some(bg) = line.style.bg {
        p.rect_filled(Rect::from_min_size(pos2(pos.x - 6.0, pos.y - 1.0), vec2(max_w + 12.0, size + 4.0)), CornerRadius::same(3), bg);
    }
    // "anahtar\t değer" satırları: değer sabit sütunda
    if line.spans.len() == 2 && line.spans[0].text.ends_with('\t') {
        let k = line.spans[0].text.trim_end_matches('\t');
        p.text(pos, Align2::LEFT_TOP, k, reg(12.0), t::TEXT2);
        let g = p.layout_job(job(&line.spans[1].text, reg(12.0), t::FG, max_w - 96.0));
        p.galley(pos2(pos.x + 96.0, pos.y), g, t::FG);
        return false;
    }
    let mut j = LayoutJob::default();
    for s in &line.spans {
        let f = if s.style.bold && !mono { sb(size) } else { font.clone() };
        j.append(&s.text, 0.0, TextFormat { font_id: f, color: s.style.fg.unwrap_or(t::FG), background: s.style.bg.unwrap_or(Color32::TRANSPARENT), ..Default::default() });
    }
    j.wrap.max_width = max_w;
    j.wrap.max_rows = 1;
    j.wrap.break_anywhere = true;
    j.wrap.overflow_character = Some('…');
    let g = p.layout_job(j);
    p.galley(pos, g, t::FG);
    false
}
