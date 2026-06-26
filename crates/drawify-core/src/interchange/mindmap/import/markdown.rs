//! Markdown 大纲导入解析器。

use crate::interchange::mindmap::diagram::DiagramBuildOptions;
use crate::interchange::mindmap::tree::{MindmapTree, MindmapTreeNode};
use crate::interchange::mindmap::mindmap_tree_to_diagram;
use crate::types::DiagramType;

// ─── Public types ──────────────────────────────────────────────────

/// Markdown 导入语法模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MarkdownImportSyntax {
    /// 自动检测（默认）
    #[default]
    Auto,
    /// ATX 标题模式
    AtxHeadings,
    /// 嵌套列表模式
    NestedList,
}

/// entity id 生成策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EntityIdStrategy {
    /// node_1, node_2, … 按 DFS 序（默认，确定性）
    #[default]
    Sequential,
    /// slugify(label)；冲突时追加 _2, _3
    Slugify,
    /// 解析 Export 写入的 <!-- drawify:entity-id -->
    FromMetadata,
}

/// Markdown 导入选项。
#[derive(Debug, Clone)]
pub struct MarkdownImportOptions {
    pub syntax: MarkdownImportSyntax,
    pub entity_id_strategy: EntityIdStrategy,
    pub strict_content: bool,
    pub max_level: u8,
    pub default_layout: Option<String>,
    pub default_theme: Option<String>,
}

impl Default for MarkdownImportOptions {
    fn default() -> Self {
        Self {
            syntax: MarkdownImportSyntax::Auto,
            entity_id_strategy: EntityIdStrategy::Sequential,
            strict_content: false,
            max_level: 6,
            default_layout: None,
            default_theme: None,
        }
    }
}

/// 导入产出。
#[derive(Debug, Clone)]
pub struct ImportOutput {
    pub tree: MindmapTree,
    pub warnings: Vec<ImportWarning>,
}

/// 导入错误。
#[derive(Debug, Clone)]
pub enum ImportError {
    /// 无有效标题/列表
    EmptyOutline,
    /// ATX 与 list 混用
    AmbiguousSyntax,
    /// 多个 H1（strict 模式下）
    MultipleRoots,
    /// 不支持的输入格式
    UnsupportedFormat,
}

/// 导入警告。
#[derive(Debug, Clone)]
pub struct ImportWarning {
    pub code: String,
    pub message: String,
    pub line: Option<usize>,
}

// ─── Internal line representation ──────────────────────────────────

/// 解析后的行类型。
#[derive(Debug, Clone)]
enum LineKind {
    /// ATX 标题：level (1-6), label text
    AtxHeading { level: u8, text: String },
    /// 无序列表项：indent depth, text
    ListItem { indent: usize, text: String },
    /// HTML 注释：raw content between <!-- and -->
    HtmlComment(String),
    /// 空行
    Blank,
    /// 其他内容（段落、代码块等）
    Other(String),
}

#[derive(Debug, Clone)]
struct ParsedLine {
    line_no: usize,
    kind: LineKind,
}

// ─── Main entry functions ──────────────────────────────────────────

/// 解析 Markdown 大纲文本为 MindmapTree。
pub fn parse_markdown_outline(
    source: &str,
    options: &MarkdownImportOptions,
) -> Result<ImportOutput, ImportError> {
    let lines = parse_lines(source);

    // 检测语法模式
    let syntax = match options.syntax {
        MarkdownImportSyntax::Auto => detect_syntax(&lines)?,
        MarkdownImportSyntax::AtxHeadings => MarkdownImportSyntax::AtxHeadings,
        MarkdownImportSyntax::NestedList => MarkdownImportSyntax::NestedList,
    };

    match syntax {
        MarkdownImportSyntax::AtxHeadings => parse_atx_outline(&lines, options),
        MarkdownImportSyntax::NestedList => {
            // 嵌套列表模式暂未实现，返回错误
            Err(ImportError::UnsupportedFormat)
        }
        MarkdownImportSyntax::Auto => unreachable!(),
    }
}

/// 一步到位：Markdown 大纲 → Diagram AST。
pub fn import_markdown_outline(
    source: &str,
    options: &MarkdownImportOptions,
) -> Result<crate::ast::Diagram, ImportError> {
    let output = parse_markdown_outline(source, options)?;
    let build_opts = DiagramBuildOptions {
        diagram_type: DiagramType::Mindmap,
        infer_entity_types: true,
        layout: options.default_layout.clone(),
        theme: options.default_theme.clone(),
        graphic_style: None,
    };
    Ok(mindmap_tree_to_diagram(&output.tree, &build_opts))
}

// ─── Syntax detection ──────────────────────────────────────────────

fn detect_syntax(lines: &[ParsedLine]) -> Result<MarkdownImportSyntax, ImportError> {
    let has_atx = lines.iter().any(|l| matches!(&l.kind, LineKind::AtxHeading { .. }));
    let has_list = lines.iter().any(|l| matches!(&l.kind, LineKind::ListItem { .. }));

    match (has_atx, has_list) {
        (true, true) => Err(ImportError::AmbiguousSyntax),
        (true, false) => Ok(MarkdownImportSyntax::AtxHeadings),
        (false, true) => Ok(MarkdownImportSyntax::NestedList),
        (false, false) => Err(ImportError::EmptyOutline),
    }
}

// ─── Line parsing ──────────────────────────────────────────────────

fn parse_lines(source: &str) -> Vec<ParsedLine> {
    source
        .lines()
        .enumerate()
        .map(|(i, raw)| {
            let line_no = i + 1;
            let trimmed = raw.trim();

            let kind = if trimmed.is_empty() {
                LineKind::Blank
            } else if let Some((level, text)) = parse_atx_heading(trimmed) {
                LineKind::AtxHeading { level, text }
            } else if let Some((indent, text)) = parse_list_item(raw) {
                LineKind::ListItem { indent, text }
            } else if let Some(content) = parse_html_comment(trimmed) {
                LineKind::HtmlComment(content)
            } else {
                LineKind::Other(trimmed.to_string())
            };

            ParsedLine { line_no, kind }
        })
        .collect()
}

/// 解析 ATX 标题行。返回 (level, text) 或 None。
/// ATX 标题：1-6 个 `#` 后跟空格。
fn parse_atx_heading(line: &str) -> Option<(u8, String)> {
    let rest = line.strip_prefix('#')?;
    let mut level: u8 = 1;
    let mut chars = rest.chars().peekable();
    while let Some(c) = chars.peek() {
        if *c == '#' && level < 6 {
            level += 1;
            chars.next();
        } else {
            break;
        }
    }
    // 必须紧跟空格
    let after_hashes: String = chars.collect();
    if after_hashes.starts_with(' ') || after_hashes.starts_with('\t') {
        let text = after_hashes.trim().to_string();
        Some((level, text))
    } else {
        None
    }
}

/// 解析无序列表项。返回 (indent, text) 或 None。
fn parse_list_item(line: &str) -> Option<(usize, String)> {
    let indent = line.chars().take_while(|c| *c == ' ').count();
    let trimmed = line[indent..].trim_start();
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
        let text = trimmed[2..].trim().to_string();
        Some((indent, text))
    } else {
        None
    }
}

/// 解析 HTML 注释。返回注释内容或 None。
fn parse_html_comment(line: &str) -> Option<String> {
    if line.starts_with("<!--") && line.ends_with("-->") {
        let content = &line[4..line.len() - 3];
        Some(content.trim().to_string())
    } else {
        None
    }
}

// ─── ATX outline parsing ───────────────────────────────────────────

struct HeadingEntry {
    level: u8,
    text: String,
    line_no: usize,
    entity_id_hint: Option<String>,
}

fn parse_atx_outline(
    lines: &[ParsedLine],
    options: &MarkdownImportOptions,
) -> Result<ImportOutput, ImportError> {
    let mut warnings: Vec<ImportWarning> = Vec::new();
    let mut headings: Vec<HeadingEntry> = Vec::new();

    // 收集 HTML 注释中 entity-id 的映射（行号 → entity_id）
    let mut entity_id_hints: Vec<(usize, String)> = Vec::new();

    // 第一遍：提取标题和注释
    let mut i = 0;
    while i < lines.len() {
        match &lines[i].kind {
            LineKind::AtxHeading { level, text } => {
                if *level > options.max_level {
                    warnings.push(ImportWarning {
                        code: "import_heading_level_skip".to_string(),
                        message: format!("heading level {} exceeds max_level {}", level, options.max_level),
                        line: Some(lines[i].line_no),
                    });
                }
                headings.push(HeadingEntry {
                    level: *level,
                    text: text.clone(),
                    line_no: lines[i].line_no,
                    entity_id_hint: None,
                });
            }
            LineKind::HtmlComment(content) => {
                // 解析 <!-- drawify:entity-id=xxx -->
                if let Some(entity_id) = parse_drawify_entity_id(content) {
                    entity_id_hints.push((lines[i].line_no, entity_id));
                }
            }
            LineKind::Blank => {}
            LineKind::Other(_) => {
                if options.strict_content {
                    warnings.push(ImportWarning {
                        code: "import_skipped_content".to_string(),
                        message: "non-outline content in strict mode".to_string(),
                        line: Some(lines[i].line_no),
                    });
                } else {
                    warnings.push(ImportWarning {
                        code: "import_skipped_content".to_string(),
                        message: "skipped non-outline content".to_string(),
                        line: Some(lines[i].line_no),
                    });
                }
            }
            LineKind::ListItem { .. } => {
                // ATX 模式下不应出现列表项，但 detect_syntax 已处理混用
            }
        }
        i += 1;
    }

    if headings.is_empty() {
        return Err(ImportError::EmptyOutline);
    }

    // 将 entity-id hints 关联到前一个标题
    for (comment_line_no, entity_id) in entity_id_hints {
        // 查找此注释之前最近的标题
        let heading_idx = headings.iter().rposition(|h| h.line_no < comment_line_no);
        if let Some(idx) = heading_idx {
            headings[idx].entity_id_hint = Some(entity_id);
        }
    }

    // 检测跳级
    for window in headings.windows(2) {
        let prev_level = window[0].level;
        let curr_level = window[1].level;
        if curr_level > prev_level + 1 {
            warnings.push(ImportWarning {
                code: "import_heading_level_skip".to_string(),
                message: format!(
                    "heading level skipped from {} to {}",
                    prev_level, curr_level
                ),
                line: Some(window[1].line_no),
            });
        }
    }

    // 处理 title/root 分离 (§7.5.1)
    let (title, root_start_idx) = determine_title_and_root(&headings);

    // 检查多个 H1
    let h1_count = headings.iter().filter(|h| h.level == 1).count();
    if h1_count > 1 && title.is_some() {
        // 当第一个 H1 被当作 title 时，其余 H1 是多个 root
        let remaining_h1 = headings[root_start_idx..]
            .iter()
            .filter(|h| h.level == 1)
            .count();
        if remaining_h1 > 1 {
            return Err(ImportError::MultipleRoots);
        }
    } else if h1_count > 1 {
        return Err(ImportError::MultipleRoots);
    }

    // 构建树
    let mut next_seq: usize = 1;
    let root = build_tree_from_headings(
        &headings[root_start_idx..],
        title.is_some(),
        options,
        &mut next_seq,
        &mut warnings,
    );

    let tree = MindmapTree {
        title,
        root,
        orphans: Vec::new(),
    };

    Ok(ImportOutput { tree, warnings })
}

/// 确定 title 与 root 的分离方式。
///
/// 规则（§7.5.1）：
/// - 单个 `#` + 子树 → `#` 是 root.label，无 diagram title
/// - `# title` + `## root` + subtree → diagram.title = title, root.label = 第二行标题文本
///
/// 启发式：如果 H1 后面只有一个 H2 子节点，且该 H2 有自己的子节点，
/// 则 H1 视为 title，H2 视为 root。否则 H1 就是 root。
fn determine_title_and_root(headings: &[HeadingEntry]) -> (Option<String>, usize) {
    if headings.len() < 2 {
        return (None, 0);
    }

    let first = &headings[0];
    let second = &headings[1];

    // 第一个必须是 H1，第二个必须是 H2
    if first.level != 1 || second.level != 2 {
        return (None, 0);
    }

    // 统计 H1 后面直接跟随的 H2 数量
    let direct_h2_count = headings[1..]
        .iter()
        .take_while(|h| h.level >= 2)
        .filter(|h| h.level == 2)
        .count();

    // 如果只有一个 H2 子节点，且该 H2 有更深的子节点（H3+），
    // 则 H1 是 title，H2 是 root
    if direct_h2_count == 1 && headings.len() > 2 && headings[2].level >= 3 {
        return (Some(first.text.clone()), 1);
    }

    // 否则 H1 就是 root
    (None, 0)
}

/// 从标题列表构建树。
fn build_tree_from_headings(
    headings: &[HeadingEntry],
    has_separate_title: bool,
    options: &MarkdownImportOptions,
    next_seq: &mut usize,
    warnings: &mut Vec<ImportWarning>,
) -> MindmapTreeNode {
    // 确定根节点的基准深度
    let base_level = headings[0].level;

    // 使用栈来构建树
    // 栈中存储 (depth_from_base, node)
    // 栈底始终是根节点
    let mut stack: Vec<(u8, MindmapTreeNode)> = Vec::new();

    for heading in headings {
        let depth_from_base = heading.level.saturating_sub(base_level);
        let label = strip_inline_formatting(&heading.text, warnings, heading.line_no);

        let entity_id = generate_entity_id(
            options.entity_id_strategy,
            &label,
            next_seq,
            heading.entity_id_hint.as_deref(),
        );

        let node = MindmapTreeNode {
            entity_id,
            label,
            entity_type: None, // 将在 mindmap_tree_to_diagram 中推断
            branch_slot: None,
            tree_depth: Some(depth_from_base as usize),
            children: Vec::new(),
        };

        // 弹出栈中深度 >= 当前深度的节点，将它们挂到父节点
        while stack.len() > 1 {
            let last_depth = stack.last().map(|(d, _)| *d).unwrap_or(0);
            if last_depth >= depth_from_base {
                let (_, completed_node) = stack.pop().unwrap();
                // 将完成的节点挂到新的栈顶
                if let Some(parent) = stack.last_mut() {
                    parent.1.children.push(completed_node);
                }
            } else {
                break;
            }
        }

        stack.push((depth_from_base, node));
    }

    // 清空栈，从内到外挂子节点
    while stack.len() > 1 {
        let (_, completed_node) = stack.pop().unwrap();
        if let Some(parent) = stack.last_mut() {
            parent.1.children.push(completed_node);
        }
    }

    let mut root = stack.pop().unwrap().1;

    // 如果有独立 title，root 的 tree_depth 应从 0 开始
    if has_separate_title {
        root.tree_depth = Some(0);
        adjust_tree_depth(&mut root, 0);
    }

    root
}

/// 递归调整 tree_depth。
fn adjust_tree_depth(node: &mut MindmapTreeNode, depth: usize) {
    node.tree_depth = Some(depth);
    for child in &mut node.children {
        adjust_tree_depth(child, depth + 1);
    }
}

/// 生成 entity id。
fn generate_entity_id(
    strategy: EntityIdStrategy,
    label: &str,
    next_seq: &mut usize,
    hint: Option<&str>,
) -> String {
    match strategy {
        EntityIdStrategy::Sequential => {
            let id = format!("node_{}", next_seq);
            *next_seq += 1;
            id
        }
        EntityIdStrategy::Slugify => {
            let slug = slugify(label);
            // 简单实现：不检测冲突，直接使用 slug
            slug
        }
        EntityIdStrategy::FromMetadata => {
            if let Some(hint_id) = hint {
                hint_id.to_string()
            } else {
                // 回退到 Sequential
                let id = format!("node_{}", next_seq);
                *next_seq += 1;
                id
            }
        }
    }
}

/// 简单 slugify：小写、空格和特殊字符替换为下划线。
fn slugify(s: &str) -> String {
    let mut result = String::new();
    let mut prev_underscore = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            result.extend(c.to_lowercase());
            prev_underscore = false;
        } else if !prev_underscore && !result.is_empty() {
            result.push('_');
            prev_underscore = true;
        }
    }
    // 去掉末尾的下划线
    let trimmed = result.trim_end_matches('_');
    if trimmed.is_empty() {
        "node".to_string()
    } else {
        trimmed.to_string()
    }
}

/// 去除行内 Markdown 格式（bold, italic, links, code）。
fn strip_inline_formatting(text: &str, warnings: &mut Vec<ImportWarning>, line_no: usize) -> String {
    let mut result = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut had_format = false;

    while i < chars.len() {
        // 代码：`code`
        if chars[i] == '`' {
            had_format = true;
            i += 1;
            while i < chars.len() && chars[i] != '`' {
                result.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                i += 1; // skip closing backtick
            }
        }
        // 粗体：**text** 或 __text__
        else if i + 1 < chars.len() && ((chars[i] == '*' && chars[i + 1] == '*') || (chars[i] == '_' && chars[i + 1] == '_')) {
            had_format = true;
            i += 2;
            let marker = chars[i - 2]; // * or _
            while i + 1 < chars.len() && !(chars[i] == marker && chars[i + 1] == marker) {
                result.push(chars[i]);
                i += 1;
            }
            if i + 1 < chars.len() {
                i += 2; // skip closing marker
            }
        }
        // 斜体：*text* 或 _text_
        else if chars[i] == '*' || chars[i] == '_' {
            had_format = true;
            let marker = chars[i];
            i += 1;
            while i < chars.len() && chars[i] != marker {
                result.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                i += 1; // skip closing marker
            }
        }
        // 链接：[text](url)
        else if chars[i] == '[' {
            had_format = true;
            i += 1;
            let mut link_text = String::new();
            while i < chars.len() && chars[i] != ']' {
                link_text.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                i += 1; // skip ]
            }
            // skip (url)
            if i < chars.len() && chars[i] == '(' {
                i += 1;
                let mut depth = 1;
                while i < chars.len() && depth > 0 {
                    if chars[i] == '(' {
                        depth += 1;
                    } else if chars[i] == ')' {
                        depth -= 1;
                    }
                    i += 1;
                }
            }
            result.push_str(&link_text);
        }
        else {
            result.push(chars[i]);
            i += 1;
        }
    }

    if had_format {
        warnings.push(ImportWarning {
            code: "import_stripped_inline_format".to_string(),
            message: "inline Markdown formatting was stripped from label".to_string(),
            line: Some(line_no),
        });
    }

    result.trim().to_string()
}

/// 解析 <!-- drawify:entity-id=xxx --> 注释。
fn parse_drawify_entity_id(content: &str) -> Option<String> {
    let content = content.trim();
    if let Some(rest) = content.strip_prefix("drawify:entity-id=") {
        let id = rest.trim().to_string();
        if !id.is_empty() {
            return Some(id);
        }
    }
    None
}

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_atx_simple() {
        let source = r#"# 产品规划

## 功能需求
## 技术方案
## 市场调研
"#;
        let options = MarkdownImportOptions::default();
        let result = parse_markdown_outline(source, &options).unwrap();
        assert!(result.tree.title.is_none());
        assert_eq!(result.tree.root.label, "产品规划");
        assert_eq!(result.tree.root.children.len(), 3);
        assert_eq!(result.tree.root.children[0].label, "功能需求");
        assert_eq!(result.tree.root.children[1].label, "技术方案");
        assert_eq!(result.tree.root.children[2].label, "市场调研");
    }

    #[test]
    fn parse_atx_with_title() {
        let source = r#"# 头脑风暴

## 产品规划

### 功能需求
### 技术方案
### 市场调研
"#;
        let options = MarkdownImportOptions::default();
        let result = parse_markdown_outline(source, &options).unwrap();
        assert_eq!(result.tree.title.as_deref(), Some("头脑风暴"));
        assert_eq!(result.tree.root.label, "产品规划");
        assert_eq!(result.tree.root.children.len(), 3);
    }

    #[test]
    fn parse_empty_outline() {
        let source = "This is just a paragraph.\nNo headings here.";
        let options = MarkdownImportOptions::default();
        let result = parse_markdown_outline(source, &options);
        assert!(matches!(result, Err(ImportError::EmptyOutline)));
    }

    #[test]
    fn parse_ambiguous_syntax() {
        let source = "# Heading\n\n- list item";
        let options = MarkdownImportOptions::default();
        let result = parse_markdown_outline(source, &options);
        assert!(matches!(result, Err(ImportError::AmbiguousSyntax)));
    }

    #[test]
    fn parse_multiple_h1() {
        let source = "# Root A\n\n## Child\n\n# Root B";
        let options = MarkdownImportOptions::default();
        let result = parse_markdown_outline(source, &options);
        assert!(matches!(result, Err(ImportError::MultipleRoots)));
    }

    #[test]
    fn parse_heading_level_skip_warning() {
        let source = "# A\n\n### B";
        let options = MarkdownImportOptions::default();
        let result = parse_markdown_outline(source, &options).unwrap();
        assert!(result.warnings.iter().any(|w| w.code == "import_heading_level_skip"));
    }

    #[test]
    fn parse_skipped_content_warning() {
        let source = "# A\n\nSome paragraph text\n\n## B";
        let options = MarkdownImportOptions::default();
        let result = parse_markdown_outline(source, &options).unwrap();
        assert!(result.warnings.iter().any(|w| w.code == "import_skipped_content"));
    }

    #[test]
    fn entity_id_sequential() {
        let source = "# A\n\n## B\n\n## C";
        let options = MarkdownImportOptions {
            entity_id_strategy: EntityIdStrategy::Sequential,
            ..Default::default()
        };
        let result = parse_markdown_outline(source, &options).unwrap();
        assert_eq!(result.tree.root.entity_id, "node_1");
        assert_eq!(result.tree.root.children[0].entity_id, "node_2");
        assert_eq!(result.tree.root.children[1].entity_id, "node_3");
    }

    #[test]
    fn entity_id_from_metadata() {
        let source = "# A\n\n<!-- drawify:entity-id=my_root -->\n\n## B";
        let options = MarkdownImportOptions {
            entity_id_strategy: EntityIdStrategy::FromMetadata,
            ..Default::default()
        };
        let result = parse_markdown_outline(source, &options).unwrap();
        assert_eq!(result.tree.root.entity_id, "my_root");
    }

    #[test]
    fn strip_bold_italic() {
        let source = "# **Bold** and *italic*";
        let options = MarkdownImportOptions::default();
        let result = parse_markdown_outline(source, &options).unwrap();
        assert_eq!(result.tree.root.label, "Bold and italic");
    }

    #[test]
    fn strip_link() {
        let source = "# [Click here](https://example.com)";
        let options = MarkdownImportOptions::default();
        let result = parse_markdown_outline(source, &options).unwrap();
        assert_eq!(result.tree.root.label, "Click here");
    }

    #[test]
    fn strip_code() {
        let source = "# Use `cargo build`";
        let options = MarkdownImportOptions::default();
        let result = parse_markdown_outline(source, &options).unwrap();
        assert_eq!(result.tree.root.label, "Use cargo build");
    }

    #[test]
    fn import_markdown_outline_produces_diagram() {
        let source = r#"# 头脑风暴

## 产品规划

### 功能需求
### 技术方案
### 市场调研
"#;
        let options = MarkdownImportOptions::default();
        let diagram = import_markdown_outline(source, &options).unwrap();
        assert_eq!(diagram.diagram_type, DiagramType::Mindmap);
        assert_eq!(diagram.title(), Some("头脑风暴"));
        // root + 3 children = 4 entities
        assert_eq!(diagram.entities.len(), 4);
        // 3 relations (root -> each child)
        assert_eq!(diagram.relations.len(), 3);
    }

    #[test]
    fn deep_nesting() {
        // 单链式深嵌套：H1 后只有一个 H2 子节点且有更深层级，
        // 启发式将其识别为 title+root 模式
        let source = "# L1\n\n## L2\n\n### L3\n\n#### L4\n\n##### L5\n\n###### L6";
        let options = MarkdownImportOptions::default();
        let result = parse_markdown_outline(source, &options).unwrap();
        assert_eq!(result.tree.title.as_deref(), Some("L1"));
        let root = &result.tree.root;
        assert_eq!(root.label, "L2");
        assert_eq!(root.children[0].label, "L3");
        assert_eq!(root.children[0].children[0].label, "L4");
        assert_eq!(root.children[0].children[0].children[0].label, "L5");
        assert_eq!(root.children[0].children[0].children[0].children[0].label, "L6");
    }

    #[test]
    fn multiple_children_at_same_level() {
        let source = "# Root\n\n## A\n\n### A1\n\n### A2\n\n## B\n\n### B1";
        let options = MarkdownImportOptions::default();
        let result = parse_markdown_outline(source, &options).unwrap();
        assert_eq!(result.tree.root.children.len(), 2);
        assert_eq!(result.tree.root.children[0].label, "A");
        assert_eq!(result.tree.root.children[0].children.len(), 2);
        assert_eq!(result.tree.root.children[1].label, "B");
        assert_eq!(result.tree.root.children[1].children.len(), 1);
    }

    #[test]
    fn single_h1_no_children() {
        let source = "# Just a root";
        let options = MarkdownImportOptions::default();
        let result = parse_markdown_outline(source, &options).unwrap();
        assert!(result.tree.title.is_none());
        assert_eq!(result.tree.root.label, "Just a root");
        assert!(result.tree.root.children.is_empty());
    }

    #[test]
    fn slugify_strategy() {
        let source = "# Hello World\n\n## Foo Bar";
        let options = MarkdownImportOptions {
            entity_id_strategy: EntityIdStrategy::Slugify,
            ..Default::default()
        };
        let result = parse_markdown_outline(source, &options).unwrap();
        assert_eq!(result.tree.root.entity_id, "hello_world");
        assert_eq!(result.tree.root.children[0].entity_id, "foo_bar");
    }
}
