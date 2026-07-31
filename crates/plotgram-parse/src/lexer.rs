//! Lexer (tokenizer) for Plotgram DSL 2.6.
//!
//! Converts source text into a token stream for the parser.
//! Handles doc-comment extraction, line comments, and all terminals in dsl-spec §2/§11.

use crate::error::ParseError;

/// Token kinds (dsl-spec §2 terminals + §10 keywords).
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Keywords
    Diagram,
    Node,
    Group,
    Partition,
    // Booleans
    True,
    False,
    // Identifiers & literals
    /// `[a-z][a-z0-9_]*` — no hyphens/dots.
    Ident(String),
    /// `[a-z][a-z0-9_.-]*` — contains at least one `-` or `.` segment.
    Atom(String),
    /// Double-quoted string literal.
    StringLit(String),
    /// Numeric literal.
    NumberLit(f64),
    // Symbols
    LBrace,  // {
    RBrace,  // }
    Colon,   // :
    Comma,   // ,  (optional attribute-block separator)
    At,      // @
    Arrow,     // ->
    DashArrow, // -->
    BiArrow,   // <->
    // Special
    Eof,
}

impl TokenKind {
    pub fn display_name(&self) -> &str {
        match self {
            Self::Diagram => "'diagram'",
            Self::Node => "'node'",
            Self::Group => "'group'",
            Self::Partition => "'partition'",
            Self::True => "'true'",
            Self::False => "'false'",
            Self::Ident(_) => "identifier",
            Self::Atom(_) => "atom",
            Self::StringLit(_) => "string",
            Self::NumberLit(_) => "number",
            Self::LBrace => "'{'",
            Self::RBrace => "'}'",
            Self::Colon => "':'",
            Self::Comma => "','",
            Self::At => "'@'",
            Self::Arrow => "'->'",
            Self::DashArrow => "'-->'",
            Self::BiArrow => "'<->'",
            Self::Eof => "end of file",
        }
    }
}

/// A token with position information.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub line: u32,
    pub column: u32,
}

/// Lexer: source text → token stream.
pub struct Lexer<'a> {
    chars: Vec<char>,
    #[allow(dead_code)]
    source: &'a str,
    pos: usize,
    line: u32,
    column: u32,
    /// File-leading doc comment (extracted before tokenize).
    pub doc_comment: Option<String>,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            chars: source.chars().collect(),
            source,
            pos: 0,
            line: 1,
            column: 1,
            doc_comment: None,
        }
    }

    /// Extract file-leading doc comment (consecutive `//` lines from line 1; blank line breaks).
    /// Must be called before `tokenize()`.
    pub fn extract_doc_comment(&mut self) -> Option<String> {
        // Skip leading whitespace (spaces, tabs, newlines)
        while let Some(ch) = self.peek() {
            if ch == ' ' || ch == '\t' || ch == '\n' || ch == '\r' {
                self.advance();
            } else {
                break;
            }
        }

        let mut lines: Vec<String> = Vec::new();
        while self.peek() == Some('/') && self.peek_at(1) == Some('/') {
            let start = self.pos;
            while let Some(ch) = self.peek() {
                if ch == '\n' {
                    break;
                }
                self.advance();
            }
            let line_text: String = self.chars[start..self.pos].iter().collect();
            lines.push(line_text);

            // Consume newline
            if self.peek() == Some('\n') {
                self.advance();
            }

            // Blank line breaks doc comment
            if self.peek() == Some('\n')
                || (self.peek() == Some('\r') && self.peek_at(1) == Some('\n'))
            {
                break;
            }
            // Skip spaces/tabs at start of next line to check for //
            let saved_pos = self.pos;
            let saved_line = self.line;
            let saved_col = self.column;
            while self.peek() == Some(' ') || self.peek() == Some('\t') {
                self.advance();
            }
            if self.peek() != Some('/') || self.peek_at(1) != Some('/') {
                // Not a continuation; rewind
                self.pos = saved_pos;
                self.line = saved_line;
                self.column = saved_col;
                break;
            }
            // Rewind the whitespace skip — we'll re-skip in next iteration
            self.pos = saved_pos;
            self.line = saved_line;
            self.column = saved_col;
            // Skip whitespace before next // line
            while self.peek() == Some(' ') || self.peek() == Some('\t') {
                self.advance();
            }
        }

        if lines.is_empty() {
            None
        } else {
            let comment = lines.join("\n");
            self.doc_comment = Some(comment.clone());
            Some(comment)
        }
    }

    /// Tokenize the remaining source into a token vector (ending with Eof).
    pub fn tokenize(&mut self) -> Result<Vec<Token>, ParseError> {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            if self.pos >= self.chars.len() {
                tokens.push(Token {
                    kind: TokenKind::Eof,
                    line: self.line,
                    column: self.column,
                });
                break;
            }
            let tok = self.next_token()?;
            tokens.push(tok);
        }
        Ok(tokens)
    }

    // ── Private helpers ─────────────────────────────────

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn advance(&mut self) -> Option<char> {
        if let Some(&ch) = self.chars.get(self.pos) {
            self.pos += 1;
            if ch == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
            Some(ch)
        } else {
            None
        }
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            // Skip whitespace
            while let Some(ch) = self.peek() {
                if ch.is_whitespace() {
                    self.advance();
                } else {
                    break;
                }
            }
            // Skip line comments (// ...)
            if self.peek() == Some('/') && self.peek_at(1) == Some('/') {
                while let Some(ch) = self.advance() {
                    if ch == '\n' {
                        break;
                    }
                }
            } else {
                break;
            }
        }
    }

    fn next_token(&mut self) -> Result<Token, ParseError> {
        let line = self.line;
        let col = self.column;
        let ch = self.peek().unwrap();

        match ch {
            '{' => {
                self.advance();
                Ok(Token { kind: TokenKind::LBrace, line, column: col })
            }
            '}' => {
                self.advance();
                Ok(Token { kind: TokenKind::RBrace, line, column: col })
            }
            ':' => {
                self.advance();
                Ok(Token { kind: TokenKind::Colon, line, column: col })
            }
            ',' => {
                self.advance();
                Ok(Token { kind: TokenKind::Comma, line, column: col })
            }
            '@' => {
                self.advance();
                Ok(Token { kind: TokenKind::At, line, column: col })
            }
            '<' => {
                // <->
                if self.peek_at(1) == Some('-') && self.peek_at(2) == Some('>') {
                    self.advance();
                    self.advance();
                    self.advance();
                    Ok(Token { kind: TokenKind::BiArrow, line, column: col })
                } else {
                    self.advance();
                    Err(ParseError::lex(line, col, format!("unexpected character '<'; did you mean '<->'?")))
                }
            }
            '-' => {
                self.advance(); // consume '-'
                if self.peek() == Some('>') {
                    self.advance(); // consume '>'
                    Ok(Token { kind: TokenKind::Arrow, line, column: col })
                } else if self.peek() == Some('-') {
                    self.advance(); // consume second '-'
                    if self.peek() == Some('>') {
                        self.advance(); // consume '>'
                        Ok(Token { kind: TokenKind::DashArrow, line, column: col })
                    } else {
                        Err(ParseError::lex(line, col, "unexpected '--'; did you mean '-->'?"))
                    }
                } else {
                    Err(ParseError::lex(line, col, "unexpected '-'; did you mean '->' or '-->'?"))
                }
            }
            '"' => self.read_string(line, col),
            c if c.is_ascii_digit() => self.read_number(line, col),
            c if c.is_ascii_lowercase() => self.read_word(line, col),
            '_' => {
                self.advance();
                Err(ParseError::lex(line, col, "identifier must start with a lowercase letter [a-z], not '_'"))
            }
            _ => {
                self.advance();
                Err(ParseError::lex(line, col, format!("unrecognized character '{ch}'")))
            }
        }
    }

    fn read_string(&mut self, line: u32, col: u32) -> Result<Token, ParseError> {
        self.advance(); // consume opening '"'
        let mut value = String::new();
        loop {
            match self.peek() {
                None => {
                    return Err(ParseError::lex(line, col, "unterminated string literal"));
                }
                Some('"') => {
                    self.advance();
                    break;
                }
                Some('\\') => {
                    self.advance();
                    match self.advance() {
                        Some('n') => value.push('\n'),
                        Some('\\') => value.push('\\'),
                        Some('"') => value.push('"'),
                        Some(c) => value.push(c),
                        None => {
                            return Err(ParseError::lex(line, col, "unterminated string escape"));
                        }
                    }
                }
                Some('\n') => {
                    return Err(ParseError::lex(line, col, "unterminated string literal (newline in string)"));
                }
                Some(c) => {
                    self.advance();
                    value.push(c);
                }
            }
        }
        if value.len() > Self::MAX_STRING_LEN {
            return Err(ParseError::lex(line, col, format!(
                "string literal too long ({} chars; max {})", value.len(), Self::MAX_STRING_LEN
            )));
        }
        Ok(Token { kind: TokenKind::StringLit(value), line, column: col })
    }

    /// §2.3: string max 256 characters.
    const MAX_STRING_LEN: usize = 256;

    fn read_number(&mut self, line: u32, col: u32) -> Result<Token, ParseError> {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                s.push(c);
                self.advance();
            } else {
                break;
            }
        }
        // Optional fractional part
        if self.peek() == Some('.') && self.peek_at(1).is_some_and(|c| c.is_ascii_digit()) {
            s.push('.');
            self.advance();
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    s.push(c);
                    self.advance();
                } else {
                    break;
                }
            }
        }
        let value: f64 = s.parse().map_err(|_| {
            ParseError::lex(line, col, format!("invalid number '{s}'"))
        })?;
        Ok(Token { kind: TokenKind::NumberLit(value), line, column: col })
    }

    fn read_word(&mut self, line: u32, col: u32) -> Result<Token, ParseError> {
        let mut word = String::new();
        // Core segment: [a-z_][a-z0-9_]*
        while let Some(c) = self.peek() {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' {
                word.push(c);
                self.advance();
            } else {
                break;
            }
        }

        // Greedily append hyphen-segments and dot-segments to form Atom
        // e.g. `top-to-bottom`, `common.clean-light`
        loop {
            match self.peek() {
                Some('-') => {
                    // Check next char is lowercase letter or digit (valid segment start)
                    if self.peek_at(1).is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit()) {
                        word.push('-');
                        self.advance();
                        while let Some(c) = self.peek() {
                            if c.is_ascii_lowercase() || c.is_ascii_digit() {
                                word.push(c);
                                self.advance();
                            } else {
                                break;
                            }
                        }
                    } else {
                        break;
                    }
                }
                Some('.') => {
                    if self.peek_at(1).is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit()) {
                        word.push('.');
                        self.advance();
                        while let Some(c) = self.peek() {
                            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' {
                                word.push(c);
                                self.advance();
                            } else {
                                break;
                            }
                        }
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }

        // §2.1/§2.2: length 1–64
        if word.len() > 64 {
            return Err(ParseError::lex(line, col, format!(
                "identifier/atom too long ({} chars; max 64)", word.len()
            )));
        }

        // Classify: keyword vs ident vs atom
        let has_extended = word.contains('-') || word.contains('.');
        if has_extended {
            Self::validate_atom_surface(&word, line, col)?;
            return Ok(Token { kind: TokenKind::Atom(word), line, column: col });
        }

        let kind = match word.as_str() {
            "diagram" => TokenKind::Diagram,
            "node" => TokenKind::Node,
            "group" => TokenKind::Group,
            "partition" => TokenKind::Partition,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            _ => TokenKind::Ident(word),
        };
        Ok(Token { kind, line, column: col })
    }

    /// §2.2: dots may not appear at atom boundaries or consecutively.
    fn validate_atom_surface(atom: &str, line: u32, col: u32) -> Result<(), ParseError> {
        if atom.starts_with('.')
            || atom.ends_with('.')
            || atom.contains("..")
        {
            return Err(ParseError::lex(
                line,
                col,
                format!(
                    "invalid atom `{atom}`: dots may not appear at boundaries or consecutively (dsl-spec §2.2)"
                ),
            ));
        }
        Ok(())
    }
}

/// Convenience: tokenize a full source string (no doc-comment extraction).
pub fn tokenize(source: &str) -> Result<Vec<Token>, ParseError> {
    let mut lexer = Lexer::new(source);
    lexer.tokenize()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<TokenKind> {
        let mut lexer = Lexer::new(source);
        lexer.tokenize().unwrap().into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn basic_tokens() {
        let toks = kinds("diagram { node a {} }");
        assert_eq!(toks, vec![
            TokenKind::Diagram,
            TokenKind::LBrace,
            TokenKind::Node,
            TokenKind::Ident("a".into()),
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::RBrace,
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn arrows() {
        let toks = kinds("a -> b --> c <-> d");
        assert_eq!(toks, vec![
            TokenKind::Ident("a".into()),
            TokenKind::Arrow,
            TokenKind::Ident("b".into()),
            TokenKind::DashArrow,
            TokenKind::Ident("c".into()),
            TokenKind::BiArrow,
            TokenKind::Ident("d".into()),
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn atom_with_hyphens_and_dots() {
        let toks = kinds("top-to-bottom common.clean-light");
        assert_eq!(toks, vec![
            TokenKind::Atom("top-to-bottom".into()),
            TokenKind::Atom("common.clean-light".into()),
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn string_with_escapes() {
        let toks = kinds(r#""hello \"world\"\n""#);
        assert_eq!(toks, vec![
            TokenKind::StringLit("hello \"world\"\n".into()),
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn number_literal() {
        let toks = kinds("42 3.14");
        assert_eq!(toks, vec![
            TokenKind::NumberLit(42.0),
            TokenKind::NumberLit(3.14),
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn at_sign() {
        let toks = kinds("@frontend");
        assert_eq!(toks, vec![
            TokenKind::At,
            TokenKind::Ident("frontend".into()),
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn doc_comment_extraction() {
        let source = "// line one\n// line two\ndiagram {}";
        let mut lexer = Lexer::new(source);
        let doc = lexer.extract_doc_comment();
        assert_eq!(doc.as_deref(), Some("// line one\n// line two"));
        let toks = lexer.tokenize().unwrap();
        assert_eq!(toks[0].kind, TokenKind::Diagram);
    }

    #[test]
    fn doc_comment_broken_by_blank_line() {
        let source = "// first\n\n// not doc\ndiagram {}";
        let mut lexer = Lexer::new(source);
        let doc = lexer.extract_doc_comment();
        assert_eq!(doc.as_deref(), Some("// first"));
    }

    #[test]
    fn line_comments_skipped() {
        let toks = kinds("node a {} // trailing comment\nnode b {}");
        assert_eq!(toks, vec![
            TokenKind::Node,
            TokenKind::Ident("a".into()),
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::Node,
            TokenKind::Ident("b".into()),
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn keywords_vs_ident() {
        let toks = kinds("group true false diagram node my_id");
        assert_eq!(toks, vec![
            TokenKind::Group,
            TokenKind::True,
            TokenKind::False,
            TokenKind::Diagram,
            TokenKind::Node,
            TokenKind::Ident("my_id".into()),
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn style_dot_key() {
        // style.fill is lexed as Atom("style.fill")
        let toks = kinds("style.fill");
        assert_eq!(toks, vec![
            TokenKind::Atom("style.fill".into()),
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn atom_dot_boundary_rejected() {
        let result = tokenize("foo.");
        assert!(result.is_err());
    }

    #[test]
    fn comma_token() {
        let toks = kinds("a, b");
        assert_eq!(toks, vec![
            TokenKind::Ident("a".into()),
            TokenKind::Comma,
            TokenKind::Ident("b".into()),
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn error_on_bad_char() {
        let result = tokenize("diagram { # }");
        assert!(result.is_err());
    }
}
