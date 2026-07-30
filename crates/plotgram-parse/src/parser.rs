//! Parser: tokens → [`crate::ast::FileAst`].
//!
//! Recursive descent parser for Plotgram DSL 2.6 (dsl-spec §11 BNF).

use std::collections::HashSet;

use plotgram_model::attr::{AttrMap, AttrValue};
use plotgram_model::graph::Arrow;

use crate::ast::*;
use crate::error::ParseError;
use crate::lexer::{Lexer, Token, TokenKind};

/// Reserved words that cannot be used as identifiers (dsl-spec §10).
/// They remain valid as atom *values* (e.g. `profile: flowchart`).
const RESERVED_WORDS: &[&str] = &[
    "diagram", "node", "group",
    "flowchart", "sequence", "architecture", "state", "er", "mindmap",
    "true", "false",
];

/// Parse a full `.pgm` source into an AST.
pub fn parse_file(source: &str) -> Result<FileAst, ParseError> {
    let mut lexer = Lexer::new(source);
    let doc_comment = lexer.extract_doc_comment();
    let tokens = lexer.tokenize()?;
    let mut parser = Parser::new(tokens);
    let diagram = parser.parse_diagram()?;
    Ok(FileAst {
        doc_comment,
        diagram,
    })
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    /// Declared node/group ids for uniqueness checking.
    declared_ids: HashSet<String>,
    /// First declaration line per id (for duplicate error messages).
    id_lines: Vec<(String, u32)>,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            declared_ids: HashSet::new(),
            id_lines: Vec::new(),
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

    fn expect(&mut self, expected: &TokenKind) -> Result<Token, ParseError> {
        if std::mem::discriminant(self.peek_kind()) == std::mem::discriminant(expected) {
            Ok(self.advance())
        } else {
            let tok = self.current();
            Err(ParseError::syntax(
                tok.line,
                tok.column,
                format!("expected {}, found {}", expected.display_name(), self.peek_kind().display_name()),
            ))
        }
    }

    fn expect_ident(&mut self) -> Result<(String, u32, u32), ParseError> {
        match self.peek_kind().clone() {
            TokenKind::Ident(name) => {
                // §10: reserved words cannot be identifiers
                if RESERVED_WORDS.contains(&name.as_str()) {
                    let tok = self.current();
                    return Err(ParseError::syntax(
                        tok.line,
                        tok.column,
                        format!("`{name}` is a reserved word and cannot be used as an identifier"),
                    ));
                }
                let tok = self.advance();
                Ok((name, tok.line, tok.column))
            }
            _ => {
                let tok = self.current();
                Err(ParseError::syntax(
                    tok.line,
                    tok.column,
                    format!("expected identifier, found {}", self.peek_kind().display_name()),
                ))
            }
        }
    }

    /// Read an atom value: Ident or Atom token (both are valid atoms in value position).
    fn expect_atom(&mut self) -> Result<(String, u32, u32), ParseError> {
        match self.peek_kind().clone() {
            TokenKind::Ident(name) => {
                let tok = self.advance();
                Ok((name, tok.line, tok.column))
            }
            TokenKind::Atom(name) => {
                let tok = self.advance();
                Ok((name, tok.line, tok.column))
            }
            // Keywords can appear as atom values (e.g. profile: flowchart)
            TokenKind::Diagram | TokenKind::Node | TokenKind::Group
            | TokenKind::True | TokenKind::False => {
                let (name, line, col) = {
                    let tok = self.current();
                    let n = match tok.kind {
                        TokenKind::Diagram => "diagram",
                        TokenKind::Node => "node",
                        TokenKind::Group => "group",
                        TokenKind::True => "true",
                        TokenKind::False => "false",
                        _ => unreachable!(),
                    };
                    (n.to_string(), tok.line, tok.column)
                };
                self.advance();
                Ok((name, line, col))
            }
            _ => {
                let tok = self.current();
                Err(ParseError::syntax(
                    tok.line,
                    tok.column,
                    format!("expected atom, found {}", self.peek_kind().display_name()),
                ))
            }
        }
    }

    fn expect_string(&mut self) -> Result<(String, u32, u32), ParseError> {
        match self.peek_kind().clone() {
            TokenKind::StringLit(s) => {
                let tok = self.advance();
                Ok((s, tok.line, tok.column))
            }
            _ => {
                let tok = self.current();
                Err(ParseError::syntax(
                    tok.line,
                    tok.column,
                    format!("expected string, found {}", self.peek_kind().display_name()),
                ))
            }
        }
    }

    /// Lookahead: is the next token a colon? (used to distinguish attrs from edges)
    fn lookahead_is_colon(&self) -> bool {
        if self.pos + 1 < self.tokens.len() {
            matches!(self.tokens[self.pos + 1].kind, TokenKind::Colon)
        } else {
            false
        }
    }

    fn register_id(&mut self, id: &str, line: u32) -> Result<(), ParseError> {
        if self.declared_ids.contains(id) {
            let first_line = self.id_lines.iter()
                .find(|(name, _)| name == id)
                .map(|(_, l)| *l)
                .unwrap_or(0);
            return Err(ParseError::DuplicateId {
                id: id.to_string(),
                first_line,
            });
        }
        self.declared_ids.insert(id.to_string());
        self.id_lines.push((id.to_string(), line));
        Ok(())
    }

    // ── Diagram ──────────────────────────────────────────

    fn parse_diagram(&mut self) -> Result<DiagramAst, ParseError> {
        self.expect(&TokenKind::Diagram)?;
        self.expect(&TokenKind::LBrace)?;

        let mut attrs = AttrMap::new();
        let mut layout: Option<AlgorithmConfigAst> = None;
        let mut edge_routing: Option<AlgorithmConfigAst> = None;
        let mut items: Vec<DiagramItem> = Vec::new();

        while !self.at_eof() && !matches!(self.peek_kind(), TokenKind::RBrace) {
            match self.peek_kind().clone() {
                TokenKind::Node => {
                    items.push(DiagramItem::Node(self.parse_node()?));
                }
                TokenKind::Group => {
                    items.push(DiagramItem::Group(self.parse_group()?));
                }
                TokenKind::Ident(_) | TokenKind::Atom(_) if self.lookahead_is_colon() => {
                    let (key, algo_config, plain_value) = self.parse_diagram_attr_special()?;
                    if let Some(config) = algo_config {
                        match key.as_str() {
                            "layout" => {
                                if layout.is_some() {
                                    return Err(ParseError::DuplicateAttr {
                                        key: "layout".into(),
                                        context: "diagram".into(),
                                    });
                                }
                                layout = Some(config);
                            }
                            "edge_routing" => {
                                if edge_routing.is_some() {
                                    return Err(ParseError::DuplicateAttr {
                                        key: "edge_routing".into(),
                                        context: "diagram".into(),
                                    });
                                }
                                edge_routing = Some(config);
                            }
                            _ => unreachable!(),
                        }
                    } else if let Some(value) = plain_value {
                        if attrs.contains_key(&key) {
                            return Err(ParseError::DuplicateAttr {
                                key: key.clone(),
                                context: "diagram".into(),
                            });
                        }
                        attrs.insert(key, value);
                    }
                }
                TokenKind::Ident(_) | TokenKind::At => {
                    // Edge declaration
                    items.push(DiagramItem::Edge(self.parse_edge()?));
                }
                _ => {
                    let tok = self.current();
                    return Err(ParseError::syntax(
                        tok.line,
                        tok.column,
                        format!("unexpected {} in diagram body", self.peek_kind().display_name()),
                    ));
                }
            }
        }

        self.expect(&TokenKind::RBrace)?;

        if !self.at_eof() {
            let tok = self.current();
            return Err(ParseError::syntax(
                tok.line,
                tok.column,
                "unexpected content after diagram closing '}'",
            ));
        }

        Ok(DiagramAst {
            attrs,
            layout,
            edge_routing,
            items,
        })
    }

    /// Parse diagram attribute with special algorithm_config handling for layout/edge_routing.
    fn parse_diagram_attr_special(&mut self) -> Result<(String, Option<AlgorithmConfigAst>, Option<AttrValue>), ParseError> {
        let key = self.parse_attribute_key()?;
        self.expect(&TokenKind::Colon)?;

        match key.as_str() {
            "layout" | "edge_routing" => {
                let config = self.parse_algorithm_config_value()?;
                Ok((key, Some(config), None))
            }
            _ => {
                let value = self.parse_attribute_value()?;
                Ok((key, None, Some(value)))
            }
        }
    }

    // ── Node ─────────────────────────────────────────────

    fn parse_node(&mut self) -> Result<NodeAst, ParseError> {
        self.advance(); // consume 'node'
        let (id, line, _col) = self.expect_ident()?;
        self.register_id(&id, line)?;

        let mut attrs = AttrMap::new();

        // Positional sugar: node <id> [string [atom [atom]]] [{ ... }]
        // §5.5.2: if next token is string → positional label/archetype/icon
        // Atoms must be on the SAME LINE as the string (avoids ambiguity with next stmt).
        if matches!(self.peek_kind(), TokenKind::StringLit(_)) {
            let (label, str_line, _) = self.expect_string()?;
            if !label.is_empty() {
                attrs.insert("label".to_string(), AttrValue::Str(label));
            }
            // Optional archetype atom (same line as string)
            if self.is_atom_start_same_line(str_line) {
                let (archetype, _, _) = self.expect_atom()?;
                attrs.insert("archetype".to_string(), AttrValue::Atom(archetype));
                // Optional icon atom (same line as string)
                if self.is_atom_start_same_line(str_line) {
                    let (icon, _, _) = self.expect_atom()?;
                    attrs.insert("icon".to_string(), AttrValue::Atom(icon));
                }
            }
        } else if self.is_atom_start() {
            let tok = self.current();
            return Err(ParseError::syntax(
                tok.line,
                tok.column,
                format!(
                    "node `{id}`: bare atom after id is not allowed; \
                     positional sugar requires a string label first \
                     (e.g. `node {id} \"label\" archetype`) (dsl-spec §5.5.3)"
                ),
            ));
        }

        // Optional attribute block
        if matches!(self.peek_kind(), TokenKind::LBrace) {
            let block_attrs = self.parse_attribute_block(&format!("node {id}"))?;
            // Merge: check conflicts between positional sugar and block
            for (k, v) in block_attrs {
                if attrs.contains_key(&k) {
                    return Err(ParseError::DuplicateAttr {
                        key: k,
                        context: format!("node {id}"),
                    });
                }
                attrs.insert(k, v);
            }
        }

        Ok(NodeAst { id, attrs })
    }

    /// Check if current token can start an atom (for positional sugar).
    fn is_atom_start(&self) -> bool {
        matches!(
            self.peek_kind(),
            TokenKind::Ident(_) | TokenKind::Atom(_)
        )
    }

    /// Check if current token is an atom AND on the same line as `line`.
    /// Prevents positional sugar from greedily consuming atoms on the next line.
    fn is_atom_start_same_line(&self, line: u32) -> bool {
        self.is_atom_start() && self.current().line == line
    }

    // ── Group ────────────────────────────────────────────

    fn parse_group(&mut self) -> Result<GroupAst, ParseError> {
        self.advance(); // consume 'group'
        let (id, line, _col) = self.expect_ident()?;
        self.register_id(&id, line)?;

        let mut attrs = AttrMap::new();

        // Positional label sugar: group <id> [string] { ... }
        if matches!(self.peek_kind(), TokenKind::StringLit(_)) {
            let (label, _, _) = self.expect_string()?;
            if !label.is_empty() {
                attrs.insert("label".to_string(), AttrValue::Str(label));
            }
        }

        self.expect(&TokenKind::LBrace)?;

        let mut items: Vec<DiagramItem> = Vec::new();

        while !self.at_eof() && !matches!(self.peek_kind(), TokenKind::RBrace) {
            match self.peek_kind().clone() {
                TokenKind::Node => {
                    items.push(DiagramItem::Node(self.parse_node()?));
                }
                TokenKind::Group => {
                    items.push(DiagramItem::Group(self.parse_group()?));
                }
                TokenKind::Ident(_) | TokenKind::Atom(_) if self.lookahead_is_colon() => {
                    // Group-level attribute (with algorithm_config support for layout/edge_routing)
                    let (key, value) = self.parse_group_attribute()?;
                    if attrs.contains_key(&key) {
                        return Err(ParseError::DuplicateAttr {
                            key: key.clone(),
                            context: format!("group {id}"),
                        });
                    }
                    attrs.insert(key, value);
                }
                TokenKind::Ident(_) | TokenKind::At => {
                    // Edge within group
                    items.push(DiagramItem::Edge(self.parse_edge()?));
                }
                _ => {
                    let tok = self.current();
                    return Err(ParseError::syntax(
                        tok.line,
                        tok.column,
                        format!("unexpected {} in group body", self.peek_kind().display_name()),
                    ));
                }
            }
        }

        self.expect(&TokenKind::RBrace)?;

        Ok(GroupAst { id, attrs, items })
    }

    // ── Edge ─────────────────────────────────────────────

    fn parse_edge(&mut self) -> Result<EdgeAst, ParseError> {
        let source = self.parse_endpoint()?;
        let arrow = self.parse_arrow()?;
        let target = self.parse_endpoint()?;

        let mut attrs = AttrMap::new();

        // Positional label sugar: src arrow tgt [string] [{ ... }]
        if matches!(self.peek_kind(), TokenKind::StringLit(_)) {
            let (label, _, _) = self.expect_string()?;
            if !label.is_empty() {
                attrs.insert("label".to_string(), AttrValue::Str(label));
            }
        }

        // Optional attribute block
        if matches!(self.peek_kind(), TokenKind::LBrace) {
            let block_attrs = self.parse_attribute_block("edge")?;
            for (k, v) in block_attrs {
                // §7.5.2: arrow / endpoints are syntax-only, never attribute keys
                if matches!(k.as_str(), "arrow" | "source" | "target") {
                    return Err(ParseError::Semantic(format!(
                        "edge attribute `{k}` is not allowed: arrow and endpoints are \
                         expressed in edge syntax only (dsl-spec §7.5.2)"
                    )));
                }
                if attrs.contains_key(&k) {
                    return Err(ParseError::DuplicateAttr {
                        key: k,
                        context: "edge".into(),
                    });
                }
                attrs.insert(k, v);
            }
        }

        Ok(EdgeAst {
            source,
            target,
            arrow,
            attrs,
        })
    }

    fn parse_endpoint(&mut self) -> Result<EndpointAst, ParseError> {
        match self.peek_kind().clone() {
            TokenKind::At => {
                self.advance(); // consume '@'
                let (id, _, _) = self.expect_ident()?;
                Ok(EndpointAst::GroupFrame(id))
            }
            TokenKind::Ident(id) => {
                self.advance();
                Ok(EndpointAst::Node(id))
            }
            _ => {
                let tok = self.current();
                Err(ParseError::syntax(
                    tok.line,
                    tok.column,
                    format!("expected endpoint (identifier or @group), found {}", self.peek_kind().display_name()),
                ))
            }
        }
    }

    fn parse_arrow(&mut self) -> Result<Arrow, ParseError> {
        match self.peek_kind() {
            TokenKind::Arrow => {
                self.advance();
                Ok(Arrow::Forward)
            }
            TokenKind::DashArrow => {
                self.advance();
                Ok(Arrow::Response)
            }
            TokenKind::BiArrow => {
                self.advance();
                Ok(Arrow::Bidirectional)
            }
            _ => {
                let tok = self.current();
                Err(ParseError::syntax(
                    tok.line,
                    tok.column,
                    format!("expected arrow ('->', '-->', '<->'), found {}", self.peek_kind().display_name()),
                ))
            }
        }
    }

    // ── Attributes ───────────────────────────────────────

    /// Parse a single `key: value` attribute pair.
    fn parse_attribute(&mut self) -> Result<(String, AttrValue), ParseError> {
        let key = self.parse_attribute_key()?;
        self.expect(&TokenKind::Colon)?;
        let value = self.parse_attribute_value()?;
        Ok((key, value))
    }

    /// Parse a group-level attribute. Handles `layout`/`edge_routing` with
    /// algorithm_config syntax (`atom [{...}]`); stores algorithm name as Atom
    /// (options parsed but discarded — group layout is `planned`, engine doesn't read).
    fn parse_group_attribute(&mut self) -> Result<(String, AttrValue), ParseError> {
        let key = self.parse_attribute_key()?;
        self.expect(&TokenKind::Colon)?;

        match key.as_str() {
            "layout" | "edge_routing" => {
                let config = self.parse_algorithm_config_value()?;
                // Store algorithm name; options discarded (planned, §14.5)
                Ok((key, AttrValue::Atom(config.name)))
            }
            _ => {
                let value = self.parse_attribute_value()?;
                Ok((key, value))
            }
        }
    }

    /// Parse attribute key: plain ident, or `style.xxx` / `meta.xxx` (lexed as Atom).
    fn parse_attribute_key(&mut self) -> Result<String, ParseError> {
        match self.peek_kind().clone() {
            TokenKind::Ident(name) => {
                if RESERVED_WORDS.contains(&name.as_str()) {
                    let tok = self.current();
                    return Err(ParseError::syntax(
                        tok.line,
                        tok.column,
                        format!("`{name}` is a reserved word and cannot be used as an attribute key"),
                    ));
                }
                self.advance();
                Ok(name)
            }
            TokenKind::Atom(name) => {
                self.advance();
                // Validate namespace prefix
                if name.starts_with("style.") || name.starts_with("meta.") {
                    Ok(name)
                } else {
                    // Atom with dots/hyphens in key position — treat as plain key
                    Ok(name)
                }
            }
            _ => {
                let tok = self.current();
                Err(ParseError::syntax(
                    tok.line,
                    tok.column,
                    format!("expected attribute key, found {}", self.peek_kind().display_name()),
                ))
            }
        }
    }

    /// Parse attribute value: string | atom | number | boolean | algorithm_config.
    fn parse_attribute_value(&mut self) -> Result<AttrValue, ParseError> {
        match self.peek_kind().clone() {
            TokenKind::StringLit(s) => {
                self.advance();
                Ok(AttrValue::Str(s))
            }
            TokenKind::NumberLit(n) => {
                self.advance();
                Ok(AttrValue::Num(n))
            }
            TokenKind::True => {
                self.advance();
                Ok(AttrValue::Bool(true))
            }
            TokenKind::False => {
                self.advance();
                Ok(AttrValue::Bool(false))
            }
            TokenKind::Ident(_) | TokenKind::Atom(_) => {
                let (atom, _, _) = self.expect_atom()?;
                // algorithm_config blocks (`atom { ... }`) are only legal for
                // diagram/group-level `layout:` / `edge_routing:`, which go through
                // parse_algorithm_config_value; generic values are plain atoms.
                Ok(AttrValue::Atom(atom))
            }
            // Keywords as atom values (e.g. profile: flowchart)
            TokenKind::Diagram | TokenKind::Node | TokenKind::Group => {
                let (atom, _, _) = self.expect_atom()?;
                Ok(AttrValue::Atom(atom))
            }
            _ => {
                let tok = self.current();
                Err(ParseError::syntax(
                    tok.line,
                    tok.column,
                    format!("expected attribute value, found {}", self.peek_kind().display_name()),
                ))
            }
        }
    }

    fn skip_optional_commas(&mut self) {
        while matches!(self.peek_kind(), TokenKind::Comma) {
            self.advance();
        }
    }

    /// Parse an attribute block `{ (key: value [,])* }`.
    fn parse_attribute_block(&mut self, context: &str) -> Result<AttrMap, ParseError> {
        self.expect(&TokenKind::LBrace)?;
        let mut attrs = AttrMap::new();

        while !self.at_eof() && !matches!(self.peek_kind(), TokenKind::RBrace) {
            let (key, value) = self.parse_attribute()?;
            if attrs.contains_key(&key) {
                return Err(ParseError::DuplicateAttr {
                    key: key.clone(),
                    context: context.to_string(),
                });
            }
            attrs.insert(key, value);
            self.skip_optional_commas();
        }

        self.expect(&TokenKind::RBrace)?;
        Ok(attrs)
    }

    /// Parse algorithm config value at diagram level: `atom [{ option_pair* }]`.
    fn parse_algorithm_config_value(&mut self) -> Result<AlgorithmConfigAst, ParseError> {
        let (name, _, _) = self.expect_atom()?;
        let mut options = AttrMap::new();

        if matches!(self.peek_kind(), TokenKind::LBrace) {
            self.advance(); // consume '{'
            while !self.at_eof() && !matches!(self.peek_kind(), TokenKind::RBrace) {
                let (key, value) = self.parse_attribute()?;
                options.insert(key, value);
                self.skip_optional_commas();
            }
            self.expect(&TokenKind::RBrace)?;
        }

        Ok(AlgorithmConfigAst { name, options })
    }

}


#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(source: &str) -> FileAst {
        parse_file(source).unwrap_or_else(|e| panic!("parse failed: {e}\nsource:\n{source}"))
    }

    fn parse_err(source: &str) -> ParseError {
        parse_file(source).unwrap_err()
    }

    #[test]
    fn minimal_diagram() {
        let ast = parse_ok("diagram { node a {} node b {} a -> b }");
        assert_eq!(ast.diagram.items.len(), 3);
        assert!(ast.diagram.layout.is_none());
    }

    #[test]
    fn diagram_with_profile_and_attrs() {
        let ast = parse_ok(r#"diagram {
            profile: flowchart
            title: "Test"
            theme: common.clean-light
            node a { label: "A" }
        }"#);
        assert_eq!(
            ast.diagram.attrs.get("profile"),
            Some(&AttrValue::Atom("flowchart".into()))
        );
        assert_eq!(
            ast.diagram.attrs.get("title"),
            Some(&AttrValue::Str("Test".into()))
        );
        assert_eq!(
            ast.diagram.attrs.get("theme"),
            Some(&AttrValue::Atom("common.clean-light".into()))
        );
    }

    #[test]
    fn diagram_with_algorithm_config() {
        let ast = parse_ok("diagram { layout: hierarchical { direction: top-to-bottom } }");
        let layout = ast.diagram.layout.unwrap();
        assert_eq!(layout.name, "hierarchical");
        assert_eq!(
            layout.options.get("direction"),
            Some(&AttrValue::Atom("top-to-bottom".into()))
        );
    }

    #[test]
    fn node_positional_sugar_label_only() {
        let ast = parse_ok(r#"diagram { node login "用户登录" }"#);
        match &ast.diagram.items[0] {
            DiagramItem::Node(n) => {
                assert_eq!(n.id, "login");
                assert_eq!(n.attrs.get("label"), Some(&AttrValue::Str("用户登录".into())));
            }
            _ => panic!("expected node"),
        }
    }

    #[test]
    fn node_positional_sugar_label_archetype_icon() {
        let ast = parse_ok(r#"diagram { node db "用户库" database mysql }"#);
        match &ast.diagram.items[0] {
            DiagramItem::Node(n) => {
                assert_eq!(n.attrs.get("label"), Some(&AttrValue::Str("用户库".into())));
                assert_eq!(n.attrs.get("archetype"), Some(&AttrValue::Atom("database".into())));
                assert_eq!(n.attrs.get("icon"), Some(&AttrValue::Atom("mysql".into())));
            }
            _ => panic!("expected node"),
        }
    }

    #[test]
    fn node_empty_string_no_label() {
        let ast = parse_ok(r#"diagram { node spacer "" database }"#);
        match &ast.diagram.items[0] {
            DiagramItem::Node(n) => {
                assert!(!n.attrs.contains_key("label"));
                assert_eq!(n.attrs.get("archetype"), Some(&AttrValue::Atom("database".into())));
            }
            _ => panic!("expected node"),
        }
    }

    #[test]
    fn node_sugar_plus_block() {
        let ast = parse_ok(r#"diagram { node db "库" database { variant: primary } }"#);
        match &ast.diagram.items[0] {
            DiagramItem::Node(n) => {
                assert_eq!(n.attrs.get("label"), Some(&AttrValue::Str("库".into())));
                assert_eq!(n.attrs.get("archetype"), Some(&AttrValue::Atom("database".into())));
                assert_eq!(n.attrs.get("variant"), Some(&AttrValue::Atom("primary".into())));
            }
            _ => panic!("expected node"),
        }
    }

    #[test]
    fn node_sugar_conflict_error() {
        let err = parse_err(r#"diagram { node db "库" { label: "冲突" } }"#);
        assert!(matches!(err, ParseError::DuplicateAttr { .. }));
    }

    #[test]
    fn group_with_label_sugar() {
        let ast = parse_ok(r#"diagram { group auth "认证" { node a {} } }"#);
        match &ast.diagram.items[0] {
            DiagramItem::Group(g) => {
                assert_eq!(g.id, "auth");
                assert_eq!(g.attrs.get("label"), Some(&AttrValue::Str("认证".into())));
                assert_eq!(g.items.len(), 1);
            }
            _ => panic!("expected group"),
        }
    }

    #[test]
    fn group_canonical() {
        let ast = parse_ok(r#"diagram {
            group compute {
                label: "计算层"
                variant: muted
                node spark { label: "Spark" }
            }
        }"#);
        match &ast.diagram.items[0] {
            DiagramItem::Group(g) => {
                assert_eq!(g.attrs.get("label"), Some(&AttrValue::Str("计算层".into())));
                assert_eq!(g.attrs.get("variant"), Some(&AttrValue::Atom("muted".into())));
            }
            _ => panic!("expected group"),
        }
    }

    #[test]
    fn edge_with_label_sugar() {
        let ast = parse_ok(r#"diagram { node a {} node b {} a -> b "请求" }"#);
        match &ast.diagram.items[2] {
            DiagramItem::Edge(e) => {
                assert_eq!(e.attrs.get("label"), Some(&AttrValue::Str("请求".into())));
                assert_eq!(e.arrow, Arrow::Forward);
            }
            _ => panic!("expected edge"),
        }
    }

    #[test]
    fn edge_with_block() {
        let ast = parse_ok(r#"diagram {
            node a {} node b {}
            a --> b { label: "响应" variant: secondary }
        }"#);
        match &ast.diagram.items[2] {
            DiagramItem::Edge(e) => {
                assert_eq!(e.arrow, Arrow::Response);
                assert_eq!(e.attrs.get("label"), Some(&AttrValue::Str("响应".into())));
                assert_eq!(e.attrs.get("variant"), Some(&AttrValue::Atom("secondary".into())));
            }
            _ => panic!("expected edge"),
        }
    }

    #[test]
    fn edge_group_endpoint() {
        let ast = parse_ok(r#"diagram {
            group fe { node web {} }
            group be { node api {} }
            @fe -> @be { from_side: east to_side: west }
        }"#);
        match &ast.diagram.items[2] {
            DiagramItem::Edge(e) => {
                assert!(matches!(&e.source, EndpointAst::GroupFrame(id) if id == "fe"));
                assert!(matches!(&e.target, EndpointAst::GroupFrame(id) if id == "be"));
                assert_eq!(e.attrs.get("from_side"), Some(&AttrValue::Atom("east".into())));
            }
            _ => panic!("expected edge"),
        }
    }

    #[test]
    fn node_bare_archetype_rejected() {
        let err = parse_err("diagram { node db database }");
        assert!(matches!(&err, ParseError::Syntax { message, .. } if message.contains("bare atom")));
    }

    #[test]
    fn attribute_block_allows_commas() {
        let ast = parse_ok(r#"diagram { node a { label: "X", archetype: start } }"#);
        match &ast.diagram.items[0] {
            DiagramItem::Node(n) => {
                assert_eq!(n.attrs.get("label"), Some(&AttrValue::Str("X".into())));
                assert_eq!(n.attrs.get("archetype"), Some(&AttrValue::Atom("start".into())));
            }
            _ => panic!("expected node"),
        }
    }

    #[test]
    fn duplicate_id_error() {
        let err = parse_err("diagram { node a {} node a {} }");
        assert!(matches!(err, ParseError::DuplicateId { id, .. } if id == "a"));
    }

    #[test]
    fn node_group_id_collision() {
        let err = parse_err("diagram { node x {} group x {} }");
        assert!(matches!(err, ParseError::DuplicateId { id, .. } if id == "x"));
    }

    #[test]
    fn style_dot_attrs() {
        let ast = parse_ok(r##"diagram { node a { style.fill: "#E3F2FD" } }"##);
        match &ast.diagram.items[0] {
            DiagramItem::Node(n) => {
                assert_eq!(n.attrs.get("style.fill"), Some(&AttrValue::Str("#E3F2FD".into())));
            }
            _ => panic!("expected node"),
        }
    }

    #[test]
    fn doc_comment_preserved() {
        let ast = parse_ok("// hello\ndiagram { node a {} }");
        assert_eq!(ast.doc_comment.as_deref(), Some("// hello"));
    }

    #[test]
    fn edge_port_attrs() {
        let ast = parse_ok(r#"diagram {
            node a {} node b {}
            a -> b { from_side: south to_side: north from_slot: 0 to_slot: 1 }
        }"#);
        match &ast.diagram.items[2] {
            DiagramItem::Edge(e) => {
                assert_eq!(e.attrs.get("from_side"), Some(&AttrValue::Atom("south".into())));
                assert_eq!(e.attrs.get("to_slot"), Some(&AttrValue::Num(1.0)));
            }
            _ => panic!("expected edge"),
        }
    }
}
