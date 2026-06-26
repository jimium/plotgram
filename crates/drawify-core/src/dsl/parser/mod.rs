//! Drawify 递归下降解析器
//!
//! 将 token 流解析为 AST（Diagram），同时收集结构化错误。

pub(super) mod expr;
pub(super) mod recovery;
pub(super) mod stmt;

use crate::ast::*;
use crate::types::DiagramType;
use crate::error::{DiagnosticError, DrawifyError, Result};
use std::collections::{HashMap, HashSet};

use super::lexer::{Lexer, Token, TokenKind};

/// 属性命名空间，用于 `parse_attribute_block` 中区分 standard / meta / style。
pub(super) enum AttrNamespace {
    Standard,
    Meta,
    Style,
}

/// 解析 Drawify 源文本为 Diagram AST
///
/// 始终走 fallback 路径以收集尽可能多的错误（spec §6.1）。
/// 如果存在任何错误，返回 `Err(DrawifyError::Parse(all_errors))`。
pub fn parse(source: &str) -> Result<Diagram> {
    let (diagram, errors, _warnings) = parse_with_diagnostics(source);
    if errors.is_empty() {
        diagram.ok_or_else(|| DrawifyError::parse_msg("no diagram produced"))
    } else {
        Err(DrawifyError::Parse(errors))
    }
}

/// 解析并返回结构化诊断信息
///
/// **降级策略**：即使有错误，也尽量返回部分 AST，而不是 None。
/// 这样可以让渲染器渲染有效的部分，同时提示错误。
pub fn parse_with_diagnostics(
    source: &str,
) -> (Option<Diagram>, Vec<DiagnosticError>, Vec<DiagnosticError>) {
    let mut lexer = Lexer::new(source);
    let doc_comment = lexer.extract_doc_comment();
    let tokens = lexer.tokenize();
    let mut parser = Parser::new(tokens, lexer.errors, source, doc_comment);

    // 尝试解析，即使失败也返回部分构建的 diagram
    let diagram = parser.parse_file_fallback();

    (diagram, parser.errors, parser.warnings)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    errors: Vec<DiagnosticError>,
    warnings: Vec<DiagnosticError>,
    source: String,
    declared_ids: HashSet<String>,
    id_first_line: HashMap<String, usize>,
    /// 从 group 解析中收集的 entity，需要添加到 diagram.entities
    pending_entities: Vec<Entity>,
    /// 从 group 解析中收集的子 group
    pending_groups: Vec<Group>,
    /// 从 group 解析中收集的 relation，需要添加到 diagram.relations
    pending_relations: Vec<Relation>,
    /// 文件开头文档注释，由 Lexer 提前提取。
    doc_comment: Option<String>,
}

impl Parser {
    fn new(
        tokens: Vec<Token>,
        lexer_errors: Vec<DiagnosticError>,
        source: &str,
        doc_comment: Option<String>,
    ) -> Self {
        Self {
            tokens,
            pos: 0,
            errors: lexer_errors,
            warnings: Vec::new(),
            source: source.to_string(),
            declared_ids: HashSet::new(),
            id_first_line: HashMap::new(),
            pending_entities: Vec::new(),
            pending_groups: Vec::new(),
            pending_relations: Vec::new(),
            doc_comment,
        }
    }

    // ── Token helpers ────────────────────────────────────

    fn current(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.current().kind
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Eof)
    }

    fn advance(&mut self) -> Token {
        let tok = self.tokens[self.pos.min(self.tokens.len() - 1)].clone();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        tok
    }

    fn expect_rbrace(&mut self) -> bool {
        if matches!(self.peek_kind(), TokenKind::RBrace) {
            self.advance();
            true
        } else {
            self.errors.push(DiagnosticError::unexpected_token(
                self.current().span,
                self.peek_kind().display_name(),
                &["'}'"],
            ));
            false
        }
    }

    fn expect_ident(&mut self) -> Option<(String, Span)> {
        if let TokenKind::Ident(name) = self.peek_kind().clone() {
            let span = self.current().span;
            self.advance();
            Some((name, span))
        } else {
            self.errors.push(DiagnosticError::unexpected_token(
                self.current().span,
                self.peek_kind().display_name(),
                &["identifier"],
            ));
            None
        }
    }

    fn expect_string(&mut self) -> Option<(String, Span)> {
        if let TokenKind::StringLit(s) = self.peek_kind().clone() {
            let span = self.current().span;
            self.advance();
            Some((s, span))
        } else {
            self.errors.push(DiagnosticError::unexpected_token(
                self.current().span,
                self.peek_kind().display_name(),
                &["string"],
            ));
            None
        }
    }

    fn expect_colon(&mut self) -> bool {
        if matches!(self.peek_kind(), TokenKind::Colon) {
            self.advance();
            true
        } else {
            self.errors.push(DiagnosticError::unexpected_token(
                self.current().span,
                self.peek_kind().display_name(),
                &["':'"],
            ));
            false
        }
    }

    fn expect_lbrace(&mut self) -> bool {
        if matches!(self.peek_kind(), TokenKind::LBrace) {
            self.advance();
            true
        } else {
            self.errors.push(DiagnosticError::unexpected_token(
                self.current().span,
                self.peek_kind().display_name(),
                &["'{'"],
            ));
            false
        }
    }

    fn last_end(&self) -> Position {
        if self.pos > 0 {
            self.tokens[self.pos - 1].span.end
        } else {
            Position::new(0, 0)
        }
    }

    fn register_id(&mut self, id: &str, span: Span) {
        if self.declared_ids.contains(id) {
            let first_line = self.id_first_line.get(id).copied().unwrap_or(0);
            self.errors
                .push(DiagnosticError::duplicate_id(span, id, first_line));
        } else {
            self.declared_ids.insert(id.to_string());
            self.id_first_line
                .insert(id.to_string(), span.start.line);
        }
    }

    // ── Top-level ────────────────────────────────────────

    /// 降级解析：即使有错误，也返回部分构建的 diagram
    ///
    /// 策略：
    /// 1. 尝试正常解析
    /// 2. 如果失败（如缺少 diagram 声明），创建一个默认 diagram
    /// 3. 尽量解析文件中的 entity/relation/group
    /// 4. 返回部分 AST + 错误列表
    fn parse_file_fallback(&mut self) -> Option<Diagram> {
        let line_count = self.source.lines().count();

        // 尝试正常解析
        match self.parse_file() {
            Ok(diagram) => Some(diagram),
            Err(_) => {
                // 降级策略：创建一个默认 diagram，然后尝试解析内容
                self.build_fallback_diagram(line_count)
            }
        }
    }

    /// 构建 fallback diagram（降级模式）
    ///
    /// 当正常解析失败时，尝试从源文本中提取有效的部分：
    /// - 跳过到第一个 entity/group/relation
    /// - 尽量解析所有能解析的内容
    /// - 返回部分构建的 diagram
    fn build_fallback_diagram(&mut self, line_count: usize) -> Option<Diagram> {
        // 创建默认 diagram（使用 flowchart 类型）
        let mut diagram = Diagram::new(
            DiagramType::Flowchart,
            SourceInfo {
                file: None,
                line_count,
            },
        );
        diagram.doc_comment = self.doc_comment.clone();

        // 跳过到 diagram body 的开始（跳过 'diagram' 关键字和类型）
        self.skip_to_diagram_body();

        // 尝试解析 diagram body（即使结构不完整）
        self.parse_diagram_body_fallback(&mut diagram);

        // 即使 diagram 是空的，也返回它（而不是 None）
        Some(diagram)
    }

    /// 跳过到 diagram body 的开始
    ///
    /// 尝试找到 '{'，如果找不到，就跳过到第一个 entity/group
    fn skip_to_diagram_body(&mut self) {
        while !self.at_eof() {
            match self.peek_kind() {
                TokenKind::LBrace => {
                    self.advance(); // consume '{'
                    return;
                }
                TokenKind::Entity | TokenKind::Group | TokenKind::NodeStyle | TokenKind::EdgeStyle => {
                    return;
                }
                TokenKind::Ident(_) => {
                    // 可能是 relation 或 diagram attribute
                    return;
                }
                _ => {
                    self.advance();
                }
            }
        }
    }

    /// 降级解析 diagram body
    ///
    /// 即使结构不完整（如缺少 '}'），也尽量解析所有内容
    fn parse_diagram_body_fallback(&mut self, diagram: &mut Diagram) {
        self.parse_diagram_body_inner(diagram, false);
    }

    /// 统一的 diagram body 解析核心
    ///
    /// `strict` 为 true 时（正常模式）：遇到 RBrace 退出循环但不消费；
    /// `strict` 为 false 时（降级模式）：遇到 RBrace 消费并返回。
    fn parse_diagram_body_inner(&mut self, diagram: &mut Diagram, strict: bool) {
        let body_start = self.current().span;
        let mut seen_config = false;

        while !self.at_eof() {
            // strict 模式：遇到 RBrace 时退出循环（不消费）
            if strict && matches!(self.peek_kind(), TokenKind::RBrace) {
                break;
            }

            let span = self.current().span;

            match self.peek_kind().clone() {
                TokenKind::Config => {
                    if seen_config {
                        self.errors.push(DiagnosticError::structure_violation(
                            span,
                            "diagram 内最多只能有一个 config block",
                        ));
                        // 跳过整个 config block
                        self.advance(); // consume 'config'
                        if matches!(self.peek_kind(), TokenKind::LBrace) {
                            self.skip_config_block();
                        }
                    } else {
                        seen_config = true;
                        self.parse_config_block(diagram);
                    }
                }
                TokenKind::Entity => {
                    if let Some(entity) = self.parse_entity(None) {
                        diagram.entities.push(entity);
                    }
                }
                TokenKind::Group => {
                    if let Some(group) = self.parse_group(None, 0) {
                        let entities = std::mem::take(&mut self.pending_entities);
                        diagram.entities.extend(entities);
                        let groups = std::mem::take(&mut self.pending_groups);
                        diagram.groups.extend(groups);
                        let relations = std::mem::take(&mut self.pending_relations);
                        diagram.relations.extend(relations);
                        diagram.groups.push(group);
                    }
                }
                TokenKind::NodeStyle | TokenKind::EdgeStyle => {
                    if let Some(decl) = self.parse_style_decl() {
                        diagram.style_decls.push(decl);
                    }
                }
                TokenKind::Ident(_) => {
                    if self.lookahead_is_attribute() {
                        // body 级别 diagram 属性（如 title: "..."）
                        if let Some(attr) = self.parse_diagram_attribute() {
                            diagram.attributes.push(attr);
                        }
                    } else if let Some(rel) = self.parse_relation() {
                        diagram.relations.push(rel);
                    }
                }
                TokenKind::RBrace => {
                    // fallback 模式：消费 RBrace 并结束
                    self.advance();
                    return;
                }
                _ => {
                    self.errors.push(DiagnosticError::syntax_error(
                        span,
                        format!("意外的 token: {}", self.peek_kind().display_name()),
                    ));
                    self.advance();
                }
            }
        }

        // 空图警告
        if diagram.entities.is_empty()
            && diagram.relations.is_empty()
            && diagram.groups.is_empty()
        {
            let span = if strict {
                body_start
            } else {
                Span::new(Position::new(1, 1), Position::new(1, 1))
            };
            self.warnings.push(DiagnosticError::empty_diagram(span));
        }
    }

    fn parse_file(&mut self) -> Result<Diagram> {
        let line_count = self.source.lines().count();
        let start_span = self.current().span;

        if self.at_eof() {
            self.errors
                .push(DiagnosticError::missing_diagram(start_span));
            return Err(DrawifyError::parse_msg("missing diagram declaration"));
        }

        if !matches!(self.peek_kind(), TokenKind::Diagram) {
            self.errors.push(DiagnosticError::unexpected_token(
                self.current().span,
                self.peek_kind().display_name(),
                &["'diagram'"],
            ));
            return Err(DrawifyError::parse_msg("expected 'diagram'"));
        }
        self.advance();

        let diagram_type = self.parse_diagram_type()?;

        if !self.expect_lbrace() {
            return Err(DrawifyError::parse_msg("expected '{'"));
        }

        let mut diagram = Diagram::new(
            diagram_type,
            SourceInfo {
                file: None,
                line_count,
            },
        );
        diagram.doc_comment = self.doc_comment.clone();

        self.parse_diagram_body(&mut diagram);

        self.expect_rbrace();

        if !self.at_eof() {
            self.errors.push(DiagnosticError::syntax_error(
                self.current().span,
                "diagram 声明后不应有额外内容",
            ));
        }

        if !self.errors.is_empty() {
            return Err(DrawifyError::Parse(self.errors.clone()));
        }

        Ok(diagram)
    }

    fn parse_diagram_type(&mut self) -> Result<DiagramType> {
        let span = self.current().span;
        let dt = match self.peek_kind() {
            TokenKind::Flowchart => DiagramType::Flowchart,
            TokenKind::Sequence => DiagramType::Sequence,
            TokenKind::Architecture => DiagramType::Architecture,
            TokenKind::State => DiagramType::State,
            TokenKind::Er => DiagramType::Er,
            TokenKind::Mindmap => DiagramType::Mindmap,
            _ => {
                self.errors.push(DiagnosticError::unexpected_token(
                    span,
                    self.peek_kind().display_name(),
                    &[
                        "'flowchart'",
                        "'sequence'",
                        "'architecture'",
                        "'state'",
                        "'er'",
                        "'mindmap'",
                    ],
                ));
                return Err(DrawifyError::parse_msg("invalid diagram type"));
            }
        };
        self.advance();
        Ok(dt)
    }

    // ── Diagram body ─────────────────────────────────────

    fn parse_diagram_body(&mut self, diagram: &mut Diagram) {
        self.parse_diagram_body_inner(diagram, true);
    }

    /// 判断当前 ident 是 diagram_attribute 还是 relation
    /// ident + ':' => attribute; ident + arrow => relation
    fn lookahead_is_attribute(&self) -> bool {
        if self.pos + 1 < self.tokens.len() {
            matches!(self.tokens[self.pos + 1].kind, TokenKind::Colon)
        } else {
            false
        }
    }

    /// 跳过整个 config block（已消费 `config` 关键字，当前指向 `{`）。
    /// 用于错误恢复：第二个 config block 出现时跳过其内容。
    fn skip_config_block(&mut self) {
        if !matches!(self.peek_kind(), TokenKind::LBrace) {
            return;
        }
        self.advance(); // consume '{'
        let mut depth = 1;
        while !self.at_eof() && depth > 0 {
            match self.peek_kind() {
                TokenKind::LBrace => {
                    depth += 1;
                    self.advance();
                }
                TokenKind::RBrace => {
                    depth -= 1;
                    self.advance();
                }
                _ => {
                    self.advance();
                }
            }
        }
    }
}

#[cfg(test)]
mod parser_config_tests {
    use super::parse;
    use crate::ast::AttributeValue;

    #[test]
    fn parse_edge_routing_config_block() {
        let source = r#"
diagram flowchart {
    config {
        edge_routing: bezier {
            tension: 0.55
        }
    }
    entity a "A"
}
"#;
        let diagram = parse(source).expect("parse");
        let attr = diagram
            .attributes
            .iter()
            .find(|a| a.key == "edge_routing")
            .expect("edge_routing attr");
        match &attr.value {
            AttributeValue::Config { algo, options } => {
                assert_eq!(algo, "bezier");
                assert_eq!(
                    options.get("tension"),
                    Some(&AttributeValue::Number(0.55))
                );
            }
            other => panic!("expected config, got {other:?}"),
        }
    }

    #[test]
    fn parse_relation_with_head_and_tail_labels() {
        let source = r#"
diagram flowchart {
    a -> b "mid" >"H" <"T"
}
"#;
        let diagram = parse(source).expect("parse");
        assert_eq!(diagram.relations.len(), 1);
        let rel = &diagram.relations[0];
        assert_eq!(rel.label.as_deref(), Some("mid"));
        assert_eq!(rel.head_label.as_deref(), Some("H"));
        assert_eq!(rel.tail_label.as_deref(), Some("T"));
        // head_label/tail_label 不应出现在 attributes.standard 中
        assert!(!rel.attributes.standard.contains_key("head_label"));
        assert!(!rel.attributes.standard.contains_key("tail_label"));
    }

    #[test]
    fn parse_relation_with_only_head_label() {
        let source = r#"
diagram flowchart {
    a -> b >"only head"
}
"#;
        let diagram = parse(source).expect("parse");
        let rel = &diagram.relations[0];
        assert!(rel.label.is_none());
        assert!(rel.tail_label.is_none());
        assert_eq!(rel.head_label.as_deref(), Some("only head"));
    }

    #[test]
    fn parse_relation_no_extra_labels() {
        let source = r#"
diagram flowchart {
    a -> b "just middle"
}
"#;
        let diagram = parse(source).expect("parse");
        let rel = &diagram.relations[0];
        assert_eq!(rel.label.as_deref(), Some("just middle"));
        assert!(rel.head_label.is_none());
        assert!(rel.tail_label.is_none());
    }
}

#[cfg(test)]
mod group_edge_tests {
    use super::parse;
    use crate::ast::ArrowType;

    /// group 内 edge 正常解析：两端都是 group 直接 entity
    #[test]
    fn parse_edge_inside_group() {
        let source = r#"
diagram architecture {
    group g1 "Group 1" {
        entity a "A" { type: service }
        entity b "B" { type: service }
        a -> b
    }
}
"#;
        let diagram = parse(source).expect("parse");
        assert_eq!(diagram.entities.len(), 2);
        assert_eq!(diagram.relations.len(), 1);
        let rel = &diagram.relations[0];
        assert_eq!(rel.from.as_str(), "a");
        assert_eq!(rel.to.as_str(), "b");
        assert_eq!(rel.arrow, ArrowType::Active);
    }

    /// group 内 edge 带标签和属性
    #[test]
    fn parse_edge_inside_group_with_label() {
        let source = r#"
diagram architecture {
    group g1 "Group 1" {
        entity a "A" { type: service }
        entity b "B" { type: service }
        a -> b "create order"
    }
}
"#;
        let diagram = parse(source).expect("parse");
        assert_eq!(diagram.relations.len(), 1);
        assert_eq!(diagram.relations[0].label.as_deref(), Some("create order"));
    }

    /// group 内 edge 端点引用外部 entity → 报错
    #[test]
    fn parse_edge_inside_group_endpoint_outside() {
        let source = r#"
diagram architecture {
    entity outside "Outside" { type: service }
    group g1 "Group 1" {
        entity a "A" { type: service }
        a -> outside
    }
}
"#;
        let result = parse(source);
        assert!(result.is_err(), "should fail with endpoint outside group");
    }

    /// 嵌套 group：父 group 内 edge 引用子 group 的 entity → 允许
    #[test]
    fn parse_edge_in_parent_group_referencing_child_entity() {
        let source = r#"
diagram architecture {
    group g1 "Parent" {
        entity top "Top" { type: service }
        group g2 "Child" {
            entity inner "Inner" { type: service }
        }
        top -> inner
    }
}
"#;
        let diagram = parse(source).expect("parse");
        assert_eq!(diagram.relations.len(), 1);
        assert_eq!(diagram.relations[0].from.as_str(), "top");
        assert_eq!(diagram.relations[0].to.as_str(), "inner");
    }

    /// 子 group 内 edge 引用父 group 的 entity → 报错
    #[test]
    fn parse_edge_in_child_group_referencing_parent_entity() {
        let source = r#"
diagram architecture {
    group g1 "Parent" {
        entity top "Top" { type: service }
        group g2 "Child" {
            entity inner "Inner" { type: service }
            inner -> top
        }
    }
}
"#;
        let result = parse(source);
        assert!(result.is_err(), "should fail: child edge references parent entity");
    }

    /// group 内 edge 和顶层 edge 混合
    #[test]
    fn parse_mixed_group_and_top_level_edges() {
        let source = r#"
diagram architecture {
    group g1 "Group 1" {
        entity a "A" { type: service }
        entity b "B" { type: service }
        a -> b "internal"
    }
    group g2 "Group 2" {
        entity c "C" { type: service }
    }
    b -> c "cross-group"
}
"#;
        let diagram = parse(source).expect("parse");
        assert_eq!(diagram.relations.len(), 2);
        // group 内 edge 和顶层 edge 都应出现在 diagram.relations 中
        let has_internal = diagram.relations.iter().any(|r| {
            r.from.as_str() == "a" && r.to.as_str() == "b" && r.label.as_deref() == Some("internal")
        });
        let has_cross = diagram.relations.iter().any(|r| {
            r.from.as_str() == "b" && r.to.as_str() == "c" && r.label.as_deref() == Some("cross-group")
        });
        assert!(has_internal, "internal edge should exist");
        assert!(has_cross, "cross-group edge should exist");
    }

    /// 被动箭头 --> 在 group 内也可用
    #[test]
    fn parse_passive_edge_inside_group() {
        let source = r#"
diagram architecture {
    group g1 "Group 1" {
        entity a "A" { type: service }
        entity b "B" { type: service }
        b --> a "callback"
    }
}
"#;
        let diagram = parse(source).expect("parse");
        assert_eq!(diagram.relations.len(), 1);
        assert_eq!(diagram.relations[0].arrow, ArrowType::Passive);
    }
}
