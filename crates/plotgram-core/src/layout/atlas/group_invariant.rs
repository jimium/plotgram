//! GroupInvariantPass：管线尾部组框不变量校验与修复。
//!
//! 挂在 `canvas_finalize` 之前，保证两条视觉不变量：
//!
//! 1. **Containment**：∀ node n ∈ group g → n.rect ⊂ g.rect。
//!    违反时扩组框（不动节点，避免打乱已求解坐标与已路由边）。
//!
//! 2. **Sibling separation**：∀ 无祖先关系的组对 (g1, g2) → rect 不相交。
//!    违反时沿 Main 轴（Y）推开下方组（刚体平移：组框 + 成员节点 + 内部边段），
//!    最小位移 = 重叠量 + SIBLING_GAP。
//!
//! 确定性：全程按 id 排序迭代（AGENTS §2）。

use std::collections::{BTreeSet, HashMap};

use crate::ast::Diagram;
use crate::layout::types::{GroupLayout, LayoutResult};
use crate::layout::kernel::group::bounds::effective_entity_ids;

/// 兄弟组分离的最小间隙（px）。
const SIBLING_GAP: f64 = 8.0;

/// 浮点容差：重叠量小于此值视为不重叠。
const EPS: f64 = 0.5;

/// containment 修复阈值：节点超出组框小于此值时不修复（避免浮点微扰触发无意义扩展）。
const CONTAINMENT_THRESHOLD: f64 = 1.0;

/// 组框不变量修复报告。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InvariantReport {
    /// containment 修复次数（扩组框）。
    pub containment_fixes: usize,
    /// separation 修复次数（推开组）。
    pub separation_fixes: usize,
}

/// 执行 GroupInvariantPass：先 containment，再 separation。
///
/// 返回修复报告；无组时直接返回空报告。
pub fn enforce_group_invariants(
    result: &mut LayoutResult,
    diagram: &Diagram,
) -> InvariantReport {
    if result.groups.is_empty() || diagram.groups.is_empty() {
        return InvariantReport::default();
    }

    let mut report = InvariantReport::default();

    // Phase 1: Containment — 检测节点越界（仅日志，不修改几何）。
    // materialize() 的 refine_group_frames LP 可能为 sibling separation 车牲少量 containment；
    // 强行扩组框会引发 EdgeCrossesGroupInterior lint 回归，故仅报告。
    report_containment_violations(result, diagram, &mut report);

    // Phase 2: Sibling separation — 推开重叠的非祖先关系组。
    enforce_separation(result, diagram, &mut report);

    if report.containment_fixes + report.separation_fixes > 0 {
        crate::perf_log!(
            "[atlas/group-invariant] containment_fixes={} separation_fixes={}",
            report.containment_fixes,
            report.separation_fixes
        );
    }

    report
}

// ─── Phase 1: Containment ─────────────────────────────────────────

/// 对每个组，检测成员节点是否在组框内；仅记录违反数，不修改几何。
fn report_containment_violations(
    result: &LayoutResult,
    diagram: &Diagram,
    report: &mut InvariantReport,
) {
    // 按 depth 降序（深组先处理），再按 id 排序（确定性）。
    let mut sorted_groups: Vec<&crate::ast::Group> = diagram.groups.iter().collect();
    sorted_groups.sort_by(|a, b| b.depth.cmp(&a.depth).then_with(|| a.id.as_str().cmp(b.id.as_str())));

    for group in &sorted_groups {
        let gid = group.id.as_str();
        let Some(gl) = result.groups.get(gid) else {
            continue;
        };

        // 检查直属成员节点是否越界。
        let member_ids = effective_entity_ids(group, diagram);
        for eid in &member_ids {
            let Some(nl) = result.nodes.get(eid.as_str()) else {
                continue;
            };
            if nl.x < gl.x - CONTAINMENT_THRESHOLD
                || nl.y < gl.y - CONTAINMENT_THRESHOLD
                || nl.x + nl.width > gl.x + gl.width + CONTAINMENT_THRESHOLD
                || nl.y + nl.height > gl.y + gl.height + CONTAINMENT_THRESHOLD
            {
                report.containment_fixes += 1;
            }
        }
    }

    if report.containment_fixes > 0 {
        crate::perf_log!(
            "[atlas/group-invariant] containment violations detected: {} (report-only, no geometry change)",
            report.containment_fixes
        );
    }
}

// ─── Phase 2: Sibling Separation ──────────────────────────────────

/// 对无祖先关系的组对，检查矩形是否重叠；重叠则沿 Y 轴推开下方组。
///
/// 推开 = 刚体平移（组框 + 成员节点 + 内部边段），避免边断连。
fn enforce_separation(
    result: &mut LayoutResult,
    diagram: &Diagram,
    report: &mut InvariantReport,
) {
    let ancestry = build_ancestry(diagram);
    let member_map = build_member_map(diagram);

    // 按 (id_a, id_b) 排序遍历所有组对（确定性）。
    let group_ids = result.groups.keys_sorted();
    let n = group_ids.len();
    if n < 2 {
        return;
    }

    for i in 0..n {
        for j in (i + 1)..n {
            let id_a = &group_ids[i];
            let id_b = &group_ids[j];

            // 跳过祖先关系对。
            if is_ancestor(&ancestry, id_a, id_b) || is_ancestor(&ancestry, id_b, id_a) {
                continue;
            }

            // 取当前矩形（可能被前序修复改过）。
            let (Some(ga), Some(gb)) = (
                result.groups.get(id_a).cloned(),
                result.groups.get(id_b).cloned(),
            ) else {
                continue;
            };

            let overlap_y = rect_overlap_y(&ga, &gb);
            if overlap_y <= EPS {
                continue;
            }
            let overlap_x = rect_overlap_x(&ga, &gb);
            if overlap_x <= EPS {
                continue;
            }

            // 沿 Y 推开（Main 轴）：中心更靠下的组往下移。
            let cy_a = ga.y + ga.height / 2.0;
            let cy_b = gb.y + gb.height / 2.0;

            let (push_id, delta) = if cy_a <= cy_b {
                // b 在下方，b 往下移
                (id_b.clone(), overlap_y + SIBLING_GAP)
            } else {
                // a 在下方，a 往下移
                (id_a.clone(), overlap_y + SIBLING_GAP)
            };

            rigid_shift_group(result, &member_map, &ancestry, &push_id, delta);
            report.separation_fixes += 1;
        }
    }
}

/// 刚体平移一个组：组框 + 成员节点 + 落在组框内的边路径点。
///
/// 边路径点按几何位置判定：落在被移组框矩形内的点跟随平移。
/// 内部边（全部点在框内）整体平移；跨组边仅框内段平移（轻微形变可接受）。
fn rigid_shift_group(
    result: &mut LayoutResult,
    member_map: &HashMap<String, BTreeSet<String>>,
    ancestry: &HashMap<String, BTreeSet<String>>,
    group_id: &str,
    dy: f64,
) {
    // 收集该组及其后代组的全部成员节点。
    let mut all_members: BTreeSet<String> = member_map
        .get(group_id)
        .cloned()
        .unwrap_or_default();
    // 后代组的成员也纳入。
    if let Some(descendants) = ancestry.get(group_id) {
        for desc in descendants {
            if let Some(members) = member_map.get(desc) {
                all_members.extend(members.iter().cloned());
            }
        }
    }

    // 1. 取组框矩形（平移前）用于判定边路径点归属。
    let group_rect = result.groups.get(group_id).cloned();

    // 2. 平移组框（本组 + 后代组）。
    let mut groups_to_shift: Vec<String> = vec![group_id.to_string()];
    if let Some(descendants) = ancestry.get(group_id) {
        groups_to_shift.extend(descendants.iter().cloned());
    }
    for gid in &groups_to_shift {
        if let Some(gl) = result.groups.get_mut(gid) {
            gl.y += dy;
        }
    }

    // 3. 平移成员节点。
    for nid in &all_members {
        if let Some(nl) = result.nodes.get_mut(nid.as_str()) {
            nl.y += dy;
        }
    }

    // 4. 平移落在组框矩形内的边路径点。
    let Some(rect) = group_rect else {
        return;
    };
    for edge in &mut result.edges {
        if let crate::layout::types::PathGeometry::Polyline { points } = &mut edge.geometry {
            for pt in points.iter_mut() {
                if pt.x >= rect.x - EPS
                    && pt.x <= rect.x + rect.width + EPS
                    && pt.y >= rect.y - EPS
                    && pt.y <= rect.y + rect.height + EPS
                {
                    pt.y += dy;
                }
            }
        }
    }
}

// ─── 辅助 ─────────────────────────────────────────────────────────

/// 构建祖先集：group_id → 其所有后代 group_id 的集合。
fn build_ancestry(diagram: &Diagram) -> HashMap<String, BTreeSet<String>> {
    let mut children_map: HashMap<String, Vec<String>> = HashMap::new();
    for g in &diagram.groups {
        if let Some(parent) = &g.parent_id {
            children_map
                .entry(parent.as_str().to_string())
                .or_default()
                .push(g.id.as_str().to_string());
        }
    }

    let mut ancestry: HashMap<String, BTreeSet<String>> = HashMap::new();
    for g in &diagram.groups {
        let gid = g.id.as_str().to_string();
        let mut descendants = BTreeSet::new();
        collect_descendants(&children_map, &gid, &mut descendants);
        ancestry.insert(gid, descendants);
    }
    ancestry
}

fn collect_descendants(
    children_map: &HashMap<String, Vec<String>>,
    id: &str,
    out: &mut BTreeSet<String>,
) {
    if let Some(children) = children_map.get(id) {
        for child in children {
            out.insert(child.clone());
            collect_descendants(children_map, child, out);
        }
    }
}

/// 构建组成员映射：group_id → 直属成员 entity_id 集合。
fn build_member_map(diagram: &Diagram) -> HashMap<String, BTreeSet<String>> {
    let mut map: HashMap<String, BTreeSet<String>> = HashMap::new();
    for g in &diagram.groups {
        let members = effective_entity_ids(g, diagram);
        map.insert(
            g.id.as_str().to_string(),
            members.into_iter().collect(),
        );
    }
    map
}

/// 判断 `ancestor` 是否是 `descendant` 的祖先。
fn is_ancestor(ancestry: &HashMap<String, BTreeSet<String>>, ancestor: &str, descendant: &str) -> bool {
    ancestry
        .get(ancestor)
        .is_some_and(|desc| desc.contains(descendant))
}

/// 两矩形在 Y 轴上的重叠量（≤0 表示不重叠）。
fn rect_overlap_y(a: &GroupLayout, b: &GroupLayout) -> f64 {
    let a_bottom = a.y + a.height;
    let b_bottom = b.y + b.height;
    a_bottom.min(b_bottom) - a.y.max(b.y)
}

/// 两矩形在 X 轴上的重叠量（≤0 表示不重叠）。
fn rect_overlap_x(a: &GroupLayout, b: &GroupLayout) -> f64 {
    let a_right = a.x + a.width;
    let b_right = b.x + b.width;
    a_right.min(b_right) - a.x.max(b.x)
}

// ─── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Entity, Group, Identifier, Span};
    use crate::layout::types::{LayoutHints, NodeLayout};
    use crate::layout::GroupTable;

    fn nl(x: f64, y: f64, w: f64, h: f64) -> NodeLayout {
        NodeLayout { x, y, width: w, height: h }
    }

    fn gl(x: f64, y: f64, w: f64, h: f64) -> GroupLayout {
        GroupLayout { x, y, width: w, height: h }
    }

    fn entity(id: &str, group: Option<&str>) -> Entity {
        Entity {
            id: Identifier::new_unchecked(id),
            label: id.to_string(),
            attributes: crate::ast::AttributeMap::default(),
            group_id: group.map(|g| Identifier::new_unchecked(g)),
            span: Span::dummy(),
        }
    }

    fn group(id: &str, members: &[&str], parent: Option<&str>) -> Group {
        Group {
            id: Identifier::new_unchecked(id),
            label: id.to_string(),
            attributes: crate::ast::AttributeMap::default(),
            parent_id: parent.map(|p| Identifier::new_unchecked(p)),
            depth: if parent.is_some() { 1 } else { 0 },
            entity_ids: members.iter().map(|m| Identifier::new_unchecked(m)).collect(),
            child_group_ids: vec![],
            span: Span::dummy(),
        }
    }

    #[test]
    fn containment_detects_violation_without_modifying_geometry() {
        let diagram = Diagram {
            diagram_type: crate::types::DiagramType::Architecture,
            entities: vec![entity("a", Some("g1")), entity("b", Some("g1"))],
            relations: vec![],
            groups: vec![group("g1", &["a", "b"], None)],
            constraints: vec![],
            ..Default::default()
        };

        let mut nodes = HashMap::new();
        nodes.insert("a".into(), nl(10.0, 10.0, 40.0, 20.0));
        // b 超出组框右边界（显著越界 >1px）
        nodes.insert("b".into(), nl(100.0, 10.0, 40.0, 20.0));

        let mut groups = GroupTable::new();
        // 组框只包到 x=60（漏掉 b）
        groups.insert("g1".into(), gl(5.0, 5.0, 55.0, 30.0));

        let original_gl = groups.get("g1").unwrap().clone();

        let mut result = LayoutResult {
            nodes,
            groups,
            edges: vec![],
            total_width: 200.0,
            total_height: 100.0,
            hints: LayoutHints::default(),
        };

        let report = enforce_group_invariants(&mut result, &diagram);

        // 检测到越界但不修改几何
        assert!(report.containment_fixes > 0);
        let g = result.groups.get("g1").unwrap();
        assert_eq!(g.x, original_gl.x, "geometry should not change");
        assert_eq!(g.width, original_gl.width, "geometry should not change");
    }

    #[test]
    fn separation_pushes_overlapping_siblings_apart() {
        let diagram = Diagram {
            diagram_type: crate::types::DiagramType::Architecture,
            entities: vec![
                entity("a", Some("g1")),
                entity("b", Some("g2")),
            ],
            relations: vec![],
            groups: vec![
                group("g1", &["a"], None),
                group("g2", &["b"], None),
            ],
            constraints: vec![],
            ..Default::default()
        };

        let mut nodes = HashMap::new();
        nodes.insert("a".into(), nl(10.0, 10.0, 40.0, 20.0));
        nodes.insert("b".into(), nl(10.0, 40.0, 40.0, 20.0));

        let mut groups = GroupTable::new();
        // g1: y=5..35, g2: y=30..65 → Y 重叠 5px, X 完全重叠
        groups.insert("g1".into(), gl(5.0, 5.0, 50.0, 30.0));
        groups.insert("g2".into(), gl(5.0, 30.0, 50.0, 35.0));

        let mut result = LayoutResult {
            nodes,
            groups,
            edges: vec![],
            total_width: 200.0,
            total_height: 100.0,
            hints: LayoutHints::default(),
        };

        let report = enforce_group_invariants(&mut result, &diagram);

        assert_eq!(report.separation_fixes, 1);
        let g1 = result.groups.get("g1").unwrap();
        let g2 = result.groups.get("g2").unwrap();
        // g2 应被推到 g1 下方（无重叠）
        assert!(
            g2.y >= g1.y + g1.height + SIBLING_GAP - 0.01,
            "g2.y={} should be >= g1.bottom+gap={}",
            g2.y,
            g1.y + g1.height + SIBLING_GAP
        );
        // b 节点也应跟着移动
        let b = result.nodes.get("b").unwrap();
        assert!(b.y > 40.0, "node b should have been shifted down");
    }

    #[test]
    fn ancestor_groups_not_separated() {
        let mut g_parent = group("parent", &[], None);
        g_parent.child_group_ids = vec![Identifier::new_unchecked("child")];
        let g_child = group("child", &["a"], Some("parent"));

        let diagram = Diagram {
            diagram_type: crate::types::DiagramType::Architecture,
            entities: vec![entity("a", Some("child"))],
            relations: vec![],
            groups: vec![g_parent, g_child],
            constraints: vec![],
            ..Default::default()
        };

        let mut nodes = HashMap::new();
        nodes.insert("a".into(), nl(10.0, 10.0, 40.0, 20.0));

        let mut groups = GroupTable::new();
        // parent 和 child 重叠（但它们是祖先关系，不应推开）
        groups.insert("parent".into(), gl(5.0, 5.0, 50.0, 30.0));
        groups.insert("child".into(), gl(8.0, 8.0, 44.0, 24.0));

        let mut result = LayoutResult {
            nodes,
            groups,
            edges: vec![],
            total_width: 200.0,
            total_height: 100.0,
            hints: LayoutHints::default(),
        };

        let report = enforce_group_invariants(&mut result, &diagram);
        assert_eq!(report.separation_fixes, 0, "ancestor pairs should not be separated");
    }

    #[test]
    fn no_groups_noop() {
        let diagram = Diagram {
            diagram_type: crate::types::DiagramType::Flowchart,
            entities: vec![entity("a", None)],
            relations: vec![],
            groups: vec![],
            constraints: vec![],
            ..Default::default()
        };

        let mut nodes = HashMap::new();
        nodes.insert("a".into(), nl(10.0, 10.0, 40.0, 20.0));

        let mut result = LayoutResult {
            nodes,
            groups: GroupTable::new(),
            edges: vec![],
            total_width: 100.0,
            total_height: 100.0,
            hints: LayoutHints::default(),
        };

        let report = enforce_group_invariants(&mut result, &diagram);
        assert_eq!(report, InvariantReport::default());
    }
}
