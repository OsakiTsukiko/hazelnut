//! Turning styled chapter text into an egui `LayoutJob`.

use crate::blocks::{self, Kind};
use crate::model::{LineStyle, Style};
use crate::theme;
use eframe::egui::{Color32, FontId, Galley, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, pos2, text::LayoutJob, text::TextFormat};
use std::ops::Range;

/// Font size of a heading line, by level.
pub fn heading_size(level: u8) -> f32 {
    theme::BODY_SIZE * [1.6, 1.32, 1.12][(level.clamp(1, 3) - 1) as usize]
}

/// Lay out a chapter body, optionally interpreting line markers.
pub fn body_job(text: &str, styles: &[Style], wrap_width: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap_width;
    if text.is_empty() {
        job.append("", 0.0, format(&Style::default()));
        return job;
    }
    // Sections are runs of chars (including each line's '\n') with equal
    // formats; a char with leading space starts a new one.
    let mut run: Option<(usize, f32, TextFormat)> = None;
    let code = crate::code::document(text, styles);
    let callouts = blocks::callouts_styled(text, styles);
    let mut ci = 0usize;
    let mut byte = 0usize;
    for (li, (start, line, b)) in blocks::parsed_lines(text, styles).into_iter().enumerate() {
        let line_bytes = line.len();
        let mut fmts: Vec<(usize, f32, TextFormat)> = Vec::new();
        for (k, (off, _)) in line.char_indices().enumerate() {
            let st = styles.get(start + k).copied().unwrap_or_default();
            let f = if let Some(code_line) = &code.lines[li] {
                TextFormat {
                    font_id: FontId::monospace(theme::BODY_SIZE * 0.9),
                    color: if code_line.fence { theme::FAINT } else { code.colors[start + k] },
                    line_height: Some(theme::BODY_SIZE * 1.6),
                    ..Default::default()
                }
            } else {
                block_format(&st, &b, k)
            };
            fmts.push((byte + off, list_space(&b, k), f));
        }
        ci += line.chars().count();
        // The newline takes the line's look so the row height matches.
        if byte + line_bytes < text.len() {
            let st = styles.get(ci).copied().unwrap_or_default();
            let mut f = block_format(&st, &b, usize::MAX);
            f.color = Color32::TRANSPARENT;
            fmts.push((byte + line_bytes, 0.0, f));
            ci += 1;
        }
        pad_callout_rows(&mut fmts, &callouts, li, byte + line_bytes < text.len());
        for (at, lead, f) in fmts {
            match &run {
                Some((_, _, cur)) if *cur == f && lead == 0.0 => {}
                _ => {
                    if let Some((from, l, cur)) = run.take() {
                        job.append(&text[from..at], l, cur);
                    }
                    run = Some((at, lead, f));
                }
            }
        }
        byte += line_bytes + 1;
    }
    if let Some((from, l, cur)) = run {
        job.append(&text[from..], l, cur);
    }
    job
}

/// Vertical padding inside callout boxes, above the first row and below the last.
pub const CALLOUT_PAD: f32 = 9.0;

/// Make a callout's first and last rows taller, keeping the text away from
/// the box edges. egui sizes a row by its tallest glyph and aligns the others
/// by `valign`, so a tall invisible glyph (the hidden `>` at the start of the
/// first line, the newline at the end of the last) adds the padding.
fn pad_callout_rows(
    fmts: &mut [(usize, f32, TextFormat)],
    callouts: &[(Range<usize>, Option<String>)],
    line: usize,
    has_newline: bool,
) {
    let first = callouts.iter().any(|(r, _)| r.start == line);
    let last = callouts.iter().any(|(r, _)| r.end == line + 1) && has_newline;
    if fmts.is_empty() || !(first || last) {
        return;
    }
    let base = fmts.iter().filter_map(|f| f.2.line_height).fold(0.0, f32::max);
    let end = fmts.len() - 1;
    if first && last {
        fmts[0].2.line_height = Some(base + CALLOUT_PAD);
        fmts[end].2.line_height = Some(base + 2.0 * CALLOUT_PAD);
        fmts.iter_mut().for_each(|f| f.2.valign = eframe::egui::Align::Center);
    } else if first {
        // Default `valign` (bottom) pushes the rest of the row down.
        fmts[0].2.line_height = Some(base + CALLOUT_PAD);
    } else {
        fmts[end].2.line_height = Some(base + CALLOUT_PAD);
        fmts.iter_mut().for_each(|f| f.2.valign = eframe::egui::Align::TOP);
    }
}

/// Extra space before char `k` of a list line: an inset (deeper for nested
/// items) before the marker, and a gap between the marker and the text.
fn list_space(b: &blocks::Block, k: usize) -> f32 {
    match b.kind {
        Kind::Bullet | Kind::Task(_) if k == b.indent => 6.0 + 14.0 * b.depth as f32,
        Kind::Bullet if k == b.marker => 6.0,
        Kind::Task(_) if k == b.marker => 4.0,
        _ => 0.0,
    }
}

/// The format of char `k` (0-based within its line) of a line of kind `b`.
fn block_format(s: &Style, b: &blocks::Block, k: usize) -> TextFormat {
    let mut f = format(s);
    if k < b.start {
        // Keep hidden syntax at subpixel width even on the caret's line so the
        // first line and wrapped lines share the body's left edge.
        f.color = Color32::TRANSPARENT;
        f.font_id.size = 0.01;
        if let Kind::Heading(level) = b.kind {
            f.line_height = Some(heading_size(level) * 1.45);
        }
        return f;
    }
    match b.kind {
        Kind::Heading(level) => {
            let size = heading_size(level);
            f.font_id = FontId::new(size, theme::family(true, s.italic));
            f.line_height = Some(size * 1.45);
            if k < b.marker {
                f.color = theme::FAINT;
            }
        }
        Kind::Bullet | Kind::Task(_) if k >= b.indent && k < b.marker => {
            // Drawn as a bullet / checkbox by `paint_blocks`.
            f.color = Color32::TRANSPARENT;
        }
        Kind::Task(true) => {
            f.color = s.fg.map_or(theme::MUTED, |c| c.gamma_multiply(0.6));
        }
        _ => {}
    }
    f
}

/// Styles as they should be *decorated* (underline, strike, highlight):
/// list markers, which are drawn as bullets and checkboxes, get none.
pub fn display_styles(text: &str, styles: &[Style]) -> Vec<Style> {
    let mut out = styles.to_vec();
    for ((start, line), code) in blocks::lines(text).zip(crate::code::lines(text, styles)) {
        if code.is_some() {
            for s in out.iter_mut().skip(start).take(line.chars().count()) {
                s.underline = None;
                s.strike = false;
                s.bg = None;
            }
        }
    }
    for (start, _line, b) in blocks::parsed_lines(text, styles) {
        let hidden = [(0, b.start), if b.is_list() { (b.indent, b.marker) } else { (0, 0) }];
        for (from, to) in hidden {
            for s in out.iter_mut().skip(start + from).take(to - from) {
                s.underline = None;
                s.strike = false;
                s.bg = None;
            }
        }
    }
    out
}

/// Per char: its glyph rect, baseline y and font ascent (newlines get an
/// empty rect at the end of their row).
fn char_boxes(galley: &Galley, origin: Pos2) -> Vec<(Rect, f32, f32)> {
    let mut out = Vec::new();
    for row in &galley.rows {
        let base = origin + row.pos.to_vec2();
        for g in &row.glyphs {
            let x0 = base.x + g.pos.x;
            let rect = Rect::from_x_y_ranges(x0..=x0 + g.advance_width, base.y..=base.y + row.size.y);
            out.push((rect, base.y + g.pos.y, g.font_ascent));
        }
        if row.ends_with_newline {
            let x = base.x + row.size.x;
            out.push((Rect::from_x_y_ranges(x..=x, base.y..=base.y + row.size.y), base.y, 0.0));
        }
    }
    out
}

/// Callout colors for a spec: (accent bar, background tint). Accepts the
/// palette's color names, `#rrggbb`, and GitHub's alert names.
pub fn callout_colors(spec: Option<&str>) -> (Color32, Color32) {
    let spec = spec.unwrap_or("").to_ascii_lowercase();
    let name = match spec.as_str() {
        "note" => "blue",
        "tip" => "green",
        "important" => "purple",
        "warning" => "yellow",
        "caution" => "red",
        "grey" => "gray",
        s => s,
    };
    let palette = theme::TEXT_COLORS.iter().position(|(n, c)| c.is_some() && n.eq_ignore_ascii_case(name));
    match (palette, crate::format::parse_hex(&spec)) {
        (Some(i), _) => {
            let bar = theme::TEXT_COLORS[i].1.unwrap_or(theme::MUTED);
            let tint = theme::HIGHLIGHTS[i].1.map_or(theme::SURFACE, |h| mix(h, theme::BG, 0.45));
            (bar, tint)
        }
        (None, Some(c)) => (c, mix(c, theme::BG, 0.84)),
        // No spec (or an unknown one): a neutral box.
        _ => (theme::MUTED, theme::SURFACE),
    }
}

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// Callout boxes (background and accent bar), to paint behind the text.
pub fn callout_shapes(galley: &Galley, origin: Pos2, text: &str, width: f32, styles: &[Style]) -> Vec<Shape> {
    let boxes = char_boxes(galley, origin);
    let starts: Vec<(usize, usize)> = blocks::lines(text).map(|(s, l)| (s, l.chars().count())).collect();
    let mut out = Vec::new();
    for (range, spec) in blocks::callouts_styled(text, styles) {
        let (first, _) = starts[range.start];
        let (last, len) = starts[range.end - 1];
        let (Some(top), Some(bottom)) = (boxes.get(first), boxes.get(last + len.saturating_sub(1))) else { continue };
        // A callout ending the text has no newline to pad its last row with.
        let extra = if range.end == starts.len() { CALLOUT_PAD } else { 0.0 };
        let rect = Rect::from_x_y_ranges(origin.x - 24.0..=origin.x + width + 20.0, top.0.top()..=bottom.0.bottom() + extra);
        let (bar, tint) = callout_colors(spec.as_deref());
        out.push(Shape::rect_filled(rect, 6.0, tint));
        let bar_rect = Rect::from_min_max(rect.min, pos2(rect.left() + 3.0, rect.bottom()));
        out.push(Shape::rect_filled(
            bar_rect,
            eframe::egui::CornerRadius { nw: 6, sw: 6, ne: 0, se: 0 },
            bar,
        ));
    }
    out
}

/// Subtle code panels behind the selection and glyphs.
pub fn code_shapes(galley: &Galley, origin: Pos2, text: &str, styles: &[Style], width: f32) -> Vec<Shape> {
    let doc = crate::code::document(text, styles);
    let boxes = char_boxes(galley, origin);
    let starts: Vec<_> = blocks::lines(text).collect();
    doc.ranges.iter().filter_map(|range| {
        let first = starts[range.start].0;
        let (last, line) = starts[range.end - 1];
        let top = boxes.get(first)?.0.top();
        let bottom = boxes.get(last + line.chars().count().saturating_sub(1))?.0.bottom();
        Some(Shape::rect_filled(Rect::from_x_y_ranges(origin.x - 10.0..=origin.x + width + 10.0, top..=bottom), 6.0, theme::SURFACE))
    }).collect()
}

/// Draw bullets and checkboxes over the (invisible) list markers. Returns
/// each checkbox's clickable rect with the char index of its `x` / space.
pub fn paint_blocks(painter: &Painter, galley: &Galley, origin: Pos2, text: &str, styles: &[Style]) -> Vec<(Rect, usize)> {
    let boxes = char_boxes(galley, origin);
    let mut checks = Vec::new();
    for (start, _line, b) in blocks::parsed_lines(text, styles) {
        let Some(&(dash, baseline, ascent)) = boxes.get(start + b.indent) else { continue };
        // Vertically centered on the x-height.
        let cy = baseline - ascent * 0.36;
        match b.kind {
            Kind::Bullet => {
                let hollow = b.depth % 2 == 1;
                let r = if hollow { 2.4 } else { 2.8 };
                let c = pos2(dash.center().x, cy);
                if hollow {
                    painter.circle_stroke(c, r, Stroke::new(1.2_f32, theme::MUTED));
                } else {
                    painter.circle_filled(c, r, theme::MUTED);
                }
            }
            Kind::Task(checked) => {
                let end = start + b.check + 1; // the ']'
                let x1 = boxes.get(end).map_or(dash.right(), |(r, _, _)| r.right());
                let size = (ascent * 0.78).clamp(11.0, 18.0);
                let rect = Rect::from_center_size(pos2((dash.left() + x1) / 2.0, cy), eframe::egui::vec2(size, size));
                if checked {
                    painter.rect_filled(rect, 3.5, theme::ACCENT);
                    let s = Stroke::new(1.8_f32, theme::BG);
                    let (l, c) = (rect.left(), rect.center());
                    let w = rect.width();
                    painter.line_segment([pos2(l + w * 0.24, c.y + w * 0.02), pos2(l + w * 0.43, c.y + w * 0.2)], s);
                    painter.line_segment([pos2(l + w * 0.43, c.y + w * 0.2), pos2(l + w * 0.77, c.y - w * 0.2)], s);
                } else {
                    painter.rect_stroke(rect, 3.5, Stroke::new(1.4_f32, theme::MUTED), StrokeKind::Inside);
                }
                checks.push((rect.expand(4.0), start + b.check));
            }
            _ => {}
        }
    }
    checks
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
        // (x0, x1, y range, color). The y range is the glyph's own line box,
        // which is the whole row except in padded (callout) rows.
        let mut run: Option<(f32, f32, f32, f32, Color32)> = None;
        let mut flush = |run: &mut Option<(f32, f32, f32, f32, Color32)>| {
            if let Some((x0, x1, y0, y1, c)) = run.take() {
                out.push((Rect::from_x_y_ranges(x0..=x1, y0..=y1), c));
            }
        };
        for g in &row.glyphs {
            let bg = styles.get(ci).and_then(|s| s.bg);
            ci += 1;
            let x0 = base.x + g.pos.x;
            let x1 = x0 + g.advance_width;
            let y0 = (base.y + g.pos.y - g.font_ascent).max(base.y);
            let y1 = (y0 + g.line_height).min(base.y + row.size.y);
            match (&mut run, bg) {
                (Some((_, end, _, _, c)), Some(b)) if *c == b => *end = x1,
                (_, b) => {
                    flush(&mut run);
                    run = b.map(|b| (x0, x1, y0, y1, b));
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
    // Hidden callout prefixes have subpixel glyphs. Use the largest visible
    // glyph on this row, including its padded baseline, to size the caret.
    let metrics = row.glyphs.iter()
        .filter(|g| g.font_ascent > 1.0)
        .max_by(|a, b| a.font_ascent.total_cmp(&b.font_ascent))
        .map(|g| (g.pos.y, g.font_ascent));
    let (baseline, ascent) = metrics.unwrap_or((row.size.y - theme::BODY_SIZE * 0.35, theme::BODY_SIZE * 1.07));
    let base_y = origin.y + row.pos.y + baseline;
    Rect::from_x_y_ranges(x..=x, base_y - ascent * 0.95..=base_y + ascent * 0.28)
}

fn snap(painter: &Painter, y: f32) -> f32 {
    let ppp = painter.pixels_per_point();
    ((y * ppp).floor() + 0.5) / ppp
}

/// Code view: the body markup in a monospace font, with the syntax tinted:
/// line markers (`#`, `-`, `[ ]`, `>`, `[!color]`) and `{spec|…}` braces in
/// the accent color, style specs and escapes muted.
pub fn code_job(src: &str, wrap_width: f32) -> LayoutJob {
    let font = FontId::monospace(14.5);
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap_width;
    let fmt = |color: Color32| TextFormat {
        font_id: font.clone(),
        color,
        line_height: Some(14.5 * 1.65),
        ..Default::default()
    };
    // Colors per byte offset, merged into sections below.
    let mut colors: Vec<(usize, Color32)> = Vec::new();
    let mut byte = 0;
    let mut char_start = 0;
    let code = crate::code::document(src, &[]);
    for (li, line) in src.split('\n').enumerate() {
        if let Some(code_line) = &code.lines[li] {
            for (i, (off, _)) in line.char_indices().enumerate() {
                colors.push((byte + off, if code_line.fence { theme::ACCENT } else { code.colors[char_start + i] }));
            }
            byte += line.len();
            if byte < src.len() { colors.push((byte, theme::TEXT)); }
            byte += 1;
            char_start += line.chars().count() + 1;
            continue;
        }
        let b = blocks::parse(line);
        let mut in_span = false;
        let mut chars = line.char_indices().enumerate().peekable();
        while let Some((k, (off, c))) = chars.next() {
            let at = byte + off;
            if k < b.marker {
                colors.push((at, theme::ACCENT));
                continue;
            }
            match c {
                '\\' => {
                    colors.push((at, theme::FAINT));
                    if let Some((_, (o, _))) = chars.next() {
                        colors.push((byte + o, theme::TEXT));
                    }
                }
                '{' if !in_span => {
                    // `{spec|` opens a span, as in `format::parse_line`.
                    let rest = &line[off + 1..];
                    let spec_end = rest.find(['|', '{', '}', '\\']).filter(|&i| rest[i..].starts_with('|'));
                    match spec_end {
                        Some(len) => {
                            colors.push((at, theme::ACCENT));
                            if len > 0 {
                                colors.push((at + 1, theme::MUTED));
                            }
                            colors.push((at + 1 + len, theme::ACCENT));
                            in_span = true;
                            while chars.peek().is_some_and(|(_, (o, _))| *o <= off + 1 + len) {
                                chars.next();
                            }
                        }
                        None => colors.push((at, theme::TEXT)),
                    }
                }
                '}' if in_span => {
                    colors.push((at, theme::ACCENT));
                    in_span = false;
                }
                _ => colors.push((at, theme::TEXT)),
            }
        }
        char_start += line.chars().count() + 1;
        byte += line.len();
        if byte < src.len() {
            colors.push((byte, theme::TEXT));
        }
        byte += 1;
    }
    let mut run: Option<(usize, Color32)> = None;
    for (at, c) in colors {
        match run {
            Some((_, cur)) if cur == c => {}
            _ => {
                if let Some((from, cur)) = run.take() {
                    job.append(&src[from..at], 0.0, fmt(cur));
                }
                run = Some((at, c));
            }
        }
    }
    match run {
        Some((from, cur)) => job.append(&src[from..], 0.0, fmt(cur)),
        None => job.append("", 0.0, fmt(theme::TEXT)),
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{self, text::CCursor};

    #[test]
    fn callout_text_aligns_and_caret_uses_visible_glyphs() {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        let _ = ctx.run(Default::default(), |ctx| {
            for text in ["> [!blue] Callout text", "> [!] ", "> [!blue] ## Heading", "> [!blue] A long callout that wraps onto multiple rows"] {
                let galley = ctx.fonts_mut(|f| f.layout_job(body_job(text, &[], 180.0)));
                let start = blocks::parse(text).start;
                let caret = caret_rect(&galley, Pos2::ZERO, CCursor::new(start));
                assert!(caret.height() > theme::BODY_SIZE, "caret must not use the hidden marker's metrics");
                assert!(caret.left().abs() < 0.1, "callout content must align with body text: {caret:?}");
                for row in &galley.rows {
                    let visible = row.glyphs.iter().find(|g| g.font_ascent > 1.0);
                    if let Some(g) = visible {
                        assert!((row.pos.x + g.pos.x).abs() < 0.1);
                    }
                }
            }
        });
    }

    #[test]
    fn literal_layout_keeps_markers_visible_and_inline_styles() {
        let text = "# Heading\n- [ ] task\n> [!blue] callout\n{b|text} > *";
        let styles = vec![Style { bold: true, escaped: true, ..Default::default() }; text.chars().count()];
        let job = body_job(text, &styles, 400.0);
        assert_eq!(job.text, text);
        assert!(job.sections.iter().all(|s| s.format.font_id.size == theme::BODY_SIZE));
        assert!(job.sections.iter().all(|s| s.format.color != Color32::TRANSPARENT || &text[s.byte_range.clone()] == "\n"));
        assert!(job.sections.iter().all(|s| s.format.font_id.family == theme::family(true, false)));
    }

    #[test]
    fn literal_typing_preserves_existing_formatting_and_survives_roundtrip() {
        use crate::{edit, format};
        let mut text = "# Existing heading\n> [!blue] Existing callout\n".to_owned();
        let mut styles = vec![Style::default(); text.chars().count()];
        let existing = styles.clone();
        for (insert, literal) in [("# Literal heading\n> [!red] Literal callout\n- literal list\n", true), ("# New heading\n- new list", false)] {
            let at = text.chars().count();
            let style = edit::typing_style(&styles, at, None);
            let change = edit::Change { at, removed: 0, inserted: insert.chars().count() };
            text.push_str(insert);
            styles = edit::apply_typing(&styles, change, Some(style), &text, literal);
        }
        assert_eq!(&styles[..existing.len()], existing.as_slice());
        let markup = format::body_markup(&text, &styles);
        assert!(!markup.contains("{literal"));
        assert!(markup.contains(r"\# Literal heading"));
        let (saved_text, saved_styles) = format::parse_body(&markup);
        assert_eq!(saved_text, text);
        let kinds: Vec<_> = blocks::lines(&saved_text).map(|(start, line)| {
            blocks::parse_styled(line, &saved_styles[start..])
        }).collect();
        assert_eq!(kinds[0].kind, Kind::Heading(1));
        assert!(kinds[1].quoted);
        assert_eq!(kinds[2].kind, Kind::Plain);
        assert!(!kinds[3].quoted);
        assert_eq!(kinds[4].kind, Kind::Plain);
        assert_eq!(kinds[5].kind, Kind::Heading(1));
        assert_eq!(kinds[6].kind, Kind::Bullet);
        assert_eq!(blocks::callouts_styled(&saved_text, &saved_styles).len(), 1);
        // Literal list markers must not start automatic continuation after
        // switching the toggle back off.
        let nl = saved_text.chars().enumerate().filter(|(_, c)| *c == '\n').nth(4).unwrap().0;
        assert!(blocks::on_enter_styled(&saved_text, nl, &saved_styles).is_none());
        let job = body_job(&saved_text, &saved_styles, 400.0);
        assert!(job.sections.iter().any(|s| s.format.font_id.size == heading_size(1)));
        assert!(job.sections.iter().any(|s| saved_text[s.byte_range.clone()].contains("Literal heading") && s.format.font_id.size == theme::BODY_SIZE));
    }

}
