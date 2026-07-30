//! MD subset -> Content AST (`content-md-spec.md` §1–§4).
//!
//! Parsing never fails: everything outside the whitelist degrades to literal
//! text. Pure function of the input string.

use crate::ast::{Block, ContentDoc, Line, ListItem, Run, RunStyle};

/// Parse content-block text into a `ContentDoc`. Never errors (spec §4).
pub fn parse(input: &str) -> ContentDoc {
    let lines = normalize(input);
    let classified: Vec<RawLine> = lines.iter().map(|l| classify(l)).collect();
    aggregate(&classified)
}

// ── §1 normalization ──────────────────────────────────────────────────────

/// Normalize newlines, strip trailing whitespace per line, drop leading and
/// trailing blank lines. Leading indentation is kept for classification.
fn normalize(input: &str) -> Vec<String> {
    let unified = input.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines: Vec<String> = unified.split('\n').map(|l| l.trim_end().to_string()).collect();
    while lines.first().is_some_and(|l| l.is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines
}

// ── §2 line classification (ordered, first match wins) ────────────────────

enum RawLine<'a> {
    Blank,
    Rule,
    Bullet(&'a str),
    Ordered(u32, &'a str),
    Text(&'a str),
}

fn classify(line: &str) -> RawLine<'_> {
    if line.trim().is_empty() {
        return RawLine::Blank;
    }
    let trimmed = line.trim();
    if trimmed.len() >= 3 && trimmed.chars().all(|c| c == '-') {
        return RawLine::Rule;
    }
    // List items require zero indentation; indented look-alikes stay Text (§2).
    if let Some(rest) = line.strip_prefix("- ") {
        return RawLine::Bullet(rest);
    }
    if let Some((number, rest)) = parse_ordered_marker(line) {
        return RawLine::Ordered(number, rest);
    }
    RawLine::Text(line)
}

/// `<1–3 digits> "." " "` at column zero.
fn parse_ordered_marker(line: &str) -> Option<(u32, &str)> {
    let digits_len = line.chars().take_while(|c| c.is_ascii_digit()).count();
    if !(1..=3).contains(&digits_len) {
        return None;
    }
    let rest = &line[digits_len..];
    let rest = rest.strip_prefix(". ")?;
    let number = line[..digits_len].parse().ok()?;
    Some((number, rest))
}

// ── §2.1 block aggregation ─────────────────────────────────────────────────

fn aggregate(lines: &[RawLine]) -> ContentDoc {
    enum Pending {
        None,
        Paragraph(Vec<Line>),
        List { ordered: bool, items: Vec<ListItem> },
    }

    let mut blocks = Vec::new();
    let mut pending = Pending::None;

    let flush = |pending: &mut Pending, blocks: &mut Vec<Block>| {
        match std::mem::replace(pending, Pending::None) {
            Pending::None => {}
            Pending::Paragraph(lines) => blocks.push(Block::Paragraph { lines }),
            Pending::List { ordered, items } => blocks.push(Block::List { ordered, items }),
        }
    };

    for raw in lines {
        match raw {
            RawLine::Blank => flush(&mut pending, &mut blocks),
            RawLine::Rule => {
                flush(&mut pending, &mut blocks);
                blocks.push(Block::Rule);
            }
            RawLine::Text(text) => {
                let line = parse_inline(text);
                match &mut pending {
                    Pending::Paragraph(lines) => lines.push(line),
                    _ => {
                        flush(&mut pending, &mut blocks);
                        pending = Pending::Paragraph(vec![line]);
                    }
                }
            }
            RawLine::Bullet(content) => {
                let item = ListItem { number: None, line: parse_inline(content) };
                match &mut pending {
                    // Same-kind items extend the current list; a kind switch starts a new one.
                    Pending::List { ordered: false, items } => items.push(item),
                    _ => {
                        flush(&mut pending, &mut blocks);
                        pending = Pending::List { ordered: false, items: vec![item] };
                    }
                }
            }
            RawLine::Ordered(n, content) => {
                let item = ListItem { number: Some(*n), line: parse_inline(content) };
                match &mut pending {
                    Pending::List { ordered: true, items } => items.push(item),
                    _ => {
                        flush(&mut pending, &mut blocks);
                        pending = Pending::List { ordered: true, items: vec![item] };
                    }
                }
            }
        }
    }
    flush(&mut pending, &mut blocks);
    ContentDoc { blocks }
}

// ── §3 inline: single left-to-right pass, no nesting, per-line scope ──────

fn parse_inline(text: &str) -> Line {
    let chars: Vec<char> = text.chars().collect();
    let mut runs: Vec<Run> = Vec::new();
    let mut plain = String::new();
    let mut i = 0;

    let flush_plain = |plain: &mut String, runs: &mut Vec<Run>| {
        if !plain.is_empty() {
            runs.push(Run::new(std::mem::take(plain), RunStyle::Plain));
        }
    };

    while i < chars.len() {
        let c = chars[i];
        // #1 escape: `\` before ` * \ yields the literal char.
        if c == '\\' && i + 1 < chars.len() && matches!(chars[i + 1], '`' | '*' | '\\') {
            plain.push(chars[i + 1]);
            i += 2;
            continue;
        }
        // #2 code span: fully literal inside, closes at next backtick on the line.
        if c == '`' {
            if let Some(close) = chars[i + 1..].iter().position(|&ch| ch == '`') {
                let content: String = chars[i + 1..i + 1 + close].iter().collect();
                if !content.is_empty() {
                    flush_plain(&mut plain, &mut runs);
                    runs.push(Run::new(content, RunStyle::Code));
                    i += close + 2;
                    continue;
                }
            }
            plain.push('`');
            i += 1;
            continue;
        }
        // #3/#4 strong / emph.
        if c == '*' {
            let strong = i + 1 < chars.len() && chars[i + 1] == '*';
            let delim = if strong { 2 } else { 1 };
            if let Some((content, next)) = scan_span(&chars, i + delim, strong) {
                let valid = !content.is_empty()
                    && !content.starts_with(char::is_whitespace)
                    && !content.ends_with(char::is_whitespace);
                if valid {
                    flush_plain(&mut plain, &mut runs);
                    let style = if strong { RunStyle::Strong } else { RunStyle::Emph };
                    runs.push(Run::new(content, style));
                    i = next;
                    continue;
                }
            }
            // No closer or invalid content: the delimiter itself is literal.
            for _ in 0..delim {
                plain.push('*');
            }
            i += delim;
            continue;
        }
        plain.push(c);
        i += 1;
    }
    flush_plain(&mut plain, &mut runs);
    Line { runs: merge_adjacent(runs) }
}

/// Scan for the closing `*` / `**`. Content is plain text: escapes apply,
/// backticks and (for strong) lone stars stay literal — no nesting.
fn scan_span(chars: &[char], start: usize, strong: bool) -> Option<(String, usize)> {
    let mut content = String::new();
    let mut k = start;
    while k < chars.len() {
        if chars[k] == '\\' && k + 1 < chars.len() && matches!(chars[k + 1], '`' | '*' | '\\') {
            content.push(chars[k + 1]);
            k += 2;
            continue;
        }
        if chars[k] == '*' {
            if strong {
                if k + 1 < chars.len() && chars[k + 1] == '*' {
                    return Some((content, k + 2));
                }
                content.push('*');
                k += 1;
                continue;
            }
            return Some((content, k + 1));
        }
        content.push(chars[k]);
        k += 1;
    }
    None
}

fn merge_adjacent(runs: Vec<Run>) -> Vec<Run> {
    let mut merged: Vec<Run> = Vec::with_capacity(runs.len());
    for run in runs {
        match merged.last_mut() {
            Some(last) if last.style == run.style => last.text.push_str(&run.text),
            _ => merged.push(run),
        }
    }
    merged
}

// ── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use RunStyle::*;

    fn line(runs: Vec<(&str, RunStyle)>) -> Line {
        Line { runs: runs.into_iter().map(|(t, s)| Run::new(t, s)).collect() }
    }
    fn para(lines: Vec<Line>) -> Block {
        Block::Paragraph { lines }
    }

    #[test]
    fn parse_table() {
        let cases: Vec<(&str, &str, Vec<Block>)> = vec![
            ("empty", "", vec![]),
            ("blank_only", "  \n\n", vec![]),
            (
                "hard_break_paragraph",
                "第一行\n第二行",
                vec![para(vec![line(vec![("第一行", Plain)]), line(vec![("第二行", Plain)])])],
            ),
            (
                "blank_line_splits_paragraphs",
                "a\n\n\nb",
                vec![para(vec![line(vec![("a", Plain)])]), para(vec![line(vec![("b", Plain)])])],
            ),
            (
                "spec_example",
                "**订单服务**\n---\n处理下单主链路：\n- 校验库存\n- 落库并发 `order.created`\n1. 幂等键去重",
                vec![
                    para(vec![line(vec![("订单服务", Strong)])]),
                    Block::Rule,
                    para(vec![line(vec![("处理下单主链路：", Plain)])]),
                    Block::List {
                        ordered: false,
                        items: vec![
                            ListItem { number: None, line: line(vec![("校验库存", Plain)]) },
                            ListItem {
                                number: None,
                                line: line(vec![("落库并发 ", Plain), ("order.created", Code)]),
                            },
                        ],
                    },
                    Block::List {
                        ordered: true,
                        items: vec![ListItem { number: Some(1), line: line(vec![("幂等键去重", Plain)]) }],
                    },
                ],
            ),
            (
                "ordered_numbers_as_authored",
                "3. c\n7. g",
                vec![Block::List {
                    ordered: true,
                    items: vec![
                        ListItem { number: Some(3), line: line(vec![("c", Plain)]) },
                        ListItem { number: Some(7), line: line(vec![("g", Plain)]) },
                    ],
                }],
            ),
            ("rule_trimmed", "  ----  ", vec![Block::Rule]),
            ("dash_pair_is_text", "--", vec![para(vec![line(vec![("--", Plain)])])]),
            // Degradations (spec §4 / §6).
            ("heading_is_literal", "# 标题", vec![para(vec![line(vec![("# 标题", Plain)])])]),
            (
                "indented_bullet_degrades",
                "  - 嵌套项",
                vec![para(vec![line(vec![("  - 嵌套项", Plain)])])],
            ),
            ("unclosed_strong", "**没闭合", vec![para(vec![line(vec![("**没闭合", Plain)])])]),
            ("emph_whitespace_bounds", "3 * 4 * 5", vec![para(vec![line(vec![("3 * 4 * 5", Plain)])])]),
            ("escapes", r"\*不强调\*", vec![para(vec![line(vec![("*不强调*", Plain)])])]),
            // Inline details.
            ("emph", "看 *这里* 了", vec![para(vec![line(vec![("看 ", Plain), ("这里", Emph), (" 了", Plain)])])]),
            ("strong_inner_star_literal", "**a*b**", vec![para(vec![line(vec![("a*b", Strong)])])]),
            ("strong_inner_backtick_literal", "**a`b`c**", vec![para(vec![line(vec![("a`b`c", Strong)])])]),
            ("code_protects_star", "`a*b`", vec![para(vec![line(vec![("a*b", Code)])])]),
            ("code_no_escape_inside", r"`a\*b`", vec![para(vec![line(vec![(r"a\*b", Code)])])]),
            ("unclosed_code", "a `b", vec![para(vec![line(vec![("a `b", Plain)])])]),
            ("empty_code_literal", "``x", vec![para(vec![line(vec![("``x", Plain)])])]),
            ("empty_strong_literal", "****", vec![para(vec![line(vec![("****", Plain)])])]),
            (
                "list_kind_switch_splits",
                "- a\n1. b",
                vec![
                    Block::List {
                        ordered: false,
                        items: vec![ListItem { number: None, line: line(vec![("a", Plain)]) }],
                    },
                    Block::List {
                        ordered: true,
                        items: vec![ListItem { number: Some(1), line: line(vec![("b", Plain)]) }],
                    },
                ],
            ),
            (
                "crlf_normalized",
                "a\r\nb\r",
                vec![para(vec![line(vec![("a", Plain)]), line(vec![("b", Plain)])])],
            ),
        ];

        for (name, input, expected) in cases {
            let got = parse(input);
            assert_eq!(got.blocks, expected, "case `{name}` failed:\n{got:#?}");
        }
    }
}
