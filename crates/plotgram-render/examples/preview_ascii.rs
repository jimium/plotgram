//! ASCII backend preview: a small hand-laid flowchart exercising boxes,
//! branches, edge labels, dashed (Response) edges, bidirectional edges and
//! CJK labels.
//!
//! Run: `cargo run -p plotgram-render --example preview_ascii`
//! Output: stdout + `target/render-preview/preview.ascii.txt`

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
    LabelSlot {
        owner: LabelOwner::Node(id.to_string()),
        role: None,
        text: text.to_string(),
        frame,
    }
}

fn edge_label(id: &str, text: &str, frame: Rect) -> LabelSlot {
    LabelSlot {
        owner: LabelOwner::Edge(id.to_string()),
        role: None,
        text: text.to_string(),
        frame,
    }
}

fn build_input() -> RenderInput {
    // 开始 → 用户登录 → 校验通过? ─是→ 主页 / ─否→ 提示错误 (--> 回到登录)
    // 主页 <-> 会话服务（双向）
    let graph = Graph {
        nodes: vec![
            node("start", "开始"),
            node("login", "用户登录"),
            node("check", "校验通过?"),
            node("home", "主页"),
            node("err", "提示错误"),
            node("session", "会话服务"),
        ],
        edges: vec![
            edge("e1", "start", "login", Arrow::Forward),
            edge("e2", "login", "check", Arrow::Forward),
            edge("e3", "check", "home", Arrow::Forward),
            edge("e4", "check", "err", Arrow::Forward),
            edge("e5", "err", "login", Arrow::Response),
            edge("e6", "home", "session", Arrow::Bidirectional),
        ],
        groups: vec![],
            partition: None,
        };

    // ── Hand-laid geometry (px, 1 char ≈ 6×12) ──
    // Column centers: left branch x=90, right branch x=330.
    let n_start = Rect::new(42.0, 12.0, 96.0, 36.0);
    let n_login = Rect::new(42.0, 84.0, 96.0, 36.0);
    let n_check = Rect::new(36.0, 156.0, 108.0, 36.0);
    let n_home = Rect::new(48.0, 240.0, 84.0, 36.0);
    let n_err = Rect::new(282.0, 156.0, 96.0, 36.0);
    let n_session = Rect::new(282.0, 240.0, 96.0, 36.0);

    let layout = LayoutResult {
        nodes: vec![
            place("start", n_start.x, n_start.y, n_start.width, n_start.height),
            place("login", n_login.x, n_login.y, n_login.width, n_login.height),
            place("check", n_check.x, n_check.y, n_check.width, n_check.height),
            place("home", n_home.x, n_home.y, n_home.width, n_home.height),
            place("err", n_err.x, n_err.y, n_err.width, n_err.height),
            place("session", n_session.x, n_session.y, n_session.width, n_session.height),
        ],
        edges: vec![
            // start ↓ login
            route("e1", "start", "login", &[(90.0, 48.0), (90.0, 84.0)]),
            // login ↓ check
            route("e2", "login", "check", &[(90.0, 120.0), (90.0, 156.0)]),
            // check ↓ home (是)
            route("e3", "check", "home", &[(90.0, 192.0), (90.0, 240.0)]),
            // check → err (否)
            route("e4", "check", "err", &[(138.0, 168.0), (282.0, 168.0)]),
            // err --> login (dashed response, elbow above)
            route(
                "e5",
                "err",
                "login",
                &[(330.0, 156.0), (330.0, 96.0), (132.0, 96.0)],
            ),
            // home <-> session (bidirectional)
            route("e6", "home", "session", &[(126.0, 252.0), (282.0, 252.0)]),
        ],
        groups: vec![],
        labels: vec![
            node_label("start", "开始", n_start),
            node_label("login", "用户登录", n_login),
            node_label("check", "校验通过?", n_check),
            node_label("home", "主页", n_home),
            node_label("err", "提示错误", n_err),
            node_label("session", "会话服务", n_session),
            edge_label("e3", "是", Rect::new(96.0, 210.0, 24.0, 12.0)),
            edge_label("e4", "否", Rect::new(198.0, 162.0, 24.0, 12.0)),
        ],
        canvas_width: 420.0,
        canvas_height: 300.0,
        diagnostics: Default::default(),
    };

    RenderInput {
        graph,
        layout,
        meta: RenderMeta {
            title: Some("登录流程".to_string()),
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
    let path = out_dir.join("preview.ascii.txt");
    std::fs::write(&path, format!("{text}\n")).expect("write ascii preview");
    eprintln!("\nwrote {}", path.display());
}
