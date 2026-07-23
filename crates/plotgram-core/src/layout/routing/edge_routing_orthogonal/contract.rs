//! Port-Stub-Approach 三段几何契约的**只读诊断**（方案 A-0）。
//!
//! 对应 [`docs/方案计划/边路由与标签问题通盘修复方案-2026-07.md`] 的 A-0 步骤：
//! 在不修改任何几何的前提下，统计三类契约的违反频度，作为后续 A-1~A-5
//! 改善效果的量化基线（手册 ★4「退化要可量化」）。
//!
//! 三条契约：
//! 1. **Stub（出段）**：跨组边首段须先出源组外边界才允许转弯（ISS-001）。
//! 2. **Approach（入段）**：末段方向须与目标端口方向一致（sanitize 已强制，恒为 0）。
//! 3. **Middle（中段）**：路径段不应增大到目标的曼哈顿距离（ISS-008/009c 绕远）。
//!
//! 另加一个端口质量信号（ISS-002 箭头倒悬的直接度量）：
//! 4. **unnatural_to_port**：目标端口不是源→目标相对位置下的“自然入边侧”。
//!    TB 图中目标在源下方（含对角）时自然入边侧为 Top（箭头朝下）；若实际选了
//!    Left/Right（箭头朝侧）则计为不自然。该信号为软度量（不阻碍），供 A-3/A-5 对比。
//!
//! 本模块**只计数、不改几何**；A-3 将在此基础上把契约升级为 sanitize 校验。
//!
//! 确定性（AGENTS.md §2）：仅按 `edges`/`relations` 的索引序遍历，
//! HashMap 只作查询，不参与迭代驱动。

use crate::ast::Relation;
use crate::layout::edge::common::edge_geometry::{node_center, port_direction};
use crate::layout::geometry::Point;
use crate::layout::group::GroupRoutingContext;
use crate::layout::{EdgeLayout, NodeLayout, Port};
use std::collections::HashMap;

/// 判定"点在矩形内"时向内收缩的边距（避免把恰好落在边界上的点误判为组内）。
const INSIDE_MARGIN: f64 = 0.5;

/// 判定"远离"的曼哈顿距离增量阈值（吸收浮点噪声）。
const AWAY_EPS: f64 = 0.5;

/// 三段契约的只读诊断结果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractDiagnostics {
    /// 跨组边中，首个转弯点仍落在源组边界内的边数（stub 未出组，ISS-001）。
    pub stub_violations: usize,
    /// 末段方向与目标端口方向不一致的边数（approach 违约）。
    /// 注：sanitize 已强制末段贴合端口，该计数恒为 0；保留以监控 sanitize 回归。
    /// ISS-002 的真正根因是端口选错方向，由 [`Self::unnatural_to_port`] 捕获。
    pub approach_violations: usize,
    /// 目标端口不是源→目标相对位置下自然入边侧的边数（箭头“倒悬”，ISS-002 直接信号）。
    pub unnatural_to_port: usize,
    /// 使到目标曼哈顿距离增大的段的总数（远离段，ISS-008/009c）。
    pub away_segments: usize,
    /// 含至少一个远离段的边数。
    pub away_edges: usize,
}

/// 对最终路由几何做三段契约诊断（只读）。
///
/// `edges` 与 `relations` 按索引一一对应。
pub fn diagnose_contract_violations(
    relations: &[Relation],
    nodes: &HashMap<String, NodeLayout>,
    edges: &[EdgeLayout],
    group_ctx: &GroupRoutingContext,
    horizontal: bool,
) -> ContractDiagnostics {
    let mut diag = ContractDiagnostics::default();
    if std::env::var("PLOTGRAM_NODES_DEBUG")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        let mut ids: Vec<&String> = nodes.keys().collect();
        ids.sort();
        for id in ids {
            let nl = &nodes[id];
            eprintln!(
                "[nodes_dbg] {:16} bbox=({:.0},{:.0})-({:.0},{:.0}) center=({:.0},{:.0})",
                id,
                nl.x,
                nl.y,
                nl.x + nl.width,
                nl.y + nl.height,
                nl.x + nl.width / 2.0,
                nl.y + nl.height / 2.0
            );
        }
    }
    for (i, edge) in edges.iter().enumerate() {
        let Some(rel) = relations.get(i) else {
            continue;
        };
        if edge.path_is_empty() {
            continue;
        }
        let points = edge.path_points();
        if points.len() < 2 {
            continue;
        }

        // 契约①：跨组边首段须出组后才转弯。
        if stub_turns_inside_source_group(rel.from.as_str(), rel.to.as_str(), &points, group_ctx)
        {
            diag.stub_violations += 1;
        }

        // 契约②：末段方向须与目标端口方向一致。
        if !approach_direction_matches(&points, edge.to_port) {
            diag.approach_violations += 1;
        }

        // 契约②'（ISS-002 直接信号）：目标端口应为相对位置下的自然入边侧。
        if let (Some(from_nl), Some(to_nl)) =
            (nodes.get(rel.from.as_str()), nodes.get(rel.to.as_str()))
        {
            let s = node_center(from_nl);
            let t = node_center(to_nl);
            let (dx, dy) = (t.x - s.x, t.y - s.y);
            let natural = natural_to_port(dx, dy, horizontal);
            let matches = match natural {
                Some(p) => edge.to_port == p,
                None => true, // 位置退化（几乎重合），不判
            };
            if contract_debug_enabled() {
                let path_str: Vec<String> = points
                    .iter()
                    .map(|p| format!("({:.0},{:.0})", p.x, p.y))
                    .collect();
                eprintln!(
                    "[contract_dbg] {} -> {} | from_port={:?} to_port={:?} natural={:?} ok={}\n    path: {}",
                    rel.from, rel.to, edge.from_port, edge.to_port, natural, matches,
                    path_str.join(" -> ")
                );
            }
            if !matches {
                diag.unnatural_to_port += 1;
            }
        }

        // 契约③：统计远离目标的段。
        if let Some(to_nl) = nodes.get(rel.to.as_str()) {
            let target = node_center(to_nl);
            let away = count_away_segments(&points, target);
            if away > 0 {
                diag.away_segments += away;
                diag.away_edges += 1;
            }
        }
    }
    diag
}

/// 契约①：跨组边的首个转弯点是否仍在源组边界内。
///
/// 仅对"源在组内、目标不在同一叶子组"的跨组边判定；同组边与无组边豁免。
/// 直线路径（无转弯）视为满足。
fn stub_turns_inside_source_group(
    from_id: &str,
    to_id: &str,
    points: &[Point],
    group_ctx: &GroupRoutingContext,
) -> bool {
    let Some(from_leaf) = group_ctx.node_leaf_group.get(from_id) else {
        return false; // 源不在任何组内，无出组契约
    };
    if group_ctx.node_leaf_group.get(to_id) == Some(from_leaf) {
        return false; // 同叶子组内部边，无需出组
    }
    let Some(group) = group_ctx.groups.get(from_leaf) else {
        return false;
    };
    if points.len() < 3 {
        return false; // 直线，无转弯
    }
    // 找首个转弯点（方向改变处）。
    for i in 1..points.len() - 1 {
        let d_in = seg_dir(&points[i - 1], &points[i]);
        let d_out = seg_dir(&points[i], &points[i + 1]);
        if d_in != (0, 0) && d_out != (0, 0) && d_in != d_out {
            return point_inside_rect(&points[i], group.x, group.y, group.width, group.height);
        }
    }
    false
}

/// 契约②：末段方向是否与目标端口的入向一致。
///
/// 目标端口 `Top` 的入向为向下 `(0,+1)`，即 `-port_direction(to_port)`。
fn approach_direction_matches(points: &[Point], to_port: Port) -> bool {
    let Some(actual) = last_nonzero_dir(points) else {
        return true; // 无有效段，不判违约
    };
    let out = port_direction(to_port);
    let expected = dir_of(-out.x, -out.y);
    actual == expected
}

/// 契约②'（ISS-002）：给定源→目标的相对位移，返回“自然入边侧”。
///
/// 原则：箭头（末段方向）应顺着主流动方向。
/// - **TB 图**：竖直优先。目标在源下方（含对角，|dx| ≤ 2·dy）→ Top（箭头朝下）；
///   目标在上方→ Bottom；仅当水平偏移显著（|dx| > 2·|dy|）才取侧向端口。
/// - **LR 图**：水平优先，规则对偶。
/// 返回 `None` 表示位置退化（几乎重合），不判定。
///
/// 该启发式是软度量：实际路由可能因避障/并线合理偏离，计数仅供改善对比。
fn natural_to_port(dx: f64, dy: f64, horizontal: bool) -> Option<Port> {
    const SIDE_DOMINANCE: f64 = 2.0;
    if dx.abs() < AWAY_EPS && dy.abs() < AWAY_EPS {
        return None;
    }
    if horizontal {
        // LR：水平为主轴
        if dx > 0.0 && dx.abs() >= dy.abs() / SIDE_DOMINANCE {
            Some(Port::Left) // 目标在右，箭头朝右入 Left
        } else if dx < 0.0 && dx.abs() >= dy.abs() / SIDE_DOMINANCE {
            Some(Port::Right)
        } else if dy > 0.0 {
            Some(Port::Top)
        } else {
            Some(Port::Bottom)
        }
    } else {
        // TB：竖直为主轴
        if dy > 0.0 && dy.abs() >= dx.abs() / SIDE_DOMINANCE {
            Some(Port::Top) // 目标在下（含对角），箭头朝下入 Top
        } else if dy < 0.0 && dy.abs() >= dx.abs() / SIDE_DOMINANCE {
            Some(Port::Bottom)
        } else if dx > 0.0 {
            Some(Port::Left)
        } else {
            Some(Port::Right)
        }
    }
}

/// 契约③：统计使到目标曼哈顿距离增大的段数。
fn count_away_segments(points: &[Point], target: Point) -> usize {
    let mut count = 0usize;
    for i in 1..points.len() {
        let d_start = manhattan(&points[i - 1], &target);
        let d_end = manhattan(&points[i], &target);
        if d_end > d_start + AWAY_EPS {
            count += 1;
        }
    }
    count
}

/// 取路径最后一段非零长度的方向（离散化为带符号单位向量）。
fn last_nonzero_dir(points: &[Point]) -> Option<(i32, i32)> {
    for i in (1..points.len()).rev() {
        let d = seg_dir(&points[i - 1], &points[i]);
        if d != (0, 0) {
            return Some(d);
        }
    }
    None
}

/// 线段方向离散化：主轴分量的符号（正交路径只有 ±1 单轴方向）。
fn seg_dir(a: &Point, b: &Point) -> (i32, i32) {
    dir_of(b.x - a.x, b.y - a.y)
}

fn dir_of(dx: f64, dy: f64) -> (i32, i32) {
    if dx.abs() > dy.abs() {
        (sign(dx), 0)
    } else if dy.abs() > dx.abs() {
        (0, sign(dy))
    } else {
        (0, 0)
    }
}

fn sign(v: f64) -> i32 {
    if v > 0.0 {
        1
    } else if v < 0.0 {
        -1
    } else {
        0
    }
}

fn manhattan(p: &Point, q: &Point) -> f64 {
    (p.x - q.x).abs() + (p.y - q.y).abs()
}

/// 临时调试开关：PLOTGRAM_CONTRACT_DEBUG=1 时打印逐边端口/坐标明细。
fn contract_debug_enabled() -> bool {
    std::env::var("PLOTGRAM_CONTRACT_DEBUG")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// 点是否严格位于矩形内部（向内收缩 `INSIDE_MARGIN`）。
fn point_inside_rect(p: &Point, x: f64, y: f64, w: f64, h: f64) -> bool {
    p.x > x + INSIDE_MARGIN
        && p.x < x + w - INSIDE_MARGIN
        && p.y > y + INSIDE_MARGIN
        && p.y < y + h - INSIDE_MARGIN
}
