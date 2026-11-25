//! Ad araması, içerik araması, hedef klasör araması ve son kullanılanlar.
#![allow(dead_code)]

use crate::index::{Index, F_DEL, F_NOISY, F_TEXT, NONE};
use crate::util::{self, CAT_CODE, CAT_DOC};
use rayon::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, RwLock};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Name,
    Content,
    /// Kopyala/taşı hedefi: yalnızca klasörler.
    Dest,
}

pub struct Req {
    pub generation: u64,
    pub query: String,
    pub tab: u8,
    pub kind: Kind,
    /// Ad/İçerik boşken: (yol, zaman) son kullanılanlar. Hedef modunda: son hedefler.
    pub recent: Vec<(String, i64)>,
    /// Gözat modu: önce bu klasör, sonra alt klasörleri, sonra diğer yerler; sorgu boşsa klasör içeriği listelenir.
    pub scope: Option<String>,
    /// Uzantı süzgeci (katlanmış, noktasız: "pdf").
    pub ext: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Item {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub mtime: i64,
    pub cat: u8,
    /// Son kullanılma zamanı (boş aramada).
    pub when: Option<i64>,
    /// İçerik eşleşmesi: (satır no, satır).
    pub line: Option<(usize, String)>,
    /// İçerik araması eşleşme kademesi (0 = birebir ifade).
    pub rank: u8,
}

pub struct Res {
    pub generation: u64,
    pub items: Vec<Item>,
    pub total: usize,
    pub done: bool,
}

pub const MAX_RESULTS: usize = 1000;

#[inline]
fn tab_ok(cat: u8, tab: u8) -> bool {
    tab == 0 || cat == tab
}

/// Bir adın sorguyla eşleşme puanı (küçük = iyi). `terms` katlanmış.
pub fn score(lower: &str, terms: &[String]) -> Option<u8> {
    let first = terms.first()?;
    for t in &terms[1..] {
        if !lower.contains(t.as_str()) {
            return None;
        }
    }
    if lower.starts_with(first.as_str()) {
        return Some(0);
    }
    if let Some(p) = lower.find(first.as_str()) {
        let prev = lower[..p].chars().next_back();
        let boundary = prev.is_some_and(|c| !c.is_alphanumeric());
        // birden fazla geçişte sınırda olanı ara
        if boundary || lower.match_indices(first.as_str()).any(|(i, _)| lower[..i].chars().next_back().is_some_and(|c| !c.is_alphanumeric())) {
            return Some(1);
        }
        return Some(2);
    }
    if terms.len() == 1 && first.chars().count() >= 2 && fuzzy(lower, first) {
        return Some(3);
    }
    None
}

/// Alt dizi (sırayla, aralıklı) eşleşmesi.
pub fn fuzzy(hay: &str, needle: &str) -> bool {
    let mut it = hay.chars();
    'outer: for n in needle.chars() {
        for h in it.by_ref() {
            if h == n {
                continue 'outer;
            }
        }
        return false;
    }
    true
}

pub fn terms_of(q: &str) -> Vec<String> {
    q.split_whitespace().map(util::fold).collect()
}

fn item_from(ix: &Index, i: u32) -> Item {
    let e = ix.entries[i as usize];
    Item {
        path: ix.path(i),
        name: ix.name(i).to_string(),
        is_dir: e.is_dir(),
        size: e.size,
        mtime: e.mtime,
        cat: e.cat,
        when: None,
        line: None,
        rank: 0,
    }
}

pub fn item_from_path(p: &str, when: Option<i64>) -> Option<Item> {
    let md = std::fs::metadata(p).ok()?;
    let name = std::path::Path::new(p).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| p.to_string());
    let is_dir = md.is_dir();
    Some(Item {
        path: p.to_string(),
        cat: util::category(&name, is_dir),
        name,
        is_dir,
        size: if is_dir { 0 } else { md.len() },
        mtime: crate::index::mtime_of(&md),
        when,
        line: None,
        rank: 0,
    })
}

/// Ad araması: önce baştan eşleşen, sonra kelime sınırı, içerir, bulanık.
pub fn search_names(ix: &Index, q: &str, tab: u8, dirs_only: bool) -> (Vec<Item>, usize) {
    search_ranked(ix, q, &Opts { tab, dirs_only, ..Default::default() })
}

/// `i` öğesi `scope` klasörünün altında mı (ebeveyn zinciri)?
fn under(ix: &Index, i: usize, scope: u32) -> bool {
    let mut c = ix.entries[i].parent;
    let mut n = 0;
    while c != NONE && n < 512 {
        if c == scope {
            return true;
        }
        c = ix.entries[c as usize].parent;
        n += 1;
    }
    false
}

/// Ad araması; `scope` verilirse yalnızca o klasörün altındakiler.
pub fn search_names_in(ix: &Index, q: &str, tab: u8, dirs_only: bool, scope: Option<u32>) -> (Vec<Item>, usize) {
    search_ranked(ix, q, &Opts { tab, dirs_only, scope, ..Default::default() })
}

#[derive(Default, Clone)]
pub struct Opts {
    pub tab: u8,
    pub dirs_only: bool,
    /// Geçerli klasör (gözat). Sonuçlarda önce doğrudan içindekiler, sonra alt klasörleri.
    pub scope: Option<u32>,
    /// `scope` varken onun dışındaki sonuçlar da (en sonda) gelsin mi?
    pub outside: bool,
    /// Uzantı süzgeci (katlanmış, noktasız).
    pub ext: Option<String>,
}

/// Sorgu bir uzantı içeriyor mu? ("rapor.pdf", ".png" → Some("pdf"/"png"))
pub fn query_ext(q: &str) -> Option<String> {
    let last = q.split_whitespace().last()?;
    let (_, ext) = last.rsplit_once('.')?;
    let ext = util::fold(ext);
    (!ext.is_empty() && ext.chars().count() <= 8 && ext.chars().all(|c| c.is_alphanumeric())).then_some(ext)
}

/// Katlanmış ad, uzantı süzgecine uyuyor mu?
pub fn ext_ok(lower_name: &str, ext: Option<&str>) -> bool {
    match ext {
        None => true,
        Some(x) => lower_name.len() > x.len() + 1 && lower_name.ends_with(x) && lower_name.as_bytes()[lower_name.len() - x.len() - 1] == b'.',
    }
}

/// Sıralama anahtarı (küçük = önce).
/// 1) bulanık eşleşmeler en sona, 2) geçerli klasörün dışındakiler sona,
/// 3) klasörler dosyalardan önce — sorgu uzantı içeriyorsa dosyalar önce,
/// 4) geçerli klasörde doğrudan olanlar alt klasörlerdekilerden önce, 5) eşleşme kalitesi.
pub fn rank_key(score: u8, noisy: bool, is_dir: bool, ext_query: bool, prox: u8) -> [u8; 5] {
    let fuzzy = (score >= 3) as u8;
    let group = if ext_query { is_dir as u8 } else { (!is_dir) as u8 };
    let outside = (prox >= 2) as u8;
    [fuzzy, outside, group, prox.min(1), score * 2 + noisy as u8]
}

/// Ad araması (sıralama kuralları `rank_key`).
pub fn search_ranked(ix: &Index, q: &str, o: &Opts) -> (Vec<Item>, usize) {
    let terms = terms_of(q);
    if terms.is_empty() {
        return (Vec::new(), 0);
    }
    let ext_query = query_ext(q).is_some();
    const CHUNK: usize = 1 << 16;
    let n = ix.entries.len();
    type Part = (Vec<([u8; 5], u16, i64, u32)>, usize);
    let parts: Vec<Part> = (0..n.div_ceil(CHUNK))
        .into_par_iter()
        .map(|c| {
            let mut v = Vec::new();
            let mut total = 0usize;
            let end = ((c + 1) * CHUNK).min(n);
            for i in c * CHUNK..end {
                let e = &ix.entries[i];
                if e.flags & F_DEL != 0 {
                    continue;
                }
                if o.dirs_only {
                    if !e.is_dir() {
                        continue;
                    }
                } else if !tab_ok(e.cat, o.tab) {
                    continue;
                }
                let lower = &ix.lower[e.lower_off as usize..e.lower_off as usize + e.lower_len as usize];
                if o.ext.is_some() && (e.is_dir() || !ext_ok(lower, o.ext.as_deref())) {
                    continue;
                }
                if let Some(s) = score(lower, &terms) {
                    let prox = match o.scope {
                        None => 0,
                        Some(sc) if e.parent == sc => 0,
                        Some(sc) if under(ix, i, sc) => 1,
                        Some(_) if o.outside => 2,
                        Some(_) => continue,
                    };
                    total += 1;
                    v.push((rank_key(s, e.flags & F_NOISY != 0, e.is_dir(), ext_query, prox), e.lower_len, -e.mtime, i as u32));
                }
            }
            if v.len() > MAX_RESULTS {
                v.select_nth_unstable(MAX_RESULTS);
                v.truncate(MAX_RESULTS);
            }
            (v, total)
        })
        .collect();
    let total = parts.iter().map(|p| p.1).sum();
    let mut all: Vec<_> = parts.into_iter().flat_map(|p| p.0).collect();
    all.sort_unstable();
    all.truncate(MAX_RESULTS);
    (all.into_iter().map(|t| item_from(ix, t.3)).collect(), total)
}

/// Uzantı süzgeci + boş sorgu: o uzantıdaki tüm dizinlenmiş dosyalar.
/// `scope` verilirse yalnızca o klasörün altındakiler (önce doğrudan içindekiler); en yeniler önce.
pub fn list_by_ext(ix: &Index, ext: &str, tab: u8, scope: Option<u32>) -> (Vec<Item>, usize) {
    const CHUNK: usize = 1 << 16;
    let n = ix.entries.len();
    type Part = (Vec<(u8, u8, i64, u32)>, usize);
    let parts: Vec<Part> = (0..n.div_ceil(CHUNK))
        .into_par_iter()
        .map(|c| {
            let mut v = Vec::new();
            let mut total = 0usize;
            let end = ((c + 1) * CHUNK).min(n);
            for i in c * CHUNK..end {
                let e = &ix.entries[i];
                if e.flags & F_DEL != 0 || e.is_dir() || !tab_ok(e.cat, tab) {
                    continue;
                }
                let lower = &ix.lower[e.lower_off as usize..e.lower_off as usize + e.lower_len as usize];
                if !ext_ok(lower, Some(ext)) {
                    continue;
                }
                let prox = match scope {
                    None => 0,
                    Some(sc) if e.parent == sc => 0,
                    Some(sc) if under(ix, i, sc) => 1,
                    Some(_) => continue,
                };
                total += 1;
                v.push((prox, (e.flags & F_NOISY != 0) as u8, -e.mtime, i as u32));
            }
            if v.len() > MAX_RESULTS {
                v.select_nth_unstable(MAX_RESULTS);
                v.truncate(MAX_RESULTS);
            }
            (v, total)
        })
        .collect();
    let total = parts.iter().map(|p| p.1).sum();
    let mut all: Vec<_> = parts.into_iter().flat_map(|p| p.0).collect();
    all.sort_unstable();
    all.truncate(MAX_RESULTS);
    (all.into_iter().map(|t| item_from(ix, t.3)).collect(), total)
}

/// Dizinde olmayan klasörde uzantıya göre canlı listeleme (sınırlı); önce doğrudan içindekiler, en yeniler önce.
pub fn walk_by_ext(dir: &str, ext: &str, generation: u64, latest: &AtomicU64) -> (Vec<Item>, usize) {
    let start = std::time::Instant::now();
    let root = std::path::PathBuf::from(dir);
    let mut stack = vec![root.clone()];
    let mut hits: Vec<(u8, i64, Item)> = Vec::new();
    let mut seen = 0usize;
    while let Some(d) = stack.pop() {
        if seen > 300_000 || start.elapsed().as_secs() >= 3 || latest.load(Ordering::Relaxed) != generation {
            break;
        }
        let prox = if d == root { 0 } else { 1 };
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            seen += 1;
            let ft = e.file_type().ok();
            let is_dir = ft.is_some_and(|f| f.is_dir());
            if is_dir {
                if !ft.is_some_and(|f| f.is_symlink()) {
                    stack.push(e.path());
                }
                continue;
            }
            let name = e.file_name().to_string_lossy().to_string();
            if !ext_ok(&util::fold(&name), Some(ext)) {
                continue;
            }
            if let Some(it) = item_from_path(&e.path().to_string_lossy(), None) {
                hits.push((prox, -it.mtime, it));
            }
        }
    }
    let total = hits.len();
    hits.sort_by_key(|h| (h.0, h.1));
    hits.truncate(MAX_RESULTS);
    (hits.into_iter().map(|h| h.2).collect(), total)
}

/// Boş arama: Windows Recent + uygulama geçmişi + dizindeki son değişenler tek listede.
pub fn recent_items(ix: &Index, tab: u8, recent: &[(String, i64)]) -> Vec<Item> {
    use std::collections::HashMap;
    let mut best: HashMap<String, (i64, Option<Item>)> = HashMap::new();
    for (p, t) in recent {
        let k = util::fold(p);
        let e = best.entry(k).or_insert((*t, None));
        if *t > e.0 {
            e.0 = *t;
        }
        if e.1.is_none() {
            e.1 = Some(Item { path: p.clone(), ..Default::default() });
        }
    }
    // Dizinden en son değişenler
    let mut cand: Vec<(i64, u32)> = ix
        .entries
        .par_iter()
        .enumerate()
        .filter(|(_, e)| e.flags & (F_DEL | F_NOISY) == 0 && e.parent != NONE && tab_ok(e.cat, tab) && e.mtime <= util::now_secs() + 60)
        .map(|(i, e)| (-e.mtime, i as u32))
        .collect();
    let k = 300.min(cand.len());
    if k > 0 && cand.len() > k {
        cand.select_nth_unstable(k);
        cand.truncate(k);
    }
    for (m, i) in cand {
        let it = item_from(ix, i);
        let key = util::fold(&it.path);
        let e = best.entry(key).or_insert((-m, None));
        if -m > e.0 {
            e.0 = -m;
        }
        if e.1.as_ref().is_none_or(|x| x.name.is_empty()) {
            e.1 = Some(it);
        }
    }
    let mut out: Vec<Item> = Vec::new();
    for (_, (t, it)) in best {
        let Some(mut it) = it else { continue };
        if it.name.is_empty() {
            // geçmiş/Recent kaydı: diskten oku
            match item_from_path(&it.path, None) {
                Some(x) => it = x,
                None => continue,
            }
        }
        if !tab_ok(it.cat, tab) {
            continue;
        }
        it.when = Some(t);
        out.push(it);
    }
    out.sort_by_key(|a| std::cmp::Reverse(a.when));
    out.truncate(300);
    out
}

/// İçerik araması. Kısmi sonuçları `send` ile akıtır; `latest` değişirse durur.
/// `scope`: dizindeki klasör kimliği; `walk`: dizinde olmayan klasör (canlı tarama).
#[allow(clippy::too_many_arguments)]
pub fn search_content(
    ix: &RwLock<Index>,
    q: &str,
    tab: u8,
    scope: Option<u32>,
    walk: Option<&str>,
    ext: Option<&str>,
    generation: u64,
    latest: &AtomicU64,
    send: &dyn Fn(Vec<Item>, bool),
) {
    let phrase = util::fold(q.trim());
    if phrase.is_empty() {
        send(Vec::new(), true);
        return;
    }
    let words: Vec<String> = terms_of(q);
    let cat_ok = |cat: u8| match tab {
        0 => true,
        t if t == CAT_CODE => cat == CAT_CODE,
        t if t == CAT_DOC => cat == CAT_DOC,
        _ => false,
    };
    let mut cand: Vec<(i64, String, String, u64, u8)> = if let Some(dir) = walk {
        text_files_under(dir, ext, generation, latest).into_iter().filter(|c| cat_ok(c.4)).collect()
    } else {
        let ix = ix.read().unwrap();
        ix.entries
            .iter()
            .enumerate()
            .filter(|(i, e)| {
                e.flags & (F_DEL | F_NOISY) == 0
                    && e.flags & F_TEXT != 0
                    && e.size <= 4 << 20
                    && cat_ok(e.cat)
                    && (ext.is_none() || ext_ok(&ix.lower[e.lower_off as usize..e.lower_off as usize + e.lower_len as usize], ext))
                    && scope.is_none_or(|sc| under(&ix, *i, sc))
            })
            .map(|(i, e)| (-e.mtime, ix.path(i as u32), ix.name(i as u32).to_string(), e.size, e.cat))
            .collect()
    };
    cand.sort_unstable_by_key(|c| c.0);
    let found = std::sync::Mutex::new(Vec::<Item>::new());
    let sort = |f: &mut Vec<Item>| f.sort_by_key(|a| (a.rank, std::cmp::Reverse(a.mtime)));
    for chunk in cand.chunks(256) {
        if latest.load(Ordering::SeqCst) != generation {
            return;
        }
        chunk.par_iter().for_each(|(m, path, name, size, cat)| {
            if latest.load(Ordering::Relaxed) != generation {
                return;
            }
            if let Some((rank, ln, line)) = grep_file_tiered(path, &phrase, &words) {
                found.lock().unwrap().push(Item {
                    path: path.clone(),
                    name: name.clone(),
                    is_dir: false,
                    size: *size,
                    mtime: -m,
                    cat: *cat,
                    when: None,
                    line: Some((ln, line)),
                    rank,
                });
            }
        });
        let mut f = found.lock().unwrap();
        sort(&mut f);
        if f.len() >= MAX_RESULTS {
            f.truncate(MAX_RESULTS);
            send(f.clone(), true);
            return;
        }
        send(f.clone(), false);
    }
    let mut f = found.into_inner().unwrap();
    sort(&mut f);
    send(f, true);
}

/// Dizinde olmayan klasör için metin dosyalarını topla (sınırlı canlı tarama).
fn text_files_under(dir: &str, ext: Option<&str>, generation: u64, latest: &AtomicU64) -> Vec<(i64, String, String, u64, u8)> {
    let start = std::time::Instant::now();
    let mut out = Vec::new();
    let mut stack = vec![std::path::PathBuf::from(dir)];
    while let Some(d) = stack.pop() {
        if out.len() > 20_000 || start.elapsed().as_secs() >= 3 || latest.load(Ordering::Relaxed) != generation {
            break;
        }
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                if !ft.is_symlink() {
                    stack.push(e.path());
                }
                continue;
            }
            if !util::is_textual(&name) || !ext_ok(&util::fold(&name), ext) {
                continue;
            }
            let Ok(md) = e.metadata() else { continue };
            if md.len() > 4 << 20 {
                continue;
            }
            let cat = util::category(&name, false);
            out.push((-crate::index::mtime_of(&md), e.path().to_string_lossy().to_string(), name, md.len(), cat));
        }
    }
    out
}

/// Dosyada ilk eşleşen satırı bul (1 tabanlı satır no).
pub fn grep_file(path: &str, needle_folded: &str) -> Option<(usize, String)> {
    let data = std::fs::read(path).ok()?;
    if data[..data.len().min(8192)].contains(&0) {
        return None;
    }
    let text = String::from_utf8_lossy(&data);
    grep_text(&text, needle_folded)
}

pub fn grep_file_tiered(path: &str, phrase: &str, words: &[String]) -> Option<(u8, usize, String)> {
    let data = std::fs::read(path).ok()?;
    if data[..data.len().min(8192)].contains(&0) {
        return None;
    }
    let text = String::from_utf8_lossy(&data);
    grep_tiered(&text, phrase, words)
}

pub fn grep_text(text: &str, needle_folded: &str) -> Option<(usize, String)> {
    for (n, line) in text.lines().enumerate() {
        if line.len() < needle_folded.len() {
            continue;
        }
        if util::fold(line).contains(needle_folded) {
            let l: String = line.trim_end().chars().take(300).collect();
            return Some((n + 1, l));
        }
    }
    None
}

fn bounded(hay: &str, needle: &str) -> bool {
    hay.match_indices(needle).any(|(i, m)| {
        let before = hay[..i].chars().next_back().is_none_or(|c| !c.is_alphanumeric());
        let after = hay[i + m.len()..].chars().next().is_none_or(|c| !c.is_alphanumeric());
        before && after
    })
}

/// Kademeli içerik eşleşmesi: (kademe, satır no, satır).
/// 0 = ifade birebir (kelime sınırlarıyla), 1 = ifade bir kelimenin parçası olarak,
/// 2 = tüm kelimeler aynı satırda, 3 = kelimelerden biri (≥3 harf). Dosyadaki en iyi satır seçilir.
pub fn grep_tiered(text: &str, phrase: &str, words: &[String]) -> Option<(u8, usize, String)> {
    let mut best: Option<(u8, usize, String)> = None;
    let multi = words.len() > 1;
    for (n, line) in text.lines().enumerate() {
        let f = util::fold(line);
        let tier = if f.contains(phrase) {
            if bounded(&f, phrase) { 0 } else { 1 }
        } else if multi && words.iter().all(|w| f.contains(w.as_str())) {
            2
        } else if multi && words.iter().any(|w| w.chars().count() >= 3 && f.contains(w.as_str())) {
            3
        } else {
            continue;
        };
        if best.as_ref().is_none_or(|b| tier < b.0) {
            best = Some((tier, n + 1, line.trim_end().chars().take(300).collect()));
            if tier == 0 {
                break;
            }
        }
    }
    best
}

/// Klasör içeriği: en son değiştirilen en üstte (eşitlikte ada göre).
pub fn list_dir(dir: &str) -> Vec<Item> {
    let mut v: Vec<(std::time::SystemTime, Item)> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let md = e.metadata().ok();
            let is_dir = md.as_ref().is_some_and(|m| m.is_dir());
            let t = md.as_ref().and_then(|m| m.modified().ok()).unwrap_or(std::time::UNIX_EPOCH);
            v.push((t, Item {
                path: e.path().to_string_lossy().to_string(),
                cat: util::category(&name, is_dir),
                size: if is_dir { 0 } else { md.as_ref().map(|m| m.len()).unwrap_or(0) },
                mtime: md.as_ref().map(crate::index::mtime_of).unwrap_or(0),
                name,
                is_dir,
                when: None,
                line: None,
                rank: 0,
            }));
        }
    }
    sort_newest(&mut v);
    v.into_iter().map(|(_, i)| i).collect()
}

/// En yeni önce; aynı zamanda ada göre (alt saniye hassasiyetinde: yeni oluşturulan öğe hemen üstte).
pub fn sort_newest(v: &mut [(std::time::SystemTime, Item)]) {
    v.sort_by_cached_key(|(t, i)| (std::cmp::Reverse(*t), util::fold(&i.name)));
}

/// Dizinde olmayan klasörlerde canlı tarama ile ad araması (sınırlı).
pub fn walk_search(dir: &str, q: &str, ext: Option<&str>, generation: u64, latest: &AtomicU64) -> (Vec<Item>, usize) {
    let terms = terms_of(q);
    let ext_query = query_ext(q).is_some();
    let start = std::time::Instant::now();
    let root = std::path::PathBuf::from(dir);
    let mut stack = vec![root.clone()];
    let mut hits: Vec<([u8; 5], usize, Item)> = Vec::new();
    let mut seen = 0usize;
    while let Some(d) = stack.pop() {
        if seen > 300_000 || start.elapsed().as_secs() >= 3 || latest.load(Ordering::Relaxed) != generation {
            break;
        }
        let prox = if d == root { 0 } else { 1 };
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            seen += 1;
            let name = e.file_name().to_string_lossy().to_string();
            let ft = e.file_type().ok();
            let is_dir = ft.is_some_and(|f| f.is_dir());
            if is_dir && !ft.is_some_and(|f| f.is_symlink()) {
                stack.push(e.path());
            }
            let lower = util::fold(&name);
            if ext.is_some() && (is_dir || !ext_ok(&lower, ext)) {
                continue;
            }
            if let Some(s) = score(&lower, &terms)
                && let Some(it) = item_from_path(&e.path().to_string_lossy(), None)
            {
                hits.push((rank_key(s, false, is_dir, ext_query, prox), name.len(), it));
            }
        }
    }
    let total = hits.len();
    hits.sort_by_key(|h| (h.0, h.1));
    hits.truncate(MAX_RESULTS);
    (hits.into_iter().map(|h| h.2).collect(), total)
}

/// Adda eşleşen karakter aralıkları (karakter indeksleri, vurgulama için).
pub fn match_ranges(name: &str, q: &str) -> Vec<(usize, usize)> {
    let lower: Vec<char> = name.chars().map(util::fold_char).collect();
    let mut out: Vec<(usize, usize)> = Vec::new();
    let terms = terms_of(q);
    for (ti, t) in terms.iter().enumerate() {
        let tc: Vec<char> = t.chars().collect();
        if tc.is_empty() || tc.len() > lower.len() {
            continue;
        }
        let pos = (0..=lower.len() - tc.len()).filter(|&i| lower[i..i + tc.len()] == tc[..]).collect::<Vec<_>>();
        // önek > kelime başı > ilk geçiş
        let best = pos
            .iter()
            .copied()
            .find(|&i| i == 0)
            .or_else(|| pos.iter().copied().find(|&i| i > 0 && !lower[i - 1].is_alphanumeric()))
            .or_else(|| pos.first().copied());
        if let Some(b) = best {
            out.push((b, b + tc.len()));
        } else if ti == 0 && terms.len() == 1 {
            // bulanık: sırayla karakterler
            let mut k = 0;
            for (i, c) in lower.iter().enumerate() {
                if k < tc.len() && *c == tc[k] {
                    out.push((i, i + 1));
                    k += 1;
                }
            }
            if k < tc.len() {
                out.clear();
            }
        }
    }
    out.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for r in out {
        if let Some(l) = merged.last_mut()
            && r.0 <= l.1
        {
            l.1 = l.1.max(r.1);
            continue;
        }
        merged.push(r);
    }
    merged
}

fn filter_ext(v: Vec<Item>, ext: Option<&str>) -> Vec<Item> {
    match ext {
        None => v,
        Some(_) => v.into_iter().filter(|i| !i.is_dir && ext_ok(&util::fold(&i.name), ext)).collect(),
    }
}

/// Arama iş parçacığını başlat.
pub fn spawn(ix: Arc<RwLock<Index>>, latest: Arc<AtomicU64>, rx: Receiver<Req>, tx: Sender<Res>) {
    std::thread::spawn(move || {
        // (klasör yolu, dizin uzunluğu) -> dizindeki kimlik
        let mut scope_cache: Option<(String, usize, Option<u32>)> = None;
        while let Ok(mut req) = rx.recv() {
            while let Ok(r) = rx.try_recv() {
                req = r;
            }
            if latest.load(Ordering::SeqCst) != req.generation {
                continue;
            }
            let q = req.query.trim().to_string();
            let ext = req.ext.clone();
            let ext = ext.as_deref();
            let generation = req.generation;
            let send_all = |items: Vec<Item>, total: usize| {
                let _ = tx.send(Res { generation, items, total, done: true });
            };
            let mut scope_id = |dir: &str| -> Option<u32> {
                let ixr = ix.read().unwrap();
                match &scope_cache {
                    Some((p, n, id)) if p == dir && *n == ixr.len() => *id,
                    _ => {
                        let id = ixr.find(dir);
                        scope_cache = Some((dir.to_string(), ixr.len(), id));
                        id
                    }
                }
            };
            match (req.kind, req.scope.clone()) {
                (Kind::Content, scope) => {
                    let sid = scope.as_deref().and_then(&mut scope_id);
                    let walk = if sid.is_none() { scope.as_deref() } else { None };
                    let tx2 = tx.clone();
                    search_content(&ix, &q, req.tab, sid, walk, ext, generation, &latest, &move |items, done| {
                        let total = items.len();
                        let _ = tx2.send(Res { generation, items, total, done });
                    });
                }
                (Kind::Name, Some(dir)) if q.is_empty() && ext.is_some() => {
                    // uzantı süzgeci + boş sorgu: bu klasör ve altındaki tüm eşleşen dosyalar
                    let e = ext.unwrap_or_default();
                    let (items, total) = match scope_id(&dir) {
                        Some(sid) => list_by_ext(&ix.read().unwrap(), e, req.tab, Some(sid)),
                        None => walk_by_ext(&dir, e, generation, &latest),
                    };
                    send_all(items, total);
                }
                (Kind::Name, None) if q.is_empty() && ext.is_some() => {
                    let (items, total) = list_by_ext(&ix.read().unwrap(), ext.unwrap_or_default(), req.tab, None);
                    send_all(items, total);
                }
                (Kind::Name, Some(dir)) if q.is_empty() => {
                    let v = filter_ext(list_dir(&dir), ext);
                    let n = v.len();
                    send_all(v, n);
                }
                (Kind::Name, Some(dir)) => {
                    let sid = scope_id(&dir);
                    let opts = Opts { tab: req.tab, scope: sid, outside: true, ext: ext.map(str::to_string), ..Default::default() };
                    let (items, total) = match sid {
                        Some(_) => search_ranked(&ix.read().unwrap(), &q, &opts),
                        None => {
                            // dizinde olmayan klasör: canlı tarama + ardından diğer yerler
                            let (mut v, t) = walk_search(&dir, &q, ext, generation, &latest);
                            let (g, gt) = search_ranked(&ix.read().unwrap(), &q, &Opts { scope: None, ..opts });
                            let seen: std::collections::HashSet<String> = v.iter().map(|i| util::fold(&i.path)).collect();
                            v.extend(g.into_iter().filter(|i| !seen.contains(&util::fold(&i.path))));
                            v.truncate(MAX_RESULTS);
                            (v, t + gt)
                        }
                    };
                    send_all(items, total);
                }
                (Kind::Name, None) if q.is_empty() => {
                    let items = filter_ext(recent_items(&ix.read().unwrap(), req.tab, &req.recent), ext);
                    let total = items.len();
                    send_all(items, total);
                }
                (Kind::Dest, _) if q.is_empty() => {
                    let items: Vec<Item> = req.recent.iter().filter_map(|(p, t)| item_from_path(p, Some(*t))).filter(|i| i.is_dir).collect();
                    let total = items.len();
                    send_all(items, total);
                }
                (k, _) => {
                    let opts = Opts { tab: req.tab, dirs_only: k == Kind::Dest, ext: ext.map(str::to_string), ..Default::default() };
                    let (items, total) = search_ranked(&ix.read().unwrap(), &q, &opts);
                    send_all(items, total);
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ix() -> Index {
        let mut ix = Index::default();
        let r = ix.push("C:", NONE, 0, 0, true, 0);
        let p = ix.push("projeler", r, 0, 10, true, 0);
        ix.push("userFetch.ts", p, 10, 5, false, 0);
        ix.push("fetchUser.ts", p, 10, 6, false, 0);
        ix.push("my_fetchUser.rs", p, 10, 7, false, 0);
        ix.push("fxexstch.txt", p, 10, 8, false, 0);
        ix.push("FetchUserler", p, 0, 9, true, 0);
        ix.push("ISTANBUL.png", p, 10, 9, false, 0);
        ix
    }
    #[test]
    fn ranking() {
        let ix = ix();
        let (r, total) = search_names(&ix, "fetchuser", 0, false);
        let names: Vec<_> = r.iter().map(|i| i.name.as_str()).collect();
        assert!(names[..2].contains(&"fetchUser.ts") && names[..2].contains(&"FetchUserler"));
        assert_eq!(names[2], "my_fetchUser.rs");
        assert!(!names.contains(&"userFetch.ts"));
        assert_eq!(total, 3);
        let (r, _) = search_names(&ix, "fetch", 4, false);
        assert!(r.iter().all(|i| i.cat == CAT_CODE));
        let (r, _) = search_names(&ix, "fetch", 0, true);
        assert_eq!(r.len(), 1);
        assert!(r[0].is_dir);
        let (r, _) = search_names(&ix, "istanbul", 0, false);
        assert_eq!(r.len(), 1);
        let (r, _) = search_names(&ix, "fch", 0, false);
        assert!(r.iter().any(|i| i.name == "fxexstch.txt"));
        let (r, _) = search_names(&ix, "user ts", 0, false);
        assert_eq!(r.len(), 2);
    }
    #[test]
    fn grep() {
        let t = "a\nimport x\nexport async function FetchUser(id)\n";
        assert_eq!(grep_text(t, "fetchuser").unwrap().0, 3);
        assert!(grep_text(t, "nope").is_none());
    }
    #[test]
    fn scoped_and_ranges() {
        let ix = ix();
        let p = (0..ix.len() as u32).find(|&i| ix.name(i) == "projeler").unwrap();
        let (r, _) = search_names_in(&ix, "fetch", 0, false, Some(p));
        assert!(r.len() >= 4 && r.iter().any(|i| i.name == "fetchUser.ts"));
        let f = (0..ix.len() as u32).find(|&i| ix.name(i) == "fetchUser.ts").unwrap();
        let (r, _) = search_names_in(&ix, "fetch", 0, false, Some(f));
        assert!(r.is_empty());
        assert_eq!(match_ranges("Ege-tatili.png", "tatil"), vec![(4, 9)]);
        assert_eq!(match_ranges("İstanbul", "ist"), vec![(0, 3)]);
    }
    #[test]
    fn folders_first_and_ext_query() {
        let mut ix = Index::default();
        let r = ix.push("C:", NONE, 0, 0, true, 0);
        let d = ix.push("docs", r, 0, 1, true, 0);
        ix.push("rapor.pdf", d, 10, 5, false, 0);
        ix.push("rapor", d, 0, 4, true, 0);
        ix.push("raporlar", d, 0, 3, true, 0);
        ix.push("rapor.png", d, 10, 6, false, 0);
        // klasörler önce
        let (r1, _) = search_names(&ix, "rapor", 0, false);
        let n: Vec<_> = r1.iter().map(|i| i.name.as_str()).collect();
        assert!(r1[0].is_dir && r1[1].is_dir, "{n:?}");
        assert!(!r1[2].is_dir && !r1[3].is_dir);
        // uzantılı sorgu: eşleşen dosya doğrudan başta
        let (r2, _) = search_names(&ix, "rapor.pdf", 0, false);
        assert_eq!(r2[0].name, "rapor.pdf");
        let (r3, _) = search_names(&ix, ".png", 0, false);
        assert_eq!(r3[0].name, "rapor.png");
        assert_eq!(query_ext("rapor.pdf").as_deref(), Some("pdf"));
        assert_eq!(query_ext(".PNG").as_deref(), Some("png"));
        assert_eq!(query_ext("rapor"), None);
        assert_eq!(query_ext("v1."), None);
        // uzantı süzgeci
        let (r4, _) = search_ranked(&ix, "rapor", &Opts { ext: Some("pdf".into()), ..Default::default() });
        assert_eq!(r4.len(), 1);
        assert_eq!(r4[0].name, "rapor.pdf");
        assert!(ext_ok("a.pdf", Some("pdf")) && !ext_ok("apdf", Some("pdf")) && !ext_ok(".pdf", Some("pdf")));
    }
    #[test]
    fn current_folder_priority() {
        let mut ix = Index::default();
        let r = ix.push("C:", NONE, 0, 0, true, 0);
        let other = ix.push("other", r, 0, 1, true, 0);
        let cur = ix.push("cur", r, 0, 1, true, 0);
        let sub = ix.push("sub", cur, 0, 1, true, 0);
        // diğer yerdekiler daha yeni ve daha kısa olsa da sonda
        ix.push("kullanici.ini", other, 1, 99, false, 0);
        ix.push("kullanici.ini", sub, 1, 50, false, 0);
        ix.push("kullanici.ini", cur, 1, 1, false, 0);
        let (v, _) = search_ranked(&ix, "kullanici", &Opts { scope: Some(cur), outside: true, ..Default::default() });
        let paths: Vec<_> = v.iter().map(|i| i.path.replace('\\', "/")).collect();
        assert_eq!(paths.len(), 3);
        assert!(paths[0].ends_with("cur/kullanici.ini"), "{paths:?}");
        assert!(paths[1].ends_with("cur/sub/kullanici.ini"), "{paths:?}");
        assert!(paths[2].ends_with("other/kullanici.ini"), "{paths:?}");
        // outside=false → yalnızca kapsam
        let (v, _) = search_ranked(&ix, "kullanici", &Opts { scope: Some(cur), ..Default::default() });
        assert_eq!(v.len(), 2);
    }
    #[test]
    fn content_exact_first() {
        let w = terms_of("fetch user");
        assert_eq!(grep_tiered("let x = 1;\nfetch user here\n", "fetch user", &w).unwrap().0, 0);
        assert_eq!(grep_tiered("refetch users\n", "fetch user", &w).unwrap().0, 1);
        assert_eq!(grep_tiered("user then fetch\n", "fetch user", &w).unwrap().0, 2);
        assert_eq!(grep_tiered("only fetch\n", "fetch user", &w).unwrap().0, 3);
        assert!(grep_tiered("nothing\n", "fetch user", &w).is_none());
        // dosyadaki en iyi satır seçilir
        let t = grep_tiered("user fetch\nfetch user\n", "fetch user", &w).unwrap();
        assert_eq!((t.0, t.1), (0, 2));
        let w1 = terms_of("rapor");
        assert_eq!(grep_tiered("RAPOR hazır", "rapor", &w1).unwrap().0, 0);
        assert_eq!(grep_tiered("raporlar", "rapor", &w1).unwrap().0, 1);
    }
    #[test]
    fn content_search_ranks_exact_files_first() {
        let d = std::env::temp_dir().join(format!("bul-ct-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("a.txt"), "user and fetch\n").unwrap();
        std::fs::write(d.join("b.txt"), "call fetch user now\n").unwrap();
        std::fs::write(d.join("c.txt"), "nothing\n").unwrap();
        let ix = RwLock::new(Index::default());
        let latest = AtomicU64::new(1);
        let out = std::sync::Mutex::new(Vec::new());
        search_content(&ix, "fetch user", 0, None, Some(&d.to_string_lossy()), None, 1, &latest, &|v, done| {
            if done {
                *out.lock().unwrap() = v;
            }
        });
        let v = out.into_inner().unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].name, "b.txt");
        assert_eq!(v[1].name, "a.txt");
        let _ = std::fs::remove_dir_all(&d);
    }
    #[test]
    fn empty_query_ext_lists_all_indexed_files() {
        let mut ix = Index::default();
        let r = ix.push("C:", NONE, 0, 0, true, 0);
        let docs = ix.push("Belgeler", r, 0, 1, true, 0);
        let sub = ix.push("Eski", docs, 0, 1, true, 0);
        let other = ix.push("Diğer", r, 0, 1, true, 0);
        ix.push("a.pdf", docs, 10, 5, false, 0);
        ix.push("b.PDF", sub, 10, 50, false, 0);
        ix.push("c.pdf", other, 10, 99, false, 0);
        ix.push("not.txt", docs, 10, 7, false, 0);
        ix.push("klasor.pdf", docs, 0, 7, true, 0);
        // genel: tüm pdf'ler, en yeni önce, klasör ve başka uzantı yok
        let (v, total) = list_by_ext(&ix, "pdf", 0, None);
        let n: Vec<_> = v.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(n, vec!["c.pdf", "b.PDF", "a.pdf"]);
        assert_eq!(total, 3);
        // kapsam: önce doğrudan içindekiler, sonra alt klasörler; dışarıdakiler yok
        let (v, _) = list_by_ext(&ix, "pdf", 0, Some(docs));
        let n: Vec<_> = v.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(n, vec!["a.pdf", "b.PDF"]);
        // kök kapsamı ("Tümü > C:") boş değil
        let (v, _) = list_by_ext(&ix, "pdf", 0, Some(r));
        assert_eq!(v.len(), 3);
    }

    #[test]
    fn empty_query_ext_through_search_thread() {
        let mut ix = Index::default();
        let r = ix.push("C:", NONE, 0, 0, true, 0);
        let d = ix.push("Belgeler", r, 0, 1, true, 0);
        ix.push("rapor.pdf", d, 10, 5, false, 0);
        ix.push("resim.png", d, 10, 5, false, 0);
        let root_path = ix.path(r);
        let ix = Arc::new(RwLock::new(ix));
        let latest = Arc::new(AtomicU64::new(1));
        let (qtx, qrx) = std::sync::mpsc::channel();
        let (rtx, rrx) = std::sync::mpsc::channel();
        spawn(ix, latest.clone(), qrx, rtx);
        // gözat: "Tümü > C:" + pdf çipi + boş sorgu
        qtx.send(Req { generation: 1, query: String::new(), tab: 0, kind: Kind::Name, recent: Vec::new(), scope: Some(root_path), ext: Some("pdf".into()) }).unwrap();
        let res = rrx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert_eq!(res.items.len(), 1);
        assert_eq!(res.items[0].name, "rapor.pdf");
        // genel arama + pdf çipi + boş sorgu
        latest.store(2, Ordering::SeqCst);
        qtx.send(Req { generation: 2, query: String::new(), tab: 0, kind: Kind::Name, recent: Vec::new(), scope: None, ext: Some("pdf".into()) }).unwrap();
        let res = rrx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert_eq!(res.items.len(), 1);
        assert_eq!(res.items[0].name, "rapor.pdf");
    }

    #[test]
    fn walk_by_ext_lists_unindexed_folder() {
        let d = std::env::temp_dir().join(format!("fp-walkext-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("alt")).unwrap();
        std::fs::write(d.join("a.pdf"), "x").unwrap();
        std::fs::write(d.join("alt").join("b.pdf"), "x").unwrap();
        std::fs::write(d.join("c.txt"), "x").unwrap();
        let latest = AtomicU64::new(1);
        let (v, total) = walk_by_ext(&d.to_string_lossy(), "pdf", 1, &latest);
        assert_eq!(total, 2);
        assert_eq!(v[0].name, "a.pdf");
        assert_eq!(v[1].name, "b.pdf");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn fuzz() {
        assert!(fuzzy("fetchuser", "fcu"));
        assert!(!fuzzy("fetchuser", "ufc"));
    }
}
