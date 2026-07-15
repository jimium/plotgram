//! 布局观测指标（不改变算法行为）。

pub mod collinear;
pub mod congestion;

pub use collinear::{
    compute_collinear_sample_metrics, node_fingerprint, CollinearBaselineSnapshot,
    CollinearOrthoStats, CollinearSampleMetrics,
};
pub use congestion::{
    compute_congestion_sample_metrics, CongestionBaselineSnapshot, CongestionSampleMetrics,
};
