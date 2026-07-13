use crate::types::DiagramType;
use crate::types::standard_attr_keys::diagram;
use crate::ast::Diagram;
use crate::error::DiagnosticError;
use crate::profile::profile_for;
use super::types::LayoutResult;
use super::traits::{LayoutStrategy, EdgeRoutingStrategy};

// ─── Layout 调度 ─────────────────────────────────────────

/// 根据 diagram 配置计算布局（统一入口）
///
/// 支持多种布局算法：
/// - "flowchart": 流程图专属分层布局（共享 sugiyama-v2 引擎）
/// - "er": ER 图专属分层布局（共享 sugiyama-v2 引擎）
/// - "state": 状态图专属布局（共享 circular 引擎）
/// - "architecture": 架构图专属布局（分组感知两阶段）
/// - "mindmap": 思维导图专属布局（中心辐射 / 单向树）
/// - "sequence": 时序图专属布局
/// - "sugiyama-v2": 通用 Sugiyama 分层布局（高级选项）
/// - "circular": 通用自适应圆形布局（高级选项）
/// - "force-directed": 通用力导向布局（高级选项）
/// - "sugiyama-v2": 增强分层布局（推荐）
/// - "sugiyama": 已别名到 sugiyama-v2（catalog 不再列出）
///
/// 可通过 diagram 属性 `layout_algo: 算法名` 切换。
///
/// 支持多种边路由算法：
/// - "orthogonal": 正交路由（折线路由）（默认）
/// - "straight": 直线路由
/// - "bezier": 贝塞尔曲线路由
/// - "spline": 障碍避让多段样条路由（可见性图 + Catmull-Rom → 多段贝塞尔，C1 连续）
/// - "circular": 弧形边路由（圆形布局专用）
/// 节点布局完成后自动执行边路由。
///
/// 解析 diagram 的有效布局方向。
///
/// - 若 AST 显式声明 `direction` → 返回该显式值
/// - 否则若 profile.default_direction 为 `Some` → 返回 profile 默认
/// - 否则 → `None`（该图不参与 direction 体系）
///
/// 所有布局代码统一通过此函数获取方向，禁止自行判断 AST 里有没有 direction 属性。
pub fn resolve_effective_direction<'a>(diagram: &'a Diagram) -> Option<&'a str> {
    if diagram.direction_attr().is_some() {
        return diagram.direction_attr();
    }
    profile_for(&diagram.diagram_type).default_direction
}

/// 布局配置的语义校验（算法是否存在、是否适用于当前图表类型）在此执行。
pub fn compute_layout(
    diagram: &Diagram,
) -> std::result::Result<LayoutResult, DiagnosticError> {
    let profile = profile_for(&diagram.diagram_type);
    let plan = crate::layout::plan::LayoutPlan::resolve(diagram, profile);
    compute_layout_with_plan(diagram, &plan)
}

/// 使用已解析的 [`crate::layout::plan::LayoutPlan`] 计算布局（`PreparedDiagram` 在 prepare 阶段已解析 plan 时走此路径）。
pub fn compute_layout_with_plan(
    diagram: &Diagram,
    plan: &crate::layout::plan::LayoutPlan,
) -> std::result::Result<LayoutResult, DiagnosticError> {
    validate_layout_config(diagram)?;
    crate::layout::pipeline::LayoutPipeline::new(diagram, plan).run()
}

fn layout_strategy_for(algo: &str) -> Option<Box<dyn LayoutStrategy>> {
    crate::layout::registry::build_layout_strategy(algo, &crate::layout::plan::LayoutPlan::default_for_catalog())
}

fn edge_routing_strategy_for(algo: &str) -> Option<Box<dyn EdgeRoutingStrategy>> {
    crate::layout::registry::build_edge_routing_strategy(algo, &crate::layout::plan::LayoutPlan::catalog_edge_plan(algo))
}

// ─── 算法注册表查询 ─────────────────────────────────────

/// 所有内置图表类型（不含 Custom，Custom 需单独通过 `supports_custom` 判断）
pub const BUILTIN_DIAGRAM_TYPES: &[DiagramType] = &[
    DiagramType::Flowchart,
    DiagramType::Sequence,
    DiagramType::Architecture,
    DiagramType::State,
    DiagramType::Er,
    DiagramType::Mindmap,
];

/// 返回所有已注册的布局策略实例（catalog / 元数据查询）。
pub(super) fn all_layout_strategies() -> Vec<Box<dyn LayoutStrategy>> {
    crate::layout::registry::all_layout_strategies()
}

/// 返回所有已注册的边路由策略实例（catalog / 元数据查询，使用 spec 默认值）。
pub(super) fn all_routing_strategies() -> Vec<Box<dyn EdgeRoutingStrategy>> {
    crate::layout::registry::all_routing_strategies()
}

/// 查询布局算法的 option 元数据。
pub fn layout_option_specs(algo: &str) -> &'static [crate::layout::algorithm_config::AlgorithmOptionSpec] {
    layout_strategy_for(algo)
        .map(|s| s.option_specs())
        .unwrap_or(&[])
}

/// 查询边路由算法的 option 元数据。
pub fn edge_routing_option_specs(algo: &str) -> &'static [crate::layout::algorithm_config::AlgorithmOptionSpec] {
    edge_routing_strategy_for(algo)
        .map(|s| s.option_specs())
        .unwrap_or(&[])
}

/// 正向查询：指定图表类型适用的布局算法名称列表。
pub fn applicable_layouts_for_type(diagram_type: &DiagramType) -> Vec<&'static str> {
    all_layout_strategies()
        .into_iter()
        .filter(|s| s.supports_diagram_type(diagram_type))
        .map(|s| s.name())
        .collect()
}

/// 正向查询：指定图表类型适用的边路由算法名称列表。
pub fn applicable_routings_for_type(diagram_type: &DiagramType) -> Vec<&'static str> {
    all_routing_strategies()
        .into_iter()
        .filter(|s| s.supports_diagram_type(diagram_type))
        .map(|s| s.name())
        .collect()
}

/// 反向查询：指定布局算法适用的图表类型列表。
pub fn diagram_types_for_layout(algo: &str) -> Vec<DiagramType> {
    let mut types: Vec<DiagramType> = BUILTIN_DIAGRAM_TYPES
        .iter()
        .filter(|dt| {
            all_layout_strategies()
                .into_iter()
                .any(|s| s.name() == algo && s.supports_diagram_type(dt))
        })
        .cloned()
        .collect();

    // 检查是否支持 Custom
    if all_layout_strategies()
        .into_iter()
        .any(|s| s.name() == algo && s.supports_custom())
    {
        types.push(DiagramType::Custom(String::new()));
    }

    types
}

/// 反向查询：指定边路由算法适用的图表类型列表。
pub fn diagram_types_for_routing(algo: &str) -> Vec<DiagramType> {
    let mut types: Vec<DiagramType> = BUILTIN_DIAGRAM_TYPES
        .iter()
        .filter(|dt| {
            all_routing_strategies()
                .into_iter()
                .any(|s| s.name() == algo && s.supports_diagram_type(dt))
        })
        .cloned()
        .collect();

    if all_routing_strategies()
        .into_iter()
        .any(|s| s.name() == algo && s.supports_custom())
    {
        types.push(DiagramType::Custom(String::new()));
    }

    types
}

pub(crate) fn known_layout_algo_names() -> Vec<&'static str> {
    crate::layout::registry::LAYOUT_ALGORITHM_NAMES.to_vec()
}

pub(crate) fn known_edge_routing_names() -> Vec<&'static str> {
    crate::layout::registry::EDGE_ROUTING_NAMES.to_vec()
}

fn layout_attr_span(diagram: &Diagram, key: &str) -> crate::ast::Span {
    diagram
        .attributes
        .iter()
        .find(|a| a.key == key)
        .map(|a| a.span)
        .unwrap_or_else(crate::ast::Span::dummy)
}

pub(crate) fn layout_config_error(
    diagram: &Diagram,
    key: &str,
    value: &str,
    known: &[&str],
) -> DiagnosticError {
    DiagnosticError::invalid_enum_value(layout_attr_span(diagram, key), key, value, known)
}

const VALID_DIRECTION_ATOMS: &[&str] = &[
    crate::types::attr_constants::direction::TOP_TO_BOTTOM,
    crate::types::attr_constants::direction::LEFT_TO_RIGHT,
    crate::types::attr_constants::direction::RADIAL,
];

/// 校验 diagram 级布局配置（算法名、路由名、方向、方向×布局交叉校验）。
///
/// 在 `compute_layout` 执行前调用；DSL 层只保证 atom 类型。
fn validate_layout_config(diagram: &Diagram) -> std::result::Result<(), DiagnosticError> {
    // 1. 逐属性基础校验
    for attr in &diagram.attributes {
        let Some(name) = attr.value.algorithm_name() else {
            continue;
        };

        match attr.key.as_str() {
            diagram::DIRECTION => {
                if !VALID_DIRECTION_ATOMS.contains(&name) {
                    return Err(DiagnosticError::invalid_enum_value(
                        attr.span,
                        diagram::DIRECTION,
                        name,
                        VALID_DIRECTION_ATOMS,
                    ));
                }
            }
            diagram::LAYOUT => validate_registered_layout_algo(diagram, attr.span, name)?,
            diagram::EDGE_ROUTING => {
                if applicable_routings_for_type(&diagram.diagram_type).is_empty() {
                    return Err(DiagnosticError::structure_violation(
                        attr.span,
                        format!(
                            "diagram type '{}' does not support edge_routing; \
                             message paths are computed by layout: sequence",
                            profile_for(&diagram.diagram_type).name,
                        ),
                    ));
                }
                validate_registered_edge_routing(diagram, attr.span, name)?;
            }
            _ => {}
        }
    }

    // 2. direction × layout 交叉校验
    validate_direction_layout_compat(diagram)?;

    Ok(())
}

/// 校验 effective direction 与当前 layout 算法的兼容性。
fn validate_direction_layout_compat(
    diagram: &Diagram,
) -> std::result::Result<(), DiagnosticError> {
    let profile = profile_for(&diagram.diagram_type);
    let plan = crate::layout::plan::LayoutPlan::resolve(diagram, profile);
    let algo = plan.layout_algo.as_str();

    let strategy = crate::layout::registry::build_layout_strategy(algo, &plan)
        .ok_or_else(|| layout_config_error(diagram, diagram::LAYOUT, algo, &[algo]))?;

    let supported = strategy.supported_directions();
    let explicit = diagram.direction_attr();

    if supported.is_empty() {
        // layout 不消费 direction
        if explicit.is_some() {
            let span = layout_attr_span(diagram, diagram::DIRECTION);
            return Err(DiagnosticError::structure_violation(
                span,
                format!(
                    "layout '{}' does not support the 'direction' attribute. \
                     Remove 'direction' from the diagram block.",
                    algo
                ),
            ));
        }
        return Ok(()); // 无需校验，不看 effective
    }

    // supported 非空：layout 消费 direction
    let effective = resolve_effective_direction(diagram).ok_or_else(|| {
        let span = layout_attr_span(diagram, diagram::DIRECTION);
        DiagnosticError::structure_violation(
            span,
            format!(
                "diagram type '{}' has no default direction configured",
                profile.name
            ),
        )
    })?;

    if !supported.contains(&effective) {
        let span = layout_attr_span(diagram, diagram::DIRECTION);
        let supported_list = supported.join(", ");
        // 生成 Hint：找到支持当前 direction 的 layout
        let hint = all_layout_strategies()
            .iter()
            .find(|s| {
                s.supports_diagram_type(&diagram.diagram_type)
                    && s.supported_directions().contains(&effective)
            })
            .map(|s| format!("\nHint: set direction: {}, or use layout: {}.", supported[0], s.name()))
            .unwrap_or_default();
        return Err(DiagnosticError::structure_violation(
            span,
            format!(
                "layout '{}' does not support direction '{}'.\n\
                 Supported directions: {}.{hint}",
                algo, effective, supported_list
            ),
        ));
    }
    Ok(())
}

fn validate_registered_layout_algo(
    diagram: &Diagram,
    span: crate::ast::Span,
    value: &str,
) -> std::result::Result<(), DiagnosticError> {
    let strategies = all_layout_strategies();
    let known: Vec<&str> = strategies.iter().map(|s| s.name()).collect();

    if !known.contains(&value) {
        return Err(DiagnosticError::invalid_enum_value(
            span, diagram::LAYOUT, value, &known,
        ));
    }

    let supported = strategies
        .iter()
        .any(|s| s.name() == value && s.supports_diagram_type(&diagram.diagram_type));
    if !supported {
        let applicable = applicable_layouts_for_type(&diagram.diagram_type);
        return Err(DiagnosticError::invalid_enum_value(
            span, diagram::LAYOUT, value, &applicable,
        ));
    }
    Ok(())
}

fn validate_registered_edge_routing(
    diagram: &Diagram,
    span: crate::ast::Span,
    value: &str,
) -> std::result::Result<(), DiagnosticError> {
    let strategies = all_routing_strategies();
    let known: Vec<&str> = strategies.iter().map(|s| s.name()).collect();

    if !known.contains(&value) {
        return Err(DiagnosticError::invalid_enum_value(
            span, diagram::EDGE_ROUTING, value, &known,
        ));
    }

    let supported = strategies
        .iter()
        .any(|s| s.name() == value && s.supports_diagram_type(&diagram.diagram_type));
    if !supported {
        let applicable = applicable_routings_for_type(&diagram.diagram_type);
        return Err(DiagnosticError::invalid_enum_value(
            span, diagram::EDGE_ROUTING, value, &applicable,
        ));
    }
    Ok(())
}
