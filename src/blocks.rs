//! Line-level formatting: headings, lists and callouts.
//!
//! These are written Markdown-style in the chapter text itself, so they need
//! no extra storage and survive copy-paste:
//!
//! ```text
//! # Heading 1      ## Heading 2      ### Heading 3
//! - bullet         - [ ] open task   - [x] done task
//! > [!blue] A callout: a tinted box. Following "> [!] " lines continue it.
//! ```
//!
//! Lists can be nested with leading spaces or tabs, and headings and lists
//! work inside callouts. The editor draws the markers as bullets, checkboxes
//! and boxes; the PDF export does the same.

use std::ops::Range;
use crate::model::Style;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Plain,
    /// Level 1–3.
    Heading(u8),
    Bullet,
    /// Checked or not.
    Task(bool),
}

/// The parsed start of a line. All offsets are char indices into the line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Block {
    pub kind: Kind,
    /// Whether the line is part of a callout (`> [!…] …`).
    pub quoted: bool,
    /// For callouts: the color spec inside `[!…]`, if this line has one
    /// (which also starts a new callout).
    pub spec: Option<(usize, usize)>,
    /// End of the callout prefix (`> [!…] `); 0 outside callouts.
    pub start: usize,
    /// End of the indent (leading spaces / tabs after `start`).
    pub indent: usize,
    /// Nesting level: a tab or two spaces per level.
    pub depth: usize,
    /// End of all syntax at the start of the line (prefix, indent for lists,
    /// marker and the space after it). Content starts here.
    pub marker: usize,
    /// For tasks: offset of the box's inner char (` ` or `x`).
    pub check: usize,
}

impl Block {
    pub fn is_list(&self) -> bool {
        matches!(self.kind, Kind::Bullet | Kind::Task(_))
    }

    /// The color spec text, e.g. `blue` or `#6aa8f0`.
    pub fn spec<'a>(&self, line: &'a str) -> Option<&'a str> {
        self.spec.and_then(|(a, b)| line.get(a..b))
    }
}

pub fn parse(line: &str) -> Block {
    let (quoted, start, spec) = quote_prefix(line);
    // Prefix chars are ASCII, so char and byte offsets agree.
    let mut b = parse_inner(&line[start..]);
    b.quoted = quoted;
    b.spec = spec;
    b.start = start;
    b.indent += start;
    b.marker += start;
    if matches!(b.kind, Kind::Task(_)) {
        b.check += start;
    }
    b
}

/// Characters that can participate in block formatting commands.
pub fn is_syntax_char(c: char) -> bool {
    matches!(c, '#' | '-' | '*' | '>' | '[' | ']' | '!' | '`' | '~')
}

/// Parse block syntax with escaped characters disabled. Replacing ASCII
/// punctuation preserves byte and character offsets into the visible text.
pub fn parse_styled(line: &str, styles: &[Style]) -> Block {
    let syntax: String = line.chars().enumerate().map(|(i, c)| {
        if is_syntax_char(c) && styles.get(i).is_some_and(|s| s.escaped) { '~' } else { c }
    }).collect();
    parse(&syntax)
}

/// Callouts require an explicit `> [!spec]` marker; `> [!]` continues one
/// (or starts a neutral callout). A bare `>` is ordinary text.
fn quote_prefix(line: &str) -> (bool, usize, Option<(usize, usize)>) {
    let Some(rest) = line.strip_prefix("> [!") else {
        return (false, 0, None);
    };
    let Some(end) = rest.find(']') else { return (false, 0, None) };
    let name = &rest[..end];
    let after = &rest[end + 1..];
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '#')
        || !(after.is_empty() || after.starts_with(' '))
    {
        return (false, 0, None);
    }
    let spec = (!name.is_empty()).then_some((4, 4 + end));
    (true, 5 + end + usize::from(after.starts_with(' ')), spec)
}

fn parse_inner(line: &str) -> Block {
    let indent = line.chars().take_while(|c| *c == ' ' || *c == '\t').count();
    let rest = &line[indent..];
    let depth = line[..indent].chars().map(|c| if c == '\t' { 2 } else { 1 }).sum::<usize>() / 2;
    let block = |kind, marker, check| Block { kind, quoted: false, spec: None, start: 0, indent, depth, marker, check };

    if indent == 0 {
        let hashes = rest.chars().take_while(|c| *c == '#').count();
        if (1..=3).contains(&hashes) && rest[hashes..].starts_with(' ') {
            return block(Kind::Heading(hashes as u8), hashes + 1, 0);
        }
    }
    let Some(after) = rest.strip_prefix('-').or_else(|| rest.strip_prefix('*')) else {
        return block(Kind::Plain, 0, 0);
    };
    let (gap, after) = match after.strip_prefix(' ') {
        Some(a) => (1, a),
        None => (0, after),
    };
    for (bx, checked) in [("[ ]", false), ("[x]", true), ("[X]", true)] {
        if let Some(tail) = after.strip_prefix(bx) {
            let check = indent + 1 + gap + 1;
            if tail.is_empty() {
                return block(Kind::Task(checked), indent + 1 + gap + 3, check);
            }
            if tail.starts_with(' ') {
                return block(Kind::Task(checked), indent + 1 + gap + 4, check);
            }
        }
    }
    if gap == 1 {
        return block(Kind::Bullet, indent + 2, 0);
    }
    block(Kind::Plain, 0, 0)
}

/// Each line with the char index it starts at.
pub fn lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut start = 0;
    text.split('\n').map(move |line| {
        let at = start;
        start += line.chars().count() + 1;
        (at, line)
    })
}

/// Parse lines in document context; code always wins over block commands.
pub fn parsed_lines<'a>(text: &'a str, styles: &[Style]) -> Vec<(usize, &'a str, Block)> {
    lines(text).zip(crate::code::lines(text, styles)).map(|((start, line), code)| {
        let block = if code.is_some() { parse("") } else { parse_styled(line, styles.get(start..).unwrap_or(&[])) };
        (start, line, block)
    }).collect()
}

/// Callouts as ranges of line indices, with their color spec.
#[cfg(test)]
fn callouts(text: &str) -> Vec<(Range<usize>, Option<String>)> {
    callouts_styled(text, &[])
}

pub fn callouts_styled(text: &str, styles: &[Style]) -> Vec<(Range<usize>, Option<String>)> {
    let mut out: Vec<(Range<usize>, Option<String>)> = Vec::new();
    let mut open = false;
    for (i, (_, line, b)) in parsed_lines(text, styles).into_iter().enumerate() {
        if !b.quoted {
            open = false;
            continue;
        }
        match out.last_mut() {
            Some((r, _)) if open && b.spec.is_none() => r.end = i + 1,
            _ => out.push((i..i + 1, b.spec(line).map(str::to_owned))),
        }
        open = true;
    }
    out
}

/// A text replacement in char indices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub at: usize,
    pub removed: usize,
    pub insert: String,
}

impl Edit {
    /// Where char position `pos` ends up after this edit.
    pub fn map(&self, pos: usize) -> usize {
        let ins = self.insert.chars().count();
        if pos <= self.at {
            pos
        } else if pos < self.at + self.removed {
            self.at + ins
        } else {
            pos - self.removed + ins
        }
    }
}

/// After a newline was typed at char `nl`: continue the list or callout the
/// previous line belongs to, or end it if that line was empty. Returns the
/// edit and where the caret goes.
#[cfg(test)]
fn on_enter(text: &str, nl: usize) -> Option<(Edit, usize)> {
    on_enter_styled(text, nl, &[])
}

pub fn on_enter_styled(text: &str, nl: usize, styles: &[Style]) -> Option<(Edit, usize)> {
    if crate::code::contains(text, styles, nl) { return None; }
    let chars: Vec<char> = text.chars().collect();
    if chars.get(nl) != Some(&'\n') {
        return None;
    }
    let start = chars[..nl].iter().rposition(|c| *c == '\n').map_or(0, |i| i + 1);
    let line: String = chars[start..nl].iter().collect();
    let b = parse_styled(&line, styles.get(start..).unwrap_or(&[]));
    if !b.is_list() && !b.quoted {
        return None;
    }
    let empty = line.chars().skip(b.marker).all(char::is_whitespace);
    let rest_empty = chars.get(nl + 1).is_none_or(|c| *c == '\n');
    if empty && rest_empty {
        // Enter on an empty item ends the list (keeping the callout, if
        // any); on an empty callout line, it ends the callout.
        let from = if b.is_list() { b.start } else { 0 };
        let edit = Edit { at: start + from, removed: nl + 1 - start - from, insert: String::new() };
        return Some((edit, start + from));
    }
    let quote = if b.quoted { "> [!] " } else { "" };
    let indent: String = line.chars().take(b.indent).skip(b.start).collect();
    let marker = match b.kind {
        Kind::Task(_) => "- [ ] ",
        Kind::Bullet => "- ",
        _ => "",
    };
    let indent = if b.is_list() { indent } else { String::new() };
    let insert = format!("{quote}{indent}{marker}");
    let caret = nl + 1 + insert.chars().count();
    Some((Edit { at: nl + 1, removed: 0, insert }, caret))
}

/// What a toolbar button asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockOp {
    /// 0 = normal text.
    Heading(u8),
    Bullet,
    Task,
}

/// Lines touched by the char range `sel`: (line index, start char, line).
fn touched(text: &str, sel: &Range<usize>) -> Vec<(usize, usize, String)> {
    lines(text)
        .enumerate()
        .filter(|(_, (start, line))| {
            let end = start + line.chars().count();
            // A selection ending right at a line's start doesn't include it.
            *start <= sel.end && end >= sel.start && !(sel.end > sel.start && *start == sel.end)
        })
        .map(|(i, (start, line))| (i, start, line.to_owned()))
        .collect()
}

/// Edits that apply `op` to every line touched by the char range `sel`,
/// in descending order (so each can be applied in turn). Applying an op
/// that all those lines already have turns them back into plain text.
/// Callout prefixes are kept.
#[cfg(test)]
fn set_blocks(text: &str, sel: Range<usize>, op: BlockOp) -> Vec<Edit> {
    set_blocks_styled(text, sel, op, &[])
}

pub fn set_blocks_styled(text: &str, sel: Range<usize>, op: BlockOp, styles: &[Style]) -> Vec<Edit> {
    let touched = touched(text, &sel);
    // With several lines, leave blank ones alone.
    let multi = touched.len() > 1;
    let touched: Vec<_> = touched
        .into_iter()
        .filter(|(_, start, l)| {
            let b = parse_styled(l, styles.get(*start..).unwrap_or(&[]));
            !crate::code::contains(text, styles, *start) && (!multi || !l.chars().skip(b.start).collect::<String>().trim().is_empty())
        })
        .collect();

    let has = |b: &Block| match op {
        BlockOp::Heading(0) => b.kind == Kind::Plain,
        BlockOp::Heading(n) => b.kind == Kind::Heading(n),
        BlockOp::Bullet => b.kind == Kind::Bullet,
        BlockOp::Task => matches!(b.kind, Kind::Task(_)),
    };
    let all = touched.iter().all(|(_, start, l)| has(&parse_styled(l, styles.get(*start..).unwrap_or(&[]))));
    let target = if all { BlockOp::Heading(0) } else { op };

    let mut edits = Vec::new();
    for (_, start, line) in touched.into_iter().rev() {
        let b = parse_styled(&line, styles.get(start..).unwrap_or(&[]));
        let indent: String = line.chars().take(b.indent).skip(b.start).collect();
        let prefix = match target {
            // Plain lines stay as they are (keeping their indentation).
            BlockOp::Heading(0) if b.kind == Kind::Plain => continue,
            BlockOp::Heading(0) if b.is_list() => indent,
            BlockOp::Heading(0) => String::new(),
            BlockOp::Heading(n) => format!("{} ", "#".repeat(n as usize)),
            BlockOp::Bullet => format!("{indent}- "),
            BlockOp::Task if matches!(b.kind, Kind::Task(_)) => continue,
            BlockOp::Task => format!("{indent}- [ ] "),
        };
        // Replace the marker (or, for plain lines, the indent).
        let removed = b.marker.max(b.indent) - b.start;
        let current: String = line.chars().skip(b.start).take(removed).collect();
        if current != prefix {
            edits.push(Edit { at: start + b.start, removed, insert: prefix });
        }
    }
    edits
}

/// Edits that make the lines touched by `sel` a callout colored `spec`
/// (`Some("")` = default color), or remove the callout (`None`). When all
/// of them are in callouts already, those whole callouts are changed.
#[cfg(test)]
fn set_callout(text: &str, sel: Range<usize>, spec: Option<&str>) -> Vec<Edit> {
    set_callout_styled(text, sel, spec, &[])
}

pub fn set_callout_styled(text: &str, sel: Range<usize>, spec: Option<&str>, styles: &[Style]) -> Vec<Edit> {
    let touched = touched(text, &sel);
    let (Some(first), Some(last)) = (touched.first(), touched.last()) else {
        return Vec::new();
    };
    let all: Vec<(usize, &str)> = lines(text).collect();
    let (mut i0, mut i1) = (first.0, last.0);
    let quoted = |i: usize| parse_styled(all[i].1, styles.get(all[i].0..).unwrap_or(&[])).quoted;
    if (i0..=i1).all(quoted) {
        for (r, _) in callouts_styled(text, styles) {
            if r.contains(&i0) {
                i0 = r.start;
            }
            if r.contains(&i1) {
                i1 = r.end - 1;
            }
        }
    }

    let mut edits = Vec::new();
    for i in (i0..=i1).rev() {
        let (start, line) = all[i];
        if crate::code::contains(text, styles, start) { continue; }
        let b = parse_styled(line, styles.get(start..).unwrap_or(&[]));
        let prefix = match spec {
            None => String::new(),
            Some(s) if i == i0 && !s.is_empty() => format!("> [!{s}] "),
            Some(_) => "> [!] ".to_owned(),
        };
        let current: String = line.chars().take(b.start).collect();
        if current != prefix {
            edits.push(Edit { at: start, removed: b.start, insert: prefix });
        }
    }
    edits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_markers() {
        assert_eq!(parse("## Title").kind, Kind::Heading(2));
        assert_eq!(parse("## Title").marker, 3);
        assert_eq!(parse("#### four").kind, Kind::Plain);
        assert_eq!(parse("#nospace").kind, Kind::Plain);
        assert_eq!(parse("- item"), Block { kind: Kind::Bullet, quoted: false, spec: None, start: 0, indent: 0, depth: 0, marker: 2, check: 0 });
        assert_eq!(parse("  * item").marker, 4);
        assert_eq!(parse("-item").kind, Kind::Plain);
        assert_eq!(parse("- [ ] task"), Block { kind: Kind::Task(false), quoted: false, spec: None, start: 0, indent: 0, depth: 0, marker: 6, check: 3 });
        assert_eq!(parse("-[x] done"), Block { kind: Kind::Task(true), quoted: false, spec: None, start: 0, indent: 0, depth: 0, marker: 5, check: 2 });
        assert_eq!(parse("\t- [X]").kind, Kind::Task(true));
        assert_eq!(parse("- [ ]x").kind, Kind::Bullet);
    }

    #[test]
    fn enter_continues_and_ends_lists() {
        let (e, caret) = on_enter("  - [x] a\n", 9).unwrap();
        assert_eq!(e, Edit { at: 10, removed: 0, insert: "  - [ ] ".into() });
        assert_eq!(caret, 18);
        let (e, caret) = on_enter("x\n- \n", 4).unwrap();
        assert_eq!(e, Edit { at: 2, removed: 3, insert: String::new() });
        assert_eq!(caret, 2);
        assert!(on_enter("# h\n", 3).is_none());
        assert!(on_enter("> ordinary text\n", 15).is_none());
        assert!(callouts("> plain\n> [!broken\n> [!blue]missing-space").is_empty());
    }

    fn apply(text: &str, edits: &[Edit]) -> String {
        let mut chars: Vec<char> = text.chars().collect();
        for e in edits {
            chars.splice(e.at..e.at + e.removed, e.insert.chars());
        }
        chars.into_iter().collect()
    }

    #[test]
    fn toggles_blocks() {
        let t = "one\n\n  two\nthree";
        let all = 0..t.chars().count();
        let bullets = apply(t, &set_blocks(t, all.clone(), BlockOp::Bullet));
        assert_eq!(bullets, "- one\n\n  - two\n- three");
        let n = bullets.chars().count();
        assert_eq!(apply(&bullets, &set_blocks(&bullets, 0..n, BlockOp::Bullet)), "one\n\n  two\nthree");
        let tasks = apply(&bullets, &set_blocks(&bullets, 0..n, BlockOp::Task));
        assert_eq!(tasks, "- [ ] one\n\n  - [ ] two\n- [ ] three");
        assert_eq!(apply(t, &set_blocks(t, 1..1, BlockOp::Heading(2))), "## one\n\n  two\nthree");
        let h = "## one";
        assert_eq!(apply(h, &set_blocks(h, 0..0, BlockOp::Heading(2))), "one");
        assert_eq!(apply(h, &set_blocks(h, 0..0, BlockOp::Heading(1))), "# one");
        // Normal text: headings and lists lose their markers; plain lines keep their indent.
        let m = "## a\n    code\n  - b";
        let n = m.chars().count();
        assert_eq!(apply(m, &set_blocks(m, 0..n, BlockOp::Heading(0))), "a\n    code\n  b");
    }

    #[test]
    fn callouts_parse_and_group() {
        let b = parse("> [!blue] - [ ] hi");
        assert!(b.quoted);
        assert_eq!(b.spec("> [!blue] - [ ] hi"), Some("blue"));
        assert_eq!((b.start, b.kind, b.marker, b.check), (10, Kind::Task(false), 16, 13));
        assert_eq!(parse(">## h").kind, Kind::Plain);
        assert!(!parse("> ordinary text").quoted);
        assert!(!parse("> [!x]y").quoted);
        assert_eq!(parse("> [!x]y").spec, None);
        let t = "> [!red] a\n> [!] b\n> [!#6aa8f0] c\nplain\n> [!] d";
        let c = callouts(t);
        assert_eq!(c, vec![(0..2, Some("red".into())), (2..3, Some("#6aa8f0".into())), (4..5, None)]);
    }

    #[test]
    fn callout_edits() {
        let t = "one\ntwo\nthree";
        let made = apply(t, &set_callout(t, 0..5, Some("green")));
        assert_eq!(made, "> [!green] one\n> [!] two\nthree");
        // Caret inside: recolor the whole callout.
        assert_eq!(apply(&made, &set_callout(&made, 16..16, Some("blue"))), "> [!blue] one\n> [!] two\nthree");
        assert_eq!(apply(&made, &set_callout(&made, 16..16, None)), "one\ntwo\nthree");
        // Lists keep their callout prefix.
        let l = "> [!red] a";
        assert_eq!(apply(l, &set_blocks(l, 0..0, BlockOp::Bullet)), "> [!red] - a");
        // Enter continues the callout, and ends it on an empty line.
        let (e, _) = on_enter("> [!red] - a\n", 12).unwrap();
        assert_eq!(e.insert, "> [!] - ");
        let (e, caret) = on_enter("x\n> [!] \n", 8).unwrap();
        assert_eq!((e.at, e.removed, caret), (2, 7, 2));
    }
}
