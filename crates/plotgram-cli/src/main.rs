//! Plotgram CLI
//!
//! 命令行工具，用于解析、验证和渲染 Plotgram 文件（.pgm）。

use clap::{Parser, Subcommand};
use plotgram_core::diff2::{self, ChangeSet, ChangeOp};
use plotgram_core::error::{DiagnosticError, PlotgramError};
use plotgram_core::interchange::mindmap::{
    import_interchange, InputFormat, MarkdownImportOptions,
};
use plotgram_core::prepare::StyleRequest;
use plotgram_core::pipeline::{import_prepare_validate, parse_prepare, parse_prepare_validate, PipelineOutput};
use plotgram_core::pipeline::{render_json, render_text};
use plotgram_core::RenderFormat;
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "plotgram",
    about = "Plotgram - Turn anything into a diagram",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// 解析并渲染 Plotgram 文件
    Render {
        /// 输入的 .pgm 文件路径
        input: String,
        /// 输出格式 (svg/ascii/json/drawio/md-outline/opml/freemind)；位图请用 plotgram-raster 转换 SVG
        #[arg(short, long, default_value = "svg")]
        format: String,
        /// 输出文件路径（默认 stdout）
        #[arg(short, long)]
        output: Option<String>,
        /// 输入格式 (dfy/md-outline)，默认根据文件扩展名推断
        #[arg(long = "input-format")]
        input_format: Option<String>,
        /// 省略画布背景（SVG 输出为透明底）
        #[arg(long = "transparent-background")]
        transparent_background: bool,
        /// 在画布顶部绘制 DSL title（默认不绘制）
        #[arg(long)]
        title: bool,
    },
    /// 验证 Plotgram 文件的语法和语义
    Validate {
        /// 输入的 .pgm 文件路径
        input: String,
        /// 诊断输出格式 (text/json)
        #[arg(short, long, default_value = "text")]
        format: String,
        /// 额外执行布局质量检查（LayoutLint strict 预设）
        #[arg(long = "layout-check")]
        layout_check: bool,
    },
    /// 对布局结果运行静态质量检查（LayoutLint）
    Lint {
        /// 输入的 .pgm 文件路径
        input: String,
        /// 输出格式 (text/json)
        #[arg(short, long, default_value = "text")]
        format: String,
        /// 预设：default（日常）/ strict|ci（门禁）/ verbose|all（全规则）
        #[arg(long, default_value = "default")]
        profile: String,
        /// 忽略的规则（逗号分隔），如 edge_crossing,edge_on_group_border
        #[arg(long = "ignore")]
        ignore: Option<String>,
        /// warning 也视为失败
        #[arg(long = "fail-on-warning")]
        fail_on_warning: bool,
        /// 生成面向 Agent 的布局建议
        #[arg(long = "advice")]
        advice: bool,
    },
    /// 将 Plotgram AST 导出为 JSON
    Export {
        /// 输入的 .pgm 文件路径
        input: String,
    },
    /// 比较两个 Plotgram 文件的差异
    Diff {
        /// 原始 Plotgram 文件
        #[arg(short = 'o', long = "old")]
        old_file: String,
        /// 新 Plotgram 文件
        #[arg(short = 'n', long = "new")]
        new_file: String,
        /// 输出格式 (text/json)
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// 将变更补丁应用到 Plotgram 文件
    Patch {
        /// 输入的 Plotgram 文件
        input: String,
        /// 变更补丁文件 (JSON 格式)
        patch_file: String,
        /// 输出文件路径（默认 stdout）
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Atlas vs LayoutPipeline 影子对拍（诊断工具，退出码恒 0；非 CI 门禁）
    Shadow {
        /// 图集清单 (.txt，每行一个 .pgm 路径，`#` 注释) 或单个 .pgm 文件
        input: String,
        /// HTML 报告输出路径（SVG 并排 + 节点 diff 表；缺省只打 stdout 汇总）
        #[arg(short, long)]
        output: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Render {
            input,
            format,
            output,
            input_format,
            transparent_background,
            title,
        }) => cmd_render(
            &input,
            &format,
            output.as_deref(),
            input_format.as_deref(),
            transparent_background,
            title,
        ),
        Some(Commands::Validate { input, format, layout_check }) => cmd_validate(&input, &format, layout_check),
        Some(Commands::Lint {
            input,
            format,
            profile,
            ignore,
            fail_on_warning,
            advice,
        }) => cmd_lint(&input, &format, &profile, ignore.as_deref(), fail_on_warning, advice),
        Some(Commands::Export { input }) => cmd_export(&input),
        Some(Commands::Diff {
            old_file,
            new_file,
            format,
        }) => cmd_diff(&old_file, &new_file, &format),
        Some(Commands::Patch {
            input,
            patch_file,
            output,
        }) => cmd_patch(&input, &patch_file, output.as_deref()),
        Some(Commands::Shadow { input, output }) => cmd_shadow(&input, output.as_deref()),
        None => {
            println!("Plotgram - Turn anything into a diagram");
            println!("使用 'plotgram --help' 查看可用命令");
        }
    }
}

fn read_source(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("错误: 无法读取文件 '{}': {}", path, e);
        std::process::exit(1);
    })
}

/// 解析 + prepare（不 validate）；用于 diff / export / patch。
fn parse_and_prepare(source: &str) -> PipelineOutput {
    parse_prepare(source, &StyleRequest::default())
}

/// 根据 --input-format 参数和文件扩展名推断输入格式。
fn resolve_input_format(cli_format: Option<&str>, input_path: &str) -> InputFormat {
    match cli_format {
        Some(f) => match f {
            "md-outline" => InputFormat::MdOutline,
            "dfy" => InputFormat::Plotgram,
            _ => {
                eprintln!(
                    "错误: 不支持的输入格式 '{}'。支持的格式: dfy, md-outline",
                    f
                );
                std::process::exit(1);
            }
        },
        None => {
            // 根据文件扩展名推断
            if input_path.ends_with(".md") || input_path.ends_with(".markdown") {
                InputFormat::MdOutline
            } else {
                InputFormat::Plotgram
            }
        }
    }
}

/// 打印诊断（text 格式）。`source` 用于显示源码片段，None 时不显示。
fn print_diagnostics(
    errors: &[DiagnosticError],
    warnings: &[DiagnosticError],
    source: Option<&str>,
) {
    for w in warnings {
        eprintln!("{}", w);
        if let Some(ref s) = w.suggestion {
            eprintln!("  建议: {}", s.text);
        }
        if let Some(src) = source {
            print_source_snippet(src, w);
        }
    }
    for e in errors {
        eprintln!("{}", e);
        if let Some(ref s) = e.suggestion {
            eprintln!("  建议: {}", s.text);
        }
        if let Some(src) = source {
            print_source_snippet(src, e);
        }
    }
}

/// 显示错误/警告对应的源码片段（带 `^` 指示位置）。
fn print_source_snippet(source: &str, err: &DiagnosticError) {
    let line = err.location.start.line;
    if line == 0 {
        return;
    }
    let lines: Vec<&str> = source.lines().collect();
    if line > lines.len() {
        return;
    }
    let src_line = lines[line - 1];
    eprintln!("  │");
    eprintln!("  │ {}", src_line);
    // 计算 `^` 指示范围
    let start_col = err.location.start.column.saturating_sub(1).min(src_line.chars().count());
    let end_col = err
        .location
        .end
        .column
        .saturating_sub(1)
        .max(start_col + 1)
        .min(src_line.chars().count());
    let prefix_len = src_line.chars().take(start_col).map(|c| c.len_utf8()).sum::<usize>();
    let marker_len = src_line
        .chars()
        .skip(start_col)
        .take(end_col.saturating_sub(start_col))
        .map(|c| c.len_utf8())
        .sum::<usize>()
        .max(1);
    let marker: String = "^".repeat(marker_len);
    eprintln!("  │ {}{}", " ".repeat(prefix_len), marker);
}

/// 打印渲染阶段 `PlotgramError` 中的完整诊断信息（含 valid_values 等上下文）。
fn print_render_error(e: PlotgramError, source: Option<&str>) {
    let diags = e.into_diagnostics();
    for d in &diags {
        eprintln!("{}", d);
        if let Some(src) = source {
            print_source_snippet(src, d);
        }
    }
}

/// 将诊断输出为 JSON（spec §4.2 结构）。
fn print_diagnostics_json(output: &PipelineOutput) {
    let json = serde_json::json!({
        "errors": output.errors,
        "warnings": output.warnings,
        "total_errors": output.total_errors,
        "total_warnings": output.total_warnings,
        "truncated": output.truncated,
        "valid": output.is_valid(),
    });
    println!("{}", serde_json::to_string_pretty(&json).unwrap_or_else(|e| {
        eprintln!("错误: 无法序列化诊断 JSON: {}", e);
        std::process::exit(1);
    }));
}

fn cmd_render(
    input: &str,
    format_str: &str,
    output: Option<&str>,
    input_format: Option<&str>,
    transparent_background: bool,
    show_title: bool,
) {
    let format = RenderFormat::from_str(format_str).unwrap_or_else(|| {
        eprintln!(
            "错误: 不支持的格式 '{}'。支持的格式: svg, ascii, json, drawio, md-outline, opml, freemind（位图请用 plotgram-raster 将 SVG 转为 PNG/WebP）",
            format_str
        );
        std::process::exit(1);
    });

    let source = read_source(input);

    // 确定输入格式
    let resolved_format = resolve_input_format(input_format, input);

    let pipeline_output = match resolved_format {
        InputFormat::MdOutline => {
            // Markdown 大纲导入路径
            let import_options = MarkdownImportOptions::default();
            match import_interchange(&source, InputFormat::MdOutline, &import_options) {
                Ok(diagram) => import_prepare_validate(diagram, &StyleRequest::default()),
                Err(e) => {
                    eprintln!("错误: 导入失败: {:?}", e);
                    std::process::exit(1);
                }
            }
        }
        InputFormat::Plotgram => {
            parse_prepare_validate(&source, &StyleRequest::default())
        }
    };

    print_diagnostics(&pipeline_output.errors, &pipeline_output.warnings, Some(&source));

    if !pipeline_output.is_valid() {
        eprintln!(
            "\n结果: {} 个错误, {} 个警告",
            pipeline_output.total_errors,
            pipeline_output.total_warnings
        );
        if pipeline_output.truncated {
            eprintln!("（部分诊断已截断，详见 total_errors/total_warnings）");
        }
        std::process::exit(1);
    }

    let prepared = pipeline_output.diagram.unwrap();

    // Render（可选：PLOTGRAM_ATLAS_PLAN_CACHE 走 Plan 增量；SVG 注入 layout）
    let mut request = plotgram_core::render::RenderRequest::new(&prepared, format);
    request.transparent_background = transparent_background;
    request.show_title = show_title;

    let cached_layout = if let Ok(cache) = std::env::var("PLOTGRAM_ATLAS_PLAN_CACHE") {
        match layout_with_optional_plan_cache(
            prepared.inner(),
            prepared.layout_plan(),
            &cache,
        ) {
            Ok(layout) => Some(layout),
            Err(e) => {
                eprintln!("错误: 增量布局失败: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        None
    };

    match output {
        Some(path) => {
            match format {
                RenderFormat::Svg => {
                    let output_content = if let Some(ref layout) = cached_layout {
                        plotgram_core::pipeline::render_svg_with_layout(&request, layout.clone())
                            .unwrap_or_else(|e| {
                                print_render_error(e, Some(&source));
                                std::process::exit(1);
                            })
                    } else {
                        render_text(&request).unwrap_or_else(|e| {
                            print_render_error(e, Some(&source));
                            std::process::exit(1);
                        })
                    };
                    fs::write(path, &output_content).unwrap_or_else(|e| {
                        eprintln!("错误: 无法写入文件 '{}': {}", path, e);
                        std::process::exit(1);
                    });
                }
                _ => {
                    let output_content = render_text(&request).unwrap_or_else(|e| {
                        print_render_error(e, Some(&source));
                        std::process::exit(1);
                    });
                    fs::write(path, &output_content).unwrap_or_else(|e| {
                        eprintln!("错误: 无法写入文件 '{}': {}", path, e);
                        std::process::exit(1);
                    });
                }
            }
            println!("{} 已写入: {}", format.to_string().to_uppercase(), path);
        }
        None => match format {
            RenderFormat::Svg => {
                let output_content = if let Some(ref layout) = cached_layout {
                    plotgram_core::pipeline::render_svg_with_layout(&request, layout.clone())
                        .unwrap_or_else(|e| {
                            print_render_error(e, Some(&source));
                            std::process::exit(1);
                        })
                } else {
                    render_text(&request).unwrap_or_else(|e| {
                        print_render_error(e, Some(&source));
                        std::process::exit(1);
                    })
                };
                println!("{}", output_content);
            }
            _ => {
                let output_content = render_text(&request).unwrap_or_else(|e| {
                    print_render_error(e, Some(&source));
                    std::process::exit(1);
                });
                println!("{}", output_content);
            }
        },
    }
}

fn cmd_validate(input: &str, format_str: &str, layout_check: bool) {
    let source = read_source(input);
    let output = parse_prepare_validate(&source, &StyleRequest::default());

    match format_str {
        "json" => {
            print_diagnostics_json(&output);
            if !output.is_valid() {
                std::process::exit(1);
            }
        }
        _ => {
            print_diagnostics(&output.errors, &output.warnings, Some(&source));

            if !output.is_valid() {
                eprintln!(
                    "\n结果: {} 个错误, {} 个警告",
                    output.total_errors, output.total_warnings
                );
                if output.truncated {
                    eprintln!("（部分诊断已截断，详见 total_errors/total_warnings）");
                }
                std::process::exit(1);
            }

            let prepared = output.diagram.unwrap();
            println!(
                "✓ 验证通过 ({} 个实体, {} 个关系, {} 个分组, {} 个警告)",
                prepared.entities.len(),
                prepared.relations.len(),
                prepared.groups.len(),
                output.warnings.len()
            );

            // 布局质量检查
            if layout_check {
                let config = plotgram_core::layout::LintConfig::strict();
                match run_layout_lint(&prepared, &config) {
                    Ok(report) => print_lint_report_text(&report, &config, true),
                    Err(e) => eprintln!("⚠ 布局计算失败，跳过质量检查: {}", e),
                }
            }
        }
    }
}

fn build_lint_config(
    profile: &str,
    ignore: Option<&str>,
    fail_on_warning: bool,
    advice: bool,
) -> plotgram_core::layout::LintConfig {
    let profile = plotgram_core::layout::parse_lint_profile(profile).unwrap_or_else(|| {
        eprintln!("未知 lint profile '{profile}'，使用 default（可选：default / strict / ci / verbose / all）");
        plotgram_core::layout::LintProfile::Default
    });
    let mut config = plotgram_core::layout::LintConfig::profile(profile);
    if let Some(ignore_list) = ignore {
        let rules = plotgram_core::layout::parse_lint_rules_list(ignore_list);
        if rules.is_empty() && !ignore_list.trim().is_empty() {
            eprintln!("警告：--ignore 未识别任何规则");
        }
        config = config.without(&rules);
    }
    config
        .with_fail_on_warning(fail_on_warning)
        .with_advice(advice)
}

fn run_layout_lint(
    prepared: &plotgram_core::ast::PreparedDiagram,
    config: &plotgram_core::layout::LintConfig,
) -> Result<plotgram_core::layout::LintReport, plotgram_core::error::DiagnosticError> {
    let diagram = prepared.inner();
    let layout = if let Ok(cache) = std::env::var("PLOTGRAM_ATLAS_PLAN_CACHE") {
        layout_with_optional_plan_cache(diagram, prepared.layout_plan(), &cache)?
    } else {
        plotgram_core::layout::compute_layout_with_plan(diagram, prepared.layout_plan())?
    };
    Ok(plotgram_core::layout::LayoutLinter::with_config(config.clone()).run(diagram, &layout))
}

fn layout_with_optional_plan_cache(
    diagram: &plotgram_core::ast::Diagram,
    plan: &plotgram_core::layout::pipeline::plan::LayoutPlan,
    cache_path: &str,
) -> Result<plotgram_core::layout::LayoutResult, plotgram_core::error::DiagnosticError> {
    let path = std::path::Path::new(cache_path);
    if path.is_file() {
        if let Ok(bytes) = std::fs::read(path) {
            if let Ok(prev) =
                serde_json::from_slice::<plotgram_core::layout::atlas::plan::Plan>(&bytes)
            {
                eprintln!("[atlas] incremental via PLOTGRAM_ATLAS_PLAN_CACHE");
                let layout =
                    plotgram_core::layout::compute_layout_incremental(diagram, &prev)?;
                if let Some(p) = layout.hints.atlas_plan.as_ref() {
                    let _ = std::fs::write(path, serde_json::to_vec_pretty(p.as_ref()).unwrap_or_default());
                }
                return Ok(layout);
            }
        }
    }
    let layout = plotgram_core::layout::compute_layout_with_plan(diagram, plan)?;
    if let Some(p) = layout.hints.atlas_plan.as_ref() {
        let _ = std::fs::write(path, serde_json::to_vec_pretty(p.as_ref()).unwrap_or_default());
    }
    Ok(layout)
}

fn print_lint_report_text(
    report: &plotgram_core::layout::LintReport,
    config: &plotgram_core::layout::LintConfig,
    exit_on_failure: bool,
) {
    if report.violations.is_empty() {
        println!("✓ 布局 lint 通过（无违规）");
        return;
    }

    let errors = report.error_count();
    let warnings = report.warning_count();
    eprintln!(
        "⚠ 布局 lint 发现 {} 个 error、{} 个 warning:",
        errors, warnings
    );

    for (idx, v) in report.violations.iter().enumerate() {
        let level = match v.severity {
            plotgram_core::layout::LintSeverity::Error => "error",
            plotgram_core::layout::LintSeverity::Warning => "warning",
        };
        let metric = v
            .metric
            .map(|m| format!(" ({m:.1})"))
            .unwrap_or_default();
        eprintln!("[{level}] {}: {}{}", v.rule.as_str(), v.message, metric);
        if !v.entity_ids.is_empty() {
            eprintln!("  entities: {}", v.entity_ids.join(", "));
        }
        if !v.group_ids.is_empty() {
            eprintln!("  groups: {}", v.group_ids.join(", "));
        }
        if let Some(idx) = v.edge_index {
            eprintln!("  edge_index: {idx}");
        }
        if !v.related_edge_indices.is_empty() {
            let related = v
                .related_edge_indices
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            eprintln!("  edge_indices: {}", related.join(", "));
        }
        for advice in report.advices_for_violation(idx) {
            eprintln!(
                "  advice(p{} {:?}): {}",
                advice.priority,
                advice.confidence,
                advice.text
            );
            if let Some(fix) = &advice.fix {
                eprintln!("    fix: {}", fix.action);
            }
        }
    }

    if exit_on_failure && !report.is_acceptable(config) {
        std::process::exit(1);
    }
}

fn cmd_lint(
    input: &str,
    format_str: &str,
    profile: &str,
    ignore: Option<&str>,
    fail_on_warning: bool,
    advice: bool,
) {
    let config = build_lint_config(profile, ignore, fail_on_warning, advice);
    let source = read_source(input);
    let output = parse_prepare_validate(&source, &StyleRequest::default());

    if !output.is_valid() {
        eprintln!("语法/语义验证未通过，跳过布局 lint");
        print_diagnostics(&output.errors, &output.warnings, Some(&source));
        std::process::exit(1);
    }

    let prepared = output.diagram.unwrap();
    let report = match run_layout_lint(&prepared, &config) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("布局计算失败: {}", e);
            std::process::exit(1);
        }
    };

    match format_str {
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&report).expect("serialize lint report")
            );
        }
        _ => print_lint_report_text(&report, &config, false),
    }

    if !report.is_acceptable(&config) {
        std::process::exit(1);
    }
}

fn cmd_export(input: &str) {
    let source = read_source(input);
    let output = parse_and_prepare(&source);

    if !output.errors.is_empty() {
        for e in &output.errors {
            eprintln!("{}", e);
        }
        std::process::exit(1);
    }

    let prepared = output.diagram.unwrap();
    let json = render_json(&prepared);
    println!("{}", json);
}

fn cmd_diff(old_file: &str, new_file: &str, format_str: &str) {
    let old_source = read_source(old_file);
    let new_source = read_source(new_file);

    let old_diagram = plotgram_core::parse(&old_source).unwrap_or_else(|e| {
        for err in e.into_diagnostics() {
            eprintln!("{}", err);
        }
        std::process::exit(1);
    });
    let new_diagram = plotgram_core::parse(&new_source).unwrap_or_else(|e| {
        for err in e.into_diagnostics() {
            eprintln!("{}", err);
        }
        std::process::exit(1);
    });

    let changes = diff2::diff(&old_diagram, &new_diagram);

    match format_str {
        "json" => {
            let json = serde_json::to_string_pretty(&changes).unwrap_or_else(|e| {
                eprintln!("错误: 无法序列化 diff 结果: {}", e);
                std::process::exit(1);
            });
            println!("{}", json);
        }
        "text" | _ => {
            let mut added = 0usize;
            let mut removed = 0usize;
            let mut modified = 0usize;
            for c in &changes.changes {
                match c.op {
                    ChangeOp::Add => added += 1,
                    ChangeOp::Remove => removed += 1,
                    ChangeOp::Modify => modified += 1,
                }
            }
            println!("变更统计: +{} -{} ~{}\n", added, removed, modified);
            for change in &changes.changes {
                let symbol = match change.op {
                    ChangeOp::Add => "+",
                    ChangeOp::Remove => "-",
                    ChangeOp::Modify => "~",
                };
                let target = format!("{:?}", change.path.target).to_lowercase();
                let id_part = change.path.id.as_deref().unwrap_or("");
                let key_part = change.path.attr_key.as_deref().unwrap_or("");
                let path_str = if key_part.is_empty() {
                    format!("/{}/{}", target, id_part)
                } else {
                    format!("/{}/{}/{}", target, id_part, key_part)
                };
                println!("{} {}", symbol, path_str);
            }
        }
    }
}

fn cmd_patch(input: &str, patch_file: &str, output: Option<&str>) {
    let source = read_source(input);

    let raw_diagram = plotgram_core::parse(&source).unwrap_or_else(|e| {
        for err in e.into_diagnostics() {
            eprintln!("{}", err);
        }
        std::process::exit(1);
    });

    let patch_content = read_source(patch_file);

    // 尝试解析为 ChangeSet 或直接解析为 Vec<Change>
    let changeset: ChangeSet =
        if let Ok(cs) = serde_json::from_str::<ChangeSet>(&patch_content) {
            cs
        } else if let Ok(changes) = serde_json::from_str::<Vec<diff2::Change>>(&patch_content) {
            ChangeSet::new(changes)
        } else {
            eprintln!(
                "错误: 无法解析补丁文件 '{}': 期望 JSON 格式的 ChangeSet 或 Change 数组",
                patch_file
            );
            std::process::exit(1);
        };

    let result = diff2::patch(&raw_diagram, &changeset);

    if !result.errors.is_empty() {
        eprintln!("警告: 应用补丁时出现 {} 个错误:", result.errors.len());
        for err in &result.errors {
            eprintln!("  - {}", err);
        }
    }

    if result.is_ok() {
        println!(
            "✓ 补丁应用成功 (已应用: {})",
            result.applied
        );
    } else {
        eprintln!(
            "✗ 补丁应用失败 (已应用: {}, 失败: {})",
            result.applied,
            result.errors.len()
        );
        std::process::exit(1);
    }

    let prepare_output = match plotgram_core::pipeline::prepare(result.diagram, &StyleRequest::default()) {
        Ok(output) => output,
        Err(e) => {
            for err in e.into_diagnostics() {
                eprintln!("{}", err);
            }
            std::process::exit(1);
        }
    };
    let json = render_json(&prepare_output.diagram);

    match output {
        Some(path) => {
            fs::write(path, &json).unwrap_or_else(|e| {
                eprintln!("错误: 无法写入文件 '{}': {}", path, e);
                std::process::exit(1);
            });
            println!("已写入: {}", path);
        }
        None => {
            println!("{}", json);
        }
    }
}

// ─── shadow 对拍（Atlas Stage 0 交付 0.6） ─────────────────────────

/// 单图对拍产出：报告 + 两张 SVG；失败时记录原因（不阻断后续图）。
struct ShadowItem {
    name: String,
    outcome: Result<(plotgram_core::layout::atlas::shadow::ShadowReport, String, String), String>,
}

/// 收集输入：单个 .pgm，或图集清单（每行一个 .pgm 路径，相对 cwd，`#` 注释）。
fn collect_shadow_inputs(input: &str) -> Vec<(String, PathBuf)> {
    let display_name = |p: &str| {
        PathBuf::from(p)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| p.to_string())
    };
    if input.ends_with(".pgm") {
        return vec![(display_name(input), PathBuf::from(input))];
    }
    let content = read_source(input);
    content
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| (display_name(l), PathBuf::from(l)))
        .collect()
}

fn run_shadow_item(
    name: &str,
    path: &PathBuf,
) -> Result<(plotgram_core::layout::atlas::shadow::ShadowReport, String, String), String> {
    use plotgram_core::layout::atlas::shadow::{run_shadow, ShadowRun};
    use plotgram_core::pipeline::render_svg_with_layout;
    use plotgram_core::render::RenderRequest;

    let source =
        fs::read_to_string(path).map_err(|e| format!("无法读取 {}: {}", path.display(), e))?;
    let output = parse_prepare_validate(&source, &StyleRequest::default());
    if !output.is_valid() {
        return Err(format!("验证未通过（{} 个错误）", output.errors.len()));
    }
    let prepared = output.diagram.unwrap();
    let ShadowRun {
        legacy,
        atlas,
        report,
    } = run_shadow(name, prepared.inner(), prepared.layout_plan())
        .map_err(|e| format!("布局失败: {}", e))?;
    let request = RenderRequest::new(&prepared, RenderFormat::Svg);
    let svg_legacy = render_svg_with_layout(&request, legacy)
        .map_err(|e| format!("legacy SVG 渲染失败: {}", e))?;
    let svg_atlas = render_svg_with_layout(&request, atlas)
        .map_err(|e| format!("atlas SVG 渲染失败: {}", e))?;
    Ok((report, svg_legacy, svg_atlas))
}

/// legacy vs atlas 影子对拍：stdout 汇总 + 可选 HTML 报告。退出码恒 0
/// （门禁关闭期只报告不阻断，23 号文 §2）。
fn cmd_shadow(input: &str, output: Option<&str>) {
    let inputs = collect_shadow_inputs(input);
    if inputs.is_empty() {
        eprintln!("图集清单为空: {}", input);
        return;
    }

    let mut items = Vec::new();
    let mut zero_count = 0usize;
    let mut fail_count = 0usize;
    for (name, path) in &inputs {
        let outcome = run_shadow_item(name, path);
        match &outcome {
            Ok((report, _, _)) => {
                if report.is_zero() {
                    zero_count += 1;
                }
                println!("{}", report.summary_line());
            }
            Err(reason) => {
                fail_count += 1;
                println!("[shadow] {}: 跳过（{}）", name, reason);
            }
        }
        items.push(ShadowItem {
            name: name.clone(),
            outcome,
        });
    }
    let compared = items.len() - fail_count;
    println!(
        "[shadow] 汇总: {}/{} 零差异（对拍 {} 图，跳过 {} 图）",
        zero_count, compared, compared, fail_count
    );

    if let Some(path) = output {
        let html = build_shadow_html(&items);
        fs::write(path, html).unwrap_or_else(|e| {
            eprintln!("错误: 无法写入文件 '{}': {}", path, e);
            std::process::exit(1);
        });
        println!("已写入: {}", path);
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// HTML 报告：顶部汇总表 + 每图节点 diff 表与两张 SVG 并排。
fn build_shadow_html(items: &[ShadowItem]) -> String {
    let mut html = String::from(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\">\
         <title>Atlas Shadow 对拍报告</title><style>\
         body{font-family:sans-serif;margin:20px}\
         table{border-collapse:collapse;margin:8px 0}\
         th,td{border:1px solid #ccc;padding:4px 10px;font-size:13px}\
         .zero{color:#0a0}.diff{color:#c00}.skip{color:#888}\
         .pair{display:flex;gap:16px;overflow-x:auto}\
         .pair>div{border:1px solid #ddd;padding:8px}\
         .pair svg{max-width:640px;height:auto}\
         details{margin:12px 0}summary{cursor:pointer;font-weight:bold}\
         </style></head><body><h1>Atlas Shadow 对拍报告</h1>\n",
    );

    // 汇总表
    html.push_str("<table><tr><th>图</th><th>节点 diff</th><th>p95 (px)</th><th>max (px)</th><th>组 diff</th><th>lint</th><th>画布 Δ</th><th>零差异</th></tr>\n");
    for item in items {
        match &item.outcome {
            Ok((r, _, _)) => {
                let mark = if r.is_zero() {
                    "<td class=\"zero\">✓</td>"
                } else {
                    "<td class=\"diff\">✗</td>"
                };
                html.push_str(&format!(
                    "<tr><td>{}</td><td>{}</td><td>{:.2}</td><td>{:.2}</td><td>{}</td><td>{}</td><td>({:.1}, {:.1})</td>{}</tr>\n",
                    html_escape(&item.name),
                    r.node_diffs.len(),
                    r.node_stats.p95,
                    r.node_stats.max,
                    r.group_diffs.len(),
                    if r.lint_legacy == r.lint_atlas { "持平" } else { "有变化" },
                    r.canvas_dw,
                    r.canvas_dh,
                    mark,
                ));
            }
            Err(reason) => {
                html.push_str(&format!(
                    "<tr><td>{}</td><td colspan=\"7\" class=\"skip\">跳过：{}</td></tr>\n",
                    html_escape(&item.name),
                    html_escape(reason),
                ));
            }
        }
    }
    html.push_str("</table>\n");

    // 每图明细
    for item in items {
        let Ok((report, svg_legacy, svg_atlas)) = &item.outcome else {
            continue;
        };
        html.push_str(&format!(
            "<details{}><summary>{} — {}</summary>\n",
            if report.is_zero() { "" } else { " open" },
            html_escape(&item.name),
            html_escape(&report.summary_line()),
        ));
        if !report.node_diffs.is_empty() {
            html.push_str("<table><tr><th>节点</th><th>dx</th><th>dy</th></tr>\n");
            for d in &report.node_diffs {
                html.push_str(&format!(
                    "<tr><td>{}</td><td>{:.2}</td><td>{:.2}</td></tr>\n",
                    html_escape(&d.id),
                    d.dx,
                    d.dy
                ));
            }
            html.push_str("</table>\n");
        }
        html.push_str(&format!(
            "<div class=\"pair\"><div><h3>legacy</h3>{}</div><div><h3>atlas</h3>{}</div></div></details>\n",
            svg_legacy, svg_atlas
        ));
    }

    html.push_str("</body></html>\n");
    html
}
