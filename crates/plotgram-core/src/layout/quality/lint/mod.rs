//! 布局静态质量检查（LayoutLint）。
//!
//! 在 `LayoutResult` 上运行一组确定性几何规则，输出可追溯到 DSL 实体的违规列表。
//! 供 CLI、测试、eval 框架消费；不依赖 SVG 渲染。

mod advice;
mod config;
mod geometry;
mod violation;

use advice::generate_lint_advices;
pub use config::{
    parse_lint_profile, parse_lint_rule, parse_lint_rules_list, LintConfig, LintProfile, RuleConfig,
};
pub use violation::{
    AdviceConfidence, LayoutKnob, LintAdvice, LayoutViolation, LintReport, LintRuleId,
    LintSeverity,
};

use crate::layout::routing::common::label_avoidance::aabb_overlap;

use crate::ast::Diagram;
use crate::layout::routing::segment_pair::{
    find_needs_separation_edge_pairs, SeparationReason,
};
use crate::layout::geometry::Point;
use crate::layout::refine::segment_intersects_node;
use crate::layout::{ContainmentViolationKind, LayoutResult};
use geometry::{
    group_overlap_area, node_overlap_area, segment_on_group_border, segments_cross,
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
        if cfg.is_enabled(LintRuleId::UnrelatedEdgeTrunkMerge) {
            check_unrelated_edge_trunk_merge(diagram, result, &mut violations);
        }
        if cfg.is_enabled(LintRuleId::EdgeOnGroupBorder) {
            check_edge_on_group_borders(diagram, result, &mut violations);
        }
        if cfg.is_enabled(LintRuleId::EdgeCrossesGroupInterior) {
            check_edge_crosses_group_interior(diagram, result, &mut violations);
        }
        if cfg.is_enabled(LintRuleId::LabelNodeOverlap) {
            check_label_node_overlaps(diagram, result, &mut violations);
        }
        if cfg.is_enabled(LintRuleId::LabelLabelOverlap) {
            check_label_label_overlaps(result, &mut violations);
        }
        if cfg.is_enabled(LintRuleId::SiblingWidthRatio) {
            check_sibling_width_ratios(result, &mut violations);
        }

        let mut violations = finalize_violations(cfg, violations);
        sort_violations(&mut violations);
        let advices = if cfg.advice_enabled {
            generate_lint_advices(diagram, result, &violations)
        } else {
            Vec::new()
        };
        LintReport {
            violations,
            advices,
        }
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
        let severity_rank = |severity: LintSeverity| match severity {
            LintSeverity::Error => 0,
            LintSeverity::Warning => 1,
        };
        a.rule
            .as_str()
            .cmp(b.rule.as_str())
            .then_with(|| severity_rank(a.severity).cmp(&severity_rank(b.severity)))
            .then_with(|| a.message.cmp(&b.message))
            .then_with(|| a.group_ids.cmp(&b.group_ids))
            .then_with(|| a.entity_ids.cmp(&b.entity_ids))
            .then_with(|| a.edge_index.cmp(&b.edge_index))
            .then_with(|| a.related_edge_indices.cmp(&b.related_edge_indices))
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

/// Iteration 3：同 y 行（RankBand）的 sibling group 宽比软指标。
///
/// 阈值 1.08（与方案验收一致）；仅 warning，不进 CI strict。
fn check_sibling_width_ratios(result: &LayoutResult, out: &mut Vec<LayoutViolation>) {
    const Y_BAND_EPS: f64 = 8.0;
    const MAX_RATIO: f64 = 1.08;

    let mut ids: Vec<&String> = result.groups.keys().collect();
    ids.sort();
    if ids.len() < 2 {
        return;
    }

    let mut visited = vec![false; ids.len()];
    for i in 0..ids.len() {
        if visited[i] {
            continue;
        }
        let yi = result.groups[ids[i]].y;
        let mut band: Vec<usize> = vec![i];
        visited[i] = true;
        for j in (i + 1)..ids.len() {
            if visited[j] {
                continue;
            }
            if (result.groups[ids[j]].y - yi).abs() <= Y_BAND_EPS {
                visited[j] = true;
                band.push(j);
            }
        }
        if band.len() < 2 {
            continue;
        }
        let mut min_w = f64::MAX;
        let mut max_w = 0.0f64;
        let mut min_id = ids[band[0]].as_str();
        let mut max_id = ids[band[0]].as_str();
        for &idx in &band {
            let w = result.groups[ids[idx]].width;
            if w < min_w {
                min_w = w;
                min_id = ids[idx].as_str();
            }
            if w > max_w {
                max_w = w;
                max_id = ids[idx].as_str();
            }
        }
        if min_w <= 1.0 {
            continue;
        }
        let ratio = max_w / min_w;
        if ratio > MAX_RATIO {
            out.push(
                LayoutViolation::new(
                    LintRuleId::SiblingWidthRatio,
                    format!(
                        "同级条带宽比过大：'{max_id}'/{max_w:.0} vs '{min_id}'/{min_w:.0} = {ratio:.3}"
                    ),
                )
                .with_metric(ratio)
                .with_groups([max_id, min_id]),
            );
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
    let mut node_ids: Vec<&String> = result.nodes.keys().collect();
    node_ids.sort();

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
            for node_id in &node_ids {
                let node_id = node_id.as_str();
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
                        .with_entities([from_id, to_id, node_id]),
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
                    .with_related_edges([i, j]),
                );
            }
        }
    }
}

/// 统计当前布局的边交叉总数（复用 check_edge_crossings 的采样+跨越判定口径）。
/// 用于 post-route 分离的 crossing-neutral 守卫：只保留不增加总交叉的偏移。
pub fn count_edge_crossings(result: &LayoutResult) -> usize {
    let edges = &result.edges;
    if edges.len() < 2 {
        return 0;
    }
    let sampled: Vec<Vec<Point>> = edges.iter().map(|e| e.sampled_path(16)).collect();
    let mut count = 0usize;
    for i in 0..sampled.len() {
        for j in (i + 1)..sampled.len() {
            if edges_share_endpoint(&edges[i], &edges[j]) {
                continue;
            }
            if polylines_cross(&sampled[i], &sampled[j]) {
                count += 1;
            }
        }
    }
    count
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
    let mut group_ids: Vec<&String> = result.groups.keys().collect();
    group_ids.sort();

    for (index, edge) in result.edges.iter().enumerate() {
        if edge.path_len() < 2 {
            continue;
        }
        let rel = &diagram.relations[index];
        let path = edge.path_points();

        for window in path.windows(2) {
            let a = window[0];
            let b = window[1];
            for gid in &group_ids {
                let gl = &result.groups[*gid];
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
    let mut group_ids: Vec<&String> = result.groups.keys().collect();
    group_ids.sort();

    for (index, edge) in result.edges.iter().enumerate() {
        if edge.path_len() < 2 {
            continue;
        }
        let rel = &diagram.relations[index];
        let from_related = endpoint_related_groups(rel.from.as_str(), &entity_group, &ancestor_sets);
        let to_related = endpoint_related_groups(rel.to.as_str(), &entity_group, &ancestor_sets);
        let path = edge.path_points();

        for gid in &group_ids {
            if from_related.contains(gid.as_str()) || to_related.contains(gid.as_str()) {
                continue;
            }
            let gl = &result.groups[*gid];
            if gl.width <= 0.0 || gl.height <= 0.0 {
                continue;
            }
            for window in path.windows(2) {
                if crate::layout::routing::common::geom_obstacle::segment_pierces_group_interior(
                    window[0], window[1], gl,
                ) {
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
            if crate::layout::routing::common::geom_obstacle::segment_pierces_group_interior(
                window[0], window[1], gl,
            ) {
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

// ─── 架构图假并线检测（P1：走 segment_pair::Classify）────────────────

/// 返回所有非语义平行段重叠的边对 (i, j)。
///
/// 裁决统一走 [`find_needs_separation_edge_pairs`]；仅保留
/// exact 的 `NonSemanticTrunk`（与历史 UnrelatedEdgeTrunkMerge 对齐：
/// 长 trunk 共线重合，不含紧间距）。
fn find_unrelated_parallel_overlaps(
    diagram: &Diagram,
    result: &LayoutResult,
) -> Vec<(usize, usize)> {
    find_needs_separation_edge_pairs(diagram, result)
        .into_iter()
        .filter(|(_, _, reason, _)| matches!(reason, SeparationReason::NonSemanticTrunk))
        .map(|(i, j, _, _)| (i, j))
        .collect()
}

/// 计算非语义平行段重叠对数（所有图类型，供 eval 框架消费）。
///
/// 语义门控：architecture 图要求边对至少共享一个 `MergeGroup` 才允许共享 trunk；
/// 其他图类型不产生 `NonSemanticTrunk`，本函数返回 0。
pub fn count_unrelated_parallel_overlaps(diagram: &Diagram, result: &LayoutResult) -> usize {
    find_unrelated_parallel_overlaps(diagram, result).len()
}

/// 检测不同源/宿边是否共享长 trunk 段（架构图语义门控未允许的假并线）。
fn check_unrelated_edge_trunk_merge(
    diagram: &Diagram,
    result: &LayoutResult,
    out: &mut Vec<LayoutViolation>,
) {
    use crate::types::DiagramType;

    if diagram.diagram_type != DiagramType::Architecture {
        return;
    }

    for (i, j) in find_unrelated_parallel_overlaps(diagram, result) {
        let rel_i = &diagram.relations[i];
        let rel_j = &diagram.relations[j];
        out.push(
            LayoutViolation::new(
                LintRuleId::UnrelatedEdgeTrunkMerge,
                format!(
                    "边 {i} ({}→{}) 与边 {j} ({}→{}) 共享非语义 trunk 段",
                    rel_i.from.as_str(),
                    rel_i.to.as_str(),
                    rel_j.from.as_str(),
                    rel_j.to.as_str(),
                ),
            )
            .with_edge_index(i)
            .with_related_edges([i, j])
            .with_entities([
                rel_i.from.as_str(),
                rel_i.to.as_str(),
                rel_j.from.as_str(),
                rel_j.to.as_str(),
            ]),
        );
    }
}

/// Lint 指标摘要（供 eval 框架与基线对比消费）。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(default)]
pub struct LintMetricsSummary {
    pub node_overlap: usize,
    pub group_overlap: usize,
    pub node_outside_group: usize,
    pub child_group_outside_parent: usize,
    pub edge_through_node: usize,
    pub edge_crossing: usize,
    pub edge_on_group_border: usize,
    pub edge_crosses_group_interior: usize,
    pub label_node_overlap: usize,
    pub label_label_overlap: usize,
    /// 不同源/宿边共享非语义 trunk 段（架构图假并线）
    pub unrelated_edge_trunk_merge: usize,
    /// 同级 sibling 同 RankBand 宽比过大（对称性软指标）
    pub sibling_width_ratio: usize,
    /// B1 诊断轨：Σ 边穿**非端点节点**内部的重叠长度（连续量，非门禁硬指标）。
    #[serde(default)]
    pub pierce_node_sev: f64,
    /// B1 诊断轨：Σ 边穿**非端点分组**内部的重叠长度（连续量，非门禁硬指标）。
    #[serde(default)]
    pub pierce_group_sev: f64,
    pub total_violations: usize,
    pub error_count: usize,
    pub warning_count: usize,
}

impl LintMetricsSummary {
    /// 从 lint 报告聚合指标。
    pub fn from_report(report: &LintReport) -> Self {
        let mut summary = Self {
            total_violations: report.violations.len(),
            error_count: report.error_count(),
            warning_count: report.warning_count(),
            ..Default::default()
        };
        for v in &report.violations {
            match v.rule {
                LintRuleId::NodeOverlap => summary.node_overlap += 1,
                LintRuleId::GroupOverlap => summary.group_overlap += 1,
                LintRuleId::NodeOutsideGroup => summary.node_outside_group += 1,
                LintRuleId::ChildGroupOutsideParent => summary.child_group_outside_parent += 1,
                LintRuleId::EdgeThroughNode => summary.edge_through_node += 1,
                LintRuleId::EdgeCrossing => summary.edge_crossing += 1,
                LintRuleId::EdgeOnGroupBorder => summary.edge_on_group_border += 1,
                LintRuleId::EdgeCrossesGroupInterior => summary.edge_crosses_group_interior += 1,
                LintRuleId::LabelNodeOverlap => summary.label_node_overlap += 1,
                LintRuleId::LabelLabelOverlap => summary.label_label_overlap += 1,
                LintRuleId::UnrelatedEdgeTrunkMerge => summary.unrelated_edge_trunk_merge += 1,
                LintRuleId::SiblingWidthRatio => summary.sibling_width_ratio += 1,
            }
        }
        summary
    }
}

/// 计算布局质量 lint 指标（verbose 配置，含全部规则）。
pub fn compute_lint_metrics(diagram: &Diagram, result: &LayoutResult) -> LintMetricsSummary {
    let report = LayoutLinter::with_config(LintConfig::verbose()).run(diagram, result);
    let mut summary = LintMetricsSummary::from_report(&report);
    let (node_sev, group_sev) = compute_pierce_severity(diagram, result);
    summary.pierce_node_sev = node_sev;
    summary.pierce_group_sev = group_sev;
    summary
}

/// B1 诊断轨：边穿障**连续严重度**（Σ 穿内部重叠长度）。
///
/// 端点豁免口径与 `check_edge_through_nodes` / `check_edge_crosses_group_interior`
/// 一致（跳过 from/to 节点、端点相关分组，长路径跳首末 stub 段），仅将布尔判定
/// 替换为重叠长度累加。属**只读诊断**，不参与硬门禁，也不移动任何几何。
fn compute_pierce_severity(diagram: &Diagram, result: &LayoutResult) -> (f64, f64) {
    use crate::layout::geometry::{Rect, EPS};

    let mut node_ids: Vec<&String> = result.nodes.keys().collect();
    node_ids.sort();
    let mut group_ids: Vec<&String> = result.groups.keys().collect();
    group_ids.sort();
    let entity_group = entity_to_group_map(diagram);
    let ancestor_sets = build_group_ancestor_sets(diagram);

    let mut node_sev = 0.0;
    let mut group_sev = 0.0;

    for (index, edge) in result.edges.iter().enumerate() {
        if edge.path_len() < 2 {
            continue;
        }
        let Some(rel) = diagram.relations.get(index) else {
            continue;
        };
        let from_id = rel.from.as_str();
        let to_id = rel.to.as_str();
        let path = edge.path_points();
        let segment_count = path.len().saturating_sub(1);
        let skip_endpoints = segment_count > 2;

        // 节点穿障：跳过 from/to 节点，长路径跳首末 stub 段。
        for (seg_i, window) in path.windows(2).enumerate() {
            if skip_endpoints && (seg_i == 0 || seg_i == segment_count - 1) {
                continue;
            }
            let a = window[0];
            let b = window[1];
            for nid in &node_ids {
                let nid = nid.as_str();
                if nid == from_id || nid == to_id {
                    continue;
                }
                let nl = &result.nodes[nid];
                node_sev += Rect::from(nl).segment_interior_overlap_length(a, b, EPS);
            }
        }

        // 分组穿障：跳过端点相关分组（含祖先链）。
        let from_related = endpoint_related_groups(from_id, &entity_group, &ancestor_sets);
        let to_related = endpoint_related_groups(to_id, &entity_group, &ancestor_sets);
        for gid in &group_ids {
            if from_related.contains(gid.as_str()) || to_related.contains(gid.as_str()) {
                continue;
            }
            let gl = &result.groups[*gid];
            if gl.width <= 0.0 || gl.height <= 0.0 {
                continue;
            }
            for window in path.windows(2) {
                group_sev += Rect::from(gl).segment_interior_overlap_length(
                    window[0],
                    window[1],
                    crate::layout::routing::common::geom_obstacle::GROUP_INTERIOR_EPS,
                );
            }
        }
    }

    (node_sev, group_sev)
}

fn check_label_node_overlaps(
    diagram: &Diagram,
    result: &LayoutResult,
    out: &mut Vec<LayoutViolation>,
) {
    let mut node_ids: Vec<&String> = result.nodes.keys().collect();
    node_ids.sort();

    for (edge_idx, edge) in result.edges.iter().enumerate() {
        if edge.labels.is_empty() {
            continue;
        }
        let rel = diagram.relations.get(edge_idx);
        for (label_idx, label) in edge.labels.iter().enumerate() {
            let bbox = label.bbox();
            for node_id in &node_ids {
                let nl = &result.nodes[*node_id];
                let node_bbox = (nl.x, nl.y, nl.x + nl.width, nl.y + nl.height);
                if aabb_overlap(&bbox, &node_bbox).is_some() {
                    let text_preview = if label.text.chars().count() > 12 {
                        format!("{}…", label.text.chars().take(12).collect::<String>())
                    } else {
                        label.text.clone()
                    };
                    let mut violation = LayoutViolation::new(
                        LintRuleId::LabelNodeOverlap,
                        format!("标签 '{text_preview}' 与节点 '{node_id}' 重叠"),
                    )
                    .with_edge_index(edge_idx)
                    .with_entities([node_id.as_str()]);
                    if let Some(rel) = rel {
                        violation = violation.with_entities([
                            rel.from.as_str(),
                            rel.to.as_str(),
                            node_id.as_str(),
                        ]);
                    }
                    let _ = label_idx;
                    out.push(violation);
                }
            }
        }
    }
}

fn check_label_label_overlaps(result: &LayoutResult, out: &mut Vec<LayoutViolation>) {
    let mut entries: Vec<(usize, usize, (f64, f64, f64, f64))> = Vec::new();
    for (edge_idx, edge) in result.edges.iter().enumerate() {
        for (label_idx, label) in edge.labels.iter().enumerate() {
            entries.push((edge_idx, label_idx, label.bbox()));
        }
    }

    for i in 0..entries.len() {
        for j in (i + 1)..entries.len() {
            let (ei, li, bi) = entries[i];
            let (ej, lj, bj) = entries[j];
            if let Some((ox, oy)) = aabb_overlap(&bi, &bj) {
                out.push(
                    LayoutViolation::new(
                        LintRuleId::LabelLabelOverlap,
                        format!("边 {ei} 标签 {li} 与边 {ej} 标签 {lj} 重叠"),
                    )
                    .with_metric(ox * oy)
                    .with_edge_index(ei),
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
            constraints: vec![],
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
            constraints: vec![],
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
            constraints: vec![],
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
            constraints: vec![],
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
            constraints: vec![],
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
            constraints: vec![],
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
