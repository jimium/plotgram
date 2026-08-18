//! draw.io XML 编码器（图层顺序写入 mxGraphModel）。
//!
//! 输入为 `RenderInput`（graph + layout + meta）；颜色先经 theme/resolve
//! 物化，再映射为 mxCell vertex / edge（非扁平 SVG）。

use std::collections::BTreeMap;
use std::fmt::Write as FmtWrite;

use plotgram_model::geometry::Rect;
use plotgram_model::graph::{Group, NodeRole};
use plotgram_model::render::RenderInput;
use plotgram_model::result::{EdgePlacement, LabelOwner, LabelSlot, NodePlacement};

use crate::resolve::{group_depth_map, ResolvedGraph};
use crate::theme::CompiledTheme;
use crate::util::escape_xml;

use super::routing::{compute_label_rel_pos, fmt_port_coord, plan_edge_routing, port_coords};
use super::style::{
    arrow_style_parts, fmt_opacity, fmt_stroke_width, is_bold, sanitize_id,
    shape_to_drawio_style, to_drawio_color,
};

/// 画布外的页面留白（draw.io 页坐标 = 画布坐标 + padding）。
const PAGE_PADDING: f64 = 20.0;

/// 分组 cell 信息（供子节点 / 嵌套 group 解析 parent 与相对坐标）。
struct GroupCellInfo {
    cell_id: String,
    abs_x: f64,
    abs_y: f64,
}

pub(crate) struct DrawioEncoder<'a> {
    input: &'a RenderInput,
    theme: &'a CompiledTheme,
    resolved: &'a ResolvedGraph,
    sketch: bool,
    xml: String,
    /// Entity 节点 id → frame（GroupAnchor 不可见、不可绑定）。
    node_frame: BTreeMap<String, Rect>,
    /// 节点 id → 所属 group id。
    node_group: BTreeMap<String, String>,
    /// group id → 父 group id（None = 顶层）。
    group_parent: BTreeMap<String, Option<String>>,
    /// group id → label 文本。
    group_label: BTreeMap<String, Option<String>>,
    group_cell: BTreeMap<String, GroupCellInfo>,
    /// edge id → 标签（首个 mid/无角色标签优先）。
    edge_label: BTreeMap<String, LabelSlot>,
}

impl<'a> DrawioEncoder<'a> {
    pub(crate) fn new(
        input: &'a RenderInput,
        theme: &'a CompiledTheme,
        resolved: &'a ResolvedGraph,
    ) -> Self {
        Self {
            input,
            theme,
            resolved,
            sketch: input.meta.render_style.as_deref() == Some("sketch"),
            xml: String::with_capacity(8192),
            node_frame: BTreeMap::new(),
            node_group: BTreeMap::new(),
            group_parent: BTreeMap::new(),
            group_label: BTreeMap::new(),
            group_cell: BTreeMap::new(),
            edge_label: BTreeMap::new(),
        }
    }

    /// 主编码流程：背景 → groups → edges → nodes（与 SVG 图层顺序一致）。
    pub(crate) fn encode(mut self) -> String {
        self.build_indexes();
        self.write_header();
        self.write_background();
        self.write_groups();
        self.write_edges();
        self.write_nodes();
        self.write_footer();
        self.xml
    }

    // ─── 索引 ────────────────────────────────────────────────

    fn build_indexes(&mut self) {
        walk_groups(&self.input.graph.groups, None, &mut self.group_parent, &mut self.group_label, &mut self.node_group);

        for np in &self.input.layout.nodes {
            let bindable = self
                .input
                .graph
                .find_node(&np.id)
                .map(|n| n.role == NodeRole::Entity)
                .unwrap_or(true);
            if bindable {
                self.node_frame.insert(np.id.clone(), np.frame);
            }
        }

        for ls in &self.input.layout.labels {
            if let LabelOwner::Edge(edge_id) = &ls.owner {
                // mid / 无角色标签优先；其余（head/tail）忽略
                let prefer = ls.role.is_none() || ls.role.as_deref() == Some("mid");
                let entry = self.edge_label.get(edge_id);
                let take = match entry {
                    None => true,
                    Some(existing) => {
                        prefer
                            && !(existing.role.is_none() || existing.role.as_deref() == Some("mid"))
                    }
                };
                if take {
                    self.edge_label.insert(edge_id.clone(), ls.clone());
                }
            }
        }
    }

    // ─── 文档骨架 ────────────────────────────────────────────

    fn write_header(&mut self) {
        let version = env!("CARGO_PKG_VERSION");
        let page_w = self.input.layout.canvas_width + PAGE_PADDING * 2.0;
        let page_h = self.input.layout.canvas_height + PAGE_PADDING * 2.0;
        let name = escape_xml(self.input.meta.title.as_deref().unwrap_or("Diagram"));
        write!(
            self.xml,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<mxfile host="Plotgram/{version}" agent="Plotgram/{version}" version="1.0">
  <diagram id="plotgram-diagram" name="{name}">
    <mxGraphModel dx="0" dy="0" grid="1" gridSize="10" guides="1" tooltips="1" connect="1" arrows="1" fold="1" page="1" pageScale="1" pageWidth="{page_w}" pageHeight="{page_h}" math="0" shadow="0">
      <root>
        <mxCell id="0" />
        <mxCell id="1" parent="0" />
"#,
        )
        .unwrap();
    }

    fn write_background(&mut self) {
        let bg = to_drawio_color(&self.theme.defaults.canvas_background);
        if bg == "none" || bg == "transparent" {
            return;
        }
        let w = self.input.layout.canvas_width + PAGE_PADDING * 2.0;
        let h = self.input.layout.canvas_height + PAGE_PADDING * 2.0;
        write!(
            self.xml,
            r#"        <mxCell id="drawio-bg" value="" style="rounded=0;whiteSpace=wrap;html=1;fillColor={bg};strokeColor=none;" vertex="1" parent="1">
          <mxGeometry x="0" y="0" width="{w}" height="{h}" as="geometry" />
        </mxCell>
"#,
        )
        .unwrap();
    }

    fn write_footer(&mut self) {
        self.xml.push_str(
            r#"      </root>
    </mxGraphModel>
  </diagram>
</mxfile>
"#,
        );
    }

    // ─── 分组 ────────────────────────────────────────────────

    /// 外层 group 先写（depth 升序），嵌套坐标相对父容器。
    fn write_groups(&mut self) {
        let depths = group_depth_map(&self.input.graph);
        let placements: Vec<(String, Rect, usize)> = self
            .input
            .layout
            .groups
            .iter()
            .map(|g| {
                (
                    g.id.clone(),
                    g.frame,
                    depths.get(&g.id).copied().unwrap_or(0),
                )
            })
            .collect();
        let mut sorted = placements;
        sorted.sort_by_key(|(_, _, d)| *d);

        for (gid, frame, _) in sorted {
            let cell_id = format!("drawio-group-{}", sanitize_id(&gid));
            let abs_x = frame.x + PAGE_PADDING;
            let abs_y = frame.y + PAGE_PADDING;

            let (parent, rel_x, rel_y) = match self.group_parent.get(&gid).and_then(|p| p.as_ref()) {
                Some(pid) => match self.group_cell.get(pid.as_str()) {
                    Some(info) => (
                        info.cell_id.clone(),
                        abs_x - info.abs_x,
                        abs_y - info.abs_y,
                    ),
                    None => ("1".to_string(), abs_x, abs_y),
                },
                None => ("1".to_string(), abs_x, abs_y),
            };

            let label = self.group_label.get(&gid).cloned().flatten();
            let has_label = label.is_some();
            let value = escape_xml(label.as_deref().unwrap_or(""));

            let gs = self.resolved.groups.get(&gid);
            let mut style_parts: Vec<String> = if has_label {
                vec![
                    "swimlane".to_string(),
                    "startSize=26".to_string(),
                    "html=1".to_string(),
                    "horizontal=1".to_string(),
                ]
            } else {
                vec![
                    "rounded=1".to_string(),
                    "whiteSpace=wrap".to_string(),
                    "html=1".to_string(),
                    "dashed=1".to_string(),
                    "container=1".to_string(),
                ]
            };
            if let Some(gs) = gs {
                style_parts.push(format!("fillColor={}", to_drawio_color(&gs.fill)));
                style_parts.push(format!("strokeColor={}", to_drawio_color(&gs.stroke)));
                if let Some(sw) = fmt_stroke_width(gs.stroke_width) {
                    style_parts.push(format!("strokeWidth={sw}"));
                }
                if let Some(op) = gs.fill_opacity {
                    style_parts.push(format!("fillOpacity={}", fmt_opacity(op)));
                }
                if gs.stroke_dasharray.is_some()
                    && !style_parts.contains(&"dashed=1".to_string())
                {
                    style_parts.push("dashed=1".to_string());
                }
                if has_label {
                    style_parts.push(format!("fontColor={}", to_drawio_color(&gs.text_fill)));
                }
            }
            if self.sketch {
                style_parts.push("sketch=1".to_string());
            }
            let style = style_parts.join(";");

            write!(
                self.xml,
                r#"        <mxCell id="{id}" value="{value}" style="{style}" vertex="1" parent="{parent}">
          <mxGeometry x="{x}" y="{y}" width="{w}" height="{h}" as="geometry" />
        </mxCell>
"#,
                id = cell_id,
                value = value,
                style = style,
                parent = parent,
                x = rel_x,
                y = rel_y,
                w = frame.width,
                h = frame.height,
            )
            .unwrap();

            self.group_cell.insert(
                gid,
                GroupCellInfo {
                    cell_id,
                    abs_x,
                    abs_y,
                },
            );
        }
    }

    // ─── 边 ──────────────────────────────────────────────────

    fn write_edges(&mut self) {
        for ep in &self.input.layout.edges {
            self.write_edge(ep);
        }
    }

    fn write_edge(&mut self, ep: &EdgePlacement) {
        let edge_id = format!("drawio-edge-{}", sanitize_id(&ep.id));
        let graph_edge = self.input.graph.find_edge(&ep.id);
        let arrow = graph_edge.map(|e| e.arrow).unwrap_or(plotgram_model::graph::Arrow::Forward);
        let es = self.resolved.edges.get(&ep.id);

        let routing = plan_edge_routing(&ep.path);
        let endpoints = ep.path.start_end();

        // source/target 始终优先绑定可见节点；不可绑定端（GroupAnchor /
        // 缺失）退化为浮动端点（sourcePoint / targetPoint）
        let source_cell = self.bindable_cell(&ep.source);
        let target_cell = self.bindable_cell(&ep.target);

        let mut style_parts: Vec<String> = vec!["html=1".to_string()];
        style_parts.extend(routing.style_parts.iter().cloned());
        style_parts.extend(arrow_style_parts(arrow).iter().map(|s| s.to_string()));

        // 端口连接点（绑定端）：沿边偏移，非固定 0.5
        if let (Some((start, _)), Some(frame)) = (
            endpoints,
            self.node_frame.get(&ep.source),
        ) {
            if source_cell.is_some() {
                let (x, y) = port_coords(start, *frame);
                style_parts.push(format!("exitX={}", fmt_port_coord(x)));
                style_parts.push(format!("exitY={}", fmt_port_coord(y)));
                style_parts.push("exitDx=0".to_string());
                style_parts.push("exitDy=0".to_string());
            }
        }
        if let (Some((_, end)), Some(frame)) = (endpoints, self.node_frame.get(&ep.target)) {
            if target_cell.is_some() {
                let (x, y) = port_coords(end, *frame);
                style_parts.push(format!("entryX={}", fmt_port_coord(x)));
                style_parts.push(format!("entryY={}", fmt_port_coord(y)));
                style_parts.push("entryDx=0".to_string());
                style_parts.push("entryDy=0".to_string());
            }
        }

        if let Some(es) = es {
            style_parts.push(format!("strokeColor={}", to_drawio_color(&es.stroke)));
            if let Some(sw) = fmt_stroke_width(es.stroke_width) {
                style_parts.push(format!("strokeWidth={sw}"));
            }
            if es.stroke_dasharray.is_some() {
                style_parts.push("dashed=1".to_string());
            }
            if let Some(op) = es.stroke_opacity {
                style_parts.push(format!("strokeOpacity={}", fmt_opacity(op)));
            }
        }
        if self.sketch {
            style_parts.push("sketch=1".to_string());
        }

        // 边标签 → edge cell 的 value + geometry 相对位置
        let label_slot = self.edge_label.get(&ep.id).cloned();
        let mut value_attr = String::new();
        if let Some(ls) = &label_slot {
            value_attr = format!(r#" value="{}""#, escape_xml(&ls.text).replace('\n', "&#xa;"));
            if let Some(es) = es {
                style_parts.push(format!("fontColor={}", to_drawio_color(&es.text_fill)));
            }
        }
        let style = style_parts.join(";");

        let geometry = self.format_edge_geometry(ep, &routing.waypoints, &label_slot, endpoints);

        let src_attr = source_cell
            .as_deref()
            .map(|c| format!(r#" source="{c}""#))
            .unwrap_or_default();
        let tgt_attr = target_cell
            .as_deref()
            .map(|c| format!(r#" target="{c}""#))
            .unwrap_or_default();

        write!(
            self.xml,
            r#"        <mxCell id="{id}"{value} style="{style}" edge="1" parent="1"{src}{tgt}>
          {geometry}
        </mxCell>
"#,
            id = edge_id,
            value = value_attr,
            style = style,
            src = src_attr,
            tgt = tgt_attr,
            geometry = geometry,
        )
        .unwrap();
    }

    /// 生成边的 mxGeometry：浮动端点 + waypoints + 标签相对位置。
    fn format_edge_geometry(
        &self,
        ep: &EdgePlacement,
        waypoints: &[plotgram_model::geometry::Point],
        label_slot: &Option<LabelSlot>,
        endpoints: Option<(plotgram_model::geometry::Point, plotgram_model::geometry::Point)>,
    ) -> String {
        let pad = PAGE_PADDING;
        let mut inner = String::new();

        // 浮动端（未绑定 source/target 时以绝对坐标锚定）
        if self.bindable_cell(&ep.source).is_none() {
            if let Some((start, _)) = endpoints {
                write!(inner, r#"<mxPoint x="{}" y="{}" as="sourcePoint" />"#, start.x + pad, start.y + pad)
                    .unwrap();
            }
        }
        if self.bindable_cell(&ep.target).is_none() {
            if let Some((_, end)) = endpoints {
                write!(inner, r#"<mxPoint x="{}" y="{}" as="targetPoint" />"#, end.x + pad, end.y + pad)
                    .unwrap();
            }
        }

        if !waypoints.is_empty() {
            let pts: Vec<String> = waypoints
                .iter()
                .map(|p| format!(r#"<mxPoint x="{}" y="{}" />"#, p.x + pad, p.y + pad))
                .collect();
            write!(inner, r#"<Array as="points">{}</Array>"#, pts.join(""))
                .unwrap();
        }

        match label_slot {
            None => {
                if inner.is_empty() {
                    r#"<mxGeometry relative="1" as="geometry" />"#.to_string()
                } else {
                    format!(
                        r#"<mxGeometry relative="1" as="geometry">
            {inner}
          </mxGeometry>"#
                    )
                }
            }
            Some(ls) => {
                let (x, mut y) = compute_label_rel_pos(ls.frame.center(), &ep.path);
                if y.abs() < 1e-9 {
                    y = 0.0; // 避免 "-0.00"
                }
                if inner.is_empty() {
                    inner = r#"<mxPoint as="offset" />"#.to_string();
                } else {
                    inner.push_str("\n            ");
                    inner.push_str(r#"<mxPoint as="offset" />"#);
                }
                format!(
                    r#"<mxGeometry x="{x:.4}" y="{y:.2}" relative="1" as="geometry">
            {inner}
          </mxGeometry>"#
                )
            }
        }
    }

    /// 端点节点可绑定（存在且为 Entity）时返回其 cell id。
    fn bindable_cell(&self, node_id: &str) -> Option<String> {
        self.node_frame
            .contains_key(node_id)
            .then(|| format!("drawio-node-{}", sanitize_id(node_id)))
    }

    // ─── 节点 ────────────────────────────────────────────────

    fn write_nodes(&mut self) {
        let placements: Vec<NodePlacement> = self.input.layout.nodes.clone();
        for np in &placements {
            // GroupAnchor 是不可见的框挂点，不导出 vertex
            let is_anchor = self
                .input
                .graph
                .find_node(&np.id)
                .map(|n| n.role == NodeRole::GroupAnchor)
                .unwrap_or(false);
            if is_anchor {
                continue;
            }
            self.write_node(np);
        }
    }

    fn write_node(&mut self, np: &NodePlacement) {
        let cell_id = format!("drawio-node-{}", sanitize_id(&np.id));
        let abs_x = np.frame.x + PAGE_PADDING;
        let abs_y = np.frame.y + PAGE_PADDING;

        let (parent, x, y) = match self.node_group.get(&np.id) {
            Some(gid) => match self.group_cell.get(gid) {
                Some(info) => (info.cell_id.clone(), abs_x - info.abs_x, abs_y - info.abs_y),
                None => ("1".to_string(), abs_x, abs_y),
            },
            None => ("1".to_string(), abs_x, abs_y),
        };

        let ns = self.resolved.nodes.get(&np.id);
        let label = self
            .input
            .graph
            .find_node(&np.id)
            .and_then(|n| n.label.clone())
            .unwrap_or_default();

        let mut style_parts: Vec<String> = match ns {
            Some(ns) => {
                let mut parts = vec![
                    shape_to_drawio_style(ns.shape),
                    "whiteSpace=wrap".to_string(),
                    "html=1".to_string(),
                ];
                parts.push(format!("fillColor={}", to_drawio_color(&ns.fill)));
                parts.push(format!("strokeColor={}", to_drawio_color(&ns.stroke)));
                parts.push(format!("fontColor={}", to_drawio_color(&ns.text_fill)));
                parts.push(format!("fontSize={}", ns.font_size.round() as i32));
                if let Some(sw) = fmt_stroke_width(ns.stroke_width) {
                    parts.push(format!("strokeWidth={sw}"));
                }
                if ns.stroke_dasharray.is_some() {
                    parts.push("dashed=1".to_string());
                }
                if let Some(op) = ns.fill_opacity {
                    parts.push(format!("fillOpacity={}", fmt_opacity(op)));
                }
                if let Some(op) = ns.stroke_opacity {
                    parts.push(format!("strokeOpacity={}", fmt_opacity(op)));
                }
                if is_bold(ns.font_weight.as_ref()) {
                    parts.push("fontStyle=1".to_string());
                }
                parts
            }
            None => vec![
                "rounded=1".to_string(),
                "whiteSpace=wrap".to_string(),
                "html=1".to_string(),
            ],
        };
        if self.sketch {
            style_parts.push("sketch=1".to_string());
        }
        let style = style_parts.join(";");

        write!(
            self.xml,
            r#"        <mxCell id="{id}" value="{value}" style="{style}" vertex="1" parent="{parent}">
          <mxGeometry x="{x}" y="{y}" width="{w}" height="{h}" as="geometry" />
        </mxCell>
"#,
            id = cell_id,
            value = escape_xml(&label).replace('\n', "&#xa;"),
            style = style,
            parent = parent,
            x = x,
            y = y,
            w = np.frame.width,
            h = np.frame.height,
        )
        .unwrap();
    }
}

/// 递归收集 group 父子 / 标签 / 节点归属。
fn walk_groups(
    groups: &[Group],
    parent: Option<&str>,
    group_parent: &mut BTreeMap<String, Option<String>>,
    group_label: &mut BTreeMap<String, Option<String>>,
    node_group: &mut BTreeMap<String, String>,
) {
    for g in groups {
        group_parent.insert(g.id.clone(), parent.map(str::to_string));
        group_label.insert(g.id.clone(), g.label.clone());
        for n in &g.nodes {
            node_group.insert(n.id.clone(), g.id.clone());
        }
        walk_groups(&g.groups, Some(&g.id), group_parent, group_label, node_group);
    }
}
