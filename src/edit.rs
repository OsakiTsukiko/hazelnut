//! Keeping per-character styles in sync with text edits, and applying formatting.

use crate::model::{Attr, LineStyle, Style};
use eframe::egui::Color32;
use std::ops::Range;

/// A single contiguous replacement: `removed` chars at `at` replaced by `inserted` chars.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Change {
    pub at: usize,
    pub removed: usize,
    pub inserted: usize,
}

/// Find the edited region between `old` and `new`.
///
/// `cursor` (char index after the edit, if known) disambiguates edits inside
/// repeated characters, e.g. typing `a` inside `aaa`.
pub fn diff(old: &str, new: &str, cursor: Option<usize>) -> Option<Change> {
    if old == new {
        return None;
    }
    let a: Vec<char> = old.chars().collect();
    let b: Vec<char> = new.chars().collect();
    let min = a.len().min(b.len());

    let mut p = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    if let Some(c) = cursor {
        let grown = b.len().saturating_sub(a.len());
        p = p.min(c.saturating_sub(grown));
    }
    let mut s = a[p..]
        .iter()
        .rev()
        .zip(b[p..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count()
        .min(min - p);
    if let Some(c) = cursor {
        s = s.min(b.len().saturating_sub(c.max(p)));
    }
    Some(Change {
        at: p,
        removed: a.len() - p - s,
        inserted: b.len() - p - s,
    })
}

/// The style newly typed text gets at char position `at`: it continues the
/// character before it (or the one after, at the start).
pub fn style_at(styles: &[Style], at: usize) -> Style {
    if at > 0 {
        styles.get(at - 1).copied().unwrap_or_default()
    } else {
        styles.first().copied().unwrap_or_default()
    }
}

/// Apply `change` to `styles`, giving inserted chars `insert_style`
/// (or the inherited style if `None`).
pub fn apply(styles: &[Style], change: Change, insert_style: Option<Style>) -> Vec<Style> {
    let at = change.at.min(styles.len());
    let end = (at + change.removed).min(styles.len());
    let ins = insert_style.unwrap_or_else(|| style_at(styles, at));
    let mut out = Vec::with_capacity(styles.len() + change.inserted);
    out.extend_from_slice(&styles[..at]);
    out.extend(std::iter::repeat_n(ins, change.inserted));
    out.extend_from_slice(&styles[end..]);
    out
}

#[cfg(test)]
/// Recompute styles for `new` text given the previous `old` text and styles.
pub fn sync(old: &str, styles: &[Style], new: &str, cursor: Option<usize>, insert_style: Option<Style>) -> Vec<Style> {
    match diff(old, new, cursor) {
        Some(ch) => apply(styles, ch, insert_style),
        None => styles.to_vec(),
    }
}

/// Toggle an attribute over a range: if every char already has it, remove it; otherwise set it.
pub fn toggle(styles: &mut [Style], range: Range<usize>, attr: Attr) {
    let range = clamp(range, styles.len());
    let all = styles[range.clone()].iter().all(|s| attr.get(s));
    for s in &mut styles[range] {
        attr.set(s, !all);
    }
}

pub fn set_fg(styles: &mut [Style], range: Range<usize>, c: Option<Color32>) {
    let range = clamp(range, styles.len());
    styles[range].iter_mut().for_each(|s| s.fg = c);
}

pub fn set_bg(styles: &mut [Style], range: Range<usize>, c: Option<Color32>) {
    let range = clamp(range, styles.len());
    styles[range].iter_mut().for_each(|s| s.bg = c);
}

pub fn set_underline(styles: &mut [Style], range: Range<usize>, kind: Option<LineStyle>) {
    let range = clamp(range, styles.len());
    styles[range].iter_mut().for_each(|s| s.underline = kind);
}

pub fn clear(styles: &mut [Style], range: Range<usize>) {
    let range = clamp(range, styles.len());
    styles[range].iter_mut().for_each(|s| *s = Style::default());
}

/// Char range of the word at (or just before) char index `at`.
pub fn word_at(text: &str, at: usize) -> (usize, usize) {
    let chars: Vec<char> = text.chars().collect();
    let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '\'';
    let at = at.min(chars.len());
    let anchor = if at < chars.len() && is_word(chars[at]) {
        at
    } else if at > 0 && is_word(chars[at - 1]) {
        at - 1
    } else {
        return (at, at);
    };
    let start = (0..anchor).rev().take_while(|&i| is_word(chars[i])).last().unwrap_or(anchor);
    let end = (anchor..chars.len()).take_while(|&i| is_word(chars[i])).last().map_or(anchor, |i| i + 1);
    (start, end)
}

fn clamp(r: Range<usize>, len: usize) -> Range<usize> {
    r.start.min(len)..r.end.min(len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_insert_in_repeats_uses_cursor() {
        assert_eq!(diff("aa", "aaa", Some(2)), Some(Change { at: 1, removed: 0, inserted: 1 }));
        assert_eq!(diff("aa", "aaa", Some(1)), Some(Change { at: 0, removed: 0, inserted: 1 }));
        assert_eq!(diff("aaa", "aa", Some(1)), Some(Change { at: 1, removed: 1, inserted: 0 }));
    }

    #[test]
    fn diff_replace() {
        assert_eq!(diff("hello world", "hello there world", None), Some(Change { at: 6, removed: 0, inserted: 6 }));
        assert_eq!(diff("abcdef", "abXef", Some(3)), Some(Change { at: 2, removed: 2, inserted: 1 }));
        assert_eq!(diff("same", "same", None), None);
    }

    #[test]
    fn apply_inherits_previous_style() {
        let b = Style { bold: true, ..Default::default() };
        let styles = vec![b, b, Style::default()];
        let out = sync("ab ", &styles, "abc ", Some(3), None);
        assert_eq!(out, vec![b, b, b, Style::default()]);
    }

    #[test]
    fn word_bounds() {
        assert_eq!(word_at("hello brave world", 8), (6, 11));
        assert_eq!(word_at("hello brave world", 11), (6, 11));
        assert_eq!(word_at("hello brave world", 0), (0, 5));
        assert_eq!(word_at("a  b", 2), (2, 2));
        assert_eq!(word_at("don't stop", 2), (0, 5));
    }

    #[test]
    fn toggle_all_or_nothing() {
        let mut s = vec![Style::default(); 4];
        s[0].bold = true;
        toggle(&mut s, 0..4, Attr::Bold);
        assert!(s.iter().all(|x| x.bold));
        toggle(&mut s, 0..4, Attr::Bold);
        assert!(s.iter().all(|x| !x.bold));
    }
}
