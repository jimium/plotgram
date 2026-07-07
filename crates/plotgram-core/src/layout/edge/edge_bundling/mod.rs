//! Edge Bundling（边捆绑）模块
//!
//! 将几何上相似、方向相近的边「捆成一束」共享路径，在保持边端点连接正确的前提下
//! 减少视觉交叉、强化流向感知、降低 Ink 量。
//!
//! 详见 `docs/architecture/布局优化/edge-bundling-research.md`。
//!
//! ## 算法流水线（§4.2）
//!
//! ```text
//! Step 1: 路径分段与特征提取          → compatibility::decompose_path / EdgeFeatures::extract
//!     ↓
//! Step 2: 边兼容性评估（构建兼容图）  → compatibility::compute_compatibility
//!     ↓
//! Step 3: 边聚类                      → clustering（P1）
//!     ↓
//! Step 4: 通道分配（Trunk 定位）      → trunk（P1）
//!     ↓
//! Step 5: 分叉点计算                  → trunk（P1）
//!     ↓
//! Step 6: 路径重写                    → path_rewrite（P2）
//!     ↓
//! Step 7: 重叠惩罚豁免 + 微调         → 后处理（P2/P3）
//! ```
//!
//! ## 确定性约定
//!
//! 按 [AGENTS.md](../../../../../AGENTS.md) 要求：
//! - 不依赖 HashMap 的 key 排序驱动迭代顺序
//! - 所有排序使用显式全序 key（含 tiebreaker）
//! - 所有数值计算确定性（无随机数）

pub mod compatibility;
pub mod clustering;
pub mod label_placement;
pub mod path_rewrite;
pub mod trunk;
pub mod types;

pub use compatibility::{compute_compatibility, decompose_path, CompatibilityBucket, EdgeFeatures};
pub use clustering::{cluster_edges, BundleCandidate};
pub use path_rewrite::{apply_bundling, rewrite_bundle_paths};
pub use trunk::allocate_trunks;
pub use types::{
    Axis, BundlingConfig, BundlingResult, EdgeBundle, EdgeBundlingDebugStats,
    EdgeBundlingHints, EdgePathRoles, LabelBundlePolicy, PathSegment, SegmentDirection,
    SegmentRole, SegmentSpan, TrunkKeepout,
};
