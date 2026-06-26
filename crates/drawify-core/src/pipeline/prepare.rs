//! 预处理编排:parse → prepare → validate。
//!
//! 本模块串联 [`crate::dsl::parser`]、[`crate::prepare`]、[`crate::validation`] 三个服务,
//! 产出可供下游布局/渲染消费的 [`PreparedDiagram`]。
//!
//! ```text
//! DSL 源码
//!     ↓ parse()
//! RawDiagram（可含缺省 type、未物化 style）
//!     ↓ prepare(raw, style_request)
//! PreparedDiagram（样式已物化、layout_plan 已解析；满足不变量）
//!     ↓ validate（[`parse_prepare_validate`] 已包含）
//!     ↓ layout / render（见 [`super::render`]）
//! ```
//!
//! [`prepare`] 依次执行：
//! 1. `apply_profile_defaults` — 补全缺失的 `entity.type`
//! 2. `expand_structure` — 派生结构语义属性（mindmap 的 `branch_slot` / `tree_depth`）
//! 3. `resolve_style_context` — 从 diagram `theme` / request 解析样式上下文
//! 4. `validate_style_decls` — StyleDecl 校验（错误阻断，警告收集）
//! 5. `materialize_styles` — 将 theme cascade 物化到 `attributes.style`
//! 6. `PreparedDiagram::new` — 封装 diagram 并调用 `LayoutPlan::resolve`

use crate::ast::{PreparedDiagram, RawDiagram};
use crate::error::{DiagnosticError, DrawifyError, Result};
use crate::prepare::{
    apply_profile_defaults, expand_structure, validate_style_decls, StyleRequest,
};
use crate::prepare::style_resolve::resolve_compiled_theme;
#[cfg(debug_assertions)]
use crate::prepare::debug_assert_prepared_invariants;
#[cfg(test)]
use crate::prepare::assert_prepared_invariants;
use crate::dsl::parser;
use crate::validation;

/// `prepare()` 产出：规范化后的图表与 prepare 阶段警告。
#[derive(Debug, Clone)]
pub struct PrepareOutput {
    pub diagram: PreparedDiagram,
    pub warnings: Vec<DiagnosticError>,
}

/// 预处理管线产出：parse → prepare（→ validate）合并诊断。
#[derive(Debug, Clone)]
pub struct PipelineOutput {
    pub diagram: Option<PreparedDiagram>,
    pub errors: Vec<DiagnosticError>,
    pub warnings: Vec<DiagnosticError>,
    /// 包含被截断的错误在内的总数（spec §6.3）
    pub total_errors: usize,
    /// 包含被截断的警告在内的总数
    pub total_warnings: usize,
    /// 是否发生了截断
    pub truncated: bool,
}

impl PipelineOutput {
    /// 解析、prepare、validate 均无阻断性错误且已产出 PreparedDiagram。
    pub fn is_valid(&self) -> bool {
        self.diagram.is_some() && self.errors.is_empty()
    }
}

impl PipelineOutput {
    fn from_parse(diagram: Option<PreparedDiagram>, errors: Vec<DiagnosticError>, warnings: Vec<DiagnosticError>) -> Self {
        let total_errors = errors.len();
        let total_warnings = warnings.len();
        Self {
            diagram,
            errors,
            warnings,
            total_errors,
            total_warnings,
            truncated: false,
        }
    }
}

/// 解析 Drawify 源文本为 RawDiagram。
///
/// RawDiagram 是 Parser 的直接产出，可能缺少 `entity.type` 等默认属性。
/// 必须经 `prepare()` 转换为 `PreparedDiagram` 后才能传给下游。
pub fn parse(source: &str) -> Result<RawDiagram> {
    let diagram = parser::parse(source)?;
    Ok(RawDiagram(diagram))
}

/// 将 RawDiagram 规范化为下游可消费的 PreparedDiagram。
///
/// 所有 CLI / Server / WASM / Diff 入口在 validate 或 render 之前必须调用。
///
/// 依次执行：
/// 1. `apply_profile_defaults` — 补全缺失的 `entity.type`
/// 2. `expand_structure` — 派生结构语义属性（mindmap 的 `branch_slot` / `tree_depth`）
/// 3. `resolve_style_context` — 从 diagram `theme` / request 解析样式上下文
/// 4. `validate_style_decls` — StyleDecl 校验（错误阻断，警告收集）
/// 5. `materialize_styles` — 将 theme cascade 物化到 `attributes.style`
/// 6. `PreparedDiagram::new` — 封装 diagram 并调用 `LayoutPlan::resolve`
///
/// 幂等：已 prepare 的图重复调用结果不变。
pub fn prepare(raw: RawDiagram, style_request: &StyleRequest) -> Result<PrepareOutput> {
    // 1. 补全 profile 默认值（entity.type 等）
    let mut diagram = apply_profile_defaults(raw.0)?;

    // 2. 派生结构语义属性（mindmap branch_slot / tree_depth 等）
    expand_structure(&mut diagram);

    // 3. 解析 CompiledTheme
    let compiled = resolve_compiled_theme(&diagram, style_request)?;
    let style_key = diagram.diagram_type.style_key().to_string();
    let theme_ctx = crate::theme::ThemeContext::new(
        &compiled,
        &style_key,
        None,
    );

    // 4. StyleDecl 校验（错误阻断，警告收集）
    let warnings = validate_style_decls(&diagram).map_err(DrawifyError::Prepare)?;

    // 5. 物化样式到 attributes.style
    let diagram = crate::theme::materialize_diagram_styles(diagram, &theme_ctx)?;

    // 6. 封装 PreparedDiagram，内部 resolve layout_plan
    let prepared = PreparedDiagram::new(diagram);
    #[cfg(debug_assertions)]
    debug_assert_prepared_invariants(&prepared);

    Ok(PrepareOutput {
        diagram: prepared,
        warnings,
    })
}

/// 解析 + prepare，不跑语义 validate（diff / patch / 仅导出 AST 等场景）。
pub fn parse_prepare(source: &str, style_request: &StyleRequest) -> PipelineOutput {
    let (diagram_opt, mut errors, mut warnings) = parser::parse_with_diagnostics(source);

    let Some(diagram) = diagram_opt else {
        return PipelineOutput::from_parse(None, errors, warnings);
    };

    match prepare(RawDiagram(diagram), style_request) {
        Ok(output) => {
            warnings.extend(output.warnings);
            PipelineOutput::from_parse(Some(output.diagram), errors, warnings)
        }
        Err(e) => {
            errors.extend(e.into_diagnostics());
            PipelineOutput::from_parse(None, errors, warnings)
        }
    }
}

/// 解析 + prepare + validate。CLI / Server / WASM 渲染与校验的标准入口。
pub fn parse_prepare_validate(source: &str, style_request: &StyleRequest) -> PipelineOutput {
    let mut output = parse_prepare(source, style_request);
    if let Some(ref diagram) = output.diagram {
        let mut result = validation::validate(diagram);
        // 排序错误（spec §6.2）
        result.sort();
        output.errors.extend(result.errors);
        output.warnings.extend(result.warnings);
        output.total_errors += result.total_errors;
        output.total_warnings += result.total_warnings;
        if result.truncated {
            output.truncated = true;
        }
    } else {
        // 没有 diagram 时，total_errors 至少是 errors.len()
        output.total_errors = output.total_errors.max(output.errors.len());
        output.total_warnings = output.total_warnings.max(output.warnings.len());
    }
    output
}

/// Import + prepare + validate pipeline entry point.
/// Takes a pre-built Diagram (from interchange import) instead of DSL source.
pub fn import_prepare_validate(
    diagram: crate::ast::Diagram,
    style_request: &StyleRequest,
) -> PipelineOutput {
    match prepare(RawDiagram(diagram), style_request) {
        Ok(prepare_output) => {
            let mut result = validation::validate(&prepare_output.diagram);
            result.sort();
            PipelineOutput {
                diagram: Some(prepare_output.diagram),
                errors: result.errors,
                warnings: result.warnings,
                total_errors: result.total_errors,
                total_warnings: result.total_warnings,
                truncated: result.truncated,
            }
        }
        Err(e) => {
            PipelineOutput {
                diagram: None,
                errors: e.into_diagnostics(),
                warnings: vec![],
                total_errors: 1,
                total_warnings: 0,
                truncated: false,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AttributeValue, TextValue};

    #[test]
    fn parse_and_prepare_state_diagram() {
        let source = r#"diagram state {
            entity pending "待支付"
            entity processing "处理中"
            entity init "初始化" { type: initial }
            pending -> processing "提交"
        }"#;

        let raw = parse(source).unwrap();
        // RawDiagram 中 pending/processing 没有 type
        assert!(raw.inner().entities[0].attributes.standard.get("type").is_none());
        assert!(raw.inner().entities[1].attributes.standard.get("type").is_none());
        // init 有显式 type
        assert_eq!(
            raw.inner().entities[2].attributes.standard.get("type"),
            Some(&AttributeValue::String(TextValue::unquoted("initial")))
        );

        let output = prepare(raw, &StyleRequest::default()).unwrap();
        let prepared = &output.diagram;
        // prepare 后 pending/processing 补全了 type: state
        assert_eq!(
            prepared.inner().entities[0].attributes.standard.get("type"),
            Some(&AttributeValue::String(TextValue::unquoted("state")))
        );
        assert_eq!(
            prepared.inner().entities[1].attributes.standard.get("type"),
            Some(&AttributeValue::String(TextValue::unquoted("state")))
        );
        // init 的显式 type 不被覆盖
        assert_eq!(
            prepared.inner().entities[2].attributes.standard.get("type"),
            Some(&AttributeValue::String(TextValue::unquoted("initial")))
        );
        // 所有 entity 应有物化的 style
        for entity in &prepared.inner().entities {
            assert!(
                entity.attributes.style.contains_key("fill"),
                "entity '{}' should have 'fill' in style after prepare",
                entity.id
            );
        }
    }

    #[test]
    fn parse_and_prepare_flowchart_diagram() {
        let source = r#"diagram flowchart {
            entity step1 "步骤1"
            entity step2 "步骤2" { type: decision }
            step1 -> step2
        }"#;

        let raw = parse(source).unwrap();
        let output = prepare(raw, &StyleRequest::default()).unwrap();
        let prepared = &output.diagram;

        // step1 补全 type: process
        assert_eq!(
            prepared.inner().entities[0].attributes.standard.get("type"),
            Some(&AttributeValue::String(TextValue::unquoted("process")))
        );
        // step2 的显式 type 不被覆盖
        assert_eq!(
            prepared.inner().entities[1].attributes.standard.get("type"),
            Some(&AttributeValue::String(TextValue::unquoted("decision")))
        );
        // 所有 entity 应有物化的 style
        for entity in &prepared.inner().entities {
            assert!(
                entity.attributes.style.contains_key("fill"),
                "entity '{}' should have 'fill' in style after prepare",
                entity.id
            );
        }
    }

    #[test]
    fn parse_and_prepare_er_diagram_no_default_type() {
        let source = r#"diagram er {
            entity user "User"
        }"#;

        let raw = parse(source).unwrap();
        let output = prepare(raw, &StyleRequest::default()).unwrap();
        let prepared = &output.diagram;

        // ER 图没有 default_entity_type，不会补全
        assert!(prepared.inner().entities[0].attributes.standard.get("type").is_none());
        // 但仍应有物化的 style（来自 defaults.node）
        assert!(
            prepared.inner().entities[0].attributes.style.contains_key("fill"),
            "ER entity should still have 'fill' in style from defaults"
        );
    }

    #[test]
    fn prepare_with_dark_mode() {
        let source = r#"diagram flowchart {
            entity step1 "步骤1"
        }"#;

        let raw = parse(source).unwrap();
        let output = prepare(raw, &StyleRequest { theme_id: None, dark_mode: true }).unwrap();
        let prepared = &output.diagram;

        // dark 模式下应有样式（来自 clean-dark 主题）
        assert!(
            prepared.inner().entities[0].attributes.style.contains_key("fill"),
            "entity should have 'fill' in dark mode"
        );
    }

    #[test]
    fn prepare_is_idempotent() {
        let source = r#"diagram state {
            entity s1 "状态1"
        }"#;

        let raw = parse(source).unwrap();
        let output = prepare(raw, &StyleRequest::default()).unwrap();
        let prepared = &output.diagram;

        // 再次 prepare（通过构造 RawDiagram）
        let raw2 = RawDiagram(prepared.inner().clone());
        let output2 = prepare(raw2, &StyleRequest::default()).unwrap();
        let prepared2 = &output2.diagram;

        assert_eq!(
            prepared.inner().entities[0].attributes.standard,
            prepared2.inner().entities[0].attributes.standard,
        );
        assert_eq!(
            prepared.inner().entities[0].attributes.style,
            prepared2.inner().entities[0].attributes.style,
        );
    }

    #[test]
    fn prepare_satisfies_invariants() {
        let source = r#"diagram flowchart {
            entity a "A"
            entity b "B"
            a -> b
        }"#;
        let raw = parse(source).unwrap();
        let output = prepare(raw, &StyleRequest::default()).unwrap();
        assert_prepared_invariants(&output.diagram);
    }

    #[test]
    fn prepare_inline_style_not_overwritten() {
        let source = r##"diagram flowchart {
            entity step1 "步骤1" { style.fill: "#FF0000" }
        }"##;

        let raw = parse(source).unwrap();
        let output = prepare(raw, &StyleRequest::default()).unwrap();
        let prepared = &output.diagram;

        // 内联 fill 不被覆盖
        assert_eq!(
            prepared.inner().entities[0].attributes.style.get("fill"),
            Some(&AttributeValue::String(TextValue::quoted("#FF0000")))
        );
        // 但 stroke 应被物化
        assert!(
            prepared.inner().entities[0].attributes.style.contains_key("stroke"),
            "entity should have 'stroke' from cascade"
        );
    }

    #[test]
    fn parse_prepare_validate_rejects_invalid_entity_type() {
        let source = r#"diagram state {
            entity bad "Bad" { type: actor }
        }"#;
        let output = parse_prepare_validate(source, &StyleRequest::default());
        assert!(output.diagram.is_some());
        assert!(!output.errors.is_empty());
    }

    #[test]
    fn parse_prepare_validate_accepts_expanded_defaults() {
        let source = r#"diagram flowchart {
            entity step "Step"
        }"#;
        let output = parse_prepare_validate(source, &StyleRequest::default());
        assert!(output.is_valid());
    }
}
