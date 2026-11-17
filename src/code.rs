//! Fenced code and shared syntax coloring for the editor and PDF exporter.
use crate::{blocks, model::Style};
use eframe::egui::Color32;
use std::{
    cell::RefCell,
    ops::Range,
    sync::{Arc, OnceLock},
};
use syntect::{
    easy::HighlightLines,
    highlighting::{Theme, ThemeSet},
    parsing::SyntaxSet,
};

#[derive(Clone, Debug)]
pub struct Line {
    pub fence: bool,
    pub language: String,
}

#[derive(Default)]
pub struct Fences {
    open: Option<(char, usize, String)>,
}

impl Fences {
    /// A fenced line or code body, otherwise ordinary notebook text.
    pub fn next(&mut self, line: &str, styles: &[Style]) -> Option<Line> {
        let trimmed = line.trim_start_matches(' ');
        let indent = line.len() - trimmed.len();
        let marker = trimmed.chars().next().unwrap_or(' ');
        let count = trimmed.chars().take_while(|c| *c == marker).count();
        let escaped = styles.iter().skip(indent).take(count).any(|s| s.escaped);
        if let Some((ch, size, language)) = &self.open {
            let language = language.clone();
            let closing = indent <= 3
                && marker == *ch
                && count >= *size
                && !escaped
                && trimmed[count..].trim().is_empty();
            if closing {
                self.open = None;
            }
            return Some(Line {
                fence: closing,
                language,
            });
        }
        if indent > 3 || !matches!(marker, '`' | '~') || count < 3 || escaped {
            return None;
        }
        let language = trimmed[count..].trim();
        if !language
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+' | '#' | '.'))
        {
            return None;
        }
        self.open = Some((marker, count, language.to_owned()));
        Some(Line {
            fence: true,
            language: language.to_owned(),
        })
    }
}

pub fn lines(text: &str, styles: &[Style]) -> Vec<Option<Line>> {
    let mut state = Fences::default();
    blocks::lines(text)
        .map(|(start, line)| state.next(line, styles.get(start..).unwrap_or(&[])))
        .collect()
}

pub fn contains(text: &str, styles: &[Style], at: usize) -> bool {
    blocks::lines(text)
        .zip(lines(text, styles))
        .any(|((start, line), code)| {
            code.is_some() && (start..=start + line.chars().count()).contains(&at)
        })
}

pub struct Document {
    pub lines: Vec<Option<Line>>,
    /// A foreground color per character, including newline positions.
    pub colors: Vec<Color32>,
    pub ranges: Vec<Range<usize>>,
}

type Cache = Option<(String, Vec<bool>, Arc<Document>)>;
thread_local! { static CACHE: RefCell<Cache> = const { RefCell::new(None) }; }

pub fn document(text: &str, styles: &[Style]) -> Arc<Document> {
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let escapes: Vec<bool> = styles.iter().map(|s| s.escaped).collect();
        if let Some((previous, flags, result)) = cache.as_ref()
            && previous == text
            && *flags == escapes
        {
            return result.clone();
        }
        let result = Arc::new(highlight(text, styles));
        *cache = Some((text.to_owned(), escapes, result.clone()));
        result
    })
}

fn highlight(text: &str, styles: &[Style]) -> Document {
    static RESOURCES: OnceLock<(SyntaxSet, Theme)> = OnceLock::new();
    let (syntaxes, theme) = RESOURCES.get_or_init(|| {
        let syntaxes = SyntaxSet::load_defaults_newlines();
        let mut themes = ThemeSet::load_defaults();
        (
            syntaxes,
            themes
                .themes
                .remove("base16-ocean.dark")
                .expect("bundled theme"),
        )
    });
    let lines = lines(text, styles);
    let mut colors = vec![crate::theme::TEXT; text.chars().count()];
    let mut highlighter = None;
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for (li, ((start, line), code)) in blocks::lines(text).zip(&lines).enumerate() {
        let Some(code) = code else {
            highlighter = None;
            continue;
        };
        if highlighter.is_none() {
            ranges.push(li..li + 1);
        } else if let Some(range) = ranges.last_mut() {
            range.end = li + 1;
        }
        if code.fence {
            if highlighter.is_some() {
                highlighter = None;
            } else {
                let token = match code.language.to_ascii_lowercase().as_str() {
                    "javascript" => "js",
                    "typescript" => "ts",
                    "python" => "py",
                    "rust" => "rs",
                    "shell" | "bash" => "sh",
                    "c++" => "cpp",
                    "csharp" | "c#" => "cs",
                    _ => &code.language,
                };
                let syntax = syntaxes
                    .find_syntax_by_token(token)
                    .or_else(|| {
                        matches!(token, "ts" | "tsx" | "typescript")
                            .then(|| syntaxes.find_syntax_by_token("js"))
                            .flatten()
                    })
                    .unwrap_or_else(|| syntaxes.find_syntax_plain_text());
                highlighter = Some(HighlightLines::new(syntax, theme));
            }
            continue;
        }
        if let Some(highlighter) = &mut highlighter {
            let source = format!("{line}\n");
            if let Ok(tokens) = highlighter.highlight_line(&source, syntaxes) {
                let mut at = start;
                for (style, token) in tokens {
                    let c = style.foreground;
                    for _ in token.chars() {
                        if let Some(color) = colors.get_mut(at) {
                            *color = Color32::from_rgb(c.r, c.g, c.b);
                        }
                        at += 1;
                    }
                }
            }
        }
    }
    Document {
        lines,
        colors,
        ranges,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{edit, format, model::Notebook, pdf, render};
    use eframe::egui::{self, FontFamily};
    use std::collections::HashSet;

    #[test]
    fn fenced_code_is_raw_and_does_not_parse_notebook_commands() {
        let source = "```rust\nfn main() {\n    let s = r\"{b|not bold} \\n é\";\n%chapter Not a chapter\n%% not a comment\n# not a heading\n- [ ] not a task\n> [!red] not a callout\n}\n```\n# Actual heading";
        let (text, styles) = format::parse_body(source);
        assert_eq!(text, source);
        assert!(styles.iter().all(|s| *s == Style::default()));
        assert_eq!(format::body_markup(&text, &styles), source);
        let nb = format::parse(&format!("%hazelnut 1\n%chapter Code\n{source}\n")).unwrap();
        assert_eq!(nb.chapters.len(), 1);
        assert_eq!(nb.chapters[0].text, source);
        let again = format::parse(&format::serialize(&nb)).unwrap();
        assert_eq!(again.chapters[0].text, source);
        let parsed = blocks::parsed_lines(&text, &styles);
        assert!(
            parsed[..10]
                .iter()
                .all(|(_, _, b)| b.kind == blocks::Kind::Plain && !b.quoted)
        );
        assert_eq!(parsed.last().unwrap().2.kind, blocks::Kind::Heading(1));
        assert!(blocks::callouts_styled(&text, &styles).is_empty());
        let nl = text
            .chars()
            .enumerate()
            .filter(|(_, c)| *c == '\n')
            .nth(6)
            .unwrap()
            .0;
        assert!(blocks::on_enter_styled(&text, nl, &styles).is_none());
    }

    #[test]
    fn escaped_fences_stay_literal_and_fence_lengths_are_respected() {
        let source = "```rust\nlet x = 1;\n```";
        let styles = edit::apply_typing(
            &[],
            edit::Change {
                at: 0,
                removed: 0,
                inserted: source.chars().count(),
            },
            None,
            source,
            true,
        );
        assert!(lines(source, &styles).iter().all(Option::is_none));
        let markup = format::body_markup(source, &styles);
        assert!(markup.starts_with(r"\`\`\`rust"));
        let (text, saved) = format::parse_body(&markup);
        assert!(lines(&text, &saved).iter().all(Option::is_none));
        let text = "````python\n```\nx = 1\n````\noutside";
        let doc = document(text, &[]);
        assert_eq!(doc.ranges, vec![0..4]);
        assert!(!doc.lines[1].as_ref().unwrap().fence);
        assert!(doc.lines[4].is_none());
        assert_eq!(format::parse_body("```\n   \n\t").0, "```\n   \n\t");
    }

    #[test]
    fn syntax_colors_are_language_aware_and_render_as_monospace() {
        for (language, code) in [
            ("rust", "fn main() { let x = 42; println!(\"hello\"); }"),
            ("python", "def hello():\n    return \"world\" # comment"),
            ("js", "const value = 42; // comment"),
            ("json", "{\"name\": 42, \"enabled\": true}"),
        ] {
            let text = format!("```{language}\n{code}\n```");
            let doc = document(&text, &[]);
            let colors: HashSet<_> = doc.colors.iter().collect();
            assert!(colors.len() >= 3, "{language} should have colored tokens");
            let ctx = egui::Context::default();
            crate::theme::install(&ctx);
            let _ = ctx.run(Default::default(), |ctx| {
                let job = render::body_job(&text, &[], 300.0);
                assert!(
                    job.sections
                        .iter()
                        .filter(|s| !job.text[s.byte_range.clone()]
                            .chars()
                            .all(char::is_whitespace))
                        .all(|s| s.format.font_id.family == FontFamily::Monospace)
                );
                let galley = ctx.fonts_mut(|f| f.layout_job(job));
                assert_eq!(galley.job.text, text);
                assert_eq!(
                    render::code_shapes(&galley, egui::Pos2::ZERO, &text, &[], 300.0).len(),
                    1
                );
            });
        }
        let doc = document("```unknown-language\nx = 42\n```", &[]);
        assert!(doc.lines[1].is_some());
    }

    #[test]
    fn pdf_embeds_monospace_and_exports_long_code_across_pages() {
        let mut nb = Notebook::new("Code");
        nb.chapters[0].text = format!("```rust\n{}\n```", "let x = 42; // code\n".repeat(100));
        let bytes = pdf::export(&nb, None);
        assert!(bytes.starts_with(b"%PDF"));
        let source = String::from_utf8_lossy(&bytes);
        assert!(source.contains("Hack-Regular"));
        assert!(source.matches("/Type /Page ").count() >= 2);
    }
}
