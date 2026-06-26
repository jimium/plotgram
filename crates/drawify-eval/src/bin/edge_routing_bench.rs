//! 边路由性能基准测试
//!
//! 专注于边路由算法的性能与质量评估：
//! - 输入：一组 showcase `.dfy` 文件
//! - 输出：每个 router 的耗时 + 边交叉数 + 边穿节点数 + 标签重叠数（JSON）
//!
//! 与 `bench` 的区别：`bench` 通过 eval engine 做全量算法对比（布局+路由），
//! 本工具只对比边路由算法，固定布局算法，输出更聚焦的路由指标。
//!
//! 用法:
//!   cargo run -p drawify-eval --bin edge_routing_bench
//!   cargo run -p drawify-eval --bin edge_routing_bench -- --output edge_bench.json
//!   cargo run -p drawify-eval --bin edge_routing_bench -- --showcase /path/to/showcase

use drawify_core::ast::{DiagramAttribute, Diagram, AttributeValue, TextValue};
use drawify_core::layout;
use drawify_core::layout::edge::common::label_avoidance::{aabb_overlap, label_bbox};
use drawify_eval::metrics::LayoutMetrics;
use serde::Serialize;
use std::fs;
use std::path::Path;
use std::time::Instant;

#[derive(Serialize)]
struct BenchReport {
    diagrams: Vec<DiagramBench>,
}

#[derive(Serialize)]
struct DiagramBench {
    name: String,
    diagram_type: String,
    node_count: usize,
    edge_count: usize,
    routers: Vec<RouterResult>,
}

#[derive(Serialize)]
struct RouterResult {
    router: String,
    elapsed_us: u64,
    edge_crossings: usize,
    edge_node_crossings: usize,
    label_overlaps: usize,
    /// P2-1: orthogonal 路由 debug 统计（仅 orthogonal 路由有值）
    #[serde(skip_serializing_if = "Option::is_none")]
    orthogonal_debug: Option<OrthoDebugReport>,
    /// P2-1: refine debug 统计（启用 refine 时有值）
    #[serde(skip_serializing_if = "Option::is_none")]
    refine_debug: Option<RefineDebugReport>,
    error: Option<String>,
}

/// P2-1: orthogonal 路由 debug 报告（bench 输出用）
#[derive(Serialize)]
struct OrthoDebugReport {
    degraded_count: usize,
    hard_filter_reject_count: usize,
    total_candidates: usize,
    edge_count: usize,
    hard_filter_reject_rate: f64,
    avg_candidates_per_edge: f64,
}

/// P2-1: refine debug 报告（bench 输出用）
#[derive(Serialize)]
struct RefineDebugReport {
    push_count: usize,
    momentum_reversals: usize,
    passes_executed: usize,
}

fn try_parse_diagram(source: &str) -> Option<Diagram> {
    let raw = drawify_core::pipeline::parse(source).ok()?;
    let output =
        drawify_core::pipeline::prepare(raw, &drawify_core::prepare::StyleRequest::default())
            .ok()?;
    Some(output.diagram.into_inner())
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

fn load_showcase(dir: &Path) -> Vec<(String, Diagram)> {
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

/// 设置 diagram 的 edge_routing 属性
fn set_edge_routing(diagram: &mut Diagram, routing: &str) {
    diagram.attributes.retain(|a| a.key != "edge_routing");
    diagram.attributes.push(DiagramAttribute {
        key: "edge_routing".to_string(),
        value: AttributeValue::String(TextValue::unquoted(routing.to_string())),
        span: drawify_core::ast::Span::dummy(),
    });
}

/// 统计标签重叠数（两两标签 bbox 相交）
fn count_label_overlaps(diagram: &Diagram, result: &layout::LayoutResult) -> usize {
    let edges = &result.edges;
    let relations = &diagram.relations;
    let mut count = 0;
    for i in 0..edges.len() {
        let Some(label_i) = relations.get(i).and_then(|r| r.label.as_ref()) else {
            continue;
        };
        let bbox_i = label_bbox(&edges[i], label_i);
        for j in (i + 1)..edges.len() {
            let Some(label_j) = relations.get(j).and_then(|r| r.label.as_ref()) else {
                continue;
            };
            let bbox_j = label_bbox(&edges[j], label_j);
            if aabb_overlap(&bbox_i, &bbox_j).is_some() {
                count += 1;
            }
        }
    }
    count
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
            let manifest_dir =
                std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
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

    let mut report = BenchReport {
        diagrams: Vec::new(),
    };

    let total_start = Instant::now();

    for (name, diagram) in &diagrams {
        let diagram_type = &diagram.diagram_type;
        let routing_names = layout::applicable_routings_for_type(diagram_type);

        let mut router_results = Vec::new();

        for routing in &routing_names {
            let mut diag = diagram.clone();
            set_edge_routing(&mut diag, routing);

            let start = Instant::now();
            let result = layout::compute_layout(&diag);
            let elapsed = start.elapsed();

            match result {
                Ok(layout) => {
                    let metrics = LayoutMetrics::compute(&diag, &layout);
                    let label_overlaps = count_label_overlaps(&diag, &layout);

                    // P2-1: 提取 debug 统计
                    let orthogonal_debug = layout.hints.orthogonal_debug.as_ref().map(|s| OrthoDebugReport {
                        degraded_count: s.degraded_count,
                        hard_filter_reject_count: s.hard_filter_reject_count,
                        total_candidates: s.total_candidates,
                        edge_count: s.edge_count,
                        hard_filter_reject_rate: s.hard_filter_reject_rate(),
                        avg_candidates_per_edge: s.avg_candidates_per_edge(),
                    });
                    let refine_debug = layout.hints.refine_debug.as_ref().map(|s| RefineDebugReport {
                        push_count: s.push_count,
                        momentum_reversals: s.momentum_reversals,
                        passes_executed: s.passes_executed,
                    });

                    router_results.push(RouterResult {
                        router: routing.to_string(),
                        elapsed_us: elapsed.as_micros() as u64,
                        edge_crossings: metrics.edge_crossings,
                        edge_node_crossings: metrics.edge_node_crossings,
                        label_overlaps,
                        orthogonal_debug,
                        refine_debug,
                        error: None,
                    });
                }
                Err(e) => {
                    router_results.push(RouterResult {
                        router: routing.to_string(),
                        elapsed_us: elapsed.as_micros() as u64,
                        edge_crossings: 0,
                        edge_node_crossings: 0,
                        label_overlaps: 0,
                        orthogonal_debug: None,
                        refine_debug: None,
                        error: Some(e.to_string()),
                    });
                }
            }
        }

        let node_count = diagram.entities.len();
        let edge_count = diagram.relations.len();

        eprintln!(
            "  {} ({:?}): {} 节点, {} 边, {} 个 router",
            name,
            diagram_type,
            node_count,
            edge_count,
            router_results.len()
        );

        report.diagrams.push(DiagramBench {
            name: name.clone(),
            diagram_type: format!("{:?}", diagram_type),
            node_count,
            edge_count,
            routers: router_results,
        });
    }

    let total_elapsed = total_start.elapsed();
    eprintln!(
        "完成！{} 个图 · 总耗时 {:.2}s",
        report.diagrams.len(),
        total_elapsed.as_secs_f64()
    );

    let json = serde_json::to_string_pretty(&report).expect("序列化失败");

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
