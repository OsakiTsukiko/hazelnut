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
        }
    }

    pub fn word_count(&self) -> usize {
        self.text.split_whitespace().count()
    }
}

#[derive(Clone, Debug)]
pub struct Notebook {
    pub title: String,
    pub chapters: Vec<Chapter>,
}

impl Notebook {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            chapters: vec![Chapter::new("")],
        }
    }

    /// The notebook shown on first launch, demonstrating the features.
    pub fn welcome() -> Self {
        let src = include_str!("../assets/welcome.hzn");
        crate::format::parse(src).unwrap_or_else(|_| Notebook::new("Notebook"))
    }
}
