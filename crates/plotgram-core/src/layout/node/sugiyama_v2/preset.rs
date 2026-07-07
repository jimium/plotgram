//! Sugiyama v2 图类型预设：门面算法共享引擎，仅参数与节点尺寸策略不同。
//!
//! [`NodeSizing`] 枚举已抽取到 [`common::node_sizing`]，供所有算法共享。

use crate::layout::LayoutResult;
use crate::layout::node::common::node_sizing::NodeSizing;

/// Sugiyama v2 布局参数集。
#[derive(Debug, Clone, Copy)]
pub struct SugiyamaPreset {
    pub default_node_width: f64,
    pub default_node_height: f64,
    pub padding: f64,
    pub layer_gap: f64,
    pub node_gap: f64,
    pub ordering_sweeps: usize,
    pub dummy_node_width: f64,
    pub dummy_node_height: f64,
    pub node_sizing: NodeSizing,
    /// 布局完成后可选调整（如 ER 整体平移对齐边距）。
    pub finish_layout: Option<fn(&mut LayoutResult, &SugiyamaPreset)>,
    /// Phase 3：长边跨层惩罚权重。
    ///
    /// 在 barycenter 评分中，dummy 邻居（长边段）的权重倍数。
    /// 1.0 = 无惩罚（与真节点同权）；> 1.0 = 鼓励长边 dummy 链竖直对齐，
    /// 减少水平偏移从而缩短边总长。
    pub long_edge_barycenter_weight: f64,
}

impl SugiyamaPreset {
    pub const fn default_node_size(self) -> (f64, f64) {
        (self.default_node_width, self.default_node_height)
    }

    pub const fn dummy_node_size(self) -> (f64, f64) {
        (self.dummy_node_width, self.dummy_node_height)
    }
}

const BASE: SugiyamaPreset = SugiyamaPreset {
    default_node_width: 160.0,
    default_node_height: 50.0,
    padding: 40.0,
    layer_gap: 84.0,
    node_gap: 56.0,
    ordering_sweeps: 8,
    dummy_node_width: 12.0,
    dummy_node_height: 8.0,
    node_sizing: NodeSizing::Standard,
    finish_layout: None,
    long_edge_barycenter_weight: 1.0,
};

/// 流程图专用 preset（`layout_algo: flowchart`）。
///
/// 相比 BASE，调高 `long_edge_barycenter_weight`：流程图常含"判断节点跳层指向远端"
/// 的长边，加权使 dummy 链更竖直对齐，减少折弯。1.8 在 1.5~2.0 推荐区间内取中，
/// 兼顾对齐收益与避免过度聚集。
pub const FLOWCHART_PRESET: SugiyamaPreset = SugiyamaPreset {
    long_edge_barycenter_weight: 1.8,
    ..BASE
};

/// ER 图专用 preset（`layout_algo: er`）：更宽层间距与同层间距，实体尺寸策略。
pub const ER_PRESET: SugiyamaPreset = SugiyamaPreset {
    layer_gap: 96.0,
    node_gap: 64.0,
    node_sizing: NodeSizing::Er,
    finish_layout: Some(finish_er_layout),
    ..BASE
};

/// 通用 `sugiyama-v2` preset：参数中性，尺寸按 diagram 类型推断。
pub const GENERIC_PRESET: SugiyamaPreset = SugiyamaPreset {
    node_sizing: NodeSizing::InferFromDiagram,
    ..BASE
};

fn finish_er_layout(result: &mut LayoutResult, preset: &SugiyamaPreset) {
    super::postprocess::normalize_layout_result_to_padding(result, preset.padding);
}
