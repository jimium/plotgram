//! 端口修正阶段（Phase 4b）
//!
//! ## 写权契约
//! - 本阶段是 from_side / to_side / endpoint_map 的**最终写者**
//! - 后续阶段（reroute、lane、sanitize）只读端口，不修改
//! - 违反此契约的修改必须通过回归证明
//!
//! ## 合并来源
//! - 原 4b `replan_slots`：按实际出口方向全局重排 slot 锚点
//! - 原 4c `phase_straighten_align`：正对端口边的锚点对齐修正
//! - 原 4e `phase_stub_fix`：反向 stub 检测与端口翻转
//!
//! ## 设计理由
//! 1. 消除阶段间写权竞争（straighten 对齐后 stub_fix 可能翻转端口）
//! 2. 三阶段共享参数列表，合并后减少冗余传参
//! 3. stub_fix 移到 reroute 之前：初始路由已产生完整路径，足以检测反向 stub；
//!    翻转端口后 reroute 可使用正确端口方向

use super::super::*;
use super::refine::{phase_straighten_align, phase_stub_fix};
use crate::layout::edge::edge_routing_orthogonal::visibility_graph::OrthogonalVisibilityGraph;
use std::collections::{BTreeSet, HashMap, HashSet};

/// Phase 4b：端口修正（合并 slot 重排 + 对齐修正 + stub 翻转）
///
/// 子阶段按序执行：
/// 1. **Slot 重排**（原 replan_slots）：按实际出口方向全局重排 slot 锚点
/// 2. **对齐修正**（原 straighten_align）：正对端口边的锚点对齐
/// 3. **Stub 翻转**（原 stub_fix）：检测反向 stub 并翻转端口方向
///
/// 每个子阶段内部自行完成受影响边的重路由。
#[allow(clippy::too_many_arguments)]
pub(crate) fn phase_port_correction(
    nodes: &HashMap<String, NodeLayout>,
    relations: &[crate::ast::Relation],
    from_side: &mut [Port],
    to_side: &mut [Port],
    endpoint_map: &mut HashMap<(usize, bool), Endpoint>,
    edges: &mut Vec<EdgeLayout>,
    grid: &mut SegmentGrid,
    cfg: &OrthoConfig,
    group_ctx: &crate::layout::group::GroupRoutingContext,
    obstacles: &PreparedObstacles,
    corridor_plan: &corridor_route::CorridorRoutePlan,
    ortho_stats: &mut crate::layout::OrthoDebugStats,
    profile: &OrthoRoutingProfile,
    feedback_edge_set: &HashSet<usize>,
    reverse_pairs: &BTreeSet<String>,
    parallel: &crate::layout::edge::common::parallel_edges::ParallelGroups,
    space_budget: &Option<crate::layout::space_budget::SpaceBudget>,
    ovg: Option<&OrthogonalVisibilityGraph>,
) {
    let t_pc = crate::layout::perf::Instant::now();
    let n = edges.len();

    // ── 子阶段 1: Slot 重排（原 4b replan_slots）──
    // 按实际出口方向全局重排 slot 锚点，消除初始排序与路由方向不一致导致的交叉。
    replan_slots(
        nodes,
        relations,
        from_side,
        to_side,
        endpoint_map,
        edges,
        grid,
        cfg,
        group_ctx,
        obstacles,
        corridor_plan,
        ortho_stats,
        profile,
    );

    // ── 子阶段 2: 对齐修正（原 4c straighten_align）──
    // 正对端口边的 slot 锚点对齐修正，在 replan_slots 之后执行确保 anchor 是最终排序结果。
    phase_straighten_align(
        nodes,
        n,
        from_side,
        to_side,
        endpoint_map,
        edges,
        grid,
        relations,
        reverse_pairs,
        parallel,
        corridor_plan,
        group_ctx,
        obstacles,
        cfg,
        profile,
        space_budget,
        ovg,
    );

    // ── 子阶段 3: Stub 翻转（原 4e stub_fix）──
    // 检测反向 stub 与侧向接入，翻转/旋转端口方向并重路由。
    // 移到 reroute 之前：初始路由路径足以检测反向 stub，翻转后 reroute 使用正确端口。
    phase_stub_fix(
        nodes,
        relations,
        from_side,
        to_side,
        endpoint_map,
        edges,
        grid,
        cfg,
        group_ctx,
        obstacles,
        corridor_plan,
        ortho_stats,
        profile,
        feedback_edge_set,
        ovg,
    );

    crate::perf_log!(
        "[perf]     4b_port_correction: {:.2}ms (flipped {} edges)",
        t_pc.elapsed().as_secs_f64() * 1000.0,
        ortho_stats.flipped_stub_edges
    );
}
