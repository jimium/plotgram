//! plotgram-eval CLI 入口
//!
//! 用法:
//!   # 评估单个文件（按图表类型自动选择算法）
//!   plotgram-eval eval flowchart.pgm
//!
//!   # 批量评估目录
//!   plotgram-eval batch showcase/
//!
//!   # 评估指定算法
//!   plotgram-eval eval flowchart.pgm -a sugiyama
//!
//!   # 布局+路由组合评估
//!   plotgram-eval eval flowchart.pgm --combinations
//!
//!   # 与历史基线对比
//!   plotgram-eval batch showcase/ --baseline .eval-history/baseline.json

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use plotgram_core::types::DiagramType;
use plotgram_eval::engine::presets;
use plotgram_eval::engine::EvalEngine;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }

    match args[1].as_str() {
        "eval" => cmd_eval(&args[2..]),
        "batch" => cmd_batch(&args[2..]),
        "algo" => cmd_algo(&args[2..]),
        "diff" => cmd_diff(&args[2..]),
        "baseline" => cmd_baseline(&args[2..]),
        "baseline-check" => cmd_baseline_check(&args[2..]),
        "-h" | "--help" => print_usage(),
        "-V" | "--version" => {
            println!("plotgram-eval {}", env!("CARGO_PKG_VERSION"));
        }
        _ => {
            eprintln!("未知命令: {}", args[1]);
            print_usage();
            std::process::exit(1);
        }
    }
}

fn print_usage() {
    eprintln!(
        r#"plotgram-eval — Plotgram 布局与边路由算法评估框架

用法:
  plotgram-eval eval <file.pgm> [选项]         评估单个文件
  plotgram-eval batch <目录> [选项]            批量评估目录下所有 .pgm 文件
  plotgram-eval algo <算法名> <目录> [选项]    算法维度评估
  plotgram-eval diff <baseline.json> <current.json>  对比两次评估结果
  plotgram-eval baseline <目录> [-o eval-data/showcase-baseline.json]  生成 showcase 质量基线
  plotgram-eval baseline-check <目录> [--baseline <文件>]  与基线对比检测回归

选项:
  -c, --compare <模式>     对比模式: auto(默认) | routing | layout | full | combinations
  -a, --algo <算法名>      指定单个算法评估
  -f, --format <格式>      输出格式: markdown(默认) | json
  -o, --output <文件>      输出到文件（默认 stdout）
  -w, --weights <类型>     权重模式: default | per-type(默认)
  --baseline <文件>        基线 JSON 文件（用于回归检测）
  --save-history           保存评估结果到历史目录
  -h, --help               显示帮助

对比模式:
  auto          按图表类型自动选择适用的算法（默认）
  routing       对比所有边路由算法
  layout        对比常用布局算法
  full          对比所有布局算法
  combinations  穷举布局+路由组合评估

算法维度评估 (algo):
  指定算法名，自动在所有适用图类型上评估，输出排名和最差案例
  例: plotgram-eval algo sugiyama showcase/

示例:
  plotgram-eval eval showcase/flowchart/s.decision-loop.pgm
  plotgram-eval eval showcase/flowchart/s.decision-loop.pgm -a sugiyama
  plotgram-eval eval showcase/flowchart/s.decision-loop.pgm -c combinations
  plotgram-eval batch showcase/ -o report.md
  plotgram-eval batch showcase/ -f json -o report.json
  plotgram-eval batch showcase/ --baseline baseline.json
  plotgram-eval algo sugiyama showcase/
  plotgram-eval diff baseline.json current.json
"#
    );
}

#[derive(Debug)]
struct CliArgs {
    compare: Option<String>,
    algo: Option<String>,
    format: Option<String>,
    output: Option<String>,
    weights: Option<String>,
    baseline: Option<String>,
    save_history: bool,
}

fn parse_cli_args(args: &[String]) -> CliArgs {
    let mut result = CliArgs {
        compare: None,
        algo: None,
        format: None,
        output: None,
        weights: None,
        baseline: None,
        save_history: false,
    };

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-c" | "--compare" => {
                i += 1;
                if i < args.len() {
                    result.compare = Some(args[i].clone());
                }
            }
            "-a" | "--algo" => {
                i += 1;
                if i < args.len() {
                    result.algo = Some(args[i].clone());
                }
            }
            "-f" | "--format" => {
                i += 1;
                if i < args.len() {
                    result.format = Some(args[i].clone());
                }
            }
            "-o" | "--output" => {
                i += 1;
                if i < args.len() {
                    result.output = Some(args[i].clone());
                }
            }
            "-w" | "--weights" => {
                i += 1;
                if i < args.len() {
                    result.weights = Some(args[i].clone());
                }
            }
            "--baseline" => {
                i += 1;
                if i < args.len() {
                    result.baseline = Some(args[i].clone());
                }
            }
            "--save-history" => {
                result.save_history = true;
            }
            _ => {}
        }
        i += 1;
    }

    result
}

fn make_engine(weights_mode: &str, diagram_type: Option<&DiagramType>) -> EvalEngine {
    match weights_mode {
        "per-type" => {
            if let Some(dt) = diagram_type {
                EvalEngine::with_weights_for_type(dt)
            } else {
                EvalEngine::new()
            }
        }
        "default" => EvalEngine::new(),
        _ => EvalEngine::new(),
    }
}

// ═══════════════════════════════════════════════════════════
//  eval 命令
// ═══════════════════════════════════════════════════════════

fn cmd_eval(args: &[String]) {
    if args.is_empty() {
        eprintln!("错误: 请指定 .pgm 文件路径");
        std::process::exit(1);
    }

    let file_path = &args[0];
    let cli = parse_cli_args(&args[1..]);

    let source = fs::read_to_string(file_path).unwrap_or_else(|e| {
        eprintln!("错误: 无法读取文件 '{}': {}", file_path, e);
        std::process::exit(1);
    });

    let diagram = parse_diagram(&source);
    let raw_stem = Path::new(file_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let name = strip_complexity_prefix(raw_stem);

    let weights_mode = cli.weights.as_deref().unwrap_or("per-type");
    let engine = make_engine(weights_mode, Some(&diagram.diagram_type));

    let mode = cli.compare.as_deref().unwrap_or("auto");
    let fmt = cli.format.as_deref().unwrap_or("markdown");

    let mut report = plotgram_eval::EvalReport::new(&format!("评估报告: {}", name));

    match mode {
        "auto" => {
            let dtype = &diagram.diagram_type;
            let type_name = dtype.display_name();

            // 布局对比
            let layout_configs = presets::layout_algos_for_type(dtype);
            if layout_configs.len() > 1 {
                let comp = engine.compare(
                    &format!("{} [{} - 布局对比]", name, type_name),
                    &diagram,
                    &layout_configs,
                );
                report.add_comparison(comp);
            }

            // 路由对比
            let routing_configs = presets::routing_algos_for_type(dtype);
            if routing_configs.len() > 1 {
                let comp = engine.compare(
                    &format!("{} [{} - 路由对比]", name, type_name),
                    &diagram,
                    &routing_configs,
                );
                report.add_comparison(comp);
            }

            // 专用算法
            if layout_configs.len() == 1 && routing_configs.len() == 1 {
                let comp = engine.compare(
                    &format!("{} [{} - 专用算法]", name, type_name),
                    &diagram,
                    &layout_configs,
                );
                report.add_comparison(comp);
            }
        }
        "combinations" => {
            let combo = engine.evaluate_combinations(name, &diagram);
            report.add_combination(combo);
        }
        _ => {
            let configs = get_configs(mode);
            let comp = engine.compare(name, &diagram, &configs);
            report.add_comparison(comp);
        }
    }

    // 指定算法的单项评估
    if let Some(ref algo_name) = cli.algo {
        let config = if algo_name.contains('+') {
            let parts: Vec<&str> = algo_name.splitn(2, '+').collect();
            presets::set_layout_and_routing(parts[0], parts[1])
        } else {
            presets::set_layout_algo(algo_name)
        };
        let result = engine.evaluate_with_dsl(&diagram, &config, Some(&source));
        if result.timed_out {
            eprintln!(
                "⚠ 超时！算法 '{}' 在 '{}' 上超过 {}s 未完成",
                result.algorithm,
                name,
                engine.timeout().as_secs()
            );
            if let Some(ref dsl) = result.timeout_dsl {
                // 保存超时 DSL 到文件
                let timeout_dir = std::path::Path::new("target/eval-timeouts");
                let _ = std::fs::create_dir_all(timeout_dir);
                let timeout_file = timeout_dir.join(format!(
                    "{}_{}.pgm",
                    name.replace('/', "_"),
                    algo_name.replace('+', "_")
                ));
                let _ = std::fs::write(&timeout_file, dsl);
                eprintln!("  超时 DSL 已保存: {}", timeout_file.display());
            }
        } else {
            eprintln!(
                "算法: {} | 评分: {:.1} ({}) | 耗时: {}μs",
                result.algorithm, result.score, result.quality_grade, result.elapsed_us
            );
        }
    }

    // 基线对比
    if let Some(ref baseline_path) = cli.baseline {
        append_baseline_diff(&mut report, baseline_path, &engine);
    }

    let content = format_report(&report, fmt);
    output_content(&content, cli.output.as_deref());

    // 保存历史
    if cli.save_history {
        save_to_history(&report, name);
    }
}

// ═══════════════════════════════════════════════════════════
//  batch 命令
// ═══════════════════════════════════════════════════════════

fn cmd_batch(args: &[String]) {
    if args.is_empty() {
        eprintln!("错误: 请指定目录路径");
        std::process::exit(1);
    }

    let dir = &args[0];
    let cli = parse_cli_args(&args[1..]);

    let dir_path = Path::new(dir);
    if !dir_path.is_dir() {
        eprintln!("错误: '{}' 不是目录", dir);
        std::process::exit(1);
    }

    let mut dfy_files: Vec<PathBuf> = Vec::new();
    collect_dfy_files(dir_path, &mut dfy_files);
    dfy_files.sort();

    if dfy_files.is_empty() {
        eprintln!("未找到 .pgm 文件");
        std::process::exit(1);
    }

    let mode = cli.compare.as_deref().unwrap_or("auto");
    let fmt = cli.format.as_deref().unwrap_or("markdown");
    let weights_mode = cli.weights.as_deref().unwrap_or("per-type");

    // 收集所有图（同时保存 DSL 源码用于超时记录）
    let mut diagrams: Vec<(String, plotgram_core::ast::Diagram, String)> = Vec::new();
    let total = dfy_files.len();
    for (i, path) in dfy_files.iter().enumerate() {
        let rel = path.strip_prefix(dir_path).unwrap_or(path);
        eprintln!("[{}/{}] 解析 {}", i + 1, total, rel.display());

        let source = fs::read_to_string(path).unwrap_or_else(|e| {
            eprintln!("  跳过（读取失败）: {}", e);
            return String::new();
        });

        if source.is_empty() {
            continue;
        }

        let diagram = match try_parse_diagram(&source) {
            Some(d) => d,
            None => {
                eprintln!("  跳过（解析失败）");
                continue;
            }
        };

        diagrams.push((clean_rel_path(&rel), diagram, source));
    }

    // 运行评估
    let content = run_batch_eval(&diagrams, mode, fmt, weights_mode);

    output_content(&content, cli.output.as_deref());

    // 保存历史
    if cli.save_history {
        if let Ok(report) = serde_json::from_str::<plotgram_eval::EvalReport>(&content) {
            save_to_history(&report, "batch");
        }
    }
}

/// 运行批量评估（按图表类型分组）
fn run_batch_eval(
    diagrams: &[(String, plotgram_core::ast::Diagram, String)],
    mode: &str,
    fmt: &str,
    weights_mode: &str,
) -> String {
    match mode {
        "auto" => {
            let mut report = plotgram_eval::EvalReport::new("Plotgram 布局与路由评估报告");

            // 按图表类型分组
            let mut by_type: HashMap<DiagramType, Vec<(String, plotgram_core::ast::Diagram, String)>> =
                HashMap::new();
            for (name, diagram, source) in diagrams {
                by_type
                    .entry(diagram.diagram_type.clone())
                    .or_default()
                    .push((name.clone(), diagram.clone(), source.clone()));
            }

            let type_order = [
                DiagramType::Flowchart,
                DiagramType::Architecture,
                DiagramType::State,
                DiagramType::Er,
                DiagramType::Sequence,
                DiagramType::Mindmap,
            ];

            for dtype in &type_order {
                let group = match by_type.get(dtype) {
                    Some(g) => g,
                    None => continue,
                };

                let type_name = dtype.display_name();
                let engine = make_engine(weights_mode, Some(dtype));

                let layout_configs = presets::layout_algos_for_type(dtype);
                let routing_configs = presets::routing_algos_for_type(dtype);

                // 布局对比
                if layout_configs.len() > 1 {
                    eprintln!("\n── {} 布局对比 ──", type_name);
                    for (i, (name, diagram, source)) in group.iter().enumerate() {
                        eprintln!("[{}/{}] {} (布局)", i + 1, group.len(), name);
                        let comp = engine.compare_with_dsl(
                            &format!("{} [{} - 布局]", name, type_name),
                            diagram,
                            &layout_configs,
                            Some(source),
                        );
                        // 报告超时
                        for r in &comp.results {
                            if r.timed_out {
                                eprintln!("  ⚠ 超时: {} 在 {}", r.algorithm, name);
                                save_timeout_dsl(name, &r.algorithm, source);
                            }
                        }
                        report.add_comparison(comp);
                    }
                }

                // 路由对比
                if routing_configs.len() > 1 {
                    eprintln!("\n── {} 路由对比 ──", type_name);
                    for (i, (name, diagram, source)) in group.iter().enumerate() {
                        eprintln!("[{}/{}] {} (路由)", i + 1, group.len(), name);
                        let comp = engine.compare_with_dsl(
                            &format!("{} [{} - 路由]", name, type_name),
                            diagram,
                            &routing_configs,
                            Some(source),
                        );
                        for r in &comp.results {
                            if r.timed_out {
                                eprintln!("  ⚠ 超时: {} 在 {}", r.algorithm, name);
                                save_timeout_dsl(name, &r.algorithm, source);
                            }
                        }
                        report.add_comparison(comp);
                    }
                }

                // 专用算法
                if layout_configs.len() == 1 && routing_configs.len() == 1 {
                    eprintln!("\n── {} 专用算法 ──", type_name);
                    for (i, (name, diagram, source)) in group.iter().enumerate() {
                        eprintln!("[{}/{}] {}", i + 1, group.len(), name);
                        let comp = engine.compare_with_dsl(
                            &format!("{} [{}]", name, type_name),
                            diagram,
                            &layout_configs,
                            Some(source),
                        );
                        for r in &comp.results {
                            if r.timed_out {
                                eprintln!("  ⚠ 超时: {} 在 {}", r.algorithm, name);
                                save_timeout_dsl(name, &r.algorithm, source);
                            }
                        }
                        report.add_comparison(comp);
                    }
                }
            }

            format_report(&report, fmt)
        }
        "combinations" => {
            let mut report = plotgram_eval::EvalReport::new("Plotgram 布局+路由组合评估报告");

            for (i, (name, diagram, source)) in diagrams.iter().enumerate() {
                eprintln!("[{}/{}] {}", i + 1, diagrams.len(), name);
                let engine = make_engine(weights_mode, Some(&diagram.diagram_type));
                let combo = engine.evaluate_combinations(name, diagram);
                // 超时检查
                for r in &combo.results {
                    if r.timed_out {
                        eprintln!("  ⚠ 超时: {} 在 {}", r.algorithm, name);
                        save_timeout_dsl(name, &r.algorithm, source);
                    }
                }
                report.add_combination(combo);
            }

            format_report(&report, fmt)
        }
        _ => {
            let configs = get_configs(mode);
            let engine = make_engine(weights_mode, None);
            let mut report = plotgram_eval::EvalReport::new("Plotgram 评估报告");
            for (i, (name, diagram, source)) in diagrams.iter().enumerate() {
                eprintln!("[{}/{}] {}", i + 1, diagrams.len(), name);
                let comp = engine.compare_with_dsl(name, diagram, &configs, Some(source));
                for r in &comp.results {
                    if r.timed_out {
                        eprintln!("  ⚠ 超时: {} 在 {}", r.algorithm, name);
                        save_timeout_dsl(name, &r.algorithm, source);
                    }
                }
                report.add_comparison(comp);
            }
            format_report(&report, fmt)
        }
    }
}

// ═══════════════════════════════════════════════════════════
//  algo 命令 — 算法维度评估
// ═══════════════════════════════════════════════════════════

fn cmd_algo(args: &[String]) {
    if args.len() < 2 {
        eprintln!("用法: plotgram-eval algo <算法名> <目录> [选项]");
        std::process::exit(1);
    }

    let algo_name = &args[0];
    let dir = &args[1];
    let cli = parse_cli_args(&args[2..]);

    let dir_path = Path::new(dir);
    if !dir_path.is_dir() {
        eprintln!("错误: '{}' 不是目录", dir);
        std::process::exit(1);
    }

    // 查找算法适用的图类型
    let applicable_types = plotgram_core::layout::diagram_types_for_layout(algo_name);
    if applicable_types.is_empty() {
        eprintln!("算法 '{}' 没有适用的图类型", algo_name);
        std::process::exit(1);
    }

    eprintln!(
        "算法 '{}' 适用于: {}",
        algo_name,
        applicable_types
            .iter()
            .map(|dt| dt.display_name())
            .collect::<Vec<_>>()
            .join(", ")
    );

    // 收集 .pgm 文件
    let mut dfy_files: Vec<PathBuf> = Vec::new();
    collect_dfy_files(dir_path, &mut dfy_files);
    dfy_files.sort();

    let engine = EvalEngine::new();
    let report = plotgram_eval::EvalReport::new(&format!("算法评估: {}", algo_name));
    let mut all_results: Vec<plotgram_eval::EvalResult> = Vec::new();

    for path in &dfy_files {
        let source = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let diagram = match try_parse_diagram(&source) {
            Some(d) => d,
            None => continue,
        };

        // 只评估适用图类型
        if !applicable_types.contains(&diagram.diagram_type) {
            continue;
        }

        let rel = path.strip_prefix(dir_path).unwrap_or(path);
        let name = clean_rel_path(rel);

        let config = presets::set_layout_algo(algo_name);
        let result = engine.evaluate(&diagram, &config);
        eprintln!("{}: {:.1} ({})", name, result.score, result.quality_grade);
        all_results.push(result);
    }

    if all_results.is_empty() {
        eprintln!("未找到适用图文件");
        return;
    }

    // 汇总统计
    let avg_score = all_results.iter().map(|r| r.score).sum::<f64>() / all_results.len() as f64;
    let max_score = all_results.iter().map(|r| r.score).fold(f64::NEG_INFINITY, f64::max);
    let min_score = all_results.iter().map(|r| r.score).fold(f64::INFINITY, f64::min);

    eprintln!("\n── 汇总 ──");
    eprintln!("样本数: {}", all_results.len());
    eprintln!("平均评分: {:.1}", avg_score);
    eprintln!("最高评分: {:.1}", max_score);
    eprintln!("最低评分: {:.1}", min_score);

    // 最差案例
    let worst = engine.find_worst_cases(&all_results, algo_name, 5);
    if !worst.is_empty() {
        eprintln!("\n── 最差案例 (Top 5) ──");
        for r in worst {
            eprintln!(
                "  {:.1} ({}) — {} 节点 / {} 边 / {}",
                r.score,
                r.quality_grade,
                r.graph_profile.node_count,
                r.graph_profile.edge_count,
                r.graph_profile.topology_summary()
            );
        }
    }

    let fmt = cli.format.as_deref().unwrap_or("markdown");
    let content = format_report(&report, fmt);
    output_content(&content, cli.output.as_deref());
}

// ═══════════════════════════════════════════════════════════
//  diff 命令 — 对比两次评估结果
// ═══════════════════════════════════════════════════════════

fn cmd_diff(args: &[String]) {
    if args.len() < 2 {
        eprintln!("用法: plotgram-eval diff <baseline.json> <current.json>");
        std::process::exit(1);
    }

    let baseline_path = &args[0];
    let current_path = &args[1];
    let cli = parse_cli_args(&args[2..]);

    let baseline_json = fs::read_to_string(baseline_path).unwrap_or_else(|e| {
        eprintln!("错误: 无法读取基线文件 '{}': {}", baseline_path, e);
        std::process::exit(1);
    });

    let current_json = fs::read_to_string(current_path).unwrap_or_else(|e| {
        eprintln!("错误: 无法读取当前文件 '{}': {}", current_path, e);
        std::process::exit(1);
    });

    let baseline: plotgram_eval::EvalReport =
        serde_json::from_str(&baseline_json).unwrap_or_else(|e| {
            eprintln!("错误: 解析基线文件失败: {}", e);
            std::process::exit(1);
        });

    let current: plotgram_eval::EvalReport =
        serde_json::from_str(&current_json).unwrap_or_else(|e| {
            eprintln!("错误: 解析当前文件失败: {}", e);
            std::process::exit(1);
        });

    let engine = EvalEngine::new();
    let mut diff_report = plotgram_eval::EvalReport::new("差异对比报告");

    // 对比相同图名的结果
    for curr_comp in &current.comparisons {
        for base_comp in &baseline.comparisons {
            if curr_comp.diagram_name != base_comp.diagram_name {
                continue;
            }

            for curr_result in &curr_comp.results {
                for base_result in &base_comp.results {
                    if curr_result.algorithm == base_result.algorithm {
                        let mut diff = engine.diff(base_result, curr_result);
                        diff.diagram_name = curr_comp.diagram_name.clone();
                        diff_report.add_diff(diff);
                    }
                }
            }
        }
    }

    let fmt = cli.format.as_deref().unwrap_or("markdown");
    let content = format_report(&diff_report, fmt);
    output_content(&content, cli.output.as_deref());
}

// ═══════════════════════════════════════════════════════════
//  baseline 命令 — showcase 质量基线
// ═══════════════════════════════════════════════════════════

fn cmd_baseline(args: &[String]) {
    if args.is_empty() {
        eprintln!("用法: plotgram-eval baseline <showcase目录> [-o eval-data/showcase-baseline.json]");
        std::process::exit(1);
    }

    let showcase_dir = Path::new(&args[0]);
    let mut output = PathBuf::from("eval-data/showcase-baseline.json");
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                if i + 1 >= args.len() {
                    eprintln!("错误: -o 需要参数");
                    std::process::exit(1);
                }
                output = PathBuf::from(&args[i + 1]);
                i += 2;
            }
            other => {
                eprintln!("未知选项: {other}");
                std::process::exit(1);
            }
        }
    }

    eprintln!("生成 showcase 质量基线: {}", showcase_dir.display());
    let baseline = plotgram_eval::generate_baseline(showcase_dir).unwrap_or_else(|e| {
        eprintln!("生成失败: {e}");
        std::process::exit(1);
    });

    baseline.save(&output).unwrap_or_else(|e| {
        eprintln!("保存失败: {e}");
        std::process::exit(1);
    });

    eprintln!(
        "✓ 已写入 {} ({} 个文件)",
        output.display(),
        baseline.entries.len()
    );
}

fn cmd_baseline_check(args: &[String]) {
    if args.is_empty() {
        eprintln!(
            "用法: plotgram-eval baseline-check <showcase目录> [--baseline eval-data/showcase-baseline.json] [-f text|json] [-o 文件]"
        );
        std::process::exit(1);
    }

    let showcase_dir = Path::new(&args[0]);
    let mut baseline_path = PathBuf::from("eval-data/showcase-baseline.json");
    let mut format = "text".to_string();
    let mut output: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--baseline" | "-b" => {
                if i + 1 >= args.len() {
                    eprintln!("错误: --baseline 需要参数");
                    std::process::exit(1);
                }
                baseline_path = PathBuf::from(&args[i + 1]);
                i += 2;
            }
            "-f" | "--format" => {
                if i + 1 >= args.len() {
                    eprintln!("错误: -f 需要参数");
                    std::process::exit(1);
                }
                format = args[i + 1].clone();
                i += 2;
            }
            "-o" | "--output" => {
                if i + 1 >= args.len() {
                    eprintln!("错误: -o 需要参数");
                    std::process::exit(1);
                }
                output = Some(args[i + 1].clone());
                i += 2;
            }
            other => {
                eprintln!("未知选项: {other}");
                std::process::exit(1);
            }
        }
    }

    eprintln!("对比 showcase 与基线");
    eprintln!("  showcase: {}", showcase_dir.display());
    eprintln!("  baseline: {}", baseline_path.display());

    let baseline = plotgram_eval::ShowcaseBaseline::load(&baseline_path).unwrap_or_else(|e| {
        eprintln!("加载基线失败: {e}");
        eprintln!("提示: 先运行 `plotgram-eval baseline showcase/` 生成基线");
        std::process::exit(1);
    });

    let mut report =
        plotgram_eval::compare_with_baseline(showcase_dir, &baseline).unwrap_or_else(|e| {
            eprintln!("对比失败: {e}");
            std::process::exit(1);
        });
    report.baseline_path = baseline_path.to_string_lossy().to_string();

    let content = if format == "json" {
        serde_json::to_string_pretty(&report).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    } else {
        report.to_markdown()
    };

    output_content(&content, output.as_deref());

    if report.has_regressions() {
        eprintln!("\n✗ 检测到 {} 项指标回归", report.regressions.len());
        std::process::exit(1);
    } else {
        eprintln!("\n✓ 未检测到指标回归");
    }
}

// ═══════════════════════════════════════════════════════════
//  辅助函数
// ═══════════════════════════════════════════════════════════

fn format_report(report: &plotgram_eval::EvalReport, fmt: &str) -> String {
    match fmt {
        "json" => report.to_json(),
        _ => report.to_markdown(),
    }
}

fn output_content(content: &str, output_path: Option<&str>) {
    match output_path {
        Some(path) => {
            fs::write(path, content).unwrap_or_else(|e| {
                eprintln!("错误: 无法写入文件 '{}': {}", path, e);
                std::process::exit(1);
            });
            eprintln!("报告已写入: {}", path);
        }
        None => println!("{}", content),
    }
}

fn append_baseline_diff(
    report: &mut plotgram_eval::EvalReport,
    baseline_path: &str,
    engine: &EvalEngine,
) {
    let baseline_json = match fs::read_to_string(baseline_path) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("警告: 无法读取基线文件 '{}': {}", baseline_path, e);
            return;
        }
    };

    let baseline: plotgram_eval::EvalReport = match serde_json::from_str(&baseline_json) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("警告: 解析基线文件失败: {}", e);
            return;
        }
    };

    let mut diffs = Vec::new();

    for curr_comp in &report.comparisons {
        for base_comp in &baseline.comparisons {
            if curr_comp.diagram_name != base_comp.diagram_name {
                continue;
            }

            for curr_result in &curr_comp.results {
                for base_result in &base_comp.results {
                    if curr_result.algorithm == base_result.algorithm {
                        let mut diff = engine.diff(base_result, curr_result);
                        diff.diagram_name = curr_comp.diagram_name.clone();
                        diffs.push(diff);
                    }
                }
            }
        }
    }

    for diff in diffs {
        report.add_diff(diff);
    }
}

fn save_to_history(report: &plotgram_eval::EvalReport, label: &str) {
    let history_dir = Path::new(".eval-history");
    if let Ok(store) = plotgram_eval::history::HistoryStore::new(history_dir) {
        match store.save(report, label) {
            Ok(path) => eprintln!("历史记录已保存: {}", path.display()),
            Err(e) => eprintln!("警告: 保存历史记录失败: {}", e),
        }
    }
}

/// 保存超时样本的 DSL 到文件
fn save_timeout_dsl(sample_name: &str, algorithm: &str, dsl_source: &str) {
    let timeout_dir = Path::new("target/eval-timeouts");
    let _ = std::fs::create_dir_all(timeout_dir);
    let filename = format!(
        "{}_{}.pgm",
        sample_name.replace('/', "_").replace(' ', "_"),
        algorithm.replace('+', "_")
    );
    let timeout_file = timeout_dir.join(&filename);
    match std::fs::write(&timeout_file, dsl_source) {
        Ok(_) => eprintln!("  超时 DSL 已保存: {}", timeout_file.display()),
        Err(e) => eprintln!("  保存超时 DSL 失败: {}", e),
    }
}

fn get_configs(mode: &str) -> Vec<plotgram_eval::AlgorithmConfig> {
    match mode {
        "layout" => presets::layout_comparison(),
        "all" => presets::full_layout_comparison(),
        "full" => presets::sugiyama_routing_comparison(),
        _ => presets::routing_comparison(),
    }
}

fn strip_complexity_prefix(stem: &str) -> &str {
    match stem.split_once('.') {
        Some((prefix, rest)) if ["c", "n", "s"].contains(&prefix) && !rest.is_empty() => rest,
        _ => stem,
    }
}

fn clean_rel_path(rel: &Path) -> String {
    let dir = rel.parent().map(|p| p.to_string_lossy().to_string());
    let stem = rel
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let clean_stem = strip_complexity_prefix(stem);
    match dir {
        Some(d) if !d.is_empty() => format!("{}/{}", d, clean_stem),
        _ => clean_stem.to_string(),
    }
}

fn collect_dfy_files(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_dfy_files(&path, files);
            } else if path.extension().is_some_and(|e| e == "dfy") {
                files.push(path);
            }
        }
    }
}

fn parse_diagram(source: &str) -> plotgram_core::ast::Diagram {
    try_parse_diagram(source).unwrap_or_else(|| {
        eprintln!("错误: 解析 .pgm 文件失败");
        std::process::exit(1);
    })
        }

fn try_parse_diagram(source: &str) -> Option<plotgram_core::ast::Diagram> {
    let raw = plotgram_core::pipeline::parse(source).ok()?;
    let output =
        plotgram_core::pipeline::prepare(raw, &plotgram_core::prepare::StyleRequest::default())
            .ok()?;
    Some(output.diagram.into_inner())
}
