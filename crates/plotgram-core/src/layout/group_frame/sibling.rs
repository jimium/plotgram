//! Sibling set 收集与重叠消解（安全网 pass）。
//!
//! 本文件从 `mod.rs` 拆分而来，仅做代码搬家，无行为变更。

use crate::ast::Diagram;
use crate::layout::{GroupLayout, LayoutResult, NodeLayout};
use std::collections::{HashMap, HashSet};

use super::spec::{Axis, GroupArrangement, GroupFrameSpec};

/// 仅消除各层 sibling group 之间的几何重叠（不重算 bounds、不做排列）。
///
/// 用于 EGB/PRS 等扩壳后恢复 `group_gap` 约束。
pub fn resolve_all_sibling_overlaps(
    spec: &GroupFrameSpec,
    diagram: &Diagram,
    layout: &mut LayoutResult,
) {
    let sibling_sets = collect_sibling_sets(diagram);
    for target_ids in sibling_sets {
        if target_ids.len() < 2 {
            continue;
        }
        let node_to_target = build_node_to_ancestor_in_set(diagram, &target_ids);
        let group_to_target = build_group_to_ancestor_in_set(diagram, &target_ids);
        resolve_sibling_overlaps(
            &target_ids,
            &mut layout.groups,
            &mut layout.nodes,
            &node_to_target,
            &group_to_target,
            &spec.arrangement,
            spec.gap,
        );
    }
}

/// 收集 sibling sets（同级 group 集合），自顶向下 BFS 顺序。
///
/// 返回顺序：第一个为顶层 group（`parent_id == None`），后续为各 parent 的直接子 group
/// 集合。父层先于子层，保证 sub-frame 递归时父框已就位。同一 parent 内按
/// `diagram.groups` 声明序（确定性）。
///
/// 利用 `Group::child_group_ids` 做 BFS：顶层入队后逐层展开子 group。
pub(super) fn collect_sibling_sets(diagram: &Diagram) -> Vec<Vec<String>> {
    use std::collections::VecDeque;

    let mut sets: Vec<Vec<String>> = Vec::new();
    let mut queue: VecDeque<String> = VecDeque::new();

    // 顶层（parent=None）— 声明序
    let top: Vec<String> = diagram
        .groups
        .iter()
        .filter(|g| g.parent_id.is_none())
        .map(|g| g.id.as_str().to_string())
        .collect();
    if !top.is_empty() {
        for id in &top {
            queue.push_back(id.clone());
        }
        sets.push(top);
    }

    // BFS：对每个出队的 parent，收集其直接子 group
    while let Some(parent_id) = queue.pop_front() {
        let parent = match diagram.find_group(&parent_id) {
            Some(g) => g,
            None => continue,
        };
        // child_group_ids 已是声明序（解析期填充）
        let children: Vec<String> = parent
            .child_group_ids
            .iter()
            .map(|c| c.as_str().to_string())
            .collect();
        if !children.is_empty() {
            for id in &children {
                queue.push_back(id.clone());
            }
            sets.push(children);
        }
    }

    sets
}

/// 节点 → 本层 target 的祖先映射。
///
/// 对每个节点，沿 `group_id` → `parent_id` 链向上找到第一个属于 `target_ids` 的 group。
/// 用于 sub-frame 平移节点时确定节点归属（target group 的所有后代节点随 target 平移）。
///
/// 确定性：按 `diagram.entities` 声明序构建。
pub(super) fn build_node_to_ancestor_in_set(
    diagram: &Diagram,
    target_ids: &[String],
) -> HashMap<String, String> {
    let target_set: HashSet<&str> = target_ids.iter().map(|s| s.as_str()).collect();
    let mut map = HashMap::new();
    for entity in &diagram.entities {
        let Some(start_gid) = entity.group_id.as_ref() else { continue; };
        let mut cur = start_gid.as_str().to_string();
        loop {
            if target_set.contains(cur.as_str()) {
                map.insert(entity.id.as_str().to_string(), cur);
                break;
            }
            let Some(g) = diagram.find_group(&cur) else { break; };
            match &g.parent_id {
                Some(p) => cur = p.as_str().to_string(),
                None => break,
            }
        }
    }
    map
}

/// group → 本层 target 的祖先映射。
///
/// 对每个 group，沿 `parent_id` 链向上找到第一个属于 `target_ids` 的 group（含自身）。
/// 用于 sub-frame 平移 target group 时同步平移其所有后代 group 框。
///
/// 确定性：按 `diagram.groups` 声明序构建。
pub(super) fn build_group_to_ancestor_in_set(
    diagram: &Diagram,
    target_ids: &[String],
) -> HashMap<String, String> {
    let target_set: HashSet<&str> = target_ids.iter().map(|s| s.as_str()).collect();
    let mut map = HashMap::new();
    for group in &diagram.groups {
        let mut cur = group.id.as_str().to_string();
        loop {
            if target_set.contains(cur.as_str()) {
                map.insert(group.id.as_str().to_string(), cur);
                break;
            }
            let Some(g) = diagram.find_group(&cur) else { break; };
            match &g.parent_id {
                Some(p) => cur = p.as_str().to_string(),
                None => break,
            }
        }
    }
    map
}

/// 消除同级 group 间的实际重叠（安全网 pass）。
///
/// # 背景
///
/// `recompute_group_bounds` 从实际节点位置重算 group 宽度，`apply_group_quantize_for`
/// 进一步 floor 左 / ceil 右（最多扩展 `2*step`）。这些后处理可能使 group 宽度超出
/// `position_macro_blocks` 放置时使用的 `intra.content_width + padding.x_delta`，
/// 导致同行（同 y band）的 group 在 x 方向重叠。
///
/// # 算法
///
/// - `Stack(Horizontal)`：按 x 排序，左→右扫描，若相邻 group 在 x 和 y 上均有重叠，
///   将后者右移 `prev_right + gap - curr_left`，同步平移组内节点与嵌套 group 框。
/// - `Stack(Vertical)`：按 y 排序，上→下扫描，若相邻 group 在 x 和 y 上均有重叠，
///   将后者下移 `prev_bottom + gap - curr_top`，同步平移。
/// - `Matrix`：不处理（二维排列的重叠应由 arrangement 本身保证）。
///
/// # 确定性
///
/// 排序使用 `partial_cmp` + group id 字典序 tie-breaker，不依赖 HashMap 迭代序。
pub(super) fn resolve_sibling_overlaps(
    target_ids: &[String],
    groups: &mut HashMap<String, GroupLayout>,
    nodes: &mut HashMap<String, NodeLayout>,
    node_to_target: &HashMap<String, String>,
    group_to_target: &HashMap<String, String>,
    arrangement: &GroupArrangement,
    gap: f64,
) {
    if target_ids.len() < 2 {
        return;
    }

    match arrangement {
        GroupArrangement::Matrix { .. } => return,
        GroupArrangement::Stack { axis } => {
            match axis {
                Axis::Horizontal => {
                    // Architecture 布局：同行 group 沿 x 并排，不同行沿 y 堆叠。
                    // 两步消除重叠：
                    // 1. 同行内按 x 排序消除 x 重叠
                    // 2. 行间按 y 排序消除 y 重叠

                    // 按 y 分行（容差 0.5px）
                    let mut rows: Vec<(f64, Vec<String>)> = Vec::new();
                    for id in target_ids {
                        let Some(g) = groups.get(id) else { continue };
                        let row_idx = rows
                            .iter()
                            .position(|(row_y, _)| (row_y - g.y).abs() < 0.5);
                        match row_idx {
                            Some(idx) => rows[idx].1.push(id.clone()),
                            None => rows.push((g.y, vec![id.clone()])),
                        }
                    }
                    rows.sort_by(|a, b| {
                        a.0.partial_cmp(&b.0)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });

                    // 步骤 1：同行内消除 x 重叠
                    for (_, row_ids) in &rows {
                        if row_ids.len() < 2 {
                            continue;
                        }
                        let mut sorted: Vec<String> = row_ids.to_vec();
                        sorted.sort_by(|a, b| {
                            let xa = groups.get(a).map(|g| g.x).unwrap_or(0.0);
                            let xb = groups.get(b).map(|g| g.x).unwrap_or(0.0);
                            xa.partial_cmp(&xb)
                                .unwrap_or(std::cmp::Ordering::Equal)
                                .then_with(|| a.cmp(b))
                        });

                        for i in 1..sorted.len() {
                            let (prev_id, curr_id) =
                                (sorted[i - 1].clone(), sorted[i].clone());
                            let prev_right = groups
                                .get(&prev_id)
                                .map(|g| g.x + g.width)
                                .unwrap_or(0.0);
                            let curr_x =
                                groups.get(&curr_id).map(|g| g.x).unwrap_or(0.0);

                            if prev_right > curr_x + 0.5 {
                                let shift = prev_right + gap - curr_x;
                                if shift > 0.5 {
                                    shift_target_horizontally(
                                        &curr_id,
                                        shift,
                                        groups,
                                        nodes,
                                        node_to_target,
                                        group_to_target,
                                    );
                                }
                            }
                        }
                    }

                    // 步骤 2：行间消除 y 重叠
                    // 收集每行的 y 范围（min_y, max_y_bottom）
                    let mut row_bounds: Vec<(f64, f64)> = Vec::new();
                    for (row_y, row_ids) in &rows {
                        let min_y = *row_y;
                        let max_bottom = row_ids
                            .iter()
                            .filter_map(|id| groups.get(id).map(|g| g.y + g.height))
                            .fold(0.0_f64, f64::max);
                        row_bounds.push((min_y, max_bottom));
                    }

                    for i in 1..row_bounds.len() {
                        let prev_bottom = row_bounds[i - 1].1;
                        let curr_top = row_bounds[i].0;
                        if prev_bottom > curr_top + 0.5 {
                            let shift = prev_bottom + gap - curr_top;
                            if shift > 0.5 {
                                // 下推当前行所有 group
                                for id in &rows[i].1 {
                                    shift_target_vertically(
                                        id,
                                        shift,
                                        groups,
                                        nodes,
                                        node_to_target,
                                        group_to_target,
                                    );
                                }
                                // 更新行边界
                                row_bounds[i].0 += shift;
                                row_bounds[i].1 += shift;
                            }
                        }
                    }
                }
                Axis::Vertical => {
                    // 按 y 排序，消除 y 重叠（仅当 x 也有重叠时才处理）
                    let mut sorted: Vec<String> = target_ids.to_vec();
                    sorted.sort_by(|a, b| {
                        let ya = groups.get(a).map(|g| g.y).unwrap_or(0.0);
                        let yb = groups.get(b).map(|g| g.y).unwrap_or(0.0);
                        ya.partial_cmp(&yb)
                            .unwrap_or(std::cmp::Ordering::Equal)
                            .then_with(|| a.cmp(b))
                    });

                    for i in 1..sorted.len() {
                        let (prev_id, curr_id) = (sorted[i - 1].clone(), sorted[i].clone());
                        let (prev_x, prev_w, prev_y, prev_h) = match groups.get(&prev_id) {
                            Some(g) => (g.x, g.width, g.y, g.height),
                            None => continue,
                        };
                        let (curr_x, curr_w, curr_y) = match groups.get(&curr_id) {
                            Some(g) => (g.x, g.width, g.y),
                            None => continue,
                        };

                        // x 方向是否有重叠
                        let x_overlap =
                            (prev_x + prev_w).min(curr_x + curr_w) - prev_x.max(curr_x);
                        if x_overlap <= 1.0 {
                            continue;
                        }

                        // y 方向是否有重叠
                        let prev_bottom = prev_y + prev_h;
                        if prev_bottom <= curr_y + 0.5 {
                            continue;
                        }

                        let shift = prev_bottom + gap - curr_y;
                        if shift < 0.5 {
                            continue;
                        }

                        shift_target_vertically(
                            &curr_id,
                            shift,
                            groups,
                            nodes,
                            node_to_target,
                            group_to_target,
                        );
                    }
                }
            }
        }
    }
}

/// 将 target group 及其所有后代节点 / 嵌套 group 框水平平移 `dx`。
fn shift_target_horizontally(
    target_id: &str,
    dx: f64,
    groups: &mut HashMap<String, GroupLayout>,
    nodes: &mut HashMap<String, NodeLayout>,
    node_to_target: &HashMap<String, String>,
    group_to_target: &HashMap<String, String>,
) {
    // 平移 target group 自身
    if let Some(g) = groups.get_mut(target_id) {
        g.x += dx;
    }

    // 平移组内节点
    for (node_id, nl) in nodes.iter_mut() {
        if node_to_target.get(node_id).map(String::as_str) == Some(target_id) {
            nl.x += dx;
        }
    }

    // 平移嵌套 group 框（target 的所有后代 group，排除 target 自身）
    let mut nested_ids: Vec<String> = groups
        .keys()
        .filter(|gid| {
            gid.as_str() != target_id
                && group_to_target.get(*gid).map(String::as_str) == Some(target_id)
        })
        .cloned()
        .collect();
    nested_ids.sort();
    for gid in nested_ids {
        if let Some(g) = groups.get_mut(&gid) {
            g.x += dx;
        }
    }
}

/// 将 target group 及其所有后代节点 / 嵌套 group 框垂直平移 `dy`。
fn shift_target_vertically(
    target_id: &str,
    dy: f64,
    groups: &mut HashMap<String, GroupLayout>,
    nodes: &mut HashMap<String, NodeLayout>,
    node_to_target: &HashMap<String, String>,
    group_to_target: &HashMap<String, String>,
) {
    if let Some(g) = groups.get_mut(target_id) {
        g.y += dy;
    }

    for (node_id, nl) in nodes.iter_mut() {
        if node_to_target.get(node_id).map(String::as_str) == Some(target_id) {
            nl.y += dy;
        }
    }

    let mut nested_ids: Vec<String> = groups
        .keys()
        .filter(|gid| {
            gid.as_str() != target_id
                && group_to_target.get(*gid).map(String::as_str) == Some(target_id)
        })
        .cloned()
        .collect();
    nested_ids.sort();
    for gid in nested_ids {
        if let Some(g) = groups.get_mut(&gid) {
            g.y += dy;
        }
    }
}
