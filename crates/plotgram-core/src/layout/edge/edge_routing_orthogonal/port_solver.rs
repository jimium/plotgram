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
use crate::layout::edge::common::edge_geometry::node_center;
use crate::layout::geometry::EPS;
use crate::layout::group::GroupRoutingContext;
use crate::layout::{NodeLayout, Port};
use std::collections::HashMap;

use super::feedback_side::FeedbackSideAssignment;
use super::slot::choose_pair_sides_with_group;

/// 端口分配结果
#[derive(Clone, Debug)]
pub struct PortAssignment {
    pub from_side: Vec<Port>,
    pub to_side: Vec<Port>,
}

/// 端口容量：同节点同侧最大边数
const PORT_CAPACITY_PER_SIDE: usize = 4;

/// 弯折预估权重
const BEND_WEIGHT: f64 = 1.0;

/// 端口冲突惩罚权重（二次：count² × CONFLICT_WEIGHT）
const CONFLICT_WEIGHT: f64 = 6.0;

/// 角度对齐惩罚权重：端口方向与对端方向偏差的惩罚
const ANGULAR_WEIGHT: f64 = 2.0;

/// 端口约束求解
///
/// 目标函数：最小化 总弯折预估 + 端口冲突惩罚
pub fn solve_port_assignment(
    relations: &[Relation],
    nodes: &HashMap<String, NodeLayout>,
    group_ctx: &GroupRoutingContext,
    feedback: &FeedbackSideAssignment,
) -> PortAssignment {
    let n = relations.len();

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

    // 2. 小规模优化：局部搜索（高度节点增加迭代）
    if n <= 50 {
        local_search_optimize(
            relations,
            nodes,
            group_ctx,
            feedback,
            &mut from_side,
            &mut to_side,
        );
    } else if n <= 120 {
        // 中等规模：仅对高度节点做局部优化
        local_search_high_degree(
            relations,
            nodes,
            feedback,
            &mut from_side,
            &mut to_side,
        );
    }

    PortAssignment { from_side, to_side }
}

/// 局部搜索优化：尝试交换端口对，改善目标函数
fn local_search_optimize(
    relations: &[Relation],
    nodes: &HashMap<String, NodeLayout>,
    group_ctx: &GroupRoutingContext,
    feedback: &FeedbackSideAssignment,
    from_side: &mut [Port],
    to_side: &mut [Port],
) {
    let n = relations.len();
    let max_iterations = 10;

    for _ in 0..max_iterations {
        let mut improved = false;

        for i in 0..n {
            // 跳过回环边
            if feedback.hints.contains_key(&i) {
                continue;
            }

            let (Some(from_nl), Some(to_nl)) = (
                nodes.get(relations[i].from.as_str()),
                nodes.get(relations[i].to.as_str()),
            ) else {
                continue;
            };

            let current_cost =
                edge_cost(i, relations, nodes, from_side, to_side);

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
                        edge_cost(i, relations, nodes, from_side, to_side);

                    if new_cost < best_cost - EPS {
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
    feedback: &FeedbackSideAssignment,
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
            if feedback.hints.contains_key(&i) {
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
            let current_cost = edge_cost(i, relations, nodes, from_side, to_side);
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
                    let new_cost = edge_cost(i, relations, nodes, from_side, to_side);
                    if new_cost < best_cost - EPS {
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

/// 计算单条边的成本（弯折预估 + 端口冲突 + 角度对齐）
fn edge_cost(
    edge_idx: usize,
    relations: &[Relation],
    nodes: &HashMap<String, NodeLayout>,
    from_side: &[Port],
    to_side: &[Port],
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

    bend_estimate * BEND_WEIGHT
        + (from_conflict + to_conflict) as f64 * CONFLICT_WEIGHT
        + (angular_from + angular_to) * ANGULAR_WEIGHT
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

/// 检查端口求解器是否启用
pub fn port_solver_enabled() -> bool {
    std::env::var("PLOTGRAM_PORT_SOLVER")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
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
    fn test_port_solver_disabled_by_default() {
        assert!(!port_solver_enabled());
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
            groups: HashMap::new(),
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

        let assignment = solve_port_assignment(&relations, &nodes, &group_ctx, &feedback);

        // a 在 b 上方，应该使用 Bottom->Top
        assert_eq!(assignment.from_side[0], Port::Bottom);
        assert_eq!(assignment.to_side[0], Port::Top);
    }
}
