//! 分阶段性能基准测试：分别测量解析、布局、路由各阶段的耗时。
//!
//! 用法:
//!   cargo run --release -p drawify-core --bin bench-phases -- showcase/architecture/c.k8s-tenant-isolation.dfy

use std::time::Instant;

use drawify_core::layout;
use drawify_core::pipeline;
use drawify_core::prepare::StyleRequest;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("用法: bench-phases <file.dfy> [runs]");
        std::process::exit(1);
    }

    let file_path = &args[1];
    let runs: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);

    let source = std::fs::read_to_string(file_path).unwrap_or_else(|e| {
        eprintln!("无法读取文件: {}", e);
        std::process::exit(1);
    });

    let style_req = StyleRequest::default();

    // ── 阶段 1: 解析 + prepare ──
    let t0 = Instant::now();
    let output = pipeline::parse_prepare(&source, &style_req);
    let parse_elapsed = t0.elapsed();

    let diagram = output.diagram.as_ref().expect("parse failed").inner().clone();
    let plan = output.diagram.as_ref().unwrap().layout_plan().clone();

    println!("═══════════════════════════════════════════");
    println!("文件: {}", file_path);
    println!("节点: {} | 边: {} | 分组: {}",
        diagram.entities.len(),
        diagram.relations.len(),
        diagram.groups.len());
    println!("═══════════════════════════════════════════");
    println!("解析+prepare 耗时: {:>8.2}ms", parse_elapsed.as_secs_f64() * 1000.0);

    // ── 预热 ──
    let _ = layout::compute_layout_with_plan(&diagram, &plan);

    // ── 阶段 2: 总耗时 (多轮) ──
    let mut total_times = Vec::new();
    for i in 0..runs {
        let start = Instant::now();
        let _ = layout::compute_layout_with_plan(&diagram, &plan);
        total_times.push(start.elapsed());
        eprintln!("  运行 {}/{}: {:.2}ms", i + 1, runs, total_times[i].as_secs_f64() * 1000.0);
    }

    total_times.sort();
    let total_median = total_times[runs / 2];
    let total_min = total_times[0];
    let total_max = total_times[runs - 1];

    println!("布局+路由总耗时 ({} 轮):", runs);
    println!("  中位数: {:>8.2}ms", total_median.as_secs_f64() * 1000.0);
    println!("  最小值: {:>8.2}ms", total_min.as_secs_f64() * 1000.0);
    println!("  最大值: {:>8.2}ms", total_max.as_secs_f64() * 1000.0);

    // ── 阶段 3: 获取布局结果并分析 ──
    let result = layout::compute_layout_with_plan(&diagram, &plan).unwrap();
    println!();
    println!("图规模:");
    println!("  节点数: {}", result.nodes.len());
    println!("  边数:   {}", result.edges.len());
    println!("  分组数: {}", result.groups.len());
    if let Some(ref ortho) = result.hints.orthogonal_debug {
        println!();
        println!("正交路由统计:");
        println!("  候选总数:     {}", ortho.total_candidates);
        println!("  硬过滤拒绝:   {}", ortho.hard_filter_reject_count);
        println!("  退化数:       {}", ortho.degraded_count);
        println!("  边总数:       {}", ortho.edge_count);
        println!("  完全重合段对: {}", ortho.edge_exact_overlap_pairs);
        println!("  间距不足段对: {}", ortho.edge_tight_spacing_pairs);
        if ortho.reroute_iterations > 0 {
            println!("  重路由轮次:   {}", ortho.reroute_iterations);
            println!("  重路由边数:   {}", ortho.rerouted_edges);
        }
        if ortho.nudge_iterations > 0 {
            println!("  Nudge轮次:    {}", ortho.nudge_iterations);
            println!("  Nudge段数:    {}", ortho.nudged_segments);
            println!("  Nudge失败:    {}", ortho.nudge_failed);
        }
    }
    if let Some(ref friendliness) = result.hints.friendliness_report {
        println!();
        println!("路由友好性: {:.2}", friendliness.score);
        println!("  拥堵分数:     {}", friendliness.congestion_score);
        println!("  长边分数:     {}", friendliness.long_edge_score);
        println!("  间隙充足度:   {}", friendliness.gap_adequacy_score);
        println!("  预测交叉:     {}", friendliness.predicted_crossings);
        println!("  端口冲突:     {}", friendliness.port_conflict_score);
    }

    println!();
    println!("═══════════════════════════════════════════");
    println!("基准测试完成");
}