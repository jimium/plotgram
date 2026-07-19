//! 拥堵 / 压力校准基线（S0 + P4.0b）
//!
//! 用法:
//!   PLOTGRAM_PRESSURE_BUDGET=0 cargo run --release -p plotgram-core --bin congestion-baseline -- \
//!     [--set FILE] [--date YYYY-MM-DD] [--calibrate] [file.pgm ...]
//!
//! 默认 set: benchmark-data/congestion-set.txt
//! `--calibrate`：额外输出 score 分位 / top-k 边 / 归一化说明（观测模式建议关 budget）。

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use plotgram_core::layout::{
    collect_edge_features, compute_congestion_sample_metrics, compute_corridor_model,
    compute_layout_with_plan, score_edges, CongestionBaselineSnapshot, CongestionSampleMetrics,
    DifficultyProfile,
};
use plotgram_core::pipeline;
use plotgram_core::prepare::StyleRequest;
use serde::Serialize;

#[derive(Debug, Serialize)]
struct CalibrateEdgeRow {
    edge_index: usize,
    from: String,
    to: String,
    score: f64,
    span_ranks: usize,
    band_deficit: f64,
    corridor_overflow: f64,
    port_pressure: usize,
    obstacle_hits: usize,
    grid_overflow: usize,
}

#[derive(Debug, Serialize)]
struct CalibrateFileReport {
    file: String,
    max_edge_score: f64,
    score_p50: f64,
    score_p90: f64,
    top_k: Vec<CalibrateEdgeRow>,
    note_norm: &'static str,
}

#[derive(Debug, Serialize)]
struct CalibrateExport {
    date: String,
    mode: &'static str,
    note: String,
    files: Vec<CalibrateFileReport>,
    congestion: CongestionBaselineSnapshot,
}

fn main() {
    // P4：默认观测模式关 D4 行为加缝，避免与校准混淆
    if env::var_os("PLOTGRAM_PRESSURE_BUDGET").is_none() {
        env::set_var("PLOTGRAM_PRESSURE_BUDGET", "0");
    }

    let args: Vec<String> = env::args().skip(1).collect();
    let mut set_file: Option<PathBuf> = None;
    let mut date = String::from("unknown");
    let mut calibrate = false;
    let mut files: Vec<PathBuf> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--set" => {
                i += 1;
                set_file = Some(PathBuf::from(
                    args.get(i).unwrap_or_else(|| die("缺少 --set 路径")),
                ));
            }
            "--date" => {
                i += 1;
                date = args.get(i).cloned().unwrap_or_else(|| die("缺少 --date"));
            }
            "--calibrate" => {
                calibrate = true;
            }
            "-h" | "--help" => {
                eprintln!(
                    "用法: congestion-baseline [--set FILE] [--date YYYY-MM-DD] [--calibrate] [file.pgm ...]\n\
                     默认 set: benchmark-data/congestion-set.txt\n\
                     默认 PLOTGRAM_PRESSURE_BUDGET=0（观测）"
                );
                process::exit(0);
            }
            other if other.starts_with('-') => die(&format!("未知参数: {other}")),
            other => files.push(PathBuf::from(other)),
        }
        i += 1;
    }

    if files.is_empty() {
        let candidates = [
            set_file.clone(),
            Some(PathBuf::from("benchmark-data/congestion-set.txt")),
        ];
        for c in candidates.into_iter().flatten() {
            if c.exists() {
                files = read_set_file(&c);
                break;
            }
        }
    }
    if files.is_empty() {
        die("无输入文件（检查 --set 或 benchmark-data/congestion-set.txt）");
    }

    let mut samples: Vec<CongestionSampleMetrics> = Vec::new();
    let mut calibrate_files: Vec<CalibrateFileReport> = Vec::new();
    for path in &files {
        let rel = path.to_string_lossy().to_string();
        eprint!("  congestion {rel} ... ");
        let (sample, cal) = measure_file(path, calibrate);
        eprintln!(
            "cross_stub={} exact_cross={} max_score={:.2} hits={} grid_ov={} shifted={:?}",
            sample.stub_cross_pair_conflicts,
            sample.stub_exact_cross_pairs,
            sample.max_edge_score,
            sample.total_obstacle_hits,
            sample.total_grid_overflow,
            sample.ortho_stub_shifted
        );
        samples.push(sample);
        if let Some(c) = cal {
            calibrate_files.push(c);
        }
    }

    let congestion = CongestionBaselineSnapshot {
        date: date.clone(),
        note: "P4.0a norm score; P1 pierce; P3 grid; PRESSURE_BUDGET default 0 for calibrate".into(),
        samples,
    };

    if calibrate {
        let export = CalibrateExport {
            date,
            mode: "calibrate",
            note: "score uses normalized features (ref_px=48, ref_edges=4, ref_port=4, ref_hits=3, ref_grid=3, ref_span=4); weights not auto-written".into(),
            files: calibrate_files,
            congestion,
        };
        println!("{}", serde_json::to_string_pretty(&export).expect("serialize"));
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&congestion).expect("serialize")
        );
    }
}

fn measure_file(path: &Path, calibrate: bool) -> (CongestionSampleMetrics, Option<CalibrateFileReport>) {
    let source = fs::read_to_string(path).unwrap_or_else(|e| die(&format!("读取失败 {path:?}: {e}")));
    let style_req = StyleRequest::default();
    let output = pipeline::parse_prepare(&source, &style_req);
    let prepared = output
        .diagram
        .as_ref()
        .unwrap_or_else(|| die(&format!("解析失败: {path:?}")));
    let diagram = prepared.inner();
    let plan = prepared.layout_plan();
    let result = compute_layout_with_plan(diagram, plan)
        .unwrap_or_else(|e| die(&format!("布局失败 {path:?}: {e}")));
    let sample = compute_congestion_sample_metrics(&path.to_string_lossy(), diagram, &result);

    let cal = if calibrate {
        let corridor = compute_corridor_model(diagram, &result);
        let features = collect_edge_features(diagram, &result, Some(&corridor));
        let scores = score_edges(&features, &DifficultyProfile::default());
        let mut sorted = scores.clone();
        sorted.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        let p50 = percentile(&sorted, 0.5);
        let p90 = percentile(&sorted, 0.9);
        let top_k: Vec<CalibrateEdgeRow> = scores
            .iter()
            .take(8)
            .filter_map(|(idx, score)| {
                features.iter().find(|f| f.edge_index == *idx).map(|f| {
                    CalibrateEdgeRow {
                        edge_index: *idx,
                        from: f.from.clone(),
                        to: f.to.clone(),
                        score: *score,
                        span_ranks: f.span_ranks,
                        band_deficit: f.band_deficit,
                        corridor_overflow: f.corridor_overflow,
                        port_pressure: f.port_pressure,
                        obstacle_hits: f.obstacle_hits,
                        grid_overflow: f.grid_overflow,
                    }
                })
            })
            .collect();
        Some(CalibrateFileReport {
            file: path.to_string_lossy().to_string(),
            max_edge_score: sample.max_edge_score,
            score_p50: p50,
            score_p90: p90,
            top_k,
            note_norm: "norm01(x/ref).clamp(0,1); channel=max(band_norm,corr_norm)",
        })
    } else {
        None
    };

    (sample, cal)
}

fn percentile(sorted_asc: &[(usize, f64)], p: f64) -> f64 {
    if sorted_asc.is_empty() {
        return 0.0;
    }
    let idx = ((sorted_asc.len() as f64 - 1.0) * p).round() as usize;
    sorted_asc[idx.min(sorted_asc.len() - 1)].1
}

fn read_set_file(path: &Path) -> Vec<PathBuf> {
    let text = fs::read_to_string(path).unwrap_or_else(|e| die(&format!("无法读 set {path:?}: {e}")));
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(PathBuf::from)
        .collect()
}

fn die(msg: &str) -> ! {
    eprintln!("error: {msg}");
    process::exit(1);
}
