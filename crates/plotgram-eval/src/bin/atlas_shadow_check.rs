//! Atlas shadow 对拍验证（诊断工具，**非 CI 门禁**）。
//!
//! 用法：
//!   cargo run -p plotgram-eval --bin atlas_shadow_check
//!   cargo run -p plotgram-eval --bin atlas_shadow_check -- --set benchmarks/sets/product-regression-set.txt
//!
//! 输出：逐图 shadow 报告 + 汇总 p95/max 统计。

use plotgram_core::layout::atlas::shadow::run_shadow;
use plotgram_core::layout::atlas::solve::is_hierarchical;
use std::fs;
use std::path::{Path, PathBuf};

/// 仓库根。
fn repo_root() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    Path::new(&manifest).join("../..")
}

/// 读取图集清单。
fn load_set(set_path: &Path, root: &Path) -> Vec<(String, String)> {
    let content = match fs::read_to_string(set_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("✗ 无法读取图集清单 {:?}: {}", set_path, e);
            std::process::exit(1);
        }
    };
    let mut entries = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let path = root.join(line);
        let source = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("  跳过 {}: {}", line, e);
                continue;
            }
        };
        let name = Path::new(line)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(line)
            .trim_end_matches(".pgm")
            .to_string();
        entries.push((name, source));
    }
    entries
}

fn try_parse(source: &str) -> Option<plotgram_core::ast::PreparedDiagram> {
    let raw = plotgram_core::pipeline::parse(source).ok()?;
    let output =
        plotgram_core::pipeline::prepare(raw, &plotgram_core::prepare::StyleRequest::default())
            .ok()?;
    Some(output.diagram)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let root = repo_root();

    // 默认图集
    let default_set = "benchmarks/sets/product-regression-set.txt";
    let set_path = if let Some(idx) = args.iter().position(|a| a == "--set") {
        args.get(idx + 1).map(|s| root.join(s)).unwrap_or_else(|| root.join(default_set))
    } else {
        root.join(default_set)
    };

    println!("═══ Atlas Stage 2 Shadow 对拍 ═══");
    println!("图集: {:?}\n", set_path);

    let entries = load_set(&set_path, &root);
    println!("共 {} 张图\n", entries.len());

    let mut tested = 0;
    let mut skipped = 0;
    let mut zero_diff = 0;
    let mut all_p95: Vec<f64> = Vec::new();
    let mut all_max: Vec<f64> = Vec::new();
    let mut failures: Vec<String> = Vec::new();

    for (name, source) in &entries {
        let prepared = match try_parse(source) {
            Some(p) => p,
            None => {
                println!("  ⚠ {}: 解析失败，跳过", name);
                skipped += 1;
                continue;
            }
        };

        let diagram = prepared.inner();
        let has_groups = !diagram.groups.is_empty();
        let hierarchical = is_hierarchical(diagram);

        // 只测无组图 Hierarchical 系
        if has_groups || !hierarchical {
            println!("  - {}: 跳过（{}{}）", 
                name,
                if has_groups { "有组" } else { "" },
                if !hierarchical { "非Hierarchical" } else { "" }
            );
            skipped += 1;
            continue;
        }

        // 跑 shadow
        match run_shadow(name, diagram, prepared.layout_plan()) {
            Ok(run) => {
                tested += 1;
                let report = &run.report;
                all_p95.push(report.node_stats.p95);
                all_max.push(report.node_stats.max);

                if report.is_zero() {
                    zero_diff += 1;
                    println!("  ✓ {}: 零差异", name);
                } else {
                    println!(
                        "  △ {}: p50={:.2} p95={:.2} max={:.2}px, diffs={}",
                        name,
                        report.node_stats.p50,
                        report.node_stats.p95,
                        report.node_stats.max,
                        report.node_diffs.len()
                    );
                    if report.node_stats.p95 > 2.0 {
                        failures.push(format!(
                            "{}: p95={:.2}px > 2px",
                            name, report.node_stats.p95
                        ));
                    }
                }
            }
            Err(e) => {
                println!("  ✗ {}: 布局失败 {:?}", name, e);
                failures.push(format!("{}: 布局失败", name));
            }
        }
    }

    // 汇总
    println!("\n═══ 汇总 ═══");
    println!("测试: {} 张（跳过 {} 张）", tested, skipped);
    println!("零差异: {} 张", zero_diff);

    if !all_p95.is_empty() {
        all_p95.sort_by(|a, b| a.total_cmp(b));
        all_max.sort_by(|a, b| a.total_cmp(b));
        let overall_p95 = all_p95[(all_p95.len() as f64 * 0.95) as usize].min(*all_p95.last().unwrap());
        let overall_max = *all_max.last().unwrap();
        println!("整体 p95: {:.2}px", overall_p95);
        println!("整体 max: {:.2}px", overall_max);
    }

    if failures.is_empty() {
        println!("\n✓ 验收通过：所有无组图 p95 <= 2px");
    } else {
        println!("\n✗ 验收失败：");
        for f in &failures {
            println!("  - {}", f);
        }
        std::process::exit(1);
    }
}
