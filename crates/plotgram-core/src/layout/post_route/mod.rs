//! 路由后处理模块：归集 PRS 扩壳、边框排斥。
//!
//! 替代旧的 `post_route_hook.rs` / `group/post_route_shell.rs` / `edge_postprocess.rs` 三处散落。

pub(super) mod shell_expand;
pub(super) mod border_repulse;

/// 可保留边占比低于该阈值时退回全图重路由（Slice F2c：增量复用门槛）。
pub const MIN_PRESERVE_RATIO: f64 = 0.10;

pub use shell_expand::post_route_shell_expand;
pub use border_repulse::{repulse_edges_only, snap_and_repulse_edges, snap_and_repulse_edges_with_guard};
