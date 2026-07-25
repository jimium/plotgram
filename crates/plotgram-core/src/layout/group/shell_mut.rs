//! G4：route 前壳预算令牌——只抬 RouteDemand / side_gutters，不写 groups。
//!
//! 历史：Phase 6 曾允许 orthosketch / post_route 扩写 `layout.groups`。
//! G4 起生产路径禁止经本模块改组几何。

use crate::ast::Diagram;
use crate::layout::LayoutResult;

/// 壳预算令牌（layout finalize → freeze 前持有）。
pub struct GroupShellMut<'a> {
    diagram: &'a Diagram,
    layout: &'a mut LayoutResult,
}

impl<'a> GroupShellMut<'a> {
    pub fn new(diagram: &'a Diagram, layout: &'a mut LayoutResult) -> Self {
        Self { diagram, layout }
    }

    /// 曼哈顿折线估溢出 → 只写入 `side_gutters`（不计 group 写权）。
    pub fn feedforward_orthosketch(&mut self) -> bool {
        crate::layout::post_route::shell_expand::feedforward_shell_from_orthosketch(
            self.diagram,
            self.layout,
        )
    }
}
