mod home;
mod titlebar;

use crate::autosave;
use crate::edit;
use crate::format;
use crate::library::{self, Library};
use crate::model::{Attr, Chapter, LineStyle, Notebook, Style};
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
    MoveUp(usize),
    MoveDown(usize),
    Delete(usize),
    Add,
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

    fn select_chapter(&mut self, i: usize) {
        self.current = i.min(self.nb.chapters.len() - 1);
        self.pending = None;
    }

    fn add_chapter(&mut self) {
        let at = self.current + 1;
        self.nb.chapters.insert(at, Chapter::new(""));
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
        match self.pick_save_path("Save notebook", &self.nb.title.clone()) {
            Some(p) => self.write(ctx, p),
            None => false,
        }
    }

    fn pick_save_path(&self, dialog_title: &str, name: &str) -> Option<PathBuf> {
        let suggested: String = name
            .chars()
            .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' })
            .collect();
        let suggested = if suggested.trim().is_empty() { "notebook".into() } else { suggested.trim().to_owned() };
        let mut dialog = rfd::FileDialog::new()
            .set_title(dialog_title)
            .add_filter("Hazelnut notebook", &[format::EXTENSION])
            .set_file_name(format!("{suggested}.{}", format::EXTENSION));
        let last_dir = self.path.clone().or_else(|| self.library.entries.first().map(|e| e.path.clone()));
        if let Some(dir) = last_dir.as_ref().and_then(|p| p.parent()) {
            dialog = dialog.set_directory(dir);
        }
        let mut p = dialog.save_file()?;
        if p.extension().is_none() {
            p.set_extension(format::EXTENSION);
        }
        Some(p)
    }

    /// Create a new notebook file named `name` (asks where to save it).
    fn create_notebook(&mut self, ctx: &egui::Context, name: &str) -> bool {
        let name = if name.trim().is_empty() { "Untitled notebook" } else { name.trim() };
        let Some(path) = self.pick_save_path("Create notebook", name) else { return false };
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
                        if icon_button(ui, "+", "New chapter  (Ctrl+T)").clicked() {
                            action = Some(ChapterAction::Add);
                        }
                    });
                });
                ui.add_space(4.0);

                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    self.file_actions(ui, ctx);
                    ui.add_space(10.0);
                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            let n = self.nb.chapters.len();
                            for i in 0..n {
                                let ch = &self.nb.chapters[i];
                                let resp = chapter_row(ui, i, &ch.title, i == self.current);
                                if resp.clicked() {
                                    action = Some(ChapterAction::Select(i));
                                }
                                resp.context_menu(|ui| {
                                    ui.set_min_width(150.0);
                                    if ui.add_enabled(i > 0, egui::Button::new("Move up")).clicked() {
                                        action = Some(ChapterAction::MoveUp(i));
                                    }
                                    if ui.add_enabled(i + 1 < n, egui::Button::new("Move down")).clicked() {
                                        action = Some(ChapterAction::MoveDown(i));
                                    }
                                    ui.separator();
                                    let del = egui::Button::new(RichText::new("Delete chapter").color(theme::DANGER));
                                    if ui.add_enabled(n > 1, del).clicked() {
                                        action = Some(ChapterAction::Delete(i));
                                    }
                                });
                            }
                        });
                    });
                });

                match action {
                    Some(ChapterAction::Select(i)) => self.select_chapter(i),
                    Some(ChapterAction::Add) => self.add_chapter(),
                    Some(ChapterAction::MoveUp(i)) => {
                        self.nb.chapters.swap(i, i - 1);
                        self.select_chapter(i - 1);
                        self.dirty = true;
                    }
                    Some(ChapterAction::MoveDown(i)) => {
                        self.nb.chapters.swap(i, i + 1);
                        self.select_chapter(i + 1);
                        self.dirty = true;
                    }
                    Some(ChapterAction::Delete(i)) => {
                        if self.nb.chapters[i].text.trim().is_empty() {
                            self.delete_chapter(i);
                        } else {
                            self.modal = Some(Modal::DeleteChapter(i));
                        }
                    }
                    None => {}
                }
            });
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
                    ("Clear formatting", "Ctrl+\\"),
                    ("New chapter", "Ctrl+T"),
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
        let ch = &mut self.nb.chapters[self.current];

        let title = ui.add(
            TextEdit::singleline(&mut ch.title)
                .id(title_id)
                .font(FontId::new(theme::TITLE_SIZE, theme::family(true, false)))
                .text_color(TEXT)
                .hint_text(RichText::new("Untitled").color(FAINT))
                .frame(false)
                .margin(Margin::ZERO)
                .desired_width(f32::INFINITY),
        );
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

        // ---- Body
        let chapter_id = ch.id;
        let pending = self.pending.filter(|p| p.chapter == chapter_id);
        let Chapter { text, styles, .. } = ch;
        let old_text = text.clone();
        let styles_now: &Vec<Style> = styles;
        let insert_style = |change: edit::Change| pending.filter(|p| p.at == change.at).map(|p| p.style);

        let mut layouter = |ui: &Ui, buf: &dyn TextBuffer, wrap: f32| {
            let s = buf.as_str();
            let job = if s == old_text {
                render::body_job(s, styles_now, wrap)
            } else {
                // Mid-edit relayout: derive styles for the new text on the fly.
                let st = match edit::diff(&old_text, s, None) {
                    Some(c) => edit::apply(styles_now, c, insert_style(c)),
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

        // Reserve slots *behind* the text: highlights, then the selection on top of them.
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

        let cursor = out.cursor_range;
        if out.response.changed() {
            let caret = cursor.map(|r| r.primary.index);
            if let Some(change) = edit::diff(&old_text, text, caret) {
                let new_styles = edit::apply(styles, change, insert_style(change));
                *styles = new_styles;
                self.dirty = true;
            }
            self.pending = None;
        }

        // ---- Paint the body: selection, text, decorations, caret.
        let wrap = out.galley.job.wrap.max_width;
        let galley = ui.fonts_mut(|f| f.layout_job(render::body_job(text, styles, wrap)));
        let origin = out.galley_pos;
        let painter = ui.painter().with_clip_rect(out.text_clip_rect);
        let has_focus = out.response.has_focus();

        let highlights = render::highlight_rects(&galley, origin, styles)
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
        render::paint_decorations(&painter, &galley, origin, styles);

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
                painter.line_segment([caret.center_top(), caret.center_bottom()], Stroke::new(2.0_f32, ACCENT));
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

    /// The floating vertical formatting bar on the right, toward the bottom.
    fn toolbar(&mut self, ctx: &egui::Context) {
        let active = self.active_style(&self.selection(ctx));
        let mut op = None;
        const SIDE: [egui::RectAlign; 2] = [egui::RectAlign::LEFT, egui::RectAlign::LEFT_START];

        egui::Area::new(Id::new("hz-toolbar"))
            .anchor(Align2::RIGHT_BOTTOM, vec2(-20.0, -(STATUS_HEIGHT + 20.0)))
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
                        attr_buttons(ui, &active, &mut op);
                        let kind = active.underline;
                        let resp = tool_button(ui, false, "Underline style", |p, r, col| {
                            let shown = kind.unwrap_or(LineStyle::Wavy);
                            render::paint_line(p, r.center().x - 8.0, r.center().x + 8.0, r.center().y, col, shown);
                        });
                        egui::Popup::menu(&resp)
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
                        let resp = tool_button(ui, false, "Text color", |p, r, col| paint_color_icon(p, r, col, fg));
                        egui::Popup::menu(&resp)
                            .align(egui::RectAlign::LEFT_END)
                            .align_alternatives(&SIDE)
                            .gap(6.0)
                            .show(|ui| {
                                if let Some(c) = swatches(ui, &theme::TEXT_COLORS, fg, false) {
                                    op = Some(FormatOp::Fg(c));
                                }
                            });

                        let bg = active.bg;
                        let resp = tool_button(ui, false, "Highlight", |p, r, col| paint_highlight_icon(p, r, col, bg));
                        egui::Popup::menu(&resp)
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
                    });
            });

        if let Some(op) = op {
            self.apply_format(ctx, op);
        }
    }

    /// Right-click menu on the page body: the same controls, colors inline.
    fn body_context_menu(&mut self, ctx: &egui::Context, body: &Response) {
        let active = self.active_style(&self.selection(ctx));
        let mut op = None;
        egui::Popup::context_menu(body)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
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
            });
        if let Some(op) = op {
            self.apply_format(ctx, op);
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
            }
        }
        self.modals(ctx);
        self.paint_pet(ctx);
        self.autosave_tick(ctx);
        self.window_border(ctx);
    }
}

// ---------------------------------------------------------------- widgets

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
    let resp = tool_button(ui, false, "Clear formatting  (Ctrl+\\)", |p, r, col| {
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
    resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand)
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

fn chapter_row(ui: &mut Ui, index: usize, title: &str, selected: bool) -> Response {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 34.0), Sense::click());
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, 8.0, SURFACE);
        let bar = Rect::from_min_size(rect.min + vec2(0.0, 10.0), vec2(2.5, rect.height() - 20.0));
        p.rect_filled(bar, 1.5, ACCENT);
    } else if resp.hovered() {
        p.rect_filled(rect, 8.0, SURFACE.gamma_multiply(0.6));
    }

    let num_col = if selected { ACCENT } else { FAINT };
    p.text(
        pos2(rect.left() + 14.0, rect.center().y),
        Align2::LEFT_CENTER,
        format!("{:02}", index + 1),
        FontId::new(11.0, theme::medium()),
        num_col,
    );

    let (label, color) = if title.trim().is_empty() {
        ("Untitled", FAINT)
    } else {
        (title, if selected { TEXT } else { MUTED })
    };
    let mut job = egui::text::LayoutJob::simple_singleline(label.to_owned(), FontId::proportional(13.5), color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(rect.width() - 50.0);
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    let pos = pos2(rect.left() + 38.0, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(pos, galley, color);

    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
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
