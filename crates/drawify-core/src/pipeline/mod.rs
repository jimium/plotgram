//! 统一管线编排:parse → prepare → validate → layout → render → encode。
//!
//! 本模块是整个 drawify-core 的"导演",只做编排(串联服务函数),不实现具体逻辑。
//! 各服务模块([`crate::dsl::parser`] / [`crate::prepare`] / [`crate::validation`] /
//! [`crate::layout`] / [`crate::render`])只提供纯函数,接受上一层的输入,输出结果。
//!
//! ```text
//! DSL 源码
//!     ↓ parser::parse()           → RawDiagram
//!     ↓ prepare::prepare()        → PreparedDiagram
//!     ↓ validation::validate()    → PreparedDiagram (已校验)
//!     ↓ scene::compute_layout()   → LayoutResult
//!     ↓ scene::build_scene()      → ExportScene
//!     ↓ encode::encode_scene()    → RenderOutput
//! ```
//!
//! - [`prepare`] 子模块:预处理编排(parse → prepare → validate)
//! - [`render`] 子模块:渲染编排(compute_layout → build_scene → encode)
//! - [`run`]:端到端入口,从 DSL 源码一步到 RenderOutput

pub mod prepare;
pub mod render;

pub use prepare::{
    import_prepare_validate, parse, parse_prepare, parse_prepare_validate, prepare, PipelineOutput,
    PrepareOutput,
};
pub use render::{
    render_bytes, render_json, render_output, render_output_with_report, render_text,
    render_with_style_json, RenderOutputWithReport,
};

use crate::error::DiagnosticError;
use crate::prepare::StyleRequest;
use crate::render::{RenderFormat, RenderOutput, RenderRequest};

/// 端到端管线产出:成功返回 `T`,失败返回诊断错误。
pub enum PipelineResult<T> {
    /// 管线成功完成,携带最终产出。
    Ok(T),
    /// parse / prepare / validate 阶段产生阻断性错误。
    Errors {
        errors: Vec<DiagnosticError>,
        warnings: Vec<DiagnosticError>,
    },
}

/// 端到端管线:DSL 源码 → RenderOutput。
///
/// 串联 parse → prepare → validate → layout → build_scene → encode。
/// 供 CLI / Server / WASM 等单格式场景使用。
///
/// # 示例
///
/// ```no_run
/// use drawify_core::pipeline;
/// use drawify_core::prepare::StyleRequest;
/// use drawify_core::render::RenderFormat;
///
/// match pipeline::run("diagram flowchart { entity a \"A\" }", &StyleRequest::default(), RenderFormat::Svg) {
///     pipeline::PipelineResult::Ok(output) => { /* 使用 output */ }
///     pipeline::PipelineResult::Errors { errors, warnings } => { /* 处理诊断 */ }
/// }
/// ```
pub fn run(
    source: &str,
    style_request: &StyleRequest,
    format: RenderFormat,
) -> PipelineResult<RenderOutput> {
    let output = parse_prepare_validate(source, style_request);
    if !output.is_valid() {
        return PipelineResult::Errors {
            errors: output.errors,
            warnings: output.warnings,
        };
    }
    let prepared = output.diagram.unwrap();
    let request = RenderRequest::new(&prepared, format);
    match render_output(&request) {
        Ok(render_output) => PipelineResult::Ok(render_output),
        Err(e) => PipelineResult::Errors {
            errors: e.into_diagnostics(),
            warnings: output.warnings,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prepare::StyleRequest;
    use crate::render::RenderFormat;

    #[test]
    fn run_produces_svg_for_valid_source() {
        let source = r#"diagram flowchart {
            entity a "A"
            entity b "B"
            a -> b
        }"#;
        match run(source, &StyleRequest::default(), RenderFormat::Svg) {
            PipelineResult::Ok(crate::render::RenderOutput::Text(svg)) => {
                assert!(svg.starts_with("<svg"));
                assert!(svg.contains("A"));
                assert!(svg.contains("B"));
            }
            PipelineResult::Ok(crate::render::RenderOutput::Binary(_)) => {
                panic!("expected text svg output");
            }
            PipelineResult::Errors { errors, .. } => {
                panic!("expected ok, got errors: {:?}", errors);
            }
        }
    }

    #[test]
    fn run_returns_errors_for_invalid_source() {
        let source = "diagram flowchart { entity a }"; // 缺少 label
        match run(source, &StyleRequest::default(), RenderFormat::Svg) {
            PipelineResult::Ok(_) => panic!("expected errors for invalid source"),
            PipelineResult::Errors { errors, .. } => {
                assert!(!errors.is_empty());
            }
        }
    }

    #[test]
    fn run_produces_ascii_for_valid_source() {
        let source = r#"diagram flowchart {
            entity a "A"
            entity b "B"
            a -> b
        }"#;
        match run(source, &StyleRequest::default(), RenderFormat::Ascii) {
            PipelineResult::Ok(crate::render::RenderOutput::Text(text)) => {
                assert!(!text.is_empty());
            }
            PipelineResult::Ok(crate::render::RenderOutput::Binary(_)) => {
                panic!("expected text ascii output");
            }
            PipelineResult::Errors { errors, .. } => {
                panic!("expected ok, got errors: {:?}", errors);
            }
        }
    }
}
