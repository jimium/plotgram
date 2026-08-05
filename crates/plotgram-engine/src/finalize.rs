//! Group envelopes, labels, canvas — after nodes/edges are final.

use plotgram_router::core::union_rects;
use plotgram_model::diagnostics::LayoutDiagnostics;
use plotgram_model::geometry::{Point, Rect};
use plotgram_model::graph::Graph;
use plotgram_model::result::{
    EdgePath, EdgePlacement, GroupPlacement, LabelOwner, LabelSlot, LayoutResult, NodePlacement,
};

const GROUP_PAD: f64 = 16.0;
const CANVAS_PAD: f64 = 24.0;
/// Must stay >= the label band height `simple_labels` draws (18.0) plus a
/// little breathing room, or the band overlaps the topmost member — a plain
/// `GROUP_PAD` on all sides isn't tall enough for a labeled group's top.
const GROUP_LABEL_TOP_PAD: f64 = 24.0;

pub fn finalize(
    graph: &Graph,
    mut nodes: Vec<NodePlacement>,
    mut edges: Vec<EdgePlacement>,
    diagnostics: LayoutDiagnostics,
) -> LayoutResult {
    let mut groups = group_frames(graph, &nodes);

    // Uniform whole-graph translate: move the content bbox so its top-left
    // sits at (CANVAS_PAD, CANVAS_PAD). `canvas_size` then adds the same
    // padding to the right/bottom, yielding symmetric margins on all sides.
    let (dx, dy) = content_shift(&nodes, &groups);
    if dx != 0.0 || dy != 0.0 {
        for n in &mut nodes {
            n.frame.x += dx;
            n.frame.y += dy;
        }
        for e in &mut edges {
            translate_edge_path(&mut e.path, dx, dy);
        }
        for g in &mut groups {
            g.frame.x += dx;
            g.frame.y += dy;
        }
    }

    let labels = simple_labels(graph, &nodes, &groups);
    let (canvas_width, canvas_height) = canvas_size(&nodes, &groups);

    LayoutResult {
        nodes,
        edges,
        groups,
        labels,
        canvas_width,
        canvas_height,
        diagnostics,
    }
}

/// Shift that moves the content bounding box origin to (CANVAS_PAD, CANVAS_PAD).
/// Layouts normalize their output to start at (0, 0); this adds the uniform
/// canvas padding on every side.
fn content_shift(nodes: &[NodePlacement], groups: &[GroupPlacement]) -> (f64, f64) {
    let mut rects: Vec<Rect> = nodes.iter().map(|n| n.frame).collect();
    rects.extend(groups.iter().map(|g| g.frame));
    match union_rects(&rects) {
        Some(u) => (CANVAS_PAD - u.x, CANVAS_PAD - u.y),
        None => (0.0, 0.0),
    }
}

fn translate_edge_path(path: &mut EdgePath, dx: f64, dy: f64) {
    let shift = |p: &mut Point| {
        p.x += dx;
        p.y += dy;
    };
    match path {
        EdgePath::Polyline { points } => {
            for p in points {
                shift(p);
            }
        }
        EdgePath::Cubic {
            start,
            end,
            controls,
        } => {
            shift(start);
            shift(end);
            for c in controls {
                shift(c);
            }
        }
    }
}

fn group_frames(graph: &Graph, nodes: &[NodePlacement]) -> Vec<GroupPlacement> {
    let mut out = Vec::new();
    for g in &graph.groups {
        collect_group(g, nodes, &mut out);
    }
    out
}

fn collect_group(
    group: &plotgram_model::graph::Group,
    nodes: &[NodePlacement],
    out: &mut Vec<GroupPlacement>,
) {
    for child in &group.groups {
        collect_group(child, nodes, out);
    }

    let member_ids = group_member_node_ids(group);
    let frames: Vec<Rect> = nodes
        .iter()
        .filter(|n| member_ids.iter().any(|id| id == &n.id))
        .map(|n| n.frame)
        .collect();

    // Nested group frames already computed — include them in envelope.
    let nested: Vec<Rect> = out
        .iter()
        .filter(|gp| group.groups.iter().any(|c| c.id == gp.id))
        .map(|gp| gp.frame)
        .collect();

    let mut all = frames;
    all.extend(nested);

    if let Some(bbox) = union_rects(&all) {
        let top_pad = if group.label.is_some() {
            GROUP_LABEL_TOP_PAD
        } else {
            GROUP_PAD
        };
        let frame = Rect::new(
            bbox.x - GROUP_PAD,
            bbox.y - top_pad,
            bbox.width + GROUP_PAD * 2.0,
            bbox.height + top_pad + GROUP_PAD,
        );
        out.push(GroupPlacement {
            id: group.id.clone(),
            frame,
        });
    }
}

fn group_member_node_ids(group: &plotgram_model::graph::Group) -> Vec<String> {
    let mut ids = Vec::new();
    for n in &group.nodes {
        ids.push(n.id.clone());
    }
    for g in &group.groups {
        ids.extend(group_member_node_ids(g));
    }
    ids
}

fn simple_labels(
    graph: &Graph,
    nodes: &[NodePlacement],
    groups: &[GroupPlacement],
) -> Vec<LabelSlot> {
    let mut labels = Vec::new();
    for n in nodes {
        if let Some(node) = graph.find_node(&n.id) {
            if let Some(text) = node.label.as_ref() {
                labels.push(LabelSlot {
                    owner: LabelOwner::Node(n.id.clone()),
                    role: None,
                    text: text.clone(),
                    frame: n.frame,
                });
            }
        }
    }
    for g in groups {
        if let Some(group) = find_group(graph, &g.id) {
            if let Some(text) = group.label.as_ref() {
                let band = Rect::new(g.frame.x, g.frame.y, g.frame.width, 18.0);
                labels.push(LabelSlot {
                    owner: LabelOwner::Group(g.id.clone()),
                    role: None,
                    text: text.clone(),
                    frame: band,
                });
            }
        }
    }
    labels
}

fn find_group<'a>(graph: &'a Graph, id: &str) -> Option<&'a plotgram_model::graph::Group> {
    fn walk<'a>(
        g: &'a plotgram_model::graph::Group,
        id: &str,
    ) -> Option<&'a plotgram_model::graph::Group> {
        if g.id == id {
            return Some(g);
        }
        for c in &g.groups {
            if let Some(f) = walk(c, id) {
                return Some(f);
            }
        }
        None
    }
    for g in &graph.groups {
        if let Some(f) = walk(g, id) {
            return Some(f);
        }
    }
    None
}

fn canvas_size(nodes: &[NodePlacement], groups: &[GroupPlacement]) -> (f64, f64) {
    let mut rects: Vec<Rect> = nodes.iter().map(|n| n.frame).collect();
    rects.extend(groups.iter().map(|g| g.frame));
    match union_rects(&rects) {
        Some(u) => (u.right() + CANVAS_PAD, u.bottom() + CANVAS_PAD),
        None => (CANVAS_PAD * 2.0, CANVAS_PAD * 2.0),
    }
}
