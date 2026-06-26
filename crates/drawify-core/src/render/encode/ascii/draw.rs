use crate::ast::{ArrowType, Diagram};
use crate::layout::{self, LayoutResult, Port};
use std::collections::HashMap;

use super::config::{AsciiDetectedEncoding, AsciiExportMetadata, AsciiExportOptions, AsciiExportResult};
use super::canvas::{text_width, truncate_string, Cell, DisplayCanvas, GridMapper, GridRect};
use super::{
    calculate_canvas_size, ARROW_DOWN, ARROW_LEFT, ARROW_RIGHT, ARROW_UP, BOX_BL,
    BOX_BR, BOX_H, BOX_TL, BOX_TR, BOX_V, DASH_H, DASH_V, GROUP_BL, GROUP_BR, GROUP_TL,
    GROUP_TR, NODE_PAD_H, PADDING,
};

pub(super) fn generate_ascii(
    diagram: &Diagram,
    layout: &LayoutResult,
    options: &AsciiExportOptions,
) -> Result<AsciiExportResult, super::AsciiExportError> {
    let has_title = diagram.title().is_some();
    let mapper = GridMapper::new(has_title);
    let (width, height) = calculate_canvas_size(layout, has_title);
    let mut canvas = DisplayCanvas::new(width, height);
    let mut node_rects: Vec<GridRect> = Vec::new();

    if let Some(title) = diagram.title() {
        let title = clean_label(title);
        let title = format!("=== {} ===", title);
        let title_x = centered_x(width, text_width(&title));
        canvas.write_text(title_x, 0, &title);
    }

    for group in &diagram.groups {
        if let Some(gl) = layout.groups.get(group.id.as_str()) {
            let (gx, gy) = mapper.to_grid(gl.x, gl.y);
            let gw = ((gl.width / super::SCALE_X).round() as usize).max(4);
            let gh = ((gl.height / super::SCALE_Y).round() as usize).max(3);
            draw_box(
                &mut canvas, gx, gy, gw, gh, GROUP_TL, GROUP_TR, GROUP_BL, GROUP_BR, BOX_V, BOX_H,
            );
            let label = clean_label(&group.label);
            canvas.write_text(gx + 2, gy, &label);
        }
    }

    for entity in &diagram.entities {
        if let Some(nl) = layout.nodes.get(entity.id.as_str()) {
            // Use original label for display-width-correct sizing
            let original_label = clean_label(&entity.label);
            let label_w = text_width(&original_label);
            let inner_w = label_w + NODE_PAD_H * 2;
            let layout_w = (nl.width / super::SCALE_X).round() as usize;
            let gw = inner_w.max(layout_w.min(inner_w + 6)).max(4);
            let gh = ((nl.height / super::SCALE_Y).round() as usize).max(3);
            let center_x = nl.x + nl.width / 2.0;
            let offset = center_x - (gw as f64 * super::SCALE_X / 2.0);
            let (gx, gy) = mapper.to_grid(offset, nl.y);

            node_rects.push(GridRect { x: gx, y: gy, w: gw, h: gh });
            draw_box(
                &mut canvas, gx, gy, gw, gh, BOX_TL, BOX_TR, BOX_BL, BOX_BR, BOX_V, BOX_H,
            );

            // Render label centered inside the box
            let max_label_w = gw.saturating_sub(NODE_PAD_H * 2);
            let display_label = truncate_string(&original_label, max_label_w);
            let lw = text_width(&display_label);
            let lx = gx + gw.saturating_sub(lw) / 2;
            let ly = gy + gh / 2;
            canvas.write_text(lx, ly, &display_label);
        }
    }

    let routes = build_ascii_routes(layout, &mapper);

    for (idx, relation) in diagram.relations.iter().enumerate() {
        if let Some(route) = routes.get(idx).and_then(Option::as_ref) {
            let dashed = matches!(relation.arrow, ArrowType::Passive);
            draw_edge_route(&mut canvas, route, dashed, &relation.arrow, &node_rects);
        }
    }

    // Post-process: render proper Unicode junction characters at route turning points
    render_junctions(&mut canvas, &routes, &node_rects);

    for (idx, relation) in diagram.relations.iter().enumerate() {
        if let Some(label) = &relation.label {
            if let Some(route) = routes.get(idx).and_then(Option::as_ref) {
                if route.points.len() >= 2 {
                    let label = clean_label(label);
                    place_edge_label(&mut canvas, route, &label);
                }
            }
        }
    }

    let text = canvas.to_string();
    let mut metadata = AsciiExportMetadata::new(options, AsciiDetectedEncoding::Utf8);
    metadata.output_bytes = text.len();
    Ok(AsciiExportResult { text, metadata })
}

fn centered_x(canvas_width: usize, content_width: usize) -> usize {
    PADDING + canvas_width.saturating_sub(PADDING * 2 + content_width) / 2
}

/// Renders proper Unicode box-drawing junction characters at route turning points
/// and shared waypoints. Computes the correct character based on which directions
/// have lines extending from the junction.
fn render_junctions(
    canvas: &mut DisplayCanvas,
    routes: &[Option<AsciiEdgeRoute>],
    node_rects: &[GridRect],
) {
    let mut junction_dirs: HashMap<(usize, usize), u8> = HashMap::new();

    for route in routes.iter().flatten() {
        let n = route.points.len();
        if n < 2 {
            continue;
        }

        for (i, &point) in route.points.iter().enumerate() {
            let mut dirs = 0u8;

            // Direction toward previous point
            if i > 0 {
                let prev = route.points[i - 1];
                dirs |= direction_bit(point, prev);
            }

            // Direction toward next point
            if i + 1 < n {
                let next = route.points[i + 1];
                dirs |= direction_bit(point, next);
            }

            if dirs != 0 {
                *junction_dirs.entry(point).or_insert(0) |= dirs;
            }
        }
    }

    // Only render junctions that connect multiple directions
    for ((x, y), dirs) in &junction_dirs {
        // Skip if inside or on box boundary
        if canvas.is_interior(*x, *y, node_rects) || is_on_rect_boundary(*x, *y, node_rects) {
            continue;
        }

        let dir_count = dirs.count_ones();
        if dir_count >= 2 {
            canvas.ensure(*y, *x);
            let ch = directions_to_char(*dirs);
            canvas.set_char(*x, *y, ch);
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

fn place_edge_label(canvas: &mut DisplayCanvas, route: &AsciiEdgeRoute, label: &str) {
    let lw = text_width(label);
    let canvas_width = canvas.rows.first().map(|row| row.len()).unwrap_or_default();

    if let Some(segment) = preferred_label_segment(route) {
        match segment.kind {
            SegmentKind::Horizontal => {
                let center = (segment.start.0 + segment.end.0) / 2;
                let x = center.saturating_sub(lw / 2);
                let y = segment.start.1.saturating_sub(1);
                canvas.clear_span(y, x.saturating_sub(1), lw + 2);
                canvas.write_text(x, y, label);
            }
            SegmentKind::Vertical => {
                let mid_y = (segment.start.1 + segment.end.1) / 2;
                let right_x = segment.start.0 + 2;
                let left_x = segment.start.0.saturating_sub(lw + 2);
                let x = if right_x + lw < canvas_width { right_x } else { left_x };
                canvas.clear_span(mid_y, x.saturating_sub(1), lw + 2);
                canvas.write_text(x, mid_y, label);
            }
            SegmentKind::Other => {}
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SegmentKind {
    Horizontal,
    Vertical,
    Other,
}

#[derive(Clone)]
struct AsciiEdgeRoute {
    points: Vec<(usize, usize)>,
}

#[derive(Clone, Copy)]
pub(super) struct RouteBusInfo {
    pub trunk_x: usize,
    pub count: usize,
}

#[derive(Clone, Copy)]
struct RouteSegment {
    start: (usize, usize),
    end: (usize, usize),
    kind: SegmentKind,
    len: usize,
}

fn build_ascii_routes(layout: &LayoutResult, mapper: &GridMapper) -> Vec<Option<AsciiEdgeRoute>> {
    let endpoints: Vec<Option<((usize, usize), (usize, usize))>> = layout
        .edges
        .iter()
        .map(|edge| {
            if edge.path_len() < 2 {
                return None;
            }
            let start_pt = edge.path_start().unwrap();
            let end_pt = edge.path_end().unwrap();
            let start = mapper.to_grid(start_pt.x, start_pt.y);
            let end = mapper.to_grid(end_pt.x, end_pt.y);
            Some((start, end))
        })
        .collect();

    let source_buses = cluster_vertical_buses(&layout.edges, &endpoints, AnchorSide::Start);
    let target_buses = cluster_vertical_buses(&layout.edges, &endpoints, AnchorSide::End);

    layout
        .edges
        .iter()
        .zip(endpoints.iter())
        .enumerate()
        .map(|(idx, (edge, endpoint))| {
            let Some((start, end)) = endpoint else {
                return None;
            };
            let points = route_edge_points(
                mapper,
                edge,
                *start,
                *end,
                source_buses.get(idx).copied().flatten(),
                target_buses.get(idx).copied().flatten(),
            );
            Some(AsciiEdgeRoute { points })
        })
        .collect()
}

pub(super) fn route_edge_points(
    mapper: &GridMapper,
    edge: &layout::EdgeLayout,
    start: (usize, usize),
    end: (usize, usize),
    source_bus: Option<RouteBusInfo>,
    target_bus: Option<RouteBusInfo>,
) -> Vec<(usize, usize)> {
    if is_vertical_flow_candidate(edge, start, end) {
        return prettify_vertical_flow(start, end, source_bus, target_bus);
    }

    if is_horizontal_flow_candidate(edge, start, end) {
        return prettify_horizontal_flow(start, end);
    }

    if edge.is_polyline() && edge.path_len() > 2 {
        return simplify_points(edge.path_points().iter().map(|p| mapper.to_grid(p.x, p.y)).collect());
    }

    let sampled = edge
        .sampled_path(12)
        .iter()
        .map(|p| mapper.to_grid(p.x, p.y))
        .collect::<Vec<_>>();
    simplify_points(sampled)
}

fn is_vertical_flow_candidate(
    edge: &layout::EdgeLayout,
    start: (usize, usize),
    end: (usize, usize),
) -> bool {
    edge.from_port == Port::Bottom
        && edge.to_port == Port::Top
        && end.1 > start.1
        && start.0 != end.0
}

fn is_horizontal_flow_candidate(
    edge: &layout::EdgeLayout,
    start: (usize, usize),
    end: (usize, usize),
) -> bool {
    matches!((edge.from_port, edge.to_port), (Port::Right, Port::Left) | (Port::Left, Port::Right))
        && start.1 != end.1
        && start.0 != end.0
}

fn prettify_vertical_flow(
    start: (usize, usize),
    end: (usize, usize),
    source_bus: Option<RouteBusInfo>,
    target_bus: Option<RouteBusInfo>,
) -> Vec<(usize, usize)> {
    let min_mid = start.1 + 1;
    let max_mid = end.1.saturating_sub(1).max(min_mid);
    if let Some(source_bus) = source_bus.filter(|bus| bus.count > 1) {
        let join_y = (start.1 + 1).min(max_mid);
        let mid_y = (start.1 + 2).min(max_mid);
        return simplify_points(vec![
            start,
            (start.0, join_y),
            (source_bus.trunk_x, join_y),
            (source_bus.trunk_x, mid_y),
            (end.0, mid_y),
            end,
        ]);
    }
    if let Some(target_bus) = target_bus.filter(|bus| bus.count > 1) {
        let merge_y = end.1.saturating_sub(2).max(min_mid);
        let mid_y = end.1.saturating_sub(3).max(min_mid);
        return simplify_points(vec![
            start,
            (start.0, mid_y),
            (target_bus.trunk_x, mid_y),
            (target_bus.trunk_x, merge_y),
            (end.0, merge_y),
            end,
        ]);
    }
    let mid_y = ((start.1 + end.1) / 2).clamp(min_mid, max_mid);
    simplify_points(vec![start, (start.0, mid_y), (end.0, mid_y), end])
}

fn prettify_horizontal_flow(start: (usize, usize), end: (usize, usize)) -> Vec<(usize, usize)> {
    let min_mid = start.0 + 1;
    let max_mid = end.0.saturating_sub(1).max(min_mid);
    let mid_x = ((start.0 + end.0) / 2).clamp(min_mid, max_mid);
    simplify_points(vec![start, (mid_x, start.1), (mid_x, end.1), end])
}

fn simplify_points(points: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
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

#[derive(Clone, Copy)]
enum AnchorSide {
    Start,
    End,
}

fn cluster_vertical_buses(
    edges: &[layout::EdgeLayout],
    endpoints: &[Option<((usize, usize), (usize, usize))>],
    side: AnchorSide,
) -> Vec<Option<RouteBusInfo>> {
    const CLUSTER_GAP: usize = 8;

    let mut result = vec![None; edges.len()];
    let mut by_row: HashMap<usize, Vec<(usize, usize)>> = HashMap::new();

    for (idx, (edge, endpoint)) in edges.iter().zip(endpoints.iter()).enumerate() {
        let Some((start, end)) = endpoint else {
            continue;
        };
        if !is_vertical_flow_candidate(edge, *start, *end) {
            continue;
        }
        let anchor = match side {
            AnchorSide::Start => *start,
            AnchorSide::End => *end,
        };
        by_row.entry(anchor.1).or_default().push((idx, anchor.0));
    }

    for anchors in by_row.values_mut() {
        anchors.sort_by_key(|(_, x)| *x);
        let mut cluster: Vec<(usize, usize)> = Vec::new();
        for &(idx, x) in anchors.iter() {
            let should_split = cluster
                .last()
                .map(|&(_, prev_x)| x.abs_diff(prev_x) > CLUSTER_GAP)
                .unwrap_or(false);
            if should_split {
                assign_cluster_bus(&mut result, &cluster);
                cluster.clear();
            }
            cluster.push((idx, x));
        }
        assign_cluster_bus(&mut result, &cluster);
    }

    result
}

fn assign_cluster_bus(result: &mut [Option<RouteBusInfo>], cluster: &[(usize, usize)]) {
    if cluster.len() < 2 {
        return;
    }
    let sum_x: usize = cluster.iter().map(|(_, x)| *x).sum();
    let trunk_x = sum_x / cluster.len();
    let bus = RouteBusInfo {
        trunk_x,
        count: cluster.len(),
    };
    for (idx, _) in cluster {
        result[*idx] = Some(bus);
    }
}

fn preferred_label_segment(route: &AsciiEdgeRoute) -> Option<RouteSegment> {
    let segments = route_segments(route);
    segments
        .iter()
        .copied()
        .filter(|segment| segment.kind == SegmentKind::Horizontal)
        .max_by_key(|segment| segment.len)
        .or_else(|| {
            segments
                .iter()
                .copied()
                .filter(|segment| segment.kind == SegmentKind::Vertical)
                .max_by_key(|segment| segment.len)
        })
}

fn route_segments(route: &AsciiEdgeRoute) -> Vec<RouteSegment> {
    route
        .points
        .windows(2)
        .map(|pair| {
            let start = pair[0];
            let end = pair[1];
            let kind = if start.0 == end.0 && start.1 != end.1 {
                SegmentKind::Vertical
            } else if start.1 == end.1 && start.0 != end.0 {
                SegmentKind::Horizontal
            } else {
                SegmentKind::Other
            };
            let len = start.0.abs_diff(end.0) + start.1.abs_diff(end.1);
            RouteSegment {
                start,
                end,
                kind,
                len,
            }
        })
        .collect()
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

fn is_on_rect_boundary(x: usize, y: usize, rects: &[GridRect]) -> bool {
    rects.iter().any(|rect| {
        let within_x = x >= rect.x && x < rect.x + rect.w;
        let within_y = y >= rect.y && y < rect.y + rect.h;
        (within_x && (y == rect.y || y == rect.y + rect.h.saturating_sub(1)))
            || (within_y && (x == rect.x || x == rect.x + rect.w.saturating_sub(1)))
    })
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
    matches!(ch, '─' | '│' | '·' | '¦')
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
        '│' | '¦' => UP | DOWN,
        '─' | '·' => LEFT | RIGHT,
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

pub(super) fn draw_box(
    canvas: &mut DisplayCanvas,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    tl: char,
    tr: char,
    bl: char,
    br: char,
    v: char,
    h_ch: char,
) {
    if w == 0 || h == 0 {
        return;
    }

    canvas.set_char(x, y, tl);
    for i in 1..w.saturating_sub(1) {
        canvas.set_char(x + i, y, h_ch);
    }
    if w > 1 {
        canvas.set_char(x + w - 1, y, tr);
    }

    if h > 1 {
        canvas.set_char(x, y + h - 1, bl);
        for i in 1..w.saturating_sub(1) {
            canvas.set_char(x + i, y + h - 1, h_ch);
        }
        if w > 1 {
            canvas.set_char(x + w - 1, y + h - 1, br);
        }
    }

    for j in 1..h.saturating_sub(1) {
        canvas.set_char(x, y + j, v);
        if w > 1 {
            canvas.set_char(x + w - 1, y + j, v);
        }
    }
}

fn draw_edge_route(
    canvas: &mut DisplayCanvas,
    route: &AsciiEdgeRoute,
    dashed: bool,
    arrow: &ArrowType,
    node_rects: &[GridRect],
) {
    for seg in route.points.windows(2) {
        let (x1, y1) = seg[0];
        let (x2, y2) = seg[1];
        draw_segment(canvas, x1, y1, x2, y2, dashed, node_rects);
    }

    let n = route.points.len();
    if n >= 2 {
        let (x1, y1) = route.points[n - 2];
        let (x2, y2) = route.points[n - 1];
        if let Some((ax, ay, ch)) = arrow_before_end(x1, y1, x2, y2) {
            if !canvas.is_interior(ax, ay, node_rects) {
                canvas.set_char(ax, ay, ch);
            }
        }

        if matches!(arrow, ArrowType::Bidirectional) {
            let (x0, y0) = route.points[0];
            if let Some((ax, ay, ch)) = arrow_before_end(x2, y2, x0, y0) {
                if !canvas.is_interior(ax, ay, node_rects) {
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
            if dashed && (y - start) % 2 == 1 {
                continue;
            }
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
            if dashed && (x - start) % 2 == 1 {
                continue;
            }
            if !canvas.is_interior(x, y1, node_rects)
                && !is_on_rect_boundary(x, y1, node_rects)
            {
                draw_line_char(canvas, x, y1, if dashed { DASH_H } else { BOX_H });
            }
        }
        maybe_merge_endpoint(canvas, end, y1, if dashed { DASH_H } else { BOX_H }, node_rects);
    } else {
        for x in x1.min(x2)..x1.max(x2) {
            if dashed && (x - x1.min(x2)) % 2 == 1 {
                continue;
            }
            if !canvas.is_interior(x, y1, node_rects)
                && !is_on_rect_boundary(x, y1, node_rects)
            {
                draw_line_char(canvas, x, y1, if dashed { DASH_H } else { BOX_H });
            }
        }
        for y in y1.min(y2)..y1.max(y2) {
            if dashed && (y - y1.min(y2)) % 2 == 1 {
                continue;
            }
            if !canvas.is_interior(x2, y, node_rects)
                && !is_on_rect_boundary(x2, y, node_rects)
            {
                draw_line_char(canvas, x2, y, if dashed { DASH_V } else { BOX_V });
            }
        }
        if !canvas.is_interior(x2, y1, node_rects)
            && !is_on_rect_boundary(x2, y1, node_rects)
        {
            // Compute direction-aware corner character for diagonal segments
            let mut dirs = 0u8;
            if x2 > x1 { dirs |= 8; } else if x2 < x1 { dirs |= 4; } // LEFT/RIGHT from horizontal
            if y2 > y1 { dirs |= 2; } else if y2 < y1 { dirs |= 1; } // UP/DOWN from vertical
            canvas.set_char(x2, y1, directions_to_char(dirs));
        }
    }
}

/// 箭头放在终点前一格，避免与节点边框重叠
pub(super) fn arrow_before_end(x1: usize, y1: usize, x2: usize, y2: usize) -> Option<(usize, usize, char)> {
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

/// Cleans a label for display: replaces control characters with spaces but preserves
/// Unicode characters (CJK, etc.) for correct display-width calculation.
fn clean_label(label: &str) -> String {
    label
        .chars()
        .map(|ch| {
            if ch.is_control() {
                ' '
            } else {
                ch
            }
        })
        .collect()
}

pub(super) fn direction_arrow(x1: usize, y1: usize, x2: usize, y2: usize) -> Option<char> {
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
