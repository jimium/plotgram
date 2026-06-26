//! draw.io XML 编码器（图层顺序写入 mxGraphModel）。

use crate::ast::ArrowType;
use crate::error::Result;
use crate::layout::geometry::Point;
use crate::render::paint::color_queries;
use crate::render::scene::{ExportEdge, ExportNode, ExportScene};
use crate::types::DiagramType;
use std::collections::BTreeMap;
use std::fmt::Write as FmtWrite;

use super::compress::compress_drawio;
use super::icon::plan_node_icon;
use super::report::{
    DegradeTier, DrawioExportOptions, DrawioFallback, ExportReport, ExportWarning,
};
use super::routing::{
    fmt_port_coord, format_edge_geometry, geometry_midpoint, plan_drawio_edge_routing,
    resolve_edge_ports,
};
use super::style::{
    arrow_style_parts, contrast_font_color, fmt_stroke_width, is_sketch_graphic_style,
    sanitize_id, shape_to_drawio_style, to_drawio_color, xml_escape,
};

// ─── 编码器内部实现 ──────────────────────────────────────────
//
// 使用一个内部结构体持有所有可变输出状态（xml / report / id 生成器 / group id 映射），
// 把编码流程拆分为按图层顺序的若干方法，避免单一巨型函数。

/// 分组布局信息（用于子节点相对坐标计算）。
#[derive(Debug, Clone)]
struct GroupLayoutInfo {
    cell_id: String,
    abs_x: f64,
    abs_y: f64,
}

pub(crate) struct DrawioEncoder<'a, 's> {
    scene: &'s ExportScene<'a>,
    options: &'s DrawioExportOptions,
    xml: String,
    report: ExportReport,
    /// group id → (drawio mxCell id, 分组绝对坐标)，供节点/嵌套 group 解析 parent 与相对坐标。
    group_id_map: BTreeMap<String, GroupLayoutInfo>,
    /// 内部 cell id 自增计数器（用于 group / edge cell）。
    cell_seq: usize,
}

impl<'a, 's> DrawioEncoder<'a, 's> {
    pub(crate) fn new(scene: &'s ExportScene<'a>, options: &'s DrawioExportOptions) -> Self {
        Self {
            scene,
            options,
            xml: String::with_capacity(8192),
            report: ExportReport::new(&scene.diagram().diagram_type),
            group_id_map: BTreeMap::new(),
            cell_seq: 0,
        }
    }

    /// 主编码流程：按 spec §5.3 图层顺序输出。
    pub(crate) fn encode(mut self) -> Result<(String, ExportReport)> {
        self.check_unsupported()?;
        self.check_empty();
        self.write_header();
        self.write_background();
        self.write_groups();
        self.write_edges();
        self.write_nodes();
        self.write_title();
        self.write_footer();
        self.finalize_report();
        // 压缩后处理：将 <diagram> 内部 XML 替换为 deflate + base64
        if self.options.compressed {
            self.xml = compress_drawio(&self.xml);
        }
        Ok((self.xml, self.report))
    }

    /// 生成下一个内部 cell id（用于 group / edge）。
    fn next_cell_id(&mut self) -> String {
        self.cell_seq += 1;
        format!("drawio-cell-{}", self.cell_seq)
    }

    /// 拒绝/降级不支持的图表类型（sequence/er）。见 spec §10。
    fn check_unsupported(&mut self) -> Result<()> {
        let diagram = self.scene.diagram();
        let unsupported = matches!(
            diagram.diagram_type,
            DiagramType::Sequence | DiagramType::Er
        );
        if !unsupported {
            return Ok(());
        }
        let dt_str = diagram.diagram_type.style_key();
        if !self.options.allow_unsupported_diagram_types {
            return Err(crate::error::DrawifyError::render_internal_msg(format!(
                "export_unsupported: format='drawio', diagram_type='{}', hint='请使用 SVG 导出，或设置 allow_unsupported_diagram_types'",
                dt_str
            )));
        }
        match self.options.fallback {
            DrawioFallback::Error => Err(crate::error::DrawifyError::render_internal_msg(
                format!("export_unsupported: format='drawio', diagram_type='{}'", dt_str),
            )),
            DrawioFallback::EmbeddedSvg => {
                self.report.global_degrade = DegradeTier::F;
                // Tier-F: 嵌入 SVG image（Phase 1 简化：仅返回错误提示）
                Err(crate::error::DrawifyError::render_internal_msg(
                    "Tier-F EmbeddedSvg fallback not yet implemented for drawio export",
                ))
            }
        }
    }

    /// 空图检测：无节点且无边时记录提示。
    fn check_empty(&mut self) {
        if self.scene.nodes.is_empty() && self.scene.edges.is_empty() {
            self.report.warnings.push(ExportWarning {
                code: "EMPTY_DIAGRAM".to_string(),
                entity_id: None,
                edge_index: None,
                tier: DegradeTier::L0,
                message: "空图：无节点、无边，仅导出画布与分组".to_string(),
            });
        }
    }

    /// 写入 mxfile / diagram / mxGraphModel / root 头部及两个根 cell（id=0, id=1）。
    fn write_header(&mut self) {
        let crate_version = env!("CARGO_PKG_VERSION");
        let pad = self.options.page_padding;
        let page_w = self.scene.canvas.width + pad * 2.0;
        let page_h = self.scene.canvas.height + pad * 2.0;
        let title = xml_escape(self.scene.canvas.title.as_deref().unwrap_or("Diagram"));
        write!(
            self.xml,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<mxfile host="Drawify/{crate_version}" agent="Drawify/{crate_version}" version="1.0">
  <diagram id="drawify-diagram" name="{title}">
    <mxGraphModel dx="0" dy="0" grid="1" gridSize="10" guides="1" tooltips="1" connect="1" arrows="1" fold="1" page="1" pageScale="1" pageWidth="{page_w}" pageHeight="{page_h}" math="0" shadow="0">
      <root>
        <mxCell id="0" />
        <mxCell id="1" parent="0" />
"#,
            crate_version = crate_version,
            title = title,
            page_w = page_w,
            page_h = page_h,
        )
        .unwrap();
    }

    /// 背景矩形（仅当画布指定了非透明背景色时）。
    fn write_background(&mut self) {
        let bg = &self.scene.canvas.background;
        if bg.is_empty() || bg == "transparent" {
            return;
        }
        let pad = self.options.page_padding;
        let w = self.scene.canvas.width + pad * 2.0;
        let h = self.scene.canvas.height + pad * 2.0;
        let bg = to_drawio_color(bg);
        write!(
            self.xml,
            r#"        <mxCell id="drawio-bg" value="" style="rounded=0;whiteSpace=wrap;html=1;fillColor={bg};strokeColor=none;" vertex="1" parent="1">
          <mxGeometry x="0" y="0" width="{w}" height="{h}" as="geometry" />
        </mxCell>
"#,
            bg = bg,
            w = w,
            h = h,
        )
        .unwrap();
    }

    /// 分组：有标签 → swimlane 容器；无标签 → 轻量虚线矩形容器。
    /// 导出 fillColor/strokeColor 以保真配色；嵌套 group 坐标相对父容器。
    fn write_groups(&mut self) {
        let pad = self.options.page_padding;
        // 先收集分组信息（避免借用冲突）
        let group_entries: Vec<_> = self.scene.groups.iter().map(|group| {
            let gid = group.group.id.as_str();
            let abs_x = group.layout.x + pad;
            let abs_y = group.layout.y + pad;
            let has_label = !group.group.label.is_empty();
            let parent_gid = group.group.parent_id.as_ref();
            (gid.to_string(), group.clone(), abs_x, abs_y, has_label, parent_gid.cloned())
        }).collect();

        for (gid, group, abs_x, abs_y, has_label, parent_gid) in group_entries {
            let cell_id = self.next_cell_id();

            // 解析 parent：若父 group 已注册则使用其 cell_id，否则 "1"
            let (parent, rel_x, rel_y) = if let Some(ref pid) = parent_gid {
                if let Some(info) = self.group_id_map.get(pid.as_str()) {
                    (info.cell_id.clone(), abs_x - info.abs_x, abs_y - info.abs_y)
                } else {
                    ("1".to_string(), abs_x, abs_y)
                }
            } else {
                ("1".to_string(), abs_x, abs_y)
            };

            self.group_id_map.insert(
                gid.clone(),
                GroupLayoutInfo { cell_id: cell_id.clone(), abs_x, abs_y },
            );

            let label = if !has_label {
                self.report.warnings.push(ExportWarning {
                    code: "GROUP_NO_LABEL".to_string(),
                    entity_id: Some(gid.clone()),
                    edge_index: None,
                    tier: DegradeTier::L1,
                    message: format!("分组 '{}' 无标签，降级为普通矩形容器", gid),
                });
                String::new()
            } else {
                xml_escape(&group.group.label)
            };

            // 样式：有标签 → swimlane；无标签 → 轻量虚线矩形容器
            let mut style_parts: Vec<String> = if has_label {
                vec![
                    "swimlane".to_string(),
                    "childLayout=stackLayout".to_string(),
                    "horizontal=1".to_string(),
                    "startSize=26".to_string(),
                    "horizontalStack=0".to_string(),
                    "resizeParent=1".to_string(),
                    "resizeParentMax=0".to_string(),
                    "resizeLast=0".to_string(),
                    "collapsible=1".to_string(),
                    "marginBottom=0".to_string(),
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

            // 分组颜色保真
            style_parts.push(format!("fillColor={}", to_drawio_color(&group.fill)));
            style_parts.push(format!("strokeColor={}", to_drawio_color(&group.stroke)));

            if self.options.include_export_metadata {
                style_parts.push(format!("drawifyGroupId={}", sanitize_id(&gid)));
            }
            let style = style_parts.join(";");

            write!(
                self.xml,
                r#"        <mxCell id="{id}" value="{label}" style="{style}" vertex="1" parent="{parent}">
          <mxGeometry x="{x}" y="{y}" width="{w}" height="{h}" as="geometry" />
        </mxCell>
"#,
                id = cell_id,
                label = label,
                style = style,
                parent = parent,
                x = rel_x,
                y = rel_y,
                w = group.layout.width,
                h = group.layout.height,
            )
            .unwrap();

            self.report.stats.groups += 1;
            self.report.stats.l0 += 1;
        }
    }

    /// 边图层：在节点之下绘制（与 SVG 三图层对齐）。
    fn write_edges(&mut self) {
        for edge in &self.scene.edges {
            self.write_edge(edge);
        }
    }

    /// 导出单条边。
    ///
    /// 关键契约（spec §7 / §11.1）：
    /// - `source`/`target` **始终**绑定到节点（即使路径无效），保证 draw.io 中可拖拽重连；
    /// - 端口 `exitX/exitY/exitDx/exitDy/entryX/entryY/entryDx/entryDy` 写入 style 字符串；
    /// - 箭头方向依据 `relation.arrow`（Active/Passive/Bidirectional），而非物化层 `EdgeStyle.arrow`；
    /// - 颜色保真：导出 strokeColor/strokeWidth/dashed（非 Passive 边的手动虚线）。
    fn write_edge(&mut self, edge: &ExportEdge<'_>) {
        let edge_id = self.next_cell_id();
        let from_id = sanitize_id(edge.relation.from.as_str());
        let to_id = sanitize_id(edge.relation.to.as_str());
        let source_cell = format!("drawio-node-{}", from_id);
        let target_cell = format!("drawio-node-{}", to_id);

        // 路由策略：优先 draw.io 原生 edgeStyle，最多 max_edge_waypoints 个拐点
        let routing = plan_drawio_edge_routing(edge, self.options, &mut self.report);

        // 端口坐标：从路径几何锚点还原沿边偏移（平行边/反向边不再共用中心 0.5）
        let (exit_x, exit_y, entry_x, entry_y) = resolve_edge_ports(edge, self.scene);

        // 构建样式：结构性属性 + 颜色保真
        let mut style_parts: Vec<String> = vec!["html=1".to_string()];
        style_parts.extend(routing.style_parts.iter().cloned());
        // 箭头方向语义（来自 relation.arrow）
        style_parts.extend(arrow_style_parts(&edge.relation.arrow).iter().copied().map(String::from));
        // 端口连接点（drawio 约定：写在 style 字符串中）
        style_parts.push(format!("exitX={}", fmt_port_coord(exit_x)));
        style_parts.push(format!("exitY={}", fmt_port_coord(exit_y)));
        style_parts.push("exitDx=0".to_string());
        style_parts.push("exitDy=0".to_string());
        style_parts.push(format!("entryX={}", fmt_port_coord(entry_x)));
        style_parts.push(format!("entryY={}", fmt_port_coord(entry_y)));
        style_parts.push("entryDx=0".to_string());
        style_parts.push("entryDy=0".to_string());

        // 边颜色保真（spec §7.3）
        style_parts.push(format!("strokeColor={}", to_drawio_color(&edge.style.stroke)));
        if let Some(sw) = fmt_stroke_width(edge.style.stroke_width) {
            style_parts.push(format!("strokeWidth={sw}"));
        }
        // 非 Passive 边的手动虚线（Passive 已由 arrow_style_parts 写入 dashed=1）
        let is_passive = matches!(edge.relation.arrow, ArrowType::Passive);
        if !is_passive && (edge.style.dashed || edge.style.stroke_dasharray.is_some()) {
            style_parts.push("dashed=1".to_string());
        }
        // sketch 模式
        if edge.style.hand_drawn || is_sketch_graphic_style(&self.scene.context.graphic_style) {
            style_parts.push("sketch=1".to_string());
        }

        // 边标签：作为 edge cell 的 value 文字（draw.io 原生，随边移动）
        let label_info = self.edge_label_with_position(edge);
        let value_attr = match &label_info {
            Some((text, _)) => format!(r#" value="{text}""#),
            None => String::new(),
        };
        if label_info.is_some() {
            style_parts.push(format!(
                "fontColor={}",
                to_drawio_color(&edge.style.label_style.text_color)
            ));
        }

        // 元数据
        if self.options.include_export_metadata {
            style_parts.push(format!("drawifyRelationIndex={}", edge.index));
            style_parts.push(format!("drawifyDegrade={}", routing.tier.as_str()));
        }
        let style = style_parts.join(";");

        let label_center = label_info.as_ref().map(|(_, pos)| *pos);
        let geometry_xml = format_edge_geometry(
            &routing.waypoints,
            label_center,
            &edge.layout.geometry,
        );

        // source/target 始终写入（spec §7.5 / §11.1 最高优先级）
        write!(
            self.xml,
            r#"        <mxCell id="{id}"{value} style="{style}" edge="1" parent="1" source="{src}" target="{tgt}">
          {geometry}
        </mxCell>
"#,
            id = edge_id,
            value = value_attr,
            style = style,
            src = source_cell,
            tgt = target_cell,
            geometry = geometry_xml,
        )
        .unwrap();

        self.report.stats.edges += 1;
        match routing.tier {
            DegradeTier::L0 => self.report.stats.l0 += 1,
            DegradeTier::L1 => self.report.stats.l1 += 1,
            DegradeTier::L2 => self.report.stats.l2 += 1,
            _ => self.report.stats.l3 += 1,
        }
    }

    /// 计算边标签文本及位置，并对多标签/head_label/tail_label 记录降级警告（spec §7.4）。
    ///
    /// 返回 `(escaped_text, label_center)` 以便写入 edge cell 的 value 与 geometry。
    fn edge_label_with_position(&mut self, edge: &ExportEdge<'_>) -> Option<(String, Point)> {
        // 多标签：Phase 1 仅导出首个
        if edge.layout.labels.len() > 1 {
            self.report.warnings.push(ExportWarning {
                code: "MULTI_LABEL".to_string(),
                entity_id: None,
                edge_index: Some(edge.index),
                tier: DegradeTier::L2,
                message: format!(
                    "边 #{} 有 {} 个标签，仅导出首个",
                    edge.index,
                    edge.layout.labels.len()
                ),
            });
        }
        // head/tail 标签：Phase 1 不导出，记录 L2 警告
        if edge.relation.head_label.is_some() {
            self.report.warnings.push(ExportWarning {
                code: "EDGE_HEAD_LABEL_DROPPED".to_string(),
                entity_id: None,
                edge_index: Some(edge.index),
                tier: DegradeTier::L2,
                message: format!(
                    "边 #{} head_label 未导出（Phase 1 仅支持单标签）",
                    edge.index
                ),
            });
        }
        if edge.relation.tail_label.is_some() {
            self.report.warnings.push(ExportWarning {
                code: "EDGE_TAIL_LABEL_DROPPED".to_string(),
                entity_id: None,
                edge_index: Some(edge.index),
                tier: DegradeTier::L2,
                message: format!(
                    "边 #{} tail_label 未导出（Phase 1 仅支持单标签）",
                    edge.index
                ),
            });
        }
        // 优先 layout.labels[0].text + center，回退 relation.label（无位置信息时用边中点）
        if let Some(first) = edge.layout.labels.first() {
            return Some((xml_escape(&first.text), first.center));
        }
        if let Some(ref rel_label) = edge.relation.label {
            let center = geometry_midpoint(&edge.layout.geometry);
            return Some((xml_escape(rel_label), center));
        }
        None
    }

    /// 节点图层：在边之上绘制。
    fn write_nodes(&mut self) {
        for node in &self.scene.nodes {
            self.write_node(node);
        }
    }

    /// 导出单个节点。
    ///
    /// 保留：形状（结构）、坐标、尺寸、标签文本、id、parent 归属。
    /// 导出：fillColor/strokeColor/strokeWidth/dashed（颜色保真）。
    /// 当 parent 指向分组时，坐标自动转为相对容器的偏移。
    fn write_node(&mut self, node: &ExportNode<'_>) {
        let entity_id = sanitize_id(node.entity.id.as_str());
        let node_cell_id = format!("drawio-node-{}", entity_id);

        let pad = self.options.page_padding;
        let abs_x = node.layout.x + pad;
        let abs_y = node.layout.y + pad;

        // 解析 parent 与相对坐标
        let (parent, node_x, node_y) = if let Some(gid) = &node.entity.group_id {
            if let Some(info) = self.group_id_map.get(gid.as_str()) {
                (info.cell_id.clone(), abs_x - info.abs_x, abs_y - info.abs_y)
            } else {
                ("1".to_string(), abs_x, abs_y)
            }
        } else {
            ("1".to_string(), abs_x, abs_y)
        };

        let (mut shape_style, mut tier) = shape_to_drawio_style(&node.style.shape);
        if tier > DegradeTier::L0 {
            self.report.warnings.push(ExportWarning {
                code: "SHAPE_DEGRADE".to_string(),
                entity_id: Some(entity_id.clone()),
                edge_index: None,
                tier,
                message: format!(
                    "节点 '{}' 形状 {:?} 降级为 {}",
                    entity_id, node.style.shape, tier.as_str()
                ),
            });
        }

        let icon_plan =
            plan_node_icon(node, self.scene, self.options, &mut self.report, &entity_id);
        tier = tier.max(icon_plan.tier);
        let label_in_image = icon_plan.label_in_image;
        if let Some(image_style) = icon_plan.image_style {
            shape_style = image_style;
        }

        // 样式：形状 + 颜色保真 + 可选 sketch
        let mut style_parts: Vec<String> =
            vec![shape_style, "whiteSpace=wrap".to_string(), "html=1".to_string()];

        // 节点颜色保真（spec §6.3）
        let fill_hex = node.style.fill.trim_start_matches('#');
        style_parts.push(format!("fillColor={}", to_drawio_color(&node.style.fill)));
        style_parts.push(format!("strokeColor={}", to_drawio_color(&node.style.stroke)));
        // 字体颜色：优先主题 text_fill，回退到根据 fill 亮度自动对比
        let font_color = {
            let theme_color = color_queries::primary_text_color(
                &self.scene.diagram().diagram_type,
                &self.scene.context,
                "",
            );
            if !theme_color.is_empty() {
                to_drawio_color(&theme_color)
            } else {
                to_drawio_color(&contrast_font_color(fill_hex))
            }
        };
        style_parts.push(format!("fontColor={}", font_color));
        if let Some(sw) = fmt_stroke_width(node.style.stroke_width) {
            style_parts.push(format!("strokeWidth={sw}"));
        }
        // 虚线（stroke_dasharray 存在时）
        if node.style.stroke_dasharray.is_some() {
            style_parts.push("dashed=1".to_string());
        }
        // sketch 模式（hand_drawn 节点或全局 excalidraw/cross-hatch 风格）
        if node.style.hand_drawn || is_sketch_graphic_style(&self.scene.context.graphic_style) {
            style_parts.push("sketch=1".to_string());
        }
        // 标签加粗（label_weight ≥ 600 → fontStyle=1）
        if let Some(ref weight) = node.style.label_weight {
            if let Ok(w) = weight.parse::<u32>() {
                if w >= 600 {
                    style_parts.push("fontStyle=1".to_string());
                }
            }
        }

        if self.options.include_export_metadata {
            style_parts.push(format!("drawifyEntityId={}", entity_id));
            if tier > DegradeTier::L0 {
                style_parts.push(format!("drawifyDegrade={}", tier.as_str()));
            }
        }
        let style = style_parts.join(";");

        // 标签：图标嵌入 SVG 时文字已在 image 内；否则写入 value
        let label = if label_in_image {
            String::new()
        } else {
            node.entity.label.replace('\n', "&#xa;")
        };

        write!(
            self.xml,
            r#"        <mxCell id="{id}" value="{label}" style="{style}" vertex="1" parent="{parent}">
          <mxGeometry x="{x}" y="{y}" width="{w}" height="{h}" as="geometry" />
        </mxCell>
"#,
            id = node_cell_id,
            label = xml_escape(&label),
            style = style,
            parent = parent,
            x = node_x,
            y = node_y,
            w = node.layout.width,
            h = node.layout.height,
        )
        .unwrap();

        self.report.stats.nodes += 1;
        match tier {
            DegradeTier::L0 => self.report.stats.l0 += 1,
            DegradeTier::L1 => self.report.stats.l1 += 1,
            DegradeTier::L2 => self.report.stats.l2 += 1,
            _ => self.report.stats.l3 += 1,
        }
    }

    /// 标题（最顶层）：独立 text cell，不参与 layout。
    /// 尺寸根据标题文本长度动态计算，避免截断。
    fn write_title(&mut self) {
        let Some(ref title) = self.scene.canvas.title else {
            return;
        };
        let pad = self.options.page_padding;
        let title_color = to_drawio_color(&self.scene.canvas.title_color);
        // 动态计算标题尺寸：fontSize=16 时约 9.6px/字符宽，行高 22px
        let char_width = 9.6_f64;
        let line_height = 22.0_f64;
        let h_padding = 16.0_f64;
        let v_padding = 8.0_f64;
        let max_line_len = title.lines().map(|l| l.chars().count()).max().unwrap_or(1) as f64;
        let line_count = title.lines().count().max(1) as f64;
        let title_w = (max_line_len * char_width + h_padding).max(60.0);
        let title_h = (line_count * line_height + v_padding).max(30.0);
        write!(
            self.xml,
            r#"        <mxCell id="drawio-title" value="{title}" style="text;html=1;strokeColor=none;fillColor=none;align=left;verticalAlign=top;whiteSpace=wrap;fontSize=16;fontStyle=1;fontColor={title_color};" vertex="1" parent="1">
          <mxGeometry x="{pad}" y="{pad}" width="{w:.0}" height="{h:.0}" as="geometry" />
        </mxCell>
"#,
            title = xml_escape(title),
            title_color = title_color,
            pad = pad,
            w = title_w,
            h = title_h,
        )
        .unwrap();
    }

    /// 闭合 root / mxGraphModel / diagram / mxfile。
    fn write_footer(&mut self) {
        self.xml.push_str(
            r#"      </root>
    </mxGraphModel>
  </diagram>
</mxfile>
"#,
        );
    }

    /// 根据统计更新全局降级级别。
    fn finalize_report(&mut self) {
        if self.report.stats.l3 > 0 {
            self.report.global_degrade = DegradeTier::L3;
        } else if self.report.stats.l2 > 0 {
            self.report.global_degrade = DegradeTier::L2;
        } else if self.report.stats.l1 > 0 {
            self.report.global_degrade = DegradeTier::L1;
        }
    }
}
