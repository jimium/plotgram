//! 路由后处理模块：归集 PRS 扩壳、边框排斥、路由后钩子。
//!
//! 替代旧的 `post_route_hook.rs` / `group/post_route_shell.rs` / `edge_postprocess.rs` 三处散落。

pub mod hook;
pub(super) mod shell_expand;
pub(super) mod border_repulse;

pub use hook::{NODE_MOVE_REROUTE_EPS, MIN_PRESERVE_RATIO};
pub(crate) use hook::{AlgoProfile, NoopPostRouteHook, ArchitecturePostRouteHook, PostRouteHook};
pub use shell_expand::post_route_shell_expand;
pub use border_repulse::{repulse_edges_only, snap_and_repulse_edges, snap_and_repulse_edges_with_guard};
