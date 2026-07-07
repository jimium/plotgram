//! 布局静态质量检查（LayoutLint）。
//!
//! 在 `LayoutResult` 上运行一组确定性几何规则，输出可追溯到 DSL 实体的违规列表。
//! 供 CLI、测试、eval 框架消费；不依赖 SVG 渲染。

mod config;
mod geometry;
mod violation;

pub use config::{
    parse_lint_profile, parse_lint_rule, parse_lint_rules_list, LintConfig, LintProfile, RuleConfig,
};
pub use violation::{
    LayoutViolation, LintReport, LintRuleId, LintSeverity,
};

use crate::ast::Diagram;
use crate::layout::edge::edge_bundling::types::BundlingResult;
use crate::layout::geometry::Point;
use crate::layout::refine::segment_intersects_node;
use crate::layout::{ContainmentViolationKind, LayoutResult};
use geometry::{
    group_overlap_area, node_overlap_area, point_in_rect_interior, segment_midpoint,
    segment_on_group_border, segments_cross,
};
use std::collections::{HashMap, HashSet};

/// 布局 lint 执行器。
#[derive(Debug, Clone)]
pub struct LayoutLinter {
    pub config: LintConfig,
}

impl Default for LayoutLinter {
    fn default() -> Self {
        Self {
            config: LintConfig::default(),
        }
    }
}

impl LayoutLinter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_config(config: LintConfig) -> Self {
        Self { config }
    }

    /// 对布局结果运行已启用的规则。
    pub fn run(&self, diagram: &Diagram, result: &LayoutResult) -> LintReport {
        let mut violations = Vec::new();
        let cfg = &self.config;

        if cfg.is_enabled(LintRuleId::NodeOverlap) {
            check_node_overlaps(result, &mut violations);
        }
        if cfg.is_enabled(LintRuleId::GroupOverlap) {
            check_group_overlaps(diagram, result, &mut violations);
        }
        if cfg.is_enabled(LintRuleId::NodeOutsideGroup)
            || cfg.is_enabled(LintRuleId::ChildGroupOutsideParent)
        {
            check_containment(diagram, result, cfg, &mut violations);
        }
        if cfg.is_enabled(LintRuleId::EdgeThroughNode) {
            check_edge_through_nodes(diagram, result, &mut violations);
        }
        if cfg.is_enabled(LintRuleId::EdgeCrossing) {
            check_edge_crossings(result, &mut violations);
        }
        if cfg.is_enabled(LintRuleId::EdgeOnGroupBorder) {
            check_edge_on_group_borders(diagram, result, &mut violations);
        }
        if cfg.is_enabled(LintRuleId::EdgeCrossesGroupInterior) {
            check_edge_crosses_group_interior(diagram, result, &mut violations);
        }

        // ── Edge Bundling 专项检查 ──
        if let Some(bundle_hints) = &result.hints.edge_bundling {
            let bundling = &bundle_hints.result;
            if !bundling.bundles.is_empty() {
                if cfg.is_enabled(LintRuleId::BundledArrowConvergence) {
                    check_bundled_arrow_convergence(diagram, bundling, &mut violations);
                }
                if cfg.is_enabled(LintRuleId::BundledOppositeFlow) {
                    check_bundled_opposite_flow(diagram, bundling, &mut violations);
                }
                if cfg.is_enabled(LintRuleId::BundleMergeDensity) {
                    check_bundle_merge_density(bundling, &mut violations);
                }
                if cfg.is_enabled(LintRuleId::BundleForkOverlap) {
                    check_bundle_fork_overlap(result, bundling, &mut violations);
                }
                if cfg.is_enabled(LintRuleId::BundleTrunkThroughNode) {
                    check_bundle_trunk_through_node(diagram, result, bundling, &mut violations);
                }
            }
        }

        let mut violations = finalize_violations(cfg, violations);
        sort_violations(&mut violations);
        LintReport { violations }
    }
}

/// 便捷入口：默认配置运行 lint。
pub fn lint_layout(diagram: &Diagram, result: &LayoutResult) -> LintReport {
    LayoutLinter::new().run(diagram, result)
}

fn finalize_violations(config: &LintConfig, violations: Vec<LayoutViolation>) -> Vec<LayoutViolation> {
    violations
        .into_iter()
        .map(|mut v| {
            v.severity = config.severity_for(v.rule);
            v
        })
        .collect()
}

fn sort_violations(violations: &mut Vec<LayoutViolation>) {
    violations.sort_by(|a, b| {
        a.rule
            .as_str()
            .cmp(b.rule.as_str())
            .then_with(|| a.message.cmp(&b.message))
            .then_with(|| a.edge_index.cmp(&b.edge_index))
    });
}

fn check_node_overlaps(result: &LayoutResult, out: &mut Vec<LayoutViolation>) {
    let mut ids: Vec<&String> = result.nodes.keys().collect();
    ids.sort();

    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            let a = &result.nodes[ids[i]];
            let b = &result.nodes[ids[j]];
            let area = node_overlap_area(a, b);
            if area > 0.0 {
                out.push(
                    LayoutViolation::new(
                        LintRuleId::NodeOverlap,
                        format!("节点 '{}' 与 '{}' 重叠", ids[i], ids[j]),
                    )
                    .with_metric(area)
                    .with_entities([ids[i].as_str(), ids[j].as_str()]),
                );
            }
        }
    }
}

fn check_group_overlaps(diagram: &Diagram, result: &LayoutResult, out: &mut Vec<LayoutViolation>) {
    let ancestor_pairs = group_ancestor_pairs(diagram);
    let mut ids: Vec<&String> = result.groups.keys().collect();
    ids.sort();

    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            let id_a = ids[i].as_str();
            let id_b = ids[j].as_str();
            if ancestor_pairs.contains(&(id_a.to_string(), id_b.to_string()))
                || ancestor_pairs.contains(&(id_b.to_string(), id_a.to_string()))
            {
                continue;
            }
            let a = &result.groups[ids[i]];
            let b = &result.groups[ids[j]];
            let area = group_overlap_area(a, b);
            if area > 0.0 {
                out.push(
                    LayoutViolation::new(
                        LintRuleId::GroupOverlap,
                        format!("分组 '{}' 与 '{}' 重叠", id_a, id_b),
                    )
                    .with_metric(area)
                    .with_groups([id_a, id_b]),
                );
            }
        }
    }
}

/// 构建有祖先后代关系的分组对（双向存入集合）。
fn group_ancestor_pairs(diagram: &Diagram) -> HashSet<(String, String)> {
    let parent_of: HashMap<&str, &str> = diagram
        .groups
        .iter()
        .filter_map(|g| g.parent_id.as_ref().map(|p| (g.id.as_str(), p.as_str())))
        .collect();

    let mut pairs = HashSet::new();
    for group in &diagram.groups {
        let mut current = parent_of.get(group.id.as_str()).copied();
        while let Some(parent) = current {
            pairs.insert((group.id.as_str().to_string(), parent.to_string()));
            current = parent_of.get(parent).copied();
        }
    }
    pairs
}

fn check_containment(
    diagram: &Diagram,
    result: &LayoutResult,
    config: &LintConfig,
    out: &mut Vec<LayoutViolation>,
) {
    let child_groups: HashSet<&str> = diagram
        .groups
        .iter()
        .flat_map(|g| g.child_group_ids.iter().map(|id| id.as_str()))
        .collect();

    for v in result.validate_group_containment(diagram) {
        let is_child_group = child_groups.contains(v.entity_id.as_str());
        let rule = if is_child_group {
            LintRuleId::ChildGroupOutsideParent
        } else {
            LintRuleId::NodeOutsideGroup
        };
        if !config.is_enabled(rule) {
            continue;
        }
        let dir = containment_direction_label(v.kind);
        let subject = if is_child_group {
            "子分组"
        } else {
            "节点"
        };
        out.push(
            LayoutViolation::new(
                rule,
                format!(
                    "{} '{}' 在分组 '{}' 中{}{:.1}px",
                    subject, v.entity_id, v.group_id, dir, v.excess
                ),
            )
            .with_metric(v.excess)
            .with_entities([&v.entity_id])
            .with_groups([&v.group_id]),
        );
    }
}

fn containment_direction_label(kind: ContainmentViolationKind) -> &'static str {
    match kind {
        ContainmentViolationKind::TopOverflow => "顶部超出 ",
        ContainmentViolationKind::BottomOverflow => "底部超出 ",
        ContainmentViolationKind::LeftOverflow => "左侧超出 ",
        ContainmentViolationKind::RightOverflow => "右侧超出 ",
    }
}

fn check_edge_through_nodes(diagram: &Diagram, result: &LayoutResult, out: &mut Vec<LayoutViolation>) {
    for (index, edge) in result.edges.iter().enumerate() {
        if edge.path_len() < 2 {
            continue;
        }
        let rel = &diagram.relations[index];
        let from_id = rel.from.as_str();
        let to_id = rel.to.as_str();
        let path = edge.path_points();
        let segment_count = path.len().saturating_sub(1);
        let skip_endpoints = segment_count > 2;

        for (seg_i, window) in path.windows(2).enumerate() {
            if skip_endpoints && (seg_i == 0 || seg_i == segment_count - 1) {
                continue;
            }
            let a = window[0];
            let b = window[1];
            let mut node_ids: Vec<&String> = result.nodes.keys().collect();
            node_ids.sort();
            for node_id in node_ids {
                if node_id == from_id || node_id == to_id {
                    continue;
                }
                let nl = &result.nodes[node_id];
                if segment_intersects_node(a, b, nl) {
                    out.push(
                        LayoutViolation::new(
                            LintRuleId::EdgeThroughNode,
                            format!(
                                "边 {} → {} 穿过节点 '{}'",
                                rel.from.as_str(),
                                rel.to.as_str(),
                                node_id
                            ),
                        )
                        .with_edge_index(index)
                        .with_entities([from_id, to_id, node_id.as_str()]),
                    );
                }
            }
        }
    }
}

fn check_edge_crossings(result: &LayoutResult, out: &mut Vec<LayoutViolation>) {
    let edges = &result.edges;
    if edges.len() < 2 {
        return;
    }

    let sampled: Vec<Vec<Point>> = edges.iter().map(|e| e.sampled_path(16)).collect();

    for i in 0..sampled.len() {
        for j in (i + 1)..sampled.len() {
            if edges_share_endpoint(&edges[i], &edges[j]) {
                continue;
            }
            if polylines_cross(&sampled[i], &sampled[j]) {
                out.push(
                    LayoutViolation::new(
                        LintRuleId::EdgeCrossing,
                        format!("边 index={i} 与边 index={j} 交叉"),
                    )
                    .with_edge_index(i)
                    .with_entities([] as [&str; 0]),
                );
            }
        }
    }
}

fn edges_share_endpoint(a: &crate::layout::EdgeLayout, b: &crate::layout::EdgeLayout) -> bool {
    if a.path_is_empty() || b.path_is_empty() {
        return false;
    }
    let a_start = a.path_start().unwrap();
    let a_end = a.path_end().unwrap();
    let b_start = b.path_start().unwrap();
    let b_end = b.path_end().unwrap();
    let eps = 2.0;
    let near = |p1: Point, p2: Point| -> bool {
        (p1.x - p2.x).abs() < eps && (p1.y - p2.y).abs() < eps
    };
    near(a_start, b_start) || near(a_start, b_end) || near(a_end, b_start) || near(a_end, b_end)
}

fn polylines_cross(a: &[Point], b: &[Point]) -> bool {
    for window_a in a.windows(2) {
        for window_b in b.windows(2) {
            if segments_cross(window_a[0], window_a[1], window_b[0], window_b[1]) {
                return true;
            }
        }
    }
    false
}

fn check_edge_on_group_borders(diagram: &Diagram, result: &LayoutResult, out: &mut Vec<LayoutViolation>) {
    for (index, edge) in result.edges.iter().enumerate() {
        if edge.path_len() < 2 {
            continue;
        }
        let rel = &diagram.relations[index];
        let path = edge.path_points();

        for window in path.windows(2) {
            let a = window[0];
            let b = window[1];
            let mut group_ids: Vec<&String> = result.groups.keys().collect();
            group_ids.sort();
            for gid in group_ids {
                let gl = &result.groups[gid];
                if gl.width <= 0.0 || gl.height <= 0.0 {
                    continue;
                }
                if segment_on_group_border(a, b, gl) {
                    out.push(
                        LayoutViolation::new(
                            LintRuleId::EdgeOnGroupBorder,
                            format!(
                                "边 {} → {} 与分组 '{}' 边框重合",
                                rel.from.as_str(),
                                rel.to.as_str(),
                                gid
                            ),
                        )
                        .with_edge_index(index)
                        .with_groups([gid.as_str()]),
                    );
                }
            }
        }
    }
}

fn check_edge_crosses_group_interior(diagram: &Diagram, result: &LayoutResult, out: &mut Vec<LayoutViolation>) {
    let entity_group = entity_to_group_map(diagram);
    let ancestor_sets = build_group_ancestor_sets(diagram);

    for (index, edge) in result.edges.iter().enumerate() {
        if edge.path_len() < 2 {
            continue;
        }
        let rel = &diagram.relations[index];
        let from_related = endpoint_related_groups(rel.from.as_str(), &entity_group, &ancestor_sets);
        let to_related = endpoint_related_groups(rel.to.as_str(), &entity_group, &ancestor_sets);
        let path = edge.path_points();

        let mut group_ids: Vec<&String> = result.groups.keys().collect();
        group_ids.sort();

        for gid in group_ids {
            if from_related.contains(gid.as_str()) || to_related.contains(gid.as_str()) {
                continue;
            }
            let gl = &result.groups[gid];
            if gl.width <= 0.0 || gl.height <= 0.0 {
                continue;
            }
            for window in path.windows(2) {
                let mid = segment_midpoint(window[0], window[1]);
                if point_in_rect_interior(mid.x, mid.y, gl) {
                    out.push(
                        LayoutViolation::new(
                            LintRuleId::EdgeCrossesGroupInterior,
                            format!(
                                "边 {} → {} 穿过分组 '{}' 内部",
                                rel.from.as_str(),
                                rel.to.as_str(),
                                gid
                            ),
                        )
                        .with_edge_index(index)
                        .with_groups([gid.as_str()]),
                    );
                    break;
                }
            }
        }
    }
}

fn entity_to_group_map(diagram: &Diagram) -> HashMap<String, String> {
    diagram
        .entities
        .iter()
        .filter_map(|e| {
            e.group_id
                .as_ref()
                .map(|g| (e.id.as_str().to_string(), g.as_str().to_string()))
        })
        .collect()
}

/// 预计算的分组关系映射，用于批量检查边是否穿越分组内部。
/// 避免在循环中对每条边重复构建 `entity_to_group_map` 和 `ancestor_sets`。
pub struct GroupInteriorMaps {
    entity_group: HashMap<String, String>,
    ancestor_sets: HashMap<String, HashSet<String>>,
}

impl GroupInteriorMaps {
    pub fn new(diagram: &Diagram) -> Self {
        Self {
            entity_group: entity_to_group_map(diagram),
            ancestor_sets: build_group_ancestor_sets(diagram),
        }
    }
}

/// 使用预计算 maps 检查指定边是否穿越非端点分组内部。
/// 与 `edge_index_crosses_group_interior` 逻辑相同，但复用 maps 避免重复构建。
pub fn edge_crosses_group_interior_with_maps(
    diagram: &Diagram,
    result: &LayoutResult,
    edge_index: usize,
    maps: &GroupInteriorMaps,
) -> bool {
    let Some(edge) = result.edges.get(edge_index) else {
        return false;
    };
    if edge.path_len() < 2 {
        return false;
    }
    let Some(rel) = diagram.relations.get(edge_index) else {
        return false;
    };
    let from_related =
        endpoint_related_groups(rel.from.as_str(), &maps.entity_group, &maps.ancestor_sets);
    let to_related =
        endpoint_related_groups(rel.to.as_str(), &maps.entity_group, &maps.ancestor_sets);
    let path = edge.path_points();

    let mut group_ids: Vec<&String> = result.groups.keys().collect();
    group_ids.sort();

    for gid in group_ids {
        if from_related.contains(gid.as_str()) || to_related.contains(gid.as_str()) {
            continue;
        }
        let Some(gl) = result.groups.get(gid) else {
            continue;
        };
        if gl.width <= 0.0 || gl.height <= 0.0 {
            continue;
        }
        for window in path.windows(2) {
            let mid = segment_midpoint(window[0], window[1]);
            if point_in_rect_interior(mid.x, mid.y, gl) {
                return true;
            }
        }
    }
    false
}

/// 检查指定边是否穿越非端点分组内部（每次调用都会重建 maps，适合单次检查）。
/// 批量检查时请用 `GroupInteriorMaps` + `edge_crosses_group_interior_with_maps`。
pub fn edge_index_crosses_group_interior(
    diagram: &Diagram,
    result: &LayoutResult,
    edge_index: usize,
) -> bool {
    let maps = GroupInteriorMaps::new(diagram);
    edge_crosses_group_interior_with_maps(diagram, result, edge_index, &maps)
}

/// 每个 group 的祖先链（含自身），用于判断边是否「合法」穿过容器内部。
fn build_group_ancestor_sets(diagram: &Diagram) -> HashMap<String, HashSet<String>> {
    let parent_of: HashMap<String, String> = diagram
        .groups
        .iter()
        .filter_map(|g| {
            g.parent_id
                .as_ref()
                .map(|p| (g.id.as_str().to_string(), p.as_str().to_string()))
        })
        .collect();

    let mut cache = HashMap::new();
    for group in &diagram.groups {
        let gid = group.id.as_str().to_string();
        let mut set = HashSet::new();
        let mut current = Some(gid.clone());
        while let Some(g) = current {
            if !set.insert(g.clone()) {
                break;
            }
            current = parent_of.get(&g).cloned();
        }
        cache.insert(gid, set);
    }
    cache
}

fn endpoint_related_groups(
    entity_id: &str,
    entity_group: &HashMap<String, String>,
    ancestor_sets: &HashMap<String, HashSet<String>>,
) -> HashSet<String> {
    let Some(direct) = entity_group.get(entity_id) else {
        return HashSet::new();
    };
    ancestor_sets
        .get(direct)
        .cloned()
        .unwrap_or_else(|| HashSet::from([direct.clone()]))
}

// ─── Edge Bundling Lint 检查 ───────────────────────────────────────

/// 检查同 bundle 内多条边指向同一节点（箭头冗余）。
///
/// 当 2+ 条边从同一 bundle 指向同一目标节点时，这些箭头在视觉上冗余，
/// 可能只需要一个合并的箭头。
fn check_bundled_arrow_convergence(
    diagram: &Diagram,
    bundling: &BundlingResult,
    out: &mut Vec<LayoutViolation>,
) {
    for bundle in &bundling.bundles {
        if bundle.edges.len() < 2 {
            continue;
        }
        // 统计 bundle 内每条边指向的 to 节点
        let mut to_groups: HashMap<&str, Vec<usize>> = HashMap::new();
        for &ei in &bundle.edges {
            if let Some(rel) = diagram.relations.get(ei) {
                to_groups.entry(rel.to.as_str()).or_default().push(ei);
            }
        }
        for (to_id, edge_indices) in &to_groups {
            if edge_indices.len() >= 2 {
                let edge_list: Vec<String> = edge_indices.iter().map(|i| i.to_string()).collect();
                out.push(
                    LayoutViolation::new(
                        LintRuleId::BundledArrowConvergence,
                        format!(
                            "bundle {} 中有 {} 条边汇聚到节点 '{}'（索引: {}），箭头冗余",
                            bundle.id,
                            edge_indices.len(),
                            to_id,
                            edge_list.join(", "),
                        ),
                    )
                    .with_metric(edge_indices.len() as f64)
                    .with_entities(edge_indices.iter().map(|i| i.to_string())),
                );
            }
        }
    }
}

/// 检查同 bundle 内存在语义反向的边。
///
/// 当 bundle 中包含出入方向相反的边时（例如 A→B 与 B→A 或 C→B），
/// 说明出入方向不一致，合并到同一主干会造成视觉混淆。
fn check_bundled_opposite_flow(
    diagram: &Diagram,
    bundling: &BundlingResult,
    out: &mut Vec<LayoutViolation>,
) {
    for bundle in &bundling.bundles {
        if bundle.edges.len() < 2 {
            continue;
        }
        // 收集 bundle 内每条边的 (from, to)
        let pairs: Vec<(usize, &str, &str)> = bundle
            .edges
            .iter()
            .filter_map(|&ei| {
                diagram
                    .relations
                    .get(ei)
                    .map(|r| (ei, r.from.as_str(), r.to.as_str()))
            })
            .collect();

        // 检查反向边：若存在 (A→B) 和 (B→A)，即方向相反
        for i in 0..pairs.len() {
            for j in (i + 1)..pairs.len() {
                let (ei_a, from_a, to_a) = pairs[i];
                let (ei_b, from_b, to_b) = pairs[j];
                // 反向边：A→B vs B→A，或 B→A vs A→B
                if from_a == to_b && to_a == from_b {
                    out.push(
                        LayoutViolation::new(
                            LintRuleId::BundledOppositeFlow,
                            format!(
                                "bundle {} 中包含反向边: index={} ({}→{}) 与 index={} ({}→{})",
                                bundle.id, ei_a, from_a, to_a, ei_b, from_b, to_b,
                            ),
                        )
                        .with_entities([ei_a.to_string(), ei_b.to_string()]),
                    );
                }
            }
        }
    }
}

/// 检查 bundle 的 merge leg 分叉点过密。
///
/// 当同一 bundle 的 entry_points 间距过近时，合入腿（merge leg）过于密集，
/// 可能导致视觉重叠。阈值取 `fork_spacing` 默认值 8px。
fn check_bundle_merge_density(
    bundling: &BundlingResult,
    out: &mut Vec<LayoutViolation>,
) {
    const MIN_ENTRY_SPACING: f64 = 8.0;

    for bundle in &bundling.bundles {
        if bundle.entry_points.len() < 2 {
            continue;
        }
        for i in 0..bundle.entry_points.len() {
            for j in (i + 1)..bundle.entry_points.len() {
                let a = bundle.entry_points[i];
                let b = bundle.entry_points[j];
                let dist = ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt();
                if dist > 0.0 && dist < MIN_ENTRY_SPACING {
                    // 确保两条边都存在
                    let ei_a = bundle.edges.get(i).copied();
                    let ei_b = bundle.edges.get(j).copied();
                    if let (Some(a), Some(b)) = (ei_a, ei_b) {
                        out.push(
                            LayoutViolation::new(
                                LintRuleId::BundleMergeDensity,
                                format!(
                                    "bundle {} 中 merge leg 分叉点过密: index={} 与 index={} 间距 {:.1}px",
                                    bundle.id, a, b, dist,
                                ),
                            )
                            .with_metric(dist)
                            .with_entities([a.to_string(), b.to_string()]),
                        );
                    }
                }
            }
        }
    }
}

/// 检查 bundle 的 fork leg 交叉。
///
/// 同一 bundle 内不同边的 fork leg（从 exit_point 到 to 端口的路径段）可能交叉，
/// 造成视觉混乱。
fn check_bundle_fork_overlap(
    result: &LayoutResult,
    bundling: &BundlingResult,
    out: &mut Vec<LayoutViolation>,
) {
    for bundle in &bundling.bundles {
        if bundle.edges.len() < 2 {
            continue;
        }
        // 收集 bundle 内每条边的 fork leg 段（从 exit_point 到路径终点）
        let fork_segments: Vec<(usize, Vec<Point>)> = bundle
            .edges
            .iter()
            .filter_map(|&ei| {
                let edge = result.edges.get(ei)?;
                let path = edge.path_points();
                if path.len() < 2 {
                    return None;
                }
                // 找 exit_point 在路径中的位置，取之后的段作为 fork leg
                if let Some(&exit) = bundle.exit_points.get(
                    bundle.edges.iter().position(|&e| e == ei)?
                ) {
                    // 从离 exit_point 最近的点开始
                    let start_idx = path
                        .iter()
                        .position(|p| (p.x - exit.x).abs() < 1.0 && (p.y - exit.y).abs() < 1.0)
                        .unwrap_or(0);
                    let fork = path[start_idx..].to_vec();
                    if fork.len() >= 2 {
                        return Some((ei, fork));
                    }
                }
                None
            })
            .collect();

        for i in 0..fork_segments.len() {
            for j in (i + 1)..fork_segments.len() {
                let (ei_a, seg_a) = &fork_segments[i];
                let (ei_b, seg_b) = &fork_segments[j];
                if polylines_cross(seg_a, seg_b) {
                    out.push(
                        LayoutViolation::new(
                            LintRuleId::BundleForkOverlap,
                            format!(
                                "bundle {} 中 fork leg 交叉: index={} 与 index={}",
                                bundle.id, ei_a, ei_b,
                            ),
                        )
                        .with_entities([ei_a.to_string(), ei_b.to_string()]),
                    );
                }
            }
        }
    }
}

/// 检查 bundle 主干是否穿过节点。
///
/// 主干段（trunk_start → trunk_end）是 bundle 内所有边的共享路径，
/// 不应穿过任何非端点节点。
fn check_bundle_trunk_through_node(
    diagram: &Diagram,
    result: &LayoutResult,
    bundling: &BundlingResult,
    out: &mut Vec<LayoutViolation>,
) {
    for bundle in &bundling.bundles {
        let trunk_start = bundle.trunk_start;
        let trunk_end = bundle.trunk_end;
        // 跳过退化主干
        let trunk_len = ((trunk_end.x - trunk_start.x).powi(2)
            + (trunk_end.y - trunk_start.y).powi(2))
        .sqrt();
        if trunk_len < 1.0 {
            continue;
        }

        // 收集 bundle 内所有边涉及的节点（端点节点不应被报告）
        let mut endpoint_nodes: HashSet<&str> = HashSet::new();
        for &ei in &bundle.edges {
            if let Some(rel) = diagram.relations.get(ei) {
                endpoint_nodes.insert(rel.from.as_str());
                endpoint_nodes.insert(rel.to.as_str());
            }
        }

        let mut node_ids: Vec<&String> = result.nodes.keys().collect();
        node_ids.sort();
        for node_id in node_ids {
            if endpoint_nodes.contains(node_id.as_str()) {
                continue;
            }
            let nl = &result.nodes[node_id];
            if segment_intersects_node(trunk_start, trunk_end, nl) {
                out.push(
                    LayoutViolation::new(
                        LintRuleId::BundleTrunkThroughNode,
                        format!(
                            "bundle {} 主干穿过节点 '{}'",
                            bundle.id, node_id,
                        ),
                    )
                    .with_entities([node_id.as_str()]),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        ArrowType, AttributeMap, Diagram, Entity, Group, Identifier, Relation, SourceInfo, Span,
    };
    use crate::layout::{GroupLayout, NodeLayout};
    use std::collections::HashMap;

    fn dummy_span() -> Span {
        Span::dummy()
    }

    fn node(_id: &str, x: f64, y: f64, w: f64, h: f64) -> NodeLayout {
        NodeLayout {
            x,
            y,
            width: w,
            height: h,
            ..Default::default()
        }
    }

    fn group_layout(x: f64, y: f64, w: f64, h: f64) -> GroupLayout {
        GroupLayout { x, y, width: w, height: h }
    }

    #[test]
    fn detects_node_overlap() {
        let span = dummy_span();
        let diagram = Diagram {
            diagram_type: crate::types::DiagramType::Flowchart,
            entities: vec![
                Entity {
                    id: Identifier::new_unchecked("a"),
                    label: "A".into(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
                Entity {
                    id: Identifier::new_unchecked("b"),
                    label: "B".into(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
            ],
            relations: vec![],
            groups: vec![],
            ..Default::default()
        };
        let result = LayoutResult {
            nodes: HashMap::from([
                ("a".into(), node("a", 0.0, 0.0, 100.0, 50.0)),
                ("b".into(), node("b", 50.0, 10.0, 100.0, 50.0)),
            ]),
            groups: HashMap::new(),
            edges: vec![],
            total_width: 200.0,
            total_height: 100.0,
            hints: Default::default(),
        };

        let report = lint_layout(&diagram, &result);
        assert!(report.by_rule(LintRuleId::NodeOverlap).next().is_some());
        assert!(!report.is_clean());
    }

    #[test]
    fn detects_group_overlap() {
        let span = dummy_span();
        let diagram = Diagram {
            diagram_type: crate::types::DiagramType::Architecture,
            entities: vec![],
            relations: vec![],
            groups: vec![
                Group {
                    id: Identifier::new_unchecked("g1"),
                    label: "G1".into(),
                    attributes: AttributeMap::default(),
                    parent_id: None,
                    depth: 0,
                    entity_ids: vec![],
                    child_group_ids: vec![],
                    span,
                },
                Group {
                    id: Identifier::new_unchecked("g2"),
                    label: "G2".into(),
                    attributes: AttributeMap::default(),
                    parent_id: None,
                    depth: 0,
                    entity_ids: vec![],
                    child_group_ids: vec![],
                    span,
                },
            ],
            ..Default::default()
        };
        let result = LayoutResult {
            nodes: HashMap::new(),
            groups: HashMap::from([
                ("g1".into(), group_layout(0.0, 0.0, 200.0, 100.0)),
                ("g2".into(), group_layout(100.0, 20.0, 200.0, 100.0)),
            ]),
            edges: vec![],
            total_width: 400.0,
            total_height: 200.0,
            hints: Default::default(),
        };

        let report = lint_layout(&diagram, &result);
        assert!(report.by_rule(LintRuleId::GroupOverlap).next().is_some());
    }

    #[test]
    fn skips_nested_group_overlap() {
        let span = dummy_span();
        let diagram = Diagram {
            diagram_type: crate::types::DiagramType::Architecture,
            entities: vec![],
            relations: vec![],
            groups: vec![
                Group {
                    id: Identifier::new_unchecked("parent"),
                    label: "P".into(),
                    attributes: AttributeMap::default(),
                    parent_id: None,
                    depth: 0,
                    entity_ids: vec![],
                    child_group_ids: vec![Identifier::new_unchecked("child")],
                    span,
                },
                Group {
                    id: Identifier::new_unchecked("child"),
                    label: "C".into(),
                    attributes: AttributeMap::default(),
                    parent_id: Some(Identifier::new_unchecked("parent")),
                    depth: 1,
                    entity_ids: vec![],
                    child_group_ids: vec![],
                    span,
                },
            ],
            ..Default::default()
        };
        let result = LayoutResult {
            nodes: HashMap::new(),
            groups: HashMap::from([
                ("parent".into(), group_layout(0.0, 0.0, 300.0, 200.0)),
                ("child".into(), group_layout(20.0, 20.0, 100.0, 80.0)),
            ]),
            edges: vec![],
            total_width: 300.0,
            total_height: 200.0,
            hints: Default::default(),
        };

        let report = lint_layout(&diagram, &result);
        assert!(report.by_rule(LintRuleId::GroupOverlap).next().is_none());
    }

    #[test]
    fn detects_node_outside_group() {
        let span = dummy_span();
        let diagram = Diagram {
            diagram_type: crate::types::DiagramType::Architecture,
            entities: vec![Entity {
                id: Identifier::new_unchecked("n1"),
                label: "N".into(),
                attributes: AttributeMap::default(),
                group_id: Some(Identifier::new_unchecked("g1")),
                span,
            }],
            relations: vec![],
            groups: vec![Group {
                id: Identifier::new_unchecked("g1"),
                label: "G".into(),
                attributes: AttributeMap::default(),
                parent_id: None,
                depth: 0,
                entity_ids: vec![Identifier::new_unchecked("n1")],
                child_group_ids: vec![],
                span,
            }],
            ..Default::default()
        };
        let result = LayoutResult {
            nodes: HashMap::from([("n1".into(), node("n1", -10.0, 10.0, 80.0, 40.0))]),
            groups: HashMap::from([("g1".into(), group_layout(0.0, 0.0, 200.0, 100.0))]),
            edges: vec![],
            total_width: 200.0,
            total_height: 100.0,
            hints: Default::default(),
        };

        let report = lint_layout(&diagram, &result);
        assert!(report.by_rule(LintRuleId::NodeOutsideGroup).next().is_some());
    }

    #[test]
    fn skips_ancestor_group_interior() {
        let span = dummy_span();
        let diagram = Diagram {
            diagram_type: crate::types::DiagramType::Architecture,
            entities: vec![
                Entity {
                    id: Identifier::new_unchecked("a"),
                    label: "A".into(),
                    attributes: AttributeMap::default(),
                    group_id: Some(Identifier::new_unchecked("child")),
                    span,
                },
                Entity {
                    id: Identifier::new_unchecked("b"),
                    label: "B".into(),
                    attributes: AttributeMap::default(),
                    group_id: Some(Identifier::new_unchecked("child")),
                    span,
                },
            ],
            relations: vec![Relation {
                from: Identifier::new_unchecked("a"),
                to: Identifier::new_unchecked("b"),
                arrow: ArrowType::Active,
                label: None,
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span,
            }],
            groups: vec![
                Group {
                    id: Identifier::new_unchecked("parent"),
                    label: "P".into(),
                    attributes: AttributeMap::default(),
                    parent_id: None,
                    depth: 0,
                    entity_ids: vec![],
                    child_group_ids: vec![Identifier::new_unchecked("child")],
                    span,
                },
                Group {
                    id: Identifier::new_unchecked("child"),
                    label: "C".into(),
                    attributes: AttributeMap::default(),
                    parent_id: Some(Identifier::new_unchecked("parent")),
                    depth: 1,
                    entity_ids: vec![
                        Identifier::new_unchecked("a"),
                        Identifier::new_unchecked("b"),
                    ],
                    child_group_ids: vec![],
                    span,
                },
            ],
            ..Default::default()
        };
        let result = LayoutResult {
            nodes: HashMap::from([
                ("a".into(), node("a", 30.0, 40.0, 60.0, 30.0)),
                ("b".into(), node("b", 30.0, 120.0, 60.0, 30.0)),
            ]),
            groups: HashMap::from([
                ("parent".into(), group_layout(0.0, 0.0, 200.0, 200.0)),
                ("child".into(), group_layout(20.0, 20.0, 160.0, 160.0)),
            ]),
            edges: vec![crate::layout::EdgeLayout {
                geometry: crate::layout::PathGeometry::Polyline {
                    points: vec![Point::new(60.0, 55.0), Point::new(60.0, 100.0), Point::new(60.0, 120.0)],
                },
                labels: vec![],
                from_port: crate::layout::Port::Bottom,
                to_port: crate::layout::Port::Top,
            }],
            total_width: 200.0,
            total_height: 200.0,
            hints: Default::default(),
        };

        let report = lint_layout(&diagram, &result);
        assert!(
            report.by_rule(LintRuleId::EdgeCrossesGroupInterior).next().is_none(),
            "穿过父 group 内部连接子 group 内节点应允许"
        );
    }

    #[test]
    fn config_can_disable_rules() {
        let span = dummy_span();
        let diagram = Diagram {
            diagram_type: crate::types::DiagramType::Flowchart,
            entities: vec![
                Entity {
                    id: Identifier::new_unchecked("a"),
                    label: "A".into(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
                Entity {
                    id: Identifier::new_unchecked("b"),
                    label: "B".into(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
            ],
            relations: vec![],
            groups: vec![],
            ..Default::default()
        };
        let result = LayoutResult {
            nodes: HashMap::from([
                ("a".into(), node("a", 0.0, 0.0, 100.0, 50.0)),
                ("b".into(), node("b", 50.0, 10.0, 100.0, 50.0)),
            ]),
            groups: HashMap::new(),
            edges: vec![],
            total_width: 200.0,
            total_height: 100.0,
            hints: Default::default(),
        };

        let report = LayoutLinter::with_config(LintConfig::strict().without(&[LintRuleId::NodeOverlap]))
            .run(&diagram, &result);
        assert!(report.by_rule(LintRuleId::NodeOverlap).next().is_none());
        assert!(report.is_clean());
    }

    #[test]
    fn clean_layout_has_no_errors() {
        let span = dummy_span();
        let diagram = Diagram {
            diagram_type: crate::types::DiagramType::Flowchart,
            entities: vec![
                Entity {
                    id: Identifier::new_unchecked("a"),
                    label: "A".into(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
                Entity {
                    id: Identifier::new_unchecked("b"),
                    label: "B".into(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
            ],
            relations: vec![Relation {
                from: Identifier::new_unchecked("a"),
                to: Identifier::new_unchecked("b"),
                arrow: ArrowType::Active,
                label: None,
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span,
            }],
            groups: vec![],
            source_info: SourceInfo::default(),
            ..Default::default()
        };
        let result = LayoutResult {
            nodes: HashMap::from([
                ("a".into(), node("a", 0.0, 0.0, 80.0, 40.0)),
                ("b".into(), node("b", 200.0, 0.0, 80.0, 40.0)),
            ]),
            groups: HashMap::new(),
            edges: vec![crate::layout::EdgeLayout {
                geometry: crate::layout::PathGeometry::Straight {
                    start: Point::new(80.0, 20.0),
                    end: Point::new(200.0, 20.0),
                },
                labels: vec![],
                from_port: crate::layout::Port::Right,
                to_port: crate::layout::Port::Left,
            }],
            total_width: 300.0,
            total_height: 60.0,
            hints: Default::default(),
        };

        let report = lint_layout(&diagram, &result);
        assert!(report.is_clean());
        assert_eq!(report.error_count(), 0);
    }
}
