//! Atlas Adapter 验证（诊断工具，**非 CI 门禁**）。
//!
//! 对图集里每张图：`Diagram` → `adapt_from_legacy` → `Plan`（validate）→
//! `ink_all` → `compare_ink_vs_legacy`，输出 Plan 统计 + Ink 拓扑一致率 + 缺口清单。
//!
//! 用法：
//!   cargo run -p plotgram-eval --bin atlas_adapter_check
//!   cargo run -p plotgram-eval --bin atlas_adapter_check -- --set benchmarks/sets/product-regression-set.txt
//!   cargo run -p plotgram-eval --bin atlas_adapter_check -- --output /tmp/adapter_check.md

use plotgram_core::ast::Diagram;
use plotgram_core::layout::atlas::adapter::{adapt_from_legacy, AdapterInput, ExpressivenessGap};
use plotgram_core::layout::atlas::ink::{compare_ink_vs_legacy, ink_all, InkContext};
use plotgram_core::layout::LayoutResult;
use plotgram_core::types::DiagramType;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// 单图验证结果。
struct DiagramCheck {
    name: String,
    dtype: DiagramType,
    /// Plan 构造是否成功。
    plan_ok: bool,
    /// Plan validate 是否通过。
    validate_ok: bool,
    /// 节点数。
    nodes: usize,
    /// channels 成功边数。
    channels_ok: usize,
    /// ports 映射边数。
    ports_count: usize,
    /// bundles 数。
    bundles: usize,
    /// Ink 拓扑一致率。
    ink_match_rate: f64,
    /// Ink 最大逐点偏差。
    ink_max_dev: f64,
    /// 表达力缺口。
    gaps: Vec<ExpressivenessGap>,
    /// 失败原因（若 Plan 构造失败）。
    error: Option<String>,
}

fn try_parse_and_layout(source: &str) -> Option<(Diagram, LayoutResult)> {
    let raw = plotgram_core::pipeline::parse(source).ok()?;
    let output =
        plotgram_core::pipeline::prepare(raw, &plotgram_core::prepare::StyleRequest::default())
            .ok()?;
    let diagram = output.diagram.into_inner();
    let layout_result = plotgram_core::layout::compute_layout(&diagram).ok()?;
    Some((diagram, layout_result))
}

/// 仓库根：`CARGO_MANIFEST_DIR`（=crates/plotgram-eval）的上两级。
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

/// 验证单图。
fn check_diagram(name: &str, source: &str) -> DiagramCheck {
    let (diagram, layout_result) = match try_parse_and_layout(source) {
        Some(v) => v,
        None => {
            return DiagramCheck {
                name: name.to_string(),
                dtype: DiagramType::Flowchart,
                plan_ok: false,
                validate_ok: false,
                nodes: 0,
                channels_ok: 0,
                ports_count: 0,
                bundles: 0,
                ink_match_rate: 0.0,
                ink_max_dev: 0.0,
                gaps: Vec::new(),
                error: Some("解析或布局失败".to_string()),
            };
        }
    };

    let input = AdapterInput {
        diagram: &diagram,
        layout_result: &layout_result,
    };

    let output = match adapt_from_legacy(&input) {
        Ok(o) => o,
        Err(e) => {
            return DiagramCheck {
                name: name.to_string(),
                dtype: diagram.diagram_type.clone(),
                plan_ok: false,
                validate_ok: false,
                nodes: 0,
                channels_ok: 0,
                ports_count: 0,
                bundles: 0,
                ink_match_rate: 0.0,
                ink_max_dev: 0.0,
                gaps: Vec::new(),
                error: Some(format!("{e}")),
            };
        }
    };

    let validate_ok = output.plan.validate().is_ok();
    let nodes = output.plan.node_slots.len();
    let channels_ok = output.plan.channels.len();
    let ports_count = output.plan.ports.len();
    let bundles = output.plan.bundles.len();

    // Ink 对拍
    let node_rects: BTreeMap<String, (f64, f64, f64, f64)> = layout_result
        .nodes
        .iter()
        .map(|(id, n)| {
            (
                id.clone(),
                (n.x, n.y, n.width, n.height),
            )
        })
        .collect();
    let ctx = InkContext::from_plan_and_rects(&output.plan, node_rects);
    let ink = ink_all(&output.plan, &output.substrate, &ctx);
    let report = compare_ink_vs_legacy(&ink, &layout_result.edges);
    let ink_match_rate = if report.total_edges == 0 {
        1.0
    } else {
        report.matched as f64 / report.total_edges as f64
    };

    DiagramCheck {
        name: name.to_string(),
        dtype: diagram.diagram_type.clone(),
        plan_ok: true,
        validate_ok,
        nodes,
        channels_ok,
        ports_count,
        bundles,
        ink_match_rate,
        ink_max_dev: report.max_pointwise_deviation,
        gaps: output.gaps,
        error: None,
    }
}

fn render_report(checks: &[DiagramCheck], set_name: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# Atlas Adapter Check — {}\n\n",
        set_name
    ));

    // 汇总表
    out.push_str("| 图 | 图种 | Plan | Validate | Nodes | Channels | Ports | Bundles | Ink一致率 | 缺口 |\n");
    out.push_str("|---|---|---|---|---|---|---|---|---|---|\n");
    for c in checks {
        let plan_s = if c.plan_ok { "✓" } else { "✗" };
        let val_s = if c.validate_ok { "✓" } else { "✗" };
        let ink_s = format!("{:.0}%", c.ink_match_rate * 100.0);
        let gap_s = if c.gaps.is_empty() {
            "—".to_string()
        } else {
            format!("{}", c.gaps.len())
        };
        out.push_str(&format!(
            "| {} | {:?} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            c.name, c.dtype, plan_s, val_s, c.nodes, c.channels_ok, c.ports_count, c.bundles, ink_s, gap_s
        ));
    }

    // 统计
    let total = checks.len();
    let plan_ok = checks.iter().filter(|c| c.plan_ok).count();
    let val_ok = checks.iter().filter(|c| c.validate_ok).count();
    let avg_ink = if total > 0 {
        checks.iter().map(|c| c.ink_match_rate).sum::<f64>() / total as f64
    } else {
        0.0
    };
    out.push_str(&format!(
        "\n**汇总**: {}/{} Plan构造成功, {}/{} validate通过, 平均Ink一致率 {:.0}%\n",
        plan_ok, total, val_ok, total, avg_ink * 100.0
    ));

    // 缺口明细
    let has_gaps = checks.iter().any(|c| !c.gaps.is_empty());
    if has_gaps {
        out.push_str("\n## 表达力缺口明细\n\n");
        for c in checks {
            if c.gaps.is_empty() {
                continue;
            }
            out.push_str(&format!("### {}\n\n", c.name));
            for g in &c.gaps {
                match g {
                    ExpressivenessGap::Infeasible { edge, from, to, reason } => {
                        out.push_str(&format!(
                            "- **Infeasible** edge={} {}→{}: {}\n",
                            edge, from, to, reason
                        ));
                    }
                    ExpressivenessGap::TopologyMismatch { edge, expected_bends, got_bends } => {
                        out.push_str(&format!(
                            "- **TopologyMismatch** edge={}: expected {} bends, got {}\n",
                            edge, expected_bends, got_bends
                        ));
                    }
                    ExpressivenessGap::UnmappedDecision { edge, kind } => {
                        out.push_str(&format!(
                            "- **UnmappedDecision** edge={}: {}\n",
                            edge, kind
                        ));
                    }
                }
            }
            out.push('\n');
        }
    }

    // 失败图
    let failures: Vec<_> = checks.iter().filter(|c| !c.plan_ok).collect();
    if !failures.is_empty() {
        out.push_str("\n## 构造失败\n\n");
        for c in &failures {
            out.push_str(&format!(
                "- **{}** ({:?}): {}\n",
                c.name,
                c.dtype,
                c.error.as_deref().unwrap_or("未知")
            ));
        }
    }

    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let root = repo_root();

    let set_path = args
        .iter()
        .position(|a| a == "--set")
        .and_then(|i| args.get(i + 1).cloned())
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("benchmarks/sets/product-regression-set.txt"));

    let output_path = args
        .iter()
        .position(|a| a == "--output" || a == "-o")
        .and_then(|i| args.get(i + 1).cloned());

    if !set_path.exists() {
        eprintln!("✗ 图集清单不存在: {:?}", set_path);
        eprintln!("  用 --set <file> 指定");
        std::process::exit(1);
    }

    eprintln!("▶ 图集清单: {:?}", set_path);
    let entries = load_set(&set_path, &root);
    eprintln!("  加载 {} 个图", entries.len());
    if entries.is_empty() {
        eprintln!("✗ 没有可验证的图");
        std::process::exit(1);
    }

    let mut checks = Vec::with_capacity(entries.len());
    for (name, source) in &entries {
        let c = check_diagram(name, source);
        let status = if c.plan_ok {
            format!(
                "Plan✓ V{} nodes={} ch={} ports={} ink={:.0}% gaps={}",
                if c.validate_ok { "✓" } else { "✗" },
                c.nodes,
                c.channels_ok,
                c.ports_count,
                c.ink_match_rate * 100.0,
                c.gaps.len()
            )
        } else {
            format!("Plan✗ ({})", c.error.as_deref().unwrap_or("?"))
        };
        eprintln!("  {:<48} {}", c.name, status);
        checks.push(c);
    }

    let set_name = set_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("custom");
    let report = render_report(&checks, set_name);

    match output_path {
        Some(path) => {
            fs::write(&path, &report).expect("写入输出文件失败");
            eprintln!("\n报告已写入 {}", path);
        }
        None => {
            println!("{}", report);
        }
    }
}
