//! 顶层分组宽度策略：fit（内容贴合）与 uniform（等宽阶段条带）

use crate::ast::Diagram;

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

/// 从 diagram 属性 `group_sizing` 读取策略
pub fn parse_group_sizing(diagram: &Diagram) -> GroupSizingPolicy {
    for attr in &diagram.attributes {
        if attr.key == "group_sizing" {
            if let Some(v) = attr.value.as_str() {
                return match v.trim().to_ascii_lowercase().as_str() {
                    "uniform" => GroupSizingPolicy::Uniform,
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

/// 与 [`crate::layout::node::common::group_bounds::GroupPadding`] 对齐的 padding 参数
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct GroupPaddingLike {
    pub x: f64,
    pub y_top: f64,
    pub x_delta: f64,
    pub y_delta: f64,
}

#[allow(dead_code)]
impl GroupPaddingLike {
    pub fn architecture_v2() -> Self {
        Self {
            x: 28.0,
            y_top: 48.0,
            x_delta: 56.0,
            y_delta: 76.0,
        }
    }

    pub fn from_group_padding(padding: f64, header_height: f64) -> Self {
        let p = crate::layout::node::common::group_bounds::GroupPadding::uniform(padding, header_height);
        Self {
            x: p.x,
            y_top: p.y_top,
            x_delta: p.x_delta,
            y_delta: p.y_delta,
        }
    }

    #[allow(dead_code)]
    pub fn default_sugiyama(group_padding: f64) -> Self {
        Self::from_group_padding(group_padding, 16.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestBlock {
        id: String,
        is_group: bool,
        width: f64,
        node_x: Vec<f64>,
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

    #[test]
    fn uniform_stretches_and_centers() {
        let mut blocks = vec![
            TestBlock {
                id: "wide".to_string(),
                is_group: true,
                width: 300.0,
                node_x: vec![50.0, 200.0],
            },
            TestBlock {
                id: "narrow".to_string(),
                is_group: true,
                width: 120.0,
                node_x: vec![10.0],
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
}
