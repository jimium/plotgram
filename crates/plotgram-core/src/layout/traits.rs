use crate::ast::Diagram;
use crate::types::DiagramType;
use crate::layout::algorithm_config::AlgorithmOptionSpec;
use crate::layout::grid_snap::{NodeAlignConfig, EdgeSnapConfig};
use super::types::LayoutResult;
use std::collections::HashSet;

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
    /// 其他布局返回 `false`（默认），由 `EdgeRoutingStrategy` 统一计算边路径。
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

/// 边路由策略 trait
///
/// 所有边路由算法都需要实现此 trait。
/// 在节点布局完成后，为每条边计算几何路径与标签位置。
pub trait EdgeRoutingStrategy {
    /// 算法名称
    fn name(&self) -> &'static str;

    /// 在节点布局完成后，为所有边计算几何路径
    fn route(&self, diagram: &Diagram, result: LayoutResult) -> LayoutResult;

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

    /// 是否输出可被 refine 后处理消费的 Polyline 路径。
    ///
    /// refine（`refine::run_refine`）只检测 `PathGeometry::Polyline` 的穿障情况，
    /// 对直线 / 贝塞尔路径是空跑。返回 `false` 的 router 不会进入 refine 循环。
    /// 默认 `false`，需要 refine 的 router（如 spline / orthogonal）覆写为 `true`。
    fn supports_refine(&self) -> bool {
        false
    }

    /// 是否需要避障索引（用于调度层决定是否预建全图障碍索引）。
    ///
    /// 当前仅 spline 路由会用到可见性图避障；其他 router 返回 `false`，
    /// 调度层据此跳过 `ObstacleIndex` 的构建开销。S3 阶段接入。
    fn needs_obstacle_index(&self) -> bool {
        false
    }

    /// 节点位移后的增量重路由（默认回退为全图重路由）。
    ///
    /// 正交路由覆写为仅重路由端点落在 `moved_node_ids` 上的边，
    /// 其余边保留已有路径并作为已路由段参与避让。
    fn route_after_node_moves(
        &self,
        diagram: &Diagram,
        result: LayoutResult,
        moved_node_ids: &HashSet<String>,
    ) -> LayoutResult {
        let _ = moved_node_ids;
        self.route(diagram, result)
    }

    /// refine 增量重路由：保留 `preserve_edges` 中的已有路径，仅重算其余边。
    ///
    /// 默认实现回退为全图重路由；orthogonal 覆写为 `preserve_edges` 增量模式。
    fn route_preserve(
        &self,
        diagram: &Diagram,
        result: LayoutResult,
        preserve_edges: &HashSet<usize>,
    ) -> LayoutResult {
        let _ = preserve_edges;
        self.route(diagram, result)
    }

    /// 声明该路由算法的边 waypoint snap 配置。
    ///
    /// 默认返回禁用配置；输出正交折线的路由算法（如 orthogonal）
    /// 应覆写此方法返回对应的 [`EdgeSnapConfig`]。
    fn edge_snap_config(&self) -> EdgeSnapConfig {
        EdgeSnapConfig::disabled()
    }
}
