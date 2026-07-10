//! 顶层分组宽度策略：fit（内容贴合）与 uniform（等宽阶段条带）

use crate::ast::Diagram;
use std::collections::HashMap;

/// 图级分组宽度策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupSizingPolicy {
    /// 组宽 = 组内内容 + padding（默认）
    Fit,
    /// 所有顶层 group 拉齐到最宽者，组内内容水平居中
    Uniform,
}

pub const VALID_GROUP_SIZING: &[&str] = crate::types::attr_constants::group_sizing::ALL;

pub fn is_valid_group_sizing_atom(raw: &str) -> bool {
    let normalized = raw.trim().to_ascii_lowercase();
    crate::types::attr_constants::group_sizing::ALL.contains(&normalized.as_str())
}

/// 从 diagram 属性 `group_sizing` 读取策略。
///
/// Phase C：默认 `Fit`（内容贴合），避免窄组被最宽组横向拉空；
/// 显式 `uniform` 才拉齐等宽条带。
pub fn parse_group_sizing(diagram: &Diagram) -> GroupSizingPolicy {
    for attr in &diagram.attributes {
        if attr.key == "group_sizing" {
            if let Some(v) = attr.value.as_str() {
                return match v.trim().to_ascii_lowercase().as_str() {
                    "uniform" => GroupSizingPolicy::Uniform,
                    "fit" => GroupSizingPolicy::Fit,
                    _ => GroupSizingPolicy::Fit,
                };
            }
        }
    }
    GroupSizingPolicy::Fit
}

/// 组块 trait：供 uniform 策略调整宽度（与 two_phase::MacroBlock 对齐）
pub trait GroupWidthBlock {
    fn block_id(&self) -> &str;
    fn is_group_block(&self) -> bool;
    fn block_width(&self) -> f64;
    fn set_block_width(&mut self, width: f64);
    fn shift_intra_nodes_x(&mut self, delta: f64);
}

/// 组块 trait：支持宽高双向 equal sizing（嵌套 sibling 用）
pub trait GroupSizeBlock: GroupWidthBlock {
    fn block_height(&self) -> f64;
    fn set_block_height(&mut self, height: f64);
    fn shift_intra_nodes_y(&mut self, delta: f64);
}

/// 将所有顶层 group 块拉齐到最宽宽度，组内节点水平居中
pub fn apply_uniform_group_width<B: GroupWidthBlock>(
    top_group_ids: &[String],
    blocks: &mut [B],
) {
    let top_set: std::collections::HashSet<&str> =
        top_group_ids.iter().map(|s| s.as_str()).collect();

    let max_width = blocks
        .iter()
        .filter(|b| b.is_group_block() && top_set.contains(b.block_id()))
        .map(|b| b.block_width())
        .fold(0.0_f64, f64::max);

    if max_width <= f64::EPSILON {
        return;
    }

    for block in blocks.iter_mut() {
        if !block.is_group_block() || !top_set.contains(block.block_id()) {
            continue;
        }
        let extra = max_width - block.block_width();
        if extra <= f64::EPSILON {
            continue;
        }
        block.set_block_width(max_width);
        block.shift_intra_nodes_x(extra / 2.0);
    }
}

pub fn apply_group_sizing_policy<B: GroupWidthBlock>(
    policy: GroupSizingPolicy,
    top_group_ids: &[String],
    blocks: &mut [B],
) {
    match policy {
        GroupSizingPolicy::Fit => {}
        GroupSizingPolicy::Uniform => apply_uniform_group_width(top_group_ids, blocks),
    }
}

/// 架构图嵌套 sibling：同一 macro rank 内的 group 块拉齐到最宽/最高，内容居中。
///
/// 确定性：rank 升序、块 id 升序迭代。
pub fn apply_equal_sibling_dimensions_per_rank<B: GroupSizeBlock>(
    macro_ranks: &HashMap<String, usize>,
    blocks: &mut [B],
) {
    let mut by_rank: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, block) in blocks.iter().enumerate() {
        if !block.is_group_block() {
            continue;
        }
        let rank = macro_ranks.get(block.block_id()).copied().unwrap_or(0);
        by_rank.entry(rank).or_default().push(i);
    }

    let mut ranks: Vec<usize> = by_rank.keys().copied().collect();
    ranks.sort_unstable();

    for rank in ranks {
        let Some(indices) = by_rank.get(&rank) else {
            continue;
        };
        if indices.len() < 2 {
            continue;
        }

        let max_width = indices
            .iter()
            .map(|&i| blocks[i].block_width())
            .fold(0.0_f64, f64::max);
        let max_height = indices
            .iter()
            .map(|&i| blocks[i].block_height())
            .fold(0.0_f64, f64::max);

        if max_width <= f64::EPSILON && max_height <= f64::EPSILON {
            continue;
        }

        for &i in indices {
            let extra_w = max_width - blocks[i].block_width();
            if extra_w > f64::EPSILON {
                blocks[i].set_block_width(max_width);
                blocks[i].shift_intra_nodes_x(extra_w / 2.0);
            }
            let extra_h = max_height - blocks[i].block_height();
            if extra_h > f64::EPSILON {
                blocks[i].set_block_height(max_height);
                blocks[i].shift_intra_nodes_y(extra_h / 2.0);
            }
        }
    }
}

/// 与 [`crate::layout::node::common::group_bounds::GroupPadding`] 对齐的 padding 参数
pub type GroupPaddingLike = crate::layout::node::common::group_bounds::GroupPadding;

#[cfg(test)]
mod tests {
    use super::*;

    struct TestBlock {
        id: String,
        is_group: bool,
        width: f64,
        height: f64,
        node_x: Vec<f64>,
        node_y: Vec<f64>,
    }

    impl GroupWidthBlock for TestBlock {
        fn block_id(&self) -> &str {
            &self.id
        }
        fn is_group_block(&self) -> bool {
            self.is_group
        }
        fn block_width(&self) -> f64 {
            self.width
        }
        fn set_block_width(&mut self, width: f64) {
            self.width = width;
        }
        fn shift_intra_nodes_x(&mut self, delta: f64) {
            for x in &mut self.node_x {
                *x += delta;
            }
        }
    }

    impl GroupSizeBlock for TestBlock {
        fn block_height(&self) -> f64 {
            self.height
        }
        fn set_block_height(&mut self, height: f64) {
            self.height = height;
        }
        fn shift_intra_nodes_y(&mut self, delta: f64) {
            for y in &mut self.node_y {
                *y += delta;
            }
        }
    }

    #[test]
    fn uniform_stretches_and_centers() {
        let mut blocks = vec![
            TestBlock {
                id: "wide".to_string(),
                is_group: true,
                width: 300.0,
                height: 200.0,
                node_x: vec![50.0, 200.0],
                node_y: vec![10.0],
            },
            TestBlock {
                id: "narrow".to_string(),
                is_group: true,
                width: 120.0,
                height: 200.0,
                node_x: vec![10.0],
                node_y: vec![10.0],
            },
        ];
        apply_uniform_group_width(
            &["wide".to_string(), "narrow".to_string()],
            &mut blocks,
        );
        assert!((blocks[0].width - 300.0).abs() < f64::EPSILON);
        assert!((blocks[1].width - 300.0).abs() < f64::EPSILON);
        assert!((blocks[1].node_x[0] - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn equal_sibling_dimensions_per_rank() {
        let mut blocks = vec![
            TestBlock {
                id: "a".to_string(),
                is_group: true,
                width: 400.0,
                height: 240.0,
                node_x: vec![0.0],
                node_y: vec![0.0],
            },
            TestBlock {
                id: "b".to_string(),
                is_group: true,
                width: 200.0,
                height: 120.0,
                node_x: vec![0.0],
                node_y: vec![0.0],
            },
        ];
        let mut ranks = HashMap::new();
        ranks.insert("a".to_string(), 0);
        ranks.insert("b".to_string(), 0);
        apply_equal_sibling_dimensions_per_rank(&ranks, &mut blocks);
        assert!((blocks[0].width - 400.0).abs() < f64::EPSILON);
        assert!((blocks[1].width - 400.0).abs() < f64::EPSILON);
        assert!((blocks[0].height - 240.0).abs() < f64::EPSILON);
        assert!((blocks[1].height - 240.0).abs() < f64::EPSILON);
        assert!((blocks[1].node_x[0] - 100.0).abs() < f64::EPSILON);
        assert!((blocks[1].node_y[0] - 60.0).abs() < f64::EPSILON);
    }
}
