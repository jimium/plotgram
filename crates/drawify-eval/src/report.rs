//! 评估报告输出模块
//!
//! 支持将评估结果输出为 JSON 或 Markdown 格式。
//! 增强功能：
//! - 差异报告（基线对比）
//! - 排名表
//! - 推荐信息
//! - 共性问题分析
//! - 按规模分桶的分组统计

use crate::engine::{CombinationReport, ComparisonReport, DiffReport, EvalResult};
use crate::metrics::QualityGrade;
use crate::profile::SizeBucket;
use drawify_core::types::DiagramType;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// 批量评估报告
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EvalReport {
    /// 报告标题
    pub title: String,
    /// 各图的对比报告
    pub comparisons: Vec<ComparisonReport>,
    /// 差异报告
    pub diffs: Vec<DiffReport>,
    /// 组合评估报告
    pub combinations: Vec<CombinationReport>,
}

impl EvalReport {
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_string(),
            comparisons: Vec::new(),
            diffs: Vec::new(),
            combinations: Vec::new(),
        }
    }

    pub fn add_comparison(&mut self, report: ComparisonReport) {
        self.comparisons.push(report);
    }

    pub fn add_diff(&mut self, diff: DiffReport) {
        self.diffs.push(diff);
    }

    pub fn add_combination(&mut self, report: CombinationReport) {
        self.combinations.push(report);
    }

    /// 输出为 JSON 字符串
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
    }

    /// 输出为 Markdown 字符串
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        md.push_str(&format!("# {}\n\n", self.title));

        // 对比报告
        for comp in &self.comparisons {
            md.push_str(&comp.to_markdown());
            md.push('\n');
        }

        // 组合评估
        for combo in &self.combinations {
            md.push_str(&combo.to_markdown());
            md.push('\n');
        }

        // 差异报告
        for diff in &self.diffs {
            md.push_str(&diff.to_markdown());
            md.push('\n');
        }

        // 按图表类型分组排名
        if !self.comparisons.is_empty() {
            self.append_type_grouped_rankings(&mut md);
        }

        // 共性问题分析
        if !self.comparisons.is_empty() {
            self.append_common_issues(&mut md);
        }

        // 按规模分桶统计
        if !self.comparisons.is_empty() {
            self.append_size_bucket_analysis(&mut md);
        }

        md
    }

    /// 写入文件
    pub fn write_to_file(&self, path: &Path) -> std::io::Result<()> {
        let content = match path.extension().and_then(|e| e.to_str()) {
            Some("json") => self.to_json(),
            _ => self.to_markdown(),
        };
        fs::write(path, content)
    }

    // ──────────────────────────────────────────────────────────
    //  按图表类型分组排名
    // ──────────────────────────────────────────────────────────

    fn append_type_grouped_rankings(&self, md: &mut String) {
        let type_labels = [
            (DiagramType::Flowchart, "流程图"),
            (DiagramType::Architecture, "架构图"),
            (DiagramType::State, "状态图"),
            (DiagramType::Er, "ER图"),
            (DiagramType::Sequence, "时序图"),
            (DiagramType::Mindmap, "思维导图"),
        ];

        for (dtype, label) in &type_labels {
            let comps: Vec<&ComparisonReport> = self
                .comparisons
                .iter()
                .filter(|c| c.diagram_type == *dtype)
                .collect();

            if comps.is_empty() {
                continue;
            }

            // 收集所有算法的平均评分
            let mut algo_scores: HashMap<String, Vec<f64>> = HashMap::new();
            for comp in &comps {
                for r in &comp.results {
                    algo_scores
                        .entry(r.algorithm.clone())
                        .or_default()
                        .push(r.score);
                }
            }

            let mut avg_scores: Vec<(String, f64)> = algo_scores
                .iter()
                .map(|(name, scores)| {
                    let avg = scores.iter().sum::<f64>() / scores.len() as f64;
                    (name.clone(), avg)
                })
                .collect();
            avg_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            if avg_scores.is_empty() {
                continue;
            }

            md.push_str(&format!("## {} — 算法排名\n\n", label));

            md.push_str("| 排名 | 算法 | 平均评分 | 质量等级 | 样本数 |\n");
            md.push_str("|------|------|----------|----------|--------|\n");

            for (i, (name, avg)) in avg_scores.iter().enumerate() {
                let count = algo_scores.get(name).map(|v| v.len()).unwrap_or(0);
                let grade = QualityGrade::from_score(*avg);
                md.push_str(&format!(
                    "| {} | {} | {:.1} | {} | {} |\n",
                    i + 1,
                    name,
                    avg,
                    grade,
                    count
                ));
            }
            md.push('\n');
        }
    }

    // ──────────────────────────────────────────────────────────
    //  共性问题分析
    // ──────────────────────────────────────────────────────────

    fn append_common_issues(&self, md: &mut String) {
        // 找出所有算法评分都低的图
        let mut poor_diagrams: Vec<(String, DiagramType, f64)> = Vec::new();

        for comp in &self.comparisons {
            if comp.results.is_empty() {
                continue;
            }
            let best_score = comp.results.iter().map(|r| r.score).fold(f64::NEG_INFINITY, f64::max);
            if best_score < 50.0 {
                poor_diagrams.push((
                    comp.diagram_name.clone(),
                    comp.diagram_type.clone(),
                    best_score,
                ));
            }
        }

        if poor_diagrams.is_empty() {
            return;
        }

        md.push_str("## 共性问题分析\n\n");
        md.push_str("以下图表所有算法评分均低于 50 分，可能存在结构特征导致布局困难：\n\n");
        md.push_str("| 图名称 | 类型 | 最高评分 |\n");
        md.push_str("|--------|------|----------|\n");

        for (name, dtype, score) in &poor_diagrams {
            md.push_str(&format!(
                "| {} | {} | {:.1} |\n",
                name,
                dtype.display_name(),
                score
            ));
        }
        md.push('\n');
    }

    // ──────────────────────────────────────────────────────────
    //  按规模分桶统计
    // ──────────────────────────────────────────────────────────

    fn append_size_bucket_analysis(&self, md: &mut String) {
        let mut bucket_data: HashMap<SizeBucket, Vec<&EvalResult>> = HashMap::new();

        for comp in &self.comparisons {
            for r in &comp.results {
                bucket_data
                    .entry(r.graph_profile.size_bucket)
                    .or_default()
                    .push(r);
            }
        }

        if bucket_data.is_empty() {
            return;
        }

        md.push_str("## 按规模分桶统计\n\n");
        md.push_str("| 规模 | 样本数 | 平均评分 | 最高评分 | 最低评分 |\n");
        md.push_str("|------|--------|----------|----------|----------|\n");

        let bucket_order = [
            SizeBucket::Tiny,
            SizeBucket::Small,
            SizeBucket::Medium,
            SizeBucket::Large,
            SizeBucket::Huge,
        ];

        for bucket in &bucket_order {
            if let Some(results) = bucket_data.get(bucket) {
                let avg = results.iter().map(|r| r.score).sum::<f64>() / results.len() as f64;
                let max = results.iter().map(|r| r.score).fold(f64::NEG_INFINITY, f64::max);
                let min = results.iter().map(|r| r.score).fold(f64::INFINITY, f64::min);
                md.push_str(&format!(
                    "| {} | {} | {:.1} | {:.1} | {:.1} |\n",
                    bucket,
                    results.len(),
                    avg,
                    max,
                    min
                ));
            }
        }
        md.push('\n');
    }
}

// ═══════════════════════════════════════════════════════════
//  ComparisonReport 的 Markdown 输出
// ═══════════════════════════════════════════════════════════

impl ComparisonReport {
    /// 生成 Markdown 格式的对比表格
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        md.push_str(&format!("## {}\n\n", self.diagram_name));

        if self.results.is_empty() {
            md.push_str("*No results*\n");
            return md;
        }

        // 图结构特征摘要
        if let Some(first) = self.results.first() {
            let p = &first.graph_profile;
            md.push_str(&format!(
                "**图特征**: {} 节点 / {} 边 / 密度 {:.2} / 深度 {} / {} / {}\n\n",
                p.node_count,
                p.edge_count,
                p.density,
                p.max_depth,
                p.size_bucket,
                p.topology_summary(),
            ));
        }

        // 排名表
        if !self.ranking.is_empty() {
            md.push_str("| 排名 | 算法 | 评分 | 等级 |\n");
            md.push_str("|------|------|------|------|\n");
            for entry in &self.ranking {
                md.push_str(&format!(
                    "| {} | {} | {:.1} | {} |\n",
                    entry.rank, entry.algorithm, entry.score, entry.grade
                ));
            }
            md.push('\n');
        }

        // 推荐信息
        if let Some(ref rec) = self.recommendation {
            md.push_str(&format!("> **推荐**: {} — {}\n\n", rec.algorithm, rec.reason));
        }

        // 详细指标表
        md.push_str("| 指标 |");
        for r in &self.results {
            md.push_str(&format!(" {} |", r.algorithm));
        }
        md.push('\n');

        md.push_str("|------|");
        for _ in &self.results {
            md.push_str("------|");
        }
        md.push('\n');

        let rows: Vec<(&str, Box<dyn Fn(&EvalResult) -> String>)> = vec![
            ("节点数", Box::new(|r| r.metrics.node_count.to_string())),
            ("边数", Box::new(|r| r.metrics.edge_count.to_string())),
            ("节点重叠对", Box::new(|r| r.metrics.node_overlap_pairs.to_string())),
            ("边穿节点", Box::new(|r| r.metrics.edge_node_crossings.to_string())),
            ("边交叉", Box::new(|r| r.metrics.edge_crossings.to_string())),
            ("总面积", Box::new(|r| format!("{:.0}", r.metrics.total_area))),
            ("边总长", Box::new(|r| format!("{:.1}", r.metrics.total_edge_length))),
            ("边长CV", Box::new(|r| format!("{:.2}", r.metrics.edge_length_cv))),
            ("宽高比", Box::new(|r| format!("{:.2}", r.metrics.aspect_ratio))),
            ("面积利用率", Box::new(|r| format!("{:.1}%", r.metrics.area_utilization * 100.0))),
            ("综合评分", Box::new(|r| format!("{:.1}", r.score))),
            ("耗时(μs)", Box::new(|r| r.elapsed_us.to_string())),
        ];

        for (label, extractor) in &rows {
            md.push_str(&format!("| {} |", label));
            for r in &self.results {
                md.push_str(&format!(" {} |", extractor(r)));
            }
            md.push('\n');
        }

        md
    }
}

// ═══════════════════════════════════════════════════════════
//  DiffReport 的 Markdown 输出
// ═══════════════════════════════════════════════════════════

impl DiffReport {
    /// 生成 Markdown 格式的差异报告
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();

        let title = if self.diagram_name.is_empty() {
            format!("差异报告: {} vs {}", self.baseline_algo, self.current_algo)
        } else {
            format!(
                "差异报告 [{}]: {} vs {}",
                self.diagram_name, self.baseline_algo, self.current_algo
            )
        };
        md.push_str(&format!("## {}\n\n", title));

        // 评分变化
        let score_icon = if self.score_diff > 0.0 { "↑" } else if self.score_diff < 0.0 { "↓" } else { "→" };
        md.push_str(&format!(
            "**评分变化**: {} {:.1} 分\n\n",
            score_icon, self.score_diff
        ));

        // 回归
        if !self.regressions.is_empty() {
            md.push_str("### 回归\n\n");
            md.push_str("| 指标 | 基线 | 当前 | 严重程度 |\n");
            md.push_str("|------|------|------|----------|\n");
            for reg in &self.regressions {
                let severity = match reg.severity {
                    crate::engine::RegressionSeverity::Minor => "轻微",
                    crate::engine::RegressionSeverity::Moderate => "中等",
                    crate::engine::RegressionSeverity::Major => "严重",
                };
                md.push_str(&format!(
                    "| {} | {:.2} | {:.2} | {} |\n",
                    reg.metric, reg.baseline, reg.current, severity
                ));
            }
            md.push('\n');
        }

        // 改善
        if !self.improvements.is_empty() {
            md.push_str("### 改善\n\n");
            md.push_str("| 指标 | 基线 | 当前 |\n");
            md.push_str("|------|------|------|\n");
            for imp in &self.improvements {
                md.push_str(&format!(
                    "| {} | {:.2} | {:.2} |\n",
                    imp.metric, imp.baseline, imp.current
                ));
            }
            md.push('\n');
        }

        // 完整指标对比
        md.push_str("### 指标明细\n\n");
        md.push_str("| 指标 | 基线 | 当前 | 差值 | 变化% | 方向 |\n");
        md.push_str("|------|------|------|------|-------|------|\n");
        for diff in &self.metric_diffs {
            let direction = match diff.direction {
                crate::engine::ChangeDirection::Improved => "✓ 改善",
                crate::engine::ChangeDirection::Regressed => "✗ 退步",
                crate::engine::ChangeDirection::Unchanged => "— 不变",
            };
            let pct = diff
                .percent_change
                .map(|p| format!("{:.1}%", p))
                .unwrap_or_else(|| "—".to_string());
            md.push_str(&format!(
                "| {} | {:.2} | {:.2} | {:+.2} | {} | {} |\n",
                diff.name, diff.baseline, diff.current, diff.diff, pct, direction
            ));
        }
        md.push('\n');

        md
    }
}

// ═══════════════════════════════════════════════════════════
//  CombinationReport 的 Markdown 输出
// ═══════════════════════════════════════════════════════════

impl CombinationReport {
    /// 生成 Markdown 格式的组合评估报告
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        md.push_str(&format!("## {} — 布局+路由组合评估\n\n", self.diagram_name));

        if let Some(ref best) = self.best_combination {
            md.push_str(&format!("> **最佳组合**: {} — {}\n\n", best.algorithm, best.reason));
        }

        // 组合矩阵表
        md.push_str("| 排名 | 组合 | 评分 | 等级 | 重叠 | 边交叉 | 耗时(μs) |\n");
        md.push_str("|------|------|------|------|------|--------|----------|\n");

        for (i, r) in self.results.iter().enumerate() {
            md.push_str(&format!(
                "| {} | {} | {:.1} | {} | {} | {} | {} |\n",
                i + 1,
                r.algorithm,
                r.score,
                r.quality_grade,
                r.metrics.node_overlap_pairs,
                r.metrics.edge_crossings,
                r.elapsed_us,
            ));
        }
        md.push('\n');

        md
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{presets, EvalEngine};
    use drawify_core::ast::*;
    use drawify_core::types::DiagramType;

    fn sample_diagram() -> Diagram {
        Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![],
            entities: vec![
                Entity {
                    id: Identifier::new_unchecked("a"),
                    label: "A".to_string(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span: Span::dummy(),
                },
                Entity {
                    id: Identifier::new_unchecked("b"),
                    label: "B".to_string(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span: Span::dummy(),
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
                span: Span::dummy(),
            }],
            groups: vec![],
            style_decls: vec![],
            doc_comment: None,
            source_info: SourceInfo {
                file: None,
                line_count: 1,
            },
        }
    }

    #[test]
    fn test_eval_report_markdown() {
        let engine = EvalEngine::new();
        let diagram = sample_diagram();
        let configs = presets::routing_comparison();
        let comp = engine.compare("test_flow", &diagram, &configs);

        let mut report = EvalReport::new("Test Report");
        report.add_comparison(comp);

        let md = report.to_markdown();
        assert!(md.contains("# Test Report"));
        assert!(md.contains("test_flow"));
        assert!(md.contains("排名"));
    }

    #[test]
    fn test_eval_report_json() {
        let engine = EvalEngine::new();
        let diagram = sample_diagram();
        let configs = presets::routing_comparison();
        let comp = engine.compare("test_flow", &diagram, &configs);

        let mut report = EvalReport::new("Test Report");
        report.add_comparison(comp);

        let json = report.to_json();
        assert!(json.contains("\"title\""));
        assert!(json.contains("\"comparisons\""));
    }

    #[test]
    fn test_diff_report_markdown() {
        let engine = EvalEngine::new();
        let diagram = sample_diagram();

        let config_a = presets::set_layout_algo("sugiyama");
        let config_b = presets::set_layout_algo("sugiyama-v2");

        let result_a = engine.evaluate(&diagram, &config_a);
        let result_b = engine.evaluate(&diagram, &config_b);

        let diff = engine.diff(&result_a, &result_b);
        let md = diff.to_markdown();
        assert!(md.contains("差异报告"));
        assert!(md.contains("sugiyama"));
    }
}
