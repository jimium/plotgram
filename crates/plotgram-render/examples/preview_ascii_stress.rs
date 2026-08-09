//! ASCII backend stress test: wider graph with multiple branches, shared
//! routing corridors, long CJK labels, and a diamond decision pattern.
//!
//! Run: `cargo run -p plotgram-render --example preview_ascii_stress`

use plotgram_model::attr::AttrMap;
use plotgram_model::geometry::{Point, Rect};
use plotgram_model::graph::{Arrow, Edge, Graph, Node};
use plotgram_model::render::{RenderInput, RenderMeta};
use plotgram_model::result::{
    EdgePath, EdgePlacement, LabelOwner, LabelSlot, LayoutResult, NodePlacement,
};
use plotgram_render::render_ascii;

fn node(id: &str, label: &str) -> Node {
    Node {
        id: id.to_string(),
        label: Some(label.to_string()),
        shape: None,
        role: Default::default(),
        host_group: None,
        anchor: None,
        partition_cell: None,
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
        from_port: None,
        to_port: None,
        weight: None,
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
        path: EdgePath::polyline(pts.iter().map(|&(x, y)| Point { x, y }).collect()),
        from_port: None,
        to_port: None,
    }
}

fn node_label(id: &str, text: &str, frame: Rect) -> LabelSlot {
    LabelSlot { owner: LabelOwner::Node(id.to_string()), role: None, text: text.to_string(), frame }
}

fn edge_label(id: &str, text: &str, frame: Rect) -> LabelSlot {
    LabelSlot { owner: LabelOwner::Edge(id.to_string()), role: None, text: text.to_string(), frame }
}

/// A deployment pipeline: 代码提交 → 构建 → 单元测试 → 集成测试 → 部署预发 → 部署生产
/// With branches: 单元测试失败 → 通知开发者; 集成测试失败 → 回滚
fn build_input() -> RenderInput {
    let graph = Graph {
        nodes: vec![
            node("commit", "代码提交"),
            node("build", "构建编译"),
            node("unit", "单元测试"),
            node("integ", "集成测试"),
            node("staging", "部署预发环境"),
            node("prod", "部署生产环境"),
            node("notify", "通知开发者修复"),
            node("rollback", "回滚版本"),
        ],
        edges: vec![
            edge("e1", "commit", "build", Arrow::Forward),
            edge("e2", "build", "unit", Arrow::Forward),
            edge("e3", "unit", "integ", Arrow::Forward),
            edge("e4", "integ", "staging", Arrow::Forward),
            edge("e5", "staging", "prod", Arrow::Forward),
            edge("e6", "unit", "notify", Arrow::Forward),
            edge("e7", "integ", "rollback", Arrow::Response),
            edge("e8", "notify", "commit", Arrow::Response),
        ],
        groups: vec![],
            partition: None,
        };

    // Layout: main pipeline vertical at x=120, branches to the right at x=360
    let n_commit  = Rect::new(72.0, 12.0, 96.0, 36.0);
    let n_build   = Rect::new(72.0, 84.0, 96.0, 36.0);
    let n_unit    = Rect::new(72.0, 156.0, 96.0, 36.0);
    let n_integ   = Rect::new(72.0, 228.0, 96.0, 36.0);
    let n_staging = Rect::new(60.0, 300.0, 120.0, 36.0);
    let n_prod    = Rect::new(60.0, 372.0, 120.0, 36.0);
    let n_notify  = Rect::new(312.0, 156.0, 132.0, 36.0);
    let n_rollback = Rect::new(312.0, 228.0, 108.0, 36.0);

    let layout = LayoutResult {
        nodes: vec![
            place("commit", n_commit.x, n_commit.y, n_commit.width, n_commit.height),
            place("build", n_build.x, n_build.y, n_build.width, n_build.height),
            place("unit", n_unit.x, n_unit.y, n_unit.width, n_unit.height),
            place("integ", n_integ.x, n_integ.y, n_integ.width, n_integ.height),
            place("staging", n_staging.x, n_staging.y, n_staging.width, n_staging.height),
            place("prod", n_prod.x, n_prod.y, n_prod.width, n_prod.height),
            place("notify", n_notify.x, n_notify.y, n_notify.width, n_notify.height),
            place("rollback", n_rollback.x, n_rollback.y, n_rollback.width, n_rollback.height),
        ],
        edges: vec![
            route("e1", "commit", "build", &[(120.0, 48.0), (120.0, 84.0)]),
            route("e2", "build", "unit", &[(120.0, 120.0), (120.0, 156.0)]),
            route("e3", "unit", "integ", &[(120.0, 192.0), (120.0, 228.0)]),
            route("e4", "integ", "staging", &[(120.0, 264.0), (120.0, 300.0)]),
            route("e5", "staging", "prod", &[(120.0, 336.0), (120.0, 372.0)]),
            // unit → notify (horizontal right)
            route("e6", "unit", "notify", &[(168.0, 174.0), (312.0, 174.0)]),
            // integ → rollback (horizontal right, dashed)
            route("e7", "integ", "rollback", &[(168.0, 246.0), (312.0, 246.0)]),
            // notify --> commit (elbow: up then left then down, dashed)
            route("e8", "notify", "commit", &[
                (378.0, 156.0), (378.0, 0.0), (120.0, 0.0), (120.0, 12.0),
            ]),
        ],
        groups: vec![],
        labels: vec![
            node_label("commit", "代码提交", n_commit),
            node_label("build", "构建编译", n_build),
            node_label("unit", "单元测试", n_unit),
            node_label("integ", "集成测试", n_integ),
            node_label("staging", "部署预发环境", n_staging),
            node_label("prod", "部署生产环境", n_prod),
            node_label("notify", "通知开发者修复", n_notify),
            node_label("rollback", "回滚版本", n_rollback),
            edge_label("e6", "失败", Rect::new(222.0, 162.0, 36.0, 12.0)),
            edge_label("e7", "失败", Rect::new(222.0, 234.0, 36.0, 12.0)),
        ],
        canvas_width: 480.0,
        canvas_height: 420.0,
        diagnostics: Default::default(),
    };

    RenderInput {
        graph,
        layout,
        meta: RenderMeta {
            title: Some("CI/CD 部署流水线".to_string()),
            theme: None,
            render_style: None,
            extra: Default::default(),
        },
    }
}

fn main() {
    let text = render_ascii(&build_input());
    println!("{text}");

    let out_dir = std::path::Path::new("target/render-preview");
    std::fs::create_dir_all(out_dir).expect("create output dir");
    let path = out_dir.join("stress.ascii.txt");
    std::fs::write(&path, format!("{text}\n")).expect("write stress preview");
    eprintln!("\nwrote {}", path.display());
}
