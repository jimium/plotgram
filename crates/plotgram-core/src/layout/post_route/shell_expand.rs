//! G4：RouteDemand / orthosketch 壳预算（不再写 groups）。
//!
//! 生产路径：orthosketch 溢出只抬 `side_gutters`；`post_route_shell_expand` 仅供测试
//! / 诊断，runner 不再调用。

use std::collections::{BTreeMap, HashSet};

use crate::ast::Diagram;
use crate::layout::geometry::Point;
use crate::layout::group::constants::{EPS, GROUP_BORDER_SHELL_PAD};
use crate::layout::group::context::build_node_to_groups;
use crate::layout::group::hierarchy::build_group_hierarchy;
use crate::layout::engines::common::group_bounds::{GutterSide, SideGutter};
use crate::layout::{GroupLayout, LayoutResult};

/// 单侧最大补扩（EGB/lane_budget 已预留主预算，PRS 只做小步安全网）。
/// Phase 2：从 48 降到 24，降低事后扩壳对条带对称的破坏。
const PRS_MAX_PER_SIDE: f64 = 24.0;

/// 路由后若边/标签越出 shell：扩壳（**非生产**；G4 runner 已删除调用）。
///
/// 保留供单测与诊断；写入 groups 会计入写权。
pub fn post_route_shell_expand(diagram: &Diagram, layout: &mut LayoutResult) -> bool {
    let grew = apply_shell_expand_from_edges(diagram, layout);
    if grew {
        crate::layout::group::write_counter::record_group_write_at("post_route_shell_expand");
    }
    grew
}

/// 将当前 `side_gutters` 物化进 `layout.groups`（不另计写权）。
///
/// 供 feedforward 抬 gutter 后、`FrozenNodeProduct` 捕获前调用；属同一 materialize 令牌。
pub fn commit_side_gutters_into_groups(diagram: &Diagram, layout: &mut LayoutResult) {
    if diagram.groups.is_empty() || layout.nodes.is_empty() {
        return;
    }
    let Some(gr) = layout.hints.group_routing.as_ref() else {
        return;
    };
    if gr.side_gutters.is_empty() {
        return;
    }
    let pad = crate::layout::engines::common::group_bounds::GroupPadding::architecture();
    let gutters = gr.side_gutters.clone();
    layout.groups = crate::layout::engines::common::group_bounds::compute_group_bounds_unrecorded(
        diagram,
        &layout.nodes,
        pad,
        crate::layout::engines::common::group_bounds::container_padding_for_leaf(pad),
        Some(&gutters),
    )
    .into();
}

/// G4：orthosketch 溢出 → 只抬 `side_gutters` hints，**不写** `layout.groups`。
///
/// 用端点中心曼哈顿折线估计边对相关 group shell 的溢出。
/// **禁止**再跑完整 preview route。
pub fn feedforward_shell_from_orthosketch(diagram: &Diagram, layout: &mut LayoutResult) -> bool {
    if diagram.groups.is_empty() || diagram.relations.is_empty() {
        return false;
    }
    let sketch = build_orthosketch_layout(diagram, layout);
    let shell_pad = layout
        .hints
        .group_routing
        .as_ref()
        .map(|h| h.border_shell_pad)
        .unwrap_or(GROUP_BORDER_SHELL_PAD);
    let hierarchy = build_group_hierarchy(diagram, &layout.groups);
    let node_to_groups = build_node_to_groups(diagram);
    let overflow = scan_shell_overflow(diagram, &sketch, shell_pad, &hierarchy, &node_to_groups);
    if overflow.is_empty() {
        return false;
    }
    let Some(gr) = layout.hints.group_routing.as_mut() else {
        // 无 hints 时建一份，仅带 gutters（走廊由 phase_d 已写时不会走到这）
        layout.hints.group_routing = Some(crate::layout::group::GroupRoutingHints {
            corridors: Vec::new(),
            border_shell_pad: shell_pad,
            side_gutters: overflow_to_side_gutters(&overflow),
        });
        return true;
    };
    merge_overflow_into_side_gutters(&mut gr.side_gutters, &overflow);
    true
}

/// 正式 route 后：扫描溢出但不扩壳；有溢出则返回 true（调用方标 Degraded）。
pub fn route_shell_overflow_remaining(diagram: &Diagram, layout: &LayoutResult) -> bool {
    if diagram.groups.is_empty() || layout.edges.is_empty() {
        return false;
    }
    let shell_pad = layout
        .hints
        .group_routing
        .as_ref()
        .map(|h| h.border_shell_pad)
        .unwrap_or(GROUP_BORDER_SHELL_PAD);
    let side_gutters = layout
        .hints
        .group_routing
        .as_ref()
        .map(|h| h.side_gutters.clone())
        .unwrap_or_default();
    let hierarchy = build_group_hierarchy(diagram, &layout.groups);
    let node_to_groups = build_node_to_groups(diagram);
    let overflow = scan_shell_overflow(diagram, layout, shell_pad, &hierarchy, &node_to_groups);
    for ((gid, side), raw_delta) in &overflow {
        if *raw_delta <= EPS {
            continue;
        }
        let reserved = side_gutters
            .get(gid)
            .map(|g| g.get_side(*side))
            .unwrap_or(0.0);
        // groups 已与 gutters 同步物化后，按全额预留抵扣。
        if raw_delta - reserved > EPS {
            return true;
        }
    }
    false
}

fn build_orthosketch_layout(diagram: &Diagram, layout: &LayoutResult) -> LayoutResult {
    let mut sketch = layout.clone();
    sketch.edges.clear();
    sketch.edges.reserve(diagram.relations.len());
    for rel in &diagram.relations {
        let Some(from) = layout.nodes.get(rel.from.as_str()) else {
            sketch.edges.push(crate::layout::EdgeLayout::empty());
            continue;
        };
        let Some(to) = layout.nodes.get(rel.to.as_str()) else {
            sketch.edges.push(crate::layout::EdgeLayout::empty());
            continue;
        };
        let a = Point::new(from.x + from.width * 0.5, from.y + from.height * 0.5);
        let b = Point::new(to.x + to.width * 0.5, to.y + to.height * 0.5);
        let mid = Point::new(b.x, a.y);
        let points = if (a.x - b.x).abs() < EPS || (a.y - b.y).abs() < EPS {
            vec![a, b]
        } else {
            vec![a, mid, b]
        };
        sketch.edges.push(crate::layout::EdgeLayout {
            geometry: crate::layout::PathGeometry::Polyline { points },
            labels: Vec::new(),
            from_port: crate::layout::Port::Right,
            to_port: crate::layout::Port::Left,
        });
    }
    sketch
}

fn overflow_to_side_gutters(
    overflow: &BTreeMap<(String, GutterSide), f64>,
) -> BTreeMap<String, SideGutter> {
    let mut out = BTreeMap::new();
    merge_overflow_into_side_gutters(&mut out, overflow);
    out
}

fn merge_overflow_into_side_gutters(
    gutters: &mut BTreeMap<String, SideGutter>,
    overflow: &BTreeMap<(String, GutterSide), f64>,
) {
    for ((gid, side), delta) in overflow {
        if *delta <= EPS {
            continue;
        }
        let capped = (*delta).min(PRS_MAX_PER_SIDE);
        gutters
            .entry(gid.clone())
            .or_default()
            .set_side(*side, capped);
    }
}

fn apply_shell_expand_from_edges(diagram: &Diagram, layout: &mut LayoutResult) -> bool {
    let shell_pad = layout
        .hints
        .group_routing
        .as_ref()
        .map(|h| h.border_shell_pad)
        .unwrap_or(GROUP_BORDER_SHELL_PAD);

    let side_gutters = layout
        .hints
        .group_routing
        .as_ref()
        .map(|h| h.side_gutters.clone())
        .unwrap_or_default();

    let hierarchy = build_group_hierarchy(diagram, &layout.groups);
    let node_to_groups = build_node_to_groups(diagram);

    let overflow = scan_shell_overflow(
        diagram,
        layout,
        shell_pad,
        &hierarchy,
        &node_to_groups,
    );
    if overflow.is_empty() {
        return false;
    }

    let mut grew = false;
    let keys: Vec<(String, GutterSide)> = overflow.keys().cloned().collect();
    for (gid, side) in keys {
        let Some(raw_delta) = overflow.get(&(gid.clone(), side)).copied() else {
            continue;
        };
        if raw_delta <= EPS {
            continue;
        }
        // 已预留 gutter 的一侧：只补超出预留的部分
        let reserved = side_gutters
            .get(&gid)
            .map(|g| match side {
                GutterSide::Left => g.left,
                GutterSide::Right => g.right,
                GutterSide::Top => g.top,
                GutterSide::Bottom => g.bottom,
            })
            .unwrap_or(0.0);
        let delta = (raw_delta - reserved * 0.5).max(0.0).min(PRS_MAX_PER_SIDE);
        if delta <= EPS {
            continue;
        }
        if let Some(gl) = layout.groups.get_mut(&gid) {
            grow_border_outward(gl, side, delta);
            grew = true;
        }
    }

    // G3：子组扩壳后父组须至少容纳子框（同一次 PRS 写，不另开写权站点）。
    if grew {
        expand_ancestors_to_fit_children(diagram, &mut layout.groups);
    }

    grew
}

/// 自深向浅：父组矩形至少包住直接子组（无额外 padding，仅闭合嵌套）。
fn expand_ancestors_to_fit_children(
    diagram: &Diagram,
    groups: &mut crate::layout::GroupTable,
) {
    let mut order: Vec<&crate::ast::Group> = diagram.groups.iter().collect();
    order.sort_by(|a, b| b.depth.cmp(&a.depth).then_with(|| a.id.as_str().cmp(b.id.as_str())));
    for gdef in order {
        if gdef.child_group_ids.is_empty() {
            continue;
        }
        let mut min_x = f64::MAX;
        let mut min_y = f64::MAX;
        let mut max_x = f64::MIN;
        let mut max_y = f64::MIN;
        let mut has_child = false;
        for child_id in &gdef.child_group_ids {
            let Some(child) = groups.get(child_id.as_str()) else {
                continue;
            };
            has_child = true;
            min_x = min_x.min(child.x);
            min_y = min_y.min(child.y);
            max_x = max_x.max(child.x + child.width);
            max_y = max_y.max(child.y + child.height);
        }
        if !has_child {
            continue;
        }
        let Some(parent) = groups.get_mut(gdef.id.as_str()) else {
            continue;
        };
        let right = parent.x + parent.width;
        let bottom = parent.y + parent.height;
        if min_x < parent.x {
            parent.width += parent.x - min_x;
            parent.x = min_x;
        }
        if min_y < parent.y {
            parent.height += parent.y - min_y;
            parent.y = min_y;
        }
        if max_x > right {
            parent.width = max_x - parent.x;
        }
        if max_y > bottom {
            parent.height = max_y - parent.y;
        }
    }
}

fn scan_shell_overflow(
    diagram: &Diagram,
    layout: &LayoutResult,
    shell_pad: f64,
    hierarchy: &crate::layout::group::hierarchy::GroupHierarchy,
    node_to_groups: &std::collections::HashMap<String, Vec<String>>,
) -> BTreeMap<(String, GutterSide), f64> {
    let mut overflow: BTreeMap<(String, GutterSide), f64> = BTreeMap::new();

    for (ei, edge) in layout.edges.iter().enumerate() {
        let Some(rel) = diagram.relations.get(ei) else {
            continue;
        };
        let relevant = relevant_groups_for_edge(
            rel.from.as_str(),
            rel.to.as_str(),
            hierarchy,
            node_to_groups,
        );
        if relevant.is_empty() {
            continue;
        }
        accumulate_path_overflow(
            &mut overflow,
            &edge.path_points(),
            &layout.groups,
            shell_pad,
            &relevant,
        );
        for i in 0..edge.label_count() {
            if let Some((x, y, w, h)) = edge.label_bbox_at(i) {
                accumulate_rect_overflow(
                    &mut overflow,
                    x,
                    y,
                    w,
                    h,
                    &layout.groups,
                    shell_pad,
                    &relevant,
                );
            }
        }
    }

    overflow
}

/// 边端点所属 leaf group 及其祖先（不含无关 sibling 如 external）。
fn relevant_groups_for_edge(
    from_id: &str,
    to_id: &str,
    hierarchy: &crate::layout::group::hierarchy::GroupHierarchy,
    node_to_groups: &std::collections::HashMap<String, Vec<String>>,
) -> HashSet<String> {
    let mut set = HashSet::new();
    for id in [from_id, to_id] {
        if let Some(leaf) = hierarchy.node_leaf_group.get(id) {
            set.insert(leaf.clone());
            if let Some(ancestors) = hierarchy.group_ancestors.get(leaf) {
                set.extend(ancestors.iter().cloned());
            }
        }
        if let Some(groups) = node_to_groups.get(id) {
            set.extend(groups.iter().cloned());
        }
    }
    set
}

fn accumulate_path_overflow(
    overflow: &mut BTreeMap<(String, GutterSide), f64>,
    path: &[Point],
    groups: &std::collections::HashMap<String, GroupLayout>,
    shell_pad: f64,
    relevant: &HashSet<String>,
) {
    if path.len() < 2 {
        return;
    }
    for w in path.windows(2) {
        let a = w[0];
        let b = w[1];
        for gid in relevant {
            if let Some(gl) = groups.get(gid) {
                accumulate_segment_overflow(overflow, a, b, gid, gl, shell_pad);
            }
        }
    }
}

fn accumulate_segment_overflow(
    overflow: &mut BTreeMap<(String, GutterSide), f64>,
    a: Point,
    b: Point,
    gid: &str,
    gl: &GroupLayout,
    shell_pad: f64,
) {
    let inner_left = gl.x + shell_pad;
    let inner_right = gl.x + gl.width - shell_pad;
    let inner_top = gl.y + shell_pad;
    let inner_bottom = gl.y + gl.height - shell_pad;

    if (a.x - b.x).abs() < EPS {
        let x = a.x;
        let min_y = a.y.min(b.y);
        let max_y = a.y.max(b.y);
        if x < inner_left - EPS && min_y < gl.y + gl.height && max_y > gl.y {
            bump_overflow(overflow, gid, GutterSide::Left, inner_left - x);
        }
        if x > inner_right + EPS && min_y < gl.y + gl.height && max_y > gl.y {
            bump_overflow(overflow, gid, GutterSide::Right, x - inner_right);
        }
    } else if (a.y - b.y).abs() < EPS {
        let y = a.y;
        let min_x = a.x.min(b.x);
        let max_x = a.x.max(b.x);
        if y < inner_top - EPS && min_x < gl.x + gl.width && max_x > gl.x {
            bump_overflow(overflow, gid, GutterSide::Top, inner_top - y);
        }
        if y > inner_bottom + EPS && min_x < gl.x + gl.width && max_x > gl.x {
            bump_overflow(overflow, gid, GutterSide::Bottom, y - inner_bottom);
        }
    }
}

fn bump_overflow(
    overflow: &mut BTreeMap<(String, GutterSide), f64>,
    gid: &str,
    side: GutterSide,
    need: f64,
) {
    let capped = need.min(PRS_MAX_PER_SIDE);
    if capped <= EPS {
        return;
    }
    let key = (gid.to_string(), side);
    let prev = overflow.get(&key).copied().unwrap_or(0.0);
    overflow.insert(key, prev.max(capped));
}

fn accumulate_rect_overflow(
    overflow: &mut BTreeMap<(String, GutterSide), f64>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    groups: &std::collections::HashMap<String, GroupLayout>,
    shell_pad: f64,
    relevant: &HashSet<String>,
) {
    let corners = [
        (x, y),
        (x + w, y),
        (x, y + h),
        (x + w, y + h),
    ];
    for gid in relevant {
        let Some(gl) = groups.get(gid) else {
            continue;
        };
        for (px, py) in corners {
            if px < gl.x + shell_pad - EPS && py >= gl.y && py <= gl.y + gl.height {
                bump_overflow(overflow, gid, GutterSide::Left, gl.x + shell_pad - px);
            }
            if px > gl.x + gl.width - shell_pad + EPS && py >= gl.y && py <= gl.y + gl.height {
                bump_overflow(
                    overflow,
                    gid,
                    GutterSide::Right,
                    px - (gl.x + gl.width - shell_pad),
                );
            }
            if py < gl.y + shell_pad - EPS && px >= gl.x && px <= gl.x + gl.width {
                bump_overflow(overflow, gid, GutterSide::Top, gl.y + shell_pad - py);
            }
            if py > gl.y + gl.height - shell_pad + EPS && px >= gl.x && px <= gl.x + gl.width {
                bump_overflow(
                    overflow,
                    gid,
                    GutterSide::Bottom,
                    py - (gl.y + gl.height - shell_pad),
                );
            }
        }
    }
}

fn grow_border_outward(gl: &mut GroupLayout, side: GutterSide, delta: f64) {
    match side {
        GutterSide::Left => {
            gl.x -= delta;
            gl.width += delta;
        }
        GutterSide::Right => {
            gl.width += delta;
        }
        GutterSide::Top => {
            gl.y -= delta;
            gl.height += delta;
        }
        GutterSide::Bottom => {
            gl.height += delta;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn grow_left_expands_without_moving_content() {
        let mut gl = GroupLayout {
            x: 100.0,
            y: 50.0,
            width: 200.0,
            height: 100.0,
            ..Default::default()
        };
        grow_border_outward(&mut gl, GutterSide::Left, 20.0);
        assert_eq!(gl.x, 80.0);
        assert_eq!(gl.width, 220.0);
    }

    #[test]
    fn bump_overflow_is_capped() {
        let mut overflow = BTreeMap::new();
        bump_overflow(&mut overflow, "g", GutterSide::Bottom, 500.0);
        assert_eq!(overflow.get(&("g".into(), GutterSide::Bottom)).copied(), Some(24.0));
    }
}
