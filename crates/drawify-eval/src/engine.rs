//! 核心评估引擎
//!
//! 提供算法评估的核心能力：
//! - 单算法评估 / 多算法对比
//! - 基线对比与差异报告
//! - 回归检测
//! - 算法排名
//! - 布局+路由组合评估
//! - 最差案例发现

use crate::metrics::{LayoutMetrics, MetricWeights, QualityGrade};
use crate::profile::GraphProfile;
use drawify_core::types::DiagramType;
use drawify_core::ast::{Diagram};
use drawify_core::layout::{compute_layout, LayoutResult};
use std::sync::mpsc;
use std::time::{Duration, Instant};

// ═══════════════════════════════════════════════════════════
//  算法配置
// ═══════════════════════════════════════════════════════════

/// 算法配置项
pub struct AlgorithmConfig {
    /// 算法标识（如 "sugiyama+orthogonal"）
    pub name: &'static str,
    /// 布局算法名称
    pub layout_algo: &'static str,
    /// 边路由算法名称（None 表示使用默认）
    pub routing_algo: Option<&'static str>,
    /// 修改 diagram 属性的闭包
    pub modifier: Box<dyn Fn(&mut Diagram)>,
}

// ═══════════════════════════════════════════════════════════
//  评估结果
// ═══════════════════════════════════════════════════════════

/// 单个算法的评估结果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EvalResult {
    /// 算法标识
    pub algorithm: String,
    /// 布局算法名称
    pub layout_algo: String,
    /// 边路由算法名称
    pub routing_algo: String,
    /// 布局质量指标
    pub metrics: LayoutMetrics,
    /// 图结构特征
    pub graph_profile: GraphProfile,
    /// 布局计算耗时（微秒）
    pub elapsed_us: u64,
    /// 质量等级
    pub quality_grade: QualityGrade,
    /// 综合评分
    pub score: f64,
    /// 是否超时
    pub timed_out: bool,
    /// 超时时的样本 DSL（仅当 timed_out=true 时有值）
    pub timeout_dsl: Option<String>,
    /// 路由友好性评估分数（V1 诊断模式，来自 LayoutHints.friendliness_report）
    #[serde(default)]
    pub friendliness_score: f64,
}

/// 多算法对比报告
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ComparisonReport {
    /// 输入图名称
    pub diagram_name: String,
    /// 图表类型
    pub diagram_type: DiagramType,
    /// 各算法的评估结果
    pub results: Vec<EvalResult>,
    /// 排名
    pub ranking: Vec<RankingEntry>,
    /// 推荐算法
    pub recommendation: Option<Recommendation>,
}

/// 排名条目
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RankingEntry {
    /// 排名（从 1 开始）
    pub rank: usize,
    /// 算法标识
    pub algorithm: String,
    /// 综合评分
    pub score: f64,
    /// 质量等级
    pub grade: QualityGrade,
}

/// 推荐信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Recommendation {
    /// 推荐的算法标识
    pub algorithm: String,
    /// 推荐理由
    pub reason: String,
}

// ═══════════════════════════════════════════════════════════
//  差异报告与回归检测
// ═══════════════════════════════════════════════════════════

/// 基线对比差异报告
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiffReport {
    /// 图名称
    pub diagram_name: String,
    /// 基线算法标识
    pub baseline_algo: String,
    /// 当前算法标识
    pub current_algo: String,
    /// 评分差异（正值表示改善，负值表示退步）
    pub score_diff: f64,
    /// 各指标差异
    pub metric_diffs: Vec<MetricDiff>,
    /// 检测到的回归
    pub regressions: Vec<Regression>,
    /// 检测到的改善
    pub improvements: Vec<Improvement>,
}

/// 单个指标的差异
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MetricDiff {
    /// 指标名称
    pub name: String,
    /// 基线值
    pub baseline: f64,
    /// 当前值
    pub current: f64,
    /// 差值（current - baseline）
    pub diff: f64,
    /// 百分比变化
    pub percent_change: Option<f64>,
    /// 变化方向
    pub direction: ChangeDirection,
}

/// 变化方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ChangeDirection {
    /// 改善
    Improved,
    /// 退步
    Regressed,
    /// 无变化
    Unchanged,
}

/// 回归
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Regression {
    /// 指标名称
    pub metric: String,
    /// 基线值
    pub baseline: f64,
    /// 当前值
    pub current: f64,
    /// 严重程度
    pub severity: RegressionSeverity,
}

/// 回归严重程度
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RegressionSeverity {
    /// 轻微（< 5% 退步）
    Minor,
    /// 中等（5-15% 退步）
    Moderate,
    /// 严重（> 15% 退步）
    Major,
}

/// 改善
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Improvement {
    /// 指标名称
    pub metric: String,
    /// 基线值
    pub baseline: f64,
    /// 当前值
    pub current: f64,
}

// ═══════════════════════════════════════════════════════════
//  组合评估
// ═══════════════════════════════════════════════════════════

/// 布局+路由组合评估报告
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CombinationReport {
    /// 图名称
    pub diagram_name: String,
    /// 图表类型
    pub diagram_type: DiagramType,
    /// 所有组合的评估结果
    pub results: Vec<EvalResult>,
    /// 最佳组合
    pub best_combination: Option<Recommendation>,
}

// ═══════════════════════════════════════════════════════════
//  评估引擎
// ═══════════════════════════════════════════════════════════

/// 默认超时时间（秒）
const DEFAULT_TIMEOUT_SECS: u64 = 3;

/// 评估引擎
pub struct EvalEngine {
    /// 指标权重（None 表示使用默认权重）
    weights: Option<MetricWeights>,
    /// 单个样本的超时时间
    timeout: Duration,
}

impl EvalEngine {
    /// 创建默认引擎
    pub fn new() -> Self {
        Self {
            weights: None,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
        }
    }

    /// 创建使用指定图类型权重的引擎
    pub fn with_weights_for_type(diagram_type: &DiagramType) -> Self {
        Self {
            weights: Some(MetricWeights::for_diagram_type(diagram_type)),
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
        }
    }

    /// 创建使用自定义权重的引擎
    pub fn with_weights(weights: MetricWeights) -> Self {
        Self {
            weights: Some(weights),
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
        }
    }

    /// 设置超时时间
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// 获取当前超时时间
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// 获取当前权重
    pub fn weights(&self) -> &MetricWeights {
        self.weights.as_ref().unwrap_or_else(|| {
            // 返回静态默认值的引用
            static DEFAULT: MetricWeights = MetricWeights {
                correctness: 0.40,
                compactness: 0.20,
                uniformity: 0.20,
                aesthetics: 0.20,
            };
            &DEFAULT
        })
    }

    /// 评估单个算法（带超时保护）
    pub fn evaluate(&self, diagram: &Diagram, config: &AlgorithmConfig) -> EvalResult {
        self.evaluate_with_dsl(diagram, config, None)
    }

    /// 评估单个算法（带超时保护，可传入 DSL 源码用于超时记录）
    ///
    /// 使用独立线程 + channel 实现真正的超时：
    /// 布局计算在独立线程中运行，主线程通过 channel 等待结果。
    /// 如果超时，主线程立即返回超时结果，不再等待计算线程。
    /// 注意：计算线程不会被强制终止，但不再阻塞评估流程。
    pub fn evaluate_with_dsl(
        &self,
        diagram: &Diagram,
        config: &AlgorithmConfig,
        dsl_source: Option<&str>,
    ) -> EvalResult {
        let mut diag = diagram.clone();
        (config.modifier)(&mut diag);

        let profile = GraphProfile::analyze(&diag);
        let timeout = self.timeout;

        // 在独立线程中执行布局计算，通过 channel 传递结果
        let (tx, rx) = mpsc::channel();
        let diag_for_layout = diag.clone();

        std::thread::spawn(move || {
            let start = Instant::now();
            let layout = match compute_layout(&diag_for_layout) {
                Ok(layout) => layout,
                Err(err) => {
                    let _ = tx.send(Err(err.to_string()));
                    return;
                }
            };
            let elapsed = start.elapsed();
            let _ = tx.send(Ok((layout, elapsed)));
        });

        // 等待结果或超时
        match rx.recv_timeout(timeout) {
            Ok(Ok((layout, elapsed))) => {
                let metrics = LayoutMetrics::compute(&diag, &layout);
                let score = self.compute_score(&metrics);
                let quality_grade = QualityGrade::from_score(score);
                let friendliness_score = layout
                    .hints
                    .friendliness_report
                    .as_ref()
                    .map(|r| r.score)
                    .unwrap_or(0.0);

                EvalResult {
                    algorithm: config.name.to_string(),
                    layout_algo: config.layout_algo.to_string(),
                    routing_algo: config
                        .routing_algo
                        .map(|s| s.to_string())
                        .unwrap_or_default(),
                    metrics,
                    graph_profile: profile,
                    elapsed_us: elapsed.as_micros() as u64,
                    quality_grade,
                    score,
                    timed_out: false,
                    timeout_dsl: None,
                    friendliness_score,
                }
            }
            Ok(Err(layout_error)) => {
                eprintln!(
                    "  ⚠ 布局配置错误！算法 '{}': {}",
                    config.name, layout_error
                );

                let zero_metrics = LayoutMetrics::zero_for(&diag);
                EvalResult {
                    algorithm: config.name.to_string(),
                    layout_algo: config.layout_algo.to_string(),
                    routing_algo: config
                        .routing_algo
                        .map(|s| s.to_string())
                        .unwrap_or_default(),
                    metrics: zero_metrics,
                    graph_profile: profile,
                    elapsed_us: 0,
                    quality_grade: QualityGrade::Poor,
                    score: 0.0,
                    timed_out: true,
                    timeout_dsl: dsl_source.map(|s| s.to_string()),
                    friendliness_score: 0.0,
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                eprintln!(
                    "  ⚠ 超时！算法 '{}' 超过 {}s 未完成",
                    config.name,
                    timeout.as_secs()
                );

                // 构造超时结果
                let zero_metrics = LayoutMetrics::zero_for(&diag);
                EvalResult {
                    algorithm: config.name.to_string(),
                    layout_algo: config.layout_algo.to_string(),
                    routing_algo: config
                        .routing_algo
                        .map(|s| s.to_string())
                        .unwrap_or_default(),
                    metrics: zero_metrics,
                    graph_profile: profile,
                    elapsed_us: timeout.as_micros() as u64,
                    quality_grade: QualityGrade::Poor,
                    score: 0.0,
                    timed_out: true,
                    timeout_dsl: dsl_source.map(|s| s.to_string()),
                    friendliness_score: 0.0,
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                // 计算线程 panic，视为超时
                eprintln!(
                    "  ⚠ 计算线程异常退出！算法 '{}'",
                    config.name
                );

                let zero_metrics = LayoutMetrics::zero_for(&diag);
                EvalResult {
                    algorithm: config.name.to_string(),
                    layout_algo: config.layout_algo.to_string(),
                    routing_algo: config
                        .routing_algo
                        .map(|s| s.to_string())
                        .unwrap_or_default(),
                    metrics: zero_metrics,
                    graph_profile: profile,
                    elapsed_us: timeout.as_micros() as u64,
                    quality_grade: QualityGrade::Poor,
                    score: 0.0,
                    timed_out: true,
                    timeout_dsl: dsl_source.map(|s| s.to_string()),
                    friendliness_score: 0.0,
                }
            }
        }
    }

    /// 对比多个算法
    pub fn compare(
        &self,
        diagram_name: &str,
        diagram: &Diagram,
        configs: &[AlgorithmConfig],
    ) -> ComparisonReport {
        self.compare_with_dsl(diagram_name, diagram, configs, None)
    }

    /// 对比多个算法（带 DSL 源码，用于超时记录）
    pub fn compare_with_dsl(
        &self,
        diagram_name: &str,
        diagram: &Diagram,
        configs: &[AlgorithmConfig],
        dsl_source: Option<&str>,
    ) -> ComparisonReport {
        let diagram_type = diagram.diagram_type.clone();
        let mut results: Vec<EvalResult> = configs
            .iter()
            .map(|config| self.evaluate_with_dsl(diagram, config, dsl_source))
            .collect();

        // 按评分降序排列
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        let ranking = Self::build_ranking(&results);
        let recommendation = Self::build_recommendation(&results, &diagram_type);

        ComparisonReport {
            diagram_name: diagram_name.to_string(),
            diagram_type,
            results,
            ranking,
            recommendation,
        }
    }

    /// 从已有 LayoutResult 创建评估结果（不重新计算布局）
    pub fn evaluate_layout(
        &self,
        diagram: &Diagram,
        algorithm: &str,
        layout_algo: &str,
        routing_algo: &str,
        result: &LayoutResult,
    ) -> EvalResult {
        let profile = GraphProfile::analyze(diagram);
        let metrics = LayoutMetrics::compute(diagram, result);
        let score = self.compute_score(&metrics);
        let quality_grade = QualityGrade::from_score(score);
        let friendliness_score = result
            .hints
            .friendliness_report
            .as_ref()
            .map(|r| r.score)
            .unwrap_or(0.0);

        EvalResult {
            algorithm: algorithm.to_string(),
            layout_algo: layout_algo.to_string(),
            routing_algo: routing_algo.to_string(),
            metrics,
            graph_profile: profile,
            elapsed_us: 0,
            quality_grade,
            score,
            timed_out: false,
            timeout_dsl: None,
            friendliness_score,
        }
    }

    /// 生成差异报告
    pub fn diff(&self, baseline: &EvalResult, current: &EvalResult) -> DiffReport {
        let score_diff = current.score - baseline.score;

        let metric_diffs = vec![
            self.metric_diff("节点重叠对", baseline.metrics.node_overlap_pairs as f64, current.metrics.node_overlap_pairs as f64, true),
            self.metric_diff("边穿节点", baseline.metrics.edge_node_crossings as f64, current.metrics.edge_node_crossings as f64, true),
            self.metric_diff("边交叉", baseline.metrics.edge_crossings as f64, current.metrics.edge_crossings as f64, true),
            self.metric_diff("总面积", baseline.metrics.total_area, current.metrics.total_area, true),
            self.metric_diff("边总长", baseline.metrics.total_edge_length, current.metrics.total_edge_length, true),
            self.metric_diff("边长CV", baseline.metrics.edge_length_cv, current.metrics.edge_length_cv, true),
            self.metric_diff("宽高比", baseline.metrics.aspect_ratio, current.metrics.aspect_ratio, true),
            self.metric_diff("面积利用率", baseline.metrics.area_utilization, current.metrics.area_utilization, false),
            self.metric_diff("综合评分", baseline.score, current.score, false),
        ];

        let mut regressions = Vec::new();
        let mut improvements = Vec::new();

        for diff in &metric_diffs {
            if diff.direction == ChangeDirection::Regressed {
                let severity = self.regression_severity(diff);
                regressions.push(Regression {
                    metric: diff.name.clone(),
                    baseline: diff.baseline,
                    current: diff.current,
                    severity,
                });
            } else if diff.direction == ChangeDirection::Improved {
                improvements.push(Improvement {
                    metric: diff.name.clone(),
                    baseline: diff.baseline,
                    current: diff.current,
                });
            }
        }

        DiffReport {
            diagram_name: String::new(),
            baseline_algo: baseline.algorithm.clone(),
            current_algo: current.algorithm.clone(),
            score_diff,
            metric_diffs,
            regressions,
            improvements,
        }
    }

    /// 评估所有布局+路由组合
    pub fn evaluate_combinations(
        &self,
        diagram_name: &str,
        diagram: &Diagram,
    ) -> CombinationReport {
        let diagram_type = &diagram.diagram_type;
        let layout_names = drawify_core::layout::applicable_layouts_for_type(diagram_type);
        let routing_names = drawify_core::layout::applicable_routings_for_type(diagram_type);

        let mut results = Vec::new();

        for &layout in &layout_names {
            for &routing in &routing_names {
                let config = presets::set_layout_and_routing(layout, routing);
                let result = self.evaluate(diagram, &config);
                results.push(result);
            }
        }

        // 按评分排序
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        let best = results.first().map(|r| Recommendation {
            algorithm: r.algorithm.clone(),
            reason: format!(
                "综合评分最高（{:.1}），质量等级：{}",
                r.score, r.quality_grade
            ),
        });

        CombinationReport {
            diagram_name: diagram_name.to_string(),
            diagram_type: diagram_type.clone(),
            results,
            best_combination: best,
        }
    }

    /// 找到算法的最差案例
    pub fn find_worst_cases<'a>(
        &self,
        results: &'a [EvalResult],
        algo_name: &str,
        top_n: usize,
    ) -> Vec<&'a EvalResult> {
        let mut filtered: Vec<&EvalResult> = results
            .iter()
            .filter(|r| r.algorithm == algo_name || r.layout_algo == algo_name)
            .collect();
        filtered.sort_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal));
        filtered.truncate(top_n);
        filtered
    }

    // ── 内部方法 ──

    fn compute_score(&self, metrics: &LayoutMetrics) -> f64 {
        if self.weights.is_some() {
            metrics.quality_score_with_weights(self.weights())
        } else {
            metrics.quality_score()
        }
    }

    fn build_ranking(results: &[EvalResult]) -> Vec<RankingEntry> {
        results
            .iter()
            .enumerate()
            .map(|(i, r)| RankingEntry {
                rank: i + 1,
                algorithm: r.algorithm.clone(),
                score: r.score,
                grade: r.quality_grade,
            })
            .collect()
    }

    fn build_recommendation(
        results: &[EvalResult],
        diagram_type: &DiagramType,
    ) -> Option<Recommendation> {
        let best = results.first()?;
        let type_name = diagram_type.display_name();

        let reason = if best.quality_grade == QualityGrade::Excellent {
            format!(
                "在{}中表现优秀（{:.1}分），推荐使用",
                type_name, best.score
            )
        } else if best.quality_grade == QualityGrade::Good {
            format!(
                "在{}中表现良好（{:.1}分），可作为首选",
                type_name, best.score
            )
        } else {
            format!(
                "在{}中表现一般（{:.1}分），仍有改进空间",
                type_name, best.score
            )
        };

        Some(Recommendation {
            algorithm: best.algorithm.clone(),
            reason,
        })
    }

    fn metric_diff(
        &self,
        name: &str,
        baseline: f64,
        current: f64,
        lower_is_better: bool,
    ) -> MetricDiff {
        let diff = current - baseline;
        let percent_change = if baseline.abs() > 0.001 {
            Some(diff / baseline.abs() * 100.0)
        } else if current.abs() > 0.001 {
            Some(f64::INFINITY)
        } else {
            None
        };

        let direction = if diff.abs() < 0.001 {
            ChangeDirection::Unchanged
        } else if lower_is_better {
            if diff < 0.0 {
                ChangeDirection::Improved
            } else {
                ChangeDirection::Regressed
            }
        } else {
            if diff > 0.0 {
                ChangeDirection::Improved
            } else {
                ChangeDirection::Regressed
            }
        };

        MetricDiff {
            name: name.to_string(),
            baseline,
            current,
            diff,
            percent_change,
            direction,
        }
    }

    fn regression_severity(&self, diff: &MetricDiff) -> RegressionSeverity {
        match diff.percent_change {
            Some(pct) if pct > 15.0 => RegressionSeverity::Major,
            Some(pct) if pct > 5.0 => RegressionSeverity::Moderate,
            _ => RegressionSeverity::Minor,
        }
    }
}

impl Default for EvalEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════
//  预定义算法配置
// ═══════════════════════════════════════════════════════════

/// 预定义的算法配置组合
pub mod presets {
    use super::AlgorithmConfig;
    use drawify_core::ast::{AttributeValue, Diagram, DiagramAttribute, TextValue};
    use drawify_core::types::DiagramType;

    /// 修改 diagram 的 layout_algo 属性
    pub fn set_layout_algo(name: &str) -> AlgorithmConfig {
        let name_for_label = name.to_string();
        let name_for_closure = name.to_string();
        AlgorithmConfig {
            name: Box::leak(name_for_label.into_boxed_str()),
            layout_algo: Box::leak(name_for_closure.clone().into_boxed_str()),
            routing_algo: None,
            modifier: Box::new(move |diag: &mut Diagram| {
                diag.attributes
                    .retain(|a| a.key != "layout");
                diag.attributes.push(DiagramAttribute {
                    key: "layout".to_string(),
                    value: AttributeValue::String(TextValue::unquoted(name_for_closure.clone())),
                    span: drawify_core::ast::Span::dummy(),
                });
            }),
        }
    }

    /// 修改 diagram 的 edge_routing 属性
    pub fn set_edge_routing(name: &str) -> AlgorithmConfig {
        let name_for_label = name.to_string();
        let name_for_closure = name.to_string();
        AlgorithmConfig {
            name: Box::leak(name_for_label.into_boxed_str()),
            layout_algo: "",
            routing_algo: Some(Box::leak(name_for_closure.clone().into_boxed_str())),
            modifier: Box::new(move |diag: &mut Diagram| {
                diag.attributes
                    .retain(|a| a.key != "edge_routing");
                diag.attributes.push(DiagramAttribute {
                    key: "edge_routing".to_string(),
                    value: AttributeValue::String(TextValue::unquoted(name_for_closure.clone())),
                    span: drawify_core::ast::Span::dummy(),
                });
            }),
        }
    }

    /// 同时设置布局算法和边路由
    pub fn set_layout_and_routing(layout: &str, routing: &str) -> AlgorithmConfig {
        let layout_algo = layout.to_string();
        let routing_algo = routing.to_string();
        let label = format!("{}+{}", layout, routing);
        AlgorithmConfig {
            name: Box::leak(label.into_boxed_str()),
            layout_algo: Box::leak(layout_algo.clone().into_boxed_str()),
            routing_algo: Some(Box::leak(routing_algo.clone().into_boxed_str())),
            modifier: Box::new(move |diag: &mut Diagram| {
                diag.attributes.retain(|a| {
                    a.key != "layout"
                        && a.key != "edge_routing"
                });
                diag.attributes.push(DiagramAttribute {
                    key: "layout".to_string(),
                    value: AttributeValue::String(TextValue::unquoted(layout_algo.clone())),
                    span: drawify_core::ast::Span::dummy(),
                });
                diag.attributes.push(DiagramAttribute {
                    key: "edge_routing".to_string(),
                    value: AttributeValue::String(TextValue::unquoted(routing_algo.clone())),
                    span: drawify_core::ast::Span::dummy(),
                });
            }),
        }
    }

    /// 常用的布局对比配置
    pub fn layout_comparison() -> Vec<AlgorithmConfig> {
        vec![
            set_layout_algo("sugiyama"),
            set_layout_algo("sugiyama-v2"),
        ]
    }

    /// 全部布局算法对比
    pub fn full_layout_comparison() -> Vec<AlgorithmConfig> {
        vec![
            set_layout_algo("sugiyama"),
            set_layout_algo("sugiyama-v2"),
            set_layout_algo("force-directed"),
            set_layout_algo("circular"),
        ]
    }

    /// 常用的边路由对比配置
    pub fn routing_comparison() -> Vec<AlgorithmConfig> {
        vec![
            set_edge_routing("orthogonal"),
            set_edge_routing("bezier"),
            set_edge_routing("spline"),
        ]
    }

    /// sugiyama + 不同边路由的完整对比
    pub fn sugiyama_routing_comparison() -> Vec<AlgorithmConfig> {
        vec![
            set_layout_and_routing("sugiyama", "orthogonal"),
            set_layout_and_routing("sugiyama", "bezier"),
            set_layout_and_routing("sugiyama", "spline"),
        ]
    }

    /// 图表类型适用的布局算法列表
    pub fn layout_algos_for_type(diagram_type: &DiagramType) -> Vec<AlgorithmConfig> {
        drawify_core::layout::applicable_layouts_for_type(diagram_type)
            .iter()
            .map(|name| set_layout_algo(name))
            .collect()
    }

    /// 图表类型适用的边路由列表
    pub fn routing_algos_for_type(diagram_type: &DiagramType) -> Vec<AlgorithmConfig> {
        drawify_core::layout::applicable_routings_for_type(diagram_type)
            .iter()
            .map(|name| set_edge_routing(name))
            .collect()
    }
}

// ═══════════════════════════════════════════════════════════
//  向后兼容的类型别名
// ═══════════════════════════════════════════════════════════

/// 向后兼容：旧版 AlgorithmResult 类型
pub type AlgorithmResult = EvalResult;

#[cfg(test)]
mod tests {
    use super::*;
    use drawify_core::ast::*;
    use drawify_core::types::DiagramType;
    use drawify_core::layout::NodeLayout;
    use std::collections::HashMap;

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
    fn test_engine_evaluate() {
        let engine = EvalEngine::new();
        let diagram = sample_diagram();
        let config = presets::set_layout_algo("sugiyama");
        let result = engine.evaluate(&diagram, &config);

        assert_eq!(result.layout_algo, "sugiyama");
        assert!(result.score > 0.0);
        assert_eq!(result.graph_profile.node_count, 2);
    }

    #[test]
    fn test_engine_compare() {
        let engine = EvalEngine::new();
        let diagram = sample_diagram();
        let configs = presets::routing_comparison();
        let report = engine.compare("test", &diagram, &configs);

        assert_eq!(report.results.len(), configs.len());
        assert!(!report.ranking.is_empty());
        assert!(report.recommendation.is_some());
        // 结果应按评分降序排列
        for i in 1..report.results.len() {
            assert!(report.results[i - 1].score >= report.results[i].score);
        }
    }

    #[test]
    fn test_engine_diff() {
        let engine = EvalEngine::new();
        let diagram = sample_diagram();

        let config_a = presets::set_layout_algo("sugiyama");
        let config_b = presets::set_layout_algo("sugiyama-v2");

        let result_a = engine.evaluate(&diagram, &config_a);
        let result_b = engine.evaluate(&diagram, &config_b);

        let diff = engine.diff(&result_a, &result_b);
        assert!(!diff.metric_diffs.is_empty());
        assert_eq!(diff.baseline_algo, "sugiyama");
        assert_eq!(diff.current_algo, "sugiyama-v2");
    }

    #[test]
    fn test_engine_with_weights() {
        let engine = EvalEngine::with_weights_for_type(&DiagramType::Flowchart);
        let diagram = sample_diagram();
        let config = presets::set_layout_algo("sugiyama");
        let result = engine.evaluate(&diagram, &config);

        assert!(result.score > 0.0);
    }

    #[test]
    fn test_find_worst_cases() {
        let engine = EvalEngine::new();
        let diagram = sample_diagram();
        let configs = presets::routing_comparison();
        let report = engine.compare("test", &diagram, &configs);

        let worst = engine.find_worst_cases(&report.results, "orthogonal", 1);
        assert!(worst.len() <= 1);
    }

    #[test]
    fn test_combination_evaluation() {
        let engine = EvalEngine::new();
        let diagram = sample_diagram();
        let report = engine.evaluate_combinations("test", &diagram);

        assert!(!report.results.is_empty());
        assert!(report.best_combination.is_some());
    }

    #[test]
    fn test_quality_grade_in_result() {
        let engine = EvalEngine::new();
        let diagram = sample_diagram();
        let config = presets::set_layout_algo("sugiyama");
        let result = engine.evaluate(&diagram, &config);

        // 确保质量等级和评分一致
        assert_eq!(result.quality_grade, QualityGrade::from_score(result.score));
    }

    #[test]
    fn test_backward_compat_algorithm_result() {
        let engine = EvalEngine::new();
        let diagram = sample_diagram();
        let config = presets::set_layout_algo("sugiyama");
        let result: AlgorithmResult = engine.evaluate(&diagram, &config);

        assert_eq!(result.layout_algo, "sugiyama");
    }
}
