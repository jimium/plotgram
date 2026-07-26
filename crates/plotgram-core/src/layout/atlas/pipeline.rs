//! `AtlasPipeline`（Stage 0 交付 0.3，23 号文 §2）。
//!
//! Atlas 三相管线（组合相 / 度量相 / 落笔相）的入口壳。**Stage 0 为空壳**：
//! 直接转发 legacy [`LayoutPipeline`](crate::layout::pipeline::runner)，
//! 保证 `PLOTGRAM_PIPELINE=atlas|shadow` 从第一天起就能出图（铁律 1）。
//! 后续 Stage 逐步接管：S2 度量相（无组图）→ S3 通道进度量相 →
//! S4 组合相真实现。每次接管在本文件替换对应阶段的转发。

use crate::ast::Diagram;
use crate::error::DiagnosticError;
use crate::layout::pipeline::plan::LayoutPlan;
use crate::layout::types::LayoutResult;

/// Atlas 管线入口（与 legacy `LayoutPipeline` 同签名，供 entry 分发）。
pub struct AtlasPipeline<'a> {
    diagram: &'a Diagram,
    plan: &'a LayoutPlan,
}

impl<'a> AtlasPipeline<'a> {
    pub fn new(diagram: &'a Diagram, plan: &'a LayoutPlan) -> Self {
        Self { diagram, plan }
    }

    pub fn run(self) -> Result<LayoutResult, DiagnosticError> {
        // Stage 0：转发 legacy。三相接管从 Stage 2 开始。
        crate::layout::pipeline::runner::LayoutPipeline::new(self.diagram, self.plan).run()
    }
}
