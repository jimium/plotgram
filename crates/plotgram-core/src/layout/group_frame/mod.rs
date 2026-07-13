//! Group Frame — 组间宏观几何统一层（L1）。
//!
//! 本模块定义组间「排列 + 尺寸 + 对齐 + 间距 + 量化」的统一规格 [`GroupFrameSpec`]，
//! 并提供从 diagram 属性解析 Spec 的 [`resolve_group_frame_spec`]。
//!
//! ## 三层 Frame 模型
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────┐
//! │  L1  Group Frame（组间）   — 顶层/同级 group 的 track 几何  │  ← 本模块
//! ├──────────────────────────────────────────────────────────┤
//! │  L2  Intra Frame（组内）   — 单 group 内节点的排列模式      │  group_layout_hint
//! ├──────────────────────────────────────────────────────────┤
//! │  L3  Node Frame（节点）   — rank/layer 对齐 + 像素量化     │  grid_snap
//! └──────────────────────────────────────────────────────────┘
//! ```
//!
//! P0 阶段仅提供类型与解析，不调用布局算法。`apply_group_frame` 整形 Pass 见 P1。
//!
//! 详见 `docs/architecture/布局优化/group-frame-spec.md`（v0.2）。

use crate::ast::Diagram;
use crate::layout::grid_snap::{snap_floor, snap_ceil};
use crate::layout::node::common::group_bounds::GroupPadding;
use crate::layout::{GroupLayout, LayoutResult, NodeLayout};
use std::collections::HashMap;

mod realign;
mod pass;
mod spec;
mod sibling;
mod bounds;
mod padding;

pub use pass::{group_padding_from_plan, GroupFramePass};
pub use realign::realign_group_rows;

pub use spec::*;
pub use sibling::resolve_all_sibling_overlaps;
pub use bounds::{
    expand_groups_to_contain_contents, shrink_groups_to_required_padding,
    expand_container_groups_to_fit_children, recompute_group_bounds,
};
pub use padding::group_padding_for_algo;

use sibling::{
    collect_sibling_sets, build_node_to_ancestor_in_set,
    build_group_to_ancestor_in_set, resolve_sibling_overlaps,
};

// ─── L1 整形 Pass（P1）─────────────────────────────────────

/// `apply_group_frame` 执行报告。
#[derive(Debug, Clone, Default)]
pub struct GroupFrameReport {
    /// 参与整形的顶层 group 数量
    pub top_group_count: usize,
    /// 处理的嵌套 sibling set 数量（不含顶层；sub-frame 递归层数，见 spec §3.1/§8 P3）
    pub nested_frames_applied: usize,
    /// 是否执行了 Matrix 二维排列
    pub matrix_applied: bool,
    /// 是否执行了 Equal 拉齐
    pub equalized: bool,
    /// 是否执行了 cross_align Start 左缘对齐
    pub cross_aligned: bool,
    /// border_align 共线处理的边框数
    pub borders_aligned: usize,
    /// quantize 量化的 group 数
    pub groups_quantized: usize,
}

/// L1 Group Frame 整形 Pass。
///
/// 按 [`GroupFrameSpec`] 对 **同级 group 集合**（sibling set）施加：arrangement →
/// border_align → quantize。节点联动分级（见 spec §2.1/§3.1）：
/// - `track_sizing` / `cross_align` / `Matrix` 排列 → **必须**同步平移组内节点
/// - `border_align` / `quantize` 微调（≤1 step）→ **只改 `GroupLayout`**，不动节点
///
/// # 嵌套 sub-frame（spec §3.1/§8 P3）
///
/// 按 parent 分层，自顶向下逐层应用：先整形顶层 group（parent=None），再对每个含子 group
/// 的 parent 递归整形其直接子 group（sub-frame）。父层先于子层执行，保证父框已落到最终
/// 位置后，子 group 在父框内重新排列。同一 `GroupFrameSpec` 应用于所有层级。
///
/// # 确定性
///
/// 所有迭代使用排序后的 `Vec`（按 group.id 字典序），不依赖 `HashMap` 迭代序
/// （见 `AGENTS.md` §2）。sibling set 收集顺序：BFS（顶层 → 各 parent 的直接子 group），
/// 同一 parent 内按 `diagram.groups` 声明序。
///
/// # 幂等性
///
/// 同一 Spec 连续执行两次结果不变（见 spec §5.3）。
pub fn apply_group_frame(
    spec: &GroupFrameSpec,
    diagram: &Diagram,
    layout: &mut LayoutResult,
) -> GroupFrameReport {
    // 自顶向下收集 sibling sets：第一个为顶层（parent=None），后续为各 parent 的直接子 group
    let sibling_sets = collect_sibling_sets(diagram);
    let mut report = GroupFrameReport {
        top_group_count: sibling_sets.first().map(|s| s.len()).unwrap_or(0),
        ..Default::default()
    };

    for (idx, target_ids) in sibling_sets.iter().enumerate() {
        if target_ids.is_empty() {
            continue;
        }
        if idx > 0 {
            report.nested_frames_applied += 1;
        }

        // 节点 / group → 本层 target 的祖先映射（确定性：按声明序构建）
        let node_to_target = build_node_to_ancestor_in_set(diagram, target_ids);
        let group_to_target = build_group_to_ancestor_in_set(diagram, target_ids);

        // 步骤 1：按 arrangement 排列
        // - Matrix：二维网格（含 track_sizing Equal 列宽/行高 + cross_align cell 内对齐）
        // - Stack：cross_align + track_sizing 分步整形
        match &spec.arrangement {
            GroupArrangement::Matrix { rows, cols } => {
                if apply_matrix_arrangement(
                    *rows,
                    *cols,
                    spec,
                    target_ids,
                    &mut layout.groups,
                    &mut layout.nodes,
                    &node_to_target,
                    &group_to_target,
                ) {
                    report.matrix_applied = true;
                }
            }
            GroupArrangement::Stack { .. } => {
                // 先等宽/等高，再做交叉轴对齐（Center 依赖等宽后的行宽）
                if matches!(spec.track_sizing, TrackSizing::Equal) {
                    if apply_equal_sizing(
                        target_ids,
                        &mut layout.groups,
                        &mut layout.nodes,
                        &node_to_target,
                        &group_to_target,
                        spec.gap,
                    ) {
                        report.equalized = true;
                    }
                }
                match spec.cross_align {
                    CrossAlign::Start => {
                        if apply_cross_align_start(
                            target_ids,
                            &mut layout.groups,
                            &mut layout.nodes,
                            &node_to_target,
                            &group_to_target,
                        ) {
                            report.cross_aligned = true;
                        }
                    }
                    // Center 仅在 Equal 后有意义：同宽条带再按行居中。
                    // Fit+Center 不做激进平移，避免破坏 SharedLines 近对齐。
                    CrossAlign::Center
                        if matches!(spec.track_sizing, TrackSizing::Equal) =>
                    {
                        if apply_cross_align_center(
                            target_ids,
                            &mut layout.groups,
                            &mut layout.nodes,
                            &node_to_target,
                            &group_to_target,
                        ) {
                            report.cross_aligned = true;
                        }
                    }
                    CrossAlign::Center | CrossAlign::End | CrossAlign::Stretch => {}
                }
            }
        }

        // 步骤 2：border_align SharedLines（只改框，不动节点；仅作用于本层 sibling set）
        if matches!(spec.border_align, BorderAlign::SharedLines) {
            report.borders_aligned +=
                apply_border_align_for(target_ids, &mut layout.groups, spec.quantize.step);
        }

        // 步骤 3：quantize groups（只改框，不动节点；仅作用于本层 sibling set）
        if spec.quantize.enabled && spec.quantize.quantize_groups {
            report.groups_quantized +=
                apply_group_quantize_for(target_ids, &mut layout.groups, spec.quantize.step);
            // quantize 后可能破坏边框共线，再跑一次 border_align
            if matches!(spec.border_align, BorderAlign::SharedLines) {
                report.borders_aligned +=
                    apply_border_align_for(target_ids, &mut layout.groups, spec.quantize.step);
            }
        }

        // 步骤 4：resolve sibling overlaps（安全网）
        // recompute_group_bounds + quantize 可能使 group 宽度超出原始 macro block 宽度
        // （quantize floor 左 / ceil 右，最多扩展 2*step），导致同行 group 重叠。
        // 此 pass 检测并消除 sibling 间的实际重叠，同步平移组内节点与嵌套 group 框。
        resolve_sibling_overlaps(
            target_ids,
            &mut layout.groups,
            &mut layout.nodes,
            &node_to_target,
            &group_to_target,
            &spec.arrangement,
            spec.gap,
        );
    }

    report
}

/// cross_align Start：所有顶层 group 左缘对齐到全局 `min(x)`，同步平移组内节点与嵌套 group 框。
///
/// **按行（y）分组对齐**：同一 y（同一 macro rank）的 group 已被 `position_macro_blocks`
/// 水平并排放置，不能强制左缘对齐到同一 x（否则同 rank group 会完全重叠）。
/// 因此按 y 将顶层 group 分行，每行整体平移使该行 `min(x)` 等于全局 `min(x)`，
/// 行内 group 保持相对 x 位置不变。不同 y 的行各自独立平移到全局 `min(x)`，
/// 形成跨 rank 的整齐左缘。
///
/// **节点联动分级**（spec §2.1/§3.1）：cross_align 属于「必须同步平移节点」级别，
/// 因此嵌套 group 框也必须同步平移，否则嵌套 group 不再包含其成员节点。
///
/// 返回 `true` 表示执行了平移。
fn apply_cross_align_start(
    top_ids: &[String],
    groups: &mut HashMap<String, GroupLayout>,
    nodes: &mut HashMap<String, NodeLayout>,
    node_to_top: &HashMap<String, String>,
    group_to_top: &HashMap<String, String>,
) -> bool {
    if top_ids.len() < 2 {
        return false;
    }

    // 全局 min(x)：所有行的对齐目标
    let target_left = top_ids
        .iter()
        .filter_map(|id| groups.get(id).map(|g| g.x))
        .fold(f64::INFINITY, f64::min);
    if !target_left.is_finite() {
        return false;
    }

    // 按 y 分行（同一 macro rank 的 group 在同一行）。
    // 确定性：先按 top_ids 声明序收集，再按 y 排序处理。
    let mut rows: Vec<(f64, Vec<String>)> = Vec::new();
    for top_id in top_ids {
        if let Some(g) = groups.get(top_id) {
            // 容差 0.5px：同一 rank 的 group y 相同
            let row_idx = rows.iter().position(|(row_y, _)| (row_y - g.y).abs() < 0.5);
            match row_idx {
                Some(idx) => rows[idx].1.push(top_id.clone()),
                None => rows.push((g.y, vec![top_id.clone()])),
            }
        }
    }
    rows.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut shifted = false;
    for (_, row_ids) in &rows {
        // 该行 min(x)
        let row_min_x = row_ids
            .iter()
            .filter_map(|id| groups.get(id).map(|g| g.x))
            .fold(f64::INFINITY, f64::min);
        if !row_min_x.is_finite() {
            continue;
        }

        let shift = target_left - row_min_x;
        if shift.abs() < 0.5 {
            continue;
        }

        // 确定性：按 row_ids 顺序（diagram.groups 声明序）
        for top_id in row_ids {
            // 平移顶层 group 框
            if let Some(g) = groups.get_mut(top_id) {
                g.x += shift;
            }

            // 平移组内节点
            for (node_id, nl) in nodes.iter_mut() {
                if node_to_top.get(node_id) == Some(top_id) {
                    nl.x += shift;
                }
            }

            // 平移嵌套 group 框（top_id 的所有后代 group，排除 top_id 自身）
            // 确定性：按 group id 字典序
            let mut nested_ids: Vec<String> = groups
                .keys()
                .filter(|gid| {
                    gid.as_str() != top_id.as_str()
                        && group_to_top.get(*gid).map(String::as_str) == Some(top_id.as_str())
                })
                .cloned()
                .collect();
            nested_ids.sort();
            for gid in nested_ids {
                if let Some(g) = groups.get_mut(&gid) {
                    g.x += shift;
                }
            }
        }
        shifted = true;
    }

    shifted
}

/// track_sizing Equal：同级 sibling 拉齐宽度；同 RankBand（同 y 行）再拉齐高度并按固定 gap 重排。
///
/// - **全 sibling set 等宽**：垂直条带（不同 rank）也同宽，对应 `group_sizing: uniform` 观感。
/// - **同 band 等高 + 固定 gap**：水平并排时消除右缘锯齿与不等间距。
/// - 组内内容相对新框居中（平移节点与嵌套 group）。
///
/// 返回 `true` 表示执行了拉齐。
fn apply_equal_sizing(
    top_ids: &[String],
    groups: &mut HashMap<String, GroupLayout>,
    nodes: &mut HashMap<String, NodeLayout>,
    node_to_top: &HashMap<String, String>,
    group_to_top: &HashMap<String, String>,
    gap: f64,
) -> bool {
    if top_ids.is_empty() {
        return false;
    }

    let max_width = top_ids
        .iter()
        .filter_map(|id| groups.get(id).map(|g| g.width))
        .fold(0.0_f64, f64::max);
    if max_width <= f64::EPSILON {
        return false;
    }

    // 垂直条带（每行仅 1 个 group）共用左缘，保证等宽后左缘共线
    let align_left = top_ids
        .iter()
        .filter_map(|id| groups.get(id).map(|g| g.x))
        .fold(f64::INFINITY, f64::min);

    let mut rows: Vec<(f64, Vec<String>)> = Vec::new();
    for top_id in top_ids {
        if let Some(g) = groups.get(top_id) {
            let row_idx = rows
                .iter()
                .position(|(row_y, _)| (row_y - g.y).abs() < 0.5);
            match row_idx {
                Some(idx) => rows[idx].1.push(top_id.clone()),
                None => rows.push((g.y, vec![top_id.clone()])),
            }
        }
    }
    rows.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let gap = gap.max(0.0);
    let mut equalized = false;

    for (_, row_ids) in &rows {
        if row_ids.is_empty() {
            continue;
        }

        let max_height = row_ids
            .iter()
            .filter_map(|id| groups.get(id).map(|g| g.height))
            .fold(0.0_f64, f64::max);

        let mut ordered = row_ids.clone();
        ordered.sort_by(|a, b| {
            let xa = groups.get(a).map(|g| g.x).unwrap_or(0.0);
            let xb = groups.get(b).map(|g| g.x).unwrap_or(0.0);
            xa.partial_cmp(&xb)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.cmp(b))
        });

        let origin_x = ordered
            .iter()
            .filter_map(|id| groups.get(id).map(|g| g.x))
            .fold(f64::INFINITY, f64::min);
        if !origin_x.is_finite() {
            continue;
        }

        // Iteration 2：保留 two_phase lane_budget 已写入的间距（取 max(spec.gap, 现有相邻 gap)）
        let mut effective_gap = gap;
        if ordered.len() >= 2 {
            for w in ordered.windows(2) {
                if let (Some(a), Some(b)) = (groups.get(&w[0]), groups.get(&w[1])) {
                    let existing = b.x - (a.x + a.width);
                    if existing.is_finite() && existing > effective_gap {
                        effective_gap = existing;
                    }
                }
            }
        }

        let mut x_cursor = origin_x;
        for (pos, top_id) in ordered.iter().enumerate() {
            let Some(old) = groups.get(top_id).cloned() else {
                continue;
            };
            let target_x = if ordered.len() == 1 {
                // 单列条带：对齐到 sibling set 左缘，再拉宽（内容居中）
                if align_left.is_finite() {
                    align_left
                } else {
                    old.x
                }
            } else {
                x_cursor
            };
            let target_y = old.y - (max_height - old.height).max(0.0) / 2.0;
            let dx = target_x - old.x;
            let dy = target_y - old.y;
            let extra_w = max_width - old.width;
            let extra_h = max_height - old.height;

            if dx.abs() > 0.5
                || dy.abs() > 0.5
                || extra_w > f64::EPSILON
                || extra_h > f64::EPSILON
            {
                equalized = true;
            }

            if let Some(g) = groups.get_mut(top_id) {
                g.x = target_x;
                g.y = target_y;
                g.width = max_width;
                if max_height > f64::EPSILON {
                    g.height = max_height;
                }
            }

            let content_dx = dx + extra_w / 2.0;
            let content_dy = dy + extra_h.max(0.0) / 2.0;
            if content_dx.abs() > f64::EPSILON || content_dy.abs() > f64::EPSILON {
                for (node_id, nl) in nodes.iter_mut() {
                    if node_to_top.get(node_id) != Some(top_id) {
                        continue;
                    }
                    if content_dx.abs() > f64::EPSILON {
                        nl.x += content_dx;
                    }
                    if content_dy.abs() > f64::EPSILON {
                        nl.y += content_dy;
                    }
                }
                let mut nested_ids: Vec<String> = groups
                    .keys()
                    .filter(|gid| {
                        gid.as_str() != top_id.as_str()
                            && group_to_top.get(*gid).map(String::as_str) == Some(top_id.as_str())
                    })
                    .cloned()
                    .collect();
                nested_ids.sort();
                for gid in nested_ids {
                    if let Some(g) = groups.get_mut(&gid) {
                        g.x += content_dx;
                        g.y += content_dy;
                    }
                }
            }

            if ordered.len() > 1 {
                x_cursor += max_width;
                if pos + 1 < ordered.len() {
                    x_cursor += effective_gap;
                }
            }
        }
    }

    equalized
}

/// cross_align Center：各 RankBand（同 y 行）相对 sibling set 的包围盒水平居中。
///
/// 多行时以全部 sibling 的 `[min_x, max_right]` 为参考宽度；每行整体平移使行中心对齐。
fn apply_cross_align_center(
    top_ids: &[String],
    groups: &mut HashMap<String, GroupLayout>,
    nodes: &mut HashMap<String, NodeLayout>,
    node_to_top: &HashMap<String, String>,
    group_to_top: &HashMap<String, String>,
) -> bool {
    if top_ids.len() < 2 {
        return false;
    }

    let full_left = top_ids
        .iter()
        .filter_map(|id| groups.get(id).map(|g| g.x))
        .fold(f64::INFINITY, f64::min);
    let full_right = top_ids
        .iter()
        .filter_map(|id| groups.get(id).map(|g| g.x + g.width))
        .fold(f64::NEG_INFINITY, f64::max);
    if !full_left.is_finite() || !full_right.is_finite() {
        return false;
    }
    let full_width = full_right - full_left;
    if full_width <= f64::EPSILON {
        return false;
    }
    let full_center = full_left + full_width / 2.0;

    let mut rows: Vec<(f64, Vec<String>)> = Vec::new();
    for top_id in top_ids {
        if let Some(g) = groups.get(top_id) {
            let row_idx = rows
                .iter()
                .position(|(row_y, _)| (row_y - g.y).abs() < 0.5);
            match row_idx {
                Some(idx) => rows[idx].1.push(top_id.clone()),
                None => rows.push((g.y, vec![top_id.clone()])),
            }
        }
    }
    rows.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut shifted = false;
    for (_, row_ids) in &rows {
        let row_left = row_ids
            .iter()
            .filter_map(|id| groups.get(id).map(|g| g.x))
            .fold(f64::INFINITY, f64::min);
        let row_right = row_ids
            .iter()
            .filter_map(|id| groups.get(id).map(|g| g.x + g.width))
            .fold(f64::NEG_INFINITY, f64::max);
        if !row_left.is_finite() || !row_right.is_finite() {
            continue;
        }
        let row_center = (row_left + row_right) / 2.0;
        let shift = full_center - row_center;
        if shift.abs() < 0.5 {
            continue;
        }

        for top_id in row_ids {
            if let Some(g) = groups.get_mut(top_id) {
                g.x += shift;
            }
            for (node_id, nl) in nodes.iter_mut() {
                if node_to_top.get(node_id) == Some(top_id) {
                    nl.x += shift;
                }
            }
            let mut nested_ids: Vec<String> = groups
                .keys()
                .filter(|gid| {
                    gid.as_str() != top_id.as_str()
                        && group_to_top.get(*gid).map(String::as_str) == Some(top_id.as_str())
                })
                .cloned()
                .collect();
            nested_ids.sort();
            for gid in nested_ids {
                if let Some(g) = groups.get_mut(&gid) {
                    g.x += shift;
                }
            }
        }
        shifted = true;
    }

    shifted
}

/// Matrix 二维排列：将顶层 group 放入行优先网格，按 `track_sizing` 决定列宽/行高，
/// 按 `cross_align` 决定 cell 内对齐，按 `gap` 累加间距。
///
/// **节点联动分级**（spec §2.1/§3.1）：Matrix 排列属于「必须同步平移节点」级别，
/// 组内节点与嵌套 group 框随顶层 group 整体平移。
///
/// # 确定性
///
/// - group 顺序：按当前几何 `(y, x)` 排序（保持主布局产出的近似序），tie-break 用 id 字典序
/// - 行列推断：见 [`infer_matrix_dims`]
///
/// # 幂等性
///
/// 排列后 group 已落在网格格点；第二次执行时 `(y, x)` 序不变、列宽/行高不变、
/// cell 内偏移不变，故 `dx`/`dy` ≈ 0，为 no-op。
fn apply_matrix_arrangement(
    rows: Option<u32>,
    cols: Option<u32>,
    spec: &GroupFrameSpec,
    top_ids: &[String],
    groups: &mut HashMap<String, GroupLayout>,
    nodes: &mut HashMap<String, NodeLayout>,
    node_to_top: &HashMap<String, String>,
    group_to_top: &HashMap<String, String>,
) -> bool {
    let n = top_ids.len();
    if n == 0 {
        return false;
    }

    // 1. 推断行列
    let (n_rows, n_cols) = infer_matrix_dims(rows, cols, n);

    // 2. 按当前几何 (y, x) 排序（行优先），tie-break 用 id 字典序（确定性）
    let mut ordered: Vec<String> = top_ids.to_vec();
    ordered.sort_by(|a, b| {
        let ga = groups.get(a);
        let gb = groups.get(b);
        let ya = ga.map(|g| g.y).unwrap_or(0.0);
        let yb = gb.map(|g| g.y).unwrap_or(0.0);
        let xa = ga.map(|g| g.x).unwrap_or(0.0);
        let xb = gb.map(|g| g.x).unwrap_or(0.0);
        ya.partial_cmp(&yb)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| xa.partial_cmp(&xb).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.cmp(b))
    });

    // 3. 计算每列最大宽度、每行最大高度（Fit = 贴合内容）
    let mut col_widths = vec![0.0_f64; n_cols];
    let mut row_heights = vec![0.0_f64; n_rows];
    for (i, id) in ordered.iter().enumerate() {
        let r = i / n_cols;
        let c = i % n_cols;
        if let Some(g) = groups.get(id) {
            col_widths[c] = col_widths[c].max(g.width);
            row_heights[r] = row_heights[r].max(g.height);
        }
    }

    // track_sizing Equal：所有列等宽、所有行等高（取 max）
    if matches!(spec.track_sizing, TrackSizing::Equal) {
        let max_w = col_widths.iter().copied().fold(0.0_f64, f64::max).max(1.0);
        let max_h = row_heights.iter().copied().fold(0.0_f64, f64::max).max(1.0);
        col_widths.fill(max_w);
        row_heights.fill(max_h);
    }
    // track_sizing Fixed：固定列宽/行高
    if let TrackSizing::Fixed(v) = spec.track_sizing {
        let v = v.max(1.0);
        col_widths.fill(v);
        row_heights.fill(v);
    }

    // 4. 网格原点：所有 group 的最小 x、最小 y
    let origin_x = ordered
        .iter()
        .filter_map(|id| groups.get(id).map(|g| g.x))
        .fold(f64::INFINITY, f64::min);
    let origin_y = ordered
        .iter()
        .filter_map(|id| groups.get(id).map(|g| g.y))
        .fold(f64::INFINITY, f64::min);
    if !origin_x.is_finite() || !origin_y.is_finite() {
        return false;
    }

    // 5. 累加 gap 得到每列 x、每行 y
    let gap = spec.gap.max(0.0);
    let mut col_x = vec![origin_x; n_cols];
    let mut row_y = vec![origin_y; n_rows];
    for c in 1..n_cols {
        col_x[c] = col_x[c - 1] + col_widths[c - 1] + gap;
    }
    for r in 1..n_rows {
        row_y[r] = row_y[r - 1] + row_heights[r - 1] + gap;
    }

    // 6. 放置每个 group 到对应 cell，按 cross_align 决定 cell 内偏移
    let mut applied = false;
    for (i, id) in ordered.iter().enumerate() {
        let r = i / n_cols;
        let c = i % n_cols;
        let Some(g) = groups.get(id) else { continue; };

        let (new_x, new_y) = match spec.cross_align {
            CrossAlign::Start | CrossAlign::Stretch => (col_x[c], row_y[r]),
            CrossAlign::Center => (
                col_x[c] + (col_widths[c] - g.width) / 2.0,
                row_y[r] + (row_heights[r] - g.height) / 2.0,
            ),
            CrossAlign::End => (
                col_x[c] + (col_widths[c] - g.width),
                row_y[r] + (row_heights[r] - g.height),
            ),
        };

        let dx = new_x - g.x;
        let dy = new_y - g.y;
        if dx.abs() < 0.5 && dy.abs() < 0.5 {
            continue;
        }

        // 平移顶层 group 框
        if let Some(g) = groups.get_mut(id) {
            g.x = new_x;
            g.y = new_y;
        }

        // 平移组内节点
        for (node_id, nl) in nodes.iter_mut() {
            if node_to_top.get(node_id) == Some(id) {
                nl.x += dx;
                nl.y += dy;
            }
        }

        // 平移嵌套 group 框（确定性：按 group id 字典序）
        let mut nested_ids: Vec<String> = groups
            .keys()
            .filter(|gid| {
                gid.as_str() != id.as_str()
                    && group_to_top.get(*gid).map(String::as_str) == Some(id.as_str())
            })
            .cloned()
            .collect();
        nested_ids.sort();
        for gid in nested_ids {
            if let Some(g) = groups.get_mut(&gid) {
                g.x += dx;
                g.y += dy;
            }
        }
        applied = true;
    }

    applied
}

/// 推断 Matrix 行列数。
///
/// - 两者均指定：直接使用（可能产生空 cell）
/// - 仅指定 `rows`：`cols = ceil(n / rows)`
/// - 仅指定 `cols`：`rows = ceil(n / cols)`
/// - 均未指定：自动推断，`cols = ceil(sqrt(n))`，`rows = ceil(n / cols)`（接近正方形）
fn infer_matrix_dims(rows: Option<u32>, cols: Option<u32>, n: usize) -> (usize, usize) {
    match (rows, cols) {
        (Some(r), Some(c)) => ((r as usize).max(1), (c as usize).max(1)),
        (Some(r), None) => {
            let r = (r as usize).max(1);
            let c = ((n + r - 1) / r).max(1);
            (r, c)
        }
        (None, Some(c)) => {
            let c = (c as usize).max(1);
            let r = ((n + c - 1) / c).max(1);
            (r, c)
        }
        (None, None) => {
            let c = ((n as f64).sqrt().ceil() as usize).max(1);
            let r = ((n + c - 1) / c).max(1);
            (r, c)
        }
    }
}

/// border_align SharedLines：同侧边框共线（左/上），只改 `GroupLayout` 不动节点。
///
/// 仅作用于 `ids` 指定的 sibling set（同级 group 之间共线），不影响其他层级。
///
/// 对齐左边缘时同步调整 width（保持右边缘不变），对齐上边缘时同步调整 height
/// （保持下边缘不变）。聚类阈值 = `step`，簇内取中位数（确定性）。
///
/// 返回对齐的边框数。
fn apply_border_align_for(
    ids: &[String],
    groups: &mut HashMap<String, GroupLayout>,
    step: f64,
) -> usize {
    if ids.len() < 2 || step <= f64::EPSILON {
        return 0;
    }

    // 确定性：按 id 字典序
    let mut sorted: Vec<String> = ids.to_vec();
    sorted.sort();

    let left_count = align_border_set(groups, &sorted, step, |g| g.x, |g, v| {
        let delta = v - g.x;
        g.width -= delta;
        g.x = v;
    });
    let top_count = align_border_set(groups, &sorted, step, |g| g.y, |g, v| {
        let delta = v - g.y;
        g.height -= delta;
        g.y = v;
    });

    left_count + top_count
}

/// 对齐同一侧边框：检测在 1 个 step 内的边框，统一到中位数（确定性）。
///
/// 返回对齐的边框数。
fn align_border_set(
    groups: &mut HashMap<String, GroupLayout>,
    ids: &[String],
    step: f64,
    get: impl Fn(&GroupLayout) -> f64,
    set: impl Fn(&mut GroupLayout, f64),
) -> usize {
    // 收集 (id, value)，按 value 升序 + id 字典序 tie-break
    let mut entries: Vec<(String, f64)> = ids
        .iter()
        .map(|id| (id.clone(), get(&groups[id])))
        .collect();
    entries.sort_by(|a, b| {
        a.1.partial_cmp(&b.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    // 聚类：相邻值差 < step 归为同一组
    let mut clusters: Vec<Vec<(String, f64)>> = Vec::new();
    for entry in entries {
        if let Some(last_cluster) = clusters.last_mut() {
            let cluster_min = last_cluster[0].1;
            if (entry.1 - cluster_min).abs() < step {
                last_cluster.push(entry);
                continue;
            }
        }
        clusters.push(vec![entry]);
    }

    // 对多于 1 个元素的聚类，统一到中位数（确定性）
    let mut aligned = 0usize;
    for cluster in &clusters {
        if cluster.len() < 2 {
            continue;
        }
        let mut values: Vec<f64> = cluster.iter().map(|(_, v)| *v).collect();
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median = values[values.len() / 2];
        for (id, _) in cluster {
            if let Some(g) = groups.get_mut(id) {
                set(g, median);
                aligned += 1;
            }
        }
    }

    aligned
}

/// quantize groups：group 四边量化到像素网格（floor 原点，ceil 远端）。
///
/// 只改 `GroupLayout`，不动节点（微调 ≤1 step，节点仍在框内）。
/// 仅作用于 `ids` 指定的 sibling set。
///
/// 返回量化的 group 数。
fn apply_group_quantize_for(
    ids: &[String],
    groups: &mut HashMap<String, GroupLayout>,
    step: f64,
) -> usize {
    if ids.is_empty() || step <= f64::EPSILON {
        return 0;
    }

    let mut quantized = 0usize;
    for id in ids {
        let Some(group) = groups.get_mut(id) else { continue; };
        let right = group.x + group.width;
        let bottom = group.y + group.height;
        let new_x = snap_floor(group.x, step);
        let new_y = snap_floor(group.y, step);
        let new_right = snap_ceil(right, step);
        let new_bottom = snap_ceil(bottom, step);
        let new_width = (new_right - new_x).max(step);
        let new_height = (new_bottom - new_y).max(step);

        if (new_x - group.x).abs() > f64::EPSILON
            || (new_y - group.y).abs() > f64::EPSILON
            || (new_width - group.width).abs() > f64::EPSILON
            || (new_height - group.height).abs() > f64::EPSILON
        {
            quantized += 1;
        }

        group.x = new_x;
        group.y = new_y;
        group.width = new_width;
        group.height = new_height;
    }

    quantized
}

#[cfg(test)]
#[path = "group_frame_tests.rs"]
mod tests;
