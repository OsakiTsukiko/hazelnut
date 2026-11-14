//! Custom window chrome: title bar, window buttons, edge resizing and border.
//!
//! The native decorations are turned off in `main.rs`.

use super::{After, HazelnutApp, SIDEBAR_WIDTH, Screen, spaced_label};
use crate::theme::{self, BG, BORDER, HOVER, MUTED, SIDEBAR, TEXT};
use eframe::egui::{
    self, Color32, CursorIcon, FontId, Id, LayerId, Order, Pos2, Rect, ResizeDirection, Sense, Stroke, Ui,
    ViewportCommand, pos2, vec2,
};

pub const TITLE_BAR_HEIGHT: f32 = 40.0;
const BUTTON_WIDTH: f32 = 46.0;
/// How close to the window edge (in points) a press starts a resize.
const RESIZE_MARGIN: f32 = 5.0;
const CLOSE_HOVER: Color32 = Color32::from_rgb(0xc4, 0x4b, 0x4b);

impl HazelnutApp {
    pub(super) fn title_bar(&mut self, ctx: &egui::Context) {
        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        let title = match self.screen {
            Screen::Home => "Hazelnut".to_owned(),
            Screen::Editor => {
                let t = self.nb.title.trim();
                if t.is_empty() { "Untitled notebook".to_owned() } else { t.to_owned() }
            }
        };
        let dirty = self.screen == Screen::Editor && self.dirty;

        egui::TopBottomPanel::top("titlebar")
            .exact_height(TITLE_BAR_HEIGHT)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(BG))
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                let p = ui.painter();

                // In the editor, the sidebar color runs up through the title bar.
                if self.screen == Screen::Editor {
                    let side = Rect::from_min_max(rect.min, pos2(rect.left() + SIDEBAR_WIDTH, rect.bottom()));
                    p.rect_filled(side, 0.0, SIDEBAR);
                    p.line_segment(
                        [pos2(side.right() - 0.5, side.top()), pos2(side.right() - 0.5, side.bottom())],
                        Stroke::new(1.0_f32, BORDER),
                    );
                }

                // Drag to move, double-click to maximize.
                let drag = ui.interact(rect, Id::new("hz-titlebar-drag"), Sense::click_and_drag());
                let on_edge = ctx
                    .input(|i| i.pointer.press_origin())
                    .is_some_and(|pos| resize_direction(ctx, pos).is_some());
                if drag.drag_started() && !on_edge {
                    ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                }
                if drag.double_clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
                }

                // Brand in the sidebar part of the bar; doubles as "back to all notebooks".
                if self.screen == Screen::Editor {
                    let r = Rect::from_min_size(pos2(rect.left() + 10.0, rect.top() + 8.0), vec2(150.0, rect.height() - 16.0));
                    let resp = ui.interact(r, Id::new("hz-brand"), Sense::click());
                    let hovered = resp.hovered();
                    if hovered {
                        ui.painter().rect_filled(r, 7.0, HOVER);
                    }
                    let x = r.left() + 6.0;
                    ui.painter().circle_filled(pos2(x + 4.0, r.center().y), 4.0, theme::ACCENT);
                    let label = if hovered { "‹  ALL NOTEBOOKS" } else { "HAZELNUT" };
                    let g = spaced_label(ui, label, 10.5, if hovered { TEXT } else { MUTED });
                    ui.painter().galley(pos2(x + 16.0, r.center().y - g.size().y / 2.0), g, MUTED);
                    if resp
                        .on_hover_text("Back to all notebooks  (Ctrl+W)")
                        .on_hover_cursor(CursorIcon::PointingHand)
                        .clicked()
                    {
                        self.guarded(ctx, After::GoHome);
                    }
                }

                // Left-aligned title (just right of the sidebar in the editor),
                // lined up with the status bar text.
                let content_left = if self.screen == Screen::Editor { rect.left() + SIDEBAR_WIDTH } else { rect.left() };
                let font = FontId::new(12.5, theme::medium());
                let g = ui.fonts_mut(|f| f.layout_no_wrap(title.clone(), font, MUTED));
                let pos = pos2(content_left + 20.0, rect.center().y - g.size().y / 2.0);
                let title_rect = Rect::from_min_size(pos, g.size());
                ui.painter().galley(pos, g, MUTED);
                if dirty {
                    ui.painter().circle_filled(
                        pos2(title_rect.right() + 9.0, title_rect.center().y),
                        3.0,
                        theme::ACCENT,
                    );
                }

                // Window buttons, right to left.
                let mut x = rect.right();
                let mut button = |ui: &mut Ui, id: &str, close: bool, icon: &dyn Fn(&egui::Painter, Pos2, Color32)| {
                    x -= BUTTON_WIDTH;
                    let r = Rect::from_min_size(pos2(x, rect.top()), vec2(BUTTON_WIDTH, rect.height()));
                    let resp = ui.interact(r, Id::new(("hz-winbtn", id)), Sense::click());
                    let col = if resp.hovered() {
                        ui.painter().rect_filled(r, 0.0, if close { CLOSE_HOVER } else { HOVER });
                        if close { Color32::WHITE } else { TEXT }
                    } else {
                        MUTED
                    };
                    icon(ui.painter(), r.center(), col);
                    resp
                };

                if button(ui, "close", true, &paint_close).clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
                let max_icon: &dyn Fn(&egui::Painter, Pos2, Color32) =
                    if maximized { &paint_restore } else { &paint_maximize };
                let tip = if maximized { "Restore" } else { "Maximize" };
                if button(ui, "max", false, max_icon).on_hover_text(tip).clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
                }
                if button(ui, "min", false, &paint_minimize).on_hover_text("Minimize").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
                }
            });
    }

    /// Resize the undecorated window by dragging its edges.
    pub(super) fn window_resize(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().maximized.unwrap_or(false) || i.viewport().fullscreen.unwrap_or(false)) {
            return;
        }
        let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) else { return };
        let Some(dir) = resize_direction(ctx, pos) else { return };
        ctx.set_cursor_icon(match dir {
            ResizeDirection::North | ResizeDirection::South => CursorIcon::ResizeVertical,
            ResizeDirection::East | ResizeDirection::West => CursorIcon::ResizeHorizontal,
            ResizeDirection::NorthWest | ResizeDirection::SouthEast => CursorIcon::ResizeNwSe,
            ResizeDirection::NorthEast | ResizeDirection::SouthWest => CursorIcon::ResizeNeSw,
        });
        if ctx.input(|i| i.pointer.primary_pressed()) {
            ctx.send_viewport_cmd(ViewportCommand::BeginResize(dir));
        }
    }

    /// A hairline around the window (it has no native frame).
    pub(super) fn window_border(&self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().maximized.unwrap_or(false) || i.viewport().fullscreen.unwrap_or(false)) {
            return;
        }
        let rect = ctx.content_rect();
        ctx.layer_painter(LayerId::new(Order::Tooltip, Id::new("hz-window-border"))).rect_stroke(
            rect,
            0.0,
            Stroke::new(1.0_f32, BORDER),
            egui::StrokeKind::Inside,
        );
    }
}

fn resize_direction(ctx: &egui::Context, pos: Pos2) -> Option<ResizeDirection> {
    if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
        return None;
    }
    let r = ctx.content_rect();
    let m = RESIZE_MARGIN;
    let (l, rt, t, b) = (pos.x - r.left() < m, r.right() - pos.x < m, pos.y - r.top() < m, r.bottom() - pos.y < m);
    // Corners get a larger grab area.
    let c = m * 3.0;
    let (lc, rc, tc, bc) = (pos.x - r.left() < c, r.right() - pos.x < c, pos.y - r.top() < c, r.bottom() - pos.y < c);
    Some(match () {
        _ if (t && lc) || (l && tc) => ResizeDirection::NorthWest,
        _ if (t && rc) || (rt && tc) => ResizeDirection::NorthEast,
        _ if (b && lc) || (l && bc) => ResizeDirection::SouthWest,
        _ if (b && rc) || (rt && bc) => ResizeDirection::SouthEast,
        _ if t => ResizeDirection::North,
        _ if b => ResizeDirection::South,
        _ if l => ResizeDirection::West,
        _ if rt => ResizeDirection::East,
        _ => return None,
    })
}

fn paint_close(p: &egui::Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.2_f32, col);
    let d = 4.5;
    p.line_segment([c + vec2(-d, -d), c + vec2(d, d)], s);
    p.line_segment([c + vec2(-d, d), c + vec2(d, -d)], s);
}

fn paint_maximize(p: &egui::Painter, c: Pos2, col: Color32) {
    let r = Rect::from_center_size(c, vec2(9.0, 9.0));
    p.rect_stroke(r, 1.5, Stroke::new(1.2_f32, col), egui::StrokeKind::Middle);
}

fn paint_restore(p: &egui::Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.2_f32, col);
    let front = Rect::from_center_size(c + vec2(-1.5, 1.5), vec2(8.0, 8.0));
    p.rect_stroke(front, 1.5, s, egui::StrokeKind::Middle);
    // The back window's visible top and right edges.
    let (tl, tr, br) = (c + vec2(0.5, -4.5), c + vec2(5.5, -4.5), c + vec2(5.5, 0.5));
    p.line_segment([tl, tr], s);
    p.line_segment([tr, br], s);
}

fn paint_minimize(p: &egui::Painter, c: Pos2, col: Color32) {
    p.line_segment([c + vec2(-5.0, 0.0), c + vec2(5.0, 0.0)], Stroke::new(1.2_f32, col));
}
