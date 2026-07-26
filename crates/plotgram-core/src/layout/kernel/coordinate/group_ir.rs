//! G1：将 Group IR 挂到 CoordinateProblem（双轴）。
//!
//! - Cross：`left`/`right` + H-G1 成员容纳 + H-G2 子组嵌套 + H-G3 sibling
//! - Main：`top`/`bottom` + 同上（sibling 按初值 top 排序）
//!
//! 约束同时写入可投影的 `MinSeparation` / bounds，以及审计用
//! `GroupContainment` / `GroupSiblingSeparation`。
//! G3：生产写回经 [`crate::layout::kernel::coordinate::session::LayoutSession`]；
//! `materialize_*` / `refine_group_frames` 亦供会话与测试使用。

use std::collections::{BTreeMap, HashMap};

use crate::ast::Diagram;
use crate::layout::kernel::group::bounds::GroupPadding;
use crate::layout::kernel::coordinate::model::{
    ConstraintSource, ConstraintSourceKind, CoordinateProblem, HardConstraint, NodeVariable,
    SolveAxis, VarId, VarKind,
};

/// 将 diagram 中的组挂到当前求解轴（`problem.axis`）。
///
/// `padding` 写入每个 `GroupVariable`，并用于 H-G1/H-G2 距离；
/// `sibling_gap` 为 H-G3 默认间隙（可被 [`draft_boost_h_g3_from_pair_gaps`] 抬高）。
pub fn attach_group_ir(
    problem: &mut CoordinateProblem,
    diagram: &Diagram,
    node_to_var: &HashMap<String, VarId>,
    padding: GroupPadding,
    sibling_gap: f64,
) {
    match problem.axis {
        SolveAxis::Cross => attach_axis(
            problem,
            diagram,
            node_to_var,
            padding,
            sibling_gap,
            AxisSides::Cross,
        ),
        SolveAxis::Main => attach_axis(
            problem,
            diagram,
            node_to_var,
            padding,
            sibling_gap,
            AxisSides::Main,
        ),
    }
}

/// 兼容旧入口：固定 Cross 轴。
pub fn attach_group_ir_cross(
    problem: &mut CoordinateProblem,
    diagram: &Diagram,
    node_to_var: &HashMap<String, VarId>,
    pad: f64,
    sibling_gap: f64,
) {
    problem.axis = SolveAxis::Cross;
    let padding = GroupPadding {
        left: pad,
        right: pad,
        top: pad,
        bottom: pad,
    };
    attach_group_ir(problem, diagram, node_to_var, padding, sibling_gap);
}

#[derive(Clone, Copy)]
enum AxisSides {
    Cross,
    Main,
}

fn attach_axis(
    problem: &mut CoordinateProblem,
    diagram: &Diagram,
    node_to_var: &HashMap<String, VarId>,
    padding: GroupPadding,
    sibling_gap: f64,
    sides: AxisSides,
) {
    if diagram.groups.is_empty() {
        return;
    }

    let mut groups =
        crate::layout::kernel::coordinate::auditor::compile_group_variables(diagram);
    if groups.is_empty() {
        return;
    }

    for g in &mut groups {
        g.padding = padding;
    }

    let contain_src = ConstraintSource {
        kind: ConstraintSourceKind::ContainerBound,
        nodes: vec![],
        note: match sides {
            AxisSides::Cross => "group containment (cross)",
            AxisSides::Main => "group containment (main)",
        },
    };
    let sibling_src = ConstraintSource {
        kind: ConstraintSourceKind::ContainerBound,
        nodes: vec![],
        note: match sides {
            AxisSides::Cross => "group sibling separation (cross)",
            AxisSides::Main => "group sibling separation (main)",
        },
    };
    let nest_src = ConstraintSource {
        kind: ConstraintSourceKind::ContainerBound,
        nodes: vec![],
        note: match sides {
            AxisSides::Cross => "group nest H-G2 (cross)",
            AxisSides::Main => "group nest H-G2 (main)",
        },
    };

    // 分配边界变量 + 初值（空成员容器用后代成员估初值）
    let extent_members: Vec<Vec<String>> = groups
        .iter()
        .enumerate()
        .map(|(gi, _)| descendant_members(gi, &groups))
        .collect();
    for (gi, g) in groups.iter_mut().enumerate() {
        let (lo_ext, hi_ext, any) =
            member_extent(problem, node_to_var, &extent_members[gi], sides);
        if !any {
            continue;
        }
        let (lo_ext, hi_ext) = if lo_ext.is_finite() && hi_ext.is_finite() {
            (lo_ext, hi_ext)
        } else {
            (0.0, 100.0)
        };
        let (pad_lo, pad_hi, lo_suffix, hi_suffix, lo_order, hi_order) = match sides {
            AxisSides::Cross => (padding.left, padding.right, "left", "right", 0, 1),
            AxisSides::Main => (padding.top, padding.bottom, "top", "bottom", 0, 1),
        };

        let lo_id = push_axis_var(
            problem,
            &format!("{}#{}", g.stable_id, lo_suffix),
            lo_ext - pad_lo,
            lo_order,
        );
        let hi_id = push_axis_var(
            problem,
            &format!("{}#{}", g.stable_id, hi_suffix),
            hi_ext + pad_hi,
            hi_order,
        );

        match sides {
            AxisSides::Cross => {
                g.left = Some(lo_id);
                g.right = Some(hi_id);
            }
            AxisSides::Main => {
                g.top = Some(lo_id);
                g.bottom = Some(hi_id);
            }
        }

        // H-G6 草稿：组框不越过画布原点一侧
        problem.hard.push(HardConstraint::LowerBound {
            var: lo_id,
            value: 0.0,
            source: ConstraintSource {
                kind: ConstraintSourceKind::ContainerBound,
                nodes: vec![g.stable_id.clone()],
                note: "H-G6 canvas lower (shadow)",
            },
        });
    }

    // H-G1：成员 ⊆ 组框
    for (gi, g) in groups.iter().enumerate() {
        let Some((lo_id, hi_id)) = axis_bounds(g, sides) else {
            continue;
        };
        let (pad_lo, pad_hi) = match sides {
            AxisSides::Cross => (g.padding.left, g.padding.right),
            AxisSides::Main => (g.padding.top, g.padding.bottom),
        };
        for mid in &g.members {
            let Some(&vid) = node_to_var.get(mid) else {
                continue;
            };
            let half = problem.vars[vid].axis_size * 0.5;
            let audit_pad = pad_lo.max(pad_hi);
            problem.hard.push(HardConstraint::GroupContainment {
                group_index: gi,
                member_var: vid,
                pad: audit_pad,
                source: contain_src.clone(),
            });
            problem.hard.push(HardConstraint::MinSeparation {
                left: lo_id,
                right: vid,
                distance: half + pad_lo,
                source: ConstraintSource {
                    kind: ConstraintSourceKind::ContainerBound,
                    nodes: vec![g.stable_id.clone(), mid.clone()],
                    note: match sides {
                        AxisSides::Cross => "group left containment",
                        AxisSides::Main => "group top containment",
                    },
                },
            });
            problem.hard.push(HardConstraint::MinSeparation {
                left: vid,
                right: hi_id,
                distance: half + pad_hi,
                source: ConstraintSource {
                    kind: ConstraintSourceKind::ContainerBound,
                    nodes: vec![g.stable_id.clone(), mid.clone()],
                    note: match sides {
                        AxisSides::Cross => "group right containment",
                        AxisSides::Main => "group bottom containment",
                    },
                },
            });
        }
    }

    // H-G2：子组 ⊆ 父组（边界变量之间）
    for (pi, parent) in groups.iter().enumerate() {
        let Some((p_lo, p_hi)) = axis_bounds(parent, sides) else {
            continue;
        };
        let (pad_lo, pad_hi) = match sides {
            AxisSides::Cross => (parent.padding.left, parent.padding.right),
            AxisSides::Main => (parent.padding.top, parent.padding.bottom),
        };
        for &ci in &parent.children {
            let Some(child) = groups.get(ci) else {
                continue;
            };
            let Some((c_lo, c_hi)) = axis_bounds(child, sides) else {
                continue;
            };
            problem.hard.push(HardConstraint::MinSeparation {
                left: p_lo,
                right: c_lo,
                distance: pad_lo,
                source: ConstraintSource {
                    kind: ConstraintSourceKind::ContainerBound,
                    nodes: vec![parent.stable_id.clone(), child.stable_id.clone()],
                    note: "H-G2 nest lo",
                },
            });
            problem.hard.push(HardConstraint::MinSeparation {
                left: c_hi,
                right: p_hi,
                distance: pad_hi,
                source: ConstraintSource {
                    kind: nest_src.kind,
                    nodes: vec![parent.stable_id.clone(), child.stable_id.clone()],
                    note: "H-G2 nest hi",
                },
            });
            let _ = pi;
        }
    }

    // H-G3：同级 sibling（按父组分组，再按初值 lo 排序）
    let mut by_parent: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (gi, g) in groups.iter().enumerate() {
        if axis_bounds(g, sides).is_none() {
            continue;
        }
        let parent_key = g
            .parent
            .and_then(|p| groups.get(p).map(|pg| pg.stable_id.clone()))
            .unwrap_or_default();
        by_parent.entry(parent_key).or_default().push(gi);
    }

    for indices in by_parent.values_mut() {
        indices.sort_by(|&a, &b| {
            let la = axis_lo_initial(&groups[a], sides, &problem.initial.values);
            let lb = axis_lo_initial(&groups[b], sides, &problem.initial.values);
            la.partial_cmp(&lb)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| groups[a].stable_id.cmp(&groups[b].stable_id))
        });
        for w in indices.windows(2) {
            let (li, ri) = (w[0], w[1]);
            let (Some((_, l_hi)), Some((r_lo, _))) =
                (axis_bounds(&groups[li], sides), axis_bounds(&groups[ri], sides))
            else {
                continue;
            };
            problem.hard.push(HardConstraint::GroupSiblingSeparation {
                left_group: li,
                right_group: ri,
                distance: sibling_gap,
                source: sibling_src.clone(),
            });
            problem.hard.push(HardConstraint::MinSeparation {
                left: l_hi,
                right: r_lo,
                distance: sibling_gap,
                source: ConstraintSource {
                    kind: ConstraintSourceKind::ContainerBound,
                    nodes: vec![
                        groups[li].stable_id.clone(),
                        groups[ri].stable_id.clone(),
                    ],
                    note: "group sibling gap",
                },
            });
        }
    }

    problem.groups = groups;
}

fn push_axis_var(
    problem: &mut CoordinateProblem,
    stable_id: &str,
    initial: f64,
    order: usize,
) -> VarId {
    let id = problem.vars.len();
    problem.vars.push(NodeVariable {
        var_id: id,
        stable_id: stable_id.to_string(),
        kind: VarKind::Axis,
        rank: usize::MAX,
        order,
        axis_size: 0.0,
        movable: true,
    });
    problem.initial.values.push(initial);
    id
}

fn descendant_members(
    gi: usize,
    groups: &[crate::layout::kernel::coordinate::model::GroupVariable],
) -> Vec<String> {
    let mut out = groups[gi].members.clone();
    let children = groups[gi].children.clone();
    for ci in children {
        out.extend(descendant_members(ci, groups));
    }
    out.sort();
    out.dedup();
    out
}

fn member_extent(
    problem: &CoordinateProblem,
    node_to_var: &HashMap<String, VarId>,
    members: &[String],
    sides: AxisSides,
) -> (f64, f64, bool) {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    let mut any = false;
    let _ = sides;
    for mid in members {
        let Some(&vid) = node_to_var.get(mid) else {
            continue;
        };
        any = true;
        let half = problem.vars[vid].axis_size * 0.5;
        let c = problem.initial.values.get(vid).copied().unwrap_or(0.0);
        lo = lo.min(c - half);
        hi = hi.max(c + half);
    }
    (lo, hi, any)
}

fn axis_bounds(
    g: &crate::layout::kernel::coordinate::model::GroupVariable,
    sides: AxisSides,
) -> Option<(VarId, VarId)> {
    match sides {
        AxisSides::Cross => Some((g.left?, g.right?)),
        AxisSides::Main => Some((g.top?, g.bottom?)),
    }
}

fn axis_lo_initial(
    g: &crate::layout::kernel::coordinate::model::GroupVariable,
    sides: AxisSides,
    initials: &[f64],
) -> f64 {
    let vid = match sides {
        AxisSides::Cross => g.left,
        AxisSides::Main => g.top,
    };
    vid.and_then(|v| initials.get(v).copied()).unwrap_or(0.0)
}

/// G2 草稿：按 `(group_a, group_b) → min_gap` 抬高已有 H-G3 距离。
///
/// 键无序：内部规范化为 `min(id), max(id)`。不完全闭环（无走廊几何）留给 G4。
pub fn draft_boost_h_g3_from_pair_gaps(
    problem: &mut CoordinateProblem,
    pair_gaps: &BTreeMap<(String, String), f64>,
) {
    if pair_gaps.is_empty() || problem.groups.is_empty() {
        return;
    }
    let id_of = |ix: usize| problem.groups.get(ix).map(|g| g.stable_id.as_str());

    for hc in &mut problem.hard {
        match hc {
            HardConstraint::GroupSiblingSeparation {
                left_group,
                right_group,
                distance,
                source,
            } => {
                let (Some(a), Some(b)) = (id_of(*left_group), id_of(*right_group)) else {
                    continue;
                };
                let key = if a <= b {
                    (a.to_string(), b.to_string())
                } else {
                    (b.to_string(), a.to_string())
                };
                if let Some(&need) = pair_gaps.get(&key) {
                    if need > *distance {
                        *distance = need;
                        source.kind = ConstraintSourceKind::RouteDemand;
                        source.note = "H-G3 RouteDemand draft boost";
                    }
                }
            }
            HardConstraint::MinSeparation {
                left,
                right,
                distance,
                source,
            } if source.note == "group sibling gap" => {
                // 用约束涉及的组 id（source.nodes）查找
                if source.nodes.len() >= 2 {
                    let a = &source.nodes[0];
                    let b = &source.nodes[1];
                    let key = if a <= b {
                        (a.clone(), b.clone())
                    } else {
                        (b.clone(), a.clone())
                    };
                    if let Some(&need) = pair_gaps.get(&key) {
                        if need > *distance {
                            *distance = need;
                            source.kind = ConstraintSourceKind::RouteDemand;
                            source.note = "H-G3 RouteDemand draft boost";
                        }
                    }
                }
                let _ = (left, right);
            }
            _ => {}
        }
    }
}

/// 由走廊负载估算 sibling 间隙草稿（lane_pitch × load，至少 `base_gap`）。
pub fn draft_sibling_gap_from_load(base_gap: f64, load: usize, lane_pitch: f64) -> f64 {
    base_gap.max(lane_pitch * load as f64)
}

/// G4：从走廊 demand 编译 H-G3 pair gaps（`min(id),max(id)` → 最小间隙）。
///
/// 源头：[`crate::layout::demand::corridor`]；负载不足一车道时仍至少 `base_gap`。
pub fn pair_gaps_from_corridor_demands(
    demands: &[crate::layout::demand::CorridorDemand],
    base_gap: f64,
    lane_pitch: f64,
) -> BTreeMap<(String, String), f64> {
    let mut gaps = BTreeMap::new();
    for d in demands {
        let need = draft_sibling_gap_from_load(base_gap, d.load.max(1), lane_pitch);
        let key = if d.group_a <= d.group_b {
            (d.group_a.clone(), d.group_b.clone())
        } else {
            (d.group_b.clone(), d.group_a.clone())
        };
        let entry = gaps.entry(key).or_insert(base_gap);
        *entry = (*entry).max(need);
    }
    gaps
}

/// G4：无几何时，按跨 leaf-group 边计数估算 sibling pair gaps（供 Cross builder）。
pub fn pair_gaps_from_cross_group_edge_loads(
    diagram: &Diagram,
    base_gap: f64,
    lane_pitch: f64,
) -> BTreeMap<(String, String), f64> {
    use std::collections::HashMap;
    let mut node_leaf: HashMap<&str, &str> = HashMap::new();
    for g in &diagram.groups {
        for mid in &g.entity_ids {
            let mid_s = mid.as_str();
            // 更深 leaf 覆盖浅层（depth 更大优先）
            match node_leaf.get(mid_s) {
                Some(prev) => {
                    let prev_depth = diagram
                        .groups
                        .iter()
                        .find(|x| x.id.as_str() == *prev)
                        .map(|x| x.depth)
                        .unwrap_or(0);
                    if g.depth >= prev_depth {
                        node_leaf.insert(mid_s, g.id.as_str());
                    }
                }
                None => {
                    node_leaf.insert(mid_s, g.id.as_str());
                }
            }
        }
    }
    let mut loads: BTreeMap<(String, String), usize> = BTreeMap::new();
    for rel in &diagram.relations {
        let Some(fa) = node_leaf.get(rel.from.as_str()).copied() else {
            continue;
        };
        let Some(tb) = node_leaf.get(rel.to.as_str()).copied() else {
            continue;
        };
        if fa == tb {
            continue;
        }
        let key = if fa <= tb {
            (fa.to_string(), tb.to_string())
        } else {
            (tb.to_string(), fa.to_string())
        };
        *loads.entry(key).or_insert(0) += 1;
    }
    let mut gaps = BTreeMap::new();
    for (key, load) in loads {
        gaps.insert(key, draft_sibling_gap_from_load(base_gap, load, lane_pitch));
    }
    gaps
}

/// 将 Cross 轴求解后的组左/右边界物化为 `GroupLayout` 的 x/width。
///
/// Stage 2b：缺失条目会 upsert（不再依赖 `compute_group_bounds` 种子表）。
pub fn materialize_group_cross_bounds(
    problem: &CoordinateProblem,
    coords: &[f64],
    groups_out: &mut HashMap<String, crate::layout::GroupLayout>,
) {
    for g in &problem.groups {
        let (Some(l), Some(r)) = (g.left, g.right) else {
            continue;
        };
        if l >= coords.len() || r >= coords.len() {
            continue;
        }
        let left = coords[l];
        let right = coords[r];
        let width = (right - left).max(1.0);
        let entry = groups_out.entry(g.stable_id.clone()).or_default();
        entry.x = left;
        entry.width = width;
    }
}

/// 将 Main 轴求解后的组顶/底边界物化为 `GroupLayout` 的 y/height。
///
/// Stage 2b：缺失条目会 upsert。
pub fn materialize_group_main_bounds(
    problem: &CoordinateProblem,
    coords: &[f64],
    groups_out: &mut HashMap<String, crate::layout::GroupLayout>,
) {
    for g in &problem.groups {
        let (Some(t), Some(b)) = (g.top, g.bottom) else {
            continue;
        };
        if t >= coords.len() || b >= coords.len() {
            continue;
        }
        let top = coords[t];
        let bottom = coords[b];
        let height = (bottom - top).max(1.0);
        let entry = groups_out.entry(g.stable_id.clone()).or_default();
        entry.y = top;
        entry.height = height;
    }
}

/// 在已有节点几何上，用 Group IR 投影一次组框（按 `axis`）。
pub fn refine_group_frames(
    diagram: &Diagram,
    nodes: &HashMap<String, crate::layout::NodeLayout>,
    groups: &mut HashMap<String, crate::layout::GroupLayout>,
    padding: GroupPadding,
    sibling_gap: f64,
    axis: SolveAxis,
) {
    if diagram.groups.is_empty() || nodes.is_empty() {
        return;
    }

    let mut problem = CoordinateProblem::build(
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        crate::layout::kernel::coordinate::model::InitialCoordinates {
            values: Vec::new(),
        },
        axis,
    );
    let mut node_to_var: HashMap<String, VarId> = HashMap::new();

    let mut ids: Vec<String> = nodes.keys().cloned().collect();
    ids.sort();
    for id in &ids {
        let Some(nl) = nodes.get(id) else {
            continue;
        };
        let vid = problem.vars.len();
        let (center, axis_size) = match axis {
            SolveAxis::Cross => (nl.x + nl.width * 0.5, nl.width),
            SolveAxis::Main => (nl.y + nl.height * 0.5, nl.height),
        };
        problem.vars.push(NodeVariable {
            var_id: vid,
            stable_id: id.clone(),
            kind: VarKind::Real,
            rank: 0,
            order: vid,
            axis_size,
            movable: false,
        });
        problem.initial.values.push(center);
        node_to_var.insert(id.clone(), vid);
        problem.hard.push(HardConstraint::Fixed {
            var: vid,
            value: center,
            source: ConstraintSource {
                kind: ConstraintSourceKind::UserConstraint,
                nodes: vec![id.clone()],
                note: "freeze node for group refine",
            },
        });
    }

    attach_group_ir(&mut problem, diagram, &node_to_var, padding, sibling_gap);
    let mut coords = problem.initial.values.clone();
    crate::layout::kernel::coordinate::projection::project_hard_constraints(&mut coords, &problem);
    match axis {
        SolveAxis::Cross => materialize_group_cross_bounds(&problem, &coords, groups),
        SolveAxis::Main => materialize_group_main_bounds(&problem, &coords, groups),
    }
}

/// 兼容旧入口：Cross 轴 refine。
pub fn refine_group_frames_cross(
    diagram: &Diagram,
    nodes: &HashMap<String, crate::layout::NodeLayout>,
    groups: &mut HashMap<String, crate::layout::GroupLayout>,
    pad: f64,
    sibling_gap: f64,
) {
    let padding = GroupPadding {
        left: pad,
        right: pad,
        top: pad,
        bottom: pad,
    };
    refine_group_frames(
        diagram,
        nodes,
        groups,
        padding,
        sibling_gap,
        SolveAxis::Cross,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AttributeMap, Diagram, Entity, Group, Identifier, Span};
    use crate::layout::kernel::coordinate::model::{
        CoordinateProblem, GroupRole, InitialCoordinates, SolveAxis,
    };
    use crate::layout::{GroupLayout, NodeLayout};

    fn make_nested_diagram() -> Diagram {
        let mut d = Diagram::default();
        d.groups = vec![
            Group {
                id: Identifier::new_unchecked("outer"),
                label: "outer".into(),
                attributes: AttributeMap::default(),
                parent_id: None,
                depth: 0,
                entity_ids: vec![],
                child_group_ids: vec![
                    Identifier::new_unchecked("a"),
                    Identifier::new_unchecked("b"),
                ],
                span: Span::dummy(),
            },
            Group {
                id: Identifier::new_unchecked("a"),
                label: "a".into(),
                attributes: AttributeMap::default(),
                parent_id: Some(Identifier::new_unchecked("outer")),
                depth: 1,
                entity_ids: vec![Identifier::new_unchecked("n1")],
                child_group_ids: vec![],
                span: Span::dummy(),
            },
            Group {
                id: Identifier::new_unchecked("b"),
                label: "b".into(),
                attributes: AttributeMap::default(),
                parent_id: Some(Identifier::new_unchecked("outer")),
                depth: 1,
                entity_ids: vec![Identifier::new_unchecked("n2")],
                child_group_ids: vec![],
                span: Span::dummy(),
            },
        ];
        d.entities = vec![
            Entity {
                id: Identifier::new_unchecked("n1"),
                label: "n1".into(),
                attributes: AttributeMap::default(),
                group_id: Some(Identifier::new_unchecked("a")),
                span: Span::dummy(),
            },
            Entity {
                id: Identifier::new_unchecked("n2"),
                label: "n2".into(),
                attributes: AttributeMap::default(),
                group_id: Some(Identifier::new_unchecked("b")),
                span: Span::dummy(),
            },
        ];
        d
    }

    fn base_problem(axis: SolveAxis) -> (CoordinateProblem, HashMap<String, VarId>) {
        let mut problem = CoordinateProblem {
            vars: Vec::new(),
            layers: Vec::new(),
            hard: Vec::new(),
            objectives: Vec::new(),
            initial: InitialCoordinates { values: Vec::new() },
            config: Default::default(),
            axis,
            groups: Vec::new(),
        };
        let mut node_to_var = HashMap::new();
        for (id, center, size) in [("n1", 50.0, 40.0), ("n2", 150.0, 40.0)] {
            let vid = problem.vars.len();
            problem.vars.push(NodeVariable {
                var_id: vid,
                stable_id: id.into(),
                kind: VarKind::Real,
                rank: 0,
                order: vid,
                axis_size: size,
                movable: false,
            });
            problem.initial.values.push(center);
            node_to_var.insert(id.into(), vid);
            problem.hard.push(HardConstraint::Fixed {
                var: vid,
                value: center,
                source: ConstraintSource {
                    kind: ConstraintSourceKind::UserConstraint,
                    nodes: vec![id.into()],
                    note: "test freeze",
                },
            });
        }
        (problem, node_to_var)
    }

    #[test]
    fn attach_cross_sets_left_right_and_structure() {
        let d = make_nested_diagram();
        let (mut problem, node_to_var) = base_problem(SolveAxis::Cross);
        let pad = GroupPadding::uniform(10.0, 0.0);
        attach_group_ir(&mut problem, &d, &node_to_var, pad, 20.0);

        assert_eq!(problem.groups.len(), 3);
        let a = problem
            .groups
            .iter()
            .find(|g| g.stable_id == "a")
            .unwrap();
        assert!(a.left.is_some() && a.right.is_some());
        assert!(a.top.is_none() && a.bottom.is_none());
        assert_eq!(a.role, GroupRole::Container);
        assert!(a.parent.is_some());

        let outer = problem
            .groups
            .iter()
            .find(|g| g.stable_id == "outer")
            .unwrap();
        assert_eq!(outer.children.len(), 2);
        assert!(outer.left.is_some() && outer.right.is_some());

        assert!(problem.hard.iter().any(|h| matches!(
            h,
            HardConstraint::GroupContainment { .. }
        )));
        assert!(problem.hard.iter().any(|h| matches!(
            h,
            HardConstraint::GroupSiblingSeparation { .. }
        )));
    }

    #[test]
    fn attach_main_sets_top_bottom() {
        let d = make_nested_diagram();
        let (mut problem, node_to_var) = base_problem(SolveAxis::Main);
        let pad = GroupPadding {
            left: 8.0,
            right: 8.0,
            top: 16.0,
            bottom: 8.0,
        };
        attach_group_ir(&mut problem, &d, &node_to_var, pad, 24.0);

        let a = problem
            .groups
            .iter()
            .find(|g| g.stable_id == "a")
            .unwrap();
        assert!(a.top.is_some() && a.bottom.is_some());
        assert!(a.left.is_none() && a.right.is_none());
        assert!((a.padding.top - 16.0).abs() < 1e-9);
    }

    #[test]
    fn project_cross_containment_and_sibling() {
        let d = make_nested_diagram();
        let (mut problem, node_to_var) = base_problem(SolveAxis::Cross);
        let pad = GroupPadding::uniform(10.0, 0.0);
        attach_group_ir(&mut problem, &d, &node_to_var, pad, 30.0);

        for g in &problem.groups {
            if g.stable_id == "a" {
                if let (Some(l), Some(r)) = (g.left, g.right) {
                    problem.initial.values[l] = 20.0;
                    problem.initial.values[r] = 80.0;
                }
            }
            if g.stable_id == "b" {
                if let (Some(l), Some(r)) = (g.left, g.right) {
                    problem.initial.values[l] = 85.0;
                    problem.initial.values[r] = 180.0;
                }
            }
        }

        let mut coords = problem.initial.values.clone();
        crate::layout::kernel::coordinate::projection::project_hard_constraints(
            &mut coords,
            &problem,
        );
        let v = crate::layout::kernel::coordinate::projection::max_hard_violation(
            &coords, &problem,
        );
        assert!(
            v < 0.1,
            "expected near-feasible after projection, max_violation={v}"
        );
    }

    #[test]
    fn draft_route_demand_boosts_sibling_gap() {
        let d = make_nested_diagram();
        let (mut problem, node_to_var) = base_problem(SolveAxis::Cross);
        attach_group_ir(
            &mut problem,
            &d,
            &node_to_var,
            GroupPadding::uniform(10.0, 0.0),
            20.0,
        );
        let mut gaps = BTreeMap::new();
        gaps.insert(("a".into(), "b".into()), 55.0);
        draft_boost_h_g3_from_pair_gaps(&mut problem, &gaps);

        let sib = problem
            .hard
            .iter()
            .find_map(|h| match h {
                HardConstraint::GroupSiblingSeparation {
                    left_group,
                    right_group,
                    distance,
                    ..
                } => {
                    let la = &problem.groups[*left_group].stable_id;
                    let lb = &problem.groups[*right_group].stable_id;
                    if (la == "a" && lb == "b") || (la == "b" && lb == "a") {
                        Some(*distance)
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .expect("sibling constraint");
        assert!((sib - 55.0).abs() < 1e-9);
    }

    #[test]
    fn shadow_refine_matches_plain_container_cross() {
        let d = make_nested_diagram();
        let mut nodes = HashMap::new();
        nodes.insert(
            "n1".into(),
            NodeLayout {
                x: 30.0,
                y: 40.0,
                width: 40.0,
                height: 30.0,
            },
        );
        nodes.insert(
            "n2".into(),
            NodeLayout {
                x: 130.0,
                y: 40.0,
                width: 40.0,
                height: 30.0,
            },
        );
        let pad = GroupPadding::uniform(10.0, 16.0);
        let plain = crate::layout::kernel::group::bounds::compute_group_bounds(
            &d, &nodes, pad,
        );
        let mut shadow = plain.clone();
        for g in shadow.values_mut() {
            g.x -= 5.0;
            g.width = (g.width - 10.0).max(1.0);
        }
        refine_group_frames(&d, &nodes, &mut shadow, pad, 20.0, SolveAxis::Cross);

        for (gid, exp) in &plain {
            let got = shadow.get(gid).unwrap();
            if gid == "a" || gid == "b" {
                assert!(
                    (got.x - exp.x).abs() < 1.5,
                    "{gid} x: got={} exp={}",
                    got.x,
                    exp.x
                );
                assert!(
                    (got.width - exp.width).abs() < 1.5,
                    "{gid} w: got={} exp={}",
                    got.width,
                    exp.width
                );
            }
        }
        let _ = GroupLayout::default();
    }
}
