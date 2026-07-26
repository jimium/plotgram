//! G3：单次布局会话。
//!
//! 对外一次 [`LayoutSession::materialize`]；内部 Cross→Main 交替投影。
//! Stage 2b：组框由 Group IR + 硬约束投影产出，**不再**经 `compute_group_bounds` 写权。

use std::collections::HashMap;

use crate::ast::Diagram;
use crate::layout::kernel::group::bounds::GroupPadding;
use crate::layout::kernel::coordinate::group_ir::{
    materialize_group_cross_bounds, materialize_group_main_bounds, refine_group_frames,
};
use crate::layout::kernel::coordinate::model::SolveAxis;
use crate::layout::{GroupLayout, GroupTable, NodeLayout};

/// 一次 layout run 的求解会话（节点已定，物化组框）。
pub struct LayoutSession<'a> {
    diagram: &'a Diagram,
    nodes: &'a HashMap<String, NodeLayout>,
    padding: GroupPadding,
    sibling_gap: f64,
}

/// 会话物化结果：组框表（唯一权威写回）。
pub struct LayoutSolution {
    pub groups: GroupTable,
}

impl<'a> LayoutSession<'a> {
    pub fn new(
        diagram: &'a Diagram,
        nodes: &'a HashMap<String, NodeLayout>,
        padding: GroupPadding,
    ) -> Self {
        Self {
            diagram,
            nodes,
            padding,
            sibling_gap: crate::layout::constants::SUGIYAMA_GROUP_PADDING,
        }
    }

    pub fn with_sibling_gap(mut self, gap: f64) -> Self {
        self.sibling_gap = gap;
        self
    }

    /// 朴素后验包围盒（计 1 次 `materialize` 写权）。
    ///
    /// 仅供对照 / 诊断；Atlas 生产路径走 [`Self::materialize`]。
    pub fn materialize_plain(self) -> LayoutSolution {
        let groups = crate::layout::kernel::group::bounds::compute_group_bounds(
            self.diagram,
            self.nodes,
            self.padding,
        );
        LayoutSolution {
            groups: groups.into(),
        }
    }

    /// Stage 2b + Post-S7 Wave2：Group IR 投影产出组框。
    ///
    /// - 以 `compute_group_bounds_unrecorded` 为几何种子（不记写权）
    /// - **Cross 轴** `refine_group_frames`：GroupContainment / SiblingSeparation
    /// - **Main 轴**：不做完整 sibling LP（水平并列与冻结节点冲突）；改为
    ///   [`expand_ancestors_to_contain_descendants`] 几何扩容，消除子组越界
    /// - **不**走计权的 `compute_group_bounds`；`write_counter` 保持 0
    pub fn materialize(self) -> LayoutSolution {
        if self.diagram.groups.is_empty() || self.nodes.is_empty() {
            return LayoutSolution {
                groups: GroupTable::default(),
            };
        }

        let mut groups = crate::layout::kernel::group::bounds::compute_group_bounds_unrecorded(
            self.diagram,
            self.nodes,
            self.padding,
            crate::layout::kernel::group::bounds::container_padding_for_leaf(self.padding),
            None,
        );

        refine_group_frames(
            self.diagram,
            self.nodes,
            &mut groups,
            self.padding,
            self.sibling_gap,
            SolveAxis::Cross,
        );

        expand_ancestors_to_contain_descendants(self.diagram, &mut groups, self.padding);

        LayoutSolution {
            groups: groups.into(),
        }
    }

    /// 将已有求解坐标按轴写入组表（供测试 / shadow；调用方负责写权记账）。
    pub fn apply_axis_coords(
        problem: &crate::layout::kernel::coordinate::model::CoordinateProblem,
        coords: &[f64],
        groups: &mut HashMap<String, GroupLayout>,
        axis: SolveAxis,
    ) {
        match axis {
            SolveAxis::Cross => materialize_group_cross_bounds(problem, coords, groups),
            SolveAxis::Main => materialize_group_main_bounds(problem, coords, groups),
        }
    }
}

/// 自底向上扩展祖先组框，使子组几何落在父框内（Wave2：替代不可行的 Main sibling LP）。
fn expand_ancestors_to_contain_descendants(
    diagram: &Diagram,
    groups: &mut HashMap<String, GroupLayout>,
    padding: GroupPadding,
) {
    let mut by_depth: Vec<(i32, String, Option<String>)> = diagram
        .groups
        .iter()
        .map(|g| {
            (
                g.depth as i32,
                g.id.as_str().to_string(),
                g.parent_id.as_ref().map(|p| p.as_str().to_string()),
            )
        })
        .collect();
    // 深组先处理，再扩父
    by_depth.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

    for (_depth, id, parent) in by_depth {
        let Some(parent_id) = parent else {
            continue;
        };
        let Some(child) = groups.get(&id).cloned() else {
            continue;
        };
        let Some(parent_gl) = groups.get_mut(&parent_id) else {
            continue;
        };
        let left = child.x - padding.left;
        let right = child.x + child.width + padding.right;
        let top = child.y - padding.top;
        let bottom = child.y + child.height + padding.bottom;
        let new_left = parent_gl.x.min(left);
        let new_top = parent_gl.y.min(top);
        let new_right = (parent_gl.x + parent_gl.width).max(right);
        let new_bottom = (parent_gl.y + parent_gl.height).max(bottom);
        parent_gl.x = new_left;
        parent_gl.y = new_top;
        parent_gl.width = (new_right - new_left).max(0.0);
        parent_gl.height = (new_bottom - new_top).max(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AttributeMap, Diagram, Entity, Group, Identifier, Span};

    fn nested_diagram() -> Diagram {
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

    #[test]
    fn materialize_ir_no_write_counter_and_covers_all_groups() {
        crate::layout::group::write_counter::reset_group_write_counters();
        let d = nested_diagram();
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
        let sol = LayoutSession::new(&d, &nodes, pad).materialize();

        assert_eq!(crate::layout::group::write_counter::group_write_count(), 0);
        assert_eq!(sol.groups.len(), 3);
        for gid in ["a", "b", "outer"] {
            let g = sol.groups.get(gid).expect(gid);
            assert!(g.width >= 1.0, "{gid} width");
            assert!(g.height >= 1.0, "{gid} height");
        }
        let a = sol.groups.get("a").unwrap();
        let n1 = &nodes["n1"];
        assert!(a.x <= n1.x + 1e-6, "contain left");
        assert!(a.x + a.width >= n1.x + n1.width - 1e-6, "contain right");
        assert!(a.y <= n1.y + 1e-6, "contain top");
        assert!(a.y + a.height >= n1.y + n1.height - 1e-6, "contain bottom");
    }
}
