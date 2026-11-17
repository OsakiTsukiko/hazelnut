//! PDF export.
//!
//! Lays chapters out on A4 pages and writes the PDF by hand, embedding
//! subsets of the Noto Sans faces the app itself uses. Colors are adapted
//! for paper: text colors are darkened and highlights lightened.

use crate::blocks::{self, Kind};
use crate::model::{Category, Chapter, LineStyle, Notebook, Style};
use crate::render;
use crate::theme;
use eframe::egui::Color32;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::io::Write as _;
use std::ops::Range;

const PAGE_W: f32 = 595.28;
const PAGE_H: f32 = 841.89;
const MARGIN_X: f32 = 68.0;
const MARGIN_TOP: f32 = 72.0;
const MARGIN_BOTTOM: f32 = 72.0;
const CONTENT_W: f32 = PAGE_W - 2.0 * MARGIN_X;

const BODY: f32 = 11.0;
const BODY_LINE: f32 = BODY * 1.6;
const TITLE: f32 = 22.0;

const INK: Color32 = Color32::from_rgb(0x1d, 0x1b, 0x18);
const MUTED: Color32 = Color32::from_rgb(0x7a, 0x75, 0x6d);
const RULE: Color32 = Color32::from_rgb(0xd8, 0xd3, 0xcb);
/// Ticked checkboxes: the app's accent, darkened for paper.
const CHECK: Color32 = Color32::from_rgb(0xb0, 0x7a, 0x48);

/// Export the whole notebook (`only: None`) or a single chapter.
pub fn export(nb: &Notebook, only: Option<usize>) -> Vec<u8> {
    let mut doc = Doc::new();
    let chapters: Vec<usize> = match only {
        Some(i) => vec![i],
        None => (0..nb.chapters.len()).collect(),
    };
    let toc = (only.is_none() && nb.chapters.len() > 1).then(|| doc.cover(nb));
    doc.numbered = true;
    let mut starts = Vec::new();
    for &i in &chapters {
        starts.push(doc.chapter(nb, &nb.chapters[i]));
    }
    if let Some(toc) = toc {
        doc.fill_toc(nb, &toc, &starts);
    }
    doc.page_numbers();

    let title = match only {
        Some(i) => non_empty(&nb.chapters[i].title, "Untitled"),
        None => non_empty(&nb.title, "Untitled notebook"),
    };
    doc.finish(title)
}

fn non_empty<'a>(s: &'a str, fallback: &'a str) -> &'a str {
    match s.trim() {
        "" => fallback,
        t => t,
    }
}

// ---------------------------------------------------------------- colors

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// Text colors are picked for a dark background; darken them for paper.
fn print_fg(c: Option<Color32>) -> Color32 {
    c.map_or(INK, |c| mix(c, Color32::BLACK, 0.38))
}

/// Highlights are dark tints; show their hue as a light tint instead.
fn print_bg(c: Color32) -> Color32 {
    mix(theme::highlight_display(c), Color32::WHITE, 0.62)
}

fn rgb(c: Color32) -> String {
    let f = |x: u8| x as f32 / 255.0;
    format!("{:.3} {:.3} {:.3}", f(c.r()), f(c.g()), f(c.b()))
}

/// Top-down page coordinate to PDF (bottom-up).
fn py(y: f32) -> f32 {
    PAGE_H - y
}

// ---------------------------------------------------------------- fonts

#[derive(Clone, Copy, PartialEq, Eq)]
enum Face {
    Regular,
    Medium,
    Bold,
    Italic,
    BoldItalic,
    Mono,
}

impl Face {
    const ALL: [Face; 6] = [Face::Regular, Face::Medium, Face::Bold, Face::Italic, Face::BoldItalic, Face::Mono];

    fn of(s: &Style) -> Face {
        match (s.bold, s.italic) {
            (false, false) => Face::Regular,
            (true, false) => Face::Bold,
            (false, true) => Face::Italic,
            (true, true) => Face::BoldItalic,
        }
    }

    fn bytes(self) -> &'static [u8] {
        match self {
            Face::Regular => theme::FONT_REGULAR,
            Face::Medium => theme::FONT_MEDIUM,
            Face::Bold => theme::FONT_BOLD,
            Face::Italic => theme::FONT_ITALIC,
            Face::BoldItalic => theme::FONT_BOLD_ITALIC,
            Face::Mono => theme::mono_font_bytes(),
        }
    }

    fn ps_name(self) -> &'static str {
        match self {
            Face::Regular => "NotoSans-Regular",
            Face::Medium => "NotoSans-Medium",
            Face::Bold => "NotoSans-Bold",
            Face::Italic => "NotoSans-Italic",
            Face::BoldItalic => "NotoSans-BoldItalic",
            Face::Mono => "Hack-Regular",
        }
    }
}

struct Font {
    face: ttf_parser::Face<'static>,
    data: &'static [u8],
    upem: f32,
    /// Glyphs drawn so far, with the character each stands for.
    used: BTreeMap<u16, char>,
}

impl Font {
    fn new(f: Face) -> Self {
        let data = f.bytes();
        let face = ttf_parser::Face::parse(data, 0).expect("bundled font parses");
        let upem = face.units_per_em() as f32;
        Self { face, data, upem, used: BTreeMap::new() }
    }

    fn gid(&self, c: char) -> u16 {
        self.face.glyph_index(c).map_or(0, |g| g.0)
    }

    /// Advance width in em.
    fn advance(&self, gid: u16) -> f32 {
        self.face.glyph_hor_advance(ttf_parser::GlyphId(gid)).unwrap_or(0) as f32 / self.upem
    }

    fn em(&self, v: f32) -> f32 {
        v * 1000.0 / self.upem
    }
}

// ---------------------------------------------------------------- layout

struct Page {
    ops: String,
    links: Vec<Link>,
    numbered: bool,
}

struct Link {
    rect: [f32; 4],
    page: usize,
    y: f32,
}

/// A character placed on a line, `x` relative to the line start.
struct Placed {
    c: char,
    style: Style,
    x: f32,
    w: f32,
}

/// A callout box whose bottom isn't known yet: its background is inserted
/// into page `page`'s content at byte `at` (so it's behind the text).
struct Callout {
    page: usize,
    at: usize,
    top: f32,
    bar: Color32,
}

const CALLOUT_PAD: f32 = 11.0;

struct TocEntry {
    page: usize,
    top: f32,
    chapter: usize,
}

struct Doc {
    fonts: Vec<Font>,
    pages: Vec<Page>,
    /// Page being drawn on.
    target: usize,
    /// Top of the free space on the current page.
    y: f32,
    /// Whether new pages get a page number.
    numbered: bool,
    /// Bookmarks: (title, page, PDF y).
    outline: Vec<(String, usize, f32)>,
}

impl Doc {
    fn new() -> Self {
        Self {
            fonts: Face::ALL.iter().map(|&f| Font::new(f)).collect(),
            pages: Vec::new(),
            target: 0,
            y: MARGIN_TOP,
            numbered: false,
            outline: Vec::new(),
        }
    }

    fn font(&self, f: Face) -> &Font {
        &self.fonts[f as usize]
    }

    fn ops(&mut self) -> &mut String {
        &mut self.pages[self.target].ops
    }

    fn new_page(&mut self) {
        self.pages.push(Page { ops: String::new(), links: Vec::new(), numbered: self.numbered });
        self.target = self.pages.len() - 1;
        self.y = MARGIN_TOP;
    }

    /// Start a new page unless `h` more points fit on this one.
    fn room(&mut self, h: f32) {
        if self.pages.is_empty() || self.y + h > PAGE_H - MARGIN_BOTTOM {
            self.new_page();
        }
    }

    fn char_width(&self, f: Face, c: char, size: f32) -> f32 {
        let font = self.font(f);
        font.advance(font.gid(c)) * size
    }

    fn measure(&self, f: Face, size: f32, s: &str) -> f32 {
        s.chars().map(|c| self.char_width(f, c, size)).sum()
    }

    /// Draw `s` with its baseline at `baseline` (top-down). Returns the width.
    fn text(&mut self, f: Face, size: f32, color: Color32, x: f32, baseline: f32, s: &str) -> f32 {
        let font = &mut self.fonts[f as usize];
        let mut hex = String::with_capacity(s.len() * 4);
        let mut w = 0.0;
        for c in s.chars() {
            let g = font.gid(c);
            font.used.entry(g).or_insert(c);
            let _ = write!(hex, "{g:04X}");
            w += font.advance(g) * size;
        }
        let op = format!(
            "BT /F{} {size:.2} Tf {} rg {x:.2} {:.2} Td <{hex}> Tj ET\n",
            f as usize,
            rgb(color),
            py(baseline)
        );
        self.ops().push_str(&op);
        w
    }

    fn fill_rect(&mut self, x: f32, top: f32, w: f32, h: f32, color: Color32) {
        let op = format!("{} rg {x:.2} {:.2} {w:.2} {h:.2} re f\n", rgb(color), py(top + h));
        self.ops().push_str(&op);
    }

    fn fill_round_rect(&mut self, x: f32, top: f32, w: f32, h: f32, r: f32, color: Color32) {
        let (x0, x1, y0, y1) = (x, x + w, py(top + h), py(top));
        let k = r * 0.5523; // bezier circle approximation
        let mut s = format!("{} rg ", rgb(color));
        let _ = write!(
            s,
            "{:.2} {y0:.2} m {:.2} {y0:.2} l {:.2} {y0:.2} {x1:.2} {:.2} {x1:.2} {:.2} c \
             {x1:.2} {:.2} l {x1:.2} {:.2} {:.2} {y1:.2} {:.2} {y1:.2} c \
             {:.2} {y1:.2} l {:.2} {y1:.2} {x0:.2} {:.2} {x0:.2} {:.2} c \
             {x0:.2} {:.2} l {x0:.2} {:.2} {:.2} {y0:.2} {:.2} {y0:.2} c f",
            x0 + r,
            x1 - r,
            x1 - r + k,
            y0 + r - k,
            y0 + r,
            y1 - r,
            y1 - r + k,
            x1 - r + k,
            x1 - r,
            x0 + r,
            x0 + r - k,
            y1 - r + k,
            y1 - r,
            y0 + r,
            y0 + r - k,
            x0 + r - k,
            x0 + r,
        );
        s.push('\n');
        self.ops().push_str(&s);
    }

    /// The category tag: a rectangle pointing right.
    fn tag(&mut self, x: f32, cy: f32, w: f32, h: f32, color: Color32) {
        let tip = h * 0.5;
        let (t, b, m) = (py(cy - h / 2.0), py(cy + h / 2.0), py(cy));
        let op = format!(
            "{} rg {x:.2} {t:.2} m {:.2} {t:.2} l {:.2} {m:.2} l {:.2} {b:.2} l {x:.2} {b:.2} l f\n",
            rgb(color),
            x + w - tip,
            x + w,
            x + w - tip
        );
        self.ops().push_str(&op);
    }

    fn stroke_path(&mut self, pts: &[(f32, f32)], width: f32, color: Color32, extra: &str) {
        let Some(((x, y), rest)) = pts.split_first() else { return };
        let mut s = format!("q {width:.2} w {} RG {extra} {x:.2} {:.2} m", rgb(color), py(*y));
        for (x, y) in rest {
            let _ = write!(s, " {x:.2} {:.2} l", py(*y));
        }
        s.push_str(" S Q\n");
        self.ops().push_str(&s);
    }

    fn decoration(&mut self, kind: LineStyle, x0: f32, x1: f32, y: f32, color: Color32, size: f32) {
        let w = (size * 0.065).max(0.6);
        match kind {
            LineStyle::Solid => self.stroke_path(&[(x0, y), (x1, y)], w, color, ""),
            LineStyle::Double => {
                self.stroke_path(&[(x0, y - 0.6), (x1, y - 0.6)], w * 0.8, color, "");
                self.stroke_path(&[(x0, y + 1.4), (x1, y + 1.4)], w * 0.8, color, "");
            }
            LineStyle::Dotted => self.stroke_path(&[(x0, y), (x1, y)], w * 1.3, color, "1 J [0 2.2] 0 d"),
            LineStyle::Dashed => self.stroke_path(&[(x0, y), (x1, y)], w, color, "[3 2] 0 d"),
            LineStyle::Wavy | LineStyle::ZigZag => {
                let (period, amp) = (4.5_f32, 1.1_f32);
                let wave = |x: f32| match kind {
                    LineStyle::Wavy => amp * (x * std::f32::consts::TAU / period).sin(),
                    _ => {
                        let t = (x / period).rem_euclid(1.0);
                        amp * (4.0 * (t - 0.5).abs() - 1.0)
                    }
                };
                let step = if kind == LineStyle::Wavy { 0.5 } else { period / 4.0 };
                let mut pts = Vec::new();
                let mut x = x0;
                while x < x1 {
                    pts.push((x, y + 0.5 + wave(x)));
                    x += step;
                }
                pts.push((x1, y + 0.5 + wave(x1)));
                self.stroke_path(&pts, w, color, "1 j");
            }
        }
    }

    /// Greedy word wrap of styled characters into lines of at most `width`.
    fn layout(&self, chars: &[(char, Style)], size: f32, width: f32) -> Vec<Vec<Placed>> {
        let cw = |k: usize| self.char_width(Face::of(&chars[k].1), chars[k].0, size);
        let n = chars.len();
        let mut lines = Vec::new();
        let mut line: Vec<Placed> = Vec::new();
        let mut x = 0.0;
        let mut i = 0;
        while i < n {
            let start = i;
            while i < n && !chars[i].0.is_whitespace() {
                i += 1;
            }
            let word_end = i;
            while i < n && chars[i].0.is_whitespace() {
                i += 1;
            }
            let word_w: f32 = (start..word_end).map(cw).sum();
            if x + word_w > width && !line.is_empty() {
                lines.push(std::mem::take(&mut line));
                x = 0.0;
            }
            for (k, &(c, style)) in chars.iter().enumerate().take(i).skip(start) {
                let w = cw(k);
                // A word longer than the line is broken anywhere.
                if k < word_end && x + w > width && !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                    x = 0.0;
                }
                line.push(Placed { c, style, x, w });
                x += w;
            }
        }
        if !line.is_empty() || lines.is_empty() {
            lines.push(line);
        }
        lines
    }

    /// Draw a laid-out line whose box starts at (`x0`, `top`).
    fn draw_line(&mut self, line: &[Placed], x0: f32, top: f32, size: f32, line_h: f32) {
        let baseline = top + line_h / 2.0 + size * 0.36;

        for (bg, r) in runs(line, |p| p.style.bg) {
            let (a, b) = span(line, &r);
            let h = size * 1.45;
            self.fill_rect(x0 + a, top + (line_h - h) / 2.0, b - a, h, print_bg(bg));
        }
        for ((face, color), r) in runs(line, |p| Some((Face::of(&p.style) as usize, print_fg(p.style.fg)))) {
            let s: String = line[r.clone()].iter().map(|p| p.c).collect();
            let face = Face::ALL[face];
            if !s.trim().is_empty() {
                self.text(face, size, color, x0 + line[r.start].x, baseline, &s);
            }
        }
        // Don't decorate trailing spaces at the end of a wrapped line.
        let end = line.iter().rposition(|p| !p.c.is_whitespace()).map_or(0, |i| i + 1);
        let line = &line[..end];
        for ((kind, color), r) in runs(line, |p| p.style.underline.map(|k| (k, print_fg(p.style.fg)))) {
            let (a, b) = span(line, &r);
            self.decoration(kind, x0 + a, x0 + b, baseline + size * 0.14, color, size);
        }
        for (color, r) in runs(line, |p| p.style.strike.then(|| print_fg(p.style.fg))) {
            let (a, b) = span(line, &r);
            self.decoration(LineStyle::Solid, x0 + a, x0 + b, baseline - size * 0.28, color, size);
        }
    }

    fn bullet(&mut self, cx: f32, cy: f32, hollow: bool) {
        let r = 1.9;
        let k = r * 0.5523;
        let (x, y) = (cx, py(cy));
        let circle = format!(
            "{:.2} {y:.2} m {:.2} {:.2} {:.2} {:.2} {x:.2} {:.2} c {:.2} {:.2} {:.2} {:.2} {:.2} {y:.2} c \
             {:.2} {:.2} {:.2} {:.2} {x:.2} {:.2} c {:.2} {:.2} {:.2} {:.2} {:.2} {y:.2} c",
            x + r, x + r, y + k, x + k, y + r, y + r,
            x - k, y + r, x - r, y + k, x - r,
            x - r, y - k, x - k, y - r, y - r,
            x + k, y - r, x + r, y - k, x + r,
        );
        let op = if hollow {
            format!("q 0.8 w {} RG {circle} S Q\n", rgb(INK))
        } else {
            format!("{} rg {circle} f\n", rgb(INK))
        };
        self.ops().push_str(&op);
    }

    fn checkbox(&mut self, x: f32, cy: f32, checked: bool) {
        let s = 8.5;
        let top = cy - s / 2.0;
        if checked {
            self.fill_round_rect(x, top, s, s, 1.8, CHECK);
            let pts = [(x + s * 0.23, cy + s * 0.02), (x + s * 0.43, cy + s * 0.22), (x + s * 0.78, cy - s * 0.2)];
            self.stroke_path(&pts, 1.2, Color32::WHITE, "1 J 1 j");
        } else {
            self.fill_round_rect(x, top, s, s, 1.8, MUTED);
            self.fill_round_rect(x + 0.8, top + 0.8, s - 1.6, s - 1.6, 1.1, Color32::WHITE);
        }
    }

    /// Like `room`, but a callout box being drawn continues on the new page.
    fn room_in(&mut self, h: f32, callout: &mut Option<Callout>) {
        if self.y + h + CALLOUT_PAD <= PAGE_H - MARGIN_BOTTOM || callout.is_none() {
            self.room(h);
            return;
        }
        let bar = callout.as_ref().map_or(MUTED, |c| c.bar);
        self.close_callout(callout);
        self.new_page();
        *callout = Some(Callout { page: self.target, at: 0, top: self.y, bar });
        self.y += CALLOUT_PAD;
    }

    /// Paint the open callout's box behind what was drawn in it.
    fn close_callout(&mut self, callout: &mut Option<Callout>) {
        let Some(c) = callout.take() else { return };
        let bottom = self.y + CALLOUT_PAD;
        let current = self.target;
        self.target = c.page;
        let len = self.pages[c.page].ops.len();
        let (x, w, h) = (MARGIN_X, CONTENT_W, bottom - c.top);
        self.fill_round_rect(x, c.top, w, h, 4.0, mix(c.bar, Color32::WHITE, 0.86));
        self.fill_round_rect(x, c.top, 3.0, h, 1.5, c.bar);
        let shapes = self.pages[c.page].ops.split_off(len);
        self.pages[c.page].ops.insert_str(c.at, &shapes);
        self.target = current;
        if self.target == c.page {
            self.y = bottom + 6.0;
        }
    }

    fn pill_width(&self, cat: &Category) -> f32 {
        self.measure(Face::Medium, 9.5, non_empty(&cat.name, "Untitled")) + 30.0
    }

    /// The category pill, vertically centered on `cy`.
    fn pill(&mut self, cat: &Category, x: f32, cy: f32) {
        let h = 18.0;
        let w = self.pill_width(cat);
        self.fill_round_rect(x, cy - h / 2.0, w, h, h / 2.0, mix(cat.color, Color32::WHITE, 0.84));
        self.tag(x + 8.0, cy, 10.0, 7.0, cat.color);
        let name = non_empty(&cat.name, "Untitled");
        self.text(Face::Medium, 9.5, print_fg(Some(cat.color)), x + 22.0, cy + 3.3, name);
    }

    // ------------------------------------------------------------ pages

    /// Title page with a table of contents (page numbers filled in later).
    fn cover(&mut self, nb: &Notebook) -> Vec<TocEntry> {
        self.new_page();
        self.y = MARGIN_TOP + 96.0;
        let bold = Style { bold: true, ..Default::default() };
        let title: Vec<_> = non_empty(&nb.title, "Untitled notebook").chars().map(|c| (c, bold)).collect();
        for line in self.layout(&title, 30.0, CONTENT_W) {
            self.draw_line(&line, MARGIN_X, self.y, 30.0, 38.0);
            self.y += 38.0;
        }
        self.y += 10.0;
        let n = nb.chapters.len();
        self.text(Face::Regular, 11.0, MUTED, MARGIN_X, self.y + 11.0, &format!("{n} chapters"));
        self.y += 44.0;
        self.stroke_path(&[(MARGIN_X, self.y), (MARGIN_X + CONTENT_W, self.y)], 0.6, RULE, "");
        self.y += 26.0;
        self.text(Face::Medium, 9.0, MUTED, MARGIN_X, self.y, "CONTENTS");
        self.y += 14.0;

        let mut entries = Vec::new();
        let mut last_group = None;
        for (i, ch) in nb.chapters.iter().enumerate() {
            let group = nb.group_of(ch);
            if last_group.is_some_and(|g| g != group) {
                self.y += 8.0;
            }
            last_group = Some(group);
            self.room(24.0);
            entries.push(TocEntry { page: self.target, top: self.y, chapter: i });
            self.y += 24.0;
        }
        entries
    }

    fn fill_toc(&mut self, nb: &Notebook, toc: &[TocEntry], starts: &[(usize, f32)]) {
        let has_tags = !nb.categories.is_empty();
        for e in toc {
            let (page, dest_y) = starts[e.chapter];
            let ch = &nb.chapters[e.chapter];
            self.target = e.page;
            let baseline = e.top + 12.0 + 11.5 * 0.36;
            let mut x = MARGIN_X;
            if has_tags {
                if let Some(cat) = nb.category(ch.category) {
                    self.tag(x, e.top + 12.0, 11.0, 7.5, cat.color);
                }
                x += 20.0;
            }
            let num = (page + 1).to_string();
            let num_w = self.measure(Face::Regular, 11.0, &num);
            let right = MARGIN_X + CONTENT_W;
            let avail = right - num_w - 24.0 - x;
            let mut title = non_empty(&ch.title, "Untitled").to_owned();
            if self.measure(Face::Regular, 11.5, &title) > avail {
                while !title.is_empty() && self.measure(Face::Regular, 11.5, &format!("{title}…")) > avail {
                    title.pop();
                }
                title = format!("{}…", title.trim_end());
            }
            let tw = self.text(Face::Regular, 11.5, INK, x, baseline, &title);
            self.text(Face::Regular, 11.0, MUTED, right - num_w, baseline, &num);
            let (a, b) = (x + tw + 8.0, right - num_w - 6.0);
            if b > a {
                self.stroke_path(&[(a, baseline), (b, baseline)], 0.9, RULE, "1 J [0 3] 0 d");
            }
            let rect = [MARGIN_X, py(e.top + 24.0), right, py(e.top)];
            self.pages[e.page].links.push(Link { rect, page, y: dest_y });
        }
    }

    /// Lay out one chapter on a new page. Returns its (page, PDF y).
    fn chapter(&mut self, nb: &Notebook, ch: &Chapter) -> (usize, f32) {
        self.new_page();
        let start = (self.target, py(self.y));
        let title = non_empty(&ch.title, "Untitled");
        self.outline.push((title.to_owned(), start.0, start.1));

        let bold = Style { bold: true, ..Default::default() };
        let chars: Vec<_> = title.chars().map(|c| (c, bold)).collect();
        let line_h = TITLE * 1.3;
        let lines = self.layout(&chars, TITLE, CONTENT_W);
        let last_w = lines.last().and_then(|l| l.last()).map_or(0.0, |p| p.x + p.w);
        for line in &lines {
            self.room(line_h);
            self.draw_line(line, MARGIN_X, self.y, TITLE, line_h);
            self.y += line_h;
        }
        if let Some(cat) = nb.category(ch.category) {
            if last_w + 14.0 + self.pill_width(cat) <= CONTENT_W {
                self.pill(cat, MARGIN_X + last_w + 14.0, self.y - line_h / 2.0);
            } else {
                self.pill(cat, MARGIN_X, self.y + 12.0);
                self.y += 24.0;
            }
        }
        self.y += 18.0;

        // Body, paragraph by paragraph.
        let text = ch.text.trim_end();
        let mut styles = ch.styles.iter().copied();
        let mut first = true;
        let mut callout: Option<Callout> = None;
        let code = crate::code::document(text, &ch.styles);
        let mut in_code = false;
        for (li, (start, para, b)) in blocks::parsed_lines(text, &ch.styles).into_iter().enumerate() {
            if let Some(code_line) = &code.lines[li] {
                self.close_callout(&mut callout);
                for _ in 0..para.chars().count() + 1 { styles.next(); }
                if code_line.fence {
                    in_code = !in_code;
                    self.y += 6.0;
                    if in_code && !code_line.language.is_empty() {
                        self.room(14.0);
                        self.text(Face::Mono, 8.0, MUTED, MARGIN_X + 12.0, self.y + 9.0, &code_line.language);
                        self.y += 14.0;
                    }
                } else {
                    self.draw_code_line(para, &code.colors[start..]);
                }
                first = false;
                continue;
            }
            let mut chars = Vec::new();
            for (k, c) in para.chars().enumerate() {
                let mut s = styles.next().unwrap_or_default();
                if k < b.marker {
                    continue; // drawn as a box, bullet or checkbox, or implied by the heading
                }
                match b.kind {
                    Kind::Heading(_) => s.bold = true,
                    // Done tasks are dimmed (after `print_fg` darkens it, a mid gray).
                    Kind::Task(true) if s.fg.is_none() => s.fg = Some(Color32::from_rgb(0xb4, 0xaf, 0xa7)),
                    _ => {}
                }
                match c {
                    '\t' => chars.extend(std::iter::repeat_n((' ', s), 4)),
                    c if c.is_control() => {}
                    c => chars.push((c, s)),
                }
            }
            styles.next(); // the '\n'
            let was_first = std::mem::replace(&mut first, false);

            // Callout boxes: a `[!spec]` or the first `>` line opens one.
            let opens = b.quoted && (b.spec.is_some() || callout.is_none());
            if callout.is_some() && (!b.quoted || opens) {
                self.close_callout(&mut callout);
            }
            if opens {
                if !was_first {
                    self.y += 4.0;
                }
                self.room(BODY_LINE + 2.0 * CALLOUT_PAD);
                let (bar, _) = render::callout_colors(b.spec(para));
                callout = Some(Callout { page: self.target, at: self.pages[self.target].ops.len(), top: self.y, bar });
                self.y += CALLOUT_PAD;
            }
            let (left, width) = if b.quoted { (MARGIN_X + 20.0, CONTENT_W - 36.0) } else { (MARGIN_X, CONTENT_W) };

            match b.kind {
                Kind::Heading(level) => {
                    let size = [18.0, 15.0, 13.0][(level.clamp(1, 3) - 1) as usize];
                    let line_h = size * 1.35;
                    if !was_first && !opens {
                        self.y += size * 0.6;
                    }
                    // Keep a heading with at least one line of what follows.
                    self.room_in(line_h + BODY_LINE, &mut callout);
                    for line in self.layout(&chars, size, width) {
                        self.room_in(line_h, &mut callout);
                        self.draw_line(&line, left, self.y, size, line_h);
                        self.y += line_h;
                    }
                    self.y += 2.0;
                }
                Kind::Bullet | Kind::Task(_) => {
                    let x = left + b.depth as f32 * 16.0;
                    let text_x = x + 16.0;
                    let lines = self.layout(&chars, BODY, left + width - text_x);
                    for (i, line) in lines.iter().enumerate() {
                        self.room_in(BODY_LINE, &mut callout);
                        if i == 0 {
                            let cy = self.y + BODY_LINE / 2.0 - BODY * 0.02;
                            match b.kind {
                                Kind::Task(done) => self.checkbox(x + 1.0, cy, done),
                                _ => self.bullet(x + 4.0, cy, b.depth % 2 == 1),
                            }
                        }
                        self.draw_line(line, text_x, self.y, BODY, BODY_LINE);
                        self.y += BODY_LINE;
                    }
                }
                Kind::Plain => {
                    if chars.is_empty() {
                        self.room_in(BODY_LINE, &mut callout);
                        self.y += BODY_LINE;
                        continue;
                    }
                    for line in self.layout(&chars, BODY, width) {
                        self.room_in(BODY_LINE, &mut callout);
                        self.draw_line(&line, left, self.y, BODY, BODY_LINE);
                        self.y += BODY_LINE;
                    }
                }
            }
        }
        self.close_callout(&mut callout);
        start
    }

    /// Fixed-width code rows preserve indentation, wrap long lines, and keep
    /// syntax colors on paper. Each wrapped row gets its own panel background.
    fn draw_code_line(&mut self, source: &str, colors: &[Color32]) {
        const SIZE: f32 = 9.5;
        const HEIGHT: f32 = 15.0;
        let advance = self.char_width(Face::Mono, ' ', SIZE);
        let columns = ((CONTENT_W - 24.0) / advance).floor().max(1.0) as usize;
        let mut chars = Vec::new();
        for (i, c) in source.chars().enumerate() {
            let color = print_fg(colors.get(i).copied());
            if c == '\t' {
                for _ in 0..4 - chars.len() % 4 { chars.push((' ', color)); }
            } else if !c.is_control() {
                chars.push((c, color));
            }
        }
        if chars.is_empty() { chars.push((' ', INK)); }
        for row in chars.chunks(columns) {
            self.room(HEIGHT);
            self.fill_round_rect(MARGIN_X, self.y, CONTENT_W, HEIGHT, 0.0, Color32::from_rgb(0xf2, 0xf3, 0xf5));
            let mut x = MARGIN_X + 12.0;
            let mut from = 0;
            while from < row.len() {
                let color = row[from].1;
                let mut to = from + 1;
                while to < row.len() && row[to].1 == color { to += 1; }
                let text: String = row[from..to].iter().map(|c| c.0).collect();
                x += self.text(Face::Mono, SIZE, color, x, self.y + 11.0, &text);
                from = to;
            }
            self.y += HEIGHT;
        }
    }

    fn page_numbers(&mut self) {
        for i in 0..self.pages.len() {
            if !self.pages[i].numbered {
                continue;
            }
            self.target = i;
            let s = (i + 1).to_string();
            let w = self.measure(Face::Regular, 9.0, &s);
            self.text(Face::Regular, 9.0, MUTED, (PAGE_W - w) / 2.0, PAGE_H - 38.0, &s);
        }
    }

    // ------------------------------------------------------------ writing

    fn finish(self, title: &str) -> Vec<u8> {
        let mut next = 0u32;
        let mut id = || {
            next += 1;
            next
        };
        let catalog = id();
        let pages_root = id();
        let info = id();

        struct FontIds {
            face: usize,
            type0: u32,
            cid: u32,
            desc: u32,
            file: u32,
            to_unicode: u32,
        }
        let fonts: Vec<FontIds> = (0..self.fonts.len())
            .filter(|&i| !self.fonts[i].used.is_empty())
            .map(|face| FontIds { face, type0: id(), cid: id(), desc: id(), file: id(), to_unicode: id() })
            .collect();
        let page_ids: Vec<(u32, u32)> = self.pages.iter().map(|_| (id(), id())).collect();
        let outline_root = id();
        let outline_ids: Vec<u32> = self.outline.iter().map(|_| id()).collect();

        let mut w = Writer::default();
        w.buf.extend_from_slice(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n");

        w.obj(
            catalog,
            &format!("<< /Type /Catalog /Pages {pages_root} 0 R /Outlines {outline_root} 0 R /PageMode /UseOutlines >>"),
        );
        let kids: Vec<String> = page_ids.iter().map(|(p, _)| format!("{p} 0 R")).collect();
        w.obj(
            pages_root,
            &format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), page_ids.len()),
        );
        w.obj(info, &format!("<< /Title {} /Producer (Hazelnut) /Creator (Hazelnut) >>", pdf_string(title)));

        // Fonts: Type0 / CIDFontType2 with glyph ids as CIDs.
        for (k, f) in fonts.iter().enumerate() {
            let font = &self.fonts[f.face];
            let face = Face::ALL[f.face];
            let name = format!("HZN{}A{}+{}", (b'A' + k as u8) as char, (b'A' + f.face as u8) as char, face.ps_name());
            w.obj(
                f.type0,
                &format!(
                    "<< /Type /Font /Subtype /Type0 /BaseFont /{name} /Encoding /Identity-H \
                     /DescendantFonts [{} 0 R] /ToUnicode {} 0 R >>",
                    f.cid, f.to_unicode
                ),
            );
            let mut widths = String::new();
            for &g in font.used.keys() {
                let _ = write!(widths, "{g} [{:.1}] ", font.advance(g) * 1000.0);
            }
            w.obj(
                f.cid,
                &format!(
                    "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /{name} \
                     /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> \
                     /FontDescriptor {} 0 R /DW 0 /W [{widths}] /CIDToGIDMap /Identity >>",
                    f.desc
                ),
            );
            let fc = &font.face;
            let bbox = fc.global_bounding_box();
            let italic = matches!(face, Face::Italic | Face::BoldItalic);
            let cap = fc.capital_height().unwrap_or(fc.ascender()) as f32;
            w.obj(
                f.desc,
                &format!(
                    "<< /Type /FontDescriptor /FontName /{name} /Flags {} /FontBBox [{:.0} {:.0} {:.0} {:.0}] \
                     /ItalicAngle {:.1} /Ascent {:.0} /Descent {:.0} /CapHeight {:.0} /StemV {} /FontFile2 {} 0 R >>",
                    if italic { 96 } else { 32 },
                    font.em(bbox.x_min as f32),
                    font.em(bbox.y_min as f32),
                    font.em(bbox.x_max as f32),
                    font.em(bbox.y_max as f32),
                    fc.italic_angle(),
                    font.em(fc.ascender() as f32),
                    font.em(fc.descender() as f32),
                    font.em(cap),
                    if matches!(face, Face::Bold | Face::BoldItalic) { 140 } else { 80 },
                    f.file
                ),
            );
            let keep: BTreeSet<u16> = font.used.keys().copied().collect();
            let file = subset_ttf(font.data, &keep).unwrap_or_else(|| font.data.to_vec());
            w.stream(f.file, &format!("/Length1 {}", file.len()), &file);
            w.stream(f.to_unicode, "", to_unicode(&font.used).as_bytes());
        }

        let font_res: String = fonts.iter().map(|f| format!("/F{} {} 0 R ", f.face, f.type0)).collect();
        for (i, page) in self.pages.iter().enumerate() {
            let (pid, cid) = page_ids[i];
            let annots: String = page
                .links
                .iter()
                .map(|l| {
                    format!(
                        "<< /Type /Annot /Subtype /Link /Rect [{:.2} {:.2} {:.2} {:.2}] /Border [0 0 0] \
                         /Dest [{} 0 R /XYZ null {:.2} null] >> ",
                        l.rect[0], l.rect[1], l.rect[2], l.rect[3], page_ids[l.page].0, l.y
                    )
                })
                .collect();
            w.obj(
                pid,
                &format!(
                    "<< /Type /Page /Parent {pages_root} 0 R /MediaBox [0 0 {PAGE_W} {PAGE_H}] \
                     /Resources << /Font << {font_res}>> >> /Contents {cid} 0 R /Annots [{annots}] >>"
                ),
            );
            w.stream(cid, "", page.ops.as_bytes());
        }

        let count = outline_ids.len();
        match (outline_ids.first(), outline_ids.last()) {
            (Some(first), Some(last)) => {
                w.obj(outline_root, &format!("<< /Type /Outlines /First {first} 0 R /Last {last} 0 R /Count {count} >>"))
            }
            _ => w.obj(outline_root, "<< /Type /Outlines /Count 0 >>"),
        }
        for (k, (title, page, y)) in self.outline.iter().enumerate() {
            let mut d = format!(
                "<< /Title {} /Parent {outline_root} 0 R /Dest [{} 0 R /XYZ null {y:.2} null]",
                pdf_string(title),
                page_ids[*page].0
            );
            if k > 0 {
                let _ = write!(d, " /Prev {} 0 R", outline_ids[k - 1]);
            }
            if k + 1 < count {
                let _ = write!(d, " /Next {} 0 R", outline_ids[k + 1]);
            }
            d.push_str(" >>");
            w.obj(outline_ids[k], &d);
        }

        w.finish(catalog, info, next)
    }
}

/// Maximal runs of consecutive items with the same non-`None` key.
fn runs<K: PartialEq>(line: &[Placed], key: impl Fn(&Placed) -> Option<K>) -> Vec<(K, Range<usize>)> {
    let mut out: Vec<(K, Range<usize>)> = Vec::new();
    for (i, p) in line.iter().enumerate() {
        let Some(k) = key(p) else { continue };
        match out.last_mut() {
            Some((last, r)) if *last == k && r.end == i => r.end = i + 1,
            _ => out.push((k, i..i + 1)),
        }
    }
    out
}

fn span(line: &[Placed], r: &Range<usize>) -> (f32, f32) {
    let last = &line[r.end - 1];
    (line[r.start].x, last.x + last.w)
}

/// A PDF text string, UTF-16BE with a byte-order mark.
fn pdf_string(s: &str) -> String {
    let mut out = String::from("<FEFF");
    for u in s.encode_utf16() {
        let _ = write!(out, "{u:04X}");
    }
    out.push('>');
    out
}

fn to_unicode(used: &BTreeMap<u16, char>) -> String {
    let mut s = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    let pairs: Vec<_> = used.iter().filter(|(g, _)| **g != 0).collect();
    for chunk in pairs.chunks(100) {
        let _ = writeln!(s, "{} beginbfchar", chunk.len());
        for (g, c) in chunk {
            let mut buf = [0u16; 2];
            let hex: String = c.encode_utf16(&mut buf).iter().map(|u| format!("{u:04X}")).collect();
            let _ = writeln!(s, "<{g:04X}> <{hex}>");
        }
        s.push_str("endbfchar\n");
    }
    s.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    s
}

#[derive(Default)]
struct Writer {
    buf: Vec<u8>,
    offsets: BTreeMap<u32, usize>,
}

impl Writer {
    fn obj(&mut self, id: u32, body: &str) {
        self.offsets.insert(id, self.buf.len());
        let _ = write!(self.buf, "{id} 0 obj\n{body}\nendobj\n");
    }

    /// A Flate-compressed stream object; `extra` goes into its dictionary.
    fn stream(&mut self, id: u32, extra: &str, data: &[u8]) {
        let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        let _ = enc.write_all(data);
        let packed = enc.finish().unwrap_or_default();
        self.offsets.insert(id, self.buf.len());
        let _ = write!(self.buf, "{id} 0 obj\n<< /Length {} /Filter /FlateDecode {extra} >>\nstream\n", packed.len());
        self.buf.extend_from_slice(&packed);
        self.buf.extend_from_slice(b"\nendstream\nendobj\n");
    }

    fn finish(mut self, root: u32, info: u32, max_id: u32) -> Vec<u8> {
        let xref = self.buf.len();
        let _ = write!(self.buf, "xref\n0 {}\n0000000000 65535 f \n", max_id + 1);
        for id in 1..=max_id {
            match self.offsets.get(&id) {
                Some(off) => {
                    let _ = writeln!(self.buf, "{off:010} 00000 n ");
                }
                None => self.buf.extend_from_slice(b"0000000000 65535 f \n"),
            }
        }
        let _ = write!(
            self.buf,
            "trailer\n<< /Size {} /Root {root} 0 R /Info {info} 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            max_id + 1
        );
        self.buf
    }
}

// ---------------------------------------------------------------- subsetting

/// Shrink a TrueType font to the glyphs in `keep` (plus the components of
/// composite glyphs). Glyph ids are unchanged, so unused glyphs just become
/// empty; layout tables (GSUB, GPOS, …) and `cmap` are dropped.
fn subset_ttf(data: &[u8], keep: &BTreeSet<u16>) -> Option<Vec<u8>> {
    let u16_at = |o: usize| Some(u16::from_be_bytes(data.get(o..o + 2)?.try_into().ok()?));
    let u32_at = |o: usize| Some(u32::from_be_bytes(data.get(o..o + 4)?.try_into().ok()?));

    let mut tables: BTreeMap<[u8; 4], &[u8]> = BTreeMap::new();
    for i in 0..u16_at(4)? as usize {
        let rec = 12 + 16 * i;
        let tag: [u8; 4] = data.get(rec..rec + 4)?.try_into().ok()?;
        let (off, len) = (u32_at(rec + 8)? as usize, u32_at(rec + 12)? as usize);
        tables.insert(tag, data.get(off..off + len)?);
    }
    let head = *tables.get(b"head")?;
    let glyf = *tables.get(b"glyf")?;
    let loca = *tables.get(b"loca")?;
    let num_glyphs = u16::from_be_bytes(tables.get(b"maxp")?.get(4..6)?.try_into().ok()?) as usize;
    let long = i16::from_be_bytes(head.get(50..52)?.try_into().ok()?) == 1;
    let offset = |g: usize| -> Option<usize> {
        Some(if long {
            u32::from_be_bytes(loca.get(g * 4..g * 4 + 4)?.try_into().ok()?) as usize
        } else {
            u16::from_be_bytes(loca.get(g * 2..g * 2 + 2)?.try_into().ok()?) as usize * 2
        })
    };
    let glyph = |g: usize| -> Option<&[u8]> { glyf.get(offset(g)?..offset(g + 1)?) };

    // Close over composite-glyph components.
    let mut kept: BTreeSet<u16> = keep.clone();
    kept.insert(0);
    let mut todo: Vec<u16> = kept.iter().copied().collect();
    while let Some(g) = todo.pop() {
        let Some(bytes) = glyph(g as usize) else { continue };
        if bytes.len() < 10 || i16::from_be_bytes([bytes[0], bytes[1]]) >= 0 {
            continue;
        }
        let mut p = 10;
        while let (Some(flags), Some(comp)) = (bytes.get(p..p + 2), bytes.get(p + 2..p + 4)) {
            let flags = u16::from_be_bytes([flags[0], flags[1]]);
            let comp = u16::from_be_bytes([comp[0], comp[1]]);
            if kept.insert(comp) {
                todo.push(comp);
            }
            p += 4 + if flags & 0x1 != 0 { 4 } else { 2 };
            p += if flags & 0x8 != 0 {
                2
            } else if flags & 0x40 != 0 {
                4
            } else if flags & 0x80 != 0 {
                8
            } else {
                0
            };
            if flags & 0x20 == 0 {
                break;
            }
        }
    }

    let mut new_glyf = Vec::new();
    let mut new_loca = Vec::with_capacity((num_glyphs + 1) * 4);
    for g in 0..num_glyphs {
        new_loca.extend_from_slice(&(new_glyf.len() as u32).to_be_bytes());
        if kept.contains(&(g as u16)) {
            new_glyf.extend_from_slice(glyph(g)?);
            while new_glyf.len() % 4 != 0 {
                new_glyf.push(0);
            }
        }
    }
    new_loca.extend_from_slice(&(new_glyf.len() as u32).to_be_bytes());

    let mut new_head = head.to_vec();
    new_head[8..12].copy_from_slice(&[0; 4]); // checkSumAdjustment
    new_head[50..52].copy_from_slice(&1i16.to_be_bytes()); // long loca

    let mut out_tables: Vec<([u8; 4], Vec<u8>)> = vec![
        (*b"head", new_head),
        (*b"glyf", new_glyf),
        (*b"loca", new_loca),
    ];
    for tag in [b"hhea", b"hmtx", b"maxp", b"OS/2", b"cvt ", b"fpgm", b"prep"] {
        if let Some(t) = tables.get(tag) {
            out_tables.push((*tag, t.to_vec()));
        }
    }
    if let Some(post) = tables.get(b"post").and_then(|p| p.get(..32)) {
        // Version 3: no glyph names.
        let mut p = post.to_vec();
        p[0..4].copy_from_slice(&0x0003_0000u32.to_be_bytes());
        out_tables.push((*b"post", p));
    }
    out_tables.sort_by_key(|(t, _)| *t);

    let n = out_tables.len() as u16;
    let pow = 1u16 << (15 - n.leading_zeros() as u16).min(15);
    let mut out = Vec::new();
    out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    out.extend_from_slice(&n.to_be_bytes());
    out.extend_from_slice(&(pow * 16).to_be_bytes());
    out.extend_from_slice(&(pow.trailing_zeros() as u16).to_be_bytes());
    out.extend_from_slice(&(n * 16 - pow * 16).to_be_bytes());
    let mut offset = 12 + 16 * out_tables.len();
    let mut body = Vec::new();
    for (tag, t) in &out_tables {
        out.extend_from_slice(tag);
        out.extend_from_slice(&checksum(t).to_be_bytes());
        out.extend_from_slice(&(offset as u32).to_be_bytes());
        out.extend_from_slice(&(t.len() as u32).to_be_bytes());
        body.extend_from_slice(t);
        while body.len() % 4 != 0 {
            body.push(0);
        }
        offset = 12 + 16 * out_tables.len() + body.len();
    }
    out.extend_from_slice(&body);
    Some(out)
}

fn checksum(t: &[u8]) -> u32 {
    t.chunks(4).fold(0u32, |sum, c| {
        let mut w = [0u8; 4];
        w[..c.len()].copy_from_slice(c);
        sum.wrapping_add(u32::from_be_bytes(w))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subset_keeps_glyphs_and_parses() {
        let font = Font::new(Face::Regular);
        let keep: BTreeSet<u16> = "Hazelnut é".chars().map(|c| font.gid(c)).collect();
        let sub = subset_ttf(font.data, &keep).unwrap();
        assert!(sub.len() < font.data.len() / 10, "subset is {} bytes", sub.len());
        let face = ttf_parser::Face::parse(&sub, 0).unwrap();
        for &g in &keep {
            let id = ttf_parser::GlyphId(g);
            assert_eq!(face.glyph_hor_advance(id), font.face.glyph_hor_advance(id));
            assert_eq!(face.glyph_bounding_box(id), font.face.glyph_bounding_box(id));
        }
    }

    #[test]
    fn exports_welcome_notebook() {
        let nb = Notebook::welcome();
        let pdf = export(&nb, None);
        assert!(pdf.starts_with(b"%PDF-1.7"));
        assert!(pdf.ends_with(b"%%EOF\n"));
        let one = export(&nb, Some(1));
        assert!(one.len() < pdf.len());
    }
}

#[cfg(test)]
mod preview {
    /// `HZN_PDF_OUT=/some/dir cargo test pdf::preview -- --ignored`
    #[test]
    #[ignore]
    fn write_preview() {
        let dir = std::env::var("HZN_PDF_OUT").expect("HZN_PDF_OUT");
        let src = std::fs::read_to_string(format!("{dir}/demo.hzn")).unwrap();
        let nb = crate::format::parse(&src).unwrap();
        std::fs::write(format!("{dir}/demo.pdf"), super::export(&nb, None)).unwrap();
        let nb = crate::model::Notebook::welcome();
        std::fs::write(format!("{dir}/welcome.pdf"), super::export(&nb, None)).unwrap();
    }
}
