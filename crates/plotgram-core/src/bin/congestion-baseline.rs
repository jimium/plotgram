//! 拥堵诊断基线（S0）：stub 占用冲突 + 层缝 deficit
//!
//! 用法:
//!   cargo run --release -p plotgram-core --bin congestion-baseline -- \
//!     [--set FILE] [--date YYYY-MM-DD] [file.pgm ...]
//!
//! 默认 set: benchmark-data/congestion-set.txt

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use plotgram_core::layout::{
    compute_congestion_sample_metrics, compute_layout_with_plan, CongestionBaselineSnapshot,
    CongestionSampleMetrics,
};
use plotgram_core::pipeline;
use plotgram_core::prepare::StyleRequest;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut set_file: Option<PathBuf> = None;
    let mut date = String::from("unknown");
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
            "-h" | "--help" => {
                eprintln!(
                    "用法: congestion-baseline [--set FILE] [--date YYYY-MM-DD] [file.pgm ...]\n\
                     默认 set: benchmark-data/congestion-set.txt"
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
    for path in &files {
        let rel = path.to_string_lossy().to_string();
        eprint!("  congestion {rel} ... ");
        let sample = measure_file(path);
        eprintln!(
            "cross_stub={} exact_cross={} layer_deficits={} max_def={:.1} shifted={:?}",
            sample.stub_cross_pair_conflicts,
            sample.stub_exact_cross_pairs,
            sample.layer_bands_with_deficit,
            sample.max_layer_deficit,
            sample.ortho_stub_shifted
        );
        samples.push(sample);
    }

    let snap = CongestionBaselineSnapshot {
        date,
        note: "S4: monitor-hub defer + outer-ring + trunk-aware reroute; S3 FanIn trunk; S2 layer; S1 stub".into(),
        samples,
    };
    println!("{}", serde_json::to_string_pretty(&snap).expect("serialize"));
}

fn measure_file(path: &Path) -> CongestionSampleMetrics {
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
    compute_congestion_sample_metrics(&path.to_string_lossy(), diagram, &result)
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
