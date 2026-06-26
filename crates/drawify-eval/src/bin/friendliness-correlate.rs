//! 路由友好性预测度量与事后路由质量的相关性分析（Phase 1.5）
//!
//! Phase 1.5 改进：
//! - 补充 Spearman 秩相关（应对事后度量稀疏性）
//! - 按布局族（层次 / 力导向 / 放射）分组校准权重
//! - 支持 --stress <dir> 扫描额外压力样本目录（扩样至 500+）
//!
//! 用法:
//!   cargo run -p drawify-eval --bin friendliness-correlate
//!   cargo run -p drawify-eval --bin friendliness-correlate -- --stress /path/to/friendliness_stress

use drawify_core::layout;
use drawify_eval::engine::presets;
use drawify_eval::engine::EvalEngine;
use drawify_eval::metrics::LayoutMetrics;
use std::fs;
use std::path::Path;

fn try_parse_diagram(source: &str) -> Option<drawify_core::ast::Diagram> {
    let raw = drawify_core::pipeline::parse(source).ok()?;
    let output =
        drawify_core::pipeline::prepare(raw, &drawify_core::prepare::StyleRequest::default())
            .ok()?;
    Some(output.diagram.into_inner())
}

fn load_dir(dir: &Path, into: &mut Vec<(String, drawify_core::ast::Diagram)>) {
    let mut dfy_files: Vec<_> = Vec::new();
    for entry in walkdir(dir) {
        let path = entry.path();
        if path.extension().map(|e| e == "dfy").unwrap_or(false) {
            dfy_files.push(path.to_path_buf());
        }
    }
    dfy_files.sort();
    for path in &dfy_files {
        let source = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if let Some(diagram) = try_parse_diagram(&source) {
            let name = path
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .trim_end_matches(".dfy")
                .to_string();
            into.push((name, diagram));
        }
    }
}

fn walkdir(dir: &Path) -> Vec<std::fs::DirEntry> {
    let mut result = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                result.extend(walkdir(&path));
            } else {
                result.push(entry);
            }
        }
    }
    result
}

/// Pearson 相关系数
fn pearson(xs: &[f64], ys: &[f64]) -> f64 {
    let n = xs.len() as f64;
    if n < 2.0 {
        return 0.0;
    }
    let mean_x: f64 = xs.iter().sum::<f64>() / n;
    let mean_y: f64 = ys.iter().sum::<f64>() / n;
    let mut cov = 0.0;
    let mut var_x = 0.0;
    let mut var_y = 0.0;
    for (x, y) in xs.iter().zip(ys.iter()) {
        let dx = x - mean_x;
        let dy = y - mean_y;
        cov += dx * dy;
        var_x += dx * dx;
        var_y += dy * dy;
    }
    let denom = (var_x * var_y).sqrt();
    if denom < f64::EPSILON {
        0.0
    } else {
        cov / denom
    }
}

/// Spearman 秩相关系数（= Pearson of rank-transformed values）
///
/// 对稀疏/非正态分布的事后度量（如 edge_node_crossings 66% 为 0）更鲁棒。
fn spearman(xs: &[f64], ys: &[f64]) -> f64 {
    if xs.len() < 2 {
        return 0.0;
    }
    let rx = rank(xs);
    let ry = rank(ys);
    pearson(&rx, &ry)
}

/// 平均秩（同值取平均秩，处理 ties）
fn rank(vals: &[f64]) -> Vec<f64> {
    let n = vals.len();
    let mut indexed: Vec<(usize, f64)> = vals.iter().enumerate().map(|(i, &v)| (i, v)).collect();
    indexed.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut ranks = vec![0.0; n];
    let mut i = 0;
    while i < n {
        let mut j = i + 1;
        while j < n && indexed[j].1 == indexed[i].1 {
            j += 1;
        }
        // [i, j) 同值，取平均秩 (i+1 + j) / 2
        let avg_rank = ((i + 1) + j) as f64 / 2.0;
        for k in i..j {
            ranks[indexed[k].0] = avg_rank;
        }
        i = j;
    }
    ranks
}

/// Z-score 归一化（mean=0, stddev=1）；stddev=0 时返回全 0
fn zscore(vals: &[f64]) -> Vec<f64> {
    let n = vals.len() as f64;
    if n == 0.0 {
        return vec![];
    }
    let mean: f64 = vals.iter().sum::<f64>() / n;
    let var: f64 = vals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
    let stddev = var.sqrt();
    if stddev < f64::EPSILON {
        return vec![0.0; vals.len()];
    }
    vals.iter().map(|v| (v - mean) / stddev).collect()
}

/// 布局族
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    Hierarchical,
    ForceDirected,
    Radial,
}

impl Family {
    fn label(&self) -> &'static str {
        match self {
            Family::Hierarchical => "层次类",
            Family::ForceDirected => "力导向类",
            Family::Radial => "放射/分组类",
        }
    }
}

fn family_of(layout_name: &str) -> Family {
    match layout_name {
        "sugiyama" | "sugiyama-v2" | "flowchart" | "er" | "state" => Family::Hierarchical,
        "force-directed" => Family::ForceDirected,
        "circular" | "architecture" | "mindmap" => Family::Radial,
        _ => Family::Hierarchical,
    }
}

struct Sample {
    diagram_name: String,
    layout_algo: String,
    family: Family,
    metrics: LayoutMetrics,
    friendliness_score: f64,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let showcase_path = args
        .iter()
        .position(|a| a == "--showcase")
        .and_then(|i| args.get(i + 1).cloned())
        .unwrap_or_else(|| {
            let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
            format!("{}/../../../showcase", manifest_dir)
        });
    let stress_path = args
        .iter()
        .position(|a| a == "--stress")
        .and_then(|i| args.get(i + 1).cloned());

    let showcase_dir = Path::new(&showcase_path);
    if !showcase_dir.exists() {
        eprintln!("✗ showcase 目录不存在: {}", showcase_path);
        std::process::exit(1);
    }

    eprintln!("▶ 扫描 showcase 目录: {}", showcase_path);
    let mut diagrams = Vec::new();
    load_dir(showcase_dir, &mut diagrams);
    eprintln!("  加载 {} 个图文件（showcase）", diagrams.len());

    if let Some(sp) = &stress_path {
        let stress_dir = Path::new(sp);
        if stress_dir.exists() {
            let before = diagrams.len();
            load_dir(stress_dir, &mut diagrams);
            eprintln!("  加载 {} 个图文件（stress: {}）", diagrams.len() - before, sp);
        } else {
            eprintln!("⚠ stress 目录不存在: {}（跳过）", sp);
        }
    }

    let engine = EvalEngine::new();
    let mut samples: Vec<Sample> = Vec::new();

    for (name, diagram) in &diagrams {
        let diagram_type = &diagram.diagram_type;
        let layout_names = layout::applicable_layouts_for_type(diagram_type);
        let layout_configs: Vec<_> = layout_names
            .iter()
            .map(|n| presets::set_layout_algo(n))
            .collect();

        for (i, config) in layout_configs.iter().enumerate() {
            let result = engine.evaluate(diagram, config);
            if result.timed_out {
                continue;
            }
            samples.push(Sample {
                diagram_name: name.clone(),
                layout_algo: layout_names[i].to_string(),
                family: family_of(layout_names[i]),
                metrics: result.metrics.clone(),
                friendliness_score: result.friendliness_score,
            });
        }
    }

    eprintln!("  收集 {} 个 (布局, 度量) 样本", samples.len());
    if samples.len() < 5 {
        eprintln!("✗ 样本数不足（< 5），无法做相关性分析");
        std::process::exit(1);
    }

    let n = samples.len();
    println!("# Phase 1.5 路由友好性预测度量相关性分析报告");
    println!();
    println!("样本数: {}", n);
    println!();

    // ── 全局相关性 ──
    println!("## 1. 全局描述性统计");
    println!();
    print_descriptive(&samples);
    println!();

    println!("## 2. 全局 Pearson + Spearman 相关系数");
    println!();
    print_correlation_table(&samples);
    println!();

    // ── 全局权重推荐 ──
    println!("## 3. 全局权重推荐（Pearson 归一化）");
    println!();
    let global_weights = print_weights(&samples);
    println!();

    // ── 复合分数相关性 ──
    println!("## 4. 复合友好度分数相关性（全局）");
    println!();
    print_composite(&samples, &global_weights);
    println!();

    // ── V1 评估器 friendliness_score 相关性 ──
    println!("## 5. V1 评估器 friendliness_score 相关性（Phase 1.5 分族权重）");
    println!();
    print_v1_evaluator(&samples);
    println!();

    // ── V1 z-score 变体（测试 z-score 归一化能否达标）──
    println!("## 5b. V1 z-score 变体（分族 z-score + 分族权重，模拟最优归一化）");
    println!();
    print_v1_zscore_variant(&samples);
    println!();

    // ── 分族相关性分析 ──
    println!("## 6. 分族相关性分析（Phase 1.5 分组校准）");
    println!();
    print_family_analysis(&samples);
    println!();

    // ── 验收判定 ──
    println!("## 7. 验收判定");
    println!();
    print_verdict(&samples, &global_weights);
    println!();

    // ── 样本明细 ──
    println!("## 8. 样本明细（按 edge_node_crossings 降序，前 20）");
    println!();
    print_sample_details(&samples);
}

fn extract_predictors(samples: &[Sample]) -> (Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>) {
    let channel_congestion: Vec<f64> = samples.iter().map(|s| s.metrics.channel_congestion).collect();
    let long_edge_count: Vec<f64> = samples.iter().map(|s| s.metrics.long_edge_count as f64).collect();
    let group_gap_deficit: Vec<f64> = samples.iter().map(|s| s.metrics.group_gap_deficit).collect();
    let predicted_crossings: Vec<f64> = samples.iter().map(|s| s.metrics.predicted_crossings as f64).collect();
    let port_conflict_score: Vec<f64> = samples.iter().map(|s| s.metrics.port_conflict_score).collect();
    let edge_node_crossings: Vec<f64> = samples.iter().map(|s| s.metrics.edge_node_crossings as f64).collect();
    let edge_crossings: Vec<f64> = samples.iter().map(|s| s.metrics.edge_crossings as f64).collect();
    (channel_congestion, long_edge_count, group_gap_deficit, predicted_crossings, port_conflict_score, edge_node_crossings, edge_crossings)
}

fn predictor_refs(samples: &[Sample]) -> [(&'static str, Vec<f64>); 5] {
    let (cc, le, gg, pc, pf, _, _) = extract_predictors(samples);
    [
        ("channel_congestion", cc),
        ("long_edge_count", le),
        ("group_gap_deficit", gg),
        ("predicted_crossings", pc),
        ("port_conflict_score", pf),
    ]
}

fn print_descriptive(samples: &[Sample]) {
    let predictors = predictor_refs(samples);
    let (_, _, _, _, _, enc, ec) = extract_predictors(samples);
    println!("| 度量 | min | median | max | mean | nonzero_count |");
    println!("|------|-----|--------|-----|------|---------------|");
    for (name, vals) in &predictors {
        print_stats(name, vals);
    }
    print_stats("edge_node_crossings (事后)", &enc);
    print_stats("edge_crossings (事后)", &ec);
}

fn print_stats(name: &str, vals: &[f64]) {
    if vals.is_empty() {
        println!("| {} | - | - | - | - | 0 |", name);
        return;
    }
    let mut sorted: Vec<f64> = vals.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let min = sorted[0];
    let max = sorted[sorted.len() - 1];
    let median = sorted[sorted.len() / 2];
    let mean: f64 = vals.iter().sum::<f64>() / vals.len() as f64;
    let nonzero = vals.iter().filter(|&&v| v > 0.0).count();
    println!("| {} | {:.2} | {:.2} | {:.2} | {:.2} | {} / {} |",
        name, min, median, max, mean, nonzero, vals.len());
}

fn print_correlation_table(samples: &[Sample]) {
    let predictors = predictor_refs(samples);
    let (_, _, _, _, _, enc, ec) = extract_predictors(samples);
    println!("| 预测度量 | Pearson vs enc | Pearson vs ec | Spearman vs enc | Spearman vs ec |");
    println!("|----------|----------------|---------------|-----------------|----------------|");
    for (name, vals) in &predictors {
        let r_enc = pearson(vals, &enc);
        let r_ec = pearson(vals, &ec);
        let s_enc = spearman(vals, &enc);
        let s_ec = spearman(vals, &ec);
        println!("| {} | {:.4} | {:.4} | {:.4} | {:.4} |", name, r_enc, r_ec, s_enc, s_ec);
    }
}

fn print_weights(samples: &[Sample]) -> Vec<f64> {
    let predictors = predictor_refs(samples);
    let (_, _, _, _, _, enc, _) = extract_predictors(samples);
    println!("以 `edge_node_crossings` 的 Pearson 相关性为权重依据：");
    println!();
    let correlations: Vec<f64> = predictors.iter().map(|(_, v)| pearson(v, &enc).max(0.0)).collect();
    let sum_w: f64 = correlations.iter().sum();
    let weight_labels = ["w1 (congestion)", "w2 (long_edge)", "w3 (group_gap)", "w4 (predicted_crossings)", "w5 (port_conflict)"];
    println!("| 度量 | r(enc) | 权重 w |");
    println!("|------|--------|--------|");
    let weights: Vec<f64> = if sum_w > f64::EPSILON {
        correlations.iter().map(|w| w / sum_w).collect()
    } else {
        vec![0.2; 5]
    };
    for (i, (name, _)) in predictors.iter().enumerate() {
        println!("| {} | {:.4} | {:.4} ({}) |", name, correlations[i], weights[i], weight_labels[i]);
    }
    weights
}

fn print_composite(samples: &[Sample], weights: &[f64]) {
    let n = samples.len();
    let predictors = predictor_refs(samples);
    let (_, _, _, _, _, enc, ec) = extract_predictors(samples);
    let z_scores: Vec<Vec<f64>> = predictors.iter().map(|(_, v)| zscore(v)).collect();
    let composite: Vec<f64> = (0..n)
        .map(|i| (0..5).map(|j| weights[j] * z_scores[j][i]).sum::<f64>())
        .collect();
    let r_enc = pearson(&composite, &enc);
    let r_ec = pearson(&composite, &ec);
    let s_enc = spearman(&composite, &enc);
    let s_ec = spearman(&composite, &ec);
    println!("各预测度量 z-score 归一化后按推荐权重加权求和：");
    println!();
    println!("| 复合分数 vs | Pearson | Spearman |");
    println!("|------------|---------|----------|");
    println!("| edge_node_crossings | {:.4} | {:.4} |", r_enc, s_enc);
    println!("| edge_crossings | {:.4} | {:.4} |", r_ec, s_ec);
}

fn print_v1_evaluator(samples: &[Sample]) {
    let scores: Vec<f64> = samples.iter().map(|s| s.friendliness_score).collect();
    let (_, _, _, _, _, enc, ec) = extract_predictors(samples);
    let r_enc = pearson(&scores, &enc);
    let r_ec = pearson(&scores, &ec);
    let s_enc = spearman(&scores, &enc);
    let s_ec = spearman(&scores, &ec);
    println!("V1 评估器（`RoutingFriendlinessEvaluator`，Phase 1.5 分族权重）复合分数：");
    println!();
    println!("| V1 friendliness_score vs | Pearson | Spearman |");
    println!("|--------------------------|---------|----------|");
    println!("| edge_node_crossings | {:.4} | {:.4} |", r_enc, s_enc);
    println!("| edge_crossings | {:.4} | {:.4} |", r_ec, s_ec);
}

/// V1 z-score 变体：模拟将 V1 评估器从软饱和归一化改为 z-score 归一化后的效果。
///
/// 两种变体：
/// - 全局 z-score + 分族权重（仅需全局 μ/σ，实现简单）
/// - 分族 z-score + 分族权重（需分族 μ/σ，效果可能更好）
///
/// 同时输出分族 μ/σ 供后续硬编码到评估器。
fn print_v1_zscore_variant(samples: &[Sample]) {
    let n = samples.len();
    let predictors = predictor_refs(samples);
    let (_, _, _, _, _, enc, ec) = extract_predictors(samples);
    let families = [Family::Hierarchical, Family::ForceDirected, Family::Radial];

    // 分族权重（与 mod.rs FriendlinessWeights 一致）
    let family_weights: [[f64; 5]; 3] = [
        [0.06, 0.26, 0.0, 0.66, 0.02],  // Hierarchical
        [0.31, 0.0, 0.01, 0.32, 0.36],  // Force-directed
        [0.33, 0.0, 0.03, 0.39, 0.25],  // Radial
    ];

    // ── 变体 A：全局 z-score + 分族权重 ──
    let global_z: Vec<Vec<f64>> = predictors.iter().map(|(_, v)| zscore(v)).collect();
    let composite_a: Vec<f64> = (0..n)
        .map(|i| {
            let fam_idx = match samples[i].family {
                Family::Hierarchical => 0,
                Family::ForceDirected => 1,
                Family::Radial => 2,
            };
            let w = family_weights[fam_idx];
            (0..5).map(|j| w[j] * global_z[j][i]).sum::<f64>()
        })
        .collect();

    // ── 变体 B：分族 z-score + 分族权重 ──
    let mut composite_b = vec![0.0f64; n];
    // 同时收集分族 μ/σ 供输出
    let mut fam_stats: Vec<[(f64, f64); 5]> = Vec::new();

    for (fam_idx, fam) in families.iter().enumerate() {
        let fam_indices: Vec<usize> = samples.iter().enumerate()
            .filter(|(_, s)| s.family == *fam)
            .map(|(i, _)| i)
            .collect();
        if fam_indices.len() < 2 {
            fam_stats.push([(0.0, 1.0); 5]);
            continue;
        }
        let w = family_weights[fam_idx];
        let mut stats = [(0.0f64, 1.0f64); 5];
        for j in 0..5 {
            let vals: Vec<f64> = fam_indices.iter().map(|&i| predictors[j].1[i]).collect();
            let mean = vals.iter().sum::<f64>() / vals.len() as f64;
            let var = vals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / vals.len() as f64;
            let std = var.sqrt().max(f64::EPSILON);
            stats[j] = (mean, std);
            for &i in &fam_indices {
                composite_b[i] += w[j] * (predictors[j].1[i] - mean) / std;
            }
        }
        fam_stats.push(stats);
    }

    let ra_enc = pearson(&composite_a, &enc);
    let ra_ec = pearson(&composite_a, &ec);
    let ra_s_enc = spearman(&composite_a, &enc);
    let rb_enc = pearson(&composite_b, &enc);
    let rb_ec = pearson(&composite_b, &ec);
    let rb_s_enc = spearman(&composite_b, &enc);

    println!("| 变体 | Pearson vs enc | Spearman vs enc | Pearson vs ec |");
    println!("|------|----------------|-----------------|---------------|");
    println!("| A: 全局 z-score + 分族权重 | {:.4} | {:.4} | {:.4} |", ra_enc, ra_s_enc, ra_ec);
    println!("| B: 分族 z-score + 分族权重 | {:.4} | {:.4} | {:.4} |", rb_enc, rb_s_enc, rb_ec);
    println!();
    println!("分族 μ / σ（供硬编码）：");
    println!();
    let metric_names = ["congestion", "long_edge", "group_gap", "predicted", "port"];
    for (fam_idx, fam) in families.iter().enumerate() {
        let count = samples.iter().filter(|s| s.family == *fam).count();
        println!("{} (n={}):", fam.label(), count);
        for j in 0..5 {
            let (m, s) = fam_stats[fam_idx][j];
            println!("  {} μ={:.4} σ={:.4}", metric_names[j], m, s);
        }
    }
}

fn print_family_analysis(samples: &[Sample]) {
    let families = [Family::Hierarchical, Family::ForceDirected, Family::Radial];
    for fam in families {
        let fam_samples: Vec<&Sample> = samples.iter().filter(|s| s.family == fam).collect();
        if fam_samples.len() < 5 {
            println!("### {}（样本数 {} < 5，跳过）", fam.label(), fam_samples.len());
            println!();
            continue;
        }
        println!("### {}（样本数 {}）", fam.label(), fam_samples.len());
        println!();
        let owned: Vec<Sample> = fam_samples.iter().map(|s| Sample {
            diagram_name: s.diagram_name.clone(),
            layout_algo: s.layout_algo.clone(),
            family: s.family,
            metrics: s.metrics.clone(),
            friendliness_score: s.friendliness_score,
        }).collect();
        let predictors = predictor_refs(&owned);
        let (_, _, _, _, _, enc, ec) = extract_predictors(&owned);
        println!("| 预测度量 | Pearson vs enc | Spearman vs enc | Pearson vs ec |");
        println!("|----------|----------------|-----------------|---------------|");
        for (name, vals) in &predictors {
            println!("| {} | {:.4} | {:.4} | {:.4} |",
                name, pearson(vals, &enc), spearman(vals, &enc), pearson(vals, &ec));
        }
        println!();

        // 分族权重推荐
        let correlations: Vec<f64> = predictors.iter().map(|(_, v)| pearson(v, &enc).max(0.0)).collect();
        let sum_w: f64 = correlations.iter().sum();
        let weight_labels = ["w1", "w2", "w3", "w4", "w5"];
        let weights: Vec<f64> = if sum_w > f64::EPSILON {
            correlations.iter().map(|w| w / sum_w).collect()
        } else {
            vec![0.2; 5]
        };
        println!("推荐权重: ");
        for (i, (name, _)) in predictors.iter().enumerate() {
            println!("- {} = {:.4}（{}）", weight_labels[i], weights[i], name);
        }
        println!();

        // 分族 V1 评估器相关性
        let scores: Vec<f64> = owned.iter().map(|s| s.friendliness_score).collect();
        println!("V1 评估器（分族权重）vs enc: Pearson = {:.4}, Spearman = {:.4}",
            pearson(&scores, &enc), spearman(&scores, &enc));
        println!();
    }
}

fn print_verdict(samples: &[Sample], weights: &[f64]) {
    let n = samples.len();
    let predictors = predictor_refs(samples);
    let (_, _, _, _, _, enc, ec) = extract_predictors(samples);
    let z_scores: Vec<Vec<f64>> = predictors.iter().map(|(_, v)| zscore(v)).collect();
    let composite: Vec<f64> = (0..n)
        .map(|i| (0..5).map(|j| weights[j] * z_scores[j][i]).sum::<f64>())
        .collect();
    let r_comp_enc = pearson(&composite, &enc);
    let r_comp_ec = pearson(&composite, &ec);
    let scores: Vec<f64> = samples.iter().map(|s| s.friendliness_score).collect();
    let r_v1_enc = pearson(&scores, &enc);
    let r_v1_ec = pearson(&scores, &ec);

    let best_enc = predictors.iter().map(|(_, v)| pearson(v, &enc)).fold(f64::NEG_INFINITY, f64::max);
    let best_ec = predictors.iter().map(|(_, v)| pearson(v, &ec)).fold(f64::NEG_INFINITY, f64::max);

    let verdict = |r: f64, thresh: f64| -> &'static str {
        if r > thresh { "✅ 通过" } else if r > thresh - 0.15 { "⚠️ 接近" } else { "❌ 未通过" }
    };

    println!("### 样本规模");
    println!();
    println!("- 总样本数: {}（目标 ≥ 500）→ {}", n, if n >= 500 { "✅ 达标" } else { "⚠️ 不足" });
    println!();
    println!("### 单维度量");
    println!();
    println!("- 最佳单维 vs `edge_node_crossings` Pearson = {:.4}（阈值 > 0.6）→ {}", best_enc, verdict(best_enc, 0.6));
    println!("- 最佳单维 vs `edge_crossings` Pearson = {:.4}（阈值 > 0.5）→ {}", best_ec, verdict(best_ec, 0.5));
    println!();
    println!("### 复合分数（五维加权）");
    println!();
    println!("- 复合 vs `edge_node_crossings` Pearson = {:.4}（阈值 > 0.6）→ {}", r_comp_enc, verdict(r_comp_enc, 0.6));
    println!("- 复合 vs `edge_crossings` Pearson = {:.4}（阈值 > 0.5）→ {}", r_comp_ec, verdict(r_comp_ec, 0.5));
    println!();
    println!("### V1 评估器 friendliness_score（分族权重）");
    println!();
    println!("- V1 vs `edge_node_crossings` Pearson = {:.4}（阈值 > 0.6）→ {}", r_v1_enc, verdict(r_v1_enc, 0.6));
    println!("- V1 vs `edge_crossings` Pearson = {:.4}（阈值 > 0.5）→ {}", r_v1_ec, verdict(r_v1_ec, 0.5));
}

fn print_sample_details(samples: &[Sample]) {
    let n = samples.len();
    let mut indexed: Vec<usize> = (0..n).collect();
    indexed.sort_by(|&a, &b| samples[b].metrics.edge_node_crossings.cmp(&samples[a].metrics.edge_node_crossings));
    println!("| diagram | layout | family | enc | ec | cong | long | gap | pred | port | v1_score |");
    println!("|---------|--------|--------|-----|----|------|------|-----|------|------|----------|");
    for &i in indexed.iter().take(20) {
        let s = &samples[i];
        let m = &s.metrics;
        println!("| {} | {} | {} | {} | {} | {:.2} | {} | {:.0} | {} | {:.0} | {:.4} |",
            s.diagram_name, s.layout_algo, s.family.label(),
            m.edge_node_crossings, m.edge_crossings,
            m.channel_congestion, m.long_edge_count,
            m.group_gap_deficit, m.predicted_crossings, m.port_conflict_score,
            s.friendliness_score);
    }
}
