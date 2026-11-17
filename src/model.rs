//! In-memory notebook model.
//!
//! A [`Notebook`] is one file on disk. It holds an ordered list of
//! [`Chapter`]s. Each chapter body is plain text plus one [`Style`] per
//! character, which keeps editing and formatting trivially in sync.

use eframe::egui::Color32;
use std::sync::atomic::{AtomicU64, Ordering};

/// Inline formatting of a single character.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Style {
    /// This syntax character has a backslash escape in the file.
    pub escaped: bool,
    pub bold: bool,
    pub italic: bool,
    /// Underline pattern, if underlined.
    pub underline: Option<LineStyle>,
    pub strike: bool,
    /// Text (foreground) color. `None` = theme default.
    pub fg: Option<Color32>,
    /// Highlight (background) color. `None` = transparent.
    pub bg: Option<Color32>,
}

/// Underline patterns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LineStyle {
    Solid,
    Double,
    Dotted,
    Dashed,
    Wavy,
    ZigZag,
}

impl LineStyle {
    pub const ALL: [LineStyle; 6] = [
        LineStyle::Solid,
        LineStyle::Double,
        LineStyle::Dotted,
        LineStyle::Dashed,
        LineStyle::Wavy,
        LineStyle::ZigZag,
    ];

    pub fn name(self) -> &'static str {
        match self {
            LineStyle::Solid => "Solid",
            LineStyle::Double => "Double",
            LineStyle::Dotted => "Dotted",
            LineStyle::Dashed => "Dashed",
            LineStyle::Wavy => "Wavy",
            LineStyle::ZigZag => "Zig-zag",
        }
    }

    /// Name used in the `.hzn` format (`u=wavy`).
    pub fn key(self) -> &'static str {
        match self {
            LineStyle::Solid => "solid",
            LineStyle::Double => "double",
            LineStyle::Dotted => "dotted",
            LineStyle::Dashed => "dashed",
            LineStyle::Wavy => "wavy",
            LineStyle::ZigZag => "zigzag",
        }
    }

    pub fn from_key(k: &str) -> Option<Self> {
        match k {
            "squiggly" | "squiggle" | "wave" => Some(LineStyle::Wavy),
            "zig-zag" => Some(LineStyle::ZigZag),
            _ => Self::ALL.into_iter().find(|l| l.key() == k),
        }
    }
}

impl Style {
    pub fn is_plain(&self) -> bool {
        *self == Style::default()
    }
}

/// Toggleable boolean attributes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attr {
    Bold,
    Italic,
    Underline,
    Strike,
}

impl Attr {
    pub fn get(self, s: &Style) -> bool {
        match self {
            Attr::Bold => s.bold,
            Attr::Italic => s.italic,
            Attr::Underline => s.underline.is_some(),
            Attr::Strike => s.strike,
        }
    }

    pub fn set(self, s: &mut Style, v: bool) {
        match self {
            Attr::Bold => s.bold = v,
            Attr::Italic => s.italic = v,
            // Turning underline on keeps an existing pattern.
            Attr::Underline if v => {
                s.underline.get_or_insert(LineStyle::Solid);
            }
            Attr::Underline => s.underline = None,
            Attr::Strike => s.strike = v,
        }
    }
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// One "page" of a notebook.
#[derive(Clone, Debug)]
pub struct Chapter {
    /// Runtime-only identity (not saved), used to key editor state.
    pub id: u64,
    pub title: String,
    pub text: String,
    /// Exactly one entry per `char` in `text`.
    pub styles: Vec<Style>,
    /// The [`Category`] this chapter is filed under, by runtime id.
    pub category: Option<u64>,
}

impl Chapter {
    pub fn new(title: impl Into<String>) -> Self {
        Self::with_content(title, String::new(), Vec::new())
    }

    pub fn with_content(title: impl Into<String>, text: String, mut styles: Vec<Style>) -> Self {
        styles.resize(text.chars().count(), Style::default());
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            title: title.into(),
            text,
            styles,
            category: None,
        }
    }

    pub fn word_count(&self) -> usize {
        self.text.split_whitespace().count()
    }
}

/// A named, colored group of chapters. Each chapter has at most one.
#[derive(Clone, Debug)]
pub struct Category {
    /// Runtime-only identity (not saved); chapters refer to it.
    pub id: u64,
    pub name: String,
    pub color: Color32,
}

impl Category {
    pub fn new(name: impl Into<String>, color: Color32) -> Self {
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            name: name.into(),
            color,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Notebook {
    pub title: String,
    pub chapters: Vec<Chapter>,
    /// In display order. Uncategorized chapters come before all of them.
    pub categories: Vec<Category>,
}

impl Notebook {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            chapters: vec![Chapter::new("")],
            categories: Vec::new(),
        }
    }

    pub fn category(&self, id: Option<u64>) -> Option<&Category> {
        id.and_then(|id| self.categories.iter().find(|c| c.id == id))
    }

    pub fn category_mut(&mut self, id: u64) -> Option<&mut Category> {
        self.categories.iter_mut().find(|c| c.id == id)
    }

    /// Sort key of a chapter's group: 0 = uncategorized, then categories in order.
    pub fn group_of(&self, ch: &Chapter) -> usize {
        ch.category
            .and_then(|id| self.categories.iter().position(|c| c.id == id))
            .map_or(0, |i| i + 1)
    }

    /// Drop dangling category references and keep chapters ordered by group
    /// (stable, so the order within a group is preserved).
    pub fn normalize(&mut self) {
        let ids: Vec<u64> = self.categories.iter().map(|c| c.id).collect();
        for ch in &mut self.chapters {
            if ch.category.is_some_and(|id| !ids.contains(&id)) {
                ch.category = None;
            }
        }
        let mut chapters = std::mem::take(&mut self.chapters);
        chapters.sort_by_key(|ch| self.group_of(ch));
        self.chapters = chapters;
    }

    /// The notebook shown on first launch, demonstrating the features.
    pub fn welcome() -> Self {
        let src = include_str!("../assets/welcome.hzn");
        crate::format::parse(src).unwrap_or_else(|_| Notebook::new("Notebook"))
    }
}
