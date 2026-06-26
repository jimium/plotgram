//! V2 反馈模式效果评估（Phase 2）
//!
//! 对比 V2 调整器开启 / 关闭时的事后路由质量，验证验收标准：
//! - `edge_node_crossings` 平均下降 > 30%
//! - `total_edge_length` 平均下降 > 10%
//! - 不引入新的 `node_overlap_pairs`
//!
//! 用法:
//!   cargo run -p drawify-eval --bin v2-effectiveness -- --showcase /path/to/showcase --stress /path/to/friendliness_stress

use drawify_core::layout;
use drawify_eval::engine::presets;
use drawify_eval::engine::EvalEngine;
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

struct ComparisonRow {
    diagram_name: String,
    layout_algo: String,
    family: Family,
    enc_off: usize,
    enc_on: usize,
    overlaps_off: usize,
    overlaps_on: usize,
    predicted_crossings_off: usize,
    predicted_crossings_on: usize,
    edge_length_off: f64,
    edge_length_on: f64,
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
        }
    }

    let engine = EvalEngine::new();
    let mut rows: Vec<ComparisonRow> = Vec::new();

    for (name, diagram) in &diagrams {
        let diagram_type = &diagram.diagram_type;
        let layout_names = layout::applicable_layouts_for_type(diagram_type);
        let layout_configs: Vec<_> = layout_names
            .iter()
            .map(|n| presets::set_layout_algo(n))
            .collect();

        for (i, config) in layout_configs.iter().enumerate() {
            let algo = &layout_names[i];

            // V2 关闭
            std::env::set_var("DRAWIFY_NO_V2_ADJUST", "1");
            let result_off = engine.evaluate(diagram, config);
            if result_off.timed_out {
                continue;
            }

            // V2 开启
            std::env::remove_var("DRAWIFY_NO_V2_ADJUST");
            let result_on = engine.evaluate(diagram, config);
            if result_on.timed_out {
                continue;
            }

            rows.push(ComparisonRow {
                diagram_name: name.clone(),
                layout_algo: algo.to_string(),
                family: family_of(algo),
                enc_off: result_off.metrics.edge_node_crossings,
                enc_on: result_on.metrics.edge_node_crossings,
                overlaps_off: result_off.metrics.node_overlap_pairs,
                overlaps_on: result_on.metrics.node_overlap_pairs,
                predicted_crossings_off: result_off.metrics.predicted_crossings,
                predicted_crossings_on: result_on.metrics.predicted_crossings,
                edge_length_off: result_off.metrics.total_edge_length,
                edge_length_on: result_on.metrics.total_edge_length,
            });
        }
    }

    // 确保退出时恢复环境变量
    std::env::remove_var("DRAWIFY_NO_V2_ADJUST");

    eprintln!("  收集 {} 个对比样本", rows.len());
    if rows.is_empty() {
        eprintln!("✗ 无有效样本");
        std::process::exit(1);
    }

    let n = rows.len();
    println!("# Phase 2 V2 反馈模式效果评估报告");
    println!();
    println!("样本数: {}", n);
    println!();

    // ── 总体验收 ──
    println!("## 1. 总体验收");
    println!();

    let total_enc_off: usize = rows.iter().map(|r| r.enc_off).sum();
    let total_enc_on: usize = rows.iter().map(|r| r.enc_on).sum();
    let total_overlaps_off: usize = rows.iter().map(|r| r.overlaps_off).sum();
    let total_overlaps_on: usize = rows.iter().map(|r| r.overlaps_on).sum();
    let total_pred_off: usize = rows.iter().map(|r| r.predicted_crossings_off).sum();
    let total_pred_on: usize = rows.iter().map(|r| r.predicted_crossings_on).sum();
    let total_el_off: f64 = rows.iter().map(|r| r.edge_length_off).sum();
    let total_el_on: f64 = rows.iter().map(|r| r.edge_length_on).sum();

    let enc_decrease = if total_enc_off > 0 {
        (1.0 - total_enc_on as f64 / total_enc_off as f64) * 100.0
    } else {
        0.0
    };
    let pred_decrease = if total_pred_off > 0 {
        (1.0 - total_pred_on as f64 / total_pred_off as f64) * 100.0
    } else {
        0.0
    };
    let overlap_delta = total_overlaps_on as i64 - total_overlaps_off as i64;
    let el_decrease = if total_el_off > 0.0 {
        (1.0 - total_el_on / total_el_off) * 100.0
    } else {
        0.0
    };

    println!("| 指标 | V2 关闭 | V2 开启 | 变化 | 验收 |");
    println!("|------|---------|---------|------|------|");
    println!(
        "| edge_node_crossings（总） | {} | {} | {:.1}% ↓ | {} |",
        total_enc_off,
        total_enc_on,
        enc_decrease,
        if enc_decrease > 30.0 { "✅ > 30%" } else { "⚠️ < 30%" }
    );
    println!(
        "| predicted_crossings（总） | {} | {} | {:.1}% ↓ | — |",
        total_pred_off, total_pred_on, pred_decrease
    );
    println!(
        "| total_edge_length（总） | {:.0} | {:.0} | {:.1}% ↓ | {} |",
        total_el_off,
        total_el_on,
        el_decrease,
        if el_decrease > 10.0 { "✅ > 10%" } else { "⚠️ < 10%" }
    );
    println!(
        "| node_overlap_pairs（总） | {} | {} | {:+} | {} |",
        total_overlaps_off,
        total_overlaps_on,
        overlap_delta,
        if overlap_delta <= 0 { "✅ 无新增" } else { "❌ 有新增" }
    );
    println!();

    // ── 逐样本统计 ──
    println!("## 2. 逐样本改善分布");
    println!();

    let enc_decreased = rows.iter().filter(|r| r.enc_on < r.enc_off).count();
    let enc_unchanged = rows.iter().filter(|r| r.enc_on == r.enc_off).count();
    let enc_increased = rows.iter().filter(|r| r.enc_on > r.enc_off).count();
    let enc_nonzero_off = rows.iter().filter(|r| r.enc_off > 0).count();
    let enc_nonzero_decreased = rows
        .iter()
        .filter(|r| r.enc_off > 0 && r.enc_on < r.enc_off)
        .count();

    println!("| 类别 | 样本数 | 占比 |");
    println!("|------|--------|------|");
    println!("| enc 下降 | {} | {:.1}% |", enc_decreased, enc_decreased as f64 / n as f64 * 100.0);
    println!("| enc 不变 | {} | {:.1}% |", enc_unchanged, enc_unchanged as f64 / n as f64 * 100.0);
    println!("| enc 上升 | {} | {:.1}% |", enc_increased, enc_increased as f64 / n as f64 * 100.0);
    println!(
        "| 其中 enc_off>0 的样本中下降比例 | {}/{} | {:.1}% |",
        enc_nonzero_decreased,
        enc_nonzero_off,
        if enc_nonzero_off > 0 {
            enc_nonzero_decreased as f64 / enc_nonzero_off as f64 * 100.0
        } else {
            0.0
        }
    );
    println!();

    // ── 分族统计 ──
    println!("## 3. 分族统计");
    println!();

    for fam in [Family::Hierarchical, Family::ForceDirected, Family::Radial] {
        let fam_rows: Vec<&ComparisonRow> = rows.iter().filter(|r| r.family == fam).collect();
        if fam_rows.is_empty() {
            continue;
        }
        let fn_count = fam_rows.len();
        let f_enc_off: usize = fam_rows.iter().map(|r| r.enc_off).sum();
        let f_enc_on: usize = fam_rows.iter().map(|r| r.enc_on).sum();
        let f_overlaps_off: usize = fam_rows.iter().map(|r| r.overlaps_off).sum();
        let f_overlaps_on: usize = fam_rows.iter().map(|r| r.overlaps_on).sum();
        let f_el_off: f64 = fam_rows.iter().map(|r| r.edge_length_off).sum();
        let f_el_on: f64 = fam_rows.iter().map(|r| r.edge_length_on).sum();
        let f_enc_decrease = if f_enc_off > 0 {
            (1.0 - f_enc_on as f64 / f_enc_off as f64) * 100.0
        } else {
            0.0
        };
        let f_overlap_delta = f_overlaps_on as i64 - f_overlaps_off as i64;
        let f_el_decrease = if f_el_off > 0.0 {
            (1.0 - f_el_on / f_el_off) * 100.0
        } else {
            0.0
        };

        println!(
        "### {}（n={}）", fam.label(), fn_count
        );
        println!();
        println!("| 指标 | V2 关闭 | V2 开启 | 变化 |");
        println!("|------|---------|---------|------|");
        println!("| enc 总 | {} | {} | {:.1}% ↓ |", f_enc_off, f_enc_on, f_enc_decrease);
        println!("| overlaps 总 | {} | {} | {:+} |", f_overlaps_off, f_overlaps_on, f_overlap_delta);
        println!("| edge_length 总 | {:.0} | {:.0} | {:.1}% |", f_el_off, f_el_on, f_el_decrease);
        println!();
    }

    // ── 改善最大的样本 ──
    println!("## 4. enc 改善最大的 10 个样本");
    println!();
    println!("| 图 | 布局 | enc off | enc on | 降幅 |");
    println!("|----|------|---------|--------|------|");

    let mut sorted: Vec<&ComparisonRow> = rows
        .iter()
        .filter(|r| r.enc_off > 0)
        .collect();
    sorted.sort_by(|a, b| {
        let dec_a = a.enc_off as f64 - a.enc_on as f64;
        let dec_b = b.enc_off as f64 - b.enc_on as f64;
        dec_b.partial_cmp(&dec_a).unwrap_or(std::cmp::Ordering::Equal)
    });

    for r in sorted.iter().take(10) {
        let decrease = if r.enc_off > 0 {
            (1.0 - r.enc_on as f64 / r.enc_off as f64) * 100.0
        } else {
            0.0
        };
        println!(
            "| {} | {} | {} | {} | {:.0}% |",
            r.diagram_name, r.layout_algo, r.enc_off, r.enc_on, decrease
        );
    }
    println!();

    // ── enc 上升的样本（如果有）──
    let increased: Vec<&ComparisonRow> = rows.iter().filter(|r| r.enc_on > r.enc_off).collect();
    if !increased.is_empty() {
        println!("## 5. enc 上升的样本（{} 个）", increased.len());
        println!();
        println!("| 图 | 布局 | enc off | enc on | 增量 |");
        println!("|----|------|---------|--------|------|");
        for r in increased.iter().take(20) {
            println!(
                "| {} | {} | {} | {} | +{} |",
                r.diagram_name, r.layout_algo, r.enc_off, r.enc_on,
                r.enc_on as i64 - r.enc_off as i64
            );
        }
        println!();
    }

    // ── 验收结论 ──
    println!("## 6. 验收结论");
    println!();
    let enc_pass = enc_decrease > 30.0;
    let el_pass = el_decrease > 10.0;
    let overlap_pass = overlap_delta <= 0;
    println!(
        "- `edge_node_crossings` 平均下降 {:.1}%：{}（阈值 > 30%）",
        enc_decrease,
        if enc_pass { "✅ 通过" } else { "⚠️ 未达" }
    );
    println!(
        "- `total_edge_length` 平均下降 {:.1}%：{}（阈值 > 10%）",
        el_decrease,
        if el_pass { "✅ 通过" } else { "⚠️ 未达" }
    );
    println!(
        "- 不引入新 `node_overlap_pairs`：{}（delta = {:+}）",
        if overlap_pass { "✅ 通过" } else { "❌ 失败" },
        overlap_delta
    );
    println!();
    if enc_pass && el_pass && overlap_pass {
        println!("**Phase 2 验收通过。**");
    } else {
        println!("**Phase 2 验收未完全通过**，需进一步调优。");
    }
}
