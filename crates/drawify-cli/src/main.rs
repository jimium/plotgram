//! Drawify CLI
//!
//! 命令行工具，用于解析、验证和渲染 Drawify 文件（.dfy）。

use clap::{Parser, Subcommand};
use drawify_core::diff2::{self, ChangeSet, ChangeOp};
use drawify_core::error::DiagnosticError;
use drawify_core::interchange::mindmap::{
    import_interchange, InputFormat, MarkdownImportOptions,
};
use drawify_core::prepare::StyleRequest;
use drawify_core::pipeline::{import_prepare_validate, parse_prepare, parse_prepare_validate, PipelineOutput};
use drawify_core::pipeline::{render_bytes, render_json, render_text};
use drawify_core::render::encode::{fonts_dir, set_fonts_dir};
use drawify_core::RenderFormat;
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "drawify",
    about = "Drawify - Turn anything into a diagram",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// 解析并渲染 Drawify 文件
    Render {
        /// 输入的 .dfy 文件路径
        input: String,
        /// 输出格式 (svg/ascii/png/webp/json/drawio/md-outline/opml/freemind)
        #[arg(short, long, default_value = "svg")]
        format: String,
        /// 输出文件路径（默认 stdout）
        #[arg(short, long)]
        output: Option<String>,
        /// 字体文件目录（覆盖 DRAWIFY_FONTS_DIR 环境变量；均未设置时使用 cwd/fonts/）
        #[arg(long = "fonts-dir")]
        fonts_dir: Option<String>,
        /// 输入格式 (dfy/md-outline)，默认根据文件扩展名推断
        #[arg(long = "input-format")]
        input_format: Option<String>,
    },
    /// 验证 Drawify 文件的语法和语义
    Validate {
        /// 输入的 .dfy 文件路径
        input: String,
        /// 诊断输出格式 (text/json)
        #[arg(short, long, default_value = "text")]
        format: String,
        /// 额外执行布局质量检查（LayoutLint 全量规则）
        #[arg(long = "layout-check")]
        layout_check: bool,
    },
    /// 对布局结果运行静态质量检查（LayoutLint）
    Lint {
        /// 输入的 .dfy 文件路径
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
    },
    /// 将 Drawify AST 导出为 JSON
    Export {
        /// 输入的 .dfy 文件路径
        input: String,
    },
    /// 比较两个 Drawify 文件的差异
    Diff {
        /// 原始 Drawify 文件
        #[arg(short = 'o', long = "old")]
        old_file: String,
        /// 新 Drawify 文件
        #[arg(short = 'n', long = "new")]
        new_file: String,
        /// 输出格式 (text/json)
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// 将变更补丁应用到 Drawify 文件
    Patch {
        /// 输入的 Drawify 文件
        input: String,
        /// 变更补丁文件 (JSON 格式)
        patch_file: String,
        /// 输出文件路径（默认 stdout）
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
            fonts_dir,
            input_format,
        }) => cmd_render(&input, &format, output.as_deref(), fonts_dir.as_deref(), input_format.as_deref()),
        Some(Commands::Validate { input, format, layout_check }) => cmd_validate(&input, &format, layout_check),
        Some(Commands::Lint {
            input,
            format,
            profile,
            ignore,
            fail_on_warning,
        }) => cmd_lint(&input, &format, &profile, ignore.as_deref(), fail_on_warning),
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
        None => {
            println!("Drawify - Turn anything into a diagram");
            println!("使用 'drawify --help' 查看可用命令");
        }
    }
}

fn read_source(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("错误: 无法读取文件 '{}': {}", path, e);
        std::process::exit(1);
    })
}

fn configure_fonts_dir(cli_fonts_dir: Option<&str>) {
    if let Some(dir) = cli_fonts_dir {
        set_fonts_dir(PathBuf::from(dir));
    }

    let dir = fonts_dir();
    if !dir.is_dir() {
        eprintln!(
            "警告: 字体目录不存在 '{}'，PNG/WebP 渲染中的中文可能显示异常",
            dir.display()
        );
    }
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
            "dfy" => InputFormat::Drawify,
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
                InputFormat::Drawify
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

fn cmd_render(input: &str, format_str: &str, output: Option<&str>, fonts_dir: Option<&str>, input_format: Option<&str>) {
    configure_fonts_dir(fonts_dir);

    let format = RenderFormat::from_str(format_str).unwrap_or_else(|| {
        eprintln!(
            "错误: 不支持的格式 '{}'。支持的格式: svg, ascii, png, webp, json, drawio, md-outline, opml, freemind",
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
        InputFormat::Drawify => {
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

    // Render
    let request = drawify_core::render::RenderRequest::new(&prepared, format);
    match output {
        Some(path) => {
            match format {
                RenderFormat::Png
                | RenderFormat::Webp => {
                    let output_bytes =
                        render_bytes(&request).unwrap_or_else(|e| {
                            eprintln!("错误: 渲染失败: {}", e);
                            std::process::exit(1);
                        });
                    fs::write(path, output_bytes).unwrap_or_else(|e| {
                        eprintln!("错误: 无法写入文件 '{}': {}", path, e);
                        std::process::exit(1);
                    });
                }
                _ => {
                    let output_content = render_text(&request)
                        .unwrap_or_else(|e| {
                            eprintln!("错误: 渲染失败: {}", e);
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
        None => {
            // 输出到 stdout
            match format {
                RenderFormat::Png
                | RenderFormat::Webp => {
                    eprintln!("错误: PNG 和 WebP 格式需要指定输出文件（使用 -o 或 --output）");
                    std::process::exit(1);
                }
                RenderFormat::Drawio => {
                    let output_content = render_text(&request)
                        .unwrap_or_else(|e| {
                            eprintln!("错误: 渲染失败: {}", e);
                            std::process::exit(1);
                        });
                    println!("{}", output_content);
                }
                _ => {
                    let output_content = render_text(&request)
                        .unwrap_or_else(|e| {
                            eprintln!("错误: 渲染失败: {}", e);
                            std::process::exit(1);
                        });
                    println!("{}", output_content);
                }
            }
        }
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
                let config = drawify_core::layout::LintConfig::strict();
                match run_layout_lint(&prepared, &config) {
                    Ok(report) => print_lint_report_text(&report, &config, true),
                    Err(e) => eprintln!("⚠ 布局计算失败，跳过质量检查: {}", e),
                }
            }
        }
    }
}

fn build_lint_config(profile: &str, ignore: Option<&str>, fail_on_warning: bool) -> drawify_core::layout::LintConfig {
    let profile = drawify_core::layout::parse_lint_profile(profile).unwrap_or_else(|| {
        eprintln!("未知 lint profile '{profile}'，使用 default（可选：default / strict / ci / verbose / all）");
        drawify_core::layout::LintProfile::Default
    });
    let mut config = drawify_core::layout::LintConfig::profile(profile);
    if let Some(ignore_list) = ignore {
        let rules = drawify_core::layout::parse_lint_rules_list(ignore_list);
        if rules.is_empty() && !ignore_list.trim().is_empty() {
            eprintln!("警告：--ignore 未识别任何规则");
        }
        config = config.without(&rules);
    }
    config.with_fail_on_warning(fail_on_warning)
}

fn run_layout_lint(
    prepared: &drawify_core::ast::PreparedDiagram,
    config: &drawify_core::layout::LintConfig,
) -> Result<drawify_core::layout::LintReport, drawify_core::error::DiagnosticError> {
    let diagram = prepared.inner();
    let layout = drawify_core::layout::compute_layout_with_plan(diagram, prepared.layout_plan())?;
    Ok(drawify_core::layout::LayoutLinter::with_config(config.clone()).run(diagram, &layout))
}

fn print_lint_report_text(
    report: &drawify_core::layout::LintReport,
    config: &drawify_core::layout::LintConfig,
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

    for v in &report.violations {
        let level = match v.severity {
            drawify_core::layout::LintSeverity::Error => "error",
            drawify_core::layout::LintSeverity::Warning => "warning",
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
    }

    if exit_on_failure && !report.is_acceptable(config) {
        std::process::exit(1);
    }
}

fn cmd_lint(input: &str, format_str: &str, profile: &str, ignore: Option<&str>, fail_on_warning: bool) {
    let config = build_lint_config(profile, ignore, fail_on_warning);
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

    let old_diagram = drawify_core::parse(&old_source).unwrap_or_else(|e| {
        for err in e.into_diagnostics() {
            eprintln!("{}", err);
        }
        std::process::exit(1);
    });
    let new_diagram = drawify_core::parse(&new_source).unwrap_or_else(|e| {
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

    let raw_diagram = drawify_core::parse(&source).unwrap_or_else(|e| {
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

    let prepare_output = match drawify_core::pipeline::prepare(result.diagram, &StyleRequest::default()) {
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
