//! 标准图（flowchart / state / architecture / mindmap）样式配置。

use crate::ast::{ArrowType, Entity, Relation};
use crate::render::paint::style_mapping;
use crate::render::visual::{ArrowStyle, EdgeStyle, NodeShape, NodeStyle};
use crate::render::CompiledRenderContext;
use crate::types::DiagramType;

/// 标准图边的样式配置。
pub struct StandardEdgeConfig {
    pub arrow_style: ArrowStyle,
    pub dashed_for_passive: bool,
    pub render_labels: bool,
}

impl StandardEdgeConfig {
    pub const ARROWED: Self = Self {
        arrow_style: ArrowStyle::Normal,
        dashed_for_passive: true,
        render_labels: true,
    };

    pub const NO_ARROW: Self = Self {
        arrow_style: ArrowStyle::None,
        dashed_for_passive: false,
        render_labels: true,
    };
}

/// 标准图在 scene 物化阶段使用的样式参数。
pub struct StandardStyleConfig {
    pub diagram_type: DiagramType,
    pub label_weight: &'static str,
    pub edge_config: StandardEdgeConfig,
    pub force_shape: Option<NodeShape>,
}

pub fn materialize_node_style(
    config: &StandardStyleConfig,
    entity: &Entity,
    context: &CompiledRenderContext,
) -> NodeStyle {
    let mut style = style_mapping::node_style_from_attributes(entity);
    if let Some(shape) = config.force_shape.clone() {
        style.shape = shape;
    }
    context.graphic_painter().decorate_node_style(&mut style);
    style
}

pub fn materialize_edge_style(
    config: &StandardStyleConfig,
    relation: &Relation,
    context: &CompiledRenderContext,
) -> EdgeStyle {
    let mut style = style_mapping::edge_style_from_attributes(relation);
    if config.edge_config.dashed_for_passive {
        style.dashed = matches!(relation.arrow, ArrowType::Passive);
    }
    style.arrow = config.edge_config.arrow_style.clone();
    context.graphic_painter().decorate_edge_style(&mut style);
    style
}

pub static FLOWCHART: StandardStyleConfig = StandardStyleConfig {
    diagram_type: DiagramType::Flowchart,
    label_weight: "500",
    edge_config: StandardEdgeConfig::ARROWED,
    force_shape: None,
};

pub static STATE: StandardStyleConfig = StandardStyleConfig {
    diagram_type: DiagramType::State,
    label_weight: "400",
    edge_config: StandardEdgeConfig::ARROWED,
    force_shape: None,
};

pub static ARCHITECTURE: StandardStyleConfig = StandardStyleConfig {
    diagram_type: DiagramType::Architecture,
    label_weight: "500",
    edge_config: StandardEdgeConfig::ARROWED,
    force_shape: None,
};

pub static MINDMAP: StandardStyleConfig = StandardStyleConfig {
    diagram_type: DiagramType::Mindmap,
    label_weight: "500",
    edge_config: StandardEdgeConfig::NO_ARROW,
    // shape 由 theme cascade 按 entity_types[root/main/branch/leaf] 物化（root=circle，其余=rounded_rect）
    force_shape: None,
};
