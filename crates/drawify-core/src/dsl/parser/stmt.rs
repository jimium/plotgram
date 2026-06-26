//! 语句解析：entity、relation、group、attribute block、style declaration

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::error::DiagnosticError;
use crate::dsl::lexer::TokenKind;

use super::AttrNamespace;
use super::Parser;

impl Parser {
    // ── Diagram attribute ────────────────────────────────

    pub(super) fn parse_diagram_attribute(&mut self) -> Option<DiagramAttribute> {
        let (key, key_span) = self.expect_ident()?;
        if !self.expect_colon() {
            return None;
        }
        let value = self.parse_value_for_standard_key(&key)?;
        Some(DiagramAttribute {
            key,
            value,
            span: Span::new(key_span.start, self.last_end()),
        })
    }

    // ── Config block ─────────────────────────────────────

    /// 解析 `config { key: value ... }` block，将属性追加到 `diagram.attributes`。
    ///
    /// 调用时当前 token 指向 `Config` 关键字。
    pub(super) fn parse_config_block(&mut self, diagram: &mut Diagram) {
        self.advance(); // consume 'config'

        if !self.expect_lbrace() {
            return;
        }

        while !self.at_eof() && !matches!(self.peek_kind(), TokenKind::RBrace) {
            if let Some(attr) = self.parse_diagram_attribute() {
                diagram.attributes.push(attr);
            }
        }

        self.expect_rbrace();
    }

    // ── Entity ───────────────────────────────────────────

    pub(super) fn parse_entity(&mut self, group_id: Option<&Identifier>) -> Option<Entity> {
        let start = self.current().span;
        self.advance(); // consume 'entity'

        let (id_str, id_span) = self.expect_ident()?;
        let id = match Identifier::new(&id_str) {
            Ok(id) => id,
            Err(_) => {
                self.errors
                    .push(DiagnosticError::invalid_identifier(id_span, &id_str));
                self.skip_to_next_statement();
                return None;
            }
        };

        let (label, _) = self.expect_string()?;
        self.register_id(id.as_str(), id_span);

        let attributes = if matches!(self.peek_kind(), TokenKind::LBrace) {
            self.parse_attribute_block(&id)
        } else {
            AttributeMap::default()
        };

        Some(Entity {
            id,
            label,
            attributes,
            group_id: group_id.cloned(),
            span: Span::new(start.start, self.last_end()),
        })
    }

    // ── Namespaced key parsing ─────────────────────────────

    /// 解析可能带命名空间前缀的属性键。
    ///
    /// 支持三种形式：
    /// - `style.fill` → ("fill", AttrNamespace::Style)
    /// - `meta.color` → ("color", AttrNamespace::Meta)
    /// - `fill` → ("fill", AttrNamespace::Standard)
    /// - `unknown.xxx` → ("unknown", AttrNamespace::Standard)（未知前缀视为标准属性）
    fn parse_namespaced_key(&mut self, attr_start: Span) -> Option<(String, AttrNamespace)> {
        match self.peek_kind().clone() {
            TokenKind::Ident(name) => {
                self.advance();
                // Check if followed by dot (meta.xxx / style.xxx pattern)
                if matches!(self.peek_kind(), TokenKind::Dot) {
                    self.advance(); // consume '.'
                    match name.as_str() {
                        ns @ ("style" | "meta") => {
                            let namespace = match ns {
                                "style" => AttrNamespace::Style,
                                "meta" => AttrNamespace::Meta,
                                _ => unreachable!(),
                            };
                            if let Some((sub_key, _)) = self.expect_ident() {
                                Some((sub_key, namespace))
                            } else {
                                Some((name, AttrNamespace::Standard))
                            }
                        }
                        _ => {
                            // unknown.xxx → treat as standard with dotted key
                            Some((name, AttrNamespace::Standard))
                        }
                    }
                } else {
                    Some((name, AttrNamespace::Standard))
                }
            }
            _ => {
                self.errors.push(DiagnosticError::unexpected_token(
                    attr_start,
                    self.peek_kind().display_name(),
                    &["attribute name"],
                ));
                self.advance();
                None
            }
        }
    }

    // ── Attribute block ──────────────────────────────────

    pub(super) fn parse_attribute_block(&mut self, _owner_id: &Identifier) -> AttributeMap {
        let mut attrs = AttributeMap::default();
        self.advance(); // consume '{'

        while !self.at_eof() && !matches!(self.peek_kind(), TokenKind::RBrace) {
            let attr_start = self.current().span;

            // Read attribute key: plain ident, "meta" "." ident, or "style" "." ident
            let (key, namespace) = match self.parse_namespaced_key(attr_start) {
                Some(pair) => pair,
                None => continue,
            };

            if !self.expect_colon() {
                continue;
            }

            if let Some(value) = self.parse_value_for_key(&key, &namespace) {
                match namespace {
                    AttrNamespace::Standard => {
                        attrs.standard.insert(key, value);
                    }
                    AttrNamespace::Meta => {
                        attrs.meta.insert(key, value);
                    }
                    AttrNamespace::Style => {
                        attrs.style.insert_with_source(
                            key,
                            value,
                            crate::ast::StyleSource::Inline,
                        );
                    }
                }
            }
        }

        if matches!(self.peek_kind(), TokenKind::RBrace) {
            self.advance();
        }

        attrs
    }

    // ── Style declaration ─────────────────────────────────

    pub(super) fn parse_style_decl(&mut self) -> Option<StyleDecl> {
        let start = self.current().span;
        let kind = match self.peek_kind() {
            TokenKind::NodeStyle => StyleDeclKind::Node,
            TokenKind::EdgeStyle => StyleDeclKind::Edge,
            _ => unreachable!(),
        };
        self.advance(); // consume 'node_style' / 'edge_style'

        let (target, _) = self.expect_ident()?;

        if !self.expect_lbrace() {
            return None;
        }

        let mut style = HashMap::new();

        while !self.at_eof() && !matches!(self.peek_kind(), TokenKind::RBrace) {
            let attr_start = self.current().span;

            // Read attribute key (plain ident only, no namespace prefix)
            let key = match self.peek_kind().clone() {
                TokenKind::Ident(name) => {
                    self.advance();
                    name
                }
                _ => {
                    self.errors.push(DiagnosticError::unexpected_token(
                        attr_start,
                        self.peek_kind().display_name(),
                        &["attribute name"],
                    ));
                    self.advance();
                    continue;
                }
            };

            if !self.expect_colon() {
                continue;
            }

            if let Some(value) = self.parse_value_for_key(&key, &AttrNamespace::Style) {
                style.insert(key, value);
            }
        }

        if matches!(self.peek_kind(), TokenKind::RBrace) {
            self.advance();
        }

        Some(StyleDecl {
            kind,
            target,
            style,
            span: Span::new(start.start, self.last_end()),
        })
    }

    // ── Group ────────────────────────────────────────────

    pub(super) fn parse_group(
        &mut self,
        parent_id: Option<&Identifier>,
        depth: u8,
    ) -> Option<Group> {
        let start = self.current().span;
        self.advance(); // consume 'group'

        let (id_str, id_span) = self.expect_ident()?;
        let id = match Identifier::new(&id_str) {
            Ok(id) => id,
            Err(_) => {
                self.errors
                    .push(DiagnosticError::invalid_identifier(id_span, &id_str));
                self.skip_to_next_statement();
                return None;
            }
        };

        let (label, _) = self.expect_string()?;
        self.register_id(id.as_str(), id_span);

        if depth > 1 {
            self.errors.push(DiagnosticError::structure_violation(
                start,
                "group 嵌套深度超过 2 层",
            ));
        }

        if !self.expect_lbrace() {
            return None;
        }

        let mut group_attrs = AttributeMap::default();
        let mut entity_ids = Vec::new();
        let mut child_group_ids = Vec::new();
        let mut local_relations = Vec::new();

        // 记录当前 pending_entities 长度，用于后续提取本 group 的后代 entity 集合
        let entity_start = self.pending_entities.len();

        while !self.at_eof() && !matches!(self.peek_kind(), TokenKind::RBrace) {
            match self.peek_kind().clone() {
                TokenKind::Entity => {
                    if let Some(entity) = self.parse_entity(Some(&id)) {
                        entity_ids.push(entity.id.clone());
                        self.pending_entities.push(entity);
                    }
                }
                TokenKind::Group => {
                    if let Some(child) = self.parse_group(Some(&id), depth + 1) {
                        child_group_ids.push(child.id.clone());
                        self.pending_groups.push(child);
                    }
                }
                TokenKind::Ident(_) if self.lookahead_is_attribute() => {
                    // Group attribute (style, color, etc.)
                    if let Some(attr) = self.parse_diagram_attribute() {
                        group_attrs.standard.insert(attr.key, attr.value);
                    }
                }
                TokenKind::Ident(_) => {
                    // group 内的 edge 连线
                    if let Some(rel) = self.parse_relation() {
                        local_relations.push(rel);
                    }
                }
                _ => {
                    self.errors.push(DiagnosticError::syntax_error(
                        self.current().span,
                        "group 内只允许 entity、嵌套 group、属性声明和 edge 连线",
                    ));
                    self.advance();
                }
            }
        }

        if matches!(self.peek_kind(), TokenKind::RBrace) {
            self.advance();
        }

        // 验证：group 内 edge 的端点必须都在当前 group 的后代 entity 中。
        // pending_entities[entity_start..] 恰好是当前 group 的所有后代 entity
        // （直接 entity + 递归子 group 的 entity），因为子 group 解析时也向
        // 同一个 pending_entities 追加。
        let descendant_set: HashSet<String> = self.pending_entities[entity_start..]
            .iter()
            .map(|e| e.id.as_str().to_string())
            .collect();

        for rel in &local_relations {
            if !descendant_set.contains(rel.from.as_str()) {
                self.errors.push(DiagnosticError::structure_violation(
                    rel.span,
                    format!(
                        "group '{}' 内的 edge 端点 '{}' 不属于该 group 的后代 entity",
                        id.as_str(),
                        rel.from.as_str()
                    ),
                ));
            }
            if !descendant_set.contains(rel.to.as_str()) {
                self.errors.push(DiagnosticError::structure_violation(
                    rel.span,
                    format!(
                        "group '{}' 内的 edge 端点 '{}' 不属于该 group 的后代 entity",
                        id.as_str(),
                        rel.to.as_str()
                    ),
                ));
            }
        }

        self.pending_relations.extend(local_relations);

        Some(Group {
            id,
            label,
            attributes: group_attrs,
            parent_id: parent_id.cloned(),
            depth,
            entity_ids,
            child_group_ids,
            span: Span::new(start.start, self.last_end()),
        })
    }

    // ── Relation ─────────────────────────────────────────

    pub(super) fn parse_relation(&mut self) -> Option<Relation> {
        let start = self.current().span;
        let (from_str, _) = self.expect_ident()?;
        let from = Identifier::new_unchecked(&from_str);

        let arrow = match self.peek_kind() {
            TokenKind::Arrow => {
                self.advance();
                ArrowType::Active
            }
            TokenKind::DashArrow => {
                self.advance();
                ArrowType::Passive
            }
            TokenKind::BiArrow => {
                self.advance();
                ArrowType::Bidirectional
            }
            _ => {
                self.errors.push(DiagnosticError::unexpected_token(
                    self.current().span,
                    self.peek_kind().display_name(),
                    &["'->'", "'-->'", "'<->'"],
                ));
                self.skip_to_next_statement();
                return None;
            }
        };

        let (to_str, _) = self.expect_ident()?;
        let to = Identifier::new_unchecked(&to_str);

        let label = if matches!(self.peek_kind(), TokenKind::StringLit(_)) {
            let (s, _) = self.expect_string()?;
            Some(s)
        } else {
            None
        };

        // 方向标签：>"H" 为 head label（靠近 to 端），<"T" 为 tail label（靠近 from 端）
        // 两者均可选，顺序不限
        let mut head_label: Option<String> = None;
        let mut tail_label: Option<String> = None;
        loop {
            match self.peek_kind() {
                TokenKind::Gt => {
                    self.advance();
                    head_label = self.expect_string().map(|(s, _)| s);
                }
                TokenKind::Lt => {
                    self.advance();
                    tail_label = self.expect_string().map(|(s, _)| s);
                }
                _ => break,
            }
        }

        let attributes = if matches!(self.peek_kind(), TokenKind::LBrace) {
            self.parse_attribute_block(&from)
        } else {
            AttributeMap::default()
        };

        Some(Relation {
            from,
            to,
            arrow,
            label,
            head_label,
            tail_label,
            attributes,
            span: Span::new(start.start, self.last_end()),
        })
    }
}
