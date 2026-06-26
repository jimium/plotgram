//! Drawify 词法分析器（Lexer/Tokenizer）
//!
//! 将源文本转换为 token 流，供解析器消费。

use crate::ast::{Position, Span};
use crate::error::DiagnosticError;

/// Token 类型
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // 关键字
    Diagram,
    Entity,
    Group,
    NodeStyle,
    EdgeStyle,
    Config,
    // 图表类型
    Flowchart,
    Sequence,
    Architecture,
    State,
    Er,
    Mindmap,
    // 布尔
    True,
    False,
    // 标识符和字面量
    Ident(String),
    StringLit(String),
    NumberLit(f64),
    // 符号
    LBrace,
    RBrace,
    LBracket,    // [
    RBracket,    // ]
    Colon,
    Dot,         // .
    Arrow,       // ->
    DashArrow,   // -->
    BiArrow,     // <->
    Gt,          // >  (head label marker)
    Lt,          // <  (tail label marker)
    // 特殊
    Eof,
}

impl TokenKind {
    pub fn keyword_str(&self) -> Option<&'static str> {
        match self {
            TokenKind::Diagram => Some("diagram"),
            TokenKind::Entity => Some("entity"),
            TokenKind::Group => Some("group"),
            TokenKind::NodeStyle => Some("node_style"),
            TokenKind::EdgeStyle => Some("edge_style"),
            TokenKind::Config => Some("config"),
            TokenKind::Flowchart => Some("flowchart"),
            TokenKind::Sequence => Some("sequence"),
            TokenKind::Architecture => Some("architecture"),
            TokenKind::State => Some("state"),
            TokenKind::Er => Some("er"),
            TokenKind::Mindmap => Some("mindmap"),
            TokenKind::True => Some("true"),
            TokenKind::False => Some("false"),
            _ => None,
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            TokenKind::Diagram => "'diagram'",
            TokenKind::Entity => "'entity'",
            TokenKind::Group => "'group'",
            TokenKind::NodeStyle => "'node_style'",
            TokenKind::EdgeStyle => "'edge_style'",
            TokenKind::Config => "'config'",
            TokenKind::Flowchart => "'flowchart'",
            TokenKind::Sequence => "'sequence'",
            TokenKind::Architecture => "'architecture'",
            TokenKind::State => "'state'",
            TokenKind::Er => "'er'",
            TokenKind::Mindmap => "'mindmap'",
            TokenKind::True => "'true'",
            TokenKind::False => "'false'",
            TokenKind::Ident(_) => "identifier",
            TokenKind::StringLit(_) => "string",
            TokenKind::NumberLit(_) => "number",
            TokenKind::LBrace => "'{'",
            TokenKind::RBrace => "'}'",
            TokenKind::LBracket => "'['",
            TokenKind::RBracket => "']'",
            TokenKind::Colon => "':'",
            TokenKind::Dot => "'.'",
            TokenKind::Arrow => "'->'",
            TokenKind::DashArrow => "'-->'",
            TokenKind::BiArrow => "'<->'",
            TokenKind::Gt => "'>'",
            TokenKind::Lt => "'<'",
            TokenKind::Eof => "end of file",
        }
    }
}

/// 带位置信息的 Token
#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

/// 词法分析器
pub struct Lexer<'a> {
    #[allow(dead_code)]
    source: &'a str,
    chars: Vec<char>,
    pos: usize,
    line: usize,
    column: usize,
    pub errors: Vec<DiagnosticError>,
    /// 文件开头连续 `//` 行组成的文档注释（已提取后不再进入 token 流）。
    pub doc_comment: Option<String>,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            chars: source.chars().collect(),
            pos: 0,
            line: 1,
            column: 1,
            errors: Vec::new(),
            doc_comment: None,
        }
    }

    /// 在 tokenize 之前提取文件开头的文档注释。
    ///
    /// 规则：跳过文件最开始的空白字符（空格、制表符、换行），然后捕获连续以 `//` 开头的行。
    /// 文档注释不进入 token 流，由 Parser 直接存入 AST 的 `doc_comment` 字段。
    pub fn extract_doc_comment(&mut self) -> Option<String> {
        // 跳过文件开头的空白
        while let Some(ch) = self.peek() {
            if ch == ' ' || ch == '\t' || ch == '\n' {
                self.advance();
            } else {
                break;
            }
        }

        let mut lines = Vec::new();
        while self.peek() == Some('/') && self.peek_at(1) == Some('/') {
            let start = self.pos;
            while let Some(ch) = self.peek() {
                if ch == '\n' {
                    break;
                }
                self.advance();
            }
            let line: String = self.chars[start..self.pos].iter().collect();
            lines.push(line);

            // 消费换行，进入下一行
            if self.peek() == Some('\n') {
                self.advance();
            }
        }

        if lines.is_empty() {
            None
        } else {
            let comment = lines.join("\n") + "\n";
            self.doc_comment = Some(comment.clone());
            Some(comment)
        }
    }

    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            if self.pos >= self.chars.len() {
                tokens.push(Token {
                    kind: TokenKind::Eof,
                    span: Span::new(
                        Position::new(self.line, self.column),
                        Position::new(self.line, self.column),
                    ),
                });
                break;
            }
            if let Some(token) = self.next_token() {
                tokens.push(token);
            }
        }
        tokens
    }

    fn current_pos(&self) -> Position {
        Position::new(self.line, self.column)
    }

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
            // Skip line comments
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

    fn next_token(&mut self) -> Option<Token> {
        let start = self.current_pos();
        let ch = self.peek()?;

        match ch {
            '{' => {
                self.advance();
                Some(Token {
                    kind: TokenKind::LBrace,
                    span: Span::new(start, self.current_pos()),
                })
            }
            '}' => {
                self.advance();
                Some(Token {
                    kind: TokenKind::RBrace,
                    span: Span::new(start, self.current_pos()),
                })
            }
            '[' => {
                self.advance();
                Some(Token {
                    kind: TokenKind::LBracket,
                    span: Span::new(start, self.current_pos()),
                })
            }
            ']' => {
                self.advance();
                Some(Token {
                    kind: TokenKind::RBracket,
                    span: Span::new(start, self.current_pos()),
                })
            }
            ':' => {
                self.advance();
                Some(Token {
                    kind: TokenKind::Colon,
                    span: Span::new(start, self.current_pos()),
                })
            }
            '.' => {
                self.advance();
                Some(Token {
                    kind: TokenKind::Dot,
                    span: Span::new(start, self.current_pos()),
                })
            }
            '<' => {
                // <->
                if self.peek_at(1) == Some('-') && self.peek_at(2) == Some('>') {
                    self.advance();
                    self.advance();
                    self.advance();
                    Some(Token {
                        kind: TokenKind::BiArrow,
                        span: Span::new(start, self.current_pos()),
                    })
                } else {
                    self.advance();
                    Some(Token {
                        kind: TokenKind::Lt,
                        span: Span::new(start, self.current_pos()),
                    })
                }
            }
            '>' => {
                self.advance();
                Some(Token {
                    kind: TokenKind::Gt,
                    span: Span::new(start, self.current_pos()),
                })
            }
            '-' => {
                self.advance(); // consume '-'
                if self.peek() == Some('>') {
                    self.advance(); // consume '>'
                    // -> 后若再跟 '>' 则为 ->>（目前不支持），仍返回 Arrow
                    Some(Token {
                        kind: TokenKind::Arrow,
                        span: Span::new(start, self.current_pos()),
                    })
                } else if self.peek() == Some('-') {
                    self.advance(); // consume second '-'
                    if self.peek() == Some('>') {
                        self.advance(); // consume '>'
                        Some(Token {
                            kind: TokenKind::DashArrow,
                            span: Span::new(start, self.current_pos()),
                        })
                    } else {
                        self.errors.push(DiagnosticError::unexpected_token(
                            Span::new(start, self.current_pos()),
                            "--",
                            &["-->"],
                        ));
                        None
                    }
                } else {
                    // '-' 后跟字母/数字，说明用户可能写了含连字符的标识符（如 sugiyama-v2）
                    let next_is_ident_char = self
                        .peek()
                        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
                    if next_is_ident_char {
                        // 回溯读取完整的连字符标识符，给出精确提示
                        let mut hyphenated = String::new();
                        // 从当前位置继续读取，收集所有 a-z0-9_- 字符
                        while let Some(c) = self.peek() {
                            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                                hyphenated.push(c);
                                self.advance();
                            } else {
                                break;
                            }
                        }
                        let underscore_version = hyphenated.replace('-', "_");
                        self.errors.push(DiagnosticError::hyphenated_identifier(
                            Span::new(start, self.current_pos()),
                            &hyphenated,
                            &underscore_version,
                        ));
                        None
                    } else {
                        self.errors.push(DiagnosticError::unexpected_token(
                            Span::new(start, self.current_pos()),
                            "-",
                            &["->", "-->"],
                        ));
                        None
                    }
                }
            }
            '"' => self.read_string(start),
            c if c.is_ascii_digit() => self.read_number(start),
            c if c.is_ascii_alphabetic() || c == '_' => self.read_word(start),
            _ => {
                self.advance();
                self.errors.push(DiagnosticError::syntax_error(
                    Span::new(start, self.current_pos()),
                    format!("无法识别的字符 '{}'", ch),
                ));
                None
            }
        }
    }

    fn read_string(&mut self, start: Position) -> Option<Token> {
        self.advance(); // consume opening '"'
        let mut value = String::new();
        loop {
            match self.peek() {
                None => {
                    self.errors.push(DiagnosticError::unterminated_string(Span::new(
                        start,
                        self.current_pos(),
                    )));
                    return None;
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
                            self.errors.push(DiagnosticError::unterminated_string(Span::new(
                                start,
                                self.current_pos(),
                            )));
                            return None;
                        }
                    }
                }
                Some('\n') => {
                    self.errors.push(DiagnosticError::unterminated_string(Span::new(
                        start,
                        self.current_pos(),
                    )));
                    return None;
                }
                Some(c) => {
                    self.advance();
                    value.push(c);
                }
            }
        }
        Some(Token {
            kind: TokenKind::StringLit(value),
            span: Span::new(start, self.current_pos()),
        })
    }

    fn read_number(&mut self, start: Position) -> Option<Token> {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                s.push(c);
                self.advance();
            } else {
                break;
            }
        }
        if self.peek() == Some('.') && self.peek_at(1).map_or(false, |c| c.is_ascii_digit()) {
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
        let value: f64 = match s.parse() {
            Ok(v) => v,
            Err(_) => {
                self.errors.push(DiagnosticError::syntax_error(
                    Span::new(start, self.current_pos()),
                    format!("无法解析数字 '{}'", s),
                ));
                0.0
            }
        };
        Some(Token {
            kind: TokenKind::NumberLit(value),
            span: Span::new(start, self.current_pos()),
        })
    }

    fn read_word(&mut self, start: Position) -> Option<Token> {
        let mut word = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' {
                word.push(c);
                self.advance();
            } else {
                break;
            }
        }

        // atom 值允许连字符分段：foo-bar-baz（仅小写开头的词）
        if word
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase())
        {
            loop {
                if self.peek() != Some('-') {
                    break;
                }
                let hyphen_pos = self.pos;
                self.advance();
                let mut segment = String::new();
                while let Some(c) = self.peek() {
                    if c.is_ascii_lowercase() || c.is_ascii_digit() {
                        segment.push(c);
                        self.advance();
                    } else {
                        break;
                    }
                }
                if segment.is_empty() {
                    self.pos = hyphen_pos;
                    break;
                }
                word.push('-');
                word.push_str(&segment);
            }
        }

        let kind = match word.as_str() {
            "diagram" => TokenKind::Diagram,
            "entity" => TokenKind::Entity,
            "group" => TokenKind::Group,
            "node_style" => TokenKind::NodeStyle,
            "edge_style" => TokenKind::EdgeStyle,
            "config" => TokenKind::Config,
            "flowchart" => TokenKind::Flowchart,
            "sequence" => TokenKind::Sequence,
            "architecture" => TokenKind::Architecture,
            "state" => TokenKind::State,
            "er" => TokenKind::Er,
            "mindmap" => TokenKind::Mindmap,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            _ => TokenKind::Ident(word),
        };
        Some(Token {
            kind,
            span: Span::new(start, self.current_pos()),
        })
    }
}

#[cfg(test)]
mod lexer_doc_comment_tests {
    use super::Lexer;

    #[test]
    fn extract_doc_comment_at_file_start() {
        let source = "// 这是文档注释\n// 第二行\ndiagram flowchart {}";
        let mut lexer = Lexer::new(source);
        let doc = lexer.extract_doc_comment();
        assert_eq!(doc.as_deref(), Some("// 这是文档注释\n// 第二行\n"));

        let tokens = lexer.tokenize();
        assert!(
            tokens.iter().all(|t| !matches!(t.kind, super::TokenKind::Ident(ref s) if s == "这是文档注释")),
            "文档注释内容不应进入 token 流"
        );
    }

    #[test]
    fn no_doc_comment_when_missing() {
        let source = "diagram flowchart {}";
        let mut lexer = Lexer::new(source);
        let doc = lexer.extract_doc_comment();
        assert!(doc.is_none());
    }

    #[test]
    fn non_doc_comments_are_discarded() {
        let source = r##"diagram flowchart {
    // 行间注释
    entity a "A" // 行尾注释
}"##;
        let mut lexer = Lexer::new(source);
        lexer.extract_doc_comment();
        let tokens = lexer.tokenize();
        // 非文件头注释被丢弃，"注释" 不应成为 token
        let has_comment_text = tokens.iter().any(|t| {
            matches!(t.kind, super::TokenKind::Ident(ref s) if s.contains("注释"))
        });
        assert!(!has_comment_text, "非文档注释应在词法阶段丢弃");
    }

    #[test]
    fn only_consecutive_leading_lines_form_doc_comment() {
        let source = "// first\n\n// not doc\ndiagram flowchart {}";
        let mut lexer = Lexer::new(source);
        let doc = lexer.extract_doc_comment();
        assert_eq!(doc.as_deref(), Some("// first\n"));
    }
}
