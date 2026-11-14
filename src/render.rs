//! Turning styled chapter text into an egui `LayoutJob`.

use crate::model::{LineStyle, Style};
use crate::theme;
use eframe::egui::{Color32, FontId, Galley, Painter, Pos2, Rect, Shape, Stroke, pos2, text::LayoutJob, text::TextFormat};
use std::ops::Range;

pub fn body_job(text: &str, styles: &[Style], wrap_width: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap_width;
    if text.is_empty() {
        job.append("", 0.0, format(&Style::default()));
        return job;
    }
    let mut start = 0;
    let mut cur = styles.first().copied().unwrap_or_default();
    for (i, (byte, _)) in text.char_indices().enumerate() {
        let s = styles.get(i).copied().unwrap_or_default();
        if s != cur {
            job.append(&text[start..byte], 0.0, format(&cur));
            start = byte;
            cur = s;
        }
    }
    job.append(&text[start..], 0.0, format(&cur));
    job
}

fn format(s: &Style) -> TextFormat {
    let color = s.fg.unwrap_or(theme::TEXT);
    TextFormat {
        font_id: FontId::new(theme::BODY_SIZE, theme::family(s.bold, s.italic)),
        color,
        // Highlights are painted by `highlight_rects` so the selection can
        // sit between them and the text.
        line_height: Some(theme::BODY_SIZE * 1.6),
        // Underline / strikethrough are painted by `paint_decorations`:
        // egui would draw them at the bottom of the (tall) line box.
        ..Default::default()
    }
}

/// Draw underlines and strikethroughs relative to each glyph's baseline.
///
/// `galley` is the laid-out body (one glyph per char, plus one implicit
/// newline per row that ends with `\n`), positioned at `origin`.
pub fn paint_decorations(painter: &Painter, galley: &Galley, origin: Pos2, styles: &[Style]) {
    // (start_x, end_x, y, color, pattern) of the line currently being extended.
    type Seg = Option<(f32, f32, f32, Color32, LineStyle)>;
    let flush = |seg: &mut Seg| {
        if let Some((x0, x1, y, c, kind)) = seg.take() {
            paint_line(painter, x0, x1, y, c, kind);
        }
    };
    let mut ci = 0usize;
    for row in &galley.rows {
        let base = origin + row.pos.to_vec2();
        let mut under: Seg = None;
        let mut strike: Seg = None;
        for g in &row.glyphs {
            let st = styles.get(ci).copied().unwrap_or_default();
            ci += 1;
            let color = st.fg.unwrap_or(theme::TEXT);
            let x0 = base.x + g.pos.x;
            let x1 = x0 + g.advance_width;
            let baseline = base.y + g.pos.y;
            let lines = [
                (&mut under, st.underline, baseline + g.font_ascent * 0.16),
                (&mut strike, st.strike.then_some(LineStyle::Solid), baseline - g.font_ascent * 0.27),
            ];
            for (seg, kind, y) in lines {
                let y = snap(painter, y);
                match (seg.as_mut(), kind) {
                    (Some((_, end, sy, sc, sk)), Some(k)) if *sy == y && *sc == color && *sk == k => *end = x1,
                    (_, kind) => {
                        flush(seg);
                        *seg = kind.map(|k| (x0, x1, y, color, k));
                    }
                }
            }
        }
        flush(&mut under);
        flush(&mut strike);
        if row.ends_with_newline {
            ci += 1;
        }
    }
}

/// Draw a horizontal line from `x0` to `x1` at `y` in the given pattern.
/// Patterns are anchored to absolute x so neighbouring runs line up.
pub fn paint_line(painter: &Painter, x0: f32, x1: f32, y: f32, color: Color32, kind: LineStyle) {
    if x1 <= x0 {
        return;
    }
    let stroke = Stroke::new(1.25_f32, color);
    match kind {
        LineStyle::Solid => {
            painter.line_segment([pos2(x0, y), pos2(x1, y)], stroke);
        }
        LineStyle::Double => {
            let thin = Stroke::new(1.0_f32, color);
            painter.line_segment([pos2(x0, y - 0.5), pos2(x1, y - 0.5)], thin);
            painter.line_segment([pos2(x0, y + 2.0), pos2(x1, y + 2.0)], thin);
        }
        LineStyle::Dotted => {
            let step = 3.0;
            let mut x = (x0 / step).ceil() * step + 0.5;
            while x <= x1 {
                painter.circle_filled(pos2(x, y), 0.8, color);
                x += step;
            }
        }
        LineStyle::Dashed => {
            let (period, dash) = (7.0, 4.0);
            let mut start = (x0 / period).floor() * period;
            while start < x1 {
                let (a, b) = (start.max(x0), (start + dash).min(x1));
                if b > a {
                    painter.line_segment([pos2(a, y), pos2(b, y)], stroke);
                }
                start += period;
            }
        }
        LineStyle::Wavy => {
            let (period, amp, cy) = (6.0_f32, 1.5_f32, y + 1.0);
            let wave = |x: f32| cy + amp * (x * std::f32::consts::TAU / period).sin();
            let mut pts = Vec::new();
            let mut x = x0;
            while x < x1 {
                pts.push(pos2(x, wave(x)));
                x += 0.75;
            }
            pts.push(pos2(x1, wave(x1)));
            painter.add(Shape::line(pts, stroke));
        }
        LineStyle::ZigZag => {
            let (period, amp, cy) = (6.0_f32, 1.75_f32, y + 1.0);
            let tri = |x: f32| {
                let t = (x / period).rem_euclid(1.0);
                cy + amp * if t < 0.5 { 4.0 * t - 1.0 } else { 3.0 - 4.0 * t }
            };
            let half = period / 2.0;
            let mut pts = vec![pos2(x0, tri(x0))];
            let mut x = (x0 / half).floor() * half + half;
            while x < x1 {
                pts.push(pos2(x, tri(x)));
                x += half;
            }
            pts.push(pos2(x1, tri(x1)));
            painter.add(Shape::line(pts, stroke));
        }
    }
}

/// Highlight (background color) rectangles, merged per row and color.
pub fn highlight_rects(galley: &Galley, origin: Pos2, styles: &[Style]) -> Vec<(Rect, Color32)> {
    let mut out: Vec<(Rect, Color32)> = Vec::new();
    let mut ci = 0usize;
    for row in &galley.rows {
        let base = origin + row.pos.to_vec2();
        let y = base.y..=base.y + row.size.y;
        let mut run: Option<(f32, f32, Color32)> = None;
        let mut flush = |run: &mut Option<(f32, f32, Color32)>| {
            if let Some((x0, x1, c)) = run.take() {
                out.push((Rect::from_x_y_ranges(x0..=x1, y.clone()), c));
            }
        };
        for g in &row.glyphs {
            let bg = styles.get(ci).and_then(|s| s.bg);
            ci += 1;
            let x0 = base.x + g.pos.x;
            let x1 = x0 + g.advance_width;
            match (&mut run, bg) {
                (Some((_, end, c)), Some(b)) if *c == b => *end = x1,
                (_, b) => {
                    flush(&mut run);
                    run = b.map(|b| (x0, x1, b));
                }
            }
        }
        flush(&mut run);
        if row.ends_with_newline {
            ci += 1;
        }
    }
    out
}

/// Rectangles covering the chars in `range`, one per visual row.
pub fn selection_rects(galley: &Galley, origin: Pos2, range: Range<usize>) -> Vec<Rect> {
    let mut out = Vec::new();
    let mut ci = 0usize;
    for (i, row) in galley.rows.iter().enumerate() {
        let base = origin + row.pos.to_vec2();
        // Extend down to the next row so a multi-line selection is one block.
        let bottom = galley.rows.get(i + 1).map_or(row.pos.y + row.size.y, |next| next.pos.y);
        let y = base.y..=origin.y + bottom.max(row.pos.y + row.size.y);
        let mut span: Option<(f32, f32)> = None;
        for g in &row.glyphs {
            if range.contains(&ci) {
                let x0 = base.x + g.pos.x;
                let x1 = x0 + g.advance_width;
                span = Some(span.map_or((x0, x1), |(a, _)| (a, x1)));
            }
            ci += 1;
        }
        if row.ends_with_newline {
            // Show a selected line break as a small sliver, like most editors.
            if range.contains(&ci) {
                let x = base.x + row.size.x;
                span = Some(span.map_or((x, x + 6.0), |(a, b)| (a, b + 6.0)));
            }
            ci += 1;
        }
        if let Some((x0, x1)) = span {
            out.push(Rect::from_x_y_ranges(x0..=x1, y));
        }
    }
    out
}

/// The caret rectangle for `cursor`, sized to the text rather than the
/// (taller) line box.
pub fn caret_rect(galley: &Galley, origin: Pos2, cursor: eframe::egui::text::CCursor) -> Rect {
    let layout = galley.layout_from_cursor(cursor);
    let x = origin.x + galley.pos_from_cursor(cursor).min.x;
    let Some(row) = galley.rows.get(layout.row) else {
        return Rect::from_min_size(origin, eframe::egui::vec2(0.0, theme::BODY_SIZE * 1.3));
    };
    // Baseline and ascent from a glyph on this row, or any row.
    let metrics = row
        .glyphs
        .first()
        .or_else(|| galley.rows.iter().find_map(|r| r.glyphs.first()))
        .map(|g| (g.pos.y, g.font_ascent));
    let (baseline, ascent) = metrics.unwrap_or((row.size.y * 0.78, theme::BODY_SIZE * 1.07));
    let base_y = origin.y + row.pos.y + baseline;
    Rect::from_x_y_ranges(x..=x, base_y - ascent * 0.95..=base_y + ascent * 0.28)
}

fn snap(painter: &Painter, y: f32) -> f32 {
    let ppp = painter.pixels_per_point();
    ((y * ppp).floor() + 0.5) / ppp
}
