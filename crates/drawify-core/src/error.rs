//! Drawify 结构化错误类型
//!
//! 每个错误都有错误码、位置信息和修复建议，供 AI Agent 进行自我修正。
//!
//! 错误码体系遵循 `docs/specs/error-model.md`：
//! - `E0xx`：Error（阻止渲染）
//! - `W0xx`：Warning（不阻止渲染）
//! - `P0xx`：Patch Error（Patch 操作错误）
//! - `E1xx`：Render Error（渲染阶段错误）

use crate::ast::Span;
use serde::{Serialize, Serializer};
use std::fmt;

// ═══════════════════════════════════════════════════════════════════════
// ErrorCode 枚举注册表
// ═══════════════════════════════════════════════════════════════════════

/// 所有错误码的中央注册表。
///
/// 使用枚举而非 `String`，在编译期保证错误码一致性。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    // ── 解析错误 (Parse Errors) ──
    E001, // SyntaxError
    E002, // DuplicateId
    E006, // UnterminatedString
    E007, // InvalidIdentifier
    E008, // UnexpectedToken
    E009, // MissingDiagram
    E010, // MultipleDiagrams

    // ── 验证错误 (Validation Errors) ──
    E003, // UndefinedReference
    E004, // InvalidAttribute
    E005, // StructureViolation
    E011, // InvalidEnumValue
    E012, // GroupRelation
    E013, // SelfLoop
    E014, // DuplicateStyleDecl
    E015, // InvalidEdgeStyleRef
    E016, // StyleTypeMismatch

    // ── 渲染错误 (Render Errors) ──
    E101, // LayoutFailed
    E102, // RenderInternal

    // ── 警告 (Warnings) ──
    W001, // OrphanEntity
    W002, // RedundantAttribute
    W003, // SelfLoopWarning
    W004, // EmptyDiagram
    W005, // UnusedGroup
    W006, // UnknownStyleSelector
    W007, // UnresolvedEdgeStyle
    W008, // UnknownSemantic
    W009, // UnknownIcon

    // ── Patch 错误 (Patch Errors) ──
    P001, // PathNotFound
    P002, // PathAlreadyExists
    P003, // InvalidPatchValue
    P004, // PatchConflict
}

impl ErrorCode {
    /// 返回错误码字符串表示，如 `"E001"`。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::E001 => "E001",
            Self::E002 => "E002",
            Self::E003 => "E003",
            Self::E004 => "E004",
            Self::E005 => "E005",
            Self::E006 => "E006",
            Self::E007 => "E007",
            Self::E008 => "E008",
            Self::E009 => "E009",
            Self::E010 => "E010",
            Self::E011 => "E011",
            Self::E012 => "E012",
            Self::E013 => "E013",
            Self::E014 => "E014",
            Self::E015 => "E015",
            Self::E016 => "E016",
            Self::E101 => "E101",
            Self::E102 => "E102",
            Self::W001 => "W001",
            Self::W002 => "W002",
            Self::W003 => "W003",
            Self::W004 => "W004",
            Self::W005 => "W005",
            Self::W006 => "W006",
            Self::W007 => "W007",
            Self::W008 => "W008",
            Self::W009 => "W009",
            Self::P001 => "P001",
            Self::P002 => "P002",
            Self::P003 => "P003",
            Self::P004 => "P004",
        }
    }

    /// 返回错误码的人类可读名称，如 `"SyntaxError"`。
    pub fn name(&self) -> &'static str {
        match self {
            Self::E001 => "SyntaxError",
            Self::E002 => "DuplicateId",
            Self::E003 => "UndefinedReference",
            Self::E004 => "InvalidAttribute",
            Self::E005 => "StructureViolation",
            Self::E006 => "UnterminatedString",
            Self::E007 => "InvalidIdentifier",
            Self::E008 => "UnexpectedToken",
            Self::E009 => "MissingDiagram",
            Self::E010 => "MultipleDiagrams",
            Self::E011 => "InvalidEnumValue",
            Self::E012 => "GroupRelation",
            Self::E013 => "SelfLoop",
            Self::E014 => "DuplicateStyleDecl",
            Self::E015 => "InvalidEdgeStyleRef",
            Self::E016 => "StyleTypeMismatch",
            Self::E101 => "LayoutFailed",
            Self::E102 => "RenderInternal",
            Self::W001 => "OrphanEntity",
            Self::W002 => "RedundantAttribute",
            Self::W003 => "SelfLoopWarning",
            Self::W004 => "EmptyDiagram",
            Self::W005 => "UnusedGroup",
            Self::W006 => "UnknownStyleSelector",
            Self::W007 => "UnresolvedEdgeStyle",
            Self::W008 => "UnknownSemantic",
            Self::W009 => "UnknownIcon",
            Self::P001 => "PathNotFound",
            Self::P002 => "PathAlreadyExists",
            Self::P003 => "InvalidPatchValue",
            Self::P004 => "PatchConflict",
        }
    }

    /// 返回该错误码对应的严重级别。
    pub fn severity(&self) -> Severity {
        match self {
            Self::W001 | Self::W002 | Self::W003 | Self::W004 | Self::W005 | Self::W006
            | Self::W007 | Self::W008 | Self::W009 => Severity::Warning,
            _ => Severity::Error,
        }
    }

    /// 返回该错误码对应的类别。
    pub fn category(&self) -> Category {
        match self {
            Self::E001 | Self::E002 | Self::E006 | Self::E007 | Self::E008 | Self::E009
            | Self::E010 => Category::Parse,
            Self::E003 | Self::E004 | Self::E005 | Self::E011 | Self::E012 | Self::E013
            | Self::E014 | Self::E015 | Self::E016 | Self::W001 | Self::W002 | Self::W003
            | Self::W004 | Self::W005 | Self::W006 | Self::W007 | Self::W008 | Self::W009 => {
                Category::Validation
            }
            Self::E101 | Self::E102 => Category::Render,
            Self::P001 | Self::P002 | Self::P003 | Self::P004 => Category::Patch,
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for ErrorCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Severity / Category
// ═══════════════════════════════════════════════════════════════════════

/// 错误严重级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

/// 错误类别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Parse,
    Validation,
    Render,
    Patch,
}

// ═══════════════════════════════════════════════════════════════════════
// Suggestion / FixAction
// ═══════════════════════════════════════════════════════════════════════

/// 修复建议
#[derive(Debug, Clone, Serialize)]
pub struct Suggestion {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<FixAction>,
}

/// 修复动作
#[derive(Debug, Clone, Serialize)]
pub struct FixAction {
    pub action: String,
    pub payload: serde_json::Value,
}

// ═══════════════════════════════════════════════════════════════════════
// DiagnosticError
// ═══════════════════════════════════════════════════════════════════════

/// Drawify 结构化错误
#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticError {
    pub code: ErrorCode,
    pub severity: Severity,
    pub category: Category,
    pub message: String,
    pub location: Span,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<Suggestion>,
}

impl DiagnosticError {
    /// 用错误码自动推导 severity 和 category，构造一个最小错误。
    fn new(code: ErrorCode, span: Span, message: impl Into<String>) -> Self {
        Self {
            code,
            severity: code.severity(),
            category: code.category(),
            message: message.into(),
            location: span,
            context: None,
            suggestion: None,
        }
    }

    // ─── 解析错误 ────────────────────────────────────────

    pub fn syntax_error(span: Span, message: impl Into<String>) -> Self {
        Self::new(ErrorCode::E001, span, message)
    }

    /// 带修复建议的语法错误（用于已知的常见语法错误模式）。
    pub fn syntax_error_with_fix(
        span: Span,
        message: impl Into<String>,
        old: &str,
        new: &str,
    ) -> Self {
        let mut err = Self::new(ErrorCode::E001, span, message);
        err.suggestion = Some(Suggestion {
            text: format!("将 '{}' 替换为 '{}'", old, new),
            fix: Some(FixAction {
                action: "replace_text".into(),
                payload: serde_json::json!({ "old": old, "new": new }),
            }),
        });
        err
    }

    pub fn duplicate_id(span: Span, id: &str, first_line: usize) -> Self {
        let mut err = Self::new(
            ErrorCode::E002,
            span,
            format!("ID '{}' 重复定义（首次定义在第 {} 行）", id, first_line),
        );
        err.context = Some(serde_json::json!({
            "duplicate_id": id,
            "first_defined_at": { "line": first_line }
        }));
        err.suggestion = Some(Suggestion {
            text: format!("请将重复的 '{}' 重命名为其他名称，如 '{}_v2'", id, id),
            fix: Some(FixAction {
                action: "rename_entity".into(),
                payload: serde_json::json!({ "old_id": id, "new_id": format!("{}_v2", id) }),
            }),
        });
        err
    }

    pub fn unterminated_string(span: Span) -> Self {
        let mut err = Self::new(ErrorCode::E006, span, "字符串字面量未闭合：缺少结束的双引号");
        err.suggestion = Some(Suggestion {
            text: "请在字符串末尾添加闭合的双引号 \" ".into(),
            fix: Some(FixAction {
                action: "replace_text".into(),
                payload: serde_json::json!({ "old": "", "new": "\"" }),
            }),
        });
        err
    }

    pub fn invalid_identifier(span: Span, id: &str) -> Self {
        let suggested = id.to_lowercase().replace('-', "_");
        let mut err = Self::new(
            ErrorCode::E007,
            span,
            format!(
                "无效的标识符 '{}'：标识符只能包含小写字母、数字和下划线",
                id
            ),
        );
        err.context = Some(serde_json::json!({
            "invalid_id": id,
            "rule": "[a-z][a-z0-9_]*"
        }));
        err.suggestion = Some(Suggestion {
            text: format!("建议使用 '{}' 替代 '{}'", suggested, id),
            fix: Some(FixAction {
                action: "replace_text".into(),
                payload: serde_json::json!({ "old": id, "new": suggested }),
            }),
        });
        err
    }

    /// 标识符中使用了连字符（如 `sugiyama-v2`），DSL 标识符只允许下划线。
    /// 使用 E007（InvalidIdentifier），因为连字符标识符是无效标识符的子集。
    pub fn hyphenated_identifier(span: Span, hyphenated: &str, underscore: &str) -> Self {
        let mut err = Self::new(
            ErrorCode::E007,
            span,
            format!(
                "标识符 '{}' 中包含连字符 '-'，DSL 标识符只允许使用下划线 '_'，请改写为 '{}'",
                hyphenated, underscore
            ),
        );
        err.context = Some(serde_json::json!({
            "hyphenated": hyphenated,
            "underscore": underscore,
            "rule": "标识符只允许 [a-z][a-z0-9_]*，不允许连字符"
        }));
        err.suggestion = Some(Suggestion {
            text: format!("将 '{}' 改写为 '{}'", hyphenated, underscore),
            fix: Some(FixAction {
                action: "replace_text".into(),
                payload: serde_json::json!({ "old": hyphenated, "new": underscore }),
            }),
        });
        err
    }

    pub fn unexpected_token(span: Span, got: &str, expected: &[&str]) -> Self {
        let mut err = Self::new(
            ErrorCode::E008,
            span,
            format!("意外的 token '{}'，期望: {}", got, expected.join(", ")),
        );
        err.context = Some(serde_json::json!({
            "unexpected": got,
            "expected": expected
        }));
        // 如果只有一个期望值，提供 replace_text fix
        if expected.len() == 1 {
            err.suggestion = Some(Suggestion {
                text: format!("请使用 '{}' 替代 '{}'", expected[0], got),
                fix: Some(FixAction {
                    action: "replace_text".into(),
                    payload: serde_json::json!({ "old": got, "new": expected[0] }),
                }),
            });
        } else {
            err.suggestion = Some(Suggestion {
                text: format!("请使用以下之一: {}", expected.join(", ")),
                fix: None,
            });
        }
        err
    }

    pub fn missing_diagram(span: Span) -> Self {
        let mut err = Self::new(ErrorCode::E009, span, "文件缺少 diagram 声明");
        err.suggestion = Some(Suggestion {
            text: "请在文件开头添加 'diagram flowchart { ... }'".into(),
            fix: Some(FixAction {
                action: "replace_text".into(),
                payload: serde_json::json!({
                    "old": "",
                    "new": "diagram flowchart {\n    \n}"
                }),
            }),
        });
        err
    }

    /// 文件包含多个 diagram 声明（E010）。
    pub fn multiple_diagrams(span: Span, first_line: usize) -> Self {
        let mut err = Self::new(
            ErrorCode::E010,
            span,
            format!("文件包含多个 diagram 声明（首次声明在第 {} 行）", first_line),
        );
        err.context = Some(serde_json::json!({
            "first_defined_at": { "line": first_line }
        }));
        err.suggestion = Some(Suggestion {
            text: "一个文件只能包含一个 diagram 声明，请移除多余的 diagram 块".into(),
            fix: None,
        });
        err
    }

    // ─── 验证错误 ────────────────────────────────────────

    pub fn undefined_reference(
        span: Span,
        referenced: &str,
        available: &[String],
    ) -> Self {
        let mut err = Self::new(
            ErrorCode::E003,
            span,
            format!("关系引用了不存在的实体 '{}'", referenced),
        );
        err.context = Some(serde_json::json!({
            "referenced_entity": referenced,
            "available_entities": available
        }));
        err.suggestion = Some(Suggestion {
            text: format!(
                "请确认实体名拼写，或在图表中定义实体 '{}'",
                referenced
            ),
            fix: Some(FixAction {
                action: "add_entity".into(),
                payload: serde_json::json!({
                    "id": referenced,
                    "label": referenced,
                    "attributes": { "standard": { "type": { "$enum": "service" } }, "meta": {} }
                }),
            }),
        });
        err
    }

    pub fn invalid_attribute(
        span: Span,
        attr_name: &str,
        entity_id: &str,
        valid_attrs: &[&str],
    ) -> Self {
        let mut err = Self::new(
            ErrorCode::E004,
            span,
            format!(
                "未知属性 '{}'：不在预定义 Schema 中，且未使用 meta. 前缀",
                attr_name
            ),
        );
        err.context = Some(serde_json::json!({
            "invalid_attribute": attr_name,
            "entity_id": entity_id,
            "valid_attributes": valid_attrs
        }));
        err.suggestion = Some(Suggestion {
            text: format!("如需自定义属性，请使用 meta. 前缀：meta.{}", attr_name),
            fix: Some(FixAction {
                action: "rename_attribute".into(),
                payload: serde_json::json!({
                    "entity_id": entity_id,
                    "old_key": attr_name,
                    "new_key": format!("meta.{}", attr_name)
                }),
            }),
        });
        err
    }

    pub fn structure_violation(span: Span, message: impl Into<String>) -> Self {
        Self::new(ErrorCode::E005, span, message)
    }

    /// 带上下文和修复建议的结构违规。
    pub fn structure_violation_with_suggestion(
        span: Span,
        message: impl Into<String>,
        suggestion_text: impl Into<String>,
    ) -> Self {
        let mut err = Self::new(ErrorCode::E005, span, message);
        err.suggestion = Some(Suggestion {
            text: suggestion_text.into(),
            fix: None,
        });
        err
    }

    pub fn invalid_enum_value(
        span: Span,
        attr: &str,
        value: &str,
        valid_values: &[&str],
    ) -> Self {
        let closest = closest_match(value, valid_values);
        let mut err = Self::new(
            ErrorCode::E011,
            span,
            format!("属性 '{}' 的值 '{}' 不在合法枚举列表中", attr, value),
        );
        err.context = Some(serde_json::json!({
            "attribute": attr,
            "invalid_value": value,
            "valid_values": valid_values
        }));
        err.suggestion = closest.as_ref().map(|suggestion| {
            let text = format!("'{}' 与 '{}' 相似，是否应使用 '{}'？", value, suggestion, suggestion);
            Suggestion {
                text,
                fix: Some(FixAction {
                    action: "replace_attribute_value".into(),
                    payload: serde_json::json!({
                        "attribute": attr,
                        "old_value": value,
                        "new_value": suggestion
                    }),
                }),
            }
        });
        err
    }

    /// group 直接参与关系连线（E012）。
    pub fn group_relation(span: Span, group_id: &str) -> Self {
        let mut err = Self::new(
            ErrorCode::E012,
            span,
            format!("不允许 group '{}' 直接参与关系连线", group_id),
        );
        err.context = Some(serde_json::json!({
            "group_id": group_id
        }));
        err.suggestion = Some(Suggestion {
            text: format!(
                "请改为引用 group '{}' 内部的具体 entity，而非 group 本身",
                group_id
            ),
            fix: None,
        });
        err
    }

    /// 不允许的自环关系（E013，非 decision 类型）。
    pub fn self_loop_error(span: Span, entity_id: &str) -> Self {
        let mut err = Self::new(
            ErrorCode::E013,
            span,
            format!("实体 '{}' 存在不允许的自环关系（仅 type: decision 允许自环）", entity_id),
        );
        err.context = Some(serde_json::json!({
            "entity_id": entity_id
        }));
        err.suggestion = Some(Suggestion {
            text: "请移除自环关系，或将实体 type 改为 decision".into(),
            fix: Some(FixAction {
                action: "remove_relation".into(),
                payload: serde_json::json!({ "from": entity_id, "to": entity_id }),
            }),
        });
        err
    }

    /// 同名 node_style 或 edge_style 重复声明（E014）。
    pub fn duplicate_style_decl(
        span: Span,
        decl_kind: &str,
        selector: &str,
        first_line: usize,
    ) -> Self {
        let mut err = Self::new(
            ErrorCode::E014,
            span,
            format!(
                "重复的 {} 声明：'{}' 已在第 {} 行声明",
                decl_kind, selector, first_line
            ),
        );
        err.context = Some(serde_json::json!({
            "decl_kind": decl_kind,
            "selector": selector,
            "first_defined_at": { "line": first_line }
        }));
        err.suggestion = Some(Suggestion {
            text: "请合并两处声明，或移除重复声明".into(),
            fix: None,
        });
        err
    }

    /// relation 上使用 `style:` 引用边样式，应使用 `line_style:`（E015）。
    pub fn invalid_edge_style_ref(span: Span, attr_name: &str) -> Self {
        let mut err = Self::new(
            ErrorCode::E015,
            span,
            format!(
                "relation 上使用了 '{}' 引用边样式，应使用 'line_style:'",
                attr_name
            ),
        );
        err.context = Some(serde_json::json!({
            "invalid_attribute": attr_name,
            "correct_attribute": "line_style"
        }));
        err.suggestion = Some(Suggestion {
            text: "请将属性名改为 'line_style'".into(),
            fix: Some(FixAction {
                action: "rename_attribute".into(),
                payload: serde_json::json!({
                    "old_key": attr_name,
                    "new_key": "line_style"
                }),
            }),
        });
        err
    }

    /// 样式属性值类型不匹配（E016）。
    pub fn style_type_mismatch(
        span: Span,
        attr: &str,
        expected_type: &str,
        actual_type: &str,
    ) -> Self {
        let mut err = Self::new(
            ErrorCode::E016,
            span,
            format!(
                "样式属性 '{}' 的值类型不匹配：期望 {}，实际为 {}",
                attr, expected_type, actual_type
            ),
        );
        err.context = Some(serde_json::json!({
            "attribute": attr,
            "expected_type": expected_type,
            "actual_type": actual_type
        }));
        err.suggestion = Some(Suggestion {
            text: format!("请使用 {} 类型的值", expected_type),
            fix: None,
        });
        err
    }

    // ─── 渲染错误 ────────────────────────────────────────

    /// 布局算法无法生成有效布局（E101）。
    pub fn layout_failed(span: Span, message: impl Into<String>) -> Self {
        Self::new(ErrorCode::E101, span, message)
    }

    /// 渲染器内部错误（E102）。
    pub fn render_internal(span: Span, message: impl Into<String>) -> Self {
        Self::new(ErrorCode::E102, span, message)
    }

    // ─── 警告 ────────────────────────────────────────────

    pub fn orphan_entity(span: Span, entity_id: &str, label: &str) -> Self {
        let mut err = Self::new(
            ErrorCode::W001,
            span,
            format!("实体 '{}' 没有与任何其他实体建立关系", entity_id),
        );
        err.context = Some(serde_json::json!({
            "entity_id": entity_id,
            "entity_label": label
        }));
        err.suggestion = Some(Suggestion {
            text: "孤立的实体不会在图表中显示有意义的连接。请检查是否遗漏了关系声明".into(),
            fix: None,
        });
        err
    }

    /// 属性存在但不影响当前图表类型的渲染（W002）。
    pub fn redundant_attribute(span: Span, attr: &str, reason: &str) -> Self {
        let mut err = Self::new(
            ErrorCode::W002,
            span,
            format!("属性 '{}' 是冗余的：{}", attr, reason),
        );
        err.context = Some(serde_json::json!({
            "attribute": attr,
            "reason": reason
        }));
        err
    }

    /// 自环关系警告（W003，type=decision 时）。
    pub fn self_loop_warning(span: Span, entity_id: &str) -> Self {
        let mut err = Self::new(
            ErrorCode::W003,
            span,
            format!("实体 '{}' 存在自环关系（type=decision 允许自环）", entity_id),
        );
        err.context = Some(serde_json::json!({
            "entity_id": entity_id
        }));
        err
    }

    pub fn empty_diagram(span: Span) -> Self {
        let mut err = Self::new(ErrorCode::W004, span, "diagram 体内无任何声明");
        err.suggestion = Some(Suggestion {
            text: "请添加 entity 和 relation 声明".into(),
            fix: None,
        });
        err
    }

    /// group 已声明但内部无任何 entity（W005）。
    pub fn unused_group(span: Span, group_id: &str) -> Self {
        let mut err = Self::new(
            ErrorCode::W005,
            span,
            format!("group '{}' 已声明但内部无任何 entity", group_id),
        );
        err.context = Some(serde_json::json!({
            "group_id": group_id
        }));
        err.suggestion = Some(Suggestion {
            text: format!("请在 group '{}' 中添加 entity，或移除该 group 声明", group_id),
            fix: Some(FixAction {
                action: "remove_group".into(),
                payload: serde_json::json!({ "id": group_id }),
            }),
        });
        err
    }

    /// node_style 的 selector 不是当前 DiagramType 支持的 entity type（W006）。
    pub fn unknown_style_selector(span: Span, selector: &str) -> Self {
        let mut err = Self::new(
            ErrorCode::W006,
            span,
            format!(
                "node_style '{}' 不是当前图表支持的 entity type，可能匹配不到任何节点",
                selector
            ),
        );
        err.context = Some(serde_json::json!({
            "selector": selector
        }));
        err
    }

    /// relation 的 line_style 引用了不存在的 edge_style 声明（W007）。
    pub fn unresolved_edge_style(span: Span, relation_desc: &str, name: &str) -> Self {
        let mut err = Self::new(
            ErrorCode::W007,
            span,
            format!(
                "关系 '{}' 引用的 edge_style '{}' 未找到对应声明",
                relation_desc, name
            ),
        );
        err.context = Some(serde_json::json!({
            "relation": relation_desc,
            "edge_style": name
        }));
        err
    }

    /// 实体的 semantic 属性不在全局词表中（W008）。
    pub fn unknown_semantic(span: Span, entity_id: &str, semantic: &str) -> Self {
        let mut err = Self::new(
            ErrorCode::W008,
            span,
            format!(
                "实体 '{}' 的 semantic '{}' 不在全局词表中，将忽略图标推断",
                entity_id, semantic
            ),
        );
        err.context = Some(serde_json::json!({
            "entity_id": entity_id,
            "semantic": semantic
        }));
        err
    }

    /// 实体的 icon 属性不在图标目录中（W009）。
    pub fn unknown_icon(span: Span, entity_id: &str, icon: &str) -> Self {
        let mut err = Self::new(
            ErrorCode::W009,
            span,
            format!(
                "实体 '{}' 的 icon '{}' 不在图标目录中，将不渲染图标",
                entity_id, icon
            ),
        );
        err.context = Some(serde_json::json!({
            "entity_id": entity_id,
            "icon": icon
        }));
        err
    }

    // ─── Patch 错误 ──────────────────────────────────────

    /// Patch 目标路径不存在（P001）。
    pub fn path_not_found(span: Span, path: &str) -> Self {
        let mut err = Self::new(
            ErrorCode::P001,
            span,
            format!("Patch 目标路径不存在: {}", path),
        );
        err.context = Some(serde_json::json!({ "path": path }));
        err
    }

    /// Add 操作的目标路径已存在（P002）。
    pub fn path_already_exists(span: Span, path: &str) -> Self {
        let mut err = Self::new(
            ErrorCode::P002,
            span,
            format!("Patch 目标路径已存在: {}", path),
        );
        err.context = Some(serde_json::json!({ "path": path }));
        err.suggestion = Some(Suggestion {
            text: "如需修改已存在的路径，请使用 Modify 操作而非 Add".into(),
            fix: None,
        });
        err
    }

    /// Patch 值不符合目标路径的类型约束（P003）。
    pub fn invalid_patch_value(span: Span, path: &str, detail: &str) -> Self {
        let mut err = Self::new(
            ErrorCode::P003,
            span,
            format!("Patch 值无效 (path={}): {}", path, detail),
        );
        err.context = Some(serde_json::json!({
            "path": path,
            "detail": detail
        }));
        err
    }

    /// 多个 Patch 操作冲突（P004）。
    pub fn patch_conflict(span: Span, detail: &str) -> Self {
        let mut err = Self::new(ErrorCode::P004, span, format!("Patch 操作冲突: {}", detail));
        err.context = Some(serde_json::json!({ "detail": detail }));
        err
    }

    // ─── LSP 兼容 ────────────────────────────────────────

    /// 转换为 LSP Diagnostic 格式的 JSON。
    ///
    /// LSP 行列从 0 开始，Drawify 从 1 开始，此处自动转换。
    pub fn to_lsp(&self) -> serde_json::Value {
        let severity_num = match self.severity {
            Severity::Error => 1,
            Severity::Warning => 2,
        };
        serde_json::json!({
            "range": {
                "start": {
                    "line": self.location.start.line.saturating_sub(1),
                    "character": self.location.start.column.saturating_sub(1)
                },
                "end": {
                    "line": self.location.end.line.saturating_sub(1),
                    "character": self.location.end.column.saturating_sub(1)
                }
            },
            "severity": severity_num,
            "code": self.code.as_str(),
            "source": "drawify",
            "message": self.message
        })
    }
}

impl fmt::Display for DiagnosticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let icon = match self.severity {
            Severity::Error => "✗",
            Severity::Warning => "⚠",
        };
        // 第一行：图标 错误码 [位置] 消息
        writeln!(
            f,
            "{} {} [line {}:{}] {}",
            icon, self.code, self.location.start.line, self.location.start.column, self.message
        )?;

        // 上下文行：展示已知的关键 context 字段
        if let Some(ref ctx) = self.context {
            for line in format_context_lines(ctx) {
                writeln!(f, "  {}", line)?;
            }
        }

        // 建议行
        if let Some(ref s) = self.suggestion {
            write!(f, "  建议: {}", s.text)?;
        }
        Ok(())
    }
}

/// 将 context JSON 格式化为人类可读的行列表。
fn format_context_lines(ctx: &serde_json::Value) -> Vec<String> {
    let mut lines = Vec::new();
    let Some(obj) = ctx.as_object() else {
        return lines;
    };

    // 列表型字段
    for (key, label) in [
        ("available_entities", "可用实体"),
        ("valid_values", "合法值"),
        ("valid_attributes", "合法属性"),
        ("expected", "期望"),
    ] {
        if let Some(arr) = obj.get(key).and_then(|v| v.as_array()) {
            let items: Vec<String> = arr
                .iter()
                .map(|v| match v.as_str() {
                    Some(s) => s.to_string(),
                    None => v.to_string(),
                })
                .collect();
            if !items.is_empty() {
                lines.push(format!("{}: {}", label, items.join(", ")));
            }
        }
    }

    // 标量型字段
    for (key, label) in [
        ("referenced_entity", "引用的实体"),
        ("invalid_attribute", "无效属性"),
        ("invalid_value", "无效值"),
        ("duplicate_id", "重复的 ID"),
        ("invalid_id", "无效的 ID"),
        ("entity_id", "实体"),
        ("group_id", "分组"),
        ("selector", "选择器"),
        ("unexpected", "实际遇到"),
        ("attribute", "属性"),
        ("expected_type", "期望类型"),
        ("actual_type", "实际类型"),
        ("path", "路径"),
    ] {
        if let Some(val) = obj.get(key) {
            let val_str = match val.as_str() {
                Some(s) => s.to_string(),
                None => val.to_string(),
            };
            lines.push(format!("{}: {}", label, val_str));
        }
    }

    lines
}

/// 在候选列表中找到与输入最相似的值（Levenshtein 距离）。
fn closest_match(input: &str, candidates: &[&str]) -> Option<String> {
    let input_lower = input.to_lowercase();
    let mut best: Option<(usize, &str)> = None;
    for candidate in candidates {
        let dist = levenshtein(&input_lower, &candidate.to_lowercase());
        let max_len = input.len().max(candidate.len());
        // 仅在相似度合理时建议（距离 < 最大长度的 50%）
        if max_len > 0 && dist <= max_len / 2 {
            match best {
                None => best = Some((dist, candidate)),
                Some((bd, _)) if dist < bd => best = Some((dist, candidate)),
                _ => {}
            }
        }
    }
    best.map(|(_, s)| s.to_string())
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        curr[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

// ═══════════════════════════════════════════════════════════════════════
// ValidationResult
// ═══════════════════════════════════════════════════════════════════════

/// 错误收集上限
pub const MAX_ERRORS: usize = 20;
pub const MAX_WARNINGS: usize = 10;

/// 验证/解析结果
#[derive(Debug)]
pub struct ValidationResult {
    pub errors: Vec<DiagnosticError>,
    pub warnings: Vec<DiagnosticError>,
    /// 包含被截断的错误在内的总数
    pub total_errors: usize,
    /// 包含被截断的警告在内的总数
    pub total_warnings: usize,
    /// 是否发生了截断
    pub truncated: bool,
}

impl ValidationResult {
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            warnings: Vec::new(),
            total_errors: 0,
            total_warnings: 0,
            truncated: false,
        }
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn is_valid(&self) -> bool {
        !self.has_errors()
    }

    pub fn add_error(&mut self, err: DiagnosticError) {
        self.total_errors += 1;
        if self.errors.len() < MAX_ERRORS {
            self.errors.push(err);
        } else {
            self.truncated = true;
        }
    }

    pub fn add_warning(&mut self, warn: DiagnosticError) {
        self.total_warnings += 1;
        if self.warnings.len() < MAX_WARNINGS {
            self.warnings.push(warn);
        } else {
            self.truncated = true;
        }
    }

    pub fn merge(&mut self, other: ValidationResult) {
        for e in other.errors {
            self.add_error(e);
        }
        for w in other.warnings {
            self.add_warning(w);
        }
    }

    /// 按优先级排序错误和警告。
    ///
    /// 排序规则（spec §6.2）：
    /// 1. 解析错误（category=parse）优先
    /// 2. 结构验证错误（E003/E005/E012/E013）
    /// 3. 属性验证错误（其他 validation errors）
    /// 4. 警告
    ///
    /// 同优先级内按行号排序。
    pub fn sort(&mut self) {
        self.errors.sort_by(|a, b| {
            error_priority(a)
                .cmp(&error_priority(b))
                .then(a.location.start.line.cmp(&b.location.start.line))
                .then(a.location.start.column.cmp(&b.location.start.column))
        });
        self.warnings.sort_by(|a, b| {
            a.location
                .start
                .line
                .cmp(&b.location.start.line)
                .then(a.location.start.column.cmp(&b.location.start.column))
        });
    }
}

impl Default for ValidationResult {
    fn default() -> Self {
        Self::new()
    }
}

/// 计算错误的排序优先级（值越小越靠前）。
fn error_priority(err: &DiagnosticError) -> u8 {
    match err.category {
        Category::Parse => 0,
        Category::Validation => {
            // 结构性错误优先于属性错误
            match err.code {
                ErrorCode::E003 | ErrorCode::E005 | ErrorCode::E012 | ErrorCode::E013 => 1,
                _ => 2,
            }
        }
        Category::Render => 3,
        Category::Patch => 4,
    }
}

// ═══════════════════════════════════════════════════════════════════════
// DrawifyError（内部错误类型）
// ═══════════════════════════════════════════════════════════════════════

/// Drawify 内部错误（用于 Rust 层面 `Result` 传播）
#[derive(Debug)]
pub enum DrawifyError {
    Parse(Vec<DiagnosticError>),
    Prepare(Vec<DiagnosticError>),
    Render(Vec<DiagnosticError>),
    Patch(Vec<DiagnosticError>),
    Style(String),
}

impl DrawifyError {
    // ── 便捷构造方法 ──

    /// 从单条消息构造 Parse 错误（包装为 E001 SyntaxError）。
    pub fn parse_msg(message: impl Into<String>) -> Self {
        Self::Parse(vec![DiagnosticError::syntax_error(
            Span::dummy(),
            message,
        )])
    }

    /// 从单条消息构造 Render 内部错误（E102）。
    pub fn render_internal_msg(message: impl Into<String>) -> Self {
        Self::Render(vec![DiagnosticError::render_internal(
            Span::dummy(),
            message,
        )])
    }

    /// 从单条消息构造 Layout 失败错误（E101）。
    pub fn layout_failed_msg(message: impl Into<String>) -> Self {
        Self::Render(vec![DiagnosticError::layout_failed(
            Span::dummy(),
            message,
        )])
    }

    /// 从单条消息构造 Patch 错误（P003）。
    pub fn patch_value_msg(path: &str, detail: impl Into<String>) -> Self {
        Self::Patch(vec![DiagnosticError::invalid_patch_value(
            Span::dummy(),
            path,
            &detail.into(),
        )])
    }

    /// 提取所有 DiagnosticError（无论哪个变体）。
    pub fn into_diagnostics(self) -> Vec<DiagnosticError> {
        match self {
            Self::Parse(errs) | Self::Prepare(errs) | Self::Render(errs) | Self::Patch(errs) => {
                errs
            }
            Self::Style(msg) => vec![DiagnosticError::render_internal(Span::dummy(), msg)],
        }
    }
}

impl fmt::Display for DrawifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(errs) => write!(f, "parse error: {} error(s)", errs.len()),
            Self::Prepare(errs) => write!(f, "prepare failed: {} error(s)", errs.len()),
            Self::Render(errs) => write!(f, "render failed: {} error(s)", errs.len()),
            Self::Patch(errs) => write!(f, "patch failed: {} error(s)", errs.len()),
            Self::Style(msg) => write!(f, "style error: {msg}"),
        }
    }
}

impl std::error::Error for DrawifyError {}

pub type Result<T> = std::result::Result<T, DrawifyError>;
