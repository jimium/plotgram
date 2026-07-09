//! PRS：路由后单次 group 壳层外扩（architecture 安全网）。

use std::collections::{BTreeMap, HashSet};

use crate::ast::Diagram;
use crate::layout::geometry::Point;
use crate::layout::group::constants::{EPS, GROUP_BORDER_SHELL_PAD};
use crate::layout::group::context::build_node_to_groups;
use crate::layout::group::hierarchy::build_group_hierarchy;
use crate::layout::node::common::group_bounds::GutterSide;
use crate::layout::{GroupLayout, LayoutResult};

/// 单侧最大补扩（EGB 已预留主预算，PRS 只做小步安全网）。
const PRS_MAX_PER_SIDE: f64 = 48.0;

/// 路由与标签完成后，若几何越出 group border shell，向外扩壳一次（不重路由）。
pub fn post_route_shell_expand(diagram: &Diagram, layout: &mut LayoutResult) -> bool {
    let shell_pad = layout
        .hints
        .group_routing
        .as_ref()
        .map(|h| h.border_shell_pad)
        .unwrap_or(GROUP_BORDER_SHELL_PAD);

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

    let keys: Vec<(String, GutterSide)> = overflow.keys().cloned().collect();
    for (gid, side) in keys {
        let Some(delta) = overflow.get(&(gid.clone(), side)).copied() else {
            continue;
        };
        if delta <= EPS {
            continue;
        }
        if let Some(gl) = layout.groups.get_mut(&gid) {
            grow_border_outward(gl, side, delta);
        }
    }

    true
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
        assert_eq!(overflow.get(&("g".into(), GutterSide::Bottom)).copied(), Some(48.0));
    }
}
