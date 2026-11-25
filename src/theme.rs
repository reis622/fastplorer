//! Renkler (maketlerdeki koyu başlatıcı teması) ve önizleme için küçük zengin metin modeli.
use eframe::egui::Color32;

pub type Color = Color32;

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color32::from_rgb(r, g, b)
}

pub const BG: Color = rgb(0x18, 0x1a, 0x1f);
pub const PANEL_TOP: Color = rgb(26, 28, 34);
pub const PANEL_BOTTOM: Color = rgb(20, 22, 27);
pub const FOOTER: Color = rgb(0x15, 0x17, 0x1c);
pub const FG: Color = rgb(0xee, 0xf0, 0xf3);
pub const TEXT2: Color = rgb(0x8b, 0x90, 0x98);
pub const TEXT3: Color = rgb(0x6a, 0x70, 0x78);
pub const DIM: Color = TEXT2;
pub const KEY: Color = rgb(0xae, 0xb3, 0xbb);
pub const META_SIZE: Color = rgb(0xb4, 0xb8, 0xc0);
pub const ACCENT: Color = rgb(0x7e, 0xb0, 0xe0);
pub const MENU_KEY: Color = rgb(0x9c, 0xc6, 0xea);
pub const MENU_BG: Color = rgb(0x22, 0x25, 0x2c);
pub const ERR: Color = rgb(0xe0, 0x6c, 0x75);
pub const OK: Color = rgb(0x8f, 0xc9, 0x8a);
pub const HIT_FG: Color = rgb(0xd6, 0xeb, 0xff);
pub const HIT_FG_SEL: Color = rgb(0xf3, 0xf8, 0xfd);
pub const MATCH_BG: Color = rgb(0x5a, 0x4c, 0x22);
pub const MATCH_FG: Color = rgb(0xff, 0xe6, 0xa8);
pub const FOCUS_LINE: Color = rgb(0x2a, 0x2c, 0x24);
pub const COMMENT: Color = rgb(0x6a, 0x72, 0x7e);
pub const STRING: Color = rgb(0x98, 0xc3, 0x79);
pub const NUMBER: Color = rgb(0xd1, 0x9a, 0x66);
pub const KEYWORD: Color = rgb(0xc6, 0x78, 0xdd);
pub const FUNC: Color = rgb(0x61, 0xaf, 0xef);
pub const TYPE: Color = rgb(0xe5, 0xc0, 0x7b);

pub const fn line() -> Color {
    Color32::from_rgba_premultiplied(18, 18, 18, 18)
}
pub const fn accent_dim() -> Color {
    // rgba(126,176,224,0.16) önceden çarpılmış
    Color32::from_rgba_premultiplied(20, 28, 36, 41)
}
pub const fn hit_bg(selected: bool) -> Color {
    if selected { Color32::from_rgba_premultiplied(45, 63, 81, 92) } else { Color32::from_rgba_premultiplied(28, 39, 49, 56) }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifier;
impl Modifier {
    pub const BOLD: Modifier = Modifier;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
}

impl Style {
    pub const fn new() -> Self {
        Style { fg: None, bg: None, bold: false }
    }
    pub const fn fg(mut self, c: Color) -> Self {
        self.fg = Some(c);
        self
    }
    pub const fn bg(mut self, c: Color) -> Self {
        self.bg = Some(c);
        self
    }
    pub const fn add_modifier(mut self, _m: Modifier) -> Self {
        self.bold = true;
        self
    }
}

#[derive(Clone, Debug, Default)]
pub struct Span {
    pub text: String,
    pub style: Style,
}

impl Span {
    pub fn styled(text: impl Into<String>, style: Style) -> Self {
        Span { text: text.into(), style }
    }
}

/// Önizleme satırı simgesi (klasör içeriği / arşiv ağacı): çizimle, yazı tipine bağlı değil.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineIcon {
    Folder,
    File,
}

#[derive(Clone, Debug, Default)]
pub struct Line {
    pub spans: Vec<Span>,
    pub style: Style,
    /// Varsa satır başında çizilen simge (ad uzunsa "…" ile kısaltılır, üzerine gelince tam ad).
    pub icon: Option<LineIcon>,
    /// Sağa yaslı ek bilgi (ör. dosya boyutu); ad kısaltılsa da her zaman görünür.
    pub right: Option<String>,
    /// Girinti düzeyi (arşiv ağacı).
    pub indent: u8,
}

impl Line {
    pub fn styled(text: impl Into<String>, style: Style) -> Self {
        Line { spans: vec![Span::styled(text, style)], ..Default::default() }
    }
    /// Klasör/dosya girdisi: simge + ad (+ sağda bilgi).
    pub fn entry(icon: LineIcon, name: impl Into<String>, style: Style, right: Option<String>, indent: u8) -> Self {
        Line { spans: vec![Span::styled(name, style)], style: Style::new(), icon: Some(icon), right, indent }
    }
    pub fn raw(text: impl Into<String>) -> Self {
        Line::styled(text, Style::new())
    }
    pub fn style(mut self, s: Style) -> Self {
        self.style = s;
        self
    }
}

impl From<Vec<Span>> for Line {
    fn from(spans: Vec<Span>) -> Self {
        Line { spans, ..Default::default() }
    }
}
