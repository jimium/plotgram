//! ASCII (Unicode box-drawing) text backend.
//!
//! Entry point: [`render_ascii`]. Consumes the same [`RenderInput`] as the SVG
//! backend and produces a plain-text diagram. Scope: simple flowcharts —
//! groups are skipped, every shape degrades to a rectangular box, and edge
//! paths are taken verbatim from layout (grid quantization + collinear
//! simplification only; no re-routing).

mod canvas;
mod draw;

use plotgram_model::graph::Arrow;
use plotgram_model::render::RenderInput;
use plotgram_model::result::LabelOwner;

use canvas::{text_width, truncate_string, DisplayCanvas, GridMapper, GridRect};
use draw::{clean_label, draw_box, draw_edge_route, render_junctions, simplify_points, BoxChars};

/// Pixels per character column (1 char ≈ 6×12 px).
pub(crate) const SCALE_X: f64 = 6.0;
/// Pixels per character row.
pub(crate) const SCALE_Y: f64 = 12.0;
/// Blank margin (in cells) around the diagram.
pub(crate) const PADDING: usize = 2;
/// Horizontal padding inside a node box, per side.
const NODE_PAD_H: usize = 2;

const BOX_H: char = '─';
const BOX_V: char = '│';
const BOX_TL: char = '┌';
const BOX_TR: char = '┐';
const BOX_BL: char = '└';
const BOX_BR: char = '┘';
const ARROW_RIGHT: char = '▶';
const ARROW_DOWN: char = '▼';
const ARROW_LEFT: char = '◀';
const ARROW_UP: char = '▲';
const DASH_H: char = '·';
const DASH_V: char = '¦';

/// Render a complete diagram to Unicode text.
pub fn render_ascii(input: &RenderInput) -> String {
    let title = input
        .meta
        .title
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());
    let mapper = GridMapper::new(title.is_some());
    let title_rows = if title.is_some() { 2 } else { 0 };

    // ── Canvas size: quantized layout canvas + padding, with a sane floor ──
    let width = ((input.layout.canvas_width / SCALE_X).ceil() as usize + PADDING * 2 + 4).max(40);
    let height =
        ((input.layout.canvas_height / SCALE_Y).ceil() as usize + PADDING * 2 + title_rows + 4)
            .max(20);
    let mut cv = DisplayCanvas::new(width, height);

    // ── Title: centered `=== title ===` on the first row ──
    if let Some(t) = title {
        let text = format!("=== {} ===", clean_label(t));
        let x = width.saturating_sub(text_width(&text)) / 2;
        cv.write_text(x, 0, &text);
    }

    // ── Nodes: every shape becomes a rectangular box ──
    // (box geometry is computed first so edges can skip interiors/boundaries)
    struct NodeBox {
        rect: GridRect,
        label: String,
    }
    let mut boxes: Vec<NodeBox> = Vec::new();
    for np in &input.layout.nodes {
        let label_text = input
            .layout
            .labels
            .iter()
            .find(|ls| ls.owner == LabelOwner::Node(np.id.clone()))
            .map(|ls| ls.text.as_str())
            .or_else(|| {
                input
                    .graph
                    .find_node(&np.id)
                    .and_then(|n| n.label.as_deref())
            })
            .unwrap_or("");
        let label = clean_label(label_text);
        let lw = text_width(&label);

        // Grid width: at least label + padding; frame width quantized, but never
        // stretch a box absurdly wider than its label (V1 policy).
        let frame_gw = (np.frame.width / SCALE_X).round() as usize;
        let gw = (lw + NODE_PAD_H * 2).max(frame_gw.min(lw + 10)).max(4);
        let gh = ((np.frame.height / SCALE_Y).round() as usize).max(3);

        // Align on the frame's center x so quantization doesn't drift boxes.
        let (cx, gy) = mapper.to_grid(np.frame.x + np.frame.width / 2.0, np.frame.y);
        let gx = cx.saturating_sub(gw / 2);
        boxes.push(NodeBox {
            rect: GridRect { x: gx, y: gy, w: gw, h: gh },
            label,
        });
    }
    let node_rects: Vec<GridRect> = boxes
        .iter()
        .map(|b| GridRect { x: b.rect.x, y: b.rect.y, w: b.rect.w, h: b.rect.h })
        .collect();

    for b in &boxes {
        let chars = BoxChars {
            tl: BOX_TL, tr: BOX_TR, bl: BOX_BL, br: BOX_BR, v: BOX_V, h: BOX_H,
        };
        draw_box(&mut cv, b.rect.x, b.rect.y, b.rect.w, b.rect.h, &chars);
    }

    // ── Edges: quantize layout polylines, simplify, draw ──
    let mut routes: Vec<Vec<(usize, usize)>> = Vec::new();
    for ep in &input.layout.edges {
        let points: Vec<(usize, usize)> = ep
            .path
            .points
            .iter()
            .map(|p| mapper.to_grid(p.x, p.y))
            .collect();
        let points = simplify_points(points);
        if points.len() < 2 {
            continue;
        }
        let arrow = input
            .graph
            .find_edge(&ep.id)
            .map(|e| e.arrow)
            .unwrap_or(Arrow::Forward);
        let dashed = arrow == Arrow::Response;
        let bidirectional = arrow == Arrow::Bidirectional;
        draw_edge_route(&mut cv, &points, dashed, bidirectional, &node_rects);
        routes.push(points);
    }

    // ── Junction characters where routes bend or share cells ──
    render_junctions(&mut cv, &routes, &node_rects);

    // ── Node labels: centered inside the box, truncated with `…` ──
    for b in &boxes {
        if b.label.is_empty() {
            continue;
        }
        let max_w = b.rect.w.saturating_sub(2);
        let label = truncate_string(&b.label, max_w);
        let lw = text_width(&label);
        let lx = b.rect.x + (b.rect.w.saturating_sub(lw)) / 2;
        let ly = b.rect.y + b.rect.h / 2;
        cv.write_text(lx, ly, &label);
    }

    // ── Edge labels: centered on the slot frame (layout already decided where) ──
    for ls in &input.layout.labels {
        let LabelOwner::Edge(_) = &ls.owner else { continue };
        let label = clean_label(&ls.text);
        if label.is_empty() {
            continue;
        }
        let lw = text_width(&label);
        let (cx, cy) = mapper.to_grid(
            ls.frame.x + ls.frame.width / 2.0,
            ls.frame.y + ls.frame.height / 2.0,
        );
        let lx = cx.saturating_sub(lw / 2);
        // One space of margin each side so the text doesn't touch line chars.
        cv.clear_span(cy, lx.saturating_sub(1), lw + 2);
        cv.write_text(lx, cy, &label);
    }

    // Group placements and group labels are intentionally skipped.

    cv.render()
}

#[cfg(test)]
mod tests {
    use super::*;
    use plotgram_model::attr::AttrMap;
    use plotgram_model::geometry::{Point, Rect};
    use plotgram_model::graph::{Edge, Graph, Node};
    use plotgram_model::render::RenderMeta;
    use plotgram_model::result::{
        EdgePath, EdgePlacement, LabelSlot, LayoutResult, NodePlacement,
    };

    fn node(id: &str, label: &str) -> Node {
        Node {
            id: id.to_string(),
            label: Some(label.to_string()),
            shape: None,
            attrs: AttrMap::new(),
        }
    }

    fn edge(id: &str, source: &str, target: &str, arrow: Arrow) -> Edge {
        Edge {
            id: id.to_string(),
            source: source.to_string(),
            target: target.to_string(),
            arrow,
            label: None,
            head_label: None,
            tail_label: None,
            attrs: AttrMap::new(),
        }
    }

    fn place(id: &str, x: f64, y: f64, w: f64, h: f64) -> NodePlacement {
        NodePlacement { id: id.to_string(), frame: Rect::new(x, y, w, h) }
    }

    fn route(id: &str, source: &str, target: &str, pts: &[(f64, f64)]) -> EdgePlacement {
        EdgePlacement {
            id: id.to_string(),
            source: source.to_string(),
            target: target.to_string(),
            path: EdgePath {
                points: pts.iter().map(|&(x, y)| Point { x, y }).collect(),
            },
        }
    }

    fn node_label(id: &str, text: &str, frame: Rect) -> LabelSlot {
        LabelSlot {
            owner: LabelOwner::Node(id.to_string()),
            role: None,
            text: text.to_string(),
            frame,
        }
    }

    /// Two horizontally-placed nodes joined by one edge.
    fn two_node_input(arrow: Arrow, labels: (&str, &str), title: Option<&str>) -> RenderInput {
        let graph = Graph {
            nodes: vec![node("a", labels.0), node("b", labels.1)],
            edges: vec![edge("e1", "a", "b", arrow)],
            groups: vec![],
        };
        let layout = LayoutResult {
            nodes: vec![place("a", 0.0, 24.0, 96.0, 36.0), place("b", 240.0, 24.0, 96.0, 36.0)],
            edges: vec![route("e1", "a", "b", &[(96.0, 42.0), (240.0, 42.0)])],
            groups: vec![],
            labels: vec![
                node_label("a", labels.0, Rect::new(0.0, 24.0, 96.0, 36.0)),
                node_label("b", labels.1, Rect::new(240.0, 24.0, 96.0, 36.0)),
            ],
            canvas_width: 360.0,
            canvas_height: 96.0,
        };
        RenderInput {
            graph,
            layout,
            meta: RenderMeta {
                title: title.map(|t| t.to_string()),
                theme: None,
                render_style: None,
            },
        }
    }

    #[test]
    fn two_nodes_one_edge_draws_boxes_and_arrow() {
        let out = render_ascii(&two_node_input(Arrow::Forward, ("Start", "End"), None));
        for ch in ['┌', '┐', '└', '┘', '─', '│', '▶'] {
            assert!(out.contains(ch), "missing {ch:?} in:\n{out}");
        }
        assert!(out.contains("Start"), "missing node label in:\n{out}");
        assert!(out.contains("End"), "missing node label in:\n{out}");
    }

    #[test]
    fn response_arrow_renders_dashed() {
        let out = render_ascii(&two_node_input(Arrow::Response, ("A", "B"), None));
        assert!(
            out.contains(DASH_H) || out.contains(DASH_V),
            "expected dashed chars in:\n{out}"
        );
    }

    #[test]
    fn bidirectional_arrow_renders_both_heads() {
        let out = render_ascii(&two_node_input(Arrow::Bidirectional, ("A", "B"), None));
        assert!(out.contains('▶'), "missing forward head in:\n{out}");
        assert!(out.contains('◀'), "missing reverse head in:\n{out}");
    }

    #[test]
    fn cjk_label_survives_intact() {
        let out = render_ascii(&two_node_input(Arrow::Forward, ("用户登录", "完成"), None));
        assert!(out.contains("用户登录"), "CJK label broken in:\n{out}");
        assert!(out.contains("完成"), "CJK label broken in:\n{out}");
    }

    #[test]
    fn title_renders_on_first_line() {
        let out = render_ascii(&two_node_input(Arrow::Forward, ("A", "B"), Some("登录流程")));
        let first = out.lines().next().unwrap_or("");
        assert!(
            first.contains("=== 登录流程 ==="),
            "title missing from first line:\n{out}"
        );
    }

    #[test]
    fn edge_label_written_at_slot() {
        let mut input = two_node_input(Arrow::Forward, ("A", "B"), None);
        input.layout.labels.push(LabelSlot {
            owner: LabelOwner::Edge("e1".to_string()),
            role: None,
            text: "yes".to_string(),
            frame: Rect::new(150.0, 30.0, 36.0, 12.0),
        });
        let out = render_ascii(&input);
        assert!(out.contains("yes"), "edge label missing in:\n{out}");
    }

    #[test]
    fn interior_blank_rows_are_preserved() {
        // Two disconnected nodes stacked vertically with a gap: the blank
        // rows between them carry geometry and must not be collapsed.
        let graph = Graph {
            nodes: vec![node("a", "Top"), node("b", "Bottom")],
            edges: vec![],
            groups: vec![],
        };
        let layout = LayoutResult {
            nodes: vec![place("a", 0.0, 0.0, 96.0, 36.0), place("b", 0.0, 120.0, 96.0, 36.0)],
            edges: vec![],
            groups: vec![],
            labels: vec![
                node_label("a", "Top", Rect::new(0.0, 0.0, 96.0, 36.0)),
                node_label("b", "Bottom", Rect::new(0.0, 120.0, 96.0, 36.0)),
            ],
            canvas_width: 96.0,
            canvas_height: 156.0,
        };
        let input = RenderInput {
            graph,
            layout,
            meta: RenderMeta { title: None, theme: None, render_style: None },
        };
        let out = render_ascii(&input);
        let lines: Vec<&str> = out.lines().collect();
        let top_bottom = lines.iter().position(|l| l.contains('└')).unwrap();
        let bottom_top = lines.iter().rposition(|l| l.contains('┌')).unwrap();
        assert!(
            lines[top_bottom + 1..bottom_top].iter().any(|l| l.is_empty()),
            "blank gap rows between boxes must survive:\n{out}"
        );
        // No leading/trailing blank lines
        assert!(!lines.first().unwrap().is_empty(), "leading blanks trimmed:\n{out}");
        assert!(!lines.last().unwrap().is_empty(), "trailing blanks trimmed:\n{out}");
    }

    #[test]
    fn groups_are_skipped() {
        use plotgram_model::result::GroupPlacement;
        let mut input = two_node_input(Arrow::Forward, ("A", "B"), None);
        input.layout.groups.push(GroupPlacement {
            id: "g1".to_string(),
            frame: Rect::new(0.0, 0.0, 400.0, 120.0),
        });
        input.layout.labels.push(LabelSlot {
            owner: LabelOwner::Group("g1".to_string()),
            role: None,
            text: "GROUP-TITLE".to_string(),
            frame: Rect::new(0.0, 0.0, 100.0, 12.0),
        });
        let out = render_ascii(&input);
        assert!(!out.contains("GROUP-TITLE"), "group label should be skipped:\n{out}");
    }
}
