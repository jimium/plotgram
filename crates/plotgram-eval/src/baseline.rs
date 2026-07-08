//! Showcase 质量基线：生成、保存与回归对比。

use crate::metrics::LayoutMetrics;
use plotgram_core::ast::Diagram;
use plotgram_core::layout::{compute_layout_with_plan, LayoutResult, LintMetricsSummary};
use plotgram_core::pipeline::parse_prepare_validate;
use plotgram_core::prepare::StyleRequest;
use plotgram_core::types::DiagramType;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Showcase 单文件基线条目。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ShowcaseBaselineEntry {
    /// 相对 showcase 根目录的路径（如 flowchart/c.payment-flow.pgm）
    pub path: String,
    pub diagram_type: String,
    pub metrics: LayoutMetrics,
    pub lint: LintMetricsSummary,
    pub score: f64,
    pub elapsed_us: u64,
}

/// Showcase 质量基线文件。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ShowcaseBaseline {
    pub version: u32,
    pub generated_at: String,
    pub entries: Vec<ShowcaseBaselineEntry>,
}

impl ShowcaseBaseline {
    pub const CURRENT_VERSION: u32 = 1;

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        fs::write(path, json)
    }

    pub fn load(path: &Path) -> std::io::Result<Self> {
        let text = fs::read_to_string(path)?;
        serde_json::from_str(&text)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    pub fn entry_map(&self) -> HashMap<&str, &ShowcaseBaselineEntry> {
        self.entries.iter().map(|e| (e.path.as_str(), e)).collect()
    }
}

/// 单指标回归记录。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MetricRegression {
    pub path: String,
    pub metric: String,
    pub baseline: f64,
    pub current: f64,
    pub diff: f64,
}

/// 基线对比报告。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BaselineCompareReport {
    pub baseline_path: String,
    pub compared_files: usize,
    pub regressions: Vec<MetricRegression>,
    pub improved: Vec<MetricRegression>,
    pub missing_in_current: Vec<String>,
    pub missing_in_baseline: Vec<String>,
}

impl BaselineCompareReport {
    pub fn has_regressions(&self) -> bool {
        !self.regressions.is_empty()
    }

    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        md.push_str("# Showcase 基线对比报告\n\n");
        md.push_str(&format!("基线文件: `{}`\n", self.baseline_path));
        md.push_str(&format!("对比文件数: {}\n\n", self.compared_files));

        if self.regressions.is_empty() {
            md.push_str("✓ 未检测到指标回归\n\n");
        } else {
            md.push_str(&format!("⚠ 检测到 {} 项回归:\n\n", self.regressions.len()));
            md.push_str("| 文件 | 指标 | 基线 | 当前 | 变化 |\n");
            md.push_str("|------|------|------|------|------|\n");
            for r in &self.regressions {
                md.push_str(&format!(
                    "| {} | {} | {:.2} | {:.2} | {:+.2} |\n",
                    r.path, r.metric, r.baseline, r.current, r.diff
                ));
            }
            md.push('\n');
        }

        if !self.improved.is_empty() {
            md.push_str(&format!("改善 {} 项（仅展示前 10）:\n\n", self.improved.len()));
            for r in self.improved.iter().take(10) {
                md.push_str(&format!(
                    "- {} / {}: {:.2} → {:.2} ({:+.2})\n",
                    r.path, r.metric, r.baseline, r.current, r.diff
                ));
            }
        }

        if !self.missing_in_current.is_empty() {
            md.push_str(&format!(
                "\n当前缺失 {} 个基线条目\n",
                self.missing_in_current.len()
            ));
        }
        if !self.missing_in_baseline.is_empty() {
            md.push_str(&format!(
                "基线缺失 {} 个新文件\n",
                self.missing_in_baseline.len()
            ));
        }

        md
    }
}

/// 从 showcase 目录生成质量基线。
pub fn generate_baseline(showcase_root: &Path) -> std::io::Result<ShowcaseBaseline> {
    let files = collect_pgm_files(showcase_root)?;
    let root = showcase_root.canonicalize().unwrap_or_else(|_| showcase_root.to_path_buf());

    let mut entries = Vec::new();
    for file in files {
        let rel = file
            .strip_prefix(&root)
            .unwrap_or(&file)
            .to_string_lossy()
            .replace('\\', "/");

        match evaluate_pgm_file(&file) {
            Ok(evaluated) => {
                println!("  ✓ {rel} — score {:.1}", evaluated.score);
                entries.push(ShowcaseBaselineEntry {
                    path: rel,
                    diagram_type: evaluated.diagram_type,
                    metrics: evaluated.metrics,
                    lint: evaluated.lint,
                    score: evaluated.score,
                    elapsed_us: evaluated.elapsed_us,
                });
            }
            Err(e) => {
                eprintln!("  ✗ {rel}: {e}");
            }
        }
    }

    entries.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(ShowcaseBaseline {
        version: ShowcaseBaseline::CURRENT_VERSION,
        generated_at: unix_timestamp_string(),
        entries,
    })
}

/// 将当前 showcase 与基线对比。
pub fn compare_with_baseline(
    showcase_root: &Path,
    baseline: &ShowcaseBaseline,
) -> std::io::Result<BaselineCompareReport> {
    let current = generate_baseline(showcase_root)?;
    let base_map = baseline.entry_map();
    let cur_map = current.entry_map();

    let mut regressions = Vec::new();
    let mut improved = Vec::new();
    let mut missing_in_current = Vec::new();

    for (path, base_entry) in &base_map {
        match cur_map.get(path) {
            Some(cur_entry) => {
                compare_entry_metrics(path, base_entry, cur_entry, &mut regressions, &mut improved);
            }
            None => missing_in_current.push((*path).to_string()),
        }
    }

    let missing_in_baseline: Vec<String> = cur_map
        .keys()
        .filter(|p| !base_map.contains_key(**p))
        .map(|p| (*p).to_string())
        .collect();

    Ok(BaselineCompareReport {
        baseline_path: String::new(),
        compared_files: base_map.len(),
        regressions,
        improved,
        missing_in_current,
        missing_in_baseline,
    })
}

fn compare_entry_metrics(
    path: &str,
    base: &ShowcaseBaselineEntry,
    cur: &ShowcaseBaselineEntry,
    regressions: &mut Vec<MetricRegression>,
    improved: &mut Vec<MetricRegression>,
) {
    let checks: [(&str, f64, f64, bool); 12] = [
        ("score", base.score, cur.score, false),
        (
            "label_node_overlaps",
            base.metrics.label_node_overlaps as f64,
            cur.metrics.label_node_overlaps as f64,
            true,
        ),
        (
            "label_label_overlaps",
            base.metrics.label_label_overlaps as f64,
            cur.metrics.label_label_overlaps as f64,
            true,
        ),
        (
            "edge_crossings",
            base.metrics.edge_crossings as f64,
            cur.metrics.edge_crossings as f64,
            true,
        ),
        (
            "edge_node_crossings",
            base.metrics.edge_node_crossings as f64,
            cur.metrics.edge_node_crossings as f64,
            true,
        ),
        (
            "node_overlap_pairs",
            base.metrics.node_overlap_pairs as f64,
            cur.metrics.node_overlap_pairs as f64,
            true,
        ),
        (
            "edge_through_groups",
            base.metrics.edge_through_groups as f64,
            cur.metrics.edge_through_groups as f64,
            true,
        ),
        ("bend_count", base.metrics.bend_count as f64, cur.metrics.bend_count as f64, true),
        (
            "edge_parallel_overlap_count",
            base.metrics.edge_parallel_overlap_count as f64,
            cur.metrics.edge_parallel_overlap_count as f64,
            true,
        ),
        ("total_area", base.metrics.total_area, cur.metrics.total_area, true),
        (
            "total_edge_length",
            base.metrics.total_edge_length,
            cur.metrics.total_edge_length,
            true,
        ),
        (
            "aspect_ratio_deviation",
            base.metrics.aspect_ratio_deviation,
            cur.metrics.aspect_ratio_deviation,
            true,
        ),
    ];

    for (name, b, c, lower_is_better) in checks {
        let diff = c - b;
        let tolerance = metric_noise_tolerance(name);
        if lower_is_better {
            if diff <= tolerance {
                if diff < -tolerance {
                    improved.push(record_for(name, path, b, c, diff));
                }
                continue;
            }
        } else if diff >= -tolerance {
            if diff > tolerance {
                improved.push(record_for(name, path, b, c, diff));
            }
            continue;
        }

        let regressed = if lower_is_better { diff > 0.0 } else { diff < 0.0 };
        let record = record_for(name, path, b, c, diff);
        if regressed {
            regressions.push(record);
        } else {
            improved.push(record);
        }
    }
}

fn metric_noise_tolerance(name: &str) -> f64 {
    match name {
        "score" => 1.5,
        "total_area" | "total_edge_length" | "aspect_ratio_deviation" => 5.0,
        "edge_crossings" | "bend_count" | "edge_node_crossings" | "edge_through_groups"
        | "edge_parallel_overlap_count" => 2.0,
        // 标签与节点重叠：零容忍
        "label_node_overlaps" | "label_label_overlaps" | "node_overlap_pairs" => 0.0,
        _ => 0.0,
    }
}

fn record_for(
    name: &str,
    path: &str,
    baseline: f64,
    current: f64,
    diff: f64,
) -> MetricRegression {
    MetricRegression {
        path: path.to_string(),
        metric: name.to_string(),
        baseline,
        current,
        diff,
    }
}

struct EvaluatedEntry {
    diagram_type: String,
    metrics: LayoutMetrics,
    lint: LintMetricsSummary,
    score: f64,
    elapsed_us: u64,
}

fn evaluate_pgm_file(path: &Path) -> Result<EvaluatedEntry, String> {
    let source = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let output = parse_prepare_validate(&source, &StyleRequest::default());
    let prepared = match output {
        o if o.is_valid() => o.diagram.expect("valid output must have diagram"),
        o => {
            let msg = o
                .errors
                .first()
                .map(|e| e.to_string())
                .unwrap_or_else(|| "parse/prepare failed".to_string());
            return Err(msg);
        }
    };

    let diagram = prepared.inner();
    let start = std::time::Instant::now();
    let layout = compute_layout_with_plan(diagram, prepared.layout_plan())
        .map_err(|e| e.to_string())?;
    let elapsed_us = start.elapsed().as_micros() as u64;

    Ok(build_entry(diagram, &layout, elapsed_us))
}

fn build_entry(diagram: &Diagram, layout: &LayoutResult, elapsed_us: u64) -> EvaluatedEntry {
    let metrics = LayoutMetrics::compute(diagram, layout);
    let score = metrics.quality_score();
    EvaluatedEntry {
        diagram_type: diagram_type_name(&diagram.diagram_type),
        metrics: metrics.clone(),
        lint: metrics.lint.clone(),
        score,
        elapsed_us,
    }
}

fn diagram_type_name(dt: &DiagramType) -> String {
    match dt {
        DiagramType::Flowchart => "flowchart".to_string(),
        DiagramType::Architecture => "architecture".to_string(),
        DiagramType::State => "state".to_string(),
        DiagramType::Er => "er".to_string(),
        DiagramType::Sequence => "sequence".to_string(),
        DiagramType::Mindmap => "mindmap".to_string(),
        DiagramType::Custom(s) => format!("custom:{s}"),
    }
}

fn collect_pgm_files(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_pgm_files_recursive(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_pgm_files_recursive(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_pgm_files_recursive(&path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("pgm") {
            out.push(path);
        }
    }
    Ok(())
}

fn unix_timestamp_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_roundtrip_json() {
        let baseline = ShowcaseBaseline {
            version: 1,
            generated_at: "0".to_string(),
            entries: vec![],
        };
        let json = serde_json::to_string(&baseline).unwrap();
        let loaded: ShowcaseBaseline = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.version, baseline.version);
        assert_eq!(loaded.entries.len(), 0);
    }

    #[test]
    fn k8s_architecture_layout_is_deterministic() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let path = root.join("showcase/architecture/c.k8s-multi-cluster-federation.pgm");
        let first = evaluate_pgm_file(&path).expect("evaluate k8s showcase");
        for run in 1..=30 {
            let current = evaluate_pgm_file(&path).expect("evaluate k8s showcase");
            assert_eq!(
                current.metrics.edge_crossings,
                first.metrics.edge_crossings,
                "edge_crossings diverged on run {run}"
            );
            assert_eq!(
                (current.metrics.total_edge_length * 10.0).round(),
                (first.metrics.total_edge_length * 10.0).round(),
                "total_edge_length diverged on run {run}"
            );
            assert_eq!(
                current.metrics.bend_count,
                first.metrics.bend_count,
                "bend_count diverged on run {run}"
            );
            assert_eq!(
                current.metrics.edge_parallel_overlap_count,
                first.metrics.edge_parallel_overlap_count,
                "edge_parallel_overlap_count diverged on run {run}"
            );
        }
    }

    #[test]
    fn layout_stress_nested_is_deterministic() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let path = root.join("showcase/architecture/c.layout-stress-nested.pgm");
        let first = evaluate_pgm_file(&path).expect("evaluate stress-nested");
        for run in 1..=30 {
            let current = evaluate_pgm_file(&path).expect("evaluate stress-nested");
            assert_eq!(
                current.metrics.edge_crossings,
                first.metrics.edge_crossings,
                "edge_crossings diverged on run {run}"
            );
            assert_eq!(
                current.metrics.bend_count,
                first.metrics.bend_count,
                "bend_count diverged on run {run}"
            );
            assert_eq!(
                current.metrics.edge_parallel_overlap_count,
                first.metrics.edge_parallel_overlap_count,
                "edge_parallel_overlap_count diverged on run {run}"
            );
            assert_eq!(
                (current.metrics.total_edge_length * 10.0).round(),
                (first.metrics.total_edge_length * 10.0).round(),
                "total_edge_length diverged on run {run}"
            );
        }
    }

    #[test]
    fn showcase_architecture_layouts_are_deterministic() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let arch_dir = root.join("showcase/architecture");
        let files = collect_pgm_files(&arch_dir).expect("list architecture showcase");
        assert!(!files.is_empty(), "expected architecture showcase files");

        for path in &files {
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(path)
                .display()
                .to_string();
            let first = evaluate_pgm_file(path).unwrap_or_else(|e| {
                panic!("evaluate {rel}: {e}");
            });
            for run in 1..=10 {
                let current = evaluate_pgm_file(path).unwrap_or_else(|e| {
                    panic!("evaluate {rel} run {run}: {e}");
                });
                assert_eq!(
                    current.metrics.edge_crossings,
                    first.metrics.edge_crossings,
                    "{rel}: edge_crossings diverged on run {run}"
                );
                assert_eq!(
                    current.metrics.bend_count,
                    first.metrics.bend_count,
                    "{rel}: bend_count diverged on run {run}"
                );
                assert_eq!(
                    current.metrics.edge_parallel_overlap_count,
                    first.metrics.edge_parallel_overlap_count,
                    "{rel}: edge_parallel_overlap_count diverged on run {run}"
                );
                assert_eq!(
                    (current.metrics.total_edge_length * 10.0).round(),
                    (first.metrics.total_edge_length * 10.0).round(),
                    "{rel}: total_edge_length diverged on run {run}"
                );
            }
        }
    }
}
