//! 表达式/值解析：attribute value 等

use std::collections::HashMap;

use crate::ast::{is_valid_atom, AttributeValue, TextValue};
use crate::error::DiagnosticError;
use crate::dsl::lexer::TokenKind;
use crate::types::standard_attr_keys::{diagram, entity, group, relation};

use super::AttrNamespace;
use super::Parser;

impl Parser {
    // ── Attribute value ──────────────────────────────────

    pub(super) fn parse_string_attribute_value(&mut self) -> Option<AttributeValue> {
        match self.peek_kind().clone() {
            TokenKind::StringLit(s) => {
                self.advance();
                Some(AttributeValue::String(TextValue::quoted(s)))
            }
            _ => {
                self.errors.push(DiagnosticError::unexpected_token(
                    self.current().span,
                    self.peek_kind().display_name(),
                    &["string"],
                ));
                None
            }
        }
    }

    fn read_atom_segment(&mut self) -> Option<String> {
        let span = self.current().span;
        let text = match self.peek_kind().clone() {
            TokenKind::StringLit(s) => {
                self.advance();
                s
            }
            TokenKind::Ident(s) => {
                self.advance();
                s
            }
            TokenKind::Flowchart
            | TokenKind::Sequence
            | TokenKind::Architecture
            | TokenKind::State
            | TokenKind::Er
            | TokenKind::Mindmap => {
                let s = match self.peek_kind() {
                    TokenKind::Flowchart => "flowchart",
                    TokenKind::Sequence => "sequence",
                    TokenKind::Architecture => "architecture",
                    TokenKind::State => "state",
                    TokenKind::Er => "er",
                    TokenKind::Mindmap => "mindmap",
                    _ => unreachable!(),
                };
                self.advance();
                s.to_string()
            }
            _ => {
                self.errors.push(DiagnosticError::unexpected_token(
                    span,
                    self.peek_kind().display_name(),
                    &["atom", "string"],
                ));
                return None;
            }
        };

        let mut atom = text;
        while matches!(self.peek_kind(), TokenKind::Dot) {
            self.advance();
            match self.peek_kind().clone() {
                TokenKind::Ident(part) => {
                    self.advance();
                    atom.push('.');
                    atom.push_str(&part);
                }
                _ => {
                    self.errors.push(DiagnosticError::unexpected_token(
                        self.current().span,
                        self.peek_kind().display_name(),
                        &["atom segment"],
                    ));
                    return None;
                }
            }
        }
        Some(atom)
    }

    pub(super) fn parse_atom_attribute_value(&mut self) -> Option<AttributeValue> {
        let span = self.current().span;
        let text = self.read_atom_segment()?;

        if !is_valid_atom(&text) {
            self.errors.push(DiagnosticError::structure_violation(
                span,
                format!(
                    "atom 值 '{}' 不合法：须以小写字母开头，仅含小写字母、数字、下划线、连字符或点号",
                    text
                ),
            ));
            return None;
        }

        Some(AttributeValue::String(TextValue::unquoted(text)))
    }

    /// 按属性键选择值解析方式（standard 命名空间）。
    pub(super) fn parse_value_for_standard_key(&mut self, key: &str) -> Option<AttributeValue> {
        match key {
            diagram::LAYOUT | diagram::EDGE_ROUTING | diagram::GROUP_FRAME => {
                self.parse_algorithm_config_value()
            }
            diagram::DIRECTION
            | entity::TYPE
            | entity::STATUS
            | entity::SEMANTIC
            | entity::ICON
            | relation::LINE_STYLE
            | group::BORDER_STYLE
            | diagram::RENDER_STYLE
            | diagram::THEME
            | diagram::GROUP_ALIGN
            | diagram::GROUP_ARRANGEMENT => self.parse_atom_attribute_value(),
            entity::OWNER
            | entity::DESCRIPTION
            | group::COLOR
            | relation::CARDINALITY
            | diagram::TITLE => self.parse_string_attribute_value(),
            _ => self.parse_attribute_value(),
        }
    }

    /// `algo` 或 `algo { key: value, ... }`
    pub(super) fn parse_algorithm_config_value(&mut self) -> Option<AttributeValue> {
        let span = self.current().span;
        let text = self.read_atom_segment()?;

        if !is_valid_atom(&text) {
            self.errors.push(DiagnosticError::structure_violation(
                span,
                format!(
                    "atom 值 '{text}' 不合法：须以小写字母开头，仅含小写字母、数字、下划线、连字符或点号"
                ),
            ));
            return None;
        }

        if matches!(self.peek_kind(), TokenKind::LBrace) {
            let options = self.parse_algorithm_option_block();
            Some(AttributeValue::Config {
                algo: text,
                options,
            })
        } else {
            Some(AttributeValue::String(TextValue::unquoted(text)))
        }
    }

    /// 算法配置块内的 `key: value` 对（无命名空间前缀）。
    pub(super) fn parse_algorithm_option_block(&mut self) -> HashMap<String, AttributeValue> {
        use std::collections::HashMap;

        let mut options = HashMap::new();
        self.advance(); // consume '{'

        while !self.at_eof() && !matches!(self.peek_kind(), TokenKind::RBrace) {
            let Some(key) = self.expect_ident().map(|(k, _)| k) else {
                self.skip_to_next_statement();
                continue;
            };
            if !self.expect_colon() {
                continue;
            }
            if let Some(value) = self.parse_attribute_value() {
                options.insert(key, value);
            }
        }

        if matches!(self.peek_kind(), TokenKind::RBrace) {
            self.advance();
        }

        options
    }

    /// 按属性键与命名空间选择值解析方式。
    pub(super) fn parse_value_for_key(
        &mut self,
        key: &str,
        namespace: &AttrNamespace,
    ) -> Option<AttributeValue> {
        match namespace {
            AttrNamespace::Standard => self.parse_value_for_standard_key(key),
            AttrNamespace::Style => match key {
                "shape" | "label_weight" | "font_weight" => self.parse_atom_attribute_value(),
                _ => self.parse_attribute_value(),
            },
            AttrNamespace::Meta => self.parse_attribute_value(),
        }
    }

    pub(super) fn parse_attribute_value(&mut self) -> Option<AttributeValue> {
        match self.peek_kind().clone() {
            TokenKind::StringLit(s) => {
                self.advance();
                Some(AttributeValue::String(TextValue::quoted(s)))
            }
            TokenKind::NumberLit(n) => {
                self.advance();
                Some(AttributeValue::Number(n))
            }
            TokenKind::True => {
                self.advance();
                Some(AttributeValue::Boolean(true))
            }
            TokenKind::False => {
                self.advance();
                Some(AttributeValue::Boolean(false))
            }
            TokenKind::Ident(_) => {
                let span = self.current().span;
                let atom = self.read_atom_segment()?;
                if is_valid_atom(&atom) {
                    Some(AttributeValue::String(TextValue::unquoted(atom)))
                } else {
                    self.errors.push(DiagnosticError::structure_violation(
                        span,
                        format!("标识符 '{}' 不是合法的 atom 值", atom),
                    ));
                    None
                }
            }
            TokenKind::Flowchart
            | TokenKind::Sequence
            | TokenKind::Architecture
            | TokenKind::State
            | TokenKind::Er
            | TokenKind::Mindmap
            | TokenKind::NodeStyle
            | TokenKind::EdgeStyle => {
                let s = match self.peek_kind() {
                    TokenKind::Flowchart => "flowchart",
                    TokenKind::Sequence => "sequence",
                    TokenKind::Architecture => "architecture",
                    TokenKind::State => "state",
                    TokenKind::Er => "er",
                    TokenKind::Mindmap => "mindmap",
                    TokenKind::NodeStyle => "node_style",
                    TokenKind::EdgeStyle => "edge_style",
                    _ => unreachable!(),
                };
                self.advance();
                Some(AttributeValue::String(TextValue::unquoted(s.to_string())))
            }
            _ => {
                self.errors.push(DiagnosticError::unexpected_token(
                    self.current().span,
                    self.peek_kind().display_name(),
                    &["string", "number", "boolean", "atom"],
                ));
                None
            }
        }
    }
}
