//! The start screen: create / open, starred and recent notebooks.

use super::{After, HazelnutApp, HomeAction, Modal, spaced_label, star_button};
use crate::library::{self, Entry};
use crate::theme::{self, ACCENT, BG, BORDER, FAINT, HOVER, MUTED, SURFACE, TEXT};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Frame, Id, Margin, Rect, Response, RichText, Sense, Stroke, Ui,
    pos2, vec2,
};

const COLUMN: f32 = 640.0;

impl HazelnutApp {
    pub(super) fn home(&mut self, ctx: &egui::Context) {
        let mut action = None;
        let starred: Vec<Entry> = self.library.starred().cloned().collect();
        let recent: Vec<Entry> = self.library.recent().cloned().collect();

        egui::CentralPanel::default().frame(Frame::new().fill(BG)).show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let avail = ui.available_width();
                let col = (avail - 64.0).clamp(280.0, COLUMN);
                let pad = ((avail - col) / 2.0).max(0.0);
                ui.horizontal_top(|ui| {
                    ui.add_space(pad);
                    ui.vertical(|ui| {
                        ui.set_width(col);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.add_space(36.0);

                        ui.horizontal(|ui| {
                            let (r, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                            ui.painter().circle_filled(r.center(), 4.0, ACCENT);
                            ui.add_space(4.0);
                            let g = spaced_label(ui, "HAZELNUT", 10.5, FAINT);
                            ui.label(g);
                        });
                        ui.add_space(18.0);
                        ui.label(
                            RichText::new("Your notebooks")
                                .size(30.0)
                                .family(theme::family(true, false))
                                .color(TEXT),
                        );
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new("Pick up where you left off, or start something new.")
                                .size(14.5)
                                .color(MUTED),
                        );
                        ui.add_space(28.0);

                        // Primary actions
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 12.0;
                            let w = (col - 12.0) / 2.0;
                            if action_card(ui, w, "New notebook", "Ctrl+N", true, paint_plus).clicked() {
                                action = Some(HomeAction::New);
                            }
                            if action_card(ui, w, "Open file…", "Ctrl+O", false, paint_open).clicked() {
                                action = Some(HomeAction::OpenDialog);
                            }
                        });

                        if !starred.is_empty() {
                            section(ui, "STARRED");
                            for e in &starred {
                                notebook_row(ui, e, &mut action);
                            }
                        }

                        section(ui, "RECENT");
                        if recent.is_empty() {
                            let msg = if starred.is_empty() {
                                "Nothing here yet. Notebooks you create or open will show up here."
                            } else {
                                "Other notebooks you open will show up here."
                            };
                            ui.add_space(6.0);
                            ui.label(RichText::new(msg).size(13.5).color(FAINT));
                        }
                        for e in &recent {
                            notebook_row(ui, e, &mut action);
                        }

                        ui.add_space(36.0);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 20.0;
                            if link(ui, "Take the tour  ›").clicked() {
                                action = Some(HomeAction::Tour);
                            }
                            if link(ui, "About").clicked() {
                                action = Some(HomeAction::About);
                            }
                        });
                        ui.add_space(64.0);
                    });
                });
            });
        });

        self.home_toast(ctx);

        match action {
            Some(HomeAction::Open(p)) => self.proceed(ctx, After::OpenPath(p)),
            Some(HomeAction::Star(p, on)) => self.library.set_starred(&p, on),
            Some(HomeAction::Remove(p)) => self.library.remove(&p),
            Some(HomeAction::New) => self.proceed(ctx, After::NewNotebook),
            Some(HomeAction::OpenDialog) => self.proceed(ctx, After::OpenDialog),
            Some(HomeAction::Tour) => self.proceed(ctx, After::Tour),
            Some(HomeAction::About) => self.modal = Some(Modal::About { show_license: false }),
            None => {}
        }
    }

    fn home_toast(&mut self, ctx: &egui::Context) {
        let Some((msg, t, err)) = self.toast.clone() else { return };
        let age = ctx.input(|i| i.time) - t;
        if age > 4.0 {
            return;
        }
        let alpha = ((4.0 - age) / 0.6).clamp(0.0, 1.0) as f32;
        egui::Area::new(Id::new("hz-home-toast"))
            .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -28.0))
            .interactable(false)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(SURFACE.gamma_multiply(alpha))
                    .stroke(Stroke::new(1.0_f32, BORDER.gamma_multiply(alpha)))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(Margin::symmetric(16, 10))
                    .show(ui, |ui| {
                        let c = if err { theme::DANGER } else { TEXT };
                        ui.label(RichText::new(msg).size(13.0).color(c.gamma_multiply(alpha)));
                    });
            });
        ctx.request_repaint();
    }
}

fn section(ui: &mut Ui, label: &str) {
    ui.add_space(36.0);
    let g = spaced_label(ui, label, 10.5, FAINT);
    ui.label(g);
    ui.add_space(8.0);
}

fn action_card(
    ui: &mut Ui,
    w: f32,
    title: &str,
    hint: &str,
    primary: bool,
    icon: fn(&egui::Painter, egui::Pos2, Color32),
) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 74.0), Sense::click());
    let p = ui.painter();
    let hovered = resp.hovered();
    p.rect(
        rect,
        12.0,
        if hovered { HOVER } else { SURFACE },
        Stroke::new(1.0_f32, if hovered && primary { ACCENT.gamma_multiply(0.5) } else { BORDER }),
        egui::StrokeKind::Inside,
    );
    let c = pos2(rect.left() + 37.0, rect.center().y);
    let (disc, glyph) = if primary { (ACCENT.gamma_multiply(0.16), ACCENT) } else { (HOVER, TEXT) };
    p.circle_filled(c, 18.0, if hovered && !primary { BORDER } else { disc });
    icon(p, c, glyph);
    p.text(
        pos2(rect.left() + 68.0, rect.center().y - 9.0),
        Align2::LEFT_CENTER,
        title,
        FontId::new(14.5, theme::medium()),
        TEXT,
    );
    p.text(
        pos2(rect.left() + 68.0, rect.center().y + 11.0),
        Align2::LEFT_CENTER,
        hint,
        FontId::proportional(11.5),
        FAINT,
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn paint_plus(p: &egui::Painter, c: egui::Pos2, col: Color32) {
    let s = Stroke::new(1.8_f32, col);
    p.line_segment([c - vec2(6.0, 0.0), c + vec2(6.0, 0.0)], s);
    p.line_segment([c - vec2(0.0, 6.0), c + vec2(0.0, 6.0)], s);
}

fn paint_open(p: &egui::Painter, c: egui::Pos2, col: Color32) {
    // A folder outline.
    let s = Stroke::new(1.5_f32, col);
    let body = Rect::from_center_size(c + vec2(0.0, 1.5), vec2(15.0, 10.5));
    p.rect_stroke(body, 2.0, s, egui::StrokeKind::Middle);
    p.line_segment([body.left_top() + vec2(1.0, 0.0), body.left_top() + vec2(3.0, -2.5)], s);
    p.line_segment([body.left_top() + vec2(3.0, -2.5), body.left_top() + vec2(6.5, -2.5)], s);
    p.line_segment([body.left_top() + vec2(6.5, -2.5), body.left_top() + vec2(8.0, 0.0)], s);
}

/// One notebook in the starred / recent lists.
fn notebook_row(ui: &mut Ui, e: &Entry, action: &mut Option<HomeAction>) {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 60.0), Sense::click());
    let hovered = ui.rect_contains_pointer(rect);
    let p = ui.painter().clone();
    if hovered {
        p.rect_filled(rect, 10.0, SURFACE);
    }

    // Notebook glyph: a small cover with a spine.
    let cover = Rect::from_center_size(pos2(rect.left() + 26.0, rect.center().y), vec2(22.0, 28.0));
    let spine = if e.starred { ACCENT } else { MUTED };
    p.rect_stroke(cover, 4.0, Stroke::new(1.2_f32, if e.exists { MUTED } else { FAINT }), egui::StrokeKind::Inside);
    p.line_segment(
        [pos2(cover.left() + 6.0, cover.top() + 1.0), pos2(cover.left() + 6.0, cover.bottom() - 1.0)],
        Stroke::new(1.2_f32, if e.exists { spine } else { FAINT }),
    );

    // Right side: star, remove (on hover), time.
    let star_rect = Rect::from_center_size(pos2(rect.right() - 26.0, rect.center().y), vec2(30.0, 30.0));
    let remove_rect = star_rect.translate(vec2(-32.0, 0.0));
    let (time, time_color) = if e.exists {
        (library::relative_time(e.opened), FAINT)
    } else {
        ("missing".to_owned(), theme::DANGER.gamma_multiply(0.8))
    };
    let time_galley = ui.fonts_mut(|f| f.layout_no_wrap(time, FontId::proportional(11.5), time_color));
    let time_right = if hovered { remove_rect.left() - 6.0 } else { star_rect.left() - 6.0 };
    let text_right = time_right - time_galley.size().x - 16.0;

    let left = rect.left() + 54.0;
    let title_color = if e.exists { TEXT } else { MUTED };
    let mut job = egui::text::LayoutJob::simple_singleline(
        e.display_title(),
        FontId::new(14.5, theme::medium()),
        title_color,
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width((text_right - left).max(40.0));
    let title = ui.fonts_mut(|f| f.layout_job(job));
    p.galley(pos2(left, rect.center().y - 9.0 - title.size().y / 2.0), title, title_color);

    let sub = library::pretty_path(&e.path);
    let mut job = egui::text::LayoutJob::simple_singleline(sub, FontId::proportional(11.5), FAINT);
    job.wrap = egui::text::TextWrapping::truncate_at_width((text_right - left).max(40.0));
    let sub = ui.fonts_mut(|f| f.layout_job(job));
    p.galley(pos2(left, rect.center().y + 10.0 - sub.size().y / 2.0), sub, FAINT);

    p.galley(
        pos2(time_right - time_galley.size().x, rect.center().y - time_galley.size().y / 2.0),
        time_galley,
        time_color,
    );

    // Buttons are registered after the row, so they win the click.
    let id = Id::new(("hz-row", &e.path));
    if e.starred || hovered {
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(star_rect));
        let tip = if e.starred { "Unstar" } else { "Star" };
        if star_button(&mut child, e.starred, hovered, 30.0).on_hover_text(tip).clicked() {
            *action = Some(HomeAction::Star(e.path.clone(), !e.starred));
        }
    }
    if hovered {
        let r = ui.interact(remove_rect, id.with("remove"), Sense::click());
        let col = if r.hovered() {
            p.rect_filled(remove_rect.shrink(1.0), 7.0, HOVER);
            TEXT
        } else {
            FAINT
        };
        let c = remove_rect.center();
        let s = Stroke::new(1.4_f32, col);
        p.line_segment([c - vec2(4.0, 4.0), c + vec2(4.0, 4.0)], s);
        p.line_segment([c + vec2(-4.0, 4.0), c + vec2(4.0, -4.0)], s);
        if r.on_hover_text("Remove from list").clicked() {
            *action = Some(HomeAction::Remove(e.path.clone()));
        }
    }

    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    if resp.clicked() && action.is_none() {
        *action = Some(HomeAction::Open(e.path.clone()));
    }
    resp.context_menu(|ui| {
        ui.set_min_width(170.0);
        if ui.add_enabled(e.exists, egui::Button::new("Open")).clicked() {
            *action = Some(HomeAction::Open(e.path.clone()));
        }
        let star = if e.starred { "Unstar" } else { "Star" };
        if ui.button(star).clicked() {
            *action = Some(HomeAction::Star(e.path.clone(), !e.starred));
        }
        ui.separator();
        if ui.button("Remove from list").clicked() {
            *action = Some(HomeAction::Remove(e.path.clone()));
        }
    });
}

fn link(ui: &mut Ui, text: &str) -> Response {
    let g = ui.fonts_mut(|f| f.layout_no_wrap(text.to_owned(), FontId::proportional(13.0), Color32::PLACEHOLDER));
    let (rect, resp) = ui.allocate_exact_size(g.size(), Sense::click());
    let col = if resp.hovered() { ACCENT } else { MUTED };
    ui.painter().galley(rect.min, g, col);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}
