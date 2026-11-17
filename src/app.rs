mod home;
mod titlebar;

use crate::autosave;
use crate::blocks::{self, BlockOp, Kind};
use crate::edit;
use crate::format;
use crate::library::{self, Library};
use crate::model::{Attr, Category, Chapter, LineStyle, Notebook, Style};
use crate::pets::{self, Pet, Sprites};
use crate::render;
use crate::settings::Settings;
use crate::theme::{self, ACCENT, BG, BORDER, FAINT, HOVER, MUTED, SIDEBAR, SURFACE, TEXT};
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Frame, Id, Key, KeyboardShortcut, Layout, Margin, Modifiers,
    Rect, Response, RichText, Sense, Stroke, TextBuffer, TextEdit, Ui, ViewportCommand, pos2, vec2,
};
use std::ops::Range;
use std::path::{Path, PathBuf};

const SIDEBAR_WIDTH: f32 = 248.0;
const COLUMN_MAX: f32 = 720.0;
const STATUS_HEIGHT: f32 = 30.0;
const SELECTION: Color32 = Color32::from_rgba_premultiplied(0x59, 0x44, 0x30, 0x6b);
const SELECTION_INACTIVE: Color32 = Color32::from_rgba_premultiplied(0x38, 0x36, 0x3a, 0x60);

#[derive(Clone, Copy)]
enum ToolbarMenu {
    Heading,
    Underline,
    TextColor,
    Highlight,
}

impl ToolbarMenu {
    const ALL: [Self; 4] = [Self::Heading, Self::Underline, Self::TextColor, Self::Highlight];

    fn key(self) -> Key {
        match self {
            Self::Heading => Key::H,
            Self::Underline => Key::U,
            Self::TextColor => Key::C,
            Self::Highlight => Key::M,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Heading => "Heading",
            Self::Underline => "Underline style",
            Self::TextColor => "Text color",
            Self::Highlight => "Highlight",
        }
    }

    fn id(self) -> Id {
        Id::new(("hz-toolbar-menu", self.label()))
    }

    fn hint(self) -> String {
        format!("{}  (Ctrl+Alt+{})", self.label(), self.key().name())
    }
}

#[derive(Clone, Copy)]
enum FormatOp {
    Toggle(Attr),
    Fg(Option<Color32>),
    Bg(Option<Color32>),
    Underline(Option<LineStyle>),
    Clear,
}

impl FormatOp {
    fn apply_to(self, s: &mut Style) {
        match self {
            FormatOp::Toggle(a) => a.set(s, !a.get(s)),
            FormatOp::Fg(c) => s.fg = c,
            FormatOp::Bg(c) => s.bg = c,
            FormatOp::Underline(k) => s.underline = k,
            FormatOp::Clear => *s = Style::default(),
        }
    }
}

#[derive(Clone)]
enum After {
    NewNotebook,
    OpenDialog,
    OpenPath(PathBuf),
    GoHome,
    Tour,
    Quit,
}

enum Modal {
    Unsaved(After),
    DeleteChapter(usize),
    /// The category manager. `focus`: category whose name field to focus.
    Categories { focus: Option<u64> },
    /// Create (`id: None`) or edit a category. A new category is assigned
    /// to chapter `assign` (by id), if set.
    Category {
        id: Option<u64>,
        name: String,
        color: Color32,
        assign: Option<u64>,
        focus: bool,
    },
    Create { name: String, focus: bool },
    About { show_license: bool },
    Settings,
    /// An autosave exists for this notebook: offer to open it instead.
    Recover { path: PathBuf, saved_at: u64 },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Home,
    Editor,
}

enum HomeAction {
    Open(PathBuf),
    Star(PathBuf, bool),
    Remove(PathBuf),
    New,
    OpenDialog,
    Tour,
    About,
    Settings,
}

enum ChapterAction {
    Select(usize),
    /// Move chapter `from` to before index `to`, into category `category`.
    Move { from: usize, to: usize, category: Option<u64> },
    SetCategory(usize, Option<u64>),
    Delete(usize),
    Add,
    NewCategory(Option<u64>),
    ManageCategories,
    Export(usize),
}

/// What to reset: inline styles, line formatting, or both.
#[derive(Clone, Copy)]
enum Reset {
    Inline,
    Lines,
    All,
}

/// A choice made in a category picker menu.
enum CategoryPick {
    Set(Option<u64>),
    New,
    Manage,
}

/// Formatting chosen with an empty selection, applied to the next typed text.
#[derive(Clone, Copy)]
struct Pending {
    chapter: u64,
    at: usize,
    style: Style,
}

pub struct HazelnutApp {
    nb: Notebook,
    path: Option<PathBuf>,
    dirty: bool,
    current: usize,
    pending: Option<Pending>,
    modal: Option<Modal>,
    toast: Option<(String, f64, bool)>,
    allow_close: bool,
    window_title: String,
    focus_title: bool,
    screen: Screen,
    library: Library,
    caret_seen: Option<egui::text_selection::CCursorRange>,
    caret_moved_at: f64,
    last_press: Option<(f64, egui::Pos2)>,
    kept_selection: Option<(u64, egui::text_selection::CCursorRange)>,
    /// Content of the last autosave, to skip identical writes.
    last_autosave: Option<String>,
    next_autosave_at: f64,
    settings: Settings,
    sprites: Sprites,
    pet: Option<Pet>,
    /// Chapter (by id) being dragged in the sidebar.
    dragging: Option<u64>,
    /// Category picked for the current chapter, applied after the frame.
    pending_category: Option<Option<u64>>,
    /// Show the body as editable markup instead of formatted text.
    code_view: bool,
    /// The code view's text, for chapter (id). Parsed into the chapter on
    /// every change; rebuilt when the view or chapter changes.
    code: Option<(u64, String)>,
}

impl HazelnutApp {
    pub fn new(cc: &eframe::CreationContext<'_>, path: Option<PathBuf>) -> Self {
        theme::install(&cc.egui_ctx);
        let mut app = Self {
            nb: Notebook::welcome(),
            path: None,
            dirty: false,
            current: 0,
            pending: None,
            modal: None,
            toast: None,
            allow_close: false,
            window_title: String::new(),
            focus_title: false,
            screen: Screen::Home,
            library: Library::load(),
            caret_seen: None,
            caret_moved_at: 0.0,
            last_press: None,
            kept_selection: None,
            last_autosave: None,
            next_autosave_at: 0.0,
            settings: Settings::load(),
            sprites: Sprites::default(),
            pet: None,
            dragging: None,
            pending_category: None,
            code_view: false,
            code: None,
        };
        app.pet = app.settings.pet.as_deref().and_then(pets::find).map(Pet::new);
        if let Some(p) = path {
            if p.exists() {
                app.open(&p, 0.0);
            } else {
                app.screen = Screen::Editor;
                // Start a fresh notebook that will be created on first save.
                let name = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                app.nb = Notebook::new(if name.is_empty() { "Untitled notebook".into() } else { name });
                app.path = Some(p);
            }
        }
        app
    }

    // ------------------------------------------------------------ helpers

    fn chapter(&self) -> &Chapter {
        &self.nb.chapters[self.current]
    }

    fn editor_id(&self) -> Id {
        Id::new(("hz-body", self.chapter().id))
    }

    fn title_id(&self) -> Id {
        Id::new(("hz-title", self.chapter().id))
    }

    fn now(ctx: &egui::Context) -> f64 {
        ctx.input(|i| i.time)
    }

    fn toast(&mut self, ctx: &egui::Context, msg: impl Into<String>, error: bool) {
        self.toast = Some((msg.into(), Self::now(ctx), error));
    }

    /// Current body selection as a sorted char range (may be empty = caret).
    fn selection(&self, ctx: &egui::Context) -> Option<Range<usize>> {
        let state = egui::text_edit::TextEditState::load(ctx, self.editor_id())?;
        let r = state.cursor.char_range()?;
        let (a, b) = (r.primary.index, r.secondary.index);
        Some(a.min(b)..a.max(b))
    }

    /// The style the toolbar should reflect.
    fn active_style(&self, sel: &Option<Range<usize>>) -> Style {
        let ch = self.chapter();
        match sel {
            Some(r) if !r.is_empty() => {
                let slice = &ch.styles[r.start.min(ch.styles.len())..r.end.min(ch.styles.len())];
                let mut s = slice.first().copied().unwrap_or_default();
                s.bold = slice.iter().all(|x| x.bold);
                s.italic = slice.iter().all(|x| x.italic);
                if !slice.iter().all(|x| x.underline.is_some()) {
                    s.underline = None;
                }
                s.strike = slice.iter().all(|x| x.strike);
                s
            }
            Some(r) => match self.pending {
                Some(p) if p.chapter == ch.id && p.at == r.start => p.style,
                _ => edit::style_at(&ch.styles, r.start),
            },
            None => Style::default(),
        }
    }

    fn apply_format(&mut self, ctx: &egui::Context, op: FormatOp) {
        let Some(sel) = self.selection(ctx) else { return };
        let id = self.editor_id();
        if sel.is_empty() {
            let mut style = self.active_style(&Some(sel.clone()));
            op.apply_to(&mut style);
            self.pending = Some(Pending {
                chapter: self.chapter().id,
                at: sel.start,
                style,
            });
        } else {
            let styles = &mut self.nb.chapters[self.current].styles;
            match op {
                FormatOp::Toggle(a) => edit::toggle(styles, sel, a),
                FormatOp::Fg(c) => edit::set_fg(styles, sel, c),
                FormatOp::Bg(c) => edit::set_bg(styles, sel, c),
                FormatOp::Underline(k) => edit::set_underline(styles, sel, k),
                FormatOp::Clear => edit::clear(styles, sel),
            }
            self.dirty = true;
        }
        ctx.memory_mut(|m| m.request_focus(id));
    }

    fn set_code_view(&mut self, ctx: &egui::Context, on: bool) {
        if self.code_view == on {
            return;
        }
        self.code_view = on;
        self.code = None;
        self.pending = None;
        let id = if on { Id::new(("hz-code", self.chapter().id)) } else { self.editor_id() };
        ctx.memory_mut(|m| m.request_focus(id));
    }

    /// The code view: the body as `.hzn` markup in a plain text editor.
    fn code_body(&mut self, ui: &mut Ui, viewport_h: f32) {
        let ch = &self.nb.chapters[self.current];
        let id = ch.id;
        if self.code.as_ref().is_none_or(|(cid, _)| *cid != id) {
            self.code = Some((id, format::body_markup(&ch.text, &ch.styles)));
        }
        let Some((_, buf)) = self.code.as_mut() else { return };
        let mut layouter = |ui: &Ui, buf: &dyn TextBuffer, wrap: f32| {
            ui.fonts_mut(|f| f.layout_job(render::code_job(buf.as_str(), wrap)))
        };
        let out = TextEdit::multiline(buf)
            .id(Id::new(("hz-code", id)))
            .frame(false)
            .margin(Margin::ZERO)
            .lock_focus(true)
            .desired_width(f32::INFINITY)
            .min_size(vec2(0.0, (viewport_h - 200.0).max(240.0)))
            .hint_text(RichText::new("Start writing…").color(FAINT).monospace())
            .layouter(&mut layouter)
            .show(ui);
        if out.response.changed() {
            let (text, styles) = format::parse_body(buf);
            let ch = &mut self.nb.chapters[self.current];
            if ch.text != text || ch.styles != styles {
                ch.text = text;
                ch.styles = styles;
                self.dirty = true;
            }
        }
    }

    fn insert_code_block(&mut self, ctx: &egui::Context, language: &str) {
        let range = self.selection(ctx).unwrap_or(0..0);
        let ch = &mut self.nb.chapters[self.current];
        let chars: Vec<_> = ch.text.chars().collect();
        let start = chars[..range.start].iter().rposition(|c| *c == '\n').map_or(0, |i| i + 1);
        let end = if range.end > range.start && chars.get(range.end - 1) == Some(&'\n') {
            range.end - 1
        } else {
            chars[range.end..].iter().position(|c| *c == '\n').map_or(chars.len(), |i| range.end + i)
        };
        let content: String = chars[start..end].iter().collect();
        // Use a fence longer than any run already present in the snippet.
        let fence_len = content.split(|c| c != '`').map(str::len).max().unwrap_or(0).max(2) + 1;
        let fence = "`".repeat(fence_len);
        let prefix = format!("{fence}{language}\n");
        let insert = format!("{prefix}{content}\n{fence}");
        apply_edit(&mut ch.text, &mut ch.styles, &blocks::Edit { at: start, removed: end - start, insert });
        let caret = start + prefix.chars().count();
        let id = self.editor_id();
        let mut state = egui::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
        let range = egui::text_selection::CCursorRange::one(egui::text::CCursor::new(caret));
        state.cursor.set_char_range(Some(range));
        state.store(ctx, id);
        self.kept_selection = Some((self.chapter().id, range));
        self.pending = None;
        self.dirty = true;
        ctx.memory_mut(|m| m.request_focus(id));
    }

    /// Turn the lines touched by the selection into headings / lists.
    fn apply_block(&mut self, ctx: &egui::Context, op: BlockOp) {
        self.edit_lines(ctx, |text, sel, styles| blocks::set_blocks_styled(text, sel, op, styles));
    }

    /// Make the selected lines a callout colored `spec` (`""` = neutral),
    /// recolor the callout the caret is in, or remove it (`None`).
    fn apply_callout(&mut self, ctx: &egui::Context, spec: Option<&str>) {
        self.edit_lines(ctx, |text, sel, styles| blocks::set_callout_styled(text, sel, spec, styles));
    }

    /// Reset formatting on the selection (or, with none, for what's typed
    /// next) and/or the lines it touches.
    fn reset_formatting(&mut self, ctx: &egui::Context, r: Reset) {
        if matches!(r, Reset::Inline | Reset::All) {
            self.apply_format(ctx, FormatOp::Clear);
        }
        if matches!(r, Reset::Lines | Reset::All) {
            // Headings and lists become plain lines, then any callout goes.
            self.edit_lines(ctx, |text, sel, styles| blocks::set_blocks_styled(text, sel, BlockOp::Heading(0), styles));
            self.apply_callout(ctx, None);
        }
    }

    /// Apply line edits computed from the text and selection, keeping the selection.
    fn edit_lines(&mut self, ctx: &egui::Context, make: impl FnOnce(&str, Range<usize>, &[Style]) -> Vec<blocks::Edit>) {
        let id = self.editor_id();
        let state = egui::text_edit::TextEditState::load(ctx, id);
        let range = state.as_ref().and_then(|s| s.cursor.char_range());
        let (a, b) = range.map_or((0, 0), |r| (r.primary.index, r.secondary.index));
        let ch = &mut self.nb.chapters[self.current];
        let edits = make(&ch.text, a.min(b)..a.max(b), &ch.styles);
        if edits.is_empty() {
            return;
        }
        let (mut a, mut b) = (a, b);
        for e in &edits {
            apply_edit(&mut ch.text, &mut ch.styles, e);
            (a, b) = (e.map(a), e.map(b));
        }
        if let Some(mut state) = state {
            let r = egui::text_selection::CCursorRange::two(egui::text::CCursor::new(b), egui::text::CCursor::new(a));
            state.cursor.set_char_range(Some(r));
            state.store(ctx, id);
            self.kept_selection = Some((ch.id, r));
        }
        self.pending = None;
        self.dirty = true;
        ctx.memory_mut(|m| m.request_focus(id));
    }

    /// The callout the caret is in: `Some(spec)` (`""` = no color given).
    fn current_callout(&self, ctx: &egui::Context) -> Option<String> {
        let at = self.selection(ctx).map_or(0, |r| r.start);
        let text = &self.chapter().text;
        let line = blocks::lines(text).take_while(|(start, _)| *start <= at).count().saturating_sub(1);
        blocks::callouts_styled(text, &self.chapter().styles)
            .into_iter()
            .find(|(r, _)| r.contains(&line))
            .map(|(_, spec)| spec.unwrap_or_default())
    }

    /// Kind of the line the caret is on.
    fn current_block(&self, ctx: &egui::Context) -> Kind {
        let at = self.selection(ctx).map_or(0, |r| r.start);
        let text = &self.chapter().text;
        blocks::parsed_lines(text, &self.chapter().styles).into_iter()
            .take_while(|(start, _, _)| *start <= at)
            .last()
            .map_or(Kind::Plain, |(_, _, block)| block.kind)
    }

    fn select_chapter(&mut self, i: usize) {
        self.current = i.min(self.nb.chapters.len() - 1);
        self.pending = None;
    }

    fn add_chapter(&mut self) {
        let at = self.current + 1;
        // Stay in the current chapter's category so the grouping holds.
        let mut ch = Chapter::new("");
        ch.category = self.chapter().category;
        self.nb.chapters.insert(at, ch);
        self.select_chapter(at);
        self.focus_title = true;
        self.dirty = true;
    }

    fn delete_chapter(&mut self, i: usize) {
        if self.nb.chapters.len() <= 1 {
            return;
        }
        self.nb.chapters.remove(i);
        if self.current >= i && self.current > 0 {
            self.current -= 1;
        }
        self.select_chapter(self.current);
        self.dirty = true;
    }

    /// Move chapter `from` to before index `to` (an index into the list as
    /// it is now) and file it under `category`. Keeps the current chapter.
    fn move_chapter(&mut self, from: usize, to: usize, category: Option<u64>) {
        let current = self.chapter().id;
        let ch = &self.nb.chapters[from];
        let to = if from < to { to - 1 } else { to };
        if to == from && ch.category == category {
            return;
        }
        let mut ch = self.nb.chapters.remove(from);
        ch.category = category;
        self.nb.chapters.insert(to.min(self.nb.chapters.len()), ch);
        self.nb.normalize();
        self.reselect(current);
        self.dirty = true;
    }

    /// File chapter `i` under `category`, at the end of that group.
    fn set_category(&mut self, i: usize, category: Option<u64>) {
        if self.nb.chapters[i].category == category {
            return;
        }
        let group = self.nb.category(category).map_or(0, |c| {
            self.nb.categories.iter().position(|k| k.id == c.id).unwrap_or(0) + 1
        });
        let to = self.nb.chapters.iter().filter(|c| self.nb.group_of(c) <= group).count();
        self.move_chapter(i, to, category);
    }

    /// Point `current` back at chapter `id` after the list was reordered.
    fn reselect(&mut self, id: u64) {
        if let Some(i) = self.nb.chapters.iter().position(|c| c.id == id) {
            self.current = i;
        }
    }

    fn category_changed(&mut self) {
        let current = self.chapter().id;
        self.nb.normalize();
        self.reselect(current);
        self.dirty = true;
    }

    fn delete_category(&mut self, id: u64) {
        self.nb.categories.retain(|c| c.id != id);
        self.category_changed();
    }

    /// Names must be non-empty and unique: the file refers to them by name.
    fn tidy_category_names(&mut self) {
        let mut seen: Vec<String> = Vec::new();
        for cat in &mut self.nb.categories {
            let base = match cat.name.trim() {
                "" => "Untitled".to_owned(),
                t => t.to_owned(),
            };
            let mut name = base.clone();
            let mut k = 2;
            while seen.contains(&name) {
                name = format!("{base} {k}");
                k += 1;
            }
            if cat.name != name {
                cat.name = name.clone();
                self.dirty = true;
            }
            seen.push(name);
        }
    }

    fn move_category(&mut self, id: u64, up: bool) {
        let cats = &mut self.nb.categories;
        let Some(i) = cats.iter().position(|c| c.id == id) else { return };
        let j = if up { i.checked_sub(1) } else { Some(i + 1).filter(|&j| j < cats.len()) };
        if let Some(j) = j {
            cats.swap(i, j);
            self.category_changed();
        }
    }

    fn open_category_modal(&mut self, id: Option<u64>, assign: Option<u64>) {
        let (name, color) = match self.nb.category(id) {
            Some(c) => (c.name.clone(), c.color),
            None => {
                // Suggest the first palette color not in use yet.
                let used: Vec<Color32> = self.nb.categories.iter().map(|c| c.color).collect();
                let palette = category_palette();
                let color = palette.iter().copied().find(|c| !used.contains(c)).unwrap_or(palette[0]);
                (String::new(), color)
            }
        };
        self.modal = Some(Modal::Category { id, name, color, assign, focus: true });
    }

    // ------------------------------------------------------------ files

    /// Open a notebook, first offering to recover an autosave if one exists.
    fn open(&mut self, path: &Path, now: f64) {
        match autosave::find(path) {
            Some(saved_at) => {
                self.modal = Some(Modal::Recover {
                    path: path.to_owned(),
                    saved_at,
                });
            }
            None => {
                self.load(path, now);
            }
        }
    }

    /// Open the autosaved copy of `path`. It stays unsaved until you save it.
    fn load_recovered(&mut self, path: &Path, now: f64) -> bool {
        match autosave::read(path) {
            Ok(nb) => {
                self.library.touch(path, &nb.title);
                self.nb = nb;
                self.path = Some(path.to_owned());
                self.dirty = true;
                self.current = 0;
                self.pending = None;
                self.screen = Screen::Editor;
                self.last_autosave = None;
                self.toast = Some(("Recovered unsaved changes — save to keep them".into(), now, false));
                true
            }
            Err(e) => {
                self.toast = Some((format!("Couldn't read the recovered copy: {e}"), now, true));
                false
            }
        }
    }

    /// Periodically write a crash-recovery copy while there are unsaved changes.
    fn autosave_tick(&mut self, ctx: &egui::Context) {
        const INTERVAL: f64 = 3.0;
        if self.screen != Screen::Editor || !self.dirty {
            self.last_autosave = None;
            return;
        }
        let Some(path) = self.path.clone() else { return };
        let now = Self::now(ctx);
        if now < self.next_autosave_at {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(self.next_autosave_at - now));
            return;
        }
        self.next_autosave_at = now + INTERVAL;
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(INTERVAL));
        let data = format::serialize(&self.nb);
        if self.last_autosave.as_deref() == Some(data.as_str()) {
            return;
        }
        if autosave::write(&path, &data).is_ok() {
            self.last_autosave = Some(data);
        }
    }

    fn load(&mut self, path: &Path, now: f64) -> bool {
        let result = std::fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|s| format::parse(&s).map_err(|e| e.to_string()));
        match result {
            Ok(nb) => {
                self.library.touch(path, &nb.title);
                self.nb = nb;
                self.path = Some(path.to_owned());
                self.dirty = false;
                self.last_autosave = None;
                self.current = 0;
                self.pending = None;
                self.screen = Screen::Editor;
                self.toast = Some((format!("Opened {}", file_name(path)), now, false));
                true
            }
            Err(e) => {
                self.toast = Some((format!("Couldn't open {}: {e}", file_name(path)), now, true));
                false
            }
        }
    }

    fn write(&mut self, ctx: &egui::Context, path: PathBuf) -> bool {
        let data = format::serialize(&self.nb);
        let tmp = path.with_extension("hzn.tmp");
        let res = std::fs::write(&tmp, data).and_then(|_| std::fs::rename(&tmp, &path));
        match res {
            Ok(()) => {
                // Saved for real: crash-recovery copies are no longer needed.
                autosave::remove(&path);
                if let Some(old) = self.path.as_ref().filter(|old| **old != path) {
                    autosave::remove(old);
                }
                self.last_autosave = None;
                self.toast(ctx, format!("Saved {}", file_name(&path)), false);
                self.library.touch(&path, &self.nb.title);
                self.path = Some(path);
                self.dirty = false;
                true
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                self.toast(ctx, format!("Couldn't save: {e}"), true);
                false
            }
        }
    }

    fn save(&mut self, ctx: &egui::Context) -> bool {
        match self.path.clone() {
            Some(p) => self.write(ctx, p),
            None => self.save_as(ctx),
        }
    }

    fn save_as(&mut self, ctx: &egui::Context) -> bool {
        match self.pick_save_path("Save notebook", &self.nb.title.clone(), "Hazelnut notebook", format::EXTENSION) {
            Some(p) => self.write(ctx, p),
            None => false,
        }
    }

    fn pick_save_path(&self, dialog_title: &str, name: &str, kind: &str, ext: &str) -> Option<PathBuf> {
        let suggested: String = name
            .chars()
            .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' })
            .collect();
        let suggested = if suggested.trim().is_empty() { "notebook".into() } else { suggested.trim().to_owned() };
        let mut dialog = rfd::FileDialog::new()
            .set_title(dialog_title)
            .add_filter(kind, &[ext])
            .set_file_name(format!("{suggested}.{ext}"));
        let last_dir = self.path.clone().or_else(|| self.library.entries.first().map(|e| e.path.clone()));
        if let Some(dir) = last_dir.as_ref().and_then(|p| p.parent()) {
            dialog = dialog.set_directory(dir);
        }
        let mut p = dialog.save_file()?;
        if p.extension().is_none() {
            p.set_extension(ext);
        }
        Some(p)
    }

    /// Export the notebook (`only: None`) or one chapter as a PDF.
    fn export_pdf(&mut self, ctx: &egui::Context, only: Option<usize>) {
        let name = match only {
            Some(i) => self.nb.chapters[i].title.clone(),
            None => self.nb.title.clone(),
        };
        let title = if only.is_some() { "Export chapter as PDF" } else { "Export notebook as PDF" };
        let Some(path) = self.pick_save_path(title, &name, "PDF document", "pdf") else { return };
        let bytes = crate::pdf::export(&self.nb, only);
        match std::fs::write(&path, bytes) {
            Ok(()) => self.toast(ctx, format!("Exported {}", file_name(&path)), false),
            Err(e) => self.toast(ctx, format!("Couldn't export: {e}"), true),
        }
    }

    /// Create a new notebook file named `name` (asks where to save it).
    fn create_notebook(&mut self, ctx: &egui::Context, name: &str) -> bool {
        let name = if name.trim().is_empty() { "Untitled notebook" } else { name.trim() };
        let Some(path) = self.pick_save_path("Create notebook", name, "Hazelnut notebook", format::EXTENSION) else { return false };
        let previous = std::mem::replace(&mut self.nb, Notebook::new(name));
        if self.write(ctx, path) {
            self.select_chapter(0);
            self.screen = Screen::Editor;
            self.focus_title = true;
            true
        } else {
            self.nb = previous;
            false
        }
    }

    fn open_dialog(&mut self, ctx: &egui::Context) {
        let mut dialog = rfd::FileDialog::new()
            .set_title("Open notebook")
            .add_filter("Hazelnut notebook", &[format::EXTENSION])
            .add_filter("All files", &["*"]);
        if let Some(dir) = self.path.as_ref().and_then(|p| p.parent()) {
            dialog = dialog.set_directory(dir);
        }
        if let Some(p) = dialog.pick_file() {
            self.open(&p, Self::now(ctx));
        }
    }

    /// Run `after`, first asking about unsaved changes if needed.
    fn guarded(&mut self, ctx: &egui::Context, after: After) {
        if self.dirty {
            self.modal = Some(Modal::Unsaved(after));
        } else {
            self.proceed(ctx, after);
        }
    }

    fn proceed(&mut self, ctx: &egui::Context, after: After) {
        match after {
            After::NewNotebook => {
                self.modal = Some(Modal::Create {
                    name: String::new(),
                    focus: true,
                });
            }
            After::GoHome => {
                self.screen = Screen::Home;
                self.path = None;
                self.dirty = false;
                self.library.refresh();
            }
            After::Tour => {
                self.nb = Notebook::welcome();
                self.path = None;
                self.dirty = false;
                self.select_chapter(0);
                self.screen = Screen::Editor;
            }
            After::OpenDialog => self.open_dialog(ctx),
            After::OpenPath(p) => {
                self.open(&p, Self::now(ctx));
            }
            After::Quit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }
    }

    // ------------------------------------------------------------ input

    fn handle_window_events(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty && !self.allow_close {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.modal = Some(Modal::Unsaved(After::Quit));
        }
        let dropped = ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone()));
        if let Some(p) = dropped {
            self.guarded(ctx, After::OpenPath(p));
        }

        let title = if self.screen == Screen::Home {
            "Hazelnut".to_owned()
        } else {
            format!(
            "{}{} — Hazelnut",
            if self.dirty { "• " } else { "" },
                if self.nb.title.trim().is_empty() { "Untitled notebook" } else { self.nb.title.trim() }
            )
        };
        if title != self.window_title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.window_title = title;
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if self.modal.is_some() {
            return;
        }
        let cmd = Modifiers::COMMAND;
        let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
        let pressed = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k)));

        if pressed(cmd, Key::O) {
            self.guarded(ctx, After::OpenDialog);
        }
        if pressed(cmd, Key::N) {
            self.guarded(ctx, After::NewNotebook);
        }
        if pressed(cmd, Key::Comma) {
            self.modal = Some(Modal::Settings);
        }
        if self.screen == Screen::Home {
            return;
        }
        if pressed(cmd_shift, Key::S) {
            self.save_as(ctx);
        } else if pressed(cmd, Key::S) {
            self.save(ctx);
        }
        if pressed(cmd, Key::W) {
            self.guarded(ctx, After::GoHome);
        }
        if !self.code_view && pressed(cmd_shift, Key::M) {
            self.settings.literal_text = !self.settings.literal_text;
            self.settings.save();
        }
        if pressed(cmd, Key::Slash) {
            self.set_code_view(ctx, !self.code_view);
        }
        if pressed(cmd, Key::E) {
            self.export_pdf(ctx, None);
        }
        if pressed(cmd, Key::T) {
            self.add_chapter();
        }
        if pressed(cmd, Key::PageDown) {
            self.select_chapter((self.current + 1).min(self.nb.chapters.len() - 1));
        }
        if pressed(cmd, Key::PageUp) {
            self.select_chapter(self.current.saturating_sub(1));
        }

        // Formatting shortcuts only while the body editor has focus.
        if ctx.memory(|m| m.has_focus(self.editor_id())) {
            if !self.code_view {
                if pressed(Modifiers::COMMAND | Modifiers::ALT, Key::K) {
                    self.insert_code_block(ctx, "");
                }
                for menu in ToolbarMenu::ALL {
                    if pressed(Modifiers::COMMAND | Modifiers::ALT, menu.key()) {
                        egui::Popup::toggle_id(ctx, menu.id());
                    }
                }
            }
            // Shift+\ is `|` on many layouts.
            if pressed(cmd_shift, Key::Backslash) || pressed(cmd_shift, Key::Pipe) {
                self.reset_formatting(ctx, Reset::All);
            }
            let ops = [
                (cmd_shift, Key::X, FormatOp::Toggle(Attr::Strike)),
                (cmd, Key::B, FormatOp::Toggle(Attr::Bold)),
                (cmd, Key::I, FormatOp::Toggle(Attr::Italic)),
                (cmd, Key::U, FormatOp::Toggle(Attr::Underline)),
                (cmd, Key::Backslash, FormatOp::Clear),
            ];
            for (m, k, op) in ops {
                if pressed(m, k) {
                    self.apply_format(ctx, op);
                }
            }
            let cmd_alt = Modifiers::COMMAND | Modifiers::ALT;
            let block_ops = [
                (cmd_alt, Key::Num0, BlockOp::Heading(0)),
                (cmd_alt, Key::Num1, BlockOp::Heading(1)),
                (cmd_alt, Key::Num2, BlockOp::Heading(2)),
                (cmd_alt, Key::Num3, BlockOp::Heading(3)),
                (cmd_shift, Key::L, BlockOp::Bullet),
                (cmd_shift, Key::K, BlockOp::Task),
            ];
            for (m, k, op) in block_ops {
                if pressed(m, k) {
                    self.apply_block(ctx, op);
                }
            }
            // Ctrl+Space also clears inline styles, for keyboards where `\`
            // is awkward to reach. (Shifted variants are checked first: egui
            // matches Ctrl+Shift+Space as Ctrl+Space too.)
            if pressed(cmd_shift, Key::Space) {
                self.reset_formatting(ctx, Reset::Lines);
            }
            if pressed(cmd, Key::Space) {
                self.reset_formatting(ctx, Reset::Inline);
            }
            if pressed(cmd_shift, Key::Q) {
                let spec = if self.current_callout(ctx).is_some() { None } else { Some("") };
                self.apply_callout(ctx, spec);
            }
        }
    }

    // ------------------------------------------------------------ sidebar

    fn sidebar(&mut self, ctx: &egui::Context) {
        let frame = Frame::new().fill(SIDEBAR).inner_margin(Margin {
            left: 16,
            right: 16,
            top: 10,
            bottom: 14,
        });
        egui::SidePanel::left("sidebar")
            .exact_width(SIDEBAR_WIDTH)
            .resizable(false)
            .frame(frame)
            .show(ctx, |ui| {
                // Notebook title
                let resp = ui.add(
                    TextEdit::singleline(&mut self.nb.title)
                        .font(FontId::new(19.0, theme::medium()))
                        .text_color(TEXT)
                        .hint_text(RichText::new("Untitled notebook").color(FAINT))
                        .frame(false)
                        .margin(Margin::ZERO)
                        .desired_width(f32::INFINITY),
                );
                if resp.changed() {
                    self.dirty = true;
                }
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    let name = self.path.as_deref().map(file_name).unwrap_or_else(|| "Not saved yet".into());
                    ui.label(RichText::new(name).size(12.0).color(MUTED));
                    if self.dirty {
                        let (r, _) = ui.allocate_exact_size(vec2(6.0, 12.0), Sense::hover());
                        ui.painter().circle_filled(r.center(), 3.0, ACCENT);
                    }
                    if let Some(path) = self.path.clone().filter(|p| p.exists()) {
                        let starred = self.library.is_starred(&path);
                        let tip = if starred { "Unstar notebook" } else { "Star notebook" };
                        if star_button(ui, starred, true, 16.0).on_hover_text(tip).clicked() {
                            self.library.set_starred(&path, !starred);
                        }
                    }
                });

                ui.add_space(26.0);
                let mut action = None;
                ui.horizontal(|ui| {
                    ui.label(RichText::new("CHAPTERS").size(10.5).color(FAINT).extra_letter_spacing(1.5));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        if icon_button(ui, "+", "New chapter  (Ctrl+T)").clicked() {
                            action = Some(ChapterAction::Add);
                        }
                        let resp = tool_icon(ui, "Categories", |p, r, col| paint_gear(p, r.center(), col));
                        if resp.clicked() {
                            action = Some(ChapterAction::ManageCategories);
                        }
                    });
                });
                ui.add_space(4.0);

                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    self.file_actions(ui, ctx);
                    ui.add_space(10.0);
                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                            self.chapter_list(ui, &mut action);
                        });
                    });
                });

                match action {
                    Some(ChapterAction::Select(i)) => self.select_chapter(i),
                    Some(ChapterAction::Add) => self.add_chapter(),
                    Some(ChapterAction::Move { from, to, category }) => self.move_chapter(from, to, category),
                    Some(ChapterAction::SetCategory(i, c)) => self.set_category(i, c),
                    Some(ChapterAction::Delete(i)) => {
                        if self.nb.chapters[i].text.trim().is_empty() {
                            self.delete_chapter(i);
                        } else {
                            self.modal = Some(Modal::DeleteChapter(i));
                        }
                    }
                    Some(ChapterAction::NewCategory(assign)) => self.open_category_modal(None, assign),
                    Some(ChapterAction::Export(i)) => self.export_pdf(ctx, Some(i)),
                    Some(ChapterAction::ManageCategories) => self.modal = Some(Modal::Categories { focus: None }),
                    None => {}
                }
            });
    }

    /// The chapters, grouped by category, with drag-and-drop reordering.
    fn chapter_list(&mut self, ui: &mut Ui, action: &mut Option<ChapterAction>) {
        ui.spacing_mut().item_spacing.y = 2.0;
        let nb = &self.nb;
        let n = nb.chapters.len();
        let has_categories = !nb.categories.is_empty();
        let dragging = self.dragging.and_then(|id| nb.chapters.iter().position(|c| c.id == id));
        let mut drag_started = None;
        let mut drag_released = false;
        // Possible drop positions: (line y, insert-before index, category).
        let mut slots: Vec<(f32, usize, Option<u64>)> = Vec::new();

        let groups = std::iter::once(None).chain(nb.categories.iter().map(Some));
        let mut i = 0;
        for (g, cat) in groups.enumerate() {
            let cat_id = cat.map(|c| c.id);
            let start = i;
            // Chapters are kept sorted by group (see `Notebook::normalize`).
            while i < n && nb.group_of(&nb.chapters[i]) == g {
                i += 1;
            }
            let end = i;

            if start == end {
                continue;
            }
            if start > 0 {
                ui.add_space(8.0); // gap between groups
            }
            // A dragged chapter can only be dropped within its own group.
            let drop_here = dragging.is_some_and(|d| (start..end).contains(&d));

            for k in start..end {
                let ch = &nb.chapters[k];
                let shown_cat = if has_categories { Some(cat) } else { None };
                let resp = chapter_row(ui, &ch.title, shown_cat, k == self.current, dragging == Some(k));
                if drop_here {
                    slots.push((resp.rect.top() - 1.0, k, cat_id));
                    if k + 1 == end {
                        slots.push((resp.rect.bottom() + 1.0, end, cat_id));
                    }
                }
                if resp.clicked() {
                    *action = Some(ChapterAction::Select(k));
                }
                if resp.drag_started() {
                    drag_started = Some(ch.id);
                }
                if resp.drag_stopped() {
                    drag_released = true;
                }
                resp.context_menu(|ui| {
                    ui.set_min_width(160.0);
                    ui.menu_button("Category", |ui| match category_menu(ui, &nb.categories, ch.category) {
                        Some(CategoryPick::Set(c)) => *action = Some(ChapterAction::SetCategory(k, c)),
                        Some(CategoryPick::New) => *action = Some(ChapterAction::NewCategory(Some(ch.id))),
                        Some(CategoryPick::Manage) => *action = Some(ChapterAction::ManageCategories),
                        None => {}
                    });
                    if ui.button("Export as PDF…").clicked() {
                        *action = Some(ChapterAction::Export(k));
                    }
                    ui.separator();
                    let del = egui::Button::new(RichText::new("Delete chapter").color(theme::DANGER));
                    if ui.add_enabled(n > 1, del).clicked() {
                        *action = Some(ChapterAction::Delete(k));
                    }
                });
            }
        }

        if drag_started.is_some() {
            self.dragging = drag_started;
        }
        let Some(from) = dragging else {
            return;
        };
        let pointer = ui.ctx().pointer_latest_pos();
        let target = pointer.and_then(|p| {
            slots
                .iter()
                .min_by(|a, b| (a.0 - p.y).abs().total_cmp(&(b.0 - p.y).abs()))
                .copied()
        });
        if drag_released || !ui.ctx().input(|i| i.pointer.any_down()) {
            self.dragging = None;
            if let Some((_, to, category)) = target {
                *action = Some(ChapterAction::Move { from, to, category });
            }
            return;
        }

        // Drop indicator, a floating label under the pointer, and edge scrolling.
        let full = ui.max_rect();
        if let Some((y, _, _)) = target {
            let line = Rect::from_min_max(pos2(full.left() + 4.0, y - 1.0), pos2(full.right() - 4.0, y + 1.0));
            ui.painter().rect_filled(line, 1.0, ACCENT);
            ui.painter().circle_filled(pos2(line.left(), y), 3.0, ACCENT);
        }
        if let Some(p) = pointer {
            let ch = &self.nb.chapters[from];
            let title = if ch.title.trim().is_empty() { "Untitled" } else { ch.title.trim() };
            let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, Id::new("hz-drag")));
            let galley = painter.layout_no_wrap(title.to_owned(), FontId::proportional(13.0), TEXT);
            let rect = Rect::from_min_size(p + vec2(12.0, 6.0), galley.size() + vec2(20.0, 12.0));
            painter.rect(rect, 7.0, SURFACE, Stroke::new(1.0_f32, BORDER), egui::StrokeKind::Inside);
            painter.galley(rect.min + vec2(10.0, 6.0), galley, TEXT);

            let clip = ui.clip_rect();
            if p.y < clip.top() + 24.0 {
                ui.scroll_with_delta(vec2(0.0, 6.0));
            } else if p.y > clip.bottom() - 24.0 {
                ui.scroll_with_delta(vec2(0.0, -6.0));
            }
        }
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        ui.ctx().request_repaint();
    }

    fn file_actions(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let save_label = RichText::new("Save").color(if self.dirty { ACCENT } else { MUTED });
            if text_button(ui, save_label, "Ctrl+S").clicked() {
                self.save(ctx);
            }
            if text_button(ui, RichText::new("Open").color(MUTED), "Ctrl+O").clicked() {
                self.guarded(ctx, After::OpenDialog);
            }
            if text_button(ui, RichText::new("New").color(MUTED), "New notebook  (Ctrl+N)").clicked() {
                self.guarded(ctx, After::NewNotebook);
            }
            let more = text_button(ui, RichText::new("•••").size(10.0).color(MUTED), "More");
            egui::Popup::menu(&more).show(|ui| {
                ui.set_min_width(190.0);
                if menu_item(ui, "Save as…", "Ctrl+Shift+S") {
                    self.save_as(ctx);
                }
                if menu_item(ui, "Export as PDF…", "Ctrl+E") {
                    self.export_pdf(ctx, None);
                }
                if menu_item(ui, "Settings…", "Ctrl+,") {
                    self.modal = Some(Modal::Settings);
                }
                if menu_item(ui, "About Hazelnut", "") {
                    self.modal = Some(Modal::About { show_license: false });
                }
                ui.separator();
                ui.label(RichText::new("SHORTCUTS").size(10.0).color(FAINT).extra_letter_spacing(1.5));
                for (k, v) in [
                    ("Bold", "Ctrl+B"),
                    ("Italic", "Ctrl+I"),
                    ("Underline", "Ctrl+U"),
                    ("Strikethrough", "Ctrl+Shift+X"),
                    ("Clear text formatting", "Ctrl+\\ / Ctrl+Space"),
                    ("Normal text (no heading, list, callout)", "Ctrl+Shift+Space"),
                    ("Clear all formatting", "Ctrl+Shift+\\"),
                    ("Heading 1 / 2 / 3", "Ctrl+Alt+1/2/3"),
                    ("Remove heading / list", "Ctrl+Alt+0"),
                    ("Bullet list", "Ctrl+Shift+L"),
                    ("Checklist", "Ctrl+Shift+K"),
                    ("Callout", "Ctrl+Shift+Q"),
                    ("New chapter", "Ctrl+T"),
                    ("Export as PDF", "Ctrl+E"),
                    ("Normal / code view", "Ctrl+/"),
                    ("Next / prev chapter", "Ctrl+PgDn/PgUp"),
                ] {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(k).size(12.5).color(MUTED));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(v).size(11.5).color(FAINT));
                        });
                    });
                }
            });
        });
    }

    // ------------------------------------------------------------ status bar

    fn status_bar(&mut self, ctx: &egui::Context) {
        let now = Self::now(ctx);
        egui::TopBottomPanel::bottom("status")
            .exact_height(STATUS_HEIGHT)
            .show_separator_line(false)
            .frame(Frame::new().fill(BG).inner_margin(Margin::symmetric(20, 0)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let ch = self.chapter();
                    let words = ch.word_count();
                    let chars = ch.text.chars().count();
                    ui.label(
                        RichText::new(format!(
                            "{words} {}  ·  {chars} characters  ·  chapter {} of {}",
                            if words == 1 { "word" } else { "words" },
                            self.current + 1,
                            self.nb.chapters.len()
                        ))
                        .size(11.5)
                        .color(FAINT),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some((msg, t, err)) = &self.toast {
                            let age = now - t;
                            if age < 4.0 {
                                let alpha = ((4.0 - age) / 0.6).clamp(0.0, 1.0) as f32;
                                let base = if *err { theme::DANGER } else { MUTED };
                                ui.label(RichText::new(msg).size(11.5).color(base.gamma_multiply(alpha)));
                                ctx.request_repaint();
                                return;
                            }
                        }
                        let (txt, col) = if self.dirty { ("Unsaved changes", MUTED) } else { ("All changes saved", FAINT) };
                        ui.label(RichText::new(txt).size(11.5).color(col));
                    });
                });
            });
    }

    // ------------------------------------------------------------ editor

    fn central(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(Frame::new().fill(BG))
            .show(ctx, |ui| {
                let avail = ui.available_width();
                let col_w = (avail - 2.0 * 80.0).clamp(240.0, COLUMN_MAX);
                let pad = ((avail - col_w) / 2.0).max(0.0);

                ui.add_space(24.0);

                let viewport_h = ui.available_height();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .id_salt(("scroll", self.chapter().id))
                    .show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            ui.add_space(pad);
                            ui.vertical(|ui| {
                                ui.set_width(col_w);
                                self.page(ui, viewport_h);
                            });
                        });
                    });
            });
    }

    fn page(&mut self, ui: &mut Ui, viewport_h: f32) {
        ui.add_space(28.0);
        let title_id = self.title_id();
        let body_id = self.editor_id();
        let current = self.current;
        let Notebook { chapters, categories, .. } = &mut self.nb;
        let ch = &mut chapters[current];
        let category = ch.category.and_then(|id| categories.iter().find(|c| c.id == id));

        // Title, with the category tag right after it.
        let pill_w = match category {
            Some(c) => {
                let name = if c.name.trim().is_empty() { "Untitled" } else { c.name.trim() };
                ui.painter().layout_no_wrap(name.to_owned(), FontId::new(12.5, theme::medium()), TEXT).size().x + 38.0
            }
            None => 22.0,
        };
        let max_w = (ui.available_width() - pill_w - 16.0).max(80.0);
        let mut pick = None;
        let title = ui
            .horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let title = chapter_title(ui, &mut ch.title, title_id, max_w);
                ui.add_space(14.0);
                let pill = category_pill(ui, category);
                egui::Popup::menu(&pill).show(|ui| {
                    pick = category_menu(ui, categories, ch.category);
                });
                title
            })
            .inner;
        if title.changed() {
            self.dirty = true;
        }
        if self.focus_title {
            title.request_focus();
            self.focus_title = false;
        }
        if title.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
            ui.memory_mut(|m| m.request_focus(body_id));
        }

        ui.add_space(22.0);

        match pick {
            Some(CategoryPick::Set(c)) => {
                // Deferred: it reorders chapters, so finish this frame first.
                self.pending_category = Some(c);
            }
            Some(CategoryPick::New) => self.open_category_modal(None, Some(self.chapter().id)),
            Some(CategoryPick::Manage) => self.modal = Some(Modal::Categories { focus: None }),
            None => {}
        }
        if self.code_view {
            self.code_body(ui, viewport_h);
            ui.add_space(120.0);
            return;
        }
        let ch = &mut self.nb.chapters[self.current];

        // ---- Body
        let chapter_id = ch.id;
        let pending = self.pending.filter(|p| p.chapter == chapter_id);
        let Chapter { text, styles, .. } = ch;
        let old_text = text.clone();
        let styles_now: &Vec<Style> = styles;
        let literal = self.settings.literal_text;
        let insert_style = |change: edit::Change| Some(edit::typing_style(
            styles_now, change.at, pending.filter(|p| p.at == change.at).map(|p| p.style),
        ));

        let mut layouter = |ui: &Ui, buf: &dyn TextBuffer, wrap: f32| {
            let s = buf.as_str();
            let job = if s == old_text {
                render::body_job(s, styles_now, wrap)
            } else {
                // Mid-edit relayout: derive styles for the new text on the fly.
                let st = match edit::diff(&old_text, s, None) {
                    Some(c) => edit::apply_typing(styles_now, c, insert_style(c), s, literal),
                    None => styles_now.clone(),
                };
                render::body_job(s, &st, wrap)
            };
            ui.fonts_mut(|f| f.layout_job(job))
        };

        // egui moves the caret on any mouse press; keep the selection when
        // right-clicking so the context menu can format it.
        let right_press = ui.input(|i| i.pointer.secondary_pressed());
        let saved_selection = right_press
            .then(|| egui::text_edit::TextEditState::load(ui.ctx(), body_id))
            .flatten()
            .and_then(|st| st.cursor.char_range())
            .filter(|r| r.primary.index != r.secondary.index);

        // Detect the second press of a double-click (egui only selects the
        // word on release, so the selection would flicker to a caret).
        let (press, press_pos, now) = ui.input(|i| (i.pointer.primary_pressed(), i.pointer.interact_pos(), i.time));
        let double_press = press
            && self
                .last_press
                .zip(press_pos)
                .is_some_and(|((t, p), q)| now - t < 0.3 && p.distance(q) < 6.0);
        if press {
            self.last_press = if double_press { None } else { press_pos.map(|p| (now, p)) };
        }

        // Reserve slots *behind* the text: callout boxes, highlights, then
        // the selection on top of them.
        let callout_slot = ui.painter().add(egui::Shape::Noop);
        let highlight_slot = ui.painter().add(egui::Shape::Noop);
        let selection_slot = ui.painter().add(egui::Shape::Noop);

        // The TextEdit only handles input (caret, selection, typing, IME,
        // scrolling). It is drawn fully transparent and the body is painted
        // below, because egui's own selection painting recolors the selected
        // glyphs (losing text colors, and skipping the last glyph of wrapped
        // rows) and only shows while focused.
        let min_h = (viewport_h - 200.0).max(240.0);
        let mut out = ui
            .scope(|ui| {
                ui.set_opacity(0.0);
                TextEdit::multiline(text)
                    .id(body_id)
                    .frame(false)
                    .margin(Margin::ZERO)
                    .lock_focus(true)
                    .desired_width(f32::INFINITY)
                    .min_size(vec2(0.0, min_h))
                    .layouter(&mut layouter)
                    .show(ui)
            })
            .inner;

        // Right-clicking inside the selection keeps it (egui collapses it on
        // any press); right-clicking elsewhere moves the caret as usual.
        if let Some(range) = saved_selection
            && out.response.hovered()
        {
            let [lo, hi] = range.sorted_cursors();
            let at = out.cursor_range.map_or(lo.index, |r| r.primary.index);
            if (lo.index..=hi.index).contains(&at) {
                out.state.cursor.set_char_range(Some(range));
                out.state.clone().store(ui.ctx(), body_id);
                out.cursor_range = Some(range);
            }
        }

        if double_press
            && out.response.hovered()
            && let Some(r) = out.cursor_range
        {
            let (a, b) = edit::word_at(text, r.primary.index);
            if a < b {
                let range = egui::text_selection::CCursorRange::two(egui::text::CCursor::new(a), egui::text::CCursor::new(b));
                out.state.cursor.set_char_range(Some(range));
                out.state.clone().store(ui.ctx(), body_id);
                out.cursor_range = Some(range);
            }
        }

        // With an IME active, egui collapses the selection whenever the editor
        // gains or loses focus (e.g. clicking the toolbar). Put it back.
        if (out.response.lost_focus() || out.response.gained_focus())
            && let Some((cid, kept)) = self.kept_selection
            && cid == chapter_id
            && kept.primary.index != kept.secondary.index
            && out
                .state
                .cursor
                .char_range()
                .is_some_and(|r| r.primary.index == r.secondary.index && r.primary.index == kept.primary.index)
        {
            out.state.cursor.set_char_range(Some(kept));
            out.state.clone().store(ui.ctx(), body_id);
            if out.cursor_range.is_some() {
                out.cursor_range = Some(kept);
            }
        }
        // egui only reports the cursor while focused; the stored state
        // keeps the selection while toolbar menus have focus.
        let selection = out.cursor_range.or_else(|| out.state.cursor.char_range());
        self.kept_selection = selection.map(|r| (chapter_id, r));

        let mut cursor = out.cursor_range;
        if out.response.changed() {
            let caret = cursor.map(|r| r.primary.index);
            if let Some(change) = edit::diff(&old_text, text, caret) {
                let new_styles = edit::apply_typing(styles, change, insert_style(change), text, literal);
                *styles = new_styles;
                self.dirty = true;
                // Enter in a list continues it (or ends it, on an empty item).
                if !literal
                    && change.removed == 0
                    && change.inserted == 1
                    && let Some((e, caret)) = blocks::on_enter_styled(text, change.at, styles)
                {
                    apply_edit(text, styles, &e);
                    let r = egui::text_selection::CCursorRange::one(egui::text::CCursor::new(caret));
                    out.state.cursor.set_char_range(Some(r));
                    out.state.clone().store(ui.ctx(), body_id);
                    cursor = Some(r);
                    self.kept_selection = Some((chapter_id, r));
                }
            }
            self.pending = None;
        }

        // ---- Paint the body: selection, text, decorations, caret.
        let wrap = out.galley.job.wrap.max_width;
        let galley = ui.fonts_mut(|f| f.layout_job(render::body_job(text, styles, wrap)));
        let origin = out.galley_pos;
        let painter = ui.painter().with_clip_rect(out.text_clip_rect);
        let has_focus = out.response.has_focus();

        ui.painter().set(callout_slot, egui::Shape::Vec({
            let mut shapes = render::callout_shapes(&galley, origin, text, wrap, styles);
            shapes.extend(render::code_shapes(&galley, origin, text, styles, wrap));
            shapes
        }));
        let shown = render::display_styles(text, styles);
        let highlights = render::highlight_rects(&galley, origin, &shown)
            .into_iter()
            .map(|(r, c)| egui::Shape::rect_filled(r, 0.0, c))
            .collect();
        ui.painter().set(highlight_slot, egui::Shape::Vec(highlights));

        if let Some(r) = selection.filter(|r| r.primary.index != r.secondary.index) {
            let [lo, hi] = r.sorted_cursors();
            let color = if has_focus { SELECTION } else { SELECTION_INACTIVE };
            let rects = render::selection_rects(&galley, origin, lo.index..hi.index);
            let shapes = rects.into_iter().map(|r| egui::Shape::rect_filled(r, 0.0, color)).collect();
            ui.painter().set(selection_slot, egui::Shape::Vec(shapes));
        }
        if text.is_empty() {
            painter.text(
                origin,
                Align2::LEFT_TOP,
                "Start writing…",
                FontId::proportional(theme::BODY_SIZE),
                FAINT,
            );
        }
        painter.galley(origin, galley.clone(), TEXT);
        render::paint_decorations(&painter, &galley, origin, &shown);
        let checks = render::paint_blocks(&painter, &galley, origin, text, styles);

        // Clicking a checkbox ticks it.
        let hover_check = |p: Option<egui::Pos2>| p.and_then(|p| checks.iter().find(|(r, _)| r.contains(p)).map(|c| c.1));
        if out.response.hovered() && hover_check(ui.input(|i| i.pointer.hover_pos())).is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if out.response.clicked()
            && let Some(at) = hover_check(out.response.interact_pointer_pos())
        {
            let byte = text.char_indices().nth(at).map(|(b, _)| b);
            if let Some(b) = byte {
                let mark = if &text[b..b + 1] == " " { "x" } else { " " };
                text.replace_range(b..b + 1, mark);
                self.dirty = true;
                ui.ctx().request_repaint();
            }
        }

        if has_focus && let Some(r) = cursor {
            let now = ui.input(|i| i.time);
            if self.caret_seen != Some(r) {
                self.caret_seen = Some(r);
                self.caret_moved_at = now;
            }
            // Solid while typing / moving, then blink.
            let t = (now - self.caret_moved_at).max(0.0);
            let phase = t % 1.0;
            let window_focused = ui.input(|i| i.focused);
            if window_focused && (t < 0.5 || phase < 0.5) {
                let caret = render::caret_rect(&galley, origin, r.primary);
                // The stroke is centered on the insertion point. Allow its
                // outer half past the text edge, within the viewport clip.
                let caret_painter = ui.painter().with_clip_rect(out.text_clip_rect.expand2(vec2(2.0, 0.0)));
                caret_painter.line_segment([caret.center_top(), caret.center_bottom()], Stroke::new(2.0_f32, ACCENT));
            }
            if window_focused {
                let next = if t < 0.5 { 0.5 - t } else { 0.5 - (phase % 0.5) };
                ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(next.max(0.016)));
            }
        }

        if let (Some(p), Some(r)) = (self.pending, cursor)
            && has_focus
            && (r.primary.index != p.at || r.secondary.index != p.at)
        {
            self.pending = None;
        }

        let body = out.response.clone();
        self.body_context_menu(ui.ctx(), &body);
        ui.add_space(120.0);
    }

    /// The floating formatting bar, vertically centered on the right edge.
    fn toolbar(&mut self, ctx: &egui::Context) {
        let active = self.active_style(&self.selection(ctx));
        let kind_now = self.current_block(ctx);
        let callout_now = self.current_callout(ctx);
        let mut op = None;
        let mut block = None;
        let mut callout: Option<Option<String>> = None;
        let code_view = self.code_view;
        let mut toggle_view = false;
        let mut code_language = None;
        const SIDE: [egui::RectAlign; 2] = [egui::RectAlign::LEFT, egui::RectAlign::LEFT_START];

        egui::Area::new(Id::new("hz-toolbar"))
            .anchor(Align2::RIGHT_CENTER, vec2(-20.0, 0.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(SURFACE)
                    .stroke(Stroke::new(1.0_f32, BORDER))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::same(4))
                    .shadow(egui::Shadow {
                        offset: [0, 6],
                        blur: 18,
                        spread: 0,
                        color: Color32::from_black_alpha(70),
                    })
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
                        // Formatting only applies to the normal view.
                        if !code_view {
                            let level = match kind_now {
                                Kind::Heading(l) => l,
                                _ => 0,
                            };
                            let resp = tool_button(ui, level > 0, &ToolbarMenu::Heading.hint(), |p, r, col| {
                                let font = FontId::new(15.0, theme::family(true, false));
                                let label = if level > 0 { format!("H{level}") } else { "H".into() };
                                p.text(r.center(), Align2::CENTER_CENTER, label, font, col);
                            });
                            egui::Popup::menu(&resp)
                                .id(ToolbarMenu::Heading.id())
                                .align(egui::RectAlign::LEFT_END)
                                .align_alternatives(&SIDE)
                                .gap(6.0)
                                .show(|ui| {
                                    if let Some(l) = heading_picker(ui, level) {
                                        block = Some(BlockOp::Heading(l));
                                    }
                                });
                            list_buttons(ui, kind_now, &mut block);
                            let resp = tool_button(ui, false, "Insert code block  (Ctrl+Alt+K)", |p, r, col| {
                                p.text(r.center(), Align2::CENTER_CENTER, "{ }", FontId::monospace(14.0), col);
                            });
                            egui::Popup::menu(&resp).align(egui::RectAlign::LEFT_END).align_alternatives(&SIDE).gap(6.0).show(|ui| {
                                ui.label(RichText::new("Code language").color(MUTED));
                                for (label, language) in [("Plain text", ""), ("Rust", "rust"), ("Python", "python"), ("JavaScript", "js"), ("TypeScript", "ts"), ("JSON", "json"), ("HTML", "html"), ("CSS", "css"), ("Bash", "bash"), ("SQL", "sql"), ("C / C++", "cpp")] {
                                    if ui.button(label).clicked() {
                                        code_language = Some(language);
                                        ui.close();
                                    }
                                }
                            });
                            let resp = tool_button(ui, callout_now.is_some(), "Callout  (Ctrl+Shift+Q)", |p, r, col| {
                                let spec = callout_now.as_deref().filter(|s| !s.is_empty());
                                paint_callout_icon(p, r, col, spec);
                            });
                            egui::Popup::menu(&resp)
                                .align(egui::RectAlign::LEFT_END)
                                .align_alternatives(&SIDE)
                                .gap(6.0)
                                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                                .show(|ui| {
                                    if let Some(c) = callout_picker(ui, callout_now.as_deref()) {
                                        callout = Some(c);
                                    }
                                });
                            separator_h(ui);
                            attr_buttons(ui, &active, &mut op);
                            let kind = active.underline;
                            let resp = tool_button(ui, false, &ToolbarMenu::Underline.hint(), |p, r, col| {
                                let shown = kind.unwrap_or(LineStyle::Wavy);
                                render::paint_line(p, r.center().x - 8.0, r.center().x + 8.0, r.center().y, col, shown);
                            });
                            egui::Popup::menu(&resp)
                                .id(ToolbarMenu::Underline.id())
                                .align(egui::RectAlign::LEFT_END)
                                .align_alternatives(&SIDE)
                                .gap(6.0)
                                .show(|ui| {
                                    if let Some(k) = underline_picker(ui, kind) {
                                        op = Some(FormatOp::Underline(k));
                                    }
                                });
                            separator_h(ui);

                            let fg = active.fg;
                            let resp = tool_button(ui, false, &ToolbarMenu::TextColor.hint(), |p, r, col| paint_color_icon(p, r, col, fg));
                            egui::Popup::menu(&resp)
                                .id(ToolbarMenu::TextColor.id())
                                .align(egui::RectAlign::LEFT_END)
                                .align_alternatives(&SIDE)
                                .gap(6.0)
                                .show(|ui| {
                                    if let Some(c) = swatches(ui, &theme::TEXT_COLORS, fg, false) {
                                        op = Some(FormatOp::Fg(c));
                                    }
                                });

                            let bg = active.bg;
                            let resp = tool_button(ui, false, &ToolbarMenu::Highlight.hint(), |p, r, col| paint_highlight_icon(p, r, col, bg));
                            egui::Popup::menu(&resp)
                                .id(ToolbarMenu::Highlight.id())
                                .align(egui::RectAlign::LEFT_END)
                                .align_alternatives(&SIDE)
                                .gap(6.0)
                                .show(|ui| {
                                    if let Some(c) = swatches(ui, &theme::HIGHLIGHTS, bg, true) {
                                        op = Some(FormatOp::Bg(c));
                                    }
                                });

                            separator_h(ui);
                            clear_button(ui, &mut op);
                            separator_h(ui);
                        }
                        if !code_view {
                            let literal = self.settings.literal_text;
                            let tip = if literal {
                                "Literal typing: on — escape newly typed formatting characters  (Ctrl+Shift+M)"
                            } else {
                                "Literal typing: off — interpret newly typed markers  (Ctrl+Shift+M)"
                            };
                            if tool_button(ui, literal, tip, |p, r, col| {
                                p.text(r.center(), Align2::CENTER_CENTER, "Aa", FontId::proportional(14.0), col);
                            }).clicked() {
                                self.settings.literal_text = !literal;
                                self.settings.save();
                                ctx.memory_mut(|m| m.request_focus(self.editor_id()));
                            }
                        }
                        let tip = if code_view { "Back to normal view  (Ctrl+/)" } else { "Code view: edit the formatting as text  (Ctrl+/)" };
                        if tool_button(ui, code_view, tip, paint_code_icon).clicked() {
                            toggle_view = true;
                        }
                    });
            });

        if let Some(language) = code_language {
            self.insert_code_block(ctx, language);
        }
        if toggle_view {
            self.set_code_view(ctx, !code_view);
        }
        if let Some(op) = op {
            self.apply_format(ctx, op);
        }
        if let Some(b) = block {
            self.apply_block(ctx, b);
        }
        if let Some(c) = callout {
            let custom = c.as_deref().is_some_and(|s| s.starts_with('#'));
            self.apply_callout(ctx, c.as_deref());
            if !custom {
                ui_close_menus(ctx);
            }
        }
    }

    /// Right-click menu on the page body: the same controls, colors inline.
    fn body_context_menu(&mut self, ctx: &egui::Context, body: &Response) {
        let active = self.active_style(&self.selection(ctx));
        let kind_now = self.current_block(ctx);
        let callout_now = self.current_callout(ctx);
        let mut reset = None;
        let mut op = None;
        let mut block = None;
        let mut callout: Option<Option<String>> = None;
        egui::Popup::context_menu(body)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                ui.horizontal(|ui| {
                    for l in 1..=3u8 {
                        let on = kind_now == Kind::Heading(l);
                        let tip = format!("Heading {l}  (Ctrl+Alt+{l})");
                        let resp = tool_button(ui, on, &tip, |p, r, col| {
                            let font = FontId::new(14.0, theme::family(true, false));
                            p.text(r.center(), Align2::CENTER_CENTER, format!("H{l}"), font, col);
                        });
                        if resp.clicked() {
                            block = Some(BlockOp::Heading(l));
                        }
                    }
                    separator(ui);
                    list_buttons(ui, kind_now, &mut block);
                });
                ui.horizontal(|ui| {
                    attr_buttons(ui, &active, &mut op);
                    separator(ui);
                    clear_button(ui, &mut op);
                });
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(2.0);
                if let Some(k) = underline_picker(ui, active.underline) {
                    op = Some(FormatOp::Underline(k));
                }
                ui.add_space(6.0);
                if let Some(c) = swatches(ui, &theme::TEXT_COLORS, active.fg, false) {
                    op = Some(FormatOp::Fg(c));
                }
                ui.add_space(6.0);
                if let Some(c) = swatches(ui, &theme::HIGHLIGHTS, active.bg, true) {
                    op = Some(FormatOp::Bg(c));
                }
                ui.add_space(6.0);
                if let Some(c) = callout_picker(ui, callout_now.as_deref()) {
                    callout = Some(c);
                }
                ui.add_space(4.0);
                ui.separator();
                for (r, label, hint) in [
                    (Reset::Inline, "Clear text formatting", "Ctrl+Space"),
                    (Reset::Lines, "Normal text", "Ctrl+Shift+Space"),
                    (Reset::All, "Clear all formatting", "Ctrl+Shift+\\"),
                ] {
                    if menu_item(ui, label, hint) {
                        reset = Some(r);
                    }
                }
            });
        if let Some(r) = reset {
            self.reset_formatting(ctx, r);
            ui_close_menus(ctx);
        }
        if let Some(op) = op {
            self.apply_format(ctx, op);
        }
        if let Some(b) = block {
            self.apply_block(ctx, b);
            ui_close_menus(ctx);
        }
        if let Some(c) = callout {
            // Custom colors are dragged in a picker: keep the menu open.
            let custom = c.as_deref().is_some_and(|s| s.starts_with('#'));
            self.apply_callout(ctx, c.as_deref());
            if !custom {
                ui_close_menus(ctx);
            }
        }
    }

    // ------------------------------------------------------------ companion

    fn set_pet(&mut self, species: Option<usize>) {
        self.settings.pet = species.map(|i| pets::SPECIES[i].id.to_owned());
        self.settings.save();
        self.pet = species.map(Pet::new);
    }

    /// The companion walks along the bottom edge of the window, above the
    /// panels but below the toolbar, menus and dialogs.
    fn paint_pet(&mut self, ctx: &egui::Context) {
        let Some(pet) = &mut self.pet else { return };
        // Walk along the bottom edge of the window.
        let area = ctx.content_rect().shrink(1.0);
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, Id::new("hz-pet")));
        pet.update_and_paint(ctx, &painter, &mut self.sprites, area);
    }

    fn settings_ui(&mut self, ui: &mut Ui, ctx: &egui::Context, keep: &mut bool) {
        ui.set_width(530.0);
        ui.label(RichText::new("Settings").size(17.0).family(theme::medium()).color(TEXT));
        ui.add_space(16.0);
        if ui.checkbox(&mut self.settings.literal_text, "Type literal text in normal mode  (Ctrl+Shift+M)").changed() {
            self.settings.save();
        }
        ui.label(RichText::new("Escape newly typed formatting characters with a backslash. Existing text is unaffected.").size(13.0).color(MUTED));
        ui.add_space(16.0);
        let g = spaced_label(ui, "COMPANION", 10.0, FAINT);
        ui.label(g);
        ui.add_space(2.0);
        ui.label(RichText::new("A little friend that wanders along the bottom of the window.").size(13.0).color(MUTED));
        ui.add_space(10.0);

        let current = self.pet.as_ref().map(|p| p.species);
        let mut picked: Option<Option<usize>> = None;
        let choices: Vec<Option<usize>> = std::iter::once(None).chain((0..pets::SPECIES.len()).map(Some)).collect();
        for row in choices.chunks(7) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for &choice in row {
                    let (rect, resp) = ui.allocate_exact_size(vec2(72.0, 82.0), Sense::click());
                    let selected = choice == current;
                    let p = ui.painter();
                    if selected {
                        p.rect_filled(rect, 10.0, ACCENT.gamma_multiply(0.12));
                        p.rect_stroke(rect, 10.0, Stroke::new(1.5_f32, ACCENT), egui::StrokeKind::Inside);
                    } else if resp.hovered() {
                        p.rect_filled(rect, 10.0, HOVER);
                    }
                    let foot = pos2(rect.center().x, rect.top() + 58.0);
                    let name = match choice {
                        Some(i) => {
                            pets::thumbnail(ctx, ui.painter(), &mut self.sprites, i, foot);
                            pets::SPECIES[i].name
                        }
                        None => {
                            let c = pos2(rect.center().x, rect.top() + 36.0);
                            let s = Stroke::new(1.3_f32, FAINT);
                            ui.painter().circle_stroke(c, 11.0, s);
                            ui.painter().line_segment([c + vec2(-7.5, 7.5), c + vec2(7.5, -7.5)], s);
                            "None"
                        }
                    };
                    ui.painter().text(
                        pos2(rect.center().x, rect.bottom() - 12.0),
                        Align2::CENTER_CENTER,
                        name,
                        FontId::proportional(11.5),
                        if selected { TEXT } else { MUTED },
                    );
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        picked = Some(choice);
                    }
                }
            });
        }
        if let Some(choice) = picked {
            self.set_pet(choice);
        }

        ui.add_space(18.0);
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if primary_button(ui, "Done").clicked() {
                    *keep = false;
                }
            });
        });
    }

    // ------------------------------------------------------------ modals

    fn modals(&mut self, ctx: &egui::Context) {
        let Some(mut modal) = self.modal.take() else { return };
        let mut keep = true;
        let frame = Frame::new()
            .fill(SURFACE)
            .stroke(Stroke::new(1.0_f32, BORDER))
            .corner_radius(CornerRadius::same(14))
            .inner_margin(Margin::same(22))
            .shadow(ctx.style().visuals.window_shadow);

        let resp = egui::Modal::new(Id::new("hz-modal"))
            .frame(frame)
            .backdrop_color(Color32::from_black_alpha(140))
            .show(ctx, |ui| {
                ui.set_width(340.0);
                match &mut modal {
                    Modal::About { show_license } => {
                        about(ui, show_license, &mut keep);
                    }
                    Modal::Settings => {
                        self.settings_ui(ui, ctx, &mut keep);
                    }
                    Modal::Create { name, focus } => {
                        ui.label(RichText::new("New notebook").size(17.0).family(theme::medium()).color(TEXT));
                        ui.add_space(4.0);
                        ui.label(RichText::new("Give it a name. You'll choose where to save it next.").color(MUTED));
                        ui.add_space(14.0);
                        let input = Frame::new()
                            .fill(BG)
                            .stroke(Stroke::new(1.0_f32, BORDER))
                            .corner_radius(CornerRadius::same(8))
                            .inner_margin(Margin::symmetric(12, 9))
                            .show(ui, |ui| {
                                ui.add(
                                    TextEdit::singleline(name)
                                        .font(FontId::proportional(15.0))
                                        .text_color(TEXT)
                                        .hint_text(RichText::new("Untitled notebook").color(FAINT))
                                        .frame(false)
                                        .margin(Margin::ZERO)
                                        .desired_width(f32::INFINITY),
                                )
                            })
                            .inner;
                        if *focus {
                            input.request_focus();
                            *focus = false;
                        }
                        let submitted = input.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                        ui.add_space(18.0);
                        ui.horizontal(|ui| {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if primary_button(ui, "Create").clicked() || submitted {
                                    let n = name.clone();
                                    if self.create_notebook(ctx, &n) {
                                        keep = false;
                                    }
                                }
                                if ui.button("Cancel").clicked() {
                                    keep = false;
                                }
                            });
                        });
                    }
                    Modal::Unsaved(after) => {
                        ui.label(RichText::new("Save changes?").size(17.0).family(theme::medium()).color(TEXT));
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(format!(
                                "“{}” has unsaved changes. They will be lost if you don't save them.",
                                if self.nb.title.trim().is_empty() { "Untitled notebook" } else { self.nb.title.trim() }
                            ))
                            .color(MUTED),
                        );
                        ui.add_space(18.0);
                        ui.horizontal(|ui| {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if primary_button(ui, "Save").clicked() {
                                    keep = false;
                                    if self.save(ctx) {
                                        self.proceed(ctx, after.clone());
                                    }
                                }
                                if ui.button("Don't save").clicked() {
                                    keep = false;
                                    if let Some(p) = &self.path {
                                        autosave::remove(p);
                                    }
                                    self.dirty = false;
                                    self.proceed(ctx, after.clone());
                                }
                                if ui.button("Cancel").clicked() {
                                    keep = false;
                                }
                            });
                        });
                    }
                    Modal::Recover { path, saved_at } => {
                        ui.label(RichText::new("Recover unsaved changes?").size(17.0).family(theme::medium()).color(TEXT));
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(format!(
                                "“{}” has changes that were never saved. Hazelnut may have closed unexpectedly. \
                                 A recovered copy was autosaved {}.",
                                file_name(path),
                                library::relative_time(*saved_at),
                            ))
                            .color(MUTED),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new("Opening the saved version deletes the recovered copy.")
                                .size(12.5)
                                .color(FAINT),
                        );
                        ui.add_space(18.0);
                        ui.horizontal(|ui| {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let now = Self::now(ctx);
                                if primary_button(ui, "Open recovered").clicked() {
                                    keep = false;
                                    self.load_recovered(path, now);
                                }
                                if ui.button("Open saved version").clicked() {
                                    keep = false;
                                    autosave::remove(path);
                                    self.load(path, now);
                                }
                                if ui.button("Cancel").clicked() {
                                    keep = false;
                                }
                            });
                        });
                    }
                    Modal::Category { id, name, color, assign, focus } => {
                        let editing = self.nb.category(*id).map(|c| c.id);
                        let heading = if editing.is_some() { "Edit category" } else { "New category" };
                        ui.label(RichText::new(heading).size(17.0).family(theme::medium()).color(TEXT));
                        ui.add_space(14.0);
                        let input = Frame::new()
                            .fill(BG)
                            .stroke(Stroke::new(1.0_f32, BORDER))
                            .corner_radius(CornerRadius::same(8))
                            .inner_margin(Margin::symmetric(12, 9))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                                    paint_tag(ui.painter(), r.center(), vec2(15.0, 10.0), Some(*color), Stroke::NONE);
                                    ui.add(
                                        TextEdit::singleline(name)
                                            .font(FontId::proportional(15.0))
                                            .text_color(TEXT)
                                            .hint_text(RichText::new("Category name").color(FAINT))
                                            .frame(false)
                                            .margin(Margin::ZERO)
                                            .desired_width(f32::INFINITY),
                                    )
                                })
                                .inner
                            })
                            .inner;
                        if *focus {
                            input.request_focus();
                            *focus = false;
                        }
                        let submitted = input.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));

                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            for c in category_palette() {
                                let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
                                let p = ui.painter();
                                if resp.hovered() {
                                    p.rect_filled(rect, 7.0, HOVER);
                                }
                                p.circle_filled(rect.center(), 8.0, c);
                                if c == *color {
                                    p.rect_stroke(rect.shrink(1.0), 7.0, Stroke::new(1.5_f32, ACCENT), egui::StrokeKind::Inside);
                                }
                                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    *color = c;
                                }
                            }
                        });

                        let trimmed = name.trim();
                        let duplicate = self.nb.categories.iter().any(|c| c.name == trimmed && Some(c.id) != editing);
                        ui.add_space(6.0);
                        if duplicate {
                            ui.label(RichText::new("A category with this name already exists.").size(12.5).color(theme::DANGER));
                        }
                        let valid = !trimmed.is_empty() && !duplicate;

                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            if let Some(cid) = editing {
                                let del = egui::Button::new(RichText::new("Delete").color(theme::DANGER)).frame_when_inactive(false);
                                if ui.add(del).on_hover_text("Its chapters are kept, without a category").clicked() {
                                    keep = false;
                                    self.delete_category(cid);
                                }
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let label = if editing.is_some() { "Save" } else { "Create" };
                                let ok = ui.add_enabled(valid, egui::Button::new(RichText::new(label).color(Color32::from_rgb(0x1a, 0x14, 0x0e))).fill(ACCENT));
                                if (ok.clicked() || submitted) && valid {
                                    keep = false;
                                    let name = trimmed.to_owned();
                                    match editing.and_then(|cid| self.nb.category_mut(cid)) {
                                        Some(cat) => {
                                            cat.name = name;
                                            cat.color = *color;
                                            self.dirty = true;
                                        }
                                        None => {
                                            let cat = Category::new(name, *color);
                                            let cid = cat.id;
                                            self.nb.categories.push(cat);
                                            self.dirty = true;
                                            let chapter = assign.and_then(|a| self.nb.chapters.iter().position(|c| c.id == a));
                                            if let Some(i) = chapter {
                                                self.set_category(i, Some(cid));
                                            }
                                        }
                                    }
                                }
                                if ui.button("Cancel").clicked() {
                                    keep = false;
                                }
                            });
                        });
                    }
                    Modal::Categories { focus } => {
                        ui.label(RichText::new("Categories").size(17.0).family(theme::medium()).color(TEXT));
                        ui.add_space(4.0);
                        ui.label(RichText::new("Chapters are grouped by category in the sidebar.").color(MUTED));
                        ui.add_space(14.0);

                        enum Op {
                            Up,
                            Down,
                            Delete,
                        }
                        let mut op = None;
                        let len = self.nb.categories.len();
                        if len == 0 {
                            ui.label(RichText::new("No categories yet.").color(FAINT));
                        }
                        ui.spacing_mut().item_spacing.y = 6.0;
                        for i in 0..len {
                            let cat = &mut self.nb.categories[i];
                            let id = cat.id;
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                // Color: the tag itself, with a palette popup.
                                let (r, resp) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
                                if resp.hovered() {
                                    ui.painter().rect_filled(r, 7.0, HOVER);
                                }
                                paint_tag(ui.painter(), r.center(), vec2(16.0, 11.0), Some(cat.color), Stroke::NONE);
                                let resp = resp.on_hover_text("Color").on_hover_cursor(egui::CursorIcon::PointingHand);
                                egui::Popup::menu(&resp).show(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 4.0;
                                        for c in category_palette() {
                                            let (rect, sw) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
                                            let p = ui.painter();
                                            if sw.hovered() {
                                                p.rect_filled(rect, 7.0, HOVER);
                                            }
                                            p.circle_filled(rect.center(), 8.0, c);
                                            if c == cat.color {
                                                p.rect_stroke(rect.shrink(1.0), 7.0, Stroke::new(1.5_f32, ACCENT), egui::StrokeKind::Inside);
                                            }
                                            if sw.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                                cat.color = c;
                                                self.dirty = true;
                                                ui.close();
                                            }
                                        }
                                    });
                                });

                                let name_w = ui.available_width() - 3.0 * 26.0;
                                let field = Frame::new()
                                    .fill(BG)
                                    .stroke(Stroke::new(1.0_f32, BORDER))
                                    .corner_radius(CornerRadius::same(6))
                                    .inner_margin(Margin::symmetric(8, 5))
                                    .show(ui, |ui| {
                                        ui.add(
                                            TextEdit::singleline(&mut cat.name)
                                                .font(FontId::proportional(14.0))
                                                .text_color(TEXT)
                                                .hint_text(RichText::new("Category name").color(FAINT))
                                                .frame(false)
                                                .margin(Margin::ZERO)
                                                .desired_width(name_w - 18.0),
                                        )
                                    })
                                    .inner;
                                if field.changed() {
                                    self.dirty = true;
                                }
                                if *focus == Some(id) {
                                    field.request_focus();
                                    *focus = None;
                                }

                                if ui.add_enabled_ui(i > 0, |ui| tool_icon(ui, "Move up", |p, r, col| paint_arrow(p, r.center(), true, col))).inner.clicked() {
                                    op = Some((id, Op::Up));
                                }
                                if ui.add_enabled_ui(i + 1 < len, |ui| tool_icon(ui, "Move down", |p, r, col| paint_arrow(p, r.center(), false, col))).inner.clicked() {
                                    op = Some((id, Op::Down));
                                }
                                if icon_button(ui, "×", "Delete (its chapters are kept, without a category)").clicked() {
                                    op = Some((id, Op::Delete));
                                }
                            });
                        }
                        match op {
                            Some((id, Op::Up)) => self.move_category(id, true),
                            Some((id, Op::Down)) => self.move_category(id, false),
                            Some((id, Op::Delete)) => self.delete_category(id),
                            None => {}
                        }

                        let names: Vec<&str> = self.nb.categories.iter().map(|c| c.name.trim()).collect();
                        let duplicate = names.iter().enumerate().any(|(i, n)| !n.is_empty() && names[..i].contains(n));
                        if duplicate {
                            ui.label(RichText::new("Two categories share a name; one will get a number.").size(12.5).color(theme::DANGER));
                        }

                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if text_button(ui, RichText::new("+  Add category").color(MUTED), "New category").clicked() {
                                let used: Vec<Color32> = self.nb.categories.iter().map(|c| c.color).collect();
                                let palette = category_palette();
                                let color = palette.iter().copied().find(|c| !used.contains(c)).unwrap_or(palette[0]);
                                let cat = Category::new("", color);
                                *focus = Some(cat.id);
                                self.nb.categories.push(cat);
                                self.dirty = true;
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if primary_button(ui, "Done").clicked() {
                                    keep = false;
                                }
                            });
                        });
                    }
                    Modal::DeleteChapter(i) => {
                        let name = self.nb.chapters.get(*i).map(|c| c.title.trim()).unwrap_or("");
                        let name = if name.is_empty() { "Untitled" } else { name };
                        ui.label(RichText::new("Delete chapter?").size(17.0).family(theme::medium()).color(TEXT));
                        ui.add_space(6.0);
                        ui.label(RichText::new(format!("“{name}” and its contents will be removed.")).color(MUTED));
                        ui.add_space(18.0);
                        ui.horizontal(|ui| {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let del = egui::Button::new(RichText::new("Delete").color(Color32::WHITE))
                                    .fill(theme::DANGER.gamma_multiply(0.8));
                                if ui.add(del).clicked() {
                                    keep = false;
                                    self.delete_chapter(*i);
                                }
                                if ui.button("Cancel").clicked() {
                                    keep = false;
                                }
                            });
                        });
                    }
                }
            });
        if resp.should_close() || ctx.input(|i| i.key_pressed(Key::Escape)) {
            keep = false;
        }
        if !keep && matches!(modal, Modal::Categories { .. }) {
            self.tidy_category_names();
        }
        if keep && self.modal.is_none() {
            self.modal = Some(modal);
        }
    }
}

impl eframe::App for HazelnutApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_window_events(ctx);
        self.handle_shortcuts(ctx);
        self.window_resize(ctx);
        self.title_bar(ctx);
        match self.screen {
            Screen::Home => self.home(ctx),
            Screen::Editor => {
                self.sidebar(ctx);
                self.status_bar(ctx);
                self.central(ctx);
                self.toolbar(ctx);
                if let Some(c) = self.pending_category.take() {
                    self.set_category(self.current, c);
                }
            }
        }
        self.modals(ctx);
        self.paint_pet(ctx);
        self.autosave_tick(ctx);
        self.window_border(ctx);
    }
}

// ---------------------------------------------------------------- widgets

/// Keep the title and its category pill sized to the edited text in the same frame.
fn chapter_title(ui: &mut Ui, text: &mut String, id: Id, max_width: f32) -> Response {
    let font = FontId::new(theme::TITLE_SIZE, theme::family(true, false));
    let width = |ui: &Ui, text: &str| {
        let shown = if text.is_empty() { "Untitled" } else { text };
        let text_width = ui.painter().layout_no_wrap(shown.to_owned(), font.clone(), TEXT).size().x;
        (text_width + 6.0).min(max_width)
    };
    let before = width(ui, text);
    let response = ui.add(
        TextEdit::singleline(text)
            .id(id)
            .font(font.clone())
            .text_color(TEXT)
            .hint_text(RichText::new("Untitled").color(FAINT))
            .frame(false)
            .margin(Margin::ZERO)
            .desired_width(before),
    );
    if response.changed() && before != width(ui, text) {
        // TextEdit processes input after allocating its rectangle. Discard the
        // stale layout so a growing title never briefly scrolls inside it.
        ui.ctx().request_discard("chapter title width changed");
    }
    response
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Small-caps style label with letter spacing.
fn spaced_label(ui: &Ui, text: &str, size: f32, color: Color32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: FontId::proportional(size),
            color,
            extra_letter_spacing: 1.8,
            ..Default::default()
        },
    );
    ui.fonts_mut(|f| f.layout_job(job))
}

/// A star toggle. Filled when `starred`; the outline only shows when `show_outline`.
fn star_button(ui: &mut Ui, starred: bool, show_outline: bool, size: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    let p = ui.painter();
    let hovered = resp.hovered();
    if hovered && size > 20.0 {
        p.rect_filled(rect.shrink(1.0), 7.0, HOVER);
    }
    let r = (size * 0.27).min(7.0);
    if starred {
        paint_star(p, rect.center(), r, Some(ACCENT), Stroke::new(1.0_f32, ACCENT));
    } else if show_outline || hovered {
        let col = if hovered { TEXT } else { FAINT };
        paint_star(p, rect.center(), r, None, Stroke::new(1.2_f32, col));
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn paint_star(p: &egui::Painter, c: egui::Pos2, r: f32, fill: Option<Color32>, stroke: Stroke) {
    let pts: Vec<egui::Pos2> = (0..10)
        .map(|k| {
            let a = (-90.0_f32 + k as f32 * 36.0).to_radians();
            let rad = if k % 2 == 0 { r } else { r * 0.45 };
            c + vec2(a.cos(), a.sin()) * rad
        })
        .collect();
    if let Some(f) = fill {
        // A star isn't convex: fill the inner pentagon plus the five tips.
        let inner: Vec<_> = (0..5).map(|i| pts[i * 2 + 1]).collect();
        p.add(egui::Shape::convex_polygon(inner, f, Stroke::NONE));
        for i in 0..5 {
            let tip = pts[i * 2];
            let a = pts[(i * 2 + 9) % 10];
            let b = pts[i * 2 + 1];
            p.add(egui::Shape::convex_polygon(vec![a, tip, b], f, Stroke::NONE));
        }
    }
    p.add(egui::Shape::closed_line(pts, stroke));
}

fn attr_buttons(ui: &mut Ui, active: &Style, op: &mut Option<FormatOp>) {
    let glyph = |bold: bool, italic: bool| FontId::new(15.0, theme::family(bold, italic));
    let attrs = [
        (Attr::Bold, "B", glyph(true, false), "Bold  (Ctrl+B)"),
        (Attr::Italic, "I", glyph(false, true), "Italic  (Ctrl+I)"),
        (Attr::Underline, "U", glyph(false, false), "Underline  (Ctrl+U)"),
        (Attr::Strike, "S", glyph(false, false), "Strikethrough  (Ctrl+Shift+X)"),
    ];
    for (attr, label, font, tip) in attrs {
        let resp = tool_button(ui, attr.get(active), tip, |p, r, col| {
            let g = p.layout_no_wrap(label.into(), font.clone(), col);
            let pos = r.center() - g.size() / 2.0;
            let text_rect = Rect::from_min_size(pos, g.size());
            p.galley(pos, g, col);
            let stroke = Stroke::new(1.4_f32, col);
            match attr {
                Attr::Underline => {
                    let y = text_rect.bottom() - 2.0;
                    p.line_segment([pos2(text_rect.left(), y), pos2(text_rect.right(), y)], stroke);
                }
                Attr::Strike => {
                    let y = text_rect.center().y + 0.5;
                    p.line_segment([pos2(text_rect.left() - 1.5, y), pos2(text_rect.right() + 1.5, y)], stroke);
                }
                _ => {}
            }
        });
        if resp.clicked() {
            *op = Some(FormatOp::Toggle(attr));
        }
    }
}

/// Replace chars per `e` in `text`, keeping `styles` in sync (inserted
/// marker chars get no style).
fn apply_edit(text: &mut String, styles: &mut Vec<Style>, e: &blocks::Edit) {
    let byte = |i: usize| text.char_indices().nth(i).map_or(text.len(), |(b, _)| b);
    let (from, to) = (byte(e.at), byte(e.at + e.removed));
    text.replace_range(from..to, &e.insert);
    let change = edit::Change { at: e.at, removed: e.removed, inserted: e.insert.chars().count() };
    *styles = edit::apply(styles, change, Some(Style::default()));
}

fn ui_close_menus(ctx: &egui::Context) {
    egui::Popup::close_all(ctx);
}

/// "Heading 1/2/3" and "Normal text" choices. Returns the picked level (0 = normal).
fn heading_picker(ui: &mut Ui, current: u8) -> Option<u8> {
    ui.set_min_width(180.0);
    let mut picked = None;
    for l in [1u8, 2, 3, 0] {
        let (label, font) = match l {
            0 => ("Normal text".to_owned(), FontId::proportional(14.0)),
            l => (format!("Heading {l}"), FontId::new(render::heading_size(l) * 0.85, theme::family(true, false))),
        };
        let hint = format!("Ctrl+Alt+{l}");
        let col = if l == current { ACCENT } else { TEXT };
        let btn = egui::Button::new(RichText::new(label).font(font).color(col)).shortcut_text(RichText::new(hint).color(FAINT));
        if ui.add(btn).clicked() {
            picked = Some(l);
        }
    }
    picked
}

/// `</>`: the code view toggle.
fn paint_code_icon(p: &egui::Painter, r: Rect, col: Color32) {
    let c = r.center();
    let s = Stroke::new(1.5_f32, col);
    p.line_segment([c + vec2(-4.0, -5.0), c + vec2(-9.0, 0.0)], s);
    p.line_segment([c + vec2(-9.0, 0.0), c + vec2(-4.0, 5.0)], s);
    p.line_segment([c + vec2(4.0, -5.0), c + vec2(9.0, 0.0)], s);
    p.line_segment([c + vec2(9.0, 0.0), c + vec2(4.0, 5.0)], s);
    p.line_segment([c + vec2(1.5, -6.0), c + vec2(-1.5, 6.0)], s);
}

/// Callout colors: the palette names, as written in `> [!name]`.
fn callout_specs() -> Vec<String> {
    theme::TEXT_COLORS.iter().filter(|(_, c)| c.is_some()).map(|(n, _)| n.to_ascii_lowercase()).collect()
}

/// A tiny callout: tinted box with an accent bar.
fn paint_callout_icon(p: &egui::Painter, r: Rect, col: Color32, spec: Option<&str>) {
    let bx = Rect::from_center_size(r.center(), vec2(18.0, 14.0));
    let (bar, tint) = match spec {
        Some(_) => render::callout_colors(spec),
        None => (col, col.gamma_multiply(0.18)),
    };
    p.rect_filled(bx, 3.0, tint);
    p.rect_filled(Rect::from_min_size(bx.min, vec2(2.5, bx.height())), egui::CornerRadius { nw: 3, sw: 3, ne: 0, se: 0 }, bar);
    for (k, w) in [(0, 9.0), (1, 6.0)] {
        let y = bx.top() + 5.0 + 4.0 * k as f32;
        p.line_segment([pos2(bx.left() + 5.5, y), pos2(bx.left() + 5.5 + w, y)], Stroke::new(1.3_f32, col));
    }
}

/// Callout colors to choose from, plus "no callout". `current` is the
/// caret's callout spec. Returns `Some(Some(spec))` or `Some(None)` (remove).
fn callout_picker(ui: &mut Ui, current: Option<&str>) -> Option<Option<String>> {
    let mut picked = None;
    ui.label(RichText::new("CALLOUT").size(10.0).color(FAINT).extra_letter_spacing(1.5));
    ui.add_space(2.0);
    let mut specs: Vec<Option<String>> = vec![None, Some(String::new())];
    specs.extend(callout_specs().into_iter().map(Some));
    for row in specs.chunks(6) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for spec in row {
                let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
                let p = ui.painter();
                if resp.hovered() {
                    p.rect_filled(rect, 7.0, HOVER);
                }
                let c = rect.center();
                match spec {
                    None => {
                        p.circle_stroke(c, 7.5, Stroke::new(1.2_f32, MUTED));
                        p.line_segment([c + vec2(-5.0, 5.0), c + vec2(5.0, -5.0)], Stroke::new(1.2_f32, MUTED));
                    }
                    Some(s) => {
                        let (bar, tint) = render::callout_colors(Some(s));
                        let bx = Rect::from_center_size(c, vec2(18.0, 16.0));
                        p.rect_filled(bx, 3.0, tint);
                        let corners = egui::CornerRadius { nw: 3, sw: 3, ne: 0, se: 0 };
                        p.rect_filled(Rect::from_min_size(bx.min, vec2(3.0, bx.height())), corners, bar);
                    }
                }
                let selected = match (spec, current) {
                    (None, None) => true,
                    (Some(s), Some(cur)) => s.eq_ignore_ascii_case(cur),
                    _ => false,
                };
                if selected {
                    p.rect_stroke(rect.shrink(1.0), 7.0, Stroke::new(1.5_f32, ACCENT), egui::StrokeKind::Inside);
                }
                let tip = match spec.as_deref() {
                    None => "No callout".to_owned(),
                    Some("") => "Neutral".to_owned(),
                    Some(s) => {
                        let mut t = s.to_owned();
                        t[..1].make_ascii_uppercase();
                        t
                    }
                };
                if resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    picked = Some(spec.clone());
                }
            }
        });
    }
    ui.add_space(4.0);
    let custom = current.and_then(format::parse_hex);
    let open = custom.is_some();
    egui::CollapsingHeader::new(RichText::new("Custom color").size(12.5).color(MUTED))
        .id_salt("hz-callout-custom")
        .default_open(open)
        .show(ui, |ui| {
            let mut c = custom.unwrap_or(ACCENT);
            if egui::widgets::color_picker::color_picker_color32(ui, &mut c, egui::widgets::color_picker::Alpha::Opaque) {
                picked = Some(Some(format::color_hex(c)));
            }
        });
    picked
}

/// Bullet list and checklist toggle buttons.
fn list_buttons(ui: &mut Ui, kind: Kind, block: &mut Option<BlockOp>) -> bool {
    let mut clicked = false;
    let resp = tool_button(ui, kind == Kind::Bullet, "Bullet list  (Ctrl+Shift+L)", |p, r, col| {
        for k in 0..3 {
            let y = r.center().y - 5.0 + 5.0 * k as f32;
            p.circle_filled(pos2(r.center().x - 6.0, y), 1.6, col);
            p.line_segment([pos2(r.center().x - 2.0, y), pos2(r.center().x + 8.0, y)], Stroke::new(1.4_f32, col));
        }
    });
    if resp.clicked() {
        *block = Some(BlockOp::Bullet);
        clicked = true;
    }
    let resp = tool_button(ui, matches!(kind, Kind::Task(_)), "Checklist  (Ctrl+Shift+K)", |p, r, col| {
        let s = Stroke::new(1.4_f32, col);
        let bx = Rect::from_center_size(r.center() - vec2(4.0, 0.0), vec2(10.0, 10.0));
        p.rect_stroke(bx, 2.5, s, egui::StrokeKind::Inside);
        let (l, c, w) = (bx.left(), bx.center(), bx.width());
        let s2 = Stroke::new(1.3_f32, col);
        p.line_segment([pos2(l + w * 0.25, c.y + w * 0.02), pos2(l + w * 0.44, c.y + w * 0.2)], s2);
        p.line_segment([pos2(l + w * 0.44, c.y + w * 0.2), pos2(l + w * 0.76, c.y - w * 0.18)], s2);
        p.line_segment([pos2(r.center().x + 4.0, r.center().y), pos2(r.center().x + 9.0, r.center().y)], s);
    });
    if resp.clicked() {
        *block = Some(BlockOp::Task);
        clicked = true;
    }
    clicked
}

/// Contents of the About dialog.
fn about(ui: &mut Ui, show_license: &mut bool, keep: &mut bool) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(12.0, 26.0), Sense::hover());
        ui.painter().circle_filled(r.center(), 5.0, ACCENT);
        ui.label(RichText::new("Hazelnut").size(22.0).family(theme::family(true, false)).color(TEXT));
    });
    ui.label(RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).size(12.5).color(FAINT));
    ui.add_space(10.0);
    ui.label(RichText::new("A small, minimalist notebook for rich-text notes.").color(MUTED));
    ui.add_space(14.0);

    let row = |ui: &mut Ui, key: &str, value: &str| {
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(84.0, 20.0), Sense::hover());
            let g = spaced_label(ui, key, 10.0, FAINT);
            ui.painter().galley(pos2(r.left(), r.center().y - g.size().y / 2.0), g, FAINT);
            ui.label(RichText::new(value).size(13.0).color(TEXT));
        });
    };
    row(ui, "AUTHOR", env!("CARGO_PKG_AUTHORS"));
    row(ui, "LICENSE", "MIT");
    row(ui, "FONTS", "Noto Sans · SIL Open Font License 1.1");
    row(ui, "BUILT WITH", "Rust, egui and wgpu");
    row(ui, "PET SPRITES", "EmanuelleDev · emanuelledev.itch.io");

    if *show_license {
        ui.add_space(12.0);
        Frame::new()
            .fill(BG)
            .stroke(Stroke::new(1.0_f32, BORDER))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                    // Re-flow the hard-wrapped license text into paragraphs.
                    let text = include_str!("../LICENSE")
                        .split("\n\n")
                        .map(|para| para.split_whitespace().collect::<Vec<_>>().join(" "))
                        .collect::<Vec<_>>()
                        .join("\n\n");
                    ui.label(RichText::new(text).size(12.0).color(MUTED));
                });
            });
    }

    ui.add_space(18.0);
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if primary_button(ui, "Close").clicked() {
                *keep = false;
            }
            let label = if *show_license { "Hide license" } else { "View license" };
            if ui.button(label).clicked() {
                *show_license = !*show_license;
            }
        });
    });
}

/// A row of underline patterns (plus "none"). Returns the clicked choice.
fn underline_picker(ui: &mut Ui, current: Option<LineStyle>) -> Option<Option<LineStyle>> {
    let mut picked = None;
    let label = spaced_label(ui, "UNDERLINE", 10.0, FAINT);
    ui.label(label);
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let choices = std::iter::once(None).chain(LineStyle::ALL.into_iter().map(Some));
        for choice in choices {
            let tip = choice.map_or("None", LineStyle::name);
            let resp = tool_button(ui, choice == current, tip, |p, r, col| match choice {
                Some(kind) => render::paint_line(p, r.center().x - 9.0, r.center().x + 9.0, r.center().y, col, kind),
                None => {
                    let c = r.center();
                    p.circle_stroke(c, 6.5, Stroke::new(1.2_f32, col));
                    p.line_segment([c + vec2(-4.5, 4.5), c + vec2(4.5, -4.5)], Stroke::new(1.2_f32, col));
                }
            });
            if resp.clicked() {
                picked = Some(choice);
            }
        }
    });
    picked
}

fn clear_button(ui: &mut Ui, op: &mut Option<FormatOp>) {
    let resp = tool_button(ui, false, "Clear text formatting  (Ctrl+\\ or Ctrl+Space)\nNormal text: Ctrl+Shift+Space\nClear all: Ctrl+Shift+\\", |p, r, col| {
        let font = FontId::new(14.0, theme::family(false, true));
        p.text(r.center() - vec2(1.5, 0.0), Align2::CENTER_CENTER, "T", font, col);
        let s = Stroke::new(1.3_f32, col);
        let c = r.center() + vec2(4.5, 4.5);
        p.line_segment([c - vec2(3.0, 3.0), c + vec2(3.0, 3.0)], s);
        p.line_segment([c + vec2(-3.0, 3.0), c + vec2(3.0, -3.0)], s);
    });
    if resp.clicked() {
        *op = Some(FormatOp::Clear);
    }
}

fn paint_color_icon(p: &egui::Painter, r: Rect, col: Color32, fg: Option<Color32>) {
    let font = FontId::new(15.0, theme::medium());
    p.text(r.center() - vec2(0.0, 2.5), Align2::CENTER_CENTER, "A", font, col);
    let bar = Rect::from_center_size(r.center() + vec2(0.0, 8.0), vec2(14.0, 3.0));
    p.rect_filled(bar, 1.5, fg.unwrap_or(TEXT));
}

fn paint_highlight_icon(p: &egui::Painter, r: Rect, col: Color32, bg: Option<Color32>) {
    let font = FontId::new(13.0, theme::medium());
    p.text(r.center() - vec2(0.0, 3.0), Align2::CENTER_CENTER, "ab", font, col);
    let bar = Rect::from_center_size(r.center() + vec2(0.0, 8.0), vec2(16.0, 4.0));
    match bg {
        Some(c) => {
            p.rect_filled(bar, 2.0, theme::highlight_display(c));
        }
        None => {
            p.rect_stroke(bar, 2.0, Stroke::new(1.0_f32, FAINT), egui::StrokeKind::Inside);
        }
    }
}

/// Horizontal divider for the vertical toolbar.
fn separator_h(ui: &mut Ui) {
    let (r, _) = ui.allocate_exact_size(vec2(32.0, 9.0), Sense::hover());
    ui.painter().line_segment(
        [pos2(r.left() + 7.0, r.center().y), pos2(r.right() - 7.0, r.center().y)],
        Stroke::new(1.0_f32, BORDER),
    );
}

fn separator(ui: &mut Ui) {
    let (r, _) = ui.allocate_exact_size(vec2(9.0, 30.0), Sense::hover());
    ui.painter().line_segment(
        [pos2(r.center().x, r.top() + 7.0), pos2(r.center().x, r.bottom() - 7.0)],
        Stroke::new(1.0_f32, BORDER),
    );
}

/// Require a fresh second of hovering for each control, including when moving
/// directly from another tooltip (egui normally skips the delay in that case).
fn delayed_tooltip(response: Response, text: &str) -> Response {
    let timer_id = response.id.with("tooltip-hover-start");
    if !response.hovered() {
        response.ctx.data_mut(|data| data.remove::<f64>(timer_id));
        return response;
    }
    let now = response.ctx.input(|input| input.time);
    let started = response.ctx.data_mut(|data| *data.get_temp_mut_or_insert_with(timer_id, || now));
    let remaining = 1.0 - (now - started);
    if remaining > 0.0 {
        response.ctx.request_repaint_after_secs(remaining as f32);
        response
    } else {
        let mut tooltip = egui::Tooltip::for_enabled(&response);
        tooltip.popup = tooltip.popup.align(egui::RectAlign::LEFT).gap(8.0);
        tooltip.show(|ui| {
            ui.label(text);
        });
        response
    }
}

/// A square toolbar button whose glyph is drawn by `paint(painter, rect, color)`.
fn tool_button(ui: &mut Ui, active: bool, tip: &str, paint: impl FnOnce(&egui::Painter, Rect, Color32)) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(32.0, 30.0), Sense::click());
    let p = ui.painter();
    let hovered = resp.hovered();
    if active {
        p.rect_filled(rect, 7.0, ACCENT.gamma_multiply(0.16));
    } else if hovered {
        p.rect_filled(rect, 7.0, HOVER);
    }
    let col = if active {
        ACCENT
    } else if hovered {
        TEXT
    } else {
        MUTED
    };
    paint(p, rect, col);
    delayed_tooltip(resp, tip).on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn icon_button(ui: &mut Ui, glyph: &str, tip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
    let col = if resp.hovered() {
        ui.painter().rect_filled(rect, 6.0, HOVER);
        TEXT
    } else {
        MUTED
    };
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(17.0), col);
    resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn text_button(ui: &mut Ui, text: RichText, tip: &str) -> Response {
    ui.add(egui::Button::new(text.size(12.5)).frame_when_inactive(false))
        .on_hover_text(tip)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn primary_button(ui: &mut Ui, label: &str) -> Response {
    ui.add(egui::Button::new(RichText::new(label).color(Color32::from_rgb(0x1a, 0x14, 0x0e))).fill(ACCENT))
}

fn menu_item(ui: &mut Ui, label: &str, hint: &str) -> bool {
    ui.add(egui::Button::new(RichText::new(label).color(TEXT)).shortcut_text(RichText::new(hint).color(FAINT)))
        .clicked()
}

/// A chapter in the sidebar. `category` is `None` when the notebook has no
/// categories at all (no tag column), else the chapter's category, if any.
fn chapter_row(ui: &mut Ui, title: &str, category: Option<Option<&Category>>, selected: bool, dragged: bool) -> Response {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 34.0), Sense::click_and_drag());
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, 8.0, SURFACE);
        let bar = Rect::from_min_size(rect.min + vec2(0.0, 10.0), vec2(2.5, rect.height() - 20.0));
        p.rect_filled(bar, 1.5, ACCENT);
    } else if resp.hovered() || dragged {
        p.rect_filled(rect, 8.0, SURFACE.gamma_multiply(0.6));
    }

    let mut text_x = rect.left() + 14.0;
    if let Some(cat) = category {
        if let Some(cat) = cat {
            let c = pos2(rect.left() + 21.0, rect.center().y);
            paint_tag(p, c, vec2(13.0, 9.0), Some(cat.color), Stroke::NONE);
        }
        text_x += 20.0;
    }

    let (label, color) = if title.trim().is_empty() {
        ("Untitled", FAINT)
    } else {
        (title, if selected { TEXT } else { MUTED })
    };
    let color = if dragged { color.gamma_multiply(0.5) } else { color };
    let mut job = egui::text::LayoutJob::simple_singleline(label.to_owned(), FontId::proportional(13.5), color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(rect.right() - 12.0 - text_x);
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    let pos = pos2(text_x, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(pos, galley, color);

    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A tag: a rectangle with a triangular point on its right.
fn paint_tag(p: &egui::Painter, center: egui::Pos2, size: egui::Vec2, fill: Option<Color32>, stroke: Stroke) {
    let (l, r) = (center.x - size.x / 2.0, center.x + size.x / 2.0);
    let (t, b) = (center.y - size.y / 2.0, center.y + size.y / 2.0);
    let tip = size.y * 0.5;
    let pts = vec![pos2(l, t), pos2(r - tip, t), pos2(r, center.y), pos2(r - tip, b), pos2(l, b)];
    p.add(egui::Shape::convex_polygon(pts, fill.unwrap_or(Color32::TRANSPARENT), stroke));
}

/// A small up or down arrow (drawn, since the fonts lack arrow glyphs).
fn paint_arrow(p: &egui::Painter, c: egui::Pos2, up: bool, col: Color32) {
    let s = if up { -1.0 } else { 1.0 };
    let stroke = Stroke::new(1.5_f32, col);
    let tip = c + vec2(0.0, 5.0 * s);
    p.line_segment([c - vec2(0.0, 5.0 * s), tip], stroke);
    p.line_segment([tip, tip - vec2(4.0, 4.0 * s)], stroke);
    p.line_segment([tip, tip - vec2(-4.0, 4.0 * s)], stroke);
}

/// A small cog: a ring with eight teeth.
fn paint_gear(p: &egui::Painter, c: egui::Pos2, col: Color32) {
    let stroke = Stroke::new(1.4_f32, col);
    p.circle_stroke(c, 4.2, stroke);
    p.circle_stroke(c, 1.6, stroke);
    for k in 0..8 {
        let a = k as f32 * std::f32::consts::TAU / 8.0;
        let d = vec2(a.cos(), a.sin());
        p.line_segment([c + d * 4.6, c + d * 6.8], Stroke::new(2.2_f32, col));
    }
}

/// A small square icon button painted by `paint(painter, rect, color)`.
fn tool_icon(ui: &mut Ui, tip: &str, paint: impl FnOnce(&egui::Painter, Rect, Color32)) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
    let col = if !ui.is_enabled() {
        BORDER
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 6.0, HOVER);
        TEXT
    } else {
        MUTED
    };
    paint(ui.painter(), rect, col);
    resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Colors offered for categories.
fn category_palette() -> Vec<Color32> {
    theme::TEXT_COLORS.iter().filter_map(|(_, c)| *c).collect()
}

/// Lists the categories to pick from, plus "none", "new" and "edit".
fn category_menu(ui: &mut Ui, categories: &[Category], current: Option<u64>) -> Option<CategoryPick> {
    ui.set_min_width(180.0);
    let mut pick = None;
    let row = |ui: &mut Ui, cat: Option<&Category>| {
        let w = ui.available_width().max(170.0);
        let (rect, resp) = ui.allocate_exact_size(vec2(w, 26.0), Sense::click());
        let p = ui.painter();
        if resp.hovered() {
            p.rect_filled(rect, 6.0, HOVER);
        }
        let c = pos2(rect.left() + 14.0, rect.center().y);
        let (name, col) = match cat {
            Some(cat) => {
                paint_tag(p, c, vec2(13.0, 9.0), Some(cat.color), Stroke::NONE);
                (if cat.name.trim().is_empty() { "Untitled" } else { cat.name.trim() }, TEXT)
            }
            None => {
                paint_tag(p, c, vec2(13.0, 9.0), None, Stroke::new(1.2_f32, FAINT));
                ("No category", MUTED)
            }
        };
        p.text(pos2(rect.left() + 28.0, rect.center().y), Align2::LEFT_CENTER, name, FontId::proportional(13.5), col);
        if cat.map(|c| c.id) == current {
            let c = pos2(rect.right() - 13.0, rect.center().y);
            let stroke = Stroke::new(1.6_f32, ACCENT);
            p.line_segment([c + vec2(-4.5, 0.0), c + vec2(-1.5, 3.0)], stroke);
            p.line_segment([c + vec2(-1.5, 3.0), c + vec2(4.5, -3.5)], stroke);
        }
        resp.clicked()
    };
    for cat in categories {
        if row(ui, Some(cat)) {
            pick = Some(CategoryPick::Set(Some(cat.id)));
        }
    }
    if row(ui, None) {
        pick = Some(CategoryPick::Set(None));
    }
    ui.separator();
    if ui.button("New category…").clicked() {
        pick = Some(CategoryPick::New);
    }
    if !categories.is_empty() && ui.button("Manage categories…").clicked() {
        pick = Some(CategoryPick::Manage);
    }
    if pick.is_some() {
        ui.close();
    }
    pick
}

/// The category tag shown next to the chapter title.
fn category_pill(ui: &mut Ui, cat: Option<&Category>) -> Response {
    match cat {
        Some(cat) => {
            let name = if cat.name.trim().is_empty() { "Untitled" } else { cat.name.trim() };
            let galley = ui.painter().layout_no_wrap(name.to_owned(), FontId::new(12.5, theme::medium()), cat.color);
            let size = vec2(galley.size().x + 38.0, 26.0);
            let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
            let p = ui.painter();
            let bg = if resp.hovered() { cat.color.gamma_multiply(0.24) } else { cat.color.gamma_multiply(0.14) };
            p.rect_filled(rect, 13.0, bg);
            paint_tag(p, pos2(rect.left() + 16.0, rect.center().y), vec2(13.0, 9.0), Some(cat.color), Stroke::NONE);
            p.galley(pos2(rect.left() + 29.0, rect.center().y - galley.size().y / 2.0), galley, cat.color);
            resp.on_hover_text("Change category").on_hover_cursor(egui::CursorIcon::PointingHand)
        }
        None => tool_icon(ui, "Set category", |p, r, col| {
            let col = if col == TEXT { MUTED } else { FAINT };
            paint_tag(p, r.center(), vec2(14.0, 10.0), None, Stroke::new(1.3_f32, col));
        }),
    }
}

/// A grid of color swatches. Returns `Some(choice)` when one is clicked.
fn swatches(
    ui: &mut Ui,
    colors: &[(&str, Option<Color32>)],
    current: Option<Color32>,
    highlight: bool,
) -> Option<Option<Color32>> {
    let mut picked = None;
    ui.label(
        RichText::new(if highlight { "HIGHLIGHT" } else { "TEXT COLOR" })
            .size(10.0)
            .color(FAINT)
            .extra_letter_spacing(1.5),
    );
    ui.add_space(2.0);
    for row in colors.chunks(5) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (name, c) in row {
                let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
                let p = ui.painter();
                if resp.hovered() {
                    p.rect_filled(rect, 7.0, HOVER);
                }
                let center = rect.center();
                match c {
                    Some(col) if highlight => {
                        let shown = theme::highlight_display(*col);
                        p.rect_filled(Rect::from_center_size(center, vec2(18.0, 18.0)), 5.0, shown);
                    }
                    Some(col) => {
                        p.circle_filled(center, 8.0, *col);
                    }
                    None => {
                        p.circle_stroke(center, 7.5, Stroke::new(1.2_f32, MUTED));
                        p.line_segment([center + vec2(-5.0, 5.0), center + vec2(5.0, -5.0)], Stroke::new(1.2_f32, MUTED));
                    }
                }
                if *c == current {
                    p.rect_stroke(rect.shrink(1.0), 7.0, Stroke::new(1.5_f32, ACCENT), egui::StrokeKind::Inside);
                }
                if resp.on_hover_text(*name).clicked() {
                    picked = Some(*c);
                }
            }
        });
    }
    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    fn title_frame(ctx: &egui::Context, text: &mut String, events: Vec<egui::Event>, max_width: f32) -> Rect {
        let mut rect = Rect::NOTHING;
        let _ = ctx.run(egui::RawInput { events, ..Default::default() }, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.horizontal(|ui| {
                    rect = chapter_title(ui, text, Id::new("test-title"), max_width).rect;
                    ui.label("category");
                });
            });
        });
        rect
    }

    fn select_title(ctx: &egui::Context, text: &str, all: bool) {
        let id = Id::new("test-title");
        let mut state = egui::text_edit::TextEditState::load(ctx, id).unwrap();
        let end = egui::text::CCursor::new(text.chars().count());
        let start = if all { egui::text::CCursor::new(0) } else { end };
        state.cursor.set_char_range(Some(egui::text::CCursorRange::two(start, end)));
        state.store(ctx, id);
        ctx.memory_mut(|m| m.request_focus(id));
    }

    #[test]
    fn chapter_title_edits_have_their_final_width_immediately() {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        let mut text = "i".to_owned();
        title_frame(&ctx, &mut text, vec![], 600.0);
        for (insert, replace) in [("W", false), ("WW", false), ("日本語 title", true), ("", true)] {
            select_title(&ctx, &text, replace);
            let event = if insert.is_empty() {
                egui::Event::Key {
                    key: Key::Backspace,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }
            } else if replace {
                egui::Event::Paste(insert.to_owned())
            } else {
                egui::Event::Text(insert.to_owned())
            };
            let expected = if replace { insert.to_owned() } else { format!("{text}{insert}") };
            let edited = title_frame(&ctx, &mut text, vec![event], 600.0);
            assert_eq!(text, expected, "input must only be applied once");
            let settled = title_frame(&ctx, &mut text, vec![], 600.0);
            assert_eq!(edited, settled, "title width must not jump on the next frame");
        }
    }

    #[test]
    fn chapter_title_stays_capped_and_shrinks_after_replacement() {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        let mut text = "Title".to_owned();
        title_frame(&ctx, &mut text, vec![], 160.0);
        select_title(&ctx, &text, true);
        let long = title_frame(&ctx, &mut text, vec![egui::Event::Paste("W".repeat(100))], 160.0);
        assert!(long.width() <= 160.0);
        assert_eq!(long, title_frame(&ctx, &mut text, vec![], 160.0));
        select_title(&ctx, &text, true);
        let short = title_frame(&ctx, &mut text, vec![egui::Event::Text("i".into())], 160.0);
        assert_eq!(text, "i");
        assert!(short.width() < long.width());
        assert_eq!(short, title_frame(&ctx, &mut text, vec![], 160.0));
    }

    #[test]
    fn toolbar_tooltips_wait_a_second_for_each_button() {
        let ctx = egui::Context::default();
        let mut rects = [Rect::NOTHING; 2];
        let mut frame = |time: f64, pointer: Option<egui::Pos2>| {
            let events = pointer.map(egui::Event::PointerMoved).into_iter().collect();
            let output = ctx.run(egui::RawInput { time: Some(time), events, ..Default::default() }, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.add_space(100.0);
                    ui.horizontal(|ui| {
                        ui.add_space(300.0);
                        ui.vertical(|ui| {
                            for (index, tip) in ["Bold  (Ctrl+B)", "Italic  (Ctrl+I)"].iter().enumerate() {
                                rects[index] = tool_button(ui, false, tip, |_, _, _| {}).rect;
                            }
                        });
                    });
                });
            });
            let labels: Vec<_> = output.shapes.iter().filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.job.text.clone()),
                _ => None,
            }).collect();
            (rects, labels.join("\n"))
        };
        let (rects, _) = frame(0.0, None);
        assert!(frame(1.0, Some(rects[0].center())).1.is_empty());
        assert!(frame(1.9, None).1.is_empty());
        frame(2.01, None); // Let egui measure the newly opened tooltip.
        assert!(frame(2.1, None).1.contains("Bold  (Ctrl+B)"));
        // Moving directly from a visible tooltip must restart the delay.
        assert!(frame(2.2, Some(rects[1].center())).1.is_empty());
        assert!(frame(3.1, None).1.is_empty());
        frame(3.21, None);
        assert!(frame(3.3, None).1.contains("Italic  (Ctrl+I)"));
    }

}
