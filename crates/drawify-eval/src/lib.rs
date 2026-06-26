//! Drawify 布局与边路由算法评估框架
//!
//! 提供完整的算法评估能力，包括：
//! - 可量化的质量指标计算
//! - 算法 A/B 对比与排名
//! - 基线对比与回归检测
//! - 图结构特征分析
//! - 按图类型定制权重
//! - 质量等级与推荐
//! - 布局+路由组合评估
//! - 结果持久化与历史追踪
//!
//! # 快速开始
//!
//! ```ignore
//! use drawify_eval::engine::{EvalEngine, presets};
//! use drawify_eval::report::EvalReport;
//! use drawify_core::pipeline;
//!
//! // 1. 计算单个布局的质量指标
//! let diagram = pipeline::parse(source).unwrap();
//! let engine = EvalEngine::new();
//! let config = presets::set_layout_algo("sugiyama");
//! let result = engine.evaluate(&diagram, &config);
//! println!("评分: {:.1} ({})", result.score, result.quality_grade);
//!
//! // 2. 对比不同算法
//! let configs = presets::routing_comparison();
//! let report = engine.compare("my_diagram", &diagram, &configs);
//! println!("{}", report.to_markdown());
//!
//! // 3. 基线对比（改进前后）
//! let diff = engine.diff(&baseline_result, &current_result);
//! if !diff.regressions.is_empty() {
//!     println!("检测到回归！");
//! }
//! ```

pub mod engine;
pub mod history;
pub mod metrics;
pub mod profile;
pub mod report;

// 向后兼容的重导出
pub use engine::{AlgorithmConfig, AlgorithmResult, ComparisonReport, EvalEngine, EvalResult};
pub use metrics::LayoutMetrics;
pub use report::EvalReport;
