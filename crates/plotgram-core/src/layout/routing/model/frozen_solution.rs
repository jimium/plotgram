//! 冻结路由解 + 增量依赖记录（Slice F2b，doc20 §5 Slice F）。
//!
//! 在 Coordinator 唯一 freeze + label solve 之后 [`FrozenRoutingSolution::capture`]
//! 捕获全图路由事实：逐边依赖记录（keyed by [`StableEdgeIdentity`]）+ 节点/分组
//! 几何指纹。调用方经 `LayoutResult.hints.frozen_routing`（`Arc`）跨渲染持有，
//! 下次渲染用 [`FrozenRoutingSolution::dirty_set`] 求「必须重路由的边集」，
//! 其余边可直接复用冻结 geometry/labels（Slice F2c 增量入口）。
//!
//! ## 设计红线（AGENTS.md）
//!
//! - §2 确定性：全部集合用 `BTreeSet`/`BTreeMap` 或显式排序，不依赖 HashMap 迭代序。
//! - 指纹量化 0.01px：亚像素抖动不触发 dirty。
//! - 不引入全局会话状态：solution 只经 hints Arc 暴露。

use crate::ast::Diagram;
use crate::layout::routing::model::stable_edge::{
    EdgeIdentityDiff, StableEdgeIdentity, StableEdgeStore,
};
use crate::layout::routing::RouteAnnotationSet;
use crate::layout::types::{EdgeLabelLayout, EdgeLayout, LayoutResult, PathGeometry, Port};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// 指纹量化倍率（0.01px）。
const FP_SCALE: f64 = 100.0;
/// 净空半径：路径包围盒外扩该距离内的节点视为「邻近障碍」。
const NEARBY_CLEARANCE: f64 = 12.0;

/// 量化几何指纹（id → 0.01px 网格上的 x/y/w/h）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeometryFingerprint {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
}

impl GeometryFingerprint {
    pub fn from_rect(x: f64, y: f64, w: f64, h: f64) -> Self {
        let q = |v: f64| (v * FP_SCALE).round() as i64;
        Self {
            x: q(x),
            y: q(y),
            w: q(w),
            h: q(h),
        }
    }
}

/// 单边依赖记录（keyed by identity）：冻结事实 + 依赖资源清单。
#[derive(Debug, Clone)]
pub struct EdgeDependencyRecord {
    /// 跨版本持久 key。
    pub identity: StableEdgeIdentity,
    /// 冻结路径几何（增量复用时原样写回）。
    pub geometry: PathGeometry,
    /// 冻结标签（label solve 终态，含 bbox 信息；增量复用时原样写回）。
    pub labels: Vec<EdgeLabelLayout>,
    /// 冻结端口（增量复用时与 geometry 一起原样写回）。
    pub from_port: Port,
    pub to_port: Port,
    /// 声明标签文案 [mid, head, tail]：文案变化时该边不可复用（dirty）。
    pub declared_labels: [Option<String>; 3],
    /// 端点节点 id。
    pub from_node: String,
    pub to_node: String,
    /// 净空（[`NEARBY_CLEARANCE`]）内邻近障碍节点 id（不含端点），升序。
    pub nearby_obstacles: Vec<String>,
    /// 路径进出的组 id（gate：路径点同时存在于组内与组外），升序。
    pub group_gates: Vec<String>,
    /// bundle/trunk key（来自 route_annotations 的 merge interval），升序去重。
    pub bundle_keys: Vec<String>,
    /// 冲突伙伴：路径包围盒重叠的其它边 identity，升序。
    pub conflict_partners: Vec<StableEdgeIdentity>,
}

/// 冻结路由解：逐边依赖记录 + 节点/分组指纹 + 边身份存储。
#[derive(Debug, Clone)]
pub struct FrozenRoutingSolution {
    /// 按声明序（下标 = 旧版本 edge idx）。
    pub records: Vec<EdgeDependencyRecord>,
    /// 旧版本边身份存储（match_identities 的 prev 侧）。
    pub edge_store: StableEdgeStore,
    pub node_fingerprints: BTreeMap<String, GeometryFingerprint>,
    pub group_fingerprints: BTreeMap<String, GeometryFingerprint>,
    /// 冻结旁路注解（zero-diff 全 preserve 时原样恢复，保字节一致）。
    pub route_annotations: Option<RouteAnnotationSet>,
}

impl FrozenRoutingSolution {
    /// 在 Coordinator freeze + label solve 之后捕获全图路由事实。
    ///
    /// 全部依赖清单确定性排序；`records[i]` 对应 `diagram.relations[i]`。
    pub fn capture(diagram: &Diagram, result: &LayoutResult) -> Self {
        let edge_store = StableEdgeStore::from_diagram(diagram);
        let node_fingerprints = node_fingerprints_of(result);
        let group_fingerprints = group_fingerprints_of(result);
        let adjacency = edge_conflict_adjacency(
            &result.edges,
            result.hints.route_annotations.as_ref(),
        );

        // 节点 id 固定序（BTreeMap key 序），用于邻近障碍扫描。
        let node_ids: Vec<&String> = node_fingerprints.keys().collect();

        let n = edge_store.len().min(result.edges.len());
        let mut records = Vec::with_capacity(n);
        for i in 0..n {
            let stable = edge_store.as_slice()[i].clone();
            let edge = &result.edges[i];
            let bbox = path_bbox(&edge.geometry);

            // 净空内邻近障碍：路径 bbox 外扩 clearance 与节点 bbox 相交（排除端点）。
            let mut nearby_obstacles = Vec::new();
            if let Some((min_x, min_y, max_x, max_y)) = bbox {
                for id in &node_ids {
                    if **id == stable.from || **id == stable.to {
                        continue;
                    }
                    if let Some(nl) = result.nodes.get(*id) {
                        let hit = nl.x < max_x + NEARBY_CLEARANCE
                            && nl.x + nl.width > min_x - NEARBY_CLEARANCE
                            && nl.y < max_y + NEARBY_CLEARANCE
                            && nl.y + nl.height > min_y - NEARBY_CLEARANCE;
                        if hit {
                            nearby_obstacles.push((*id).clone());
                        }
                    }
                }
            }

            // group gates：路径锚点同时存在于组内与组外 → 该组是路径的进出口。
            let mut group_gates = Vec::new();
            let pts = edge.geometry.anchor_points();
            for (gid, gl) in group_fingerprints.keys().filter_map(|gid| {
                result.groups.get(gid).map(|gl| (gid, gl))
            }) {
                let mut inside = false;
                let mut outside = false;
                for p in pts.iter() {
                    let is_in = p.x >= gl.x
                        && p.x <= gl.x + gl.width
                        && p.y >= gl.y
                        && p.y <= gl.y + gl.height;
                    if is_in {
                        inside = true;
                    } else {
                        outside = true;
                    }
                }
                if inside && outside {
                    group_gates.push(gid.clone());
                }
            }

            // bundle/trunk key：merge interval 的 group_key（缺省用量化轨道坐标）。
            let bundle_keys = bundle_keys_of(i, result.hints.route_annotations.as_ref());

            let conflict_partners: Vec<StableEdgeIdentity> = adjacency[i]
                .iter()
                .map(|&j| edge_store.as_slice()[j].identity.clone())
                .collect();

            records.push(EdgeDependencyRecord {
                identity: stable.identity,
                geometry: edge.geometry.clone(),
                labels: edge.labels.clone(),
                from_port: edge.from_port,
                to_port: edge.to_port,
                declared_labels: diagram
                    .relations
                    .get(i)
                    .map(|r| [r.label.clone(), r.head_label.clone(), r.tail_label.clone()])
                    .unwrap_or_default(),
                from_node: stable.from,
                to_node: stable.to,
                nearby_obstacles,
                group_gates,
                bundle_keys,
                conflict_partners,
            });
        }

        Self {
            records,
            edge_store,
            node_fingerprints,
            group_fingerprints,
            route_annotations: result.hints.route_annotations.clone(),
        }
    }

    /// 求新版本中「必须重路由」的边集（新版本下标，升序）。
    ///
    /// 初始 dirty：新增边 + 端点/邻近障碍/组 gate 指纹变化的 retained 边；
    /// 随后沿 conflicts + bundle 依赖图做固定序连通分量闭包扩张
    /// （单节点移动不会无关地重路由全图——只波及依赖分量内的边）。
    pub fn dirty_set(
        &self,
        new_store: &StableEdgeStore,
        new_node_fingerprints: &BTreeMap<String, GeometryFingerprint>,
        new_group_fingerprints: &BTreeMap<String, GeometryFingerprint>,
    ) -> (BTreeSet<usize>, EdgeIdentityDiff) {
        let diff = new_store.match_identities(&self.edge_store);

        let changed_nodes =
            changed_keys(&self.node_fingerprints, new_node_fingerprints);
        let changed_groups =
            changed_keys(&self.group_fingerprints, new_group_fingerprints);

        // 初始 dirty（prev 下标空间）：removed 边 + 依赖资源变化的 retained 边。
        let mut dirty_prev: BTreeSet<usize> = diff.removed.iter().copied().collect();
        for &(prev_idx, new_idx) in &diff.retained {
            let Some(rec) = self.records.get(prev_idx) else {
                dirty_prev.insert(prev_idx);
                continue;
            };
            // 声明标签标志变化（增/删标签）同样视为 dirty（标签不可复用）。
        let label_flags_changed = match (
                self.edge_store.as_slice().get(prev_idx),
                new_store.as_slice().get(new_idx),
            ) {
                (Some(p), Some(n)) => {
                    p.has_mid_label != n.has_mid_label
                        || p.has_head_label != n.has_head_label
                        || p.has_tail_label != n.has_tail_label
                }
                _ => true,
            };
            let touched = label_flags_changed
                || changed_nodes.contains(&rec.from_node)
                || changed_nodes.contains(&rec.to_node)
                || rec
                    .nearby_obstacles
                    .iter()
                    .any(|id| changed_nodes.contains(id))
                || rec.group_gates.iter().any(|id| changed_groups.contains(id));
            if touched {
                dirty_prev.insert(prev_idx);
            }
        }

        // 依赖图闭包（prev 空间）：conflicts + bundle 连通分量，固定序迭代到不动点。
        let adjacency = self.prev_dependency_adjacency();
        loop {
            let mut grew = false;
            for idx in dirty_prev.clone() {
                if let Some(partners) = adjacency.get(&idx) {
                    for &p in partners {
                        if dirty_prev.insert(p) {
                            grew = true;
                        }
                    }
                }
            }
            if !grew {
                break;
            }
        }

        // 映射回新版本下标：added 全 dirty + dirty_prev 中 retained 的 new_idx。
        let mut dirty_new: BTreeSet<usize> = diff.added.iter().copied().collect();
        for &(prev_idx, new_idx) in &diff.retained {
            if dirty_prev.contains(&prev_idx) {
                dirty_new.insert(new_idx);
            }
        }
        (dirty_new, diff)
    }

    /// prev 空间依赖邻接表：conflict partners（identity → prev idx）+ 同 bundle key。
    fn prev_dependency_adjacency(&self) -> BTreeMap<usize, BTreeSet<usize>> {
        let idx_by_identity: HashMap<&StableEdgeIdentity, usize> = self
            .records
            .iter()
            .enumerate()
            .map(|(i, r)| (&r.identity, i))
            .collect();
        let mut adjacency: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
        // conflict partners（双向）。
        for (i, rec) in self.records.iter().enumerate() {
            for partner in &rec.conflict_partners {
                if let Some(&j) = idx_by_identity.get(partner) {
                    if i != j {
                        adjacency.entry(i).or_default().insert(j);
                        adjacency.entry(j).or_default().insert(i);
                    }
                }
            }
        }
        // bundle：同 key 的边两两相连。
        let mut by_bundle: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for (i, rec) in self.records.iter().enumerate() {
            for key in &rec.bundle_keys {
                by_bundle.entry(key.as_str()).or_default().push(i);
            }
        }
        for members in by_bundle.values() {
            for &a in members {
                for &b in members {
                    if a != b {
                        adjacency.entry(a).or_default().insert(b);
                    }
                }
            }
        }
        adjacency
    }
}

/// 节点几何指纹（量化 0.01px），BTreeMap 保证 key 固定序。
pub fn node_fingerprints_of(result: &LayoutResult) -> BTreeMap<String, GeometryFingerprint> {
    result
        .nodes
        .iter()
        .map(|(id, nl)| {
            (
                id.clone(),
                GeometryFingerprint::from_rect(nl.x, nl.y, nl.width, nl.height),
            )
        })
        .collect()
}

/// 分组几何指纹（量化 0.01px）。
pub fn group_fingerprints_of(result: &LayoutResult) -> BTreeMap<String, GeometryFingerprint> {
    result
        .groups
        .iter()
        .map(|(id, gl)| {
            (
                id.clone(),
                GeometryFingerprint::from_rect(gl.x, gl.y, gl.width, gl.height),
            )
        })
        .collect()
}

/// 冲突邻接表：路径包围盒两两重叠 + 同 bundle key（下标空间，确定性）。
///
/// E3 repair loop 的 affected-set 扩张与 capture 的 conflict partners 共用此函数。
pub fn edge_conflict_adjacency(
    edges: &[EdgeLayout],
    annotations: Option<&RouteAnnotationSet>,
) -> Vec<BTreeSet<usize>> {
    let bboxes: Vec<Option<(f64, f64, f64, f64)>> =
        edges.iter().map(|e| path_bbox(&e.geometry)).collect();
    let mut adjacency = vec![BTreeSet::new(); edges.len()];
    for i in 0..edges.len() {
        let Some(a) = bboxes[i] else { continue };
        for j in (i + 1)..edges.len() {
            let Some(b) = bboxes[j] else { continue };
            let overlap = a.0 < b.2 && a.2 > b.0 && a.1 < b.3 && a.3 > b.1;
            if overlap {
                adjacency[i].insert(j);
                adjacency[j].insert(i);
            }
        }
    }
    // 同 bundle key 的边互为依赖伙伴。
    if annotations.is_some() {
        let mut by_bundle: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for i in 0..edges.len() {
            for key in bundle_keys_of(i, annotations) {
                by_bundle.entry(key).or_default().push(i);
            }
        }
        for members in by_bundle.values() {
            for &a in members {
                for &b in members {
                    if a != b {
                        adjacency[a].insert(b);
                    }
                }
            }
        }
    }
    adjacency
}

/// 某边的 bundle/trunk key 列表（升序去重）。
///
/// 来自 route_annotations 的 merge interval：优先 `group_key`；缺省用
/// 「方向 + 量化轨道坐标」合成 key（同 trunk 的边坐标一致 → key 一致）。
fn bundle_keys_of(edge_index: usize, annotations: Option<&RouteAnnotationSet>) -> Vec<String> {
    let mut keys = BTreeSet::new();
    if let Some(ann) = annotations.and_then(|set| set.get(edge_index)) {
        for mi in &ann.merge_intervals {
            let key = match &mi.group_key {
                Some(k) => k.clone(),
                None => format!(
                    "mi:{}:{}",
                    if mi.horizontal { "h" } else { "v" },
                    (mi.coord * FP_SCALE).round() as i64
                ),
            };
            keys.insert(key);
        }
    }
    keys.into_iter().collect()
}

/// 路径包围盒（锚点 + Bezier 控制点的 AABB）；空路径为 `None`。
fn path_bbox(geometry: &PathGeometry) -> Option<(f64, f64, f64, f64)> {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut fold = |x: f64, y: f64| {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    };
    if geometry.is_empty() {
        return None;
    }
    for p in geometry.anchor_points().iter() {
        fold(p.x, p.y);
    }
    if let Some(controls) = geometry.bezier_controls() {
        for c in controls {
            fold(c.x, c.y);
        }
    }
    if min_x.is_finite() {
        Some((min_x, min_y, max_x, max_y))
    } else {
        None
    }
}

/// 两份指纹表的差异 key 集合：值不同、新增或删除的 id。
fn changed_keys(
    prev: &BTreeMap<String, GeometryFingerprint>,
    new: &BTreeMap<String, GeometryFingerprint>,
) -> BTreeSet<String> {
    let mut changed = BTreeSet::new();
    for (id, fp) in prev {
        match new.get(id) {
            Some(new_fp) if new_fp == fp => {}
            _ => {
                changed.insert(id.clone());
            }
        }
    }
    for id in new.keys() {
        if !prev.contains_key(id) {
            changed.insert(id.clone());
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ArrowType, AttributeMap, Identifier, Relation, SourceInfo, Span};
    use crate::layout::geometry::Point;
    use crate::layout::types::{LayoutHints, NodeLayout, Port};
    use crate::types::DiagramType;

    fn rel(from: &str, to: &str) -> Relation {
        Relation {
            from: Identifier::new_unchecked(from),
            to: Identifier::new_unchecked(to),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: Span::dummy(),
        }
    }

    fn diagram_with(relations: Vec<Relation>) -> Diagram {
        let mut d = Diagram::new(DiagramType::Flowchart, SourceInfo::default());
        d.relations = relations;
        d
    }

    fn node(x: f64, y: f64) -> NodeLayout {
        NodeLayout {
            x,
            y,
            width: 10.0,
            height: 10.0,
        }
    }

    fn straight_edge(x0: f64, y0: f64, x1: f64, y1: f64) -> EdgeLayout {
        EdgeLayout {
            geometry: PathGeometry::Straight {
                start: Point::new(x0, y0),
                end: Point::new(x1, y1),
            },
            labels: Vec::new(),
            from_port: Port::Right,
            to_port: Port::Left,
        }
    }

    /// 两条相互远离的边：a→b（上方）与 c→d（下方 500px），依赖分量互不相交。
    fn fixture() -> (Diagram, LayoutResult) {
        let diagram = diagram_with(vec![rel("a", "b"), rel("c", "d")]);
        let mut nodes = HashMap::new();
        nodes.insert("a".to_string(), node(0.0, 0.0));
        nodes.insert("b".to_string(), node(100.0, 0.0));
        nodes.insert("c".to_string(), node(0.0, 500.0));
        nodes.insert("d".to_string(), node(100.0, 500.0));
        let result = LayoutResult {
            nodes,
            groups: crate::layout::GroupTable::new(),
            edges: vec![
                straight_edge(10.0, 5.0, 100.0, 5.0),
                straight_edge(10.0, 505.0, 100.0, 505.0),
            ],
            total_width: 110.0,
            total_height: 510.0,
            hints: LayoutHints::default(),
        };
        (diagram, result)
    }

    #[test]
    fn zero_diff_yields_empty_dirty_set() {
        let (diagram, result) = fixture();
        let frozen = FrozenRoutingSolution::capture(&diagram, &result);
        let new_store = StableEdgeStore::from_diagram(&diagram);
        let (dirty, diff) = frozen.dirty_set(
            &new_store,
            &node_fingerprints_of(&result),
            &group_fingerprints_of(&result),
        );
        assert!(dirty.is_empty());
        assert_eq!(diff.retained, vec![(0, 0), (1, 1)]);
        assert!(diff.added.is_empty() && diff.removed.is_empty());
    }

    #[test]
    fn single_node_move_dirties_only_its_dependency_component() {
        // F2 退出判据：单节点指纹变化的 dirty_set 只含端点/邻近/依赖分量内的边。
        let (diagram, result) = fixture();
        let frozen = FrozenRoutingSolution::capture(&diagram, &result);

        let mut moved = result.clone();
        moved.nodes.get_mut("a").unwrap().x += 50.0;
        let new_store = StableEdgeStore::from_diagram(&diagram);
        let (dirty, _) = frozen.dirty_set(
            &new_store,
            &node_fingerprints_of(&moved),
            &group_fingerprints_of(&moved),
        );
        // 只有 a→b（edge 0）dirty；c→d（edge 1）不受波及。
        assert_eq!(dirty.into_iter().collect::<Vec<_>>(), vec![0]);
    }

    #[test]
    fn sub_pixel_jitter_does_not_dirty() {
        // 指纹量化 0.01px：更小的抖动不触发 dirty。
        let (diagram, result) = fixture();
        let frozen = FrozenRoutingSolution::capture(&diagram, &result);
        let mut jittered = result.clone();
        jittered.nodes.get_mut("a").unwrap().x += 0.001;
        let new_store = StableEdgeStore::from_diagram(&diagram);
        let (dirty, _) = frozen.dirty_set(
            &new_store,
            &node_fingerprints_of(&jittered),
            &group_fingerprints_of(&jittered),
        );
        assert!(dirty.is_empty());
    }

    #[test]
    fn added_relation_dirties_only_new_edge() {
        let (diagram, result) = fixture();
        let frozen = FrozenRoutingSolution::capture(&diagram, &result);

        // 头部插入新 relation：既有边 positional id 变化但 identity 保持 clean。
        let new_diagram =
            diagram_with(vec![rel("x", "y"), rel("a", "b"), rel("c", "d")]);
        let new_store = StableEdgeStore::from_diagram(&new_diagram);
        let (dirty, diff) = frozen.dirty_set(
            &new_store,
            &node_fingerprints_of(&result),
            &group_fingerprints_of(&result),
        );
        assert_eq!(dirty.into_iter().collect::<Vec<_>>(), vec![0]);
        assert_eq!(diff.retained, vec![(0, 1), (1, 2)]);
    }

    #[test]
    fn conflict_partners_propagate_dirty_via_closure() {
        // 两条 bbox 重叠的边构成依赖分量：动其一端点 → 两条边都 dirty。
        let diagram = diagram_with(vec![rel("a", "b"), rel("a", "d")]);
        let mut nodes = HashMap::new();
        nodes.insert("a".to_string(), node(0.0, 0.0));
        nodes.insert("b".to_string(), node(100.0, 0.0));
        nodes.insert("d".to_string(), node(100.0, 20.0));
        let result = LayoutResult {
            nodes,
            groups: crate::layout::GroupTable::new(),
            edges: vec![
                straight_edge(10.0, 5.0, 100.0, 5.0),
                straight_edge(10.0, 15.0, 100.0, 0.0),
            ],
            total_width: 110.0,
            total_height: 30.0,
            hints: LayoutHints::default(),
        };
        let frozen = FrozenRoutingSolution::capture(&diagram, &result);
        assert!(!frozen.records[0].conflict_partners.is_empty());

        let mut moved = result.clone();
        moved.nodes.get_mut("b").unwrap().y += 40.0;
        let new_store = StableEdgeStore::from_diagram(&diagram);
        let (dirty, _) = frozen.dirty_set(
            &new_store,
            &node_fingerprints_of(&moved),
            &group_fingerprints_of(&moved),
        );
        // b 只是 edge 0 的端点，但 edge 1 与之 bbox 重叠 → 闭包扩张进 dirty。
        assert_eq!(dirty.into_iter().collect::<Vec<_>>(), vec![0, 1]);
    }
}
