//! Plan 逐字段 diff（23 号文 Stage 1 交付 1.2）：报告两个 Plan 的离散决策差异。
//!
//! 输出全部按键升序（确定性）；`Display` 给出人读摘要，供未来对拍报告 /
//! Stage 6 增量（Plan diff → 局部重算）使用。

use super::{EdgePorts, GroupKey, GroupScopeSpec, Plan, Slot};
use crate::layout::atlas::channel::{Bundle, EdgeId, GateId, NodeKey, TrackId};
use std::collections::BTreeMap;
use std::fmt;

/// 单键的变化形态。
#[derive(Debug, Clone, PartialEq)]
pub enum Change<T> {
    /// 仅存在于 b（新增）。
    Added(T),
    /// 仅存在于 a（移除）。
    Removed(T),
    /// 两侧都有但不等：`(a 值, b 值)`。
    Changed(T, T),
}

/// 一条边的分字段差异（只列出有变化的字段）。
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeChange {
    pub edge: EdgeId,
    pub ports: Option<Change<EdgePorts>>,
    pub gates: Option<Change<Vec<GateId>>>,
    pub channels: Option<Change<Vec<TrackId>>>,
}

/// 两个 Plan 的差异报告。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlanDiff {
    /// substrate 摘要是否不同。
    pub substrate_changed: bool,
    /// 节点槽位差异（键升序）。
    pub node_slots: Vec<(NodeKey, Change<Slot>)>,
    /// 组作用域差异（键升序）。
    pub group_scopes: Vec<(GroupKey, Change<GroupScopeSpec>)>,
    /// 边差异（EdgeId 升序；每条边分字段报告 ports/gates/channels）。
    pub edges: Vec<EdgeChange>,
    /// 仅 b 有的合流束。
    pub bundles_added: Vec<Bundle>,
    /// 仅 a 有的合流束。
    pub bundles_removed: Vec<Bundle>,
}

impl PlanDiff {
    /// 无任何差异（provenance 不参与判定——溯源是元数据，不是决策本身）。
    pub fn is_empty(&self) -> bool {
        !self.substrate_changed
            && self.node_slots.is_empty()
            && self.group_scopes.is_empty()
            && self.edges.is_empty()
            && self.bundles_added.is_empty()
            && self.bundles_removed.is_empty()
    }
}

impl fmt::Display for PlanDiff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            return write!(f, "PlanDiff: 无差异");
        }
        writeln!(f, "PlanDiff:")?;
        if self.substrate_changed {
            writeln!(f, "  substrate 摘要变化")?;
        }
        for (node, c) in &self.node_slots {
            writeln!(f, "  node {node}: {}", change_kind(c))?;
        }
        for (group, c) in &self.group_scopes {
            writeln!(f, "  group {group}: {}", change_kind(c))?;
        }
        for e in &self.edges {
            let mut fields = Vec::new();
            if e.ports.is_some() {
                fields.push("ports");
            }
            if e.gates.is_some() {
                fields.push("gates");
            }
            if e.channels.is_some() {
                fields.push("channels");
            }
            writeln!(f, "  edge {}: {} 变化", e.edge, fields.join("/"))?;
        }
        if !self.bundles_added.is_empty() || !self.bundles_removed.is_empty() {
            writeln!(
                f,
                "  bundles: +{} -{}",
                self.bundles_added.len(),
                self.bundles_removed.len()
            )?;
        }
        Ok(())
    }
}

fn change_kind<T>(c: &Change<T>) -> &'static str {
    match c {
        Change::Added(_) => "新增",
        Change::Removed(_) => "移除",
        Change::Changed(..) => "变更",
    }
}

/// 有序 map 的通用键级 diff（键升序输出）。
fn diff_map<K: Ord + Clone, V: PartialEq + Clone>(
    a: &BTreeMap<K, V>,
    b: &BTreeMap<K, V>,
) -> Vec<(K, Change<V>)> {
    let mut out = Vec::new();
    // 键并集：a 全部键 + b 独有键。chain 结果**非**全局有序（b 独有键
    // 整体缀后，如 a={2}, b={1,2} → 迭代序 2,1），末尾排序保证键升序
    for k in a.keys().chain(b.keys().filter(|k| !a.contains_key(*k))) {
        match (a.get(k), b.get(k)) {
            (Some(va), Some(vb)) if va != vb => {
                out.push((k.clone(), Change::Changed(va.clone(), vb.clone())));
            }
            (Some(va), None) => out.push((k.clone(), Change::Removed(va.clone()))),
            (None, Some(vb)) => out.push((k.clone(), Change::Added(vb.clone()))),
            _ => {}
        }
    }
    out.sort_by(|(ka, _), (kb, _)| ka.cmp(kb));
    out
}

/// 逐字段比较两个 Plan（a = 基准，b = 对照）。
pub fn diff(a: &Plan, b: &Plan) -> PlanDiff {
    // 边差异：先按字段各自 diff，再按 EdgeId 归并成 EdgeChange
    let mut per_edge: BTreeMap<EdgeId, EdgeChange> = BTreeMap::new();
    fn entry(map: &mut BTreeMap<EdgeId, EdgeChange>, edge: EdgeId) -> &mut EdgeChange {
        map.entry(edge).or_insert(EdgeChange {
            edge,
            ports: None,
            gates: None,
            channels: None,
        })
    }
    for (edge, c) in diff_map(&a.ports, &b.ports) {
        entry(&mut per_edge, edge).ports = Some(c);
    }
    for (edge, c) in diff_map(&a.gates, &b.gates) {
        entry(&mut per_edge, edge).gates = Some(c);
    }
    for (edge, c) in diff_map(&a.channels, &b.channels) {
        entry(&mut per_edge, edge).channels = Some(c);
    }

    // bundles 集合差（Bundle: Eq；O(n·m)，束数小）
    let bundles_added = b
        .bundles
        .iter()
        .filter(|x| !a.bundles.contains(x))
        .cloned()
        .collect();
    let bundles_removed = a
        .bundles
        .iter()
        .filter(|x| !b.bundles.contains(x))
        .cloned()
        .collect();

    PlanDiff {
        substrate_changed: a.substrate != b.substrate,
        node_slots: diff_map(&a.node_slots, &b.node_slots),
        group_scopes: diff_map(&a.group_scopes, &b.group_scopes),
        edges: per_edge.into_values().collect(),
        bundles_added,
        bundles_removed,
    }
}
