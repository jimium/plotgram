//! Visual preview: hand-crafted RenderInput covering all 12 shapes,
//! variant styles, a group, edges and labels. Outputs SVG files for eyeballing.
//!
//! Run: `cargo run -p plotgram-render --example preview`
//! Output: `target/render-preview/*.svg`

use plotgram_model::attr::{AttrMap, AttrValue};
use plotgram_model::geometry::{Point, Rect};
use plotgram_model::graph::{Arrow, Edge, Graph, Node};
use plotgram_model::render::{RenderInput, RenderMeta};
use plotgram_model::result::{
    EdgePath, EdgePlacement, GroupPlacement, LabelOwner, LabelSlot, LayoutResult, NodePlacement,
};
use plotgram_render::render_svg;

fn node(id: &str, label: &str, shape: Option<&str>, variant: Option<&str>) -> Node {
    let mut attrs = AttrMap::new();
    if let Some(v) = variant {
        attrs.insert("variant".to_string(), AttrValue::Atom(v.to_string()));
    }
    Node {
        id: id.to_string(),
        label: Some(label.to_string()),
        shape: shape.map(|s| s.to_string()),
        role: Default::default(),
        host_group: None,
        anchor: None,
        attrs,
    }
}

fn edge(id: &str, source: &str, target: &str) -> Edge {
    edge_with(id, source, target, Arrow::Forward)
}

fn edge_with(id: &str, source: &str, target: &str, arrow: Arrow) -> Edge {
    Edge {
        id: id.to_string(),
        source: source.to_string(),
        target: target.to_string(),
        arrow,
        label: None,
        head_label: None,
        tail_label: None,
        from_port: None,
        to_port: None,
        edge_group: None,
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
        from_port: None,
        to_port: None,
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

fn build_input(theme: Option<&str>, render_style: Option<&str>) -> RenderInput {
    // ── Graph: 12 shapes, variants where themes define paint overrides ──
    let graph = Graph {
        nodes: vec![
            // Row 1: a small flow
            node("start", "开始", Some("stadium"), None),
            node("check", "库存足够?", Some("diamond"), Some("info")),
            node("svc", "订单服务", Some("rounded_rect"), Some("primary")),
            node("db", "订单库", Some("cylinder"), Some("secondary")),
            // Row 2: process-ish shapes
            node("doc", "对账单", Some("document"), None),
            node("sub", "扣减库存", Some("subprocess"), None),
            node("para", "导入数据", Some("parallelogram"), None),
            node("gw", "网关", Some("hexagon"), Some("info")),
            // Row 3: actors & misc
            node("buyer", "买家", Some("person"), Some("secondary")),
            node("cache", "缓存", Some("circle"), Some("primary")),
            node("cdn", "CDN", Some("cloud"), Some("muted")),
            node("plain", "普通矩形", Some("rect"), None),
        ],
        edges: vec![
            edge("e1", "start", "check"),
            edge("e2", "check", "svc"),
            edge("e3", "check", "sub"),
            edge("e4", "svc", "db"),
            // `-->` response: renders dashed
            edge_with("e5", "doc", "buyer", Arrow::Response),
            // `<->` bidirectional: arrowheads on both ends
            edge_with("e6", "cdn", "plain", Arrow::Bidirectional),
        ],
        groups: vec![],
    };

    // ── Hand layout ──
    let nodes = vec![
        place("start", 40.0, 48.0, 110.0, 44.0),
        place("check", 210.0, 38.0, 130.0, 64.0),
        place("svc", 410.0, 48.0, 130.0, 44.0),
        place("db", 600.0, 40.0, 100.0, 60.0),
        place("doc", 40.0, 170.0, 110.0, 60.0),
        place("sub", 210.0, 172.0, 130.0, 56.0),
        place("para", 410.0, 175.0, 130.0, 50.0),
        place("gw", 600.0, 172.0, 100.0, 56.0),
        place("buyer", 67.0, 300.0, 56.0, 72.0),
        place("cache", 240.0, 310.0, 70.0, 70.0),
        place("cdn", 380.0, 300.0, 140.0, 80.0),
        place("plain", 580.0, 315.0, 130.0, 50.0),
    ];

    let edges = vec![
        route("e1", "start", "check", &[(150.0, 70.0), (210.0, 70.0)]),
        route("e2", "check", "svc", &[(340.0, 70.0), (410.0, 70.0)]),
        route("e3", "check", "sub", &[(275.0, 102.0), (275.0, 172.0)]),
        route("e4", "svc", "db", &[(540.0, 70.0), (600.0, 70.0)]),
        route("e5", "doc", "buyer", &[(95.0, 230.0), (95.0, 300.0)]),
        route("e6", "cdn", "plain", &[(520.0, 340.0), (580.0, 340.0)]),
    ];

    // Group wrapping the backend pair (svc + db)
    let groups = vec![GroupPlacement {
        id: "backend".to_string(),
        frame: Rect::new(395.0, 18.0, 320.0, 96.0),
    }];
    // Note: group style resolution needs the group in graph too — use defaults here
    // by registering it in graph.groups with empty children.
    let mut graph = graph;
    graph.groups.push(plotgram_model::graph::Group {
        id: "backend".to_string(),
        label: Some("后端服务".to_string()),
        attrs: AttrMap::new(),
        nodes: vec![],
        edges: vec![],
        groups: vec![],
    });

    // Labels: node labels centered on frames, edge labels, group header
    let mut labels: Vec<LabelSlot> = nodes
        .iter()
        .map(|np| {
            let text = graph
                .nodes
                .iter()
                .find(|n| n.id == np.id)
                .and_then(|n| n.label.clone())
                .unwrap_or_default();
            // person: label below the figure
            let frame = if np.id == "buyer" {
                Rect::new(np.frame.x - 20.0, np.frame.bottom() + 4.0, np.frame.width + 40.0, 18.0)
            } else {
                np.frame
            };
            node_label(&np.id, &text, frame)
        })
        .collect();
    labels.push(LabelSlot {
        owner: LabelOwner::Edge("e2".to_string()),
        role: Some("mid".to_string()),
        text: "是".to_string(),
        frame: Rect::new(360.0, 52.0, 30.0, 16.0),
    });
    labels.push(LabelSlot {
        owner: LabelOwner::Edge("e3".to_string()),
        role: Some("mid".to_string()),
        text: "否".to_string(),
        frame: Rect::new(282.0, 128.0, 30.0, 16.0),
    });
    labels.push(LabelSlot {
        owner: LabelOwner::Group("backend".to_string()),
        role: None,
        text: "后端服务".to_string(),
        frame: Rect::new(405.0, 22.0, 70.0, 18.0),
    });

    let layout = LayoutResult {
        nodes,
        edges,
        groups,
        labels,
        canvas_width: 740.0,
        canvas_height: 420.0,
    };

    RenderInput {
        graph,
        layout,
        meta: RenderMeta {
            title: None,
            theme: theme.map(|s| s.to_string()),
            render_style: render_style.map(|s| s.to_string()),
        },
    }
}

fn main() {
    let out_dir = std::path::Path::new("target/render-preview");
    std::fs::create_dir_all(out_dir).expect("create output dir");

    let variants = [
        ("preview.clean-light.svg", None, None),
        ("preview.clean-dark.svg", Some("common.clean-dark"), None),
        ("preview.blueprint.svg", Some("common.blueprint"), None),
        ("preview.sketch.svg", None, Some("sketch")),
    ];

    for (file, theme, style) in variants {
        let input = build_input(theme, style);
        let svg = render_svg(&input);
        let path = out_dir.join(file);
        std::fs::write(&path, &svg).expect("write svg");
        println!("wrote {} ({} bytes)", path.display(), svg.len());
    }
}
