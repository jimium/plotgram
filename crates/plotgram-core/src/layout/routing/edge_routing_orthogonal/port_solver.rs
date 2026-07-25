//! 端口约束求解器
//!
//! Phase B2：将端口选择从纯几何规则升级为约束求解，减少后处理修复。
//!
//! 当前问题：
//! - `choose_pair_sides_with_group` 是纯几何规则（`|dy| >= |dx| * threshold`）
//! - 不考虑同节点其他边的端口分配
//! - 导致 replan_slots / stub_fix 需要大量后处理
//!
//! 求解策略：
//! - 小规模（< 50 边）：贪心 + 局部搜索（swap 端口对）
//! - 大规模：保持当前几何规则

use crate::ast::Relation;
use crate::layout::routing::common::edge_geometry::node_center;
use crate::layout::geometry::{Point, EPS};
use crate::layout::group::GroupRoutingContext;
use crate::layout::{NodeLayout, Port};
use std::collections::HashMap;

use super::feedback_side::FeedbackSideAssignment;
use super::path::port_outward;
use super::slot::choose_pair_sides_with_group;

/// 端口分配结果（Phase 6：字段私有；求解后只读消费，禁止下游 `&mut` 改 side）。
#[derive(Clone, Debug)]
pub struct PortAssignment {
    from_side: Vec<Port>,
    to_side: Vec<Port>,
}

impl PortAssignment {
    pub fn from_side(&self) -> &[Port] {
        &self.from_side
    }

    pub fn to_side(&self) -> &[Port] {
        &self.to_side
    }

    /// 消费为可变向量（仅 port_slot 初始化 lane/slot 时一次性拆出）。
    pub fn into_sides(self) -> (Vec<Port>, Vec<Port>) {
        (self.from_side, self.to_side)
    }
}

/// 端口求解器输入（Slice 5：把 pre-route 端口 side 的全部上下文收敛到求解器）。
///
/// v2 模式下求解器成为 pre-route 端口 side 的唯一写者：feedback / monitor 侧向
/// 逃逸作为锁定硬约束，fanin 对齐作为求解后覆写，反向 stub 预测并入目标函数。
pub struct PortSolverInput<'a> {
    pub relations: &'a [Relation],
    pub nodes: &'a HashMap<String, NodeLayout>,
    pub group_ctx: &'a GroupRoutingContext,
    pub feedback: &'a FeedbackSideAssignment,
    /// S4.x：监控边同排侧廊被堵时改正对端口。
    pub s4_monitor_corridor: bool,
    pub horizontal: bool,
    /// v2 反向 stub 惩罚开关（Slice B：从 RoutingConfig 注入）。
    pub port_solver_v2: bool,
}

/// 端口容量：同节点同侧最大边数
const PORT_CAPACITY_PER_SIDE: usize = 4;

/// 弯折预估权重
const BEND_WEIGHT: f64 = 1.0;

/// 端口冲突惩罚权重（二次：count² × CONFLICT_WEIGHT）
const CONFLICT_WEIGHT: f64 = 6.0;

/// 角度对齐惩罚权重：端口方向与对端方向偏差的惩罚
const ANGULAR_WEIGHT: f64 = 2.0;

/// 最小改善阈值：只有成本下降超过此值才接受端口变更（避免微小改善导致路由退化）
const MIN_IMPROVEMENT: f64 = 2.0;

/// 反向 stub 惩罚权重（v2）：端口 outward 指向对端反方向即判反向 stub。
/// 权重高于 CONFLICT_WEIGHT，使求解器优先在 pre-route 翻正端口，
/// 取代 post-route 的 `fix_reverse_stub_ports` 翻转（Slice 5 单一写者）。
const REVERSE_STUB_WEIGHT: f64 = 8.0;

/// 端口约束求解
///
/// 目标函数：最小化 总弯折预估 + 端口冲突惩罚 (+ v2: 反向 stub 惩罚)
///
/// v2（`PLOTGRAM_PORT_SOLVER_V2=1`）下求解器成为 pre-route 端口 side 的唯一写者：
/// feedback / monitor 逃逸作锁定硬约束、fanin 对齐作求解后覆写、反向 stub 并入目标。
pub fn solve_port_assignment(input: &PortSolverInput) -> PortAssignment {
    let PortSolverInput {
        relations,
        nodes,
        group_ctx,
        feedback,
        s4_monitor_corridor,
        horizontal,
        port_solver_v2,
    } = *input;
    let n = relations.len();
    // 反向 stub 惩罚（默认开）。
    let reverse_weight = if port_solver_v2 { REVERSE_STUB_WEIGHT } else { 0.0 };

    // 1. 初始化：使用几何规则
    let mut from_side = vec![Port::Bottom; n];
    let mut to_side = vec![Port::Top; n];

    for (i, rel) in relations.iter().enumerate() {
        // 回环边使用 feedback 分配
        if let Some(hint) = feedback.hints.get(&i) {
            from_side[i] = hint.from_side;
            to_side[i] = hint.to_side;
            continue;
        }

        let (Some(from_nl), Some(to_nl)) =
            (nodes.get(rel.from.as_str()), nodes.get(rel.to.as_str()))
        else {
            continue;
        };

        let (fs, ts) = choose_pair_sides_with_group(
            from_nl,
            to_nl,
            rel.from.as_str(),
            rel.to.as_str(),
            Some(group_ctx),
        );
        from_side[i] = fs;
        to_side[i] = ts;
    }

    // 锁定集合：feedback 回环边不参与局部搜索（与原 pipeline 一致）。
    let mut locked = vec![false; n];
    for i in 0..n {
        if feedback.hints.contains_key(&i) {
            locked[i] = true;
        }
    }

    // 2. 局部搜索优化（按规模分档；确定性：edge_index 升序 + 固定候选侧顺序）
    if n <= 50 {
        local_search_optimize(relations, nodes, &locked, reverse_weight, &mut from_side, &mut to_side);
    } else if n <= 120 {
        // 中等规模：仅对高度节点做局部优化
        local_search_high_degree(relations, nodes, &locked, reverse_weight, &mut from_side, &mut to_side);
    }

    // monitor 侧向逃逸：在局部搜索之后覆写（与原 pipeline 顺序一致，
    // 保证 flag-off 时行为与 Slice 4 逐字节等价）。
    if s4_monitor_corridor {
        super::feedback_side::apply_monitor_hub_escape_ports(
            relations,
            nodes,
            &mut from_side,
            &mut to_side,
            horizontal,
        );
    }

    // fanin 对齐作为求解后覆写（仅 to_side，与原 pipeline 语义一致）。
    apply_fanin_target_sides(relations, nodes, &mut to_side);

    PortAssignment { from_side, to_side }
}

/// 局部搜索优化：尝试交换端口对，改善目标函数
fn local_search_optimize(
    relations: &[Relation],
    nodes: &HashMap<String, NodeLayout>,
    locked: &[bool],
    reverse_weight: f64,
    from_side: &mut [Port],
    to_side: &mut [Port],
) {
    let n = relations.len();
    let max_iterations = 10;

    for _ in 0..max_iterations {
        let mut improved = false;

        for i in 0..n {
            // 跳过锁定边（feedback / monitor）
            if locked[i] {
                continue;
            }

            let (Some(_from_nl), Some(_to_nl)) = (
                nodes.get(relations[i].from.as_str()),
                nodes.get(relations[i].to.as_str()),
            ) else {
                continue;
            };

            let current_cost =
                edge_cost(i, relations, nodes, from_side, to_side, reverse_weight);

            // 尝试所有端口组合
            let ports = [Port::Top, Port::Bottom, Port::Left, Port::Right];
            let mut best_cost = current_cost;
            let mut best_from = from_side[i];
            let mut best_to = to_side[i];

            for &fs in &ports {
                for &ts in &ports {
                    // 跳过无效组合（同侧）
                    if fs == ts {
                        continue;
                    }

                    // 临时修改
                    let old_from = from_side[i];
                    let old_to = to_side[i];
                    from_side[i] = fs;
                    to_side[i] = ts;

                    let new_cost =
                        edge_cost(i, relations, nodes, from_side, to_side, reverse_weight);

                    if new_cost < best_cost - MIN_IMPROVEMENT {
                        best_cost = new_cost;
                        best_from = fs;
                        best_to = ts;
                    }

                    // 恢复
                    from_side[i] = old_from;
                    to_side[i] = old_to;
                }
            }

            // 应用最佳
            if best_from != from_side[i] || best_to != to_side[i] {
                from_side[i] = best_from;
                to_side[i] = best_to;
                improved = true;
            }
        }

        if !improved {
            break;
        }
    }
}

/// 中等规模优化：仅对高度节点（度 >= 5）相关的边做局部搜索
fn local_search_high_degree(
    relations: &[Relation],
    nodes: &HashMap<String, NodeLayout>,
    locked: &[bool],
    reverse_weight: f64,
    from_side: &mut [Port],
    to_side: &mut [Port],
) {
    let n = relations.len();

    // 统计每个节点的度
    let mut degree: HashMap<&str, usize> = HashMap::new();
    for rel in relations {
        *degree.entry(rel.from.as_str()).or_default() += 1;
        *degree.entry(rel.to.as_str()).or_default() += 1;
    }

    // 找出高度节点相关的边
    let high_degree_edges: Vec<usize> = (0..n)
        .filter(|&i| {
            if locked[i] {
                return false;
            }
            let from_deg = degree.get(relations[i].from.as_str()).copied().unwrap_or(0);
            let to_deg = degree.get(relations[i].to.as_str()).copied().unwrap_or(0);
            from_deg >= 5 || to_deg >= 5
        })
        .collect();

    if high_degree_edges.is_empty() {
        return;
    }

    let ports = [Port::Top, Port::Bottom, Port::Left, Port::Right];
    for _ in 0..5 {
        let mut improved = false;
        for &i in &high_degree_edges {
            let current_cost = edge_cost(i, relations, nodes, from_side, to_side, reverse_weight);
            let mut best_cost = current_cost;
            let mut best_from = from_side[i];
            let mut best_to = to_side[i];

            for &fs in &ports {
                for &ts in &ports {
                    if fs == ts {
                        continue;
                    }
                    let old_from = from_side[i];
                    let old_to = to_side[i];
                    from_side[i] = fs;
                    to_side[i] = ts;
                    let new_cost = edge_cost(i, relations, nodes, from_side, to_side, reverse_weight);
                    if new_cost < best_cost - MIN_IMPROVEMENT {
                        best_cost = new_cost;
                        best_from = fs;
                        best_to = ts;
                    }
                    from_side[i] = old_from;
                    to_side[i] = old_to;
                }
            }

            if best_from != from_side[i] || best_to != to_side[i] {
                from_side[i] = best_from;
                to_side[i] = best_to;
                improved = true;
            }
        }
        if !improved {
            break;
        }
    }
}

/// 计算单条边的成本（弯折预估 + 端口冲突 + 角度对齐 + v2：反向 stub）
fn edge_cost(
    edge_idx: usize,
    relations: &[Relation],
    nodes: &HashMap<String, NodeLayout>,
    from_side: &[Port],
    to_side: &[Port],
    reverse_weight: f64,
) -> f64 {
    let rel = &relations[edge_idx];
    let (Some(from_nl), Some(to_nl)) = (
        nodes.get(rel.from.as_str()),
        nodes.get(rel.to.as_str()),
    ) else {
        return f64::INFINITY;
    };

    let from_center = node_center(from_nl);
    let to_center = node_center(to_nl);
    let dx = to_center.x - from_center.x;
    let dy = to_center.y - from_center.y;

    // 弯折预估：正对端口 = 0 弯折，L 形 = 1 弯折，Z 形 = 2 弯折
    let fs = from_side[edge_idx];
    let ts = to_side[edge_idx];
    let bend_estimate = estimate_bends(fs, ts, dx, dy);

    // 端口冲突：同节点同侧边数（二次惩罚）
    let from_conflict = count_port_conflicts(
        rel.from.as_str(),
        fs,
        edge_idx,
        relations,
        from_side,
    );
    let to_conflict = count_port_conflicts(
        rel.to.as_str(),
        ts,
        edge_idx,
        relations,
        to_side,
    );

    // 角度对齐惩罚：端口方向与对端方向的偏差
    let angular_from = angular_misalignment(fs, dx, dy);
    let angular_to = angular_misalignment(ts, -dx, -dy);

    // v2：反向 stub 惩罚（outward 指向对端反方向）；reverse_weight=0 时无影响（v1）。
    let reverse_penalty = if reverse_weight > 0.0 {
        reverse_stub_penalty(fs, from_center, to_center)
            + reverse_stub_penalty(ts, to_center, from_center)
    } else {
        0.0
    };

    bend_estimate * BEND_WEIGHT
        + (from_conflict + to_conflict) as f64 * CONFLICT_WEIGHT
        + (angular_from + angular_to) * ANGULAR_WEIGHT
        + reverse_penalty * reverse_weight
}

/// 反向 stub 预测（pre-route 几何判据）：端口 outward 指向对端节点的反方向
/// 即判定为反向 stub。返回 0.0（对齐）到 1.0（完全反向）。
///
/// 与已下线的 post-route `fix_reverse_stub_ports` 翻转同口径：outward 与
/// 「自→对端」方向点积为负时，路径必须反向折返才能接入端口 → 反向 stub。
/// Slice 5 将该判据前移到 pre-route，取代 post-route 翻转。
fn reverse_stub_penalty(side: Port, self_center: Point, other_center: Point) -> f64 {
    let (ox, oy) = port_outward(side);
    let dx = other_center.x - self_center.x;
    let dy = other_center.y - self_center.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < EPS {
        return 0.0;
    }
    let proj = (ox * dx + oy * dy) / len;
    if proj < 0.0 {
        -proj
    } else {
        0.0
    }
}

/// fanin 对齐覆写（v2）：同宿 FanIn 若全部源节点位于目标同一侧，
/// 统一使用目标正对端口（复用 `aligned_fanin_target_port` 几何判据）。
/// 确定性：目标节点按 id 升序（BTreeMap）遍历。
fn apply_fanin_target_sides(
    relations: &[Relation],
    nodes: &HashMap<String, NodeLayout>,
    to_side: &mut [Port],
) {
    let mut by_target: std::collections::BTreeMap<&str, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (edge_index, relation) in relations.iter().enumerate() {
        by_target
            .entry(relation.to.as_str())
            .or_default()
            .push(edge_index);
    }
    for (target_id, members) in by_target {
        if let Some(common) =
            super::phases::aligned_fanin_target_port(target_id, &members, relations, nodes)
        {
            for edge_index in members {
                if let Some(side) = to_side.get_mut(edge_index) {
                    *side = common;
                }
            }
        }
    }
}

/// 预估弯折数
fn estimate_bends(from: Port, to: Port, dx: f64, dy: f64) -> f64 {
    // 正对端口：Bottom->Top 或 Top->Bottom（垂直），Left->Right 或 Right->Left（水平）
    let is_opposite_vertical = (from == Port::Bottom && to == Port::Top)
        || (from == Port::Top && to == Port::Bottom);
    let is_opposite_horizontal = (from == Port::Left && to == Port::Right)
        || (from == Port::Right && to == Port::Left);

    if is_opposite_vertical || is_opposite_horizontal {
        // 正对：检查是否对齐
        if is_opposite_vertical && dx.abs() < EPS {
            return 0.0; // 直线
        }
        if is_opposite_horizontal && dy.abs() < EPS {
            return 0.0; // 直线
        }
        return 1.0; // 正对但错位：1 弯折
    }

    // L 形端口对：1 弯折
    let is_vertical_from = from == Port::Top || from == Port::Bottom;
    let is_vertical_to = to == Port::Top || to == Port::Bottom;
    if is_vertical_from != is_vertical_to {
        return 1.0;
    }

    // 同向端口：2+ 弯折
    2.0
}

/// 计算端口冲突数（同节点同侧超出容量的边数，二次惩罚）
fn count_port_conflicts(
    node_id: &str,
    side: Port,
    exclude_idx: usize,
    relations: &[Relation],
    sides: &[Port],
) -> usize {
    let mut count: usize = 0;
    for (i, rel) in relations.iter().enumerate() {
        if i == exclude_idx {
            continue;
        }
        if rel.from.as_str() == node_id && sides[i] == side {
            count += 1;
        }
        if rel.to.as_str() == node_id && sides[i] == side {
            count += 1;
        }
    }
    // 二次惩罚：超出容量后按平方增长
    let overflow = count.saturating_sub(PORT_CAPACITY_PER_SIDE - 1);
    overflow * overflow
}

/// 角度对齐惩罚：端口方向与对端方向的偏差。
/// 返回 0.0（完美对齐）到 2.0（完全反向）。
fn angular_misalignment(port: Port, dx: f64, dy: f64) -> f64 {
    let len = (dx * dx + dy * dy).sqrt();
    if len < EPS {
        return 0.0;
    }
    // 端口理想方向向量
    let (px, py) = match port {
        Port::Top => (0.0, -1.0),
        Port::Bottom => (0.0, 1.0),
        Port::Left => (-1.0, 0.0),
        Port::Right => (1.0, 0.0),
    };
    // 对端方向单位向量
    let nx = dx / len;
    let ny = dy / len;
    // 点积：1.0 = 完美对齐，-1.0 = 完全反向
    let dot = px * nx + py * ny;
    // 转换为惩罚：0.0（对齐）到 2.0（反向）
    1.0 - dot
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::NodeLayout;

    fn node(x: f64, y: f64, w: f64, h: f64) -> NodeLayout {
        NodeLayout {
            x,
            y,
            width: w,
            height: h,
            ..Default::default()
        }
    }

    #[test]
    fn test_estimate_bends_opposite() {
        // 正对端口，对齐
        assert_eq!(estimate_bends(Port::Bottom, Port::Top, 0.0, 100.0), 0.0);
        assert_eq!(estimate_bends(Port::Left, Port::Right, 100.0, 0.0), 0.0);

        // 正对端口，错位
        assert_eq!(estimate_bends(Port::Bottom, Port::Top, 50.0, 100.0), 1.0);
    }

    #[test]
    fn test_estimate_bends_l_shaped() {
        // L 形端口对
        assert_eq!(estimate_bends(Port::Bottom, Port::Left, 50.0, 50.0), 1.0);
        assert_eq!(estimate_bends(Port::Right, Port::Top, 50.0, 50.0), 1.0);
    }

    #[test]
    fn test_estimate_bends_same_direction() {
        // 同向端口
        assert_eq!(estimate_bends(Port::Bottom, Port::Bottom, 0.0, 100.0), 2.0);
        assert_eq!(estimate_bends(Port::Left, Port::Left, 100.0, 0.0), 2.0);
    }

    #[test]
    fn test_port_solver_basic() {
        // 基础测试：两个节点，一条边，验证求解器产生合理端口分配
        use crate::ast::{ArrowType, AttributeMap, Identifier, Relation, Span};
        use crate::layout::group::GroupRoutingContext;
        use crate::layout::types::LayoutHints;
        use crate::layout::LayoutResult;
        use super::super::feedback_side::FeedbackSideAssignment;

        let mut nodes = HashMap::new();
        nodes.insert("a".to_string(), node(0.0, 0.0, 80.0, 40.0));
        nodes.insert("b".to_string(), node(0.0, 100.0, 80.0, 40.0));

        let relations = vec![Relation {
            from: Identifier::new_unchecked("a"),
            to: Identifier::new_unchecked("b"),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: Span::dummy(),
        }];

        let layout_result = LayoutResult {
            nodes: HashMap::new(),
            groups: crate::layout::GroupTable::new(),
            edges: vec![],
            total_width: 0.0,
            total_height: 0.0,
            hints: LayoutHints::default(),
        };
        let group_ctx = GroupRoutingContext::from_layout(
            &crate::ast::Diagram::default(),
            &layout_result,
            "orthogonal",
        );
        let feedback = FeedbackSideAssignment::default();

        let input = PortSolverInput {
            relations: &relations,
            nodes: &nodes,
            group_ctx: &group_ctx,
            feedback: &feedback,
            s4_monitor_corridor: false,
            horizontal: false,
            port_solver_v2: true,
        };
        let assignment = solve_port_assignment(&input);

        // a 在 b 上方，应该使用 Bottom->Top
        assert_eq!(assignment.from_side()[0], Port::Bottom);
        assert_eq!(assignment.to_side()[0], Port::Top);
    }

    #[test]
    fn test_reverse_stub_penalty_geometry() {
        // b 在 a 下方：Bottom 端口对齐（无惩罚），Top 端口反向（惩罚 ≈ 1.0）。
        let a = Point::new(40.0, 20.0);
        let b = Point::new(40.0, 120.0);
        assert!(reverse_stub_penalty(Port::Bottom, a, b) < EPS);
        assert!((reverse_stub_penalty(Port::Top, a, b) - 1.0).abs() < 1e-6);
        // 侧向（Left/Right）垂直于位移：无惩罚。
        assert!(reverse_stub_penalty(Port::Left, a, b) < EPS);
    }

    #[test]
    fn test_v2_avoids_reverse_stub() {
        // v2 开启时，求解器应避免为向下边选择 Top 出端（反向 stub）。
        use crate::ast::{ArrowType, AttributeMap, Identifier, Relation, Span};
        use crate::layout::group::GroupRoutingContext;
        use crate::layout::types::LayoutHints;
        use crate::layout::LayoutResult;
        use super::super::feedback_side::FeedbackSideAssignment;

        let mut nodes = HashMap::new();
        nodes.insert("a".to_string(), node(0.0, 0.0, 80.0, 40.0));
        nodes.insert("b".to_string(), node(0.0, 200.0, 80.0, 40.0));
        let relations = vec![Relation {
            from: Identifier::new_unchecked("a"),
            to: Identifier::new_unchecked("b"),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: Span::dummy(),
        }];
        let layout_result = LayoutResult {
            nodes: HashMap::new(),
            groups: crate::layout::GroupTable::new(),
            edges: vec![],
            total_width: 0.0,
            total_height: 0.0,
            hints: LayoutHints::default(),
        };
        let group_ctx = GroupRoutingContext::from_layout(
            &crate::ast::Diagram::default(),
            &layout_result,
            "orthogonal",
        );
        let feedback = FeedbackSideAssignment::default();
        let input = PortSolverInput {
            relations: &relations,
            nodes: &nodes,
            group_ctx: &group_ctx,
            feedback: &feedback,
            s4_monitor_corridor: false,
            horizontal: false,
            port_solver_v2: true,
        };

        // 确定性：两次求解结果一致（不依赖 env 切换时的随机性）。
        let r1 = solve_port_assignment(&input);
        let r2 = solve_port_assignment(&input);
        assert_eq!(r1.from_side, r2.from_side);
        assert_eq!(r1.to_side, r2.to_side);
        // 无论 v1/v2，向下边都应 Bottom->Top（无反向 stub）。
        assert_eq!(r1.from_side[0], Port::Bottom);
        assert_eq!(r1.to_side[0], Port::Top);
    }

    // 构造一条 relation 的测试辅助。
    fn rel(from: &str, to: &str) -> crate::ast::Relation {
        use crate::ast::{ArrowType, AttributeMap, Identifier, Relation, Span};
        Relation {
            from: Identifier::new_unchecked(from),
            to: Identifier::new_unchecked(to),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: Span::dummy(),
        }
    }

    fn empty_group_ctx() -> crate::layout::group::GroupRoutingContext {
        use crate::layout::types::LayoutHints;
        use crate::layout::LayoutResult;
        let layout_result = LayoutResult {
            nodes: HashMap::new(),
            groups: crate::layout::GroupTable::new(),
            edges: vec![],
            total_width: 0.0,
            total_height: 0.0,
            hints: LayoutHints::default(),
        };
        crate::layout::group::GroupRoutingContext::from_layout(
            &crate::ast::Diagram::default(),
            &layout_result,
            "orthogonal",
        )
    }

    #[test]
    fn test_feedback_hint_locked() {
        // feedback 硬约束：带 hint 的边（回环）side 被锁定，求解器不得改写。
        use super::super::feedback_side::{FeedbackSideAssignment, FeedbackSideHint};

        let mut nodes = HashMap::new();
        nodes.insert("a".to_string(), node(0.0, 0.0, 80.0, 40.0));
        nodes.insert("b".to_string(), node(0.0, 200.0, 80.0, 40.0));
        let relations = vec![rel("a", "b")];
        let group_ctx = empty_group_ctx();

        // hint 指定一个「非自然」的 Left->Right（几何上应为 Bottom->Top）。
        let mut feedback = FeedbackSideAssignment::default();
        feedback.hints.insert(
            0,
            FeedbackSideHint {
                from_side: Port::Left,
                to_side: Port::Right,
                lane: 0,
            },
        );
        let input = PortSolverInput {
            relations: &relations,
            nodes: &nodes,
            group_ctx: &group_ctx,
            feedback: &feedback,
            s4_monitor_corridor: false,
            horizontal: false,
            port_solver_v2: true,
        };
        let assignment = solve_port_assignment(&input);
        // 锁定：hint 的 side 原样保留，未被局部搜索翻正。
        assert_eq!(assignment.from_side()[0], Port::Left);
        assert_eq!(assignment.to_side()[0], Port::Right);
    }

    #[test]
    fn test_fanin_target_sides_aligned() {
        // fanin 同侧奖励：两条边汇入同一目标 c，且源节点同处一行、位于 c 上方，
        // 则两条边的 to_side 统一为目标的正对端口（Top）。
        use super::super::feedback_side::FeedbackSideAssignment;

        let mut nodes = HashMap::new();
        nodes.insert("a".to_string(), node(0.0, 0.0, 80.0, 40.0));
        nodes.insert("b".to_string(), node(200.0, 0.0, 80.0, 40.0));
        nodes.insert("c".to_string(), node(100.0, 200.0, 80.0, 40.0));
        let relations = vec![rel("a", "c"), rel("b", "c")];
        let group_ctx = empty_group_ctx();
        let feedback = FeedbackSideAssignment::default();
        let input = PortSolverInput {
            relations: &relations,
            nodes: &nodes,
            group_ctx: &group_ctx,
            feedback: &feedback,
            s4_monitor_corridor: false,
            horizontal: false,
            port_solver_v2: true,
        };
        let assignment = solve_port_assignment(&input);
        // 两条汇入边统一到 c 的 Top 端口（源在上方）。
        assert_eq!(assignment.to_side()[0], Port::Top);
        assert_eq!(assignment.to_side()[1], Port::Top);
    }

    #[test]
    fn test_reverse_stub_penalty_fires_in_cost() {
        // 反向 stub 预测命中：对 b 在 a 正下方的边，反向配置（from=Top, to=Bottom）
        // 在 v2 目标中比 v1 额外承担反向 stub 惩罚（两端各 ~1.0 × 权重）。
        let mut nodes = HashMap::new();
        nodes.insert("a".to_string(), node(0.0, 0.0, 80.0, 40.0));
        nodes.insert("b".to_string(), node(0.0, 200.0, 80.0, 40.0));
        let relations = vec![rel("a", "b")];

        // 反向配置：Top 出端 + Bottom 入端（两端都朝对端反方向）。
        let from_side = vec![Port::Top];
        let to_side = vec![Port::Bottom];

        let cost_v1 = edge_cost(0, &relations, &nodes, &from_side, &to_side, 0.0);
        let cost_v2 = edge_cost(0, &relations, &nodes, &from_side, &to_side, REVERSE_STUB_WEIGHT);
        // v2 的额外量 = 反向 stub 惩罚 × 权重 ≈ 2.0 × REVERSE_STUB_WEIGHT。
        let extra = cost_v2 - cost_v1;
        assert!(
            (extra - 2.0 * REVERSE_STUB_WEIGHT).abs() < 0.5,
            "反向 stub 惩罚未按预期计入 edge_cost: extra={extra}"
        );
        // 对齐配置（Bottom->Top）不应产生反向 stub 惩罚。
        let aligned_extra = edge_cost(0, &relations, &nodes, &[Port::Bottom], &[Port::Top], REVERSE_STUB_WEIGHT)
            - edge_cost(0, &relations, &nodes, &[Port::Bottom], &[Port::Top], 0.0);
        assert!(aligned_extra.abs() < EPS, "对齐配置不应有反向 stub 惩罚: {aligned_extra}");
    }
}
