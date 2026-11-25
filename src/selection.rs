//! Explorer tarzı seçim modeli: sabit çapa + imleç.
//!
//! - Düz tık / ok: tek öğe, çapa buraya.
//! - Ctrl+tık: öğeyi ekle/çıkar, çapa buraya.
//! - Shift+tık / Shift+ok: çapa..hedef aralığı (her iki yönde; yön değişince aralık küçülür/büyür).
//! - Ctrl+Shift+tık: önceki seçim korunur, çapa..hedef eklenir.
//! - "Yapışkan" kip (Çoklu seçim modu): oklar seçimi bozmaz, Shift aralıkları mevcut seçime eklenir.
//!
//! `set` boşsa yalnızca imleçteki öğe seçili sayılır (tek seçim).

use std::collections::BTreeSet;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub cursor: usize,
    pub anchor: usize,
    pub set: BTreeSet<usize>,
    /// Shift aralığının üzerine ekleneceği taban seçim (Ctrl+tık / yapışkan kip).
    pub base: BTreeSet<usize>,
}

impl Selection {
    #[cfg(test)]
    pub fn new(cursor: usize) -> Self {
        Selection { cursor, anchor: cursor, ..Default::default() }
    }

    /// Düz tık: tek seçim, çapa buraya.
    pub fn plain(&mut self, i: usize) {
        self.cursor = i;
        self.anchor = i;
        self.set.clear();
        self.base.clear();
    }

    /// Ctrl+tık / Boşluk: öğeyi ekle ya da çıkar. `include_cursor`: seçim boşken imleçteki
    /// öğe de seçime girsin mi (Explorer'da ilk Ctrl+tık davranışı).
    pub fn toggle(&mut self, i: usize, include_cursor: bool) {
        if include_cursor && self.set.is_empty() && self.cursor != i {
            self.set.insert(self.cursor);
        }
        if !self.set.remove(&i) {
            self.set.insert(i);
        }
        self.cursor = i;
        self.anchor = i;
        self.base = self.set.clone();
    }

    /// Shift+tık: çapa..i aralığı. `keep_base`: Ctrl+Shift / yapışkan kip.
    pub fn extend_to(&mut self, i: usize, keep_base: bool) {
        let (lo, hi) = if self.anchor <= i { (self.anchor, i) } else { (i, self.anchor) };
        let mut s = if keep_base { self.base.clone() } else { BTreeSet::new() };
        s.extend(lo..=hi);
        self.set = s;
        self.cursor = i;
    }

    /// Ok tuşları. `n` = öğe sayısı.
    pub fn step(&mut self, d: isize, n: usize, shift: bool, sticky: bool) {
        if n == 0 {
            return;
        }
        let t = (self.cursor as isize + d).clamp(0, n as isize - 1) as usize;
        if shift {
            self.extend_to(t, sticky);
        } else if sticky {
            self.cursor = t;
            self.anchor = t;
            self.base = self.set.clone();
        } else {
            self.plain(t);
        }
    }

    pub fn select_all(&mut self, n: usize) {
        self.set = (0..n).collect();
        self.base = self.set.clone();
    }

    /// Seçili öğeler (boşsa imleçteki tek öğe).
    #[cfg(test)]
    pub fn effective(&self, n: usize) -> Vec<usize> {
        if self.set.is_empty() {
            if self.cursor < n { vec![self.cursor] } else { Vec::new() }
        } else {
            self.set.iter().copied().filter(|&i| i < n).collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn set(v: &[usize]) -> BTreeSet<usize> {
        v.iter().copied().collect()
    }

    #[test]
    fn shift_click_both_directions() {
        let mut s = Selection::new(0);
        s.plain(5);
        s.extend_to(8, false);
        assert_eq!(s.set, set(&[5, 6, 7, 8]));
        // aşağı seçtikten sonra yukarıya Shift+tık: çapa 5 sabit, aralık 2..5
        s.extend_to(2, false);
        assert_eq!(s.set, set(&[2, 3, 4, 5]));
        assert_eq!(s.anchor, 5);
        assert_eq!(s.cursor, 2);
        // tekrar aşağı
        s.extend_to(6, false);
        assert_eq!(s.set, set(&[5, 6]));
        // çapanın kendisi
        s.extend_to(5, false);
        assert_eq!(s.set, set(&[5]));
    }

    #[test]
    fn shift_arrows_grow_and_shrink() {
        let mut s = Selection::new(4);
        for _ in 0..3 {
            s.step(1, 20, true, false);
        }
        assert_eq!(s.set, set(&[4, 5, 6, 7]));
        s.step(-1, 20, true, false);
        assert_eq!(s.set, set(&[4, 5, 6]));
        s.step(-1, 20, true, false);
        s.step(-1, 20, true, false);
        assert_eq!(s.set, set(&[4]));
        s.step(-1, 20, true, false);
        s.step(-1, 20, true, false);
        assert_eq!(s.set, set(&[2, 3, 4]));
        assert_eq!(s.anchor, 4);
        // sınırlar
        for _ in 0..10 {
            s.step(-1, 20, true, false);
        }
        assert_eq!(s.cursor, 0);
        assert_eq!(s.set, set(&[0, 1, 2, 3, 4]));
        // düz ok seçimi bırakır
        s.step(1, 20, false, false);
        assert!(s.set.is_empty());
        assert_eq!((s.cursor, s.anchor), (1, 1));
    }

    #[test]
    fn ctrl_click_toggles_and_sets_anchor() {
        let mut s = Selection::new(0);
        s.plain(2);
        s.toggle(5, true);
        assert_eq!(s.set, set(&[2, 5]));
        s.toggle(2, true);
        assert_eq!(s.set, set(&[5]));
        assert_eq!(s.anchor, 2);
        // Ctrl+tık sonrası düz Shift+tık: yalnızca çapa..hedef (Explorer)
        s.toggle(7, true);
        s.extend_to(9, false);
        assert_eq!(s.set, set(&[7, 8, 9]));
    }

    #[test]
    fn ctrl_shift_click_extends() {
        let mut s = Selection::new(0);
        s.plain(1);
        s.toggle(3, true); // {1,3}, çapa 3
        s.extend_to(5, true);
        assert_eq!(s.set, set(&[1, 3, 4, 5]));
        s.extend_to(0, true);
        assert_eq!(s.set, set(&[0, 1, 2, 3]));
    }

    #[test]
    fn sticky_mode_keeps_marks() {
        let mut s = Selection::new(0);
        s.toggle(0, false);
        s.step(1, 10, false, true);
        s.step(1, 10, false, true);
        assert_eq!(s.set, set(&[0]));
        s.toggle(2, false);
        assert_eq!(s.set, set(&[0, 2]));
        s.step(1, 10, true, true);
        s.step(1, 10, true, true);
        assert_eq!(s.set, set(&[0, 2, 3, 4]));
        s.step(-1, 10, true, true);
        assert_eq!(s.set, set(&[0, 2, 3]));
        s.select_all(10);
        assert_eq!(s.set.len(), 10);
        assert_eq!(s.effective(10).len(), 10);
        let e = Selection::new(3);
        assert_eq!(e.effective(10), vec![3]);
    }
}
