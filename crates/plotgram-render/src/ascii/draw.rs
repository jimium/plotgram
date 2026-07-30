//! Box, segment and junction drawing on the character canvas.
//!
//! Routes arrive as already-quantized grid polylines; this module only draws.
//! No re-routing decisions are made here (single writer: layout owns paths).

use std::collections::BTreeMap;

use super::canvas::{Cell, DisplayCanvas, GridRect};
use super::{ARROW_DOWN, ARROW_LEFT, ARROW_RIGHT, ARROW_UP, BOX_H, BOX_V, DASH_H, DASH_V};

/// Drop consecutive duplicates and collapse collinear runs.
pub(super) fn simplify_points(points: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    let mut simplified: Vec<(usize, usize)> = Vec::new();
    for point in points {
        if simplified.last() == Some(&point) {
            continue;
        }
        simplified.push(point);
        while simplified.len() >= 3 {
            let len = simplified.len();
            let a = simplified[len - 3];
            let b = simplified[len - 2];
            let c = simplified[len - 1];
            let collinear = (a.0 == b.0 && b.0 == c.0) || (a.1 == b.1 && b.1 == c.1);
            if collinear {
                simplified.remove(len - 2);
            } else {
                break;
            }
        }
    }
    simplified
}

/// Cleans a label for display: replaces control characters with spaces but
/// preserves Unicode characters (CJK, etc.) for correct display-width math.
pub(super) fn clean_label(label: &str) -> String {
    label
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect()
}

/// Box-drawing glyph set for [`draw_box`].
pub(super) struct BoxChars {
    pub tl: char,
    pub tr: char,
    pub bl: char,
    pub br: char,
    pub v: char,
    pub h: char,
}

pub(super) fn draw_box(
    canvas: &mut DisplayCanvas,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    chars: &BoxChars,
) {
    if w == 0 || h == 0 {
        return;
    }

    canvas.set_char(x, y, chars.tl);
    for i in 1..w.saturating_sub(1) {
        canvas.set_char(x + i, y, chars.h);
    }
    if w > 1 {
        canvas.set_char(x + w - 1, y, chars.tr);
    }

    if h > 1 {
        canvas.set_char(x, y + h - 1, chars.bl);
        for i in 1..w.saturating_sub(1) {
            canvas.set_char(x + i, y + h - 1, chars.h);
        }
        if w > 1 {
            canvas.set_char(x + w - 1, y + h - 1, chars.br);
        }
    }

    for j in 1..h.saturating_sub(1) {
        canvas.set_char(x, y + j, chars.v);
        if w > 1 {
            canvas.set_char(x + w - 1, y + j, chars.v);
        }
    }
}

pub(super) fn draw_edge_route(
    canvas: &mut DisplayCanvas,
    points: &[(usize, usize)],
    dashed: bool,
    bidirectional: bool,
    node_rects: &[GridRect],
) {
    for seg in points.windows(2) {
        let (x1, y1) = seg[0];
        let (x2, y2) = seg[1];
        draw_segment(canvas, x1, y1, x2, y2, dashed, node_rects);
    }

    // Draw corner characters at interior waypoints (direction changes).
    // draw_segment uses exclusive ranges so the waypoint cell itself is missed.
    for i in 1..points.len().saturating_sub(1) {
        let (px, py) = points[i - 1];
        let (cx, cy) = points[i];
        let (nx, ny) = points[i + 1];
        if canvas.is_interior(cx, cy, node_rects) || is_on_rect_boundary(cx, cy, node_rects) {
            continue;
        }
        let dirs = direction_bit((cx, cy), (px, py)) | direction_bit((cx, cy), (nx, ny));
        if dirs.count_ones() >= 2 {
            canvas.ensure(cy, cx);
            canvas.set_char(cx, cy, directions_to_char(dirs));
        }
    }

    let n = points.len();
    if n >= 2 {
        let (x1, y1) = points[n - 2];
        let (x2, y2) = points[n - 1];
        if let Some((mut ax, mut ay, ch)) = arrow_before_end(x1, y1, x2, y2) {
            // Fallback: if arrow lands on a boundary, step back one cell.
            if is_on_rect_boundary(ax, ay, node_rects) {
                if x1 == x2 {
                    if y2 > y1 { ay = ay.saturating_sub(1); } else { ay += 1; }
                } else if y1 == y2 {
                    if x2 > x1 { ax = ax.saturating_sub(1); } else { ax += 1; }
                }
            }
            // A horizontal arrow snug against a box corner reads badly (`▶└`);
            // step back one cell and blank the gap. Plain borders stay snug (`▶│`).
            if y1 == y2 && is_box_corner(canvas, x2, y2) {
                canvas.clear_span(ay, ax, 1);
                if x2 > x1 { ax = ax.saturating_sub(1); } else { ax += 1; }
            }
            if !canvas.is_interior(ax, ay, node_rects)
                && !is_on_rect_boundary(ax, ay, node_rects)
            {
                canvas.set_char(ax, ay, ch);
            }
        }

        if bidirectional {
            let (x0, y0) = points[0];
            let (bx, by) = points[1];
            if let Some((mut ax, mut ay, ch)) = arrow_before_end(bx, by, x0, y0) {
                if is_on_rect_boundary(ax, ay, node_rects) {
                    if bx == x0 {
                        if y0 > by { ay = ay.saturating_sub(1); } else { ay += 1; }
                    } else if by == y0 {
                        if x0 > bx { ax = ax.saturating_sub(1); } else { ax += 1; }
                    }
                }
                if by == y0 && is_box_corner(canvas, x0, y0) {
                    canvas.clear_span(ay, ax, 1);
                    if x0 > bx { ax = ax.saturating_sub(1); } else { ax += 1; }
                }
                if !canvas.is_interior(ax, ay, node_rects)
                    && !is_on_rect_boundary(ax, ay, node_rects)
                {
                    canvas.set_char(ax, ay, ch);
                }
            }
        }
    }
}

pub(super) fn draw_segment(
    canvas: &mut DisplayCanvas,
    x1: usize,
    y1: usize,
    x2: usize,
    y2: usize,
    dashed: bool,
    node_rects: &[GridRect],
) {
    if x1 == x2 {
        let (start, end) = if y1 <= y2 { (y1, y2) } else { (y2, y1) };
        for y in start..end {
            if !canvas.is_interior(x1, y, node_rects)
                && !is_on_rect_boundary(x1, y, node_rects)
            {
                draw_line_char(canvas, x1, y, if dashed { DASH_V } else { BOX_V });
            }
        }
        maybe_merge_endpoint(canvas, x1, end, if dashed { DASH_V } else { BOX_V }, node_rects);
    } else if y1 == y2 {
        let (start, end) = if x1 <= x2 { (x1, x2) } else { (x2, x1) };
        for x in start..end {
            if !canvas.is_interior(x, y1, node_rects)
                && !is_on_rect_boundary(x, y1, node_rects)
            {
                draw_line_char(canvas, x, y1, if dashed { DASH_H } else { BOX_H });
            }
        }
        maybe_merge_endpoint(canvas, end, y1, if dashed { DASH_H } else { BOX_H }, node_rects);
    } else {
        // Diagonal in grid space: draw as horizontal-then-vertical elbow.
        for x in x1.min(x2)..x1.max(x2) {
            if !canvas.is_interior(x, y1, node_rects)
                && !is_on_rect_boundary(x, y1, node_rects)
            {
                draw_line_char(canvas, x, y1, if dashed { DASH_H } else { BOX_H });
            }
        }
        for y in y1.min(y2)..y1.max(y2) {
            if !canvas.is_interior(x2, y, node_rects)
                && !is_on_rect_boundary(x2, y, node_rects)
            {
                draw_line_char(canvas, x2, y, if dashed { DASH_V } else { BOX_V });
            }
        }
        if !canvas.is_interior(x2, y1, node_rects)
            && !is_on_rect_boundary(x2, y1, node_rects)
        {
            // Compute direction-aware corner character for the elbow
            let mut dirs = 0u8;
            if x2 > x1 { dirs |= 8; } else if x2 < x1 { dirs |= 4; }
            if y2 > y1 { dirs |= 2; } else if y2 < y1 { dirs |= 1; }
            canvas.set_char(x2, y1, directions_to_char(dirs));
        }
    }
}

/// Renders proper Unicode box-drawing junction characters at route turning
/// points and shared waypoints, based on which directions have lines.
pub(super) fn render_junctions(
    canvas: &mut DisplayCanvas,
    routes: &[Vec<(usize, usize)>],
    node_rects: &[GridRect],
) {
    // BTreeMap: deterministic iteration (AGENTS.md §3)
    let mut junction_dirs: BTreeMap<(usize, usize), u8> = BTreeMap::new();

    for points in routes {
        let n = points.len();
        if n < 2 {
            continue;
        }

        for (i, &point) in points.iter().enumerate() {
            let mut dirs = 0u8;
            if i > 0 {
                dirs |= direction_bit(point, points[i - 1]);
            }
            if i + 1 < n {
                dirs |= direction_bit(point, points[i + 1]);
            }
            if dirs != 0 {
                *junction_dirs.entry(point).or_insert(0) |= dirs;
            }
        }
    }

    // Only render junctions that connect multiple directions
    for ((x, y), dirs) in &junction_dirs {
        if canvas.is_interior(*x, *y, node_rects) || is_on_rect_boundary(*x, *y, node_rects) {
            continue;
        }
        if dirs.count_ones() >= 2 {
            canvas.ensure(*y, *x);
            // Don't overwrite arrow characters placed by draw_edge_route.
            if let Cell::Char(existing) = canvas.rows[*y][*x] {
                if matches!(existing, '▶' | '◀' | '▲' | '▼') {
                    continue;
                }
            }
            canvas.set_char(*x, *y, directions_to_char(*dirs));
        }
    }
}

fn direction_bit(from: (usize, usize), to: (usize, usize)) -> u8 {
    if to.1 < from.1 {
        1 // UP
    } else if to.1 > from.1 {
        2 // DOWN
    } else if to.0 < from.0 {
        4 // LEFT
    } else if to.0 > from.0 {
        8 // RIGHT
    } else {
        0
    }
}

fn maybe_merge_endpoint(
    canvas: &mut DisplayCanvas,
    x: usize,
    y: usize,
    ch: char,
    node_rects: &[GridRect],
) {
    if canvas.is_interior(x, y, node_rects) {
        return;
    }
    // On a box boundary: preserve the box border character (don't overwrite corners).
    if is_on_rect_boundary(x, y, node_rects) {
        return;
    }
    canvas.ensure(y, x);
    if let Cell::Char(existing) = canvas.rows[y][x] {
        if is_line(existing) || is_junction(existing) {
            draw_line_char(canvas, x, y, ch);
        }
    }
}

pub(super) fn is_on_rect_boundary(x: usize, y: usize, rects: &[GridRect]) -> bool {
    rects.iter().any(|rect| {
        let within_x = x >= rect.x && x < rect.x + rect.w;
        let within_y = y >= rect.y && y < rect.y + rect.h;
        (within_x && (y == rect.y || y == rect.y + rect.h.saturating_sub(1)))
            || (within_y && (x == rect.x || x == rect.x + rect.w.saturating_sub(1)))
    })
}

/// Whether the canvas cell currently holds a box corner glyph.
fn is_box_corner(canvas: &DisplayCanvas, x: usize, y: usize) -> bool {
    matches!(
        canvas.rows.get(y).and_then(|row| row.get(x)),
        Some(Cell::Char('┌' | '┐' | '└' | '┘'))
    )
}

fn draw_line_char(canvas: &mut DisplayCanvas, x: usize, y: usize, ch: char) {
    canvas.ensure(y, x);
    let merged = match canvas.rows[y][x] {
        Cell::Empty | Cell::WideCont => ch,
        Cell::Char(existing) => merge_line_char(existing, ch),
    };
    canvas.set_char(x, y, merged);
}

fn merge_line_char(existing: char, incoming: char) -> char {
    if existing == incoming {
        return existing;
    }
    // Preserve arrow characters
    if matches!(existing, '▶' | '◀' | '▲' | '▼') {
        return existing;
    }
    // Merge using direction-aware logic
    let dirs = char_directions(existing) | char_directions(incoming);
    if dirs == 0 {
        return incoming;
    }
    directions_to_char(dirs)
}

fn is_line(ch: char) -> bool {
    matches!(ch, '─' | '│' | '╌' | '╎')
}

fn is_junction(ch: char) -> bool {
    matches!(ch, '┌' | '┐' | '└' | '┘' | '├' | '┤' | '┬' | '┴' | '┼')
}

/// Returns the set of directions a box-drawing or line character connects to.
fn char_directions(ch: char) -> u8 {
    const UP: u8 = 1;
    const DOWN: u8 = 2;
    const LEFT: u8 = 4;
    const RIGHT: u8 = 8;
    match ch {
        '│' | '╎' => UP | DOWN,
        '─' | '╌' => LEFT | RIGHT,
        '┌' => DOWN | RIGHT,
        '┐' => DOWN | LEFT,
        '└' => UP | RIGHT,
        '┘' => UP | LEFT,
        '├' => UP | DOWN | RIGHT,
        '┤' => UP | DOWN | LEFT,
        '┬' => DOWN | LEFT | RIGHT,
        '┴' => UP | LEFT | RIGHT,
        '┼' => UP | DOWN | LEFT | RIGHT,
        _ => 0,
    }
}

/// Maps a set of direction bits to the appropriate Unicode box-drawing character.
fn directions_to_char(dirs: u8) -> char {
    const UP: u8 = 1;
    const DOWN: u8 = 2;
    const LEFT: u8 = 4;
    const RIGHT: u8 = 8;
    match dirs {
        d if d == (DOWN | RIGHT) => '┌',
        d if d == (DOWN | LEFT) => '┐',
        d if d == (UP | RIGHT) => '└',
        d if d == (UP | LEFT) => '┘',
        d if d == (UP | DOWN) => '│',
        d if d == (LEFT | RIGHT) => '─',
        d if d == (UP | DOWN | RIGHT) => '├',
        d if d == (UP | DOWN | LEFT) => '┤',
        d if d == (DOWN | LEFT | RIGHT) => '┬',
        d if d == (UP | LEFT | RIGHT) => '┴',
        d if d == (UP | DOWN | LEFT | RIGHT) => '┼',
        _ => '·',
    }
}

/// 箭头放在终点前一格，避免与节点边框重叠
fn arrow_before_end(x1: usize, y1: usize, x2: usize, y2: usize) -> Option<(usize, usize, char)> {
    if x1 == x2 {
        if y2 > y1 {
            Some((x2, y2.saturating_sub(1), ARROW_DOWN))
        } else if y2 < y1 {
            Some((x2, y2 + 1, ARROW_UP))
        } else {
            None
        }
    } else if y1 == y2 {
        if x2 > x1 {
            Some((x2.saturating_sub(1), y2, ARROW_RIGHT))
        } else if x2 < x1 {
            Some((x2 + 1, y2, ARROW_LEFT))
        } else {
            None
        }
    } else {
        direction_arrow(x1, y1, x2, y2).map(|ch| (x2, y2, ch))
    }
}

fn direction_arrow(x1: usize, y1: usize, x2: usize, y2: usize) -> Option<char> {
    let dx = x2 as i32 - x1 as i32;
    let dy = y2 as i32 - y1 as i32;
    if dx.abs() >= dy.abs() {
        if dx > 0 {
            Some(ARROW_RIGHT)
        } else if dx < 0 {
            Some(ARROW_LEFT)
        } else {
            None
        }
    } else if dy > 0 {
        Some(ARROW_DOWN)
    } else if dy < 0 {
        Some(ARROW_UP)
    } else {
        None
    }
}
