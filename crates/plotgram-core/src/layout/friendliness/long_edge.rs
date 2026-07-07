//! 长边跨层度（Sugiyama 专属）
//!
//! 在分层布局中，跨越多层的边需要绕行或拆分，路由友好度低。
//! 需要 `LayoutHints.sugiyama_ranks` 提供 rank 信息。

use crate::ast::Diagram;
use crate::layout::LayoutResult;

/// 长边跨层度评估结果
#[derive(Debug, Clone)]
pub struct LongEdgeResult {
    /// 跨层边数（rank 跨度 > 1 的边数）
    pub count: usize,
    /// 总跨层深度（Σ (span - 1)）
    pub total_span: usize,
    /// 热点边索引
    pub edge_indices: Vec<usize>,
}

/// 计算长边跨层度
pub fn evaluate(diagram: &Diagram, result: &LayoutResult) -> LongEdgeResult {
    let Some(ranks) = &result.hints.sugiyama_ranks else {
        return LongEdgeResult {
            count: 0,
            total_span: 0,
            edge_indices: vec![],
        };
    };

    let mut count = 0;
    let mut total_span = 0;
    let mut edge_indices = Vec::new();

    for (i, rel) in diagram.relations.iter().enumerate() {
        match (ranks.get(rel.from.as_str()), ranks.get(rel.to.as_str())) {
            (Some(&r1), Some(&r2)) => {
                let span = r1.abs_diff(r2);
                if span > 1 {
                    count += 1;
                    total_span += span - 1;
                    edge_indices.push(i);
                }
            }
            _ => {}
        }
    }

    LongEdgeResult {
        count,
        total_span,
        edge_indices,
    }
}
