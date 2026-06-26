//! drawio 导出集成测试：全管线 parse → prepare → render(Drawio)。
//!
//! 覆盖 flowchart / state / 双向边等场景，验证导出结果符合 drawio 规范：
//! - XML 结构合法（mxfile / mxGraphModel / root）；
//! - 节点为可编辑 vertex，边为带 source/target 的 edge；
//! - 颜色保真导出（fillColor/strokeColor）；
//! - 箭头方向语义正确（Active/Passive/Bidirectional）；
//! - 端口连接点写入 style 字符串。

use drawify_core::ast::{
    ArrowType, AttributeMap, Diagram, DiagramAttribute, Entity, Identifier, PreparedDiagram,
    Relation, SourceInfo, Span, TextValue,
};
use drawify_core::pipeline::{parse, prepare, render_text};
use drawify_core::prepare::StyleRequest;
use drawify_core::render::encode::drawio::{
    encode_scene_inner, DrawioExportOptions, DrawioRenderer, ExportReport,
};
use drawify_core::render::encode::FormatEncoder;
use drawify_core::render::scene::export_scene;
use drawify_core::render::{RenderFormat, RenderRequest};
use drawify_core::types::DiagramType;

/// 运行全管线，返回 drawio XML。
fn render_drawio(source: &str) -> String {
    let raw = parse(source).expect("parse");
    let output = prepare(raw, &StyleRequest::default()).expect("prepare");
    let prepared = &output.diagram;
    let request = RenderRequest::new(prepared, RenderFormat::Drawio);
    render_text(&request).expect("render drawio")
}

fn render_drawio_with_report(source: &str) -> (String, ExportReport) {
    let raw = parse(source).expect("parse");
    let output = prepare(raw, &StyleRequest::default()).expect("prepare");
    let request = RenderRequest::new(&output.diagram, RenderFormat::Drawio);
    let scene = export_scene(&request).expect("export scene");
    encode_scene_inner(&scene, &DrawioExportOptions::default()).expect("encode")
}

fn create_flowchart_prepared() -> PreparedDiagram {
    let span = Span::dummy();
    let mut diagram = Diagram::new(
        DiagramType::Flowchart,
        SourceInfo {
            file: None,
            line_count: 1,
        },
    );
    diagram.attributes.push(DiagramAttribute {
        key: "title".to_string(),
        value: drawify_core::ast::AttributeValue::String(TextValue::quoted(
            "Test Diagram".to_string(),
        )),
        span,
    });
    diagram.entities = vec![
        Entity {
            id: Identifier::new_unchecked("a"),
            label: "Start".to_string(),
            attributes: AttributeMap::default(),
            group_id: None,
            span,
        },
        Entity {
            id: Identifier::new_unchecked("b"),
            label: "Process".to_string(),
            attributes: AttributeMap::default(),
            group_id: None,
            span,
        },
        Entity {
            id: Identifier::new_unchecked("c"),
            label: "End".to_string(),
            attributes: AttributeMap::default(),
            group_id: None,
            span,
        },
    ];
    diagram.relations = vec![
        Relation {
            from: Identifier::new_unchecked("a"),
            to: Identifier::new_unchecked("b"),
            arrow: ArrowType::Active,
            label: Some("next".to_string()),
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span,
        },
        Relation {
            from: Identifier::new_unchecked("b"),
            to: Identifier::new_unchecked("c"),
            arrow: ArrowType::Passive,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span,
        },
    ];
    PreparedDiagram::new(diagram)
}

fn encode_flowchart(prepared: &PreparedDiagram) -> (String, ExportReport) {
    let request = RenderRequest::new(prepared, RenderFormat::Drawio);
    let scene = export_scene(&request).expect("export scene");
    encode_scene_inner(&scene, &DrawioExportOptions::default()).expect("encode")
}

/// 校验 drawio XML 的公共结构断言。
fn assert_valid_drawio(xml: &str) {
    assert!(xml.starts_with("<?xml"), "应以 XML 声明开头: {xml}");
    assert!(xml.contains("<mxfile"), "缺 mxfile: {xml}");
    assert!(xml.contains("host=\"Drawify/"), "缺 host: {xml}");
    assert!(xml.contains("<diagram"), "缺 diagram: {xml}");
    assert!(xml.contains("<mxGraphModel"), "缺 mxGraphModel: {xml}");
    assert!(xml.contains("<root>"), "缺 root: {xml}");
    assert!(xml.contains("<mxCell id=\"0\""), "缺根 cell 0: {xml}");
    assert!(xml.contains("<mxCell id=\"1\""), "缺根 cell 1: {xml}");
    assert!(xml.contains("</root>"), "缺 </root>: {xml}");
    assert!(xml.contains("</mxfile>"), "缺 </mxfile>: {xml}");
}

#[test]
fn flowchart_active_passive_export() {
    let source = r#"
diagram flowchart {
    title: "线性流程"
    config { direction: left-to-right }

    entity start "开始" { type: start }
    entity process "处理" { type: process }
    entity end "结束" { type: end }

    start -> process
    process -> end
}
"#;
    let xml = render_drawio(source);
    assert_valid_drawio(&xml);

    // 节点
    assert!(xml.contains("drawio-node-start"), "缺节点 start: {xml}");
    assert!(xml.contains("drawio-node-process"), "缺节点 process: {xml}");
    assert!(xml.contains("drawio-node-end"), "缺节点 end: {xml}");

    // 边：source/target 始终绑定
    assert!(xml.contains(r#"source="drawio-node-start""#), "缺 source: {xml}");
    assert!(xml.contains(r#"target="drawio-node-process""#), "缺 target: {xml}");
    assert!(xml.contains(r#"source="drawio-node-process""#));
    assert!(xml.contains(r#"target="drawio-node-end""#));

    // Active 边：endArrow=block，无 startArrow
    assert!(xml.contains("endArrow=block"), "缺 endArrow=block: {xml}");
    assert!(
        !xml.contains("startArrow="),
        "Active 边不应有 startArrow: {xml}"
    );

    // 端口写入 style
    assert!(xml.contains("exitX=") && xml.contains("entryX="), "缺端口: {xml}");
    assert!(xml.contains("edgeStyle=none"), "缺 edgeStyle=none: {xml}");

    // 颜色保真导出
    assert!(xml.contains("fillColor="), "节点应导出 fillColor: {xml}");
    assert!(xml.contains("strokeColor="), "节点/边应导出 strokeColor: {xml}");
}

#[test]
fn state_diagram_export() {
    let source = r#"
diagram state {
    title: "开关状态"

    entity init "" { type: initial }
    entity off "关闭" { type: state }
    entity on "开启" { type: state }

    init -> off
    off -> on "按下开关"
    on -> off "再次按下"
}
"#;
    let xml = render_drawio(source);
    assert_valid_drawio(&xml);

    // 状态图节点
    assert!(xml.contains("drawio-node-init"), "缺节点 init: {xml}");
    assert!(xml.contains("drawio-node-off"), "缺节点 off: {xml}");
    assert!(xml.contains("drawio-node-on"), "缺节点 on: {xml}");

    // 边标签
    assert!(xml.contains("按下开关"), "缺边标签: {xml}");
    assert!(xml.contains("再次按下"), "缺边标签: {xml}");

    // 边数量：3 条
    let edge_count = xml.matches(r#"edge="1""#).count();
    assert_eq!(edge_count, 3, "应有 3 条边，实际 {edge_count}: {xml}");

    // 每条边都有 source + target
    let source_count = xml.matches("source=\"drawio-node-").count();
    let target_count = xml.matches("target=\"drawio-node-").count();
    assert_eq!(edge_count, source_count, "source 数量应等于边数: {xml}");
    assert_eq!(edge_count, target_count, "target 数量应等于边数: {xml}");
}

#[test]
fn bidirectional_edge_export() {
    let source = r#"
diagram flowchart {
    title: "双向同步"
    config { direction: left-to-right }

    entity x "左" { type: process }
    entity y "右" { type: process }

    x <-> y "sync"
}
"#;
    let xml = render_drawio(source);
    assert_valid_drawio(&xml);

    // 双向边：startArrow + endArrow 均为 block
    assert!(
        xml.contains("startArrow=block"),
        "双向边应导出 startArrow=block: {xml}"
    );
    assert!(
        xml.contains("endArrow=block"),
        "双向边应导出 endArrow=block: {xml}"
    );
    // source/target 绑定
    assert!(xml.contains(r#"source="drawio-node-x""#), "缺 source: {xml}");
    assert!(xml.contains(r#"target="drawio-node-y""#), "缺 target: {xml}");
    // 标签
    assert!(xml.contains("sync"), "缺边标签 sync: {xml}");
}

#[test]
fn passive_edge_is_dashed() {
    let source = r#"
diagram flowchart {
    title: "被动边"
    config { direction: left-to-right }

    entity a "A" { type: process }
    entity b "B" { type: process }

    a --> b "响应"
}
"#;
    let xml = render_drawio(source);
    assert_valid_drawio(&xml);
    // Passive 边应导出 dashed=1
    assert!(
        xml.contains("dashed=1"),
        "Passive 边应导出 dashed=1: {xml}"
    );
    assert!(xml.contains("endArrow=block"), "Passive 边仍应有 endArrow: {xml}");
}

#[test]
fn parallel_reverse_edges_use_distinct_ports() {
    let source = r#"
diagram flowchart {
    title: "反向平行边"
    config { direction: top-to-bottom }

    entity client "客户端" { type: client }
    entity gateway "网关" { type: gateway }

    client -> gateway "请求"
    gateway --> client "响应"
}
"#;
    let xml = render_drawio(source);

    let mut client_to_gateway_exit_x = None;
    let mut gateway_to_client_exit_x = None;

    for line in xml.lines() {
        if !line.contains(r#"edge="1""#) {
            continue;
        }
        if line.contains(r#"source="drawio-node-client""#)
            && line.contains(r#"target="drawio-node-gateway""#)
        {
            client_to_gateway_exit_x = line
                .split(';')
                .find_map(|part| part.strip_prefix("exitX="))
                .map(|v| v.parse::<f64>().unwrap());
        }
        if line.contains(r#"source="drawio-node-gateway""#)
            && line.contains(r#"target="drawio-node-client""#)
        {
            gateway_to_client_exit_x = line
                .split(';')
                .find_map(|part| part.strip_prefix("exitX="))
                .map(|v| v.parse::<f64>().unwrap());
        }
    }

    let fwd = client_to_gateway_exit_x.expect("client->gateway 边");
    let rev = gateway_to_client_exit_x.expect("gateway->client 边");
    assert!(
        (fwd - rev).abs() > 0.05,
        "反向平行边 exitX 应分离，实际 fwd={fwd} rev={rev}: {xml}"
    );
}

#[test]
fn color_and_structure_exported() {
    // 综合验证：颜色保真导出，结构性信息保留
    let source = r#"
diagram flowchart {
    title: "颜色保真"
    config { direction: top-to-bottom }

    entity a "A" { type: start }
    entity b "B" { type: process }
    a -> b
}
"#;
    let xml = render_drawio(source);
    assert_valid_drawio(&xml);

    // 颜色应导出到节点/边上
    let mut found_fill = false;
    let mut found_stroke = false;
    for line in xml.lines() {
        let is_node_or_edge =
            line.contains(r#"id="drawio-node-"#) || line.contains(r#"edge="1""#);
        if !is_node_or_edge {
            continue;
        }
        if line.contains("fillColor=") { found_fill = true; }
        if line.contains("strokeColor=") { found_stroke = true; }
    }
    assert!(found_fill, "节点应包含 fillColor: {xml}");
    assert!(found_stroke, "节点/边应包含 strokeColor: {xml}");

    // 结构性元数据保留
    assert!(xml.contains("drawifyEntityId="), "应保留 drawifyEntityId: {xml}");
    assert!(
        xml.contains("drawifyRelationIndex="),
        "应保留 drawifyRelationIndex: {xml}"
    );
    // 形状保留
    assert!(
        xml.contains("shape=rectangle") || xml.contains("rounded=1"),
        "应保留形状映射: {xml}"
    );
}

#[test]
fn title_rendered_on_top_layer() {
    let source = r#"
diagram flowchart {
    title: "顶层标题"
    config { direction: left-to-right }
    entity a "A" { type: start }
    entity b "B" { type: end }
    a -> b
}
"#;
    let xml = render_drawio(source);
    assert_valid_drawio(&xml);
    assert!(xml.contains("顶层标题"), "缺标题: {xml}");
    // title 应在最后一个节点之后（顶层）
    let title_pos = xml.find("drawio-title").expect("title cell");
    let last_node_pos = xml.rfind("drawio-node-b").expect("last node");
    assert!(
        title_pos > last_node_pos,
        "title 应在节点之上（顶层）: {xml}"
    );
}

#[test]
fn edge_label_is_edge_value() {
    // 边标签应作为 edge cell 的 value 文字，随边移动
    let source = r#"
diagram flowchart {
    title: "边标签归属"
    config { direction: left-to-right }

    entity a "A" { type: process }
    entity b "B" { type: process }

    a -> b "连接标签"
}
"#;
    let xml = render_drawio(source);
    assert_valid_drawio(&xml);

    assert!(xml.contains("连接标签"), "缺边标签文本: {xml}");
    assert!(
        !xml.contains("edgeLabel"),
        "不应再使用独立 edgeLabel cell: {xml}"
    );
    assert!(
        !xml.contains("-label"),
        "不应再有独立 label cell id: {xml}"
    );

    let edge_line = xml
        .lines()
        .find(|line| line.contains(r#"edge="1""#) && line.contains("连接标签"))
        .expect("带标签的边 cell");
    assert!(
        edge_line.contains(r#"value="连接标签""#),
        "标签应写在边 cell 的 value 属性: {edge_line}"
    );

    // 标签位置通过 geometry relative=1 + offset 锚定
    assert!(
        xml.contains(r#"relative="1""#),
        "边 geometry 应有 relative=\"1\": {xml}"
    );
    assert!(
        xml.contains(r#"<mxPoint as="offset""#),
        "边 geometry 应有 mxPoint offset: {xml}"
    );
}


// ── 编码器契约（原 mod.rs 单元测试迁移）──────────────────────

#[test]
fn drawio_renderer_metadata() {
    assert_eq!(DrawioRenderer.name(), "drawio");
    assert_eq!(DrawioRenderer.file_extension(), "drawio");
}

#[test]
fn drawio_encoder_produces_valid_mxfile() {
    let prepared = create_flowchart_prepared();
    let (xml, report) = encode_flowchart(&prepared);

    assert!(xml.starts_with("<?xml"));
    assert!(xml.contains("<mxfile"));
    assert!(xml.contains("host=\"Drawify/"));
    assert!(xml.contains("drawio-node-a"));
    assert!(xml.contains("next"));
    assert_eq!(report.stats.nodes, 3);
    assert_eq!(report.stats.edges, 2);
}

#[test]
fn drawio_encoder_via_trait() {
    let prepared = create_flowchart_prepared();
    let request = RenderRequest::new(&prepared, RenderFormat::Drawio);
    let scene = export_scene(&request).expect("export scene");
    let output = DrawioRenderer.encode_scene(&scene).expect("encode via trait");
    let xml = output.into_text().expect("text output");
    assert!(xml.contains("<mxfile"));
    assert!(xml.contains("drawio-node-a"));
}

#[test]
fn node_and_edge_colors_use_hash_prefix() {
    let (xml, _) = encode_flowchart(&create_flowchart_prepared());
    for line in xml.lines() {
        if line.contains("fillColor=") && !line.contains("fillColor=none") {
            assert!(line.contains("fillColor=#"), "fillColor 应带 # 前缀: {line}");
        }
        if line.contains("strokeColor=") && !line.contains("strokeColor=none") {
            assert!(line.contains("strokeColor=#"), "strokeColor 应带 # 前缀: {line}");
        }
    }
}

#[test]
fn edge_head_tail_label_warnings() {
    let span = Span::dummy();
    let mut diagram = Diagram::new(
        DiagramType::Flowchart,
        SourceInfo {
            file: None,
            line_count: 1,
        },
    );
    diagram.entities = vec![
        Entity {
            id: Identifier::new_unchecked("a"),
            label: "A".to_string(),
            attributes: AttributeMap::default(),
            group_id: None,
            span,
        },
        Entity {
            id: Identifier::new_unchecked("b"),
            label: "B".to_string(),
            attributes: AttributeMap::default(),
            group_id: None,
            span,
        },
    ];
    diagram.relations = vec![Relation {
        from: Identifier::new_unchecked("a"),
        to: Identifier::new_unchecked("b"),
        arrow: ArrowType::Active,
        label: Some("main".to_string()),
        head_label: Some("head".to_string()),
        tail_label: Some("tail".to_string()),
        attributes: AttributeMap::default(),
        span,
    }];
    let (_, report) = encode_flowchart(&PreparedDiagram::new(diagram));
    assert!(report.warnings.iter().any(|w| w.code == "EDGE_HEAD_LABEL_DROPPED"));
    assert!(report.warnings.iter().any(|w| w.code == "EDGE_TAIL_LABEL_DROPPED"));
}

#[test]
fn layer_order_edges_before_nodes() {
    let (xml, _) = encode_flowchart(&create_flowchart_prepared());
    let first_edge = xml.find(r#"edge="1""#).expect("edge cell");
    let first_node = xml.find("drawio-node-a").expect("node cell");
    assert!(first_edge < first_node, "边应在节点之下: {xml}");
}

#[test]
fn empty_diagram_warning() {
    let prepared = PreparedDiagram::new(Diagram::new(
        DiagramType::Flowchart,
        SourceInfo {
            file: None,
            line_count: 1,
        },
    ));
    let request = RenderRequest::new(&prepared, RenderFormat::Drawio);
    let scene = export_scene(&request).expect("export scene");
    let (xml, report) = encode_scene_inner(&scene, &DrawioExportOptions::default()).expect("encode");
    assert!(xml.contains("<mxfile"));
    assert_eq!(report.stats.nodes, 0);
    assert!(report.warnings.iter().any(|w| w.code == "EMPTY_DIAGRAM"));
}

#[test]
fn sequence_rejected_by_default() {
    let prepared = PreparedDiagram::new(Diagram::new(
        DiagramType::Sequence,
        SourceInfo {
            file: None,
            line_count: 1,
        },
    ));
    let request = RenderRequest::new(&prepared, RenderFormat::Drawio);
    let scene = export_scene(&request).expect("export scene");
    let result = encode_scene_inner(&scene, &DrawioExportOptions::default());
    assert!(result.is_err());
    assert!(format!("{:?}", result.unwrap_err()).contains("sequence"));
}

#[test]
fn architecture_icon_embedded_as_image() {
    let source = r#"
diagram architecture {
    title: "图标 L1"

    entity user "用户" {
        type: frontend
        semantic: user
    }
    entity db "数据库" {
        type: database
        semantic: postgres
    }

    user -> db
}
"#;
    let (xml, report) = render_drawio_with_report(source);
    assert_valid_drawio(&xml);

    let user_line = xml
        .lines()
        .find(|l| l.contains(r#"id="drawio-node-user""#) && l.contains(r#"vertex="1""#))
        .expect("user node cell");
    assert!(
        user_line.contains("shape=image") && user_line.contains("image=data:image/svg+xml,"),
        "架构图节点应嵌入 SVG image: {user_line}"
    );
    assert!(report.stats.l1 >= 1, "应有 L1 图标嵌入: {report:?}");
}

#[test]
fn architecture_icon_embed_disabled_falls_back_to_label() {
    let source = r#"
diagram architecture {
    entity svc "服务" { type: service semantic: api }
}
"#;
    let raw = parse(source).expect("parse");
    let output = prepare(raw, &StyleRequest::default()).expect("prepare");
    let request = RenderRequest::new(&output.diagram, RenderFormat::Drawio);
    let scene = export_scene(&request).expect("export scene");
    let mut options = DrawioExportOptions::default();
    options.embed_icons = false;
    let (xml, report) = encode_scene_inner(&scene, &options).expect("encode");

    assert!(!xml.contains("image=data:image/svg+xml,"));
    assert!(report.warnings.iter().any(|w| w.code == "ICON_EMBED_DISABLED"));
    assert!(xml.contains("服务"));
}
