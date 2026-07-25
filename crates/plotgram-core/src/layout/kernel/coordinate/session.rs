//! G3：单次布局会话。
//!
//! 对外一次 [`LayoutSession::materialize`]；内部允许 Cross→Main 交替投影。
//! 组框写入 `GroupTable` 的权威通道：本模块（或等价的单次 `compute_group_bounds`）。

use std::collections::HashMap;

use crate::ast::Diagram;
use crate::layout::kernel::group::bounds::{compute_group_bounds, GroupPadding};
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

    /// G3 生产主路径：朴素容器，单次 `compute_group_bounds`（计 1 次写）。
    pub fn materialize_plain(self) -> LayoutSolution {
        let groups = compute_group_bounds(self.diagram, self.nodes, self.padding);
        LayoutSolution {
            groups: groups.into(),
        }
    }

    /// 在朴素种子上做 Cross→Main IR 投影后写回（仍只计一次 `compute_group_bounds` 写；
    /// refine 原地改边界，不另开写权站点）。
    pub fn materialize(self) -> LayoutSolution {
        let mut groups = compute_group_bounds(self.diagram, self.nodes, self.padding);
        if self.diagram.groups.is_empty() || self.nodes.is_empty() {
            return LayoutSolution {
                groups: groups.into(),
            };
        }
        refine_group_frames(
            self.diagram,
            self.nodes,
            &mut groups,
            self.padding,
            self.sibling_gap,
            SolveAxis::Cross,
        );
        refine_group_frames(
            self.diagram,
            self.nodes,
            &mut groups,
            self.padding,
            self.sibling_gap,
            SolveAxis::Main,
        );
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
