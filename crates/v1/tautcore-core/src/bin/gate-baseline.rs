//! 门禁基线采集（质量严重度 / lint / 节点指纹）
//!
//! 用法:
//!   cargo run --release -p tautcore-core --bin gate-baseline -- \
//!     [--runs N] [--set FILE] [--date YYYY-MM-DD] [file.taut ...]
//!
//! 默认读取 `benchmarks/sets/product-regression-set.txt`（日常质量硬门禁）。
//! 采集 stress 探针：`--set benchmarks/sets/stress-probe-set.txt`。
//! 输出 JSON 到 stdout；每条样本可由文件名首段解析角色（product/stress/...）。

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use tautcore_core::layout::{
    compute_collinear_sample_metrics, compute_layout_with_plan, CollinearBaselineSnapshot,
    CollinearSampleMetrics,
};
use tautcore_core::pipeline;
use tautcore_core::prepare::StyleRequest;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut runs: usize = 1;
    let mut set_file: Option<PathBuf> = None;
    let mut date = String::from("unknown");
    let mut files: Vec<PathBuf> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--runs" => {
                i += 1;
                runs = args
                    .get(i)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_else(|| die("无效 --runs"));
            }
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
                    "用法: gate-baseline [--runs N] [--set FILE] [--date YYYY-MM-DD] [file.taut ...]\n\
                     默认 set: benchmarks/sets/product-regression-set.txt"
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
            Some(PathBuf::from("benchmarks/sets/product-regression-set.txt")),
        ];
        let mut loaded = Vec::new();
        for c in candidates.into_iter().flatten() {
            if c.exists() {
                loaded = read_set_file(&c);
                break;
            }
        }
        files = loaded;
    }

    if files.is_empty() {
        die("无输入文件（检查 --set 或 benchmarks/sets/product-regression-set.txt）");
    }

    let mut samples: Vec<CollinearSampleMetrics> = Vec::new();
    for path in &files {
        let rel = path.to_string_lossy().to_string();
        eprint!("  gate {rel} ... ");
        let sample = measure_file(path, runs);
        let fp_short = &sample.node_fp[..8.min(sample.node_fp.len())];
        eprintln!(
            "exact_sev={:.1} tight_sev={:.1} fp={}",
            sample.exact_sev, sample.tight_sev, fp_short
        );
        samples.push(sample);
    }

    let snap = CollinearBaselineSnapshot {
        date,
        note: "role-aware baseline: product-gate hard; stress/demo quality soft".into(),
        samples,
        perf_runs: None,
    };
    println!("{}", serde_json::to_string_pretty(&snap).expect("serialize"));
}

fn measure_file(path: &Path, runs: usize) -> CollinearSampleMetrics {
    let source = fs::read_to_string(path).unwrap_or_else(|e| die(&format!("读取失败 {path:?}: {e}")));
    let style_req = StyleRequest::default();
    let output = pipeline::parse_prepare(&source, &style_req);
    let prepared = output
        .diagram
        .as_ref()
        .unwrap_or_else(|| die(&format!("解析失败: {path:?}")));
    let diagram = prepared.inner();
    let plan = prepared.layout_plan();

    let mut last = None;
    for _ in 0..runs.max(1) {
        last = Some(
            compute_layout_with_plan(diagram, plan)
                .unwrap_or_else(|e| die(&format!("布局失败 {path:?}: {e}"))),
        );
    }
    let result = last.expect("runs>=1");
    let label = path.to_string_lossy();
    compute_collinear_sample_metrics(&label, diagram, &result)
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
