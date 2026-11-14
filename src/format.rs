//! The `.hzn` Hazelnut notebook format.
//!
//! A small, line-oriented, human-readable text format. See `FORMAT.md`.
//!
//! ```text
//! %hazelnut 1
//! %title My notebook
//!
//! %chapter Groceries
//! Buy {b|milk} and {i,fg=#ef6f6c|tomatoes}.
//! ```

use crate::model::{Chapter, LineStyle, Notebook, Style};
use eframe::egui::Color32;
use std::fmt::Write as _;

pub const VERSION: u32 = 1;
pub const EXTENSION: &str = "hzn";

#[derive(Debug)]
pub enum ParseError {
    NotANotebook,
    UnsupportedVersion(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::NotANotebook => write!(f, "not a Hazelnut notebook (missing %hazelnut header)"),
            ParseError::UnsupportedVersion(v) => write!(f, "unsupported notebook version {v}"),
        }
    }
}

impl std::error::Error for ParseError {}

// ---------------------------------------------------------------- writing

pub fn serialize(nb: &Notebook) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "%hazelnut {VERSION}");
    let _ = writeln!(out, "%title {}", one_line(&nb.title));
    for ch in &nb.chapters {
        out.push('\n');
        let _ = writeln!(out, "%chapter {}", one_line(&ch.title));
        write_body(&mut out, &ch.text, &ch.styles);
    }
    out
}

fn one_line(s: &str) -> String {
    s.replace(['\n', '\r'], " ").trim().to_owned()
}

fn write_body(out: &mut String, text: &str, styles: &[Style]) {
    // Trailing blank lines are not significant in the format.
    let body = text.trim_end_matches(['\n', ' ', '\t']);
    if body.is_empty() {
        return;
    }
    let mut idx = 0usize; // char index into `styles`
    for line in body.split('\n') {
        let mut rendered = String::new();
        let mut run = String::new();
        let mut run_style: Option<Style> = None;
        for c in line.chars() {
            let st = styles.get(idx).copied().unwrap_or_default();
            idx += 1;
            if run_style != Some(st) {
                flush_run(&mut rendered, &run, run_style);
                run.clear();
                run_style = Some(st);
            }
            run.push(c);
        }
        flush_run(&mut rendered, &run, run_style);
        idx += 1; // the '\n'

        if rendered.starts_with('%') {
            out.push('\\');
        }
        out.push_str(&rendered);
        out.push('\n');
    }
}

fn flush_run(out: &mut String, run: &str, style: Option<Style>) {
    if run.is_empty() {
        return;
    }
    let style = style.unwrap_or_default();
    if style.is_plain() {
        escape_into(out, run);
    } else {
        out.push('{');
        out.push_str(&style_spec(&style));
        out.push('|');
        escape_into(out, run);
        out.push('}');
    }
}

fn escape_into(out: &mut String, s: &str) {
    for c in s.chars() {
        if matches!(c, '\\' | '{' | '}') {
            out.push('\\');
        }
        out.push(c);
    }
}

fn style_spec(s: &Style) -> String {
    let mut parts: Vec<String> = Vec::new();
    if s.bold {
        parts.push("b".into());
    }
    if s.italic {
        parts.push("i".into());
    }
    match s.underline {
        Some(LineStyle::Solid) => parts.push("u".into()),
        Some(kind) => parts.push(format!("u={}", kind.key())),
        None => {}
    }
    if s.strike {
        parts.push("s".into());
    }
    if let Some(c) = s.fg {
        parts.push(format!("fg={}", color_hex(c)));
    }
    if let Some(c) = s.bg {
        parts.push(format!("bg={}", color_hex(c)));
    }
    parts.join(",")
}

pub fn color_hex(c: Color32) -> String {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

pub fn parse_hex(s: &str) -> Option<Color32> {
    let h = s.strip_prefix('#')?;
    let byte = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
    match h.len() {
        6 => Some(Color32::from_rgb(byte(0)?, byte(2)?, byte(4)?)),
        8 => Some(Color32::from_rgba_unmultiplied(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
        _ => None,
    }
}

// ---------------------------------------------------------------- reading

pub fn parse(src: &str) -> Result<Notebook, ParseError> {
    let mut lines = src.lines().peekable();

    // Header: first non-blank line must be `%hazelnut <version>`.
    while lines.peek().is_some_and(|l| l.trim().is_empty()) {
        lines.next();
    }
    let header = lines.next().ok_or(ParseError::NotANotebook)?;
    let version = header
        .strip_prefix("%hazelnut")
        .ok_or(ParseError::NotANotebook)?
        .trim();
    match version.parse::<u32>() {
        Ok(v) if v <= VERSION => {}
        _ => return Err(ParseError::UnsupportedVersion(version.to_owned())),
    }

    let mut nb = Notebook {
        title: String::from("Untitled notebook"),
        chapters: Vec::new(),
    };
    let mut current: Option<(String, Vec<&str>)> = None;

    for line in lines {
        if line.starts_with("%%") {
            continue; // comment
        }
        if let Some(directive) = line.strip_prefix('%') {
            let (name, arg) = directive.split_once(' ').unwrap_or((directive, ""));
            match name {
                "title" => nb.title = arg.trim().to_owned(),
                "chapter" => {
                    if let Some((t, body)) = current.take() {
                        nb.chapters.push(build_chapter(t, &body));
                    }
                    current = Some((arg.trim().to_owned(), Vec::new()));
                }
                _ => {} // unknown directives are ignored (forward compatibility)
            }
            continue;
        }
        match &mut current {
            Some((_, body)) => body.push(line),
            None if !line.trim().is_empty() => {
                // Text before any chapter: put it in an implicit one.
                current = Some((String::from("Untitled"), vec![line]));
            }
            None => {}
        }
    }
    if let Some((t, body)) = current.take() {
        nb.chapters.push(build_chapter(t, &body));
    }
    if nb.chapters.is_empty() {
        nb.chapters.push(Chapter::new("Untitled"));
    }
    Ok(nb)
}

fn build_chapter(title: String, lines: &[&str]) -> Chapter {
    let mut end = lines.len();
    while end > 0 && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    let mut text = String::new();
    let mut styles = Vec::new();
    for (i, line) in lines[..end].iter().enumerate() {
        if i > 0 {
            let st = styles.last().copied().unwrap_or_default();
            text.push('\n');
            styles.push(st);
        }
        parse_line(line, &mut text, &mut styles);
    }
    Chapter::with_content(title, text, styles)
}

fn parse_line(line: &str, text: &mut String, styles: &mut Vec<Style>) {
    let mut push = |c: char, s: Style| {
        text.push(c);
        styles.push(s);
    };
    let mut chars = line.chars();
    let mut span: Option<Style> = None;
    while let Some(c) = chars.next() {
        let cur = span.unwrap_or_default();
        match c {
            '\\' => push(chars.next().unwrap_or('\\'), cur),
            '{' if span.is_none() => {
                // Look ahead for `spec|`; otherwise treat `{` literally.
                let mut look = chars.clone();
                let mut spec = String::new();
                let mut found = false;
                for d in look.by_ref() {
                    match d {
                        '|' => {
                            found = true;
                            break;
                        }
                        '{' | '}' | '\\' => break,
                        _ => spec.push(d),
                    }
                }
                if found {
                    chars = look;
                    span = Some(parse_spec(&spec));
                } else {
                    push('{', cur);
                }
            }
            '}' if span.is_some() => span = None,
            _ => push(c, cur),
        }
    }
}

fn parse_spec(spec: &str) -> Style {
    let mut s = Style::default();
    for tok in spec.split(',').map(str::trim) {
        match tok {
            "b" => s.bold = true,
            "i" => s.italic = true,
            "u" => s.underline = Some(LineStyle::Solid),
            "s" => s.strike = true,
            _ => {
                if let Some(v) = tok.strip_prefix("u=") {
                    s.underline = Some(LineStyle::from_key(v.trim()).unwrap_or(LineStyle::Solid));
                } else if let Some(v) = tok.strip_prefix("fg=") {
                    s.fg = parse_hex(v.trim());
                } else if let Some(v) = tok.strip_prefix("bg=") {
                    s.bg = parse_hex(v.trim());
                }
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chapter(text: &str, styles: Vec<Style>) -> Chapter {
        Chapter::with_content("T", text.into(), styles)
    }

    #[test]
    fn roundtrip_styles_and_escapes() {
        let bold = Style { bold: true, ..Default::default() };
        let fancy = Style {
            italic: true,
            underline: Some(LineStyle::Wavy),
            strike: true,
            fg: Some(Color32::from_rgb(0xef, 0x6f, 0x6c)),
            bg: Some(Color32::from_rgb(0x23, 0x39, 0x5c)),
            ..Default::default()
        };
        let text = "%not a directive {x} \\ |\nhello world\n\nend";
        let mut styles = vec![Style::default(); text.chars().count()];
        for s in &mut styles[31..36] {
            *s = bold;
        }
        for s in &mut styles[38..41] {
            *s = fancy;
        }
        let nb = Notebook {
            title: "Notes".into(),
            chapters: vec![chapter(text, styles.clone()), chapter("", vec![])],
        };
        let out = serialize(&nb);
        let back = parse(&out).unwrap();
        assert_eq!(back.title, "Notes");
        assert_eq!(back.chapters.len(), 2);
        assert_eq!(back.chapters[0].text, text);
        // Newline styles are not stored; compare visible characters only.
        let visible = |c: &Chapter| -> Vec<Style> {
            c.text.chars().zip(&c.styles).filter(|(ch, _)| *ch != '\n').map(|(_, s)| *s).collect()
        };
        assert_eq!(visible(&back.chapters[0]), visible(&nb.chapters[0]));
        assert_eq!(back.chapters[1].text, "");
    }

    #[test]
    fn lenient_parsing() {
        let nb = parse("%hazelnut 1\n%chapter A\n{b|bold} {oops} \\{x\\}\n%future thing\n").unwrap();
        assert_eq!(nb.chapters[0].text, "bold {oops} {x}");
        assert!(nb.chapters[0].styles[0].bold);
        assert!(!nb.chapters[0].styles[5].bold);
        assert!(parse("hello").is_err());
    }
}

#[cfg(test)]
mod welcome_tests {
    use super::*;

    #[test]
    fn welcome_notebook_roundtrips() {
        let nb = parse(include_str!("../assets/welcome.hzn")).unwrap();
        assert_eq!(nb.chapters.len(), 3);
        let again = parse(&serialize(&nb)).unwrap();
        for (a, b) in nb.chapters.iter().zip(&again.chapters) {
            assert_eq!(a.title, b.title);
            assert_eq!(a.text, b.text);
            assert_eq!(a.styles, b.styles);
        }
    }
}
