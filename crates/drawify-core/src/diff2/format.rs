//! Intent Formatter — 语义 AST → DSL 文本
//!
//! 将 [`RawDiagram`] 还原为可再解析的 DSL 文本。
//!
//! 保证语义 round-trip：`parse(format(diagram))` 产出的 RawDiagram 与原 diagram 语义等价。
//! 不保留原文注释与排版；输出顺序按确定性排序（属性/元素按 key/id 排序）。
//!
//! ## 输出结构
//!
//! ```text
//! <文件头文档注释, 原样输出>
//! diagram <type> "<title>" {
//!     config {
//!         <diagram 属性, 按 key 排序>
//!     }
//!
//!     <style_decls, 按 (kind, target) 排序>
//!
//!     <顶层 groups, 按 id 排序, 递归包含 entity / 子 group>
//!
//!     <顶层 entities (group_id=None), 按 id 排序>
//!
//!     <relations, 按 (from, to, label) 排序>
//! }
//! ```

use crate::ast::*;

/// 将 RawDiagram 格式化为 DSL 文本。
pub fn format(diagram: &RawDiagram) -> String {
    let d = diagram.inner();
    let mut out = String::new();

    // 文件头文档注释原样输出
    if let Some(doc) = &d.doc_comment {
        out.push_str(doc);
    }

    out.push_str("diagram ");
    out.push_str(d.diagram_type.style_key());

    out.push_str(" {\n");

    let mut sections: Vec<String> = Vec::new();

    // title 作为 body 级别属性输出（不进 config block）
    {
        let mut s = String::new();
        if let Some(title_attr) = d.attributes.iter().find(|a| a.key == "title") {
            push_indent(&mut s, 1);
            s.push_str("title: ");
            format_value(&title_attr.value, &mut s, 1);
            s.push('\n');
            sections.push(s);
        }
    }

    // diagram 属性（排除 title，按 key 排序），包装在 config block 中
    {
        let mut s = String::new();
        let mut attrs: Vec<&DiagramAttribute> = d
            .attributes
            .iter()
            .filter(|a| a.key != "title")
            .collect();
        attrs.sort_by(|a, b| a.key.cmp(&b.key));
        if !attrs.is_empty() {
            push_indent(&mut s, 1);
            s.push_str("config {\n");
            for attr in attrs {
                push_indent(&mut s, 2);
                s.push_str(&attr.key);
                s.push_str(": ");
                format_value(&attr.value, &mut s, 2);
                s.push('\n');
            }
            push_indent(&mut s, 1);
            s.push_str("}\n");
            sections.push(s);
        }
    }

    // style_decls（按 (kind, target) 排序）
    {
        let mut s = String::new();
        let mut decls: Vec<&StyleDecl> = d.style_decls.iter().collect();
        decls.sort_by(|a, b| style_decl_sort_key(a).cmp(&style_decl_sort_key(b)));
        for decl in decls {
            format_style_decl(decl, &mut s, 1);
        }
        if !s.is_empty() {
            sections.push(s);
        }
    }

    // 顶层 groups（按 id 排序，递归）
    {
        let mut s = String::new();
        let mut top_groups: Vec<&Group> =
            d.groups.iter().filter(|g| g.parent_id.is_none()).collect();
        top_groups.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        for group in top_groups {
            format_group(group, d, &mut s, 1);
        }
        if !s.is_empty() {
            sections.push(s);
        }
    }

    // 顶层 entities（group_id=None，按 id 排序）
    {
        let mut s = String::new();
        let mut top_entities: Vec<&Entity> =
            d.entities.iter().filter(|e| e.group_id.is_none()).collect();
        top_entities.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        for entity in top_entities {
            format_entity(entity, &mut s, 1);
        }
        if !s.is_empty() {
            sections.push(s);
        }
    }

    // relations（按 (from, to, label) 排序）
    {
        let mut s = String::new();
        let mut relations: Vec<&Relation> = d.relations.iter().collect();
        relations.sort_by(|a, b| {
            let ka = (a.from.as_str(), a.to.as_str(), a.label.as_deref().unwrap_or(""));
            let kb = (b.from.as_str(), b.to.as_str(), b.label.as_deref().unwrap_or(""));
            ka.cmp(&kb)
        });
        for relation in relations {
            format_relation(relation, &mut s, 1);
        }
        if !s.is_empty() {
            sections.push(s);
        }
    }

    for (i, section) in sections.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(section);
    }

    out.push_str("}\n");
    out
}

// ─── 值格式化 ──────────────────────────────────────────────────────

/// 格式化 [`AttributeValue`]。
///
/// `indent_level` 为当前属性所在缩进层级（用于 Config 块的 options 和闭合括号缩进）。
fn format_value(value: &AttributeValue, out: &mut String, indent_level: usize) {
    match value {
        AttributeValue::String(s) => {
            if s.quoted {
                format_string(s, out);
            } else {
                out.push_str(s);
            }
        }
        AttributeValue::Number(n) => format_number(*n, out),
        AttributeValue::Boolean(b) => out.push_str(if *b { "true" } else { "false" }),
        AttributeValue::Config { algo, options } => {
            out.push_str(algo);
            out.push_str(" {\n");
            let mut keys: Vec<&String> = options.keys().collect();
            keys.sort();
            for key in &keys {
                push_indent(out, indent_level + 1);
                out.push_str(key);
                out.push_str(": ");
                format_value(&options[*key], out, indent_level + 1);
                out.push('\n');
            }
            push_indent(out, indent_level);
            out.push('}');
        }
    }
}

/// 格式化字符串字面量（含转义）。
fn format_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            _ => out.push(c),
        }
    }
    out.push('"');
}

/// 格式化数值：整数输出为 `N`，小数输出为 `N.M`。
fn format_number(n: f64, out: &mut String) {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 1e16 {
        out.push_str(&format!("{}", n as i64));
    } else {
        out.push_str(&format!("{}", n));
    }
}

// ─── 属性块格式化 ──────────────────────────────────────────────────

/// 格式化 [`AttributeMap`]：standard → style → meta，各命名空间内按 key 排序。
fn format_attributes(attrs: &AttributeMap, out: &mut String, indent_level: usize) {
    // standard
    {
        let mut keys: Vec<&String> = attrs.standard.keys().collect();
        keys.sort();
        for key in &keys {
            push_indent(out, indent_level);
            out.push_str(key);
            out.push_str(": ");
            format_value(&attrs.standard[*key], out, indent_level);
            out.push('\n');
        }
    }
    // style
    {
        let mut keys: Vec<&String> = attrs.style.keys().collect();
        keys.sort();
        for key in &keys {
            push_indent(out, indent_level);
            out.push_str("style.");
            out.push_str(key);
            out.push_str(": ");
            format_value(attrs.style.get(key).unwrap(), out, indent_level);
            out.push('\n');
        }
    }
    // meta
    {
        let mut keys: Vec<&String> = attrs.meta.keys().collect();
        keys.sort();
        for key in &keys {
            push_indent(out, indent_level);
            out.push_str("meta.");
            out.push_str(key);
            out.push_str(": ");
            format_value(&attrs.meta[*key], out, indent_level);
            out.push('\n');
        }
    }
}

fn has_attributes(attrs: &AttributeMap) -> bool {
    !attrs.standard.is_empty() || !attrs.style.is_empty() || !attrs.meta.is_empty()
}

// ─── 元素格式化 ────────────────────────────────────────────────────

fn format_entity(entity: &Entity, out: &mut String, indent_level: usize) {
    push_indent(out, indent_level);
    out.push_str("entity ");
    out.push_str(entity.id.as_str());
    out.push(' ');
    format_string(&entity.label, out);

    if has_attributes(&entity.attributes) {
        out.push_str(" {\n");
        format_attributes(&entity.attributes, out, indent_level + 1);
        push_indent(out, indent_level);
        out.push_str("}\n");
    } else {
        out.push('\n');
    }
}

fn format_relation(relation: &Relation, out: &mut String, indent_level: usize) {
    push_indent(out, indent_level);
    out.push_str(relation.from.as_str());
    out.push(' ');
    out.push_str(arrow_str(&relation.arrow));
    out.push(' ');
    out.push_str(relation.to.as_str());

    if let Some(label) = &relation.label {
        out.push(' ');
        format_string(label, out);
    }

    if let Some(h) = &relation.head_label {
        out.push_str(" >");
        format_string(h, out);
    }
    if let Some(t) = &relation.tail_label {
        out.push_str(" <");
        format_string(t, out);
    }

    if has_attributes(&relation.attributes) {
        out.push_str(" {\n");
        format_attributes(&relation.attributes, out, indent_level + 1);
        push_indent(out, indent_level);
        out.push_str("}\n");
    } else {
        out.push('\n');
    }
}

fn format_group(group: &Group, diagram: &Diagram, out: &mut String, indent_level: usize) {
    push_indent(out, indent_level);
    out.push_str("group ");
    out.push_str(group.id.as_str());
    out.push(' ');
    format_string(&group.label, out);
    out.push_str(" {\n");

    // group 属性
    format_attributes(&group.attributes, out, indent_level + 1);

    // 组内 entities（按 id 排序）
    let mut entity_ids: Vec<&Identifier> = group.entity_ids.iter().collect();
    entity_ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    for eid in entity_ids {
        if let Some(entity) = diagram.entities.iter().find(|e| &e.id == eid) {
            format_entity(entity, out, indent_level + 1);
        }
    }

    // 子 groups（按 id 排序，递归）
    let mut child_ids: Vec<&Identifier> = group.child_group_ids.iter().collect();
    child_ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    for cid in child_ids {
        if let Some(child) = diagram.groups.iter().find(|g| &g.id == cid) {
            format_group(child, diagram, out, indent_level + 1);
        }
    }

    push_indent(out, indent_level);
    out.push_str("}\n");
}

fn format_style_decl(decl: &StyleDecl, out: &mut String, indent_level: usize) {
    push_indent(out, indent_level);
    out.push_str(match decl.kind {
        StyleDeclKind::Node => "node_style ",
        StyleDeclKind::Edge => "edge_style ",
    });
    out.push_str(&decl.target);
    out.push_str(" {\n");

    let mut keys: Vec<&String> = decl.style.keys().collect();
    keys.sort();
    for key in &keys {
        push_indent(out, indent_level + 1);
        out.push_str(key);
        out.push_str(": ");
        format_value(&decl.style[*key], out, indent_level + 1);
        out.push('\n');
    }

    push_indent(out, indent_level);
    out.push_str("}\n");
}

// ─── 辅助 ──────────────────────────────────────────────────────────

fn push_indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("    ");
    }
}

fn arrow_str(arrow: &ArrowType) -> &'static str {
    match arrow {
        ArrowType::Active => "->",
        ArrowType::Passive => "-->",
        ArrowType::Bidirectional => "<->",
    }
}

fn style_decl_sort_key(decl: &StyleDecl) -> String {
    let kind = match decl.kind {
        StyleDeclKind::Node => "node_style",
        StyleDeclKind::Edge => "edge_style",
    };
    format!("{}/{}", kind, decl.target)
}
