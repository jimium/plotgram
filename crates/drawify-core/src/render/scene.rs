//! ExportScene 中间层。
//!
//! 负责把 `PreparedDiagram` 标准化为渲染器无关的导出场景,分两步:
//! - [`compute_layout`]:布局计算(纯拓扑,不依赖渲染格式)
//! - [`build_scene`]:视觉物化(依赖 layout + context,物化节点/边/分组视觉属性)
//!
//! [`export_scene`] 为向后兼容的一步到位入口(= compute_layout + build_scene)。
//! 后续 SVG/ASCII/PNG/WebP 等格式编码共享这份中间表示;
//! SVG 编码见 [`crate::render::paint::scene_svg`]。

use crate::ast::{Diagram, Entity, Group, PreparedDiagram, Relation};
use crate::kinds;
use crate::render::visual::{EdgeStyle, NodeStyle};
use crate::error::{DrawifyError, Result};
use crate::layout::{self, EdgeLayout, GroupLayout, LayoutResult, NodeLayout, RefinementReport};
use crate::render::paint::color_queries;
use crate::render::{RenderRequest, CompiledRenderContext};
use serde::Serialize;

/// 画布级导出信息。
#[derive(Debug, Clone, Serialize)]
pub struct ExportCanvas {
    pub width: f64,
    pub height: f64,
    pub title: Option<String>,
    pub background: String,
    pub title_color: String,
    pub attribution: bool,
}

/// 导出的节点元素。
#[derive(Debug, Clone, Serialize)]
pub struct ExportNode<'a> {
    pub entity: &'a Entity,
    pub layout: NodeLayout,
    pub style: NodeStyle,
}

/// 导出的边元素。
#[derive(Debug, Clone, Serialize)]
pub struct ExportEdge<'a> {
    pub index: usize,
    pub relation: &'a Relation,
    pub layout: EdgeLayout,
    pub style: EdgeStyle,
}

/// 导出的分组元素。
#[derive(Debug, Clone, Serialize)]
pub struct ExportGroup<'a> {
    pub group: &'a Group,
    pub layout: GroupLayout,
    pub fill: String,
    pub stroke: String,
    pub label_color: String,
    pub stroke_width: f64,
    /// 渲染层级：depth 越小越先画（背景），越大越后画（前景）。
    /// 外层容器组 depth=0 先画，内嵌子组 depth=1+ 后画，确保子组不被父组覆盖。
    pub z_index: u8,
    /// 圆角半径（像素）。顶层分组默认 8，嵌套分组默认 6。
    pub border_radius: f64,
    /// 是否绘制阴影。仅顶层分组（depth=0）启用阴影，嵌套分组不启用。
    pub has_shadow: bool,
}

/// 渲染前的标准化导出场景。
pub struct ExportScene<'a> {
    pub prepared: &'a PreparedDiagram,
    pub canvas: ExportCanvas,
    pub layout: LayoutResult,
    pub context: CompiledRenderContext,
    pub nodes: Vec<ExportNode<'a>>,
    pub edges: Vec<ExportEdge<'a>>,
    pub groups: Vec<ExportGroup<'a>>,
    /// 布局意图修正报告。
    ///
    /// 由 `export_scene` 在调用 `compute_layout_with_plan_and_overlay` 后填充。
    /// `overlay` 为 `None` 时为 `None`；`build_scene` 独立调用时为 `None`。
    pub refinement_report: Option<RefinementReport>,
}

impl<'a> ExportScene<'a> {
    pub fn diagram(&self) -> &Diagram {
        self.prepared.inner()
    }
}

/// 布局计算(独立步骤,不依赖渲染格式)。
///
/// 将 `PreparedDiagram` 经 `LayoutPlan` 计算为 `LayoutResult`。
/// 抽离为独立函数便于调用方复用布局结果(WASM 多格式场景)、
/// 做布局后处理(refine 优化)或缓存。
pub fn compute_layout(diagram: &PreparedDiagram) -> Result<LayoutResult> {
    layout::compute_layout_with_plan(diagram.inner(), diagram.layout_plan())
        .map_err(|e| DrawifyError::layout_failed_msg(e.to_string()))
}

/// 视觉物化(依赖 layout + context,不依赖编码格式)。
///
/// 将 `RenderRequest` + 已算好的 `LayoutResult` 物化为渲染器无关的 `ExportScene`:
/// 解析主题/graphic style、物化节点/边/分组的最终基础视觉属性。
pub fn build_scene<'a>(
    request: &'a RenderRequest<'a>,
    layout: LayoutResult,
) -> Result<ExportScene<'a>> {
    let context = request.resolve_context()?;
    let diagram = request.diagram.inner();
    let entry = kinds::entry_for(&diagram.diagram_type);

    let nodes = diagram
        .entities
        .iter()
        .filter_map(|entity| {
            layout
                .nodes
                .get(entity.id.as_str())
                .cloned()
                .map(|node_layout| ExportNode {
                    entity,
                    layout: node_layout,
                    style: (entry.materialize_node_style)(entity, &context),
                })
        })
        .collect();

    let edges = diagram
        .relations
        .iter()
        .enumerate()
        .map(|(index, relation)| ExportEdge {
            index,
            relation,
            layout: layout.edges.get(index).cloned().unwrap_or_else(EdgeLayout::empty),
            style: (entry.materialize_edge_style)(relation, &context),
        })
        .collect();

    // group 视觉属性从 prepare 物化后的 `attributes.style` 读取。
    let mut groups: Vec<ExportGroup<'_>> = diagram
        .groups
        .iter()
        .filter_map(|group| {
            layout.groups.get(group.id.as_str()).cloned().map(|group_layout| {
                let style = &group.attributes.style;
                let fill = style
                    .get("fill")
                    .and_then(|v| v.as_str())
                    .unwrap_or("#f0f0f0")
                    .to_string();
                let stroke = style
                    .get("stroke")
                    .and_then(|v| v.as_str())
                    .unwrap_or("#ccc")
                    .to_string();
                let label_color = style
                    .get("text_fill")
                    .or_else(|| style.get("label_color"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("#666")
                    .to_string();
                let stroke_width = style
                    .get("stroke_width")
                    .and_then(|v| match v {
                        crate::ast::AttributeValue::Number(n) => Some(*n),
                        crate::ast::AttributeValue::String(s) => s.parse().ok(),
                        _ => None,
                    })
                    .unwrap_or(if group.depth == 0 { 2.0 } else { 1.5 });
                let border_radius = style
                    .get("border_radius")
                    .and_then(|v| match v {
                        crate::ast::AttributeValue::Number(n) => Some(*n),
                        crate::ast::AttributeValue::String(s) => s.parse().ok(),
                        _ => None,
                    })
                    .unwrap_or(if group.depth == 0 { 8.0 } else { 6.0 });
                ExportGroup {
                    group,
                    layout: group_layout,
                    fill,
                    stroke,
                    label_color,
                    stroke_width,
                    z_index: group.depth,
                    border_radius,
                    has_shadow: group.depth == 0,
                }
            })
        })
        .collect();
    // 按 z_index 升序排列：外层容器(depth=0)先画为背景，内嵌子组(depth=1+)后画为前景
    groups.sort_by_key(|g| g.z_index);

    let mut background = color_queries::canvas_background(diagram, &context);
    if request.transparent_background {
        background = "transparent".to_string();
    }

    let canvas = ExportCanvas {
        width: layout.total_width,
        height: layout.total_height,
        title: diagram.title().map(str::to_owned),
        background,
        title_color: color_queries::title_color(diagram, &context),
        attribution: request.attribution,
    };

    Ok(ExportScene {
        prepared: request.diagram,
        canvas,
        layout,
        context,
        nodes,
        edges,
        groups,
        refinement_report: None,
    })
}

/// 将 `RenderRequest` 导出为标准化场景(向后兼容:布局 + 物化一步到位)。
///
/// 等价于 `compute_layout_with_plan_and_overlay` 后再 `build_scene(request, layout)`，
/// 并将意图修正报告写入 `ExportScene.refinement_report`。
/// 新调用方推荐显式分两步调用,便于复用布局结果。
pub fn export_scene<'a>(request: &'a RenderRequest<'a>) -> Result<ExportScene<'a>> {
    // 直接使用 PreparedDiagram 中已缓存的 LayoutPlan（避免重复 resolve），
    // 并透传 layout_overlay 至布局阶段。
    let (layout, refinement_report) = layout::compute_layout_with_plan_and_overlay(
        request.diagram.inner(),
        request.diagram.layout_plan(),
        request.layout_overlay,
    )
    .map_err(|e| DrawifyError::layout_failed_msg(e.to_string()))?;

    let mut scene = build_scene(request, layout)?;
    scene.refinement_report = refinement_report;
    Ok(scene)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        ArrowType, AttributeMap, AttributeValue, Diagram, DiagramAttribute, Group, Identifier,
        Relation, SourceInfo, Span, TextValue,
    };
    use crate::types::DiagramType;
    use crate::render::RenderFormat;

    fn sample_prepared() -> PreparedDiagram {
        let span = Span::dummy();
        PreparedDiagram::new(Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![DiagramAttribute {
                key: "title".to_string(),
                value: AttributeValue::String(TextValue::quoted("Export Scene".to_string())),
                span,
            }],
            entities: vec![
                crate::ast::Entity {
                    id: Identifier::new_unchecked("a"),
                    label: "A".to_string(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
                crate::ast::Entity {
                    id: Identifier::new_unchecked("b"),
                    label: "B".to_string(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
            ],
            relations: vec![Relation {
                from: Identifier::new_unchecked("a"),
                to: Identifier::new_unchecked("b"),
                arrow: ArrowType::Active,
                label: Some("next".to_string()),
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span,
            }],
            groups: vec![],
            style_decls: vec![],
            source_info: SourceInfo {
                file: None,
                line_count: 1,
            },
            ..Default::default()
        })
    }

    fn sample_grouped_prepared() -> PreparedDiagram {
        let span = Span::dummy();
        PreparedDiagram::new(Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![],
            entities: vec![
                crate::ast::Entity {
                    id: Identifier::new_unchecked("a"),
                    label: "A".to_string(),
                    attributes: AttributeMap::default(),
                    group_id: Some(Identifier::new_unchecked("team")),
                    span,
                },
                crate::ast::Entity {
                    id: Identifier::new_unchecked("b"),
                    label: "B".to_string(),
                    attributes: AttributeMap::default(),
                    group_id: Some(Identifier::new_unchecked("team")),
                    span,
                },
            ],
            relations: vec![Relation {
                from: Identifier::new_unchecked("a"),
                to: Identifier::new_unchecked("b"),
                arrow: ArrowType::Active,
                label: None,
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span,
            }],
            groups: vec![Group {
                id: Identifier::new_unchecked("team"),
                label: "Team".to_string(),
                attributes: AttributeMap::default(),
                parent_id: None,
                depth: 0,
                entity_ids: vec![
                    Identifier::new_unchecked("a"),
                    Identifier::new_unchecked("b"),
                ],
                child_group_ids: vec![],
                span,
            }],
            style_decls: vec![],
            source_info: SourceInfo {
                file: None,
                line_count: 1,
            },
            ..Default::default()
        })
    }

    #[test]
    fn export_scene_materializes_layout_and_styles() {
        let diagram = sample_prepared();
        let request = RenderRequest::new(&diagram, RenderFormat::Svg);
        let scene = export_scene(&request).expect("export scene");

        assert_eq!(scene.diagram().diagram_type, DiagramType::Flowchart);
        assert_eq!(scene.nodes.len(), 2);
        assert_eq!(scene.edges.len(), 1);
        assert_eq!(scene.canvas.title.as_deref(), Some("Export Scene"));
        assert!(scene.canvas.width > 0.0);
        assert!(scene.canvas.height > 0.0);
        assert!(scene
            .edges
            .first()
            .is_some_and(|edge| edge.layout.path_len() >= 2));
    }

    #[test]
    fn export_scene_materializes_groups() {
        let diagram = sample_grouped_prepared();
        let request = RenderRequest::new(&diagram, RenderFormat::Svg);
        let scene = export_scene(&request).expect("export grouped scene");

        assert_eq!(scene.groups.len(), 1);
        let group = &scene.groups[0];
        assert_eq!(group.group.label, "Team");
        assert!(group.layout.width > 0.0);
        assert!(group.layout.height > 0.0);
        assert!(!group.fill.is_empty());
        assert!(!group.stroke.is_empty());
        assert!(!group.label_color.is_empty());
    }

    #[test]
    fn compute_layout_and_build_scene_split_works() {
        let diagram = sample_prepared();
        let request = RenderRequest::new(&diagram, RenderFormat::Svg);

        // 显式分两步:布局 → 物化
        let layout = compute_layout(&diagram).expect("compute layout");
        assert!(layout.total_width > 0.0);
        assert!(layout.total_height > 0.0);
        assert_eq!(layout.nodes.len(), 2);

        let scene = build_scene(&request, layout).expect("build scene");
        assert_eq!(scene.nodes.len(), 2);
        assert_eq!(scene.edges.len(), 1);
        assert!(scene.canvas.width > 0.0);
    }

    #[test]
    fn split_equivalent_to_export_scene() {
        // 拆分调用与一步到位应产出等价的 canvas 尺寸与节点数
        let diagram = sample_prepared();
        let request = RenderRequest::new(&diagram, RenderFormat::Svg);

        let one_shot = export_scene(&request).expect("one-shot");
        let layout = compute_layout(&diagram).expect("layout");
        let split = build_scene(&request, layout).expect("split");

        assert_eq!(one_shot.canvas.width, split.canvas.width);
        assert_eq!(one_shot.canvas.height, split.canvas.height);
        assert_eq!(one_shot.nodes.len(), split.nodes.len());
        assert_eq!(one_shot.edges.len(), split.edges.len());
    }
}
