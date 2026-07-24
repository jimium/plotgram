//! 空间契约（Space Contract）：布局预留、路由消费、后处理守约。
//!
//! 同层节点缝、有 label 的水平边、端口 clearance 在布局阶段显式预算，
//! 避免末端「刚好不碰」式补丁。
//!
//! ## L3 迁移方向
//!
//! 本模块将逐步拆分为：
//! - [`SpacingDemandStore`](super::spacing_contract::SpacingDemandStore)：布局求解输入
//! - [`RoutingContract`](super::spacing_contract::RoutingContract)：路由只读消费
//!
//! 当前 `SpaceBudget` 保留为兼容层，提供 `to_spacing_demand()` 和 `to_routing_contract()` 拆分方法。

use crate::ast::Diagram;
use crate::layout::constants::{DEFAULT_LABEL_PADDING, GRID_SNAP_NODE_GAP_ARCH};
use crate::layout::routing::common::label_avoidance::estimate_label_width;
use crate::layout::group::constants::PORT_STUB_CLEARANCE;
use crate::layout::NodeLayout;
use std::collections::{BTreeMap, HashMap, HashSet};

pub use super::spacing_contract::{RoutingContract, SpacingDemandStore};

/// 默认同层节点间距（与 architecture NODE_GAP / grid snap 对齐）。
pub const DEFAULT_NODE_GAP: f64 = GRID_SNAP_NODE_GAP_ARCH;

/// 有 label 的边：标签宽 + 两侧 padding 后的最小缝。
const LABEL_GAP_PAD: f64 = DEFAULT_LABEL_PADDING * 2.0 + 8.0;

/// 全图空间预算：成对最小间距 + 端口 clearance + 走廊升档请求。
#[derive(Debug, Clone, Default)]
pub struct SpaceBudget {
    /// 无特殊边时的默认同层间距。
    pub default_node_gap: f64,
    /// 规范化 pair `(min_id, max_id)` → 最小边距（节点外缘到外缘）。
    pub pair_gaps: BTreeMap<(String, String), f64>,
    /// 端口外向 stub 长度。
    pub port_clearance: f64,
    /// 路由 0 候选时请求抬高走廊/车道预算（S2）。
    pub corridor_boost_requested: bool,
    /// D4 P0：竖直 rank 缝下界；`None` 时 `enforce_vertical_rank_gaps` 用 `default_node_gap`。
    pub min_vertical_rank_gap: Option<f64>,
}

impl SpaceBudget {
    pub fn new() -> Self {
        Self {
            default_node_gap: DEFAULT_NODE_GAP,
            pair_gaps: BTreeMap::new(),
            port_clearance: PORT_STUB_CLEARANCE,
            corridor_boost_requested: false,
            min_vertical_rank_gap: None,
        }
    }

    /// 从 diagram relations 构建：有 label 的边抬高两端点最小缝。
    pub fn from_diagram(diagram: &Diagram) -> Self {
        let mut budget = Self::new();
        let mut rels: Vec<(&str, &str, Option<&str>)> = diagram
            .relations
            .iter()
            .map(|r| (r.from.as_str(), r.to.as_str(), r.label.as_deref()))
            .collect();
        // 确定性：按端点 id 排序
        rels.sort_by(|a, b| a.0.cmp(b.0).then(a.1.cmp(b.1)).then(a.2.cmp(&b.2)));

        for (from, to, label) in rels {
            if from == to {
                continue;
            }
            let mut gap = budget.default_node_gap;
            if let Some(text) = label {
                if !text.is_empty() {
                    let label_w = estimate_label_width(text) + LABEL_GAP_PAD;
                    gap = gap.max(label_w);
                }
            }
            budget.set_pair_gap(from, to, gap);
        }
        budget
    }

    /// 无组 architecture：把同排相邻节点的 demand 缝写入契约，防止 refine 压回 `default_node_gap`。
    pub fn enrich_adjacent_rank_demand(
        &mut self,
        layers: &[Vec<String>],
        nodes: &HashMap<String, NodeLayout>,
        diagram: &Diagram,
    ) {
        let profile = crate::layout::demand::band::EdgeBandDemandProfile::for_diagram(
            diagram.diagram_type.clone(),
            !diagram.groups.is_empty(),
        );
        if profile.horizontal_max_extra <= 0.0 {
            return;
        }
        let parallel_gap = crate::layout::routing::segment_pair::parallel_gap_for_diagram(
            diagram.diagram_type.clone(),
        );
        for layer in layers {
            if layer.len() < 2 {
                continue;
            }
            let mut ordered: Vec<&String> = layer.iter().collect();
            ordered.sort_by(|a, b| {
                let ca = nodes
                    .get(a.as_str())
                    .map(|nl| nl.x + nl.width / 2.0)
                    .unwrap_or(0.0);
                let cb = nodes
                    .get(b.as_str())
                    .map(|nl| nl.x + nl.width / 2.0)
                    .unwrap_or(0.0);
                ca.partial_cmp(&cb)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.cmp(b))
            });
            let layer_ids: HashSet<&str> = layer.iter().map(|s| s.as_str()).collect();
            for w in ordered.windows(2) {
                let left = w[0].as_str();
                let right = w[1].as_str();
                let gap = crate::layout::demand::band::adjacent_rank_gap(
                    left,
                    right,
                    &layer_ids,
                    &diagram.relations,
                    self.default_node_gap,
                    parallel_gap,
                    profile,
                );
                self.set_pair_gap(left, right, gap);
            }
        }
    }

    pub fn set_pair_gap(&mut self, a: &str, b: &str, gap: f64) {
        let key = canonical_pair(a, b);
        let entry = self.pair_gaps.entry(key).or_insert(self.default_node_gap);
        *entry = entry.max(gap);
    }

    /// 两节点外缘之间的最小间距。
    pub fn min_gap(&self, a: &str, b: &str) -> f64 {
        self.pair_gaps
            .get(&canonical_pair(a, b))
            .copied()
            .unwrap_or(self.default_node_gap)
    }

    pub fn request_corridor_boost(&mut self) {
        self.corridor_boost_requested = true;
    }

    pub fn take_corridor_boost(&mut self) -> bool {
        let v = self.corridor_boost_requested;
        self.corridor_boost_requested = false;
        v
    }

    /// 竖直 rank 缝下界（D4 P0）。
    pub fn vertical_rank_gap(&self) -> f64 {
        self.min_vertical_rank_gap
            .unwrap_or(self.default_node_gap)
            .max(self.default_node_gap)
    }

    // ─── L3 拆分方法 ─────────────────────────────────────────────────────────

    /// 提取布局空间需求（编译进 CoordinateProblem 的输入）。
    pub fn to_spacing_demand(&self) -> SpacingDemandStore {
        SpacingDemandStore {
            default_node_gap: self.default_node_gap,
            pair_gaps: self.pair_gaps.clone(),
            min_vertical_rank_gap: self.min_vertical_rank_gap,
        }
    }

    /// 提取路由只读契约。
    pub fn to_routing_contract(&self) -> RoutingContract {
        RoutingContract {
            port_clearance: self.port_clearance,
            corridor_boost_requested: self.corridor_boost_requested,
        }
    }

    /// 从拆分后的两个对象重建 SpaceBudget（兼容旧代码）。
    pub fn from_parts(demand: &SpacingDemandStore, contract: &RoutingContract) -> Self {
        Self {
            default_node_gap: demand.default_node_gap,
            pair_gaps: demand.pair_gaps.clone(),
            port_clearance: contract.port_clearance,
            corridor_boost_requested: contract.corridor_boost_requested,
            min_vertical_rank_gap: demand.min_vertical_rank_gap,
        }
    }

    /// D4 P0 + P1.2/P3.2：用只读压力模型抬缝。
    ///
    /// - 邻层 `deficit` → 抬 `min_vertical_rank_gap`（cap 于 diagram profile.max_extra）
    /// - 廊级 `load > capacity` → `request_corridor_boost` + 跨廊组节点 pair_gaps
    /// - 边级高 `obstacle_hits` / 高 `grid_overflow` → 端点 pair_gaps + 竖缝 / corridor_boost
    ///   （可单独 `PLOTGRAM_EDGE_PRESSURE_BUDGET=0` 关闭边级项）
    pub fn enrich_from_pressure(
        &mut self,
        diagram: &Diagram,
        nodes: &HashMap<String, NodeLayout>,
        corridor: &crate::layout::demand::CorridorModel,
        bands: &[crate::layout::demand::BandDemand],
        edge_features: &[crate::layout::demand::EdgeFeatures],
    ) {
        let profile = crate::layout::demand::band::EdgeBandDemandProfile::for_diagram(
            diagram.diagram_type.clone(),
            !diagram.groups.is_empty(),
        );
        let max_extra = if profile.max_extra.is_finite() {
            profile.max_extra
        } else {
            48.0
        };

        let max_def = bands.iter().map(|b| b.deficit).fold(0.0_f64, f64::max);
        if max_def > 1.0 {
            let extra = max_def.min(max_extra);
            let target = self.default_node_gap + extra;
            self.min_vertical_rank_gap = Some(
                self.min_vertical_rank_gap
                    .unwrap_or(self.default_node_gap)
                    .max(target),
            );
        }

        let lane = crate::layout::demand::CORRIDOR_LANE_PITCH;
        for c in corridor.demands.iter().filter(|d| d.is_over()) {
            self.request_corridor_boost();
            let overflow = c.overflow() as f64;
            let extra = (overflow * lane).min(max_extra).max(lane);
            // 跨廊两组：抬两端点节点对中「投影最近」的若干对
            let mut ga_nodes: Vec<&str> = diagram
                .entities
                .iter()
                .filter(|e| {
                    e.group_id
                        .as_ref()
                        .is_some_and(|g| g.as_str() == c.group_a)
                })
                .map(|e| e.id.as_str())
                .collect();
            let mut gb_nodes: Vec<&str> = diagram
                .entities
                .iter()
                .filter(|e| {
                    e.group_id
                        .as_ref()
                        .is_some_and(|g| g.as_str() == c.group_b)
                })
                .map(|e| e.id.as_str())
                .collect();
            ga_nodes.sort_unstable();
            gb_nodes.sort_unstable();
            let mut best: Vec<(f64, &str, &str)> = Vec::new();
            for a in &ga_nodes {
                let Some(na) = nodes.get(*a) else { continue };
                for b in &gb_nodes {
                    let Some(nb) = nodes.get(*b) else { continue };
                    let dist = match c.axis {
                        crate::layout::group::CorridorAxis::Vertical => {
                            // 组左右排列：水平间距
                            if na.x <= nb.x {
                                nb.x - (na.x + na.width)
                            } else {
                                na.x - (nb.x + nb.width)
                            }
                        }
                        crate::layout::group::CorridorAxis::Horizontal => {
                            if na.y <= nb.y {
                                nb.y - (na.y + na.height)
                            } else {
                                na.y - (nb.y + nb.height)
                            }
                        }
                    };
                    best.push((dist, *a, *b));
                }
            }
            best.sort_by(|x, y| {
                x.0.partial_cmp(&y.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| x.1.cmp(y.1))
                    .then_with(|| x.2.cmp(y.2))
            });
            for (_, a, b) in best.into_iter().take(4) {
                let required = self.min_gap(a, b).max(self.default_node_gap + extra);
                self.set_pair_gap(a, b, required);
            }
        }

        if edge_pressure_budget_enabled() {
            self.enrich_from_edge_features(diagram, nodes, edge_features, max_extra, lane);
        }
    }

    /// P1.2 / P3.2：边级 hits / grid_overflow → soft 加缝。
    ///
    /// 阈值来自 Phase 0 校准：flowchart 顶部分位数 `grid≈4` 且 `hits=0`，
    /// architecture 热点多为 `hits≥1` 且 `grid≥6`；故 hits≥1 或 grid≥6 才触发。
    /// 跨组穿透边额外抬两组间最近节点对（与廊 OVER 同出口），否则仅端点 pair 在
    /// 不同排时 `enforce_horizontal_gaps` 兑现不了。
    fn enrich_from_edge_features(
        &mut self,
        diagram: &Diagram,
        nodes: &HashMap<String, NodeLayout>,
        edge_features: &[crate::layout::demand::EdgeFeatures],
        max_extra: f64,
        lane: f64,
    ) {
        const HITS_TRIG: usize = 1;
        const GRID_TRIG: usize = 6;

        let mut feats: Vec<&crate::layout::demand::EdgeFeatures> = edge_features.iter().collect();
        feats.sort_by_key(|f| f.edge_index);

        let mut max_vert_extra = 0.0_f64;
        let mut pierce_hot = false;
        for f in feats {
            let hits = f.obstacle_hits;
            let grid = f.grid_overflow;
            if hits < HITS_TRIG && grid < GRID_TRIG {
                continue;
            }

            let hits_extra = if hits >= HITS_TRIG {
                (hits as f64) * lane
            } else {
                0.0
            };
            let grid_extra = if grid >= GRID_TRIG {
                ((grid.saturating_sub(crate::layout::demand::GRID_SOFT_CAP)) as f64) * 8.0
            } else {
                0.0
            };
            let extra = hits_extra.max(grid_extra).min(max_extra).max(0.0);
            if extra > 0.0 {
                let required = self
                    .min_gap(&f.from, &f.to)
                    .max(self.default_node_gap + extra);
                self.set_pair_gap(&f.from, &f.to, required);
            }

            // 有穿透的跨层边：竖缝是同 leaf-group 内 enforce 的主杠杆
            if hits >= HITS_TRIG && f.span_ranks >= 1 {
                max_vert_extra =
                    max_vert_extra.max(((hits as f64) * lane).min(max_extra));
            }

            // 跨组穿透：抬两组间投影最近的若干对（仿廊 OVER）
            if hits >= HITS_TRIG {
                pierce_hot = true;
                let ga = diagram
                    .entities
                    .iter()
                    .find(|e| e.id.as_str() == f.from)
                    .and_then(|e| e.group_id.as_ref().map(|g| g.as_str()));
                let gb = diagram
                    .entities
                    .iter()
                    .find(|e| e.id.as_str() == f.to)
                    .and_then(|e| e.group_id.as_ref().map(|g| g.as_str()));
                if let (Some(ga), Some(gb)) = (ga, gb) {
                    if ga != gb && extra > 0.0 {
                        raise_nearest_cross_group_pairs(self, diagram, nodes, ga, gb, extra);
                    }
                }
            }
        }

        if max_vert_extra > 1.0 {
            let target = self.default_node_gap + max_vert_extra;
            self.min_vertical_rank_gap = Some(
                self.min_vertical_rank_gap
                    .unwrap_or(self.default_node_gap)
                    .max(target),
            );
        }

        if pierce_hot {
            self.request_corridor_boost();
        }

        crate::perf_log!(
            "[edge-pressure] pierce_hot={} min_vert_gap={:?} pair_gaps={} corridor_boost={}",
            pierce_hot,
            self.min_vertical_rank_gap,
            self.pair_gaps.len(),
            self.corridor_boost_requested
        );
    }
}

/// 两组间投影最近的最多 4 对节点抬 `pair_gaps`（水平距离；跨组主缝）。
fn raise_nearest_cross_group_pairs(
    budget: &mut SpaceBudget,
    diagram: &Diagram,
    nodes: &HashMap<String, NodeLayout>,
    group_a: &str,
    group_b: &str,
    extra: f64,
) {
    let mut ga_nodes: Vec<&str> = diagram
        .entities
        .iter()
        .filter(|e| e.group_id.as_ref().is_some_and(|g| g.as_str() == group_a))
        .map(|e| e.id.as_str())
        .collect();
    let mut gb_nodes: Vec<&str> = diagram
        .entities
        .iter()
        .filter(|e| e.group_id.as_ref().is_some_and(|g| g.as_str() == group_b))
        .map(|e| e.id.as_str())
        .collect();
    ga_nodes.sort_unstable();
    gb_nodes.sort_unstable();
    let mut best: Vec<(f64, &str, &str)> = Vec::new();
    for a in &ga_nodes {
        let Some(na) = nodes.get(*a) else { continue };
        for b in &gb_nodes {
            let Some(nb) = nodes.get(*b) else { continue };
            let dist = if na.x <= nb.x {
                nb.x - (na.x + na.width)
            } else {
                na.x - (nb.x + nb.width)
            };
            best.push((dist, *a, *b));
        }
    }
    best.sort_by(|x, y| {
        x.0.partial_cmp(&y.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| x.1.cmp(y.1))
            .then_with(|| x.2.cmp(y.2))
    });
    for (_, a, b) in best.into_iter().take(4) {
        let required = budget.min_gap(a, b).max(budget.default_node_gap + extra);
        budget.set_pair_gap(a, b, required);
    }
}

fn edge_pressure_budget_enabled() -> bool {
    !std::env::var("PLOTGRAM_EDGE_PRESSURE_BUDGET")
        .map(|v| v == "0" || v.eq_ignore_ascii_case("false"))
        .unwrap_or(false)
}

fn canonical_pair(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

/// 水平方向（同排）强制满足空间契约：Y 有重叠的节点对按 `min_gap` 推开。
///
/// 返回被移动的节点 id（确定性：按 id 排序迭代）。
pub fn enforce_horizontal_gaps(
    nodes: &mut HashMap<String, NodeLayout>,
    budget: &SpaceBudget,
) -> Vec<String> {
    if nodes.len() <= 1 {
        return Vec::new();
    }
    let mut ids: Vec<String> = nodes.keys().cloned().collect();
    ids.sort();
    let mut moved: BTreeMap<String, ()> = BTreeMap::new();

    for _ in 0..24 {
        let mut any = false;
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                let (ai, bi) = (&ids[i], &ids[j]);
                let (ax, ay, aw, ah) = {
                    let n = &nodes[ai];
                    (n.x, n.y, n.width, n.height)
                };
                let (bx, by, bw, bh) = {
                    let n = &nodes[bi];
                    (n.x, n.y, n.width, n.height)
                };
                // 仅处理「同排」：Y 区间重叠超过一半较短边
                let y_overlap = (ay + ah).min(by + bh) - ay.max(by);
                let min_h = ah.min(bh);
                if y_overlap < min_h * 0.5 {
                    continue;
                }

                let required = budget.min_gap(ai, bi);
                let (left_id, right_id, left_right, right_left) = if ax <= bx {
                    (ai.as_str(), bi.as_str(), ax + aw, bx)
                } else {
                    (bi.as_str(), ai.as_str(), bx + bw, ax)
                };
                let gap = right_left - left_right;
                if gap + 0.5 >= required {
                    continue;
                }
                let deficit = required - gap;
                let half = deficit / 2.0;
                if let Some(nl) = nodes.get_mut(left_id) {
                    nl.x -= half;
                }
                if let Some(nl) = nodes.get_mut(right_id) {
                    nl.x += half;
                }
                moved.insert(left_id.to_string(), ());
                moved.insert(right_id.to_string(), ());
                any = true;
            }
        }
        if !any {
            break;
        }
    }

    moved.into_keys().collect()
}

/// 竖直 rank 轴强制最小层缝。
///
/// refine 会单独推动问题节点；若只看 AABB「同列」对，会漏掉斜向相连节点，
/// 也会误推跨组但仅仅投影重叠的无关节点。这里以布局写入的 rank 为权威：
/// 发现同一 leaf-group（无组图为 root）内相邻 rank band 间距不足时，整体下移
/// 当前及后续 rank，保持同层与拓扑顺序。跨组 rank 不可直接比较其绝对 y。
///
/// 返回被移动的节点 id（确定性：按 rank、id 排序）。
pub fn enforce_vertical_rank_gaps(
    nodes: &mut HashMap<String, NodeLayout>,
    budget: &SpaceBudget,
    ranks: &HashMap<String, usize>,
    scopes: &HashMap<String, String>,
    reverse_pairs: &HashSet<(String, String)>,
) -> Vec<String> {
    if nodes.len() <= 1 || ranks.is_empty() {
        return Vec::new();
    }

    let mut by_scope: BTreeMap<String, BTreeMap<usize, Vec<String>>> = BTreeMap::new();
    for (id, &rank) in ranks {
        if nodes.contains_key(id) {
            by_scope
                .entry(scopes.get(id).cloned().unwrap_or_default())
                .or_default()
                .entry(rank)
                .or_default()
                .push(id.clone());
        }
    }
    let mut moved: BTreeMap<String, ()> = BTreeMap::new();

    for by_rank in by_scope.values_mut() {
        if by_rank.len() <= 1 {
            continue;
        }
        for ids in by_rank.values_mut() {
            ids.sort();
        }

        let rank_keys: Vec<usize> = by_rank.keys().copied().collect();
        for boundary in 1..rank_keys.len() {
            let upper_rank = rank_keys[boundary - 1];
            let lower_rank = rank_keys[boundary];
            let mut deficit = 0.0f64;
            for upper_id in &by_rank[&upper_rank] {
                for lower_id in &by_rank[&lower_rank] {
                    let (Some(upper), Some(lower)) = (nodes.get(upper_id), nodes.get(lower_id))
                    else {
                        continue;
                    };
                    if upper.y > lower.y {
                        continue;
                    }
                    let gap = lower.y - (upper.y + upper.height);
                    let x_overlap =
                        (upper.x + upper.width).min(lower.x + lower.width) - upper.x.max(lower.x);
                    let projected_collision =
                        gap < 0.5 && x_overlap >= upper.width.min(lower.width) * 0.5;
                    let key = if upper_id <= lower_id {
                        (upper_id.clone(), lower_id.clone())
                    } else {
                        (lower_id.clone(), upper_id.clone())
                    };
                    let reverse_pair = reverse_pairs.contains(&key);
                    if projected_collision || reverse_pair {
                        deficit = deficit.max(budget.vertical_rank_gap() - gap);
                    }
                }
            }
            if deficit <= 0.5 {
                continue;
            }
            for &rank in &rank_keys[boundary..] {
                for id in &by_rank[&rank] {
                    if let Some(node) = nodes.get_mut(id) {
                        node.y += deficit;
                        moved.insert(id.clone(), ());
                    }
                }
            }
        }
    }

    moved.into_keys().collect()
}

/// 节点的 leaf-group scope；空串表示无组 root。
pub fn node_group_scopes(diagram: &Diagram) -> HashMap<String, String> {
    diagram
        .entities
        .iter()
        .map(|entity| {
            (
                entity.id.as_str().to_string(),
                entity
                    .group_id
                    .as_ref()
                    .map(|id| id.as_str().to_string())
                    .unwrap_or_default(),
            )
        })
        .collect()
}

/// 同一无向节点对同时存在两个方向的 relation。
pub fn reverse_relation_pairs(diagram: &Diagram) -> HashSet<(String, String)> {
    let directed: HashSet<(String, String)> = diagram
        .relations
        .iter()
        .map(|rel| (rel.from.as_str().to_string(), rel.to.as_str().to_string()))
        .collect();
    let mut out = HashSet::new();
    for (from, to) in &directed {
        if directed.contains(&(to.clone(), from.clone())) {
            let key = if from <= to {
                (from.clone(), to.clone())
            } else {
                (to.clone(), from.clone())
            };
            out.insert(key);
        }
    }
    out
}

/// 检查是否仍有水平方向违反契约的节点对。
pub fn horizontal_gap_violations(
    nodes: &HashMap<String, NodeLayout>,
    budget: &SpaceBudget,
) -> Vec<(String, String, f64, f64)> {
    let mut ids: Vec<String> = nodes.keys().cloned().collect();
    ids.sort();
    let mut out = Vec::new();
    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            let a = &ids[i];
            let b = &ids[j];
            let na = &nodes[a];
            let nb = &nodes[b];
            let y_overlap = (na.y + na.height).min(nb.y + nb.height) - na.y.max(nb.y);
            let min_h = na.height.min(nb.height);
            if y_overlap < min_h * 0.5 {
                continue;
            }
            let required = budget.min_gap(a, b);
            let gap = if na.x <= nb.x {
                nb.x - (na.x + na.width)
            } else {
                na.x - (nb.x + nb.width)
            };
            if gap + 0.5 < required {
                out.push((a.clone(), b.clone(), gap, required));
            }
        }
    }
    out
}

/// 任意一对节点 AABB 真实相交（含斜向部分重叠）。
///
/// `horizontal_gap_violations` 要求 Y 重叠 ≥50% 才视为同排；refine 对角推开后
/// 常留下「斜向相交」而被漏检，故兜底须用本谓词。
pub fn has_node_aabb_overlaps(nodes: &HashMap<String, NodeLayout>) -> bool {
    const EPS: f64 = 0.5;
    let mut ids: Vec<String> = nodes.keys().cloned().collect();
    ids.sort();
    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            let a = &nodes[&ids[i]];
            let b = &nodes[&ids[j]];
            let ox = (a.x + a.width).min(b.x + b.width) - a.x.max(b.x);
            let oy = (a.y + a.height).min(b.y + b.height) - a.y.max(b.y);
            if ox > EPS && oy > EPS {
                return true;
            }
        }
    }
    false
}

/// 契约失败时的兜底消重叠：margin 取自 SpaceBudget（无则 default_node_gap）。
///
/// 若提供 `sugiyama_ranks`，先按 rank 恢复同排（统一 Y + 水平缝），再跑 BruteForce，
/// 避免 refine 对角推开后把同层拆成上下两排或误推穿模。
pub fn resolve_residual_with_budget(
    nodes: &mut HashMap<String, NodeLayout>,
    budget: Option<&SpaceBudget>,
) {
    resolve_residual_with_budget_and_ranks(nodes, budget, None);
}

/// 同上，可带 Sugiyama rank 做同排恢复。
pub fn resolve_residual_with_budget_and_ranks(
    nodes: &mut HashMap<String, NodeLayout>,
    budget: Option<&SpaceBudget>,
    ranks: Option<&HashMap<String, usize>>,
) {
    let margin = budget
        .map(|b| b.default_node_gap)
        .unwrap_or(DEFAULT_NODE_GAP);
    let ran_rank_realign = if let (Some(b), Some(r)) = (budget, ranks) {
        realign_shared_rank_rows(nodes, r, b);
        true
    } else {
        false
    };
    if let Some(b) = budget {
        enforce_horizontal_gaps(nodes, b);
    }
    // 同排回排已清掉 AABB：不必再 BruteForce（会把同排拆成斜向）
    if ran_rank_realign && !has_node_aabb_overlaps(nodes) {
        return;
    }
    use crate::layout::engines::common::overlap::{
        BruteForceResolver, OverlapConfig, OverlapResolver,
    };
    let config = OverlapConfig {
        margin,
        max_iterations: 30,
        step_factor: 0.5,
    };
    let empty = HashMap::new();
    BruteForceResolver::new(20).resolve(nodes, &empty, &config);
    if let Some(b) = budget {
        enforce_horizontal_gaps(nodes, b);
    }
}

/// 同 Sugiyama rank 的节点：Y 对齐到中位数中心，再按当前 X 序水平缝推开。
///
/// 仅处理「含有 AABB 重叠对」的 rank，避免有组大图无重叠行被整排重排。
pub fn realign_shared_rank_rows(
    nodes: &mut HashMap<String, NodeLayout>,
    ranks: &HashMap<String, usize>,
    budget: &SpaceBudget,
) {
    const EPS: f64 = 0.5;
    let mut by_rank: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for (id, &rank) in ranks {
        if nodes.contains_key(id) {
            by_rank.entry(rank).or_default().push(id.clone());
        }
    }
    for (_rank, mut ids) in by_rank {
        if ids.len() < 2 {
            continue;
        }
        let mut has_overlap = false;
        'pairs: for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                let a = &nodes[&ids[i]];
                let b = &nodes[&ids[j]];
                let ox = (a.x + a.width).min(b.x + b.width) - a.x.max(b.x);
                let oy = (a.y + a.height).min(b.y + b.height) - a.y.max(b.y);
                if ox > EPS && oy > EPS {
                    has_overlap = true;
                    break 'pairs;
                }
            }
        }
        if !has_overlap {
            continue;
        }

        ids.sort_by(|a, b| {
            let ca = nodes[a].x + nodes[a].width * 0.5;
            let cb = nodes[b].x + nodes[b].width * 0.5;
            ca.partial_cmp(&cb)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.cmp(b))
        });

        let mut cys: Vec<f64> = ids
            .iter()
            .map(|id| nodes[id].y + nodes[id].height * 0.5)
            .collect();
        cys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let med_cy = cys[cys.len() / 2];
        for id in &ids {
            if let Some(nl) = nodes.get_mut(id) {
                nl.y = med_cy - nl.height * 0.5;
            }
        }

        for i in 1..ids.len() {
            let gap = budget.min_gap(&ids[i - 1], &ids[i]);
            let min_left = nodes[&ids[i - 1]].x + nodes[&ids[i - 1]].width + gap;
            if let Some(nl) = nodes.get_mut(&ids[i]) {
                if nl.x < min_left {
                    nl.x = min_left;
                }
            }
        }
        for i in (0..ids.len().saturating_sub(1)).rev() {
            let gap = budget.min_gap(&ids[i], &ids[i + 1]);
            let max_right = nodes[&ids[i + 1]].x - gap;
            if let Some(nl) = nodes.get_mut(&ids[i]) {
                let right = nl.x + nl.width;
                if right > max_right {
                    nl.x = max_right - nl.width;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ArrowType, AttributeMap, Diagram, Identifier, Relation, Span};

    fn rel(from: &str, to: &str, label: Option<&str>) -> Relation {
        Relation {
            from: Identifier::new_unchecked(from),
            to: Identifier::new_unchecked(to),
            arrow: ArrowType::Active,
            label: label.map(|s| s.to_string()),
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: Span::dummy(),
        }
    }

    #[test]
    fn labeled_edge_raises_pair_gap() {
        let diagram = Diagram {
            diagram_type: crate::types::DiagramType::Architecture,
            relations: vec![rel("db_master", "db_replica", Some("主从同步"))],
            ..Default::default()
        };
        let budget = SpaceBudget::from_diagram(&diagram);
        let gap = budget.min_gap("db_master", "db_replica");
        assert!(
            gap >= 52.0,
            "labeled pair gap should fit 主从同步, got {gap}"
        );
        assert_eq!(
            budget.min_gap("db_replica", "db_master"),
            gap,
            "pair gap must be symmetric"
        );
    }

    #[test]
    fn enforce_separates_tight_siblings() {
        let diagram = Diagram {
            diagram_type: crate::types::DiagramType::Architecture,
            relations: vec![rel("a", "b", Some("主从同步"))],
            ..Default::default()
        };
        let budget = SpaceBudget::from_diagram(&diagram);
        let mut nodes = HashMap::from([
            (
                "a".to_string(),
                NodeLayout {
                    x: 100.0,
                    y: 200.0,
                    width: 112.0,
                    height: 50.0,
                    ..Default::default()
                },
            ),
            (
                "b".to_string(),
                NodeLayout {
                    x: 200.0,
                    y: 200.0,
                    width: 112.0,
                    height: 50.0,
                    ..Default::default()
                },
            ),
        ]);
        enforce_horizontal_gaps(&mut nodes, &budget);
        let a = &nodes["a"];
        let b = &nodes["b"];
        let gap = b.x - (a.x + a.width);
        assert!(
            gap + 0.5 >= budget.min_gap("a", "b"),
            "gap={gap} required={}",
            budget.min_gap("a", "b")
        );
    }

    #[test]
    fn enforce_vertical_rank_gaps_moves_whole_lower_band() {
        let budget = SpaceBudget::new();
        let mut nodes = HashMap::from([
            (
                "upper_a".to_string(),
                NodeLayout {
                    x: 100.0,
                    y: 100.0,
                    width: 112.0,
                    height: 50.0,
                    ..Default::default()
                },
            ),
            (
                "lower_a".to_string(),
                NodeLayout {
                    x: 260.0,
                    y: 150.0, // 贴边：gap_y=0
                    width: 112.0,
                    height: 50.0,
                    ..Default::default()
                },
            ),
            (
                "lower_b".to_string(),
                NodeLayout {
                    x: 420.0,
                    y: 150.0,
                    width: 112.0,
                    height: 50.0,
                    ..Default::default()
                },
            ),
        ]);
        let ranks = HashMap::from([
            ("upper_a".to_string(), 0usize),
            ("lower_a".to_string(), 1usize),
            ("lower_b".to_string(), 1usize),
        ]);
        let scopes = HashMap::from([
            ("upper_a".to_string(), "g".to_string()),
            ("lower_a".to_string(), "g".to_string()),
            ("lower_b".to_string(), "g".to_string()),
        ]);
        let reverse_pairs = HashSet::from([("lower_a".to_string(), "upper_a".to_string())]);
        enforce_vertical_rank_gaps(&mut nodes, &budget, &ranks, &scopes, &reverse_pairs);
        let gap = nodes["lower_a"].y - (nodes["upper_a"].y + nodes["upper_a"].height);
        assert!(
            gap + 0.5 >= budget.default_node_gap,
            "gap={gap} required={}",
            budget.default_node_gap
        );
        assert_eq!(nodes["lower_a"].y, nodes["lower_b"].y);
    }
}
