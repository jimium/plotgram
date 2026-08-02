//! Group envelopes, labels, canvas — after nodes/edges are final.

use plotgram_model::geometry::Rect;
use plotgram_model::graph::Graph;
use plotgram_model::result::{
    GroupPlacement, LabelOwner, LabelSlot, LayoutResult, NodePlacement,
};
use plotgram_router::core::expand_union;

const GROUP_PAD: f64 = 16.0;
const CANVAS_PAD: f64 = 24.0;

pub fn finalize(
    graph: &Graph,
    nodes: Vec<NodePlacement>,
    edges: Vec<plotgram_model::result::EdgePlacement>,
) -> LayoutResult {
    let groups = group_frames(graph, &nodes);
    let labels = simple_labels(graph, &nodes, &groups);
    let (canvas_width, canvas_height) = canvas_size(&nodes, &groups);

    LayoutResult {
        nodes,
        edges,
        groups,
        labels,
        canvas_width,
        canvas_height,
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

    if let Some(frame) = expand_union(&all, GROUP_PAD) {
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

fn find_group<'a>(
    graph: &'a Graph,
    id: &str,
) -> Option<&'a plotgram_model::graph::Group> {
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
    match plotgram_router::core::union_rects(&rects) {
        Some(u) => (
            u.right() + CANVAS_PAD,
            u.bottom() + CANVAS_PAD,
        ),
        None => (CANVAS_PAD * 2.0, CANVAS_PAD * 2.0),
    }
}
