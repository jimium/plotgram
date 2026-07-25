use crate::ast::Diagram;
use crate::types::DiagramType;
use crate::layout::algorithm_config::AlgorithmOptionSpec;
use crate::layout::routing::model::prepared::PreparedRoutingInput;
use crate::layout::snap::grid_snap::{NodeAlignConfig, EdgeSnapConfig};
use super::types::{EdgeLayout, LayoutResult};

// ─── Layout Trait ────────────────────────────────────────

/// 布局策略 trait
///
/// 所有布局算法都需要实现此 trait。
pub trait LayoutStrategy {
    /// 算法名称
    fn name(&self) -> &'static str;

    /// 根据 Diagram 计算布局（节点 + 分组）
    fn compute(&self, diagram: &Diagram) -> LayoutResult;

    /// 该布局算法是否在 `compute` 阶段自行产出边几何信息。
    ///
    /// 返回 `true` 时，`compute_layout` 将跳过通用边路由后处理，
    /// 避免覆盖已经精心计算好的路径。
    ///
    /// 当前返回 `true` 的布局：`sequence`。
    /// 其他布局返回 `false`（默认），由 `RoutingRecipeDyn` 统一计算边路径。
    fn produces_edge_geometry(&self) -> bool {
        false
    }

    /// 该算法适用的内置图表类型列表。
    ///
    /// 算法是自身适用范围的权威：由算法声明自己适合哪些图类型，
    /// 而非由图类型集中罗列适用算法。
    fn applicable_diagram_types(&self) -> &'static [DiagramType] {
        &[]
    }

    /// 是否支持 Custom 图表类型（默认 false）。
    ///
    /// Custom 类型无法放入 `applicable_diagram_types` 的静态数组，
    /// 因此单独声明。
    fn supports_custom(&self) -> bool {
        false
    }

    /// 判断是否支持指定的图表类型。
    ///
    /// 默认实现：Custom 类型走 `supports_custom()`，其余走 `applicable_diagram_types()` 包含判断。
    fn supports_diagram_type(&self, diagram_type: &DiagramType) -> bool {
        match diagram_type {
            DiagramType::Custom(_) => self.supports_custom(),
            other => self.applicable_diagram_types().contains(other),
        }
    }

    /// 该算法支持的 DSL 配置块 option 列表。
    fn option_specs(&self) -> &'static [AlgorithmOptionSpec] {
        &[]
    }

    /// 该布局算法支持的方向列表。
    ///
    /// 空切片表示不消费 diagram 级 `direction`（如 sequence、circular）。
    /// 非空时，`validate_layout_config` 会校验 effective direction 是否在支持列表中。
    fn supported_directions(&self) -> &'static [&'static str] {
        &[]
    }

    /// 声明该布局算法的节点结构对齐配置。
    ///
    /// 默认返回禁用配置；需要节点对齐的算法（Sugiyama 系、Architecture 等）
    /// 应覆写此方法返回对应的 [`NodeAlignConfig`]。
    fn node_align_config(&self) -> NodeAlignConfig {
        NodeAlignConfig::disabled()
    }
}

// ─── EdgeRouting Trait ───────────────────────────────────

/// 路由产物：只含 edges + hints delta（不含 nodes/groups）。
///
/// 类型上不可能修改 nodes/groups（Slice B 退出判据）。
pub struct RoutingProduct {
    pub edges: Vec<EdgeLayout>,
    /// 路由写入的分组路由提示。
    pub group_routing: Option<crate::layout::group::GroupRoutingHints>,
    /// 路由写入的边注释集。
    pub route_annotations: Option<crate::layout::routing::RouteAnnotationSet>,
    /// 正交路由调试统计。
    pub orthogonal_debug: Option<super::types::OrthoDebugStats>,
}

/// 边路由策略 trait
///
/// 所有边路由算法都需要实现此 trait。
/// 在节点布局完成后，为每条边计算几何路径与标签位置。
pub trait RoutingRecipeDyn {
    /// 算法名称
    fn name(&self) -> &'static str;

    /// 唯一路由入口：消费只读 PreparedRoutingInput，产出 RoutingProduct。
    ///
    /// 类型上不可能修改 nodes/groups。
    fn route(&self, input: &PreparedRoutingInput<'_>) -> RoutingProduct;

    /// 该路由算法适用的内置图表类型列表。
    fn applicable_diagram_types(&self) -> &'static [DiagramType] {
        &[]
    }

    /// 是否支持 Custom 图表类型（默认 false）。
    fn supports_custom(&self) -> bool {
        false
    }

    /// 判断是否支持指定的图表类型。
    fn supports_diagram_type(&self, diagram_type: &DiagramType) -> bool {
        match diagram_type {
            DiagramType::Custom(_) => self.supports_custom(),
            other => self.applicable_diagram_types().contains(other),
        }
    }

    /// 该算法支持的 DSL 配置块 option 列表。
    fn option_specs(&self) -> &'static [AlgorithmOptionSpec] {
        &[]
    }

    /// 声明该路由算法的边 waypoint snap 配置。
    ///
    /// 默认返回禁用配置；输出正交折线的路由算法（如 orthogonal）
    /// 应覆写此方法返回对应的 [`EdgeSnapConfig`]。
    fn edge_snap_config(&self) -> EdgeSnapConfig {
        EdgeSnapConfig::disabled()
    }
}
