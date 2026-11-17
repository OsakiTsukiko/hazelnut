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

use crate::model::{Category, Chapter, LineStyle, Notebook, Style};
use eframe::egui::Color32;
use std::fmt::Write as _;

pub const VERSION: u32 = 1;
pub const EXTENSION: &str = "hzn";
/// Color for categories whose `%category` line has none.
const DEFAULT_CATEGORY_COLOR: Color32 = Color32::from_rgb(0x8c, 0x88, 0x80);

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
    for cat in &nb.categories {
        let _ = writeln!(out, "%category {} {}", color_hex(cat.color), one_line(&cat.name));
    }
    for ch in &nb.chapters {
        out.push('\n');
        let _ = writeln!(out, "%chapter {}", one_line(&ch.title));
        if let Some(cat) = nb.category(ch.category) {
            let _ = writeln!(out, "%tag {}", one_line(&cat.name));
        }
        write_body(&mut out, &ch.text, &ch.styles);
    }
    out
}

/// A chapter body as `.hzn` markup: inline styles as `{spec|text}`, with
/// escapes (the code view shows and edits this).
pub fn body_markup(text: &str, styles: &[Style]) -> String {
    let mut out = String::new();
    write_body(&mut out, text, styles);
    out.strip_suffix('\n').map(str::to_owned).unwrap_or(out)
}

/// Parse body markup (see [`body_markup`]) back into text and styles.
pub fn parse_body(src: &str) -> (String, Vec<Style>) {
    let lines: Vec<&str> = src.split('\n').collect();
    let ch = build_chapter(String::new(), &lines);
    (ch.text, ch.styles)
}

fn one_line(s: &str) -> String {
    s.replace(['\n', '\r'], " ").trim().to_owned()
}

fn write_body(out: &mut String, text: &str, styles: &[Style]) {
    // Trailing blank lines are not significant in the format.
    let code = crate::code::lines(text, styles);
    let body = if code.last().is_some_and(Option::is_some) { text } else { text.trim_end_matches(['\n', ' ', '\t']) };
    if body.is_empty() {
        return;
    }
    let mut fences = crate::code::Fences::default();
    let mut idx = 0usize; // char index into `styles`
    for line in body.split('\n') {
        if fences.next(line, styles.get(idx..).unwrap_or(&[])).is_some() {
            out.push_str(line);
            out.push('\n');
            idx += line.chars().count() + 1;
            continue;
        }
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
    let mut style = style.unwrap_or_default();
    let escaped = style.escaped;
    style.escaped = false;
    if style.is_plain() {
        escape_into(out, run, escaped);
    } else {
        out.push('{');
        out.push_str(&style_spec(&style));
        out.push('|');
        escape_into(out, run, escaped);
        out.push('}');
    }
}

fn escape_into(out: &mut String, s: &str, escaped: bool) {
    for c in s.chars() {
        if matches!(c, '\\' | '{' | '}') || (escaped && crate::blocks::is_syntax_char(c)) {
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
        categories: Vec::new(),
    };
    let mut current: Option<(String, Vec<&str>)> = None;
    // Category name per chapter, resolved once all categories are known.
    let mut tags: Vec<Option<String>> = Vec::new();
    let mut tag: Option<String> = None;

    let mut fences = crate::code::Fences::default();
    for line in lines {
        if fences.next(line, &[]).is_some() {
            if let Some((_, body)) = &mut current {
                body.push(line);
            } else {
                current = Some((String::from("Untitled"), vec![line]));
            }
            continue;
        }
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
                        tags.push(tag.take());
                    }
                    tag = None;
                    current = Some((arg.trim().to_owned(), Vec::new()));
                }
                "category" => {
                    let arg = arg.trim();
                    let (color, name) = match arg.split_once(' ') {
                        Some((c, n)) if parse_hex(c).is_some() => (parse_hex(c), n.trim()),
                        _ => (None, arg),
                    };
                    if !name.is_empty() && !nb.categories.iter().any(|c| c.name == name) {
                        nb.categories.push(Category::new(name, color.unwrap_or(DEFAULT_CATEGORY_COLOR)));
                    }
                }
                "tag" if current.is_some() => {
                    let name = arg.trim();
                    tag = (!name.is_empty()).then(|| name.to_owned());
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
        tags.push(tag.take());
    }
    for (ch, tag) in nb.chapters.iter_mut().zip(tags) {
        let Some(name) = tag else { continue };
        let id = match nb.categories.iter().find(|c| c.name == name) {
            Some(c) => c.id,
            None => {
                // Used but never declared: create it with a default color.
                let c = Category::new(name, DEFAULT_CATEGORY_COLOR);
                let id = c.id;
                nb.categories.push(c);
                id
            }
        };
        ch.category = Some(id);
    }
    if nb.chapters.is_empty() {
        nb.chapters.push(Chapter::new("Untitled"));
    }
    nb.normalize();
    Ok(nb)
}

fn build_chapter(title: String, lines: &[&str]) -> Chapter {
    let mut end = lines.len();
    let mut state = crate::code::Fences::default();
    let ends_in_code = lines.iter().map(|line| state.next(line, &[])).last().flatten().is_some();
    if !ends_in_code {
        while end > 0 && lines[end - 1].trim().is_empty() { end -= 1; }
    }
    let mut text = String::new();
    let mut styles = Vec::new();
    let mut fences = crate::code::Fences::default();
    for (i, line) in lines[..end].iter().enumerate() {
        if i > 0 {
            let st = styles.last().copied().unwrap_or_default();
            text.push('\n');
            styles.push(st);
        }
        if fences.next(line, &[]).is_some() {
            text.push_str(line);
            styles.extend(std::iter::repeat_n(Style::default(), line.chars().count()));
        } else {
            parse_line(line, &mut text, &mut styles);
        }
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
            '\\' => {
                let c = chars.next().unwrap_or('\\');
                push(c, Style { escaped: crate::blocks::is_syntax_char(c), ..cur });
            }
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
            // Compatibility with earlier notes; saving uses backslash escapes.
            "literal" => s.escaped = true,
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
            categories: Vec::new(),
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
mod markup_tests {
    use super::*;

    #[test]
    fn body_markup_roundtrips() {
        let nb = parse(include_str!("../assets/welcome.hzn")).unwrap();
        for ch in &nb.chapters {
            let src = body_markup(&ch.text, &ch.styles);
            let (text, styles) = parse_body(&src);
            assert_eq!(text, ch.text);
            let visible = |t: &str, s: &[Style]| -> Vec<Style> {
                t.chars().zip(s).filter(|(c, _)| *c != '\n').map(|(_, s)| *s).collect()
            };
            assert_eq!(visible(&text, &styles), visible(&ch.text, &ch.styles));
        }
        assert_eq!(parse_body("%x {b|y}").0, "%x y");
    }
}

#[cfg(test)]
mod category_tests {
    use super::*;

    #[test]
    fn categories_roundtrip_and_group() {
        let src = "%hazelnut 1\n%category #6aa8f0 Ideas\n%category Plain\n\
                   %chapter A\n%tag Ideas\nbody\n%chapter B\nfree\n%chapter C\n%tag Undeclared\n";
        let nb = parse(src).unwrap();
        assert_eq!(nb.categories.len(), 3);
        assert_eq!(nb.categories[0].color, Color32::from_rgb(0x6a, 0xa8, 0xf0));
        assert_eq!(nb.categories[1].name, "Plain");
        // Uncategorized first, then by category order.
        let titles: Vec<_> = nb.chapters.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles, ["B", "A", "C"]);
        assert_eq!(nb.category(nb.chapters[1].category).unwrap().name, "Ideas");
        assert_eq!(nb.chapters[1].text, "body");

        let again = parse(&serialize(&nb)).unwrap();
        let names = |n: &Notebook| -> Vec<Option<String>> {
            n.chapters.iter().map(|c| n.category(c.category).map(|k| k.name.clone())).collect()
        };
        assert_eq!(names(&again), names(&nb));
        assert_eq!(again.categories.len(), 3);
        assert_eq!(again.categories[0].color, nb.categories[0].color);
    }
}

#[cfg(test)]
mod welcome_tests {
    use super::*;

    #[test]
    fn welcome_notebook_roundtrips() {
        let nb = parse(include_str!("../assets/welcome.hzn")).unwrap();
        assert_eq!(nb.chapters.len(), 4);
        let again = parse(&serialize(&nb)).unwrap();
        for (a, b) in nb.chapters.iter().zip(&again.chapters) {
            assert_eq!(a.title, b.title);
            assert_eq!(a.text, b.text);
            assert_eq!(a.styles, b.styles);
        }
    }
}


#[cfg(test)]
mod escape_tests {
    use super::*;
    use crate::{blocks, edit};

    #[test]
    fn typing_escapes_match_manual_code_and_leave_ordinary_characters_alone() {
        let text = "# Title\n- item\n> [!blue] note\nordinary words é";
        let change = edit::Change { at: 0, removed: 0, inserted: text.chars().count() };
        let styles = edit::apply_typing(&[], change, None, text, true);
        assert_eq!(body_markup(text, &styles), r"\# Title
\- item
\> \[\!blue\] note
ordinary words é");
        let markup = body_markup(text, &styles);
        let (decoded, decoded_styles) = parse_body(&markup);
        assert_eq!(decoded, text);
        assert_eq!(decoded_styles, styles);
        for (start, line) in blocks::lines(&decoded) {
            let b = blocks::parse_styled(line, &decoded_styles[start..]);
            assert_eq!(b.kind, blocks::Kind::Plain);
            assert!(!b.quoted);
        }
        assert!(text.chars().zip(&styles).all(|(c, s)| s.escaped == blocks::is_syntax_char(c)));
        let (text, styles) = parse_body(r"\# example");
        assert_eq!(blocks::parse_styled(&text, &styles).kind, blocks::Kind::Plain);
        assert_eq!(body_markup(&text, &styles), "\\# example");
    }

    #[test]
    fn escaped_characters_can_still_have_advanced_styles() {
        let (text, styles) = parse_body(r"{b,fg=#ff0000|\# example}");
        assert_eq!(text, "# example");
        assert!(styles[0].escaped);
        assert!(styles.iter().all(|s| s.bold && s.fg == Some(Color32::RED)));
        let encoded = body_markup(&text, &styles);
        assert!(!encoded.contains("literal"));
        assert!(encoded.contains(r"\#"));
        assert_eq!(parse_body(&encoded), (text, styles));
    }

    #[test]
    fn literal_toggle_does_not_change_spaces_or_existing_markers() {
        let text = "# heading";
        let original = vec![Style::default()];
        let styles = edit::apply_typing(&original,
            edit::Change { at: 1, removed: 0, inserted: 8 }, None, text, true);
        assert!(styles.iter().all(|s| !s.escaped));
        assert_eq!(blocks::parse_styled(text, &styles).kind, blocks::Kind::Heading(1));
    }
}
