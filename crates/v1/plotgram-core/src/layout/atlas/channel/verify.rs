//! 路径级作用域自反证（22 号文 §8「构造保证的证明义务」/ 27 号文 §3.1 借道）。
//!
//! [`super::search::ScopeMask`] 是**构造机制**（搜索期硬过滤）；本检查器是它的
//! **独立证明义务**：不复用掩码的过滤代码路径（允许集由 groups 的 parent 链
//! 在此重新推导），对已产出的路径逐轨道、逐转移断言。任何非空返回都意味着
//! L8 机制回归（借道复活），探针以「恒 0」回归数字监控（A2 独立证据链）。
//!
//! 输入取 `&[TrackId]` + `&[GateId]` 而非 `RouteOutcome`：`Plan.channels` /
//! `Plan.gates` 的条目可直接复验（同一份决策，两处一个口径）。

use super::substrate::{GateId, GroupId, Substrate, TrackId};
use std::collections::BTreeSet;

/// 路径作用域违规项。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteScopeViolation {
    /// 路径引用了基底中不存在的轨道。
    UnknownTrack(TrackId),
    /// 轨道属于与两端无关的组（借道穿组，A2 归零对象）。
    ForeignScope { track: TrackId, scope: GroupId },
    /// 相邻轨道 scope 变化，但 gates 序列中无对应闸口。
    MissingGate { from: TrackId, to: TrackId },
    /// 闸口与其对应的 scope 变化不匹配（该轨道对不在 gate 的 crossings 配对里）。
    GateMismatch {
        gate: GateId,
        from: TrackId,
        to: TrackId,
    },
    /// gates 序列比路径上的 scope 变化多出的闸口。
    UnexpectedGate(GateId),
}

/// 对一条已选路径做作用域断言（独立于 `ScopeMask`，见模块头）。
///
/// 检查三件事：
/// 1. 每条轨道的 scope ∈ `{None} ∪ chain(u_scope) ∪ chain(v_scope)`
///    （链在此沿 parent 手动上溯，不经掩码/`scope_chain` 代码）；
/// 2. 相邻轨道每次 scope 变化都按顺序对应 `gates` 的一个闸口，且该轨道对
///    确实在闸口的 `crossings` 一一配对里（同 scope 转移是 link，不耗 gate）；
/// 3. `gates` 无多余项。
///
/// 空路径（`Infeasible` 结果）自然通过（`gates` 也须为空）。
pub fn verify_route_scope(
    substrate: &Substrate,
    tracks: &[TrackId],
    gates: &[GateId],
    u_scope: Option<GroupId>,
    v_scope: Option<GroupId>,
) -> Vec<RouteScopeViolation> {
    let mut violations = Vec::new();

    // 允许集：独立推导（手动沿 parent 上溯）
    let mut allowed: BTreeSet<Option<GroupId>> = BTreeSet::new();
    allowed.insert(None);
    for start in [u_scope, v_scope] {
        let mut cur = start;
        while let Some(g) = cur {
            allowed.insert(Some(g));
            cur = substrate.group(g).and_then(|s| s.parent);
        }
    }

    // 1. 逐轨道解析 + scope 检查
    let mut scopes: Vec<(TrackId, Option<GroupId>)> = Vec::with_capacity(tracks.len());
    for &t in tracks {
        match substrate.track(t) {
            None => violations.push(RouteScopeViolation::UnknownTrack(t)),
            Some(tr) => scopes.push((t, tr.scope)),
        }
    }
    if scopes.len() != tracks.len() {
        return violations; // 有未知轨道：转移检查失真，只报解析违规
    }
    for &(t, scope) in &scopes {
        if let Some(g) = scope {
            if !allowed.contains(&Some(g)) {
                violations.push(RouteScopeViolation::ForeignScope { track: t, scope: g });
            }
        }
    }

    // 2. scope 变化 ↔ gates 顺序一一配对
    let mut gate_iter = gates.iter();
    for w in scopes.windows(2) {
        let ((a, sa), (b, sb)) = (w[0], w[1]);
        if sa == sb {
            continue; // 同 scope link 转移（跨 scope link 构建期已拒绝）
        }
        match gate_iter.next() {
            None => violations.push(RouteScopeViolation::MissingGate { from: a, to: b }),
            Some(&g) => {
                let paired = substrate.gate(g).is_some_and(|gate| {
                    gate.crossings
                        .iter()
                        .any(|&(i, o)| (i, o) == (a, b) || (i, o) == (b, a))
                });
                if !paired {
                    violations.push(RouteScopeViolation::GateMismatch {
                        gate: g,
                        from: a,
                        to: b,
                    });
                }
            }
        }
    }

    // 3. 多余闸口
    for &g in gate_iter {
        violations.push(RouteScopeViolation::UnexpectedGate(g));
    }

    violations
}
