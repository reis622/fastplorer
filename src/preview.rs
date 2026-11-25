//! Önizleme üretimi (arka plan iş parçacığında).

use crate::theme::{self as t, Line, LineIcon, Modifier, Span, Style};
use crate::util::{self, CAT_ARCHIVE, CAT_AUDIO, CAT_IMAGE, CAT_VIDEO};
use std::sync::mpsc::{Receiver, Sender};

pub struct PreviewReq {
    pub key: u64,
    pub path: String,
    pub is_dir: bool,
    pub line: Option<usize>,
    pub needle: Option<String>,
    pub w: u16,
    pub h: u16,
}

/// Önizleme resmi (RGBA, ölçeklenmiş) ve özgün boyutu.
pub struct PreviewImage {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
    pub orig: (u32, u32),
}

#[derive(Default)]
pub struct Preview {
    pub key: u64,
    pub info: String,
    pub lines: Vec<Line>,
    pub footer: String,
    pub scroll: usize,
    pub image: Option<PreviewImage>,
}

pub fn spawn(rx: Receiver<PreviewReq>, tx: Sender<Preview>) {
    std::thread::spawn(move || {
        while let Ok(mut r) = rx.recv() {
            while let Ok(n) = rx.try_recv() {
                r = n;
            }
            let p = build(&r);
            if tx.send(p).is_err() {
                return;
            }
        }
    });
}

fn meta_lines(path: &str, md: Option<&std::fs::Metadata>) -> Vec<Line> {
    let mut v = Vec::new();
    let kv = |k: &str, val: String| Line::from(vec![Span::styled(format!("{k}\t"), Style::new().fg(t::DIM)), Span::styled(val, Style::new().fg(t::FG))]);
    let name = std::path::Path::new(path).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    v.push(kv(t!("Tür", "Type"), util::type_label(&name, md.is_some_and(|m| m.is_dir()))));
    if let Some(m) = md {
        if !m.is_dir() {
            v.push(kv(t!("Boyut", "Size"), tf!("{} ({} bayt)", "{} ({} bytes)", util::fmt_size(m.len()), util::fmt_count(m.len() as usize))));
        }
        v.push(kv(t!("Değiştirilme", "Modified"), util::fmt_date(crate::index::mtime_of(m))));
        if let Some(c) = m.created().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()) {
            v.push(kv(t!("Oluşturulma", "Created"), util::fmt_date(c.as_secs() as i64)));
        }
        if m.permissions().readonly() {
            v.push(kv(t!("Öznitelik", "Attributes"), t!("Salt okunur", "Read-only").into()));
        }
    }
    if let Some(p) = std::path::Path::new(path).parent() {
        v.push(kv(t!("Konum", "Location"), p.to_string_lossy().to_string()));
    }
    v
}

pub fn build(r: &PreviewReq) -> Preview {
    let mut p = Preview { key: r.key, ..Default::default() };
    if r.path == crate::recycle::VIRTUAL_PATH {
        p.info = crate::recycle::title().into();
        p.lines.push(Line::raw(t!("Silinen dosyalarınız burada.", "Your deleted files are here.")));
        p.lines.push(Line::raw(t!("İçine girince gerçek adları, özgün klasörleri ve silinme tarihleriyle görünür.", "Open it to see their real names, original folders and deletion dates.")));
        p.footer = t!("Enter ya da çift tık ile içine gir", "Enter or double-click to open").into();
        return p;
    }
    let md = std::fs::metadata(&r.path).ok();
    let Some(md) = md else {
        p.info = t!("Bulunamadı", "Not found").into();
        p.lines.push(Line::styled(t!("Öğe artık mevcut değil.", "The item no longer exists."), Style::new().fg(t::ERR)));
        return p;
    };
    let name = std::path::Path::new(&r.path).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| r.path.clone());
    if md.is_dir() || r.is_dir {
        folder(&r.path, &mut p);
        p.footer = t!("Enter ile Gezgin'de aç  ·  Alt+Enter üst klasörde göster", "Enter opens in Explorer  ·  Alt+Enter shows in parent folder").into();
        return p;
    }
    let cat = util::category(&name, false);
    let size = util::fmt_size(md.len());
    if util::is_textual(&name) {
        text(&r.path, &name, r, &mut p);
        p.footer = t!("Enter ile varsayılan uygulamada aç", "Enter opens in the default app").into();
        return p;
    }
    match cat {
        CAT_IMAGE => {
            image_preview(&r.path, r.w, r.h, &mut p);
            p.info = format!("{}  ·  {}", util::ext_of(&name).to_uppercase(), size) + &p.info;
            p.footer = t!("Enter ile görüntüleyicide aç", "Enter opens in the viewer").into();
        }
        CAT_ARCHIVE if util::ext_of(&name) == "zip" || util::ext_of(&name) == "jar" => {
            zip_tree(&r.path, &mut p);
            p.info = format!("{}  ·  {}", p.info, size);
            p.footer = t!("Enter ile arşivi aç  ·  Alt+Enter klasörü aç", "Enter opens the archive  ·  Alt+Enter opens the folder").into();
        }
        _ => {
            let ext = util::ext_of(&name);
            if ext == "docx" && docx_text(&r.path, &mut p) {
                p.info = tf!("Word belgesi  ·  {size}", "Word document  ·  {size}");
            } else {
                p.info = format!("{}  ·  {}", util::type_label(&name, false), size);
                p.lines = meta_lines(&r.path, Some(&md));
            }
            p.footer = match cat {
                CAT_VIDEO => t!("Enter ile oynat (varsayılan oynatıcı)  ·  Alt+Enter klasörü aç", "Enter plays it (default player)  ·  Alt+Enter opens the folder").into(),
                CAT_AUDIO => t!("Enter ile çal (varsayılan oynatıcı)  ·  Alt+Enter klasörü aç", "Enter plays it (default player)  ·  Alt+Enter opens the folder").into(),
                _ => t!("Enter ile varsayılan uygulamada aç", "Enter opens in the default app").into(),
            };
            if matches!(cat, CAT_VIDEO | CAT_AUDIO) {
                p.lines.push(Line::raw(""));
                p.lines.push(Line::from(vec![
                    Span::styled(t!(" ▶ Oynat ", " ▶ Play "), Style::new().fg(t::BG).bg(t::ACCENT).add_modifier(Modifier::BOLD)),
                    Span::styled(t!("  Enter / çift tık", "  Enter / double-click"), Style::new().fg(t::DIM)),
                ]));
            }
        }
    }
    p
}

fn folder(path: &str, p: &mut Preview) {
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    if let Ok(rd) = std::fs::read_dir(path) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            let isd = e.file_type().map(|f| f.is_dir()).unwrap_or(false);
            if isd {
                dirs.push(n);
            } else {
                let sz = e.metadata().map(|m| m.len()).unwrap_or(0);
                files.push((n, sz));
            }
        }
    }
    dirs.sort_by_key(|a| util::fold(a));
    files.sort_by_key(|a| util::fold(&a.0));
    p.info = tf!("Klasör  ·  {} klasör  ·  {} dosya", "Folder  ·  {} folders  ·  {} files", dirs.len(), files.len());
    for d in dirs.iter().take(400) {
        p.lines.push(Line::entry(LineIcon::Folder, d.clone(), Style::new().fg(t::FG).add_modifier(Modifier::BOLD), None, 0));
    }
    for (f, s) in files.iter().take(400) {
        p.lines.push(Line::entry(LineIcon::File, f.clone(), Style::new().fg(t::FG), Some(util::fmt_size(*s)), 0));
    }
    let more = dirs.len().saturating_sub(400) + files.len().saturating_sub(400);
    if more > 0 {
        p.lines.push(Line::styled(tf!("… ve {} öğe daha", "… and {} more", util::fmt_count(more)), Style::new().fg(t::DIM)));
    }
    if dirs.is_empty() && files.is_empty() {
        p.lines.push(Line::styled(t!("(boş klasör)", "(empty folder)"), Style::new().fg(t::DIM)));
    }
}

fn zip_tree(path: &str, p: &mut Preview) {
    let Ok(f) = std::fs::File::open(path) else { return };
    let Ok(mut z) = zip::ZipArchive::new(f) else {
        p.info = t!("ZIP (okunamadı)", "ZIP (unreadable)").into();
        return;
    };
    let mut names: Vec<String> = (0..z.len()).filter_map(|i| z.by_index_raw(i).ok().and_then(|f| f.name().ok().map(|n| n.replace('\\', "/")))).collect();
    names.sort();
    let mut seen = std::collections::HashSet::new();
    let (mut nf, mut nd) = (0, 0);
    for n in &names {
        let parts: Vec<&str> = n.split('/').filter(|s| !s.is_empty()).collect();
        let is_dir = n.ends_with('/');
        for (d, part) in parts.iter().enumerate() {
            let last = d + 1 == parts.len();
            let key = parts[..=d].join("/");
            let indent = d.min(12) as u8;
            if last && !is_dir {
                nf += 1;
                if p.lines.len() < 600 {
                    p.lines.push(Line::entry(LineIcon::File, part.to_string(), Style::new().fg(t::FG), None, indent));
                }
            } else if seen.insert(key) {
                nd += 1;
                if p.lines.len() < 600 {
                    p.lines.push(Line::entry(LineIcon::Folder, part.to_string(), Style::new().fg(t::FG).add_modifier(Modifier::BOLD), None, indent));
                }
            }
        }
    }
    p.info = tf!("ZIP  ·  {nf} dosya  ·  {nd} klasör", "ZIP  ·  {nf} files  ·  {nd} folders");
}

fn docx_text(path: &str, p: &mut Preview) -> bool {
    use std::io::Read;
    let Ok(f) = std::fs::File::open(path) else { return false };
    let Ok(mut z) = zip::ZipArchive::new(f) else { return false };
    let Ok(mut doc) = z.by_name("word/document.xml") else { return false };
    let mut xml = String::new();
    if doc.by_ref().take(8 << 20).read_to_string(&mut xml).is_err() {
        return false;
    }
    for para in xml.split("</w:p>").take(400) {
        let mut out = String::new();
        let mut in_tag = false;
        for c in para.chars() {
            match c {
                '<' => in_tag = true,
                '>' => in_tag = false,
                _ if !in_tag => out.push(c),
                _ => {}
            }
        }
        let out = out.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'");
        p.lines.push(Line::styled(out, Style::new().fg(t::FG)));
    }
    true
}

fn image_preview(path: &str, w: u16, h: u16, p: &mut Preview) {
    let img = match image::ImageReader::open(path).and_then(|r| r.with_guessed_format()).map_err(|e| e.to_string()).and_then(|r| r.decode().map_err(|e| e.to_string())) {
        Ok(i) => i,
        Err(_) => {
            p.lines.push(Line::styled(t!("Bu görüntü biçimi önizlenemiyor.", "This image format cannot be previewed."), Style::new().fg(t::DIM)));
            return;
        }
    };
    p.info = format!("  ·  {} × {}", img.width(), img.height());
    let (w, h) = (w.max(16) as u32, h.max(16) as u32);
    let small = if img.width() > w || img.height() > h { img.resize(w, h, image::imageops::FilterType::Triangle) } else { img.clone() };
    let small = small.to_rgba8();
    p.image = Some(PreviewImage { w: small.width() as usize, h: small.height() as usize, rgba: small.into_raw(), orig: (img.width(), img.height()) });
}

const KEYWORDS: &[&str] = &[
    "fn", "let", "mut", "pub", "use", "mod", "struct", "enum", "impl", "trait", "match", "if", "else", "for", "while", "loop", "return",
    "break", "continue", "in", "as", "const", "static", "type", "where", "async", "await", "move", "function", "var", "export", "import",
    "from", "class", "extends", "new", "this", "self", "Self", "def", "elif", "try", "except", "catch", "finally", "throw", "throws",
    "with", "yield", "lambda", "package", "func", "go", "defer", "interface", "public", "private", "protected", "void", "int", "bool",
    "string", "true", "false", "null", "nil", "None", "True", "False", "undefined", "typeof", "switch", "case", "default", "do", "echo",
    "then", "fi", "end", "local", "select", "insert", "update", "delete", "where", "and", "or", "not", "is", "of", "namespace", "using",
];

fn comment_prefix(ext: &str) -> &'static str {
    match ext {
        "py" | "sh" | "bash" | "toml" | "yaml" | "yml" | "rb" | "pl" | "r" | "ps1" | "psm1" | "conf" | "cfg" | "gitignore" | "env" | "dockerfile" | "make" | "cmake" => "#",
        "sql" | "lua" | "hs" => "--",
        "ini" => ";",
        "bat" | "cmd" => "::",
        "md" | "txt" | "csv" | "log" | "rst" | "tex" | "json" => "\u{0}",
        _ => "//",
    }
}

/// Basit sözdizimi renklendirme: yorum, metin, sayı, anahtar kelime.
pub fn highlight(line: &str, ext: &str, in_block: &mut bool) -> Vec<(char, Style)> {
    let plain = matches!(ext, "md" | "txt" | "csv" | "log" | "rst" | "tex");
    let cs: Vec<char> = line.chars().collect();
    let mut out = Vec::with_capacity(cs.len());
    let base = Style::new().fg(t::FG);
    if plain {
        return cs.into_iter().map(|c| (c, base)).collect();
    }
    let cp: Vec<char> = comment_prefix(ext).chars().collect();
    let c_like = cp == ['/', '/'];
    let mut i = 0;
    while i < cs.len() {
        if *in_block {
            out.push((cs[i], Style::new().fg(t::COMMENT)));
            if cs[i] == '*' && cs.get(i + 1) == Some(&'/') {
                out.push(('/', Style::new().fg(t::COMMENT)));
                i += 2;
                *in_block = false;
                continue;
            }
            i += 1;
            continue;
        }
        let c = cs[i];
        if c_like && c == '/' && cs.get(i + 1) == Some(&'*') {
            *in_block = true;
            out.push((c, Style::new().fg(t::COMMENT)));
            i += 1;
            continue;
        }
        if !cp.is_empty() && cs[i..].starts_with(&cp) {
            out.extend(cs[i..].iter().map(|&c| (c, Style::new().fg(t::COMMENT))));
            break;
        }
        if c == '"' || c == '\'' || c == '`' {
            let q = c;
            out.push((c, Style::new().fg(t::STRING)));
            i += 1;
            while i < cs.len() {
                out.push((cs[i], Style::new().fg(t::STRING)));
                if cs[i] == '\\' && i + 1 < cs.len() {
                    out.push((cs[i + 1], Style::new().fg(t::STRING)));
                    i += 2;
                    continue;
                }
                i += 1;
                if cs[i - 1] == q {
                    break;
                }
            }
            continue;
        }
        if c.is_ascii_digit() && (i == 0 || !(cs[i - 1].is_alphanumeric() || cs[i - 1] == '_')) {
            while i < cs.len() && (cs[i].is_ascii_alphanumeric() || cs[i] == '.' || cs[i] == '_') {
                out.push((cs[i], Style::new().fg(t::NUMBER)));
                i += 1;
            }
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let s = i;
            while i < cs.len() && (cs[i].is_alphanumeric() || cs[i] == '_') {
                i += 1;
            }
            let w: String = cs[s..i].iter().collect();
            let st = if KEYWORDS.contains(&w.as_str()) {
                Style::new().fg(t::KEYWORD)
            } else if cs.get(i) == Some(&'(') {
                Style::new().fg(t::FUNC)
            } else if w.chars().next().is_some_and(|c| c.is_uppercase()) {
                Style::new().fg(t::TYPE)
            } else {
                base
            };
            out.extend(cs[s..i].iter().map(|&c| (c, st)));
            continue;
        }
        out.push((c, base));
        i += 1;
    }
    out
}

/// Karakter-stil dizisinde aranan metni sarı arka planla vurgula.
pub fn mark(chars: &mut [(char, Style)], needle_folded: &str) {
    let n: Vec<char> = needle_folded.chars().collect();
    if n.is_empty() || chars.len() < n.len() {
        return;
    }
    let folded: Vec<char> = chars.iter().map(|c| util::fold_char(c.0)).collect();
    let mut i = 0;
    while i + n.len() <= folded.len() {
        if folded[i..i + n.len()] == n[..] {
            for c in &mut chars[i..i + n.len()] {
                c.1 = Style::new().fg(t::MATCH_FG).bg(t::MATCH_BG);
            }
            i += n.len();
        } else {
            i += 1;
        }
    }
}

pub fn group(chars: Vec<(char, Style)>) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    let mut cur = String::new();
    let mut st: Option<Style> = None;
    for (c, s) in chars {
        if st != Some(s) && !cur.is_empty() {
            spans.push(Span::styled(std::mem::take(&mut cur), st.unwrap_or_default()));
        }
        st = Some(s);
        cur.push(c);
    }
    if !cur.is_empty() {
        spans.push(Span::styled(cur, st.unwrap_or_default()));
    }
    spans
}

fn text(path: &str, name: &str, r: &PreviewReq, p: &mut Preview) {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            p.lines.push(Line::styled(tf!("Okunamadı: {e}", "Could not read: {e}"), Style::new().fg(t::ERR)));
            return;
        }
    };
    let ext = util::ext_of(name);
    let text = String::from_utf8_lossy(&data[..data.len().min(4 << 20)]).replace('\t', "    ");
    let total = text.lines().count();
    p.info = tf!("{}  ·  {} satır  ·  {}", "{}  ·  {} lines  ·  {}", util::type_label(name, false), util::fmt_count(total), util::fmt_size(data.len() as u64));
    if data[..data.len().min(8192)].contains(&0) {
        p.lines.push(Line::styled(t!("İkili dosya, metin önizlemesi yok.", "Binary file, no text preview."), Style::new().fg(t::DIM)));
        return;
    }
    let focus = r.line.unwrap_or(0);
    let start = focus.saturating_sub(12).saturating_sub(1);
    let gutter = (start + 600).min(total.max(1)).to_string().len().max(3);
    let needle = r.needle.as_deref().map(util::fold);
    let mut in_block = false;
    for (n, line) in text.lines().enumerate() {
        let mut chars = highlight(line, &ext, &mut in_block);
        if n < start {
            continue;
        }
        if n > start + 600 {
            break;
        }
        if let Some(nd) = &needle {
            mark(&mut chars, nd);
        }
        let is_focus = r.line == Some(n + 1);
        let mut spans = vec![Span::styled(format!("{:>gutter$} ", n + 1), Style::new().fg(if is_focus { t::MATCH_BG } else { t::DIM }))];
        spans.extend(group(chars));
        let mut l = Line::from(spans);
        if is_focus {
            l = l.style(Style::new().bg(t::FOCUS_LINE));
            p.scroll = (n - start).saturating_sub(8);
        }
        p.lines.push(l);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hl() {
        let mut b = false;
        let v = highlight("export async function fetchUser(id) { // x", "ts", &mut b);
        assert_eq!(v.len(), 42);
        assert_eq!(v[0].1.fg, Some(t::KEYWORD));
        assert_eq!(v.last().unwrap().1.fg, Some(t::COMMENT));
        let mut v = highlight("const user = fetchUser()", "ts", &mut b);
        mark(&mut v, "fetchuser");
        assert_eq!(v[13].1.bg, Some(t::MATCH_BG));
        let _ = highlight("/* a", "rs", &mut b);
        assert!(b);
    }
}
