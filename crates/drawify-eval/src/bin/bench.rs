//! 算法性能基准测试
//!
//! 扫描 showcase 目录的真实 .dfy 文件，分别评估布局算法和路由算法，
//! 输出 JSON 供可视化脚本使用。
//!
//! 布局和路由是两个独立维度，分开评估：
//! - 布局对比：固定路由为默认，只比布局算法
//! - 路由对比：固定布局为最佳，只比路由算法
//!
//! 用法:
//!   cargo run -p drawify-eval --bin bench
//!   cargo run -p drawify-eval --bin bench -- --output bench_result.json
//!   cargo run -p drawify-eval --bin bench -- --showcase /path/to/showcase

use drawify_core::layout;
use drawify_eval::engine::presets;
use drawify_eval::engine::EvalEngine;
use drawify_eval::report::EvalReport;
use std::fs;
use std::path::Path;
use std::time::Instant;

fn try_parse_diagram(source: &str) -> Option<drawify_core::ast::Diagram> {
    let raw = drawify_core::pipeline::parse(source).ok()?;
    let output =
        drawify_core::pipeline::prepare(raw, &drawify_core::prepare::StyleRequest::default())
            .ok()?;
    Some(output.diagram.into_inner())
}

fn load_showcase(dir: &Path) -> Vec<(String, drawify_core::ast::Diagram)> {
    let mut diagrams = Vec::new();

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
            Err(e) => {
                eprintln!("  跳过 {:?}: {}", path.file_name().unwrap(), e);
                continue;
            }
        };

        match try_parse_diagram(&source) {
            Some(diagram) => {
                let name = path
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .trim_end_matches(".dfy")
                    .to_string();
                diagrams.push((name, diagram));
            }
            None => {
                eprintln!("  跳过 {:?}: 解析失败", path.file_name().unwrap());
            }
        }
    }

    diagrams
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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let output_path = args
        .iter()
        .position(|a| a == "--output" || a == "-o")
        .and_then(|i| args.get(i + 1).cloned());

    let showcase_path = args
        .iter()
        .position(|a| a == "--showcase")
        .and_then(|i| args.get(i + 1).cloned())
        .unwrap_or_else(|| {
            let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
            format!("{}/../../../showcase", manifest_dir)
        });

    let showcase_dir = Path::new(&showcase_path);
    if !showcase_dir.exists() {
        eprintln!("✗ showcase 目录不存在: {}", showcase_path);
        eprintln!("  用 --showcase 指定路径");
        std::process::exit(1);
    }

    eprintln!("▶ 扫描 showcase 目录: {}", showcase_path);
    let diagrams = load_showcase(showcase_dir);
    eprintln!("  加载 {} 个图文件", diagrams.len());

    if diagrams.is_empty() {
        eprintln!("✗ 没有找到 .dfy 文件");
        std::process::exit(1);
    }

    let mut type_counts: std::collections::HashMap<drawify_core::types::DiagramType, usize> =
        std::collections::HashMap::new();
    for (_, d) in &diagrams {
        *type_counts.entry(d.diagram_type.clone()).or_insert(0) += 1;
    }
    for (dt, count) in &type_counts {
        eprintln!("    {:?}: {} 个", dt, count);
    }

    let engine = EvalEngine::new();
    let mut report = EvalReport::new("算法性能基准测试");

    let total_start = Instant::now();

    for (name, diagram) in &diagrams {
        let diagram_type = &diagram.diagram_type;
        let layout_names = layout::applicable_layouts_for_type(diagram_type);
        let routing_names = layout::applicable_routings_for_type(diagram_type);

        // ── 1. 布局算法对比（固定路由为默认）──
        let layout_configs: Vec<_> = layout_names
            .iter()
            .map(|n| presets::set_layout_algo(n))
            .collect();

        if !layout_configs.is_empty() {
            let comp = engine.compare(name, diagram, &layout_configs);
            report.add_comparison(comp);
        }

        // ── 2. 路由算法对比（固定布局为最佳）──
        // 先找出最佳布局，再用最佳布局对比路由
        if routing_names.len() > 1 {
            // 找出该图类型下最佳布局
            let best_layout = layout_names.first().copied().unwrap_or("sugiyama");
            let routing_configs: Vec<_> = routing_names
                .iter()
                .map(|r| presets::set_layout_and_routing(best_layout, r))
                .collect();

            if !routing_configs.is_empty() {
                let comp = engine.compare(
                    &format!("{} [routing]", name),
                    diagram,
                    &routing_configs,
                );
                report.add_comparison(comp);
            }
        }
    }

    let total_elapsed = total_start.elapsed();
    eprintln!(
        "完成！{} 个图 · 总耗时 {:.2}s",
        diagrams.len(),
        total_elapsed.as_secs_f64()
    );

    let json = report.to_json();

    match output_path {
        Some(path) => {
            fs::write(&path, &json).expect("写入输出文件失败");
            eprintln!("结果已写入 {}", path);
        }
        None => {
            println!("{}", json);
        }
    }
}
