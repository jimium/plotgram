//! SVG 调试元数据（`data-dfy-*`），由 `svg-debug` feature 控制。
//!
//! 开发阶段默认开启，便于在浏览器 DevTools 中将 DOM 元素映射回 DSL 实体/边。

use std::fmt::Write;

use crate::ast::{ArrowType, Entity, Group, Relation};

use super::svg_utils;

fn arrow_type_name(arrow: &ArrowType) -> &'static str {
    match arrow {
        ArrowType::Active => "active",
        ArrowType::Passive => "passive",
        ArrowType::Bidirectional => "bidirectional",
    }
}

fn append_source_line(attrs: &mut String, line: usize) {
    if line > 0 {
        write!(attrs, r#" data-dfy-source-line="{line}""#).unwrap();
    }
}

/// 根 `<svg>` 上的调试标记。
pub fn svg_root_attr() -> &'static str {
    #[cfg(feature = "svg-debug")]
    {
        r#" data-dfy-debug="1""#
    }
    #[cfg(not(feature = "svg-debug"))]
    {
        ""
    }
}

pub fn open_node_g(entity: &Entity, svg: &mut String) {
    #[cfg(feature = "svg-debug")]
    {
        let id = svg_utils::escape_xml(entity.id.as_str());
        let mut attrs = format!(r#"data-dfy-kind="node" data-dfy-id="{id}""#);
        append_source_line(&mut attrs, entity.span.start.line);
        writeln!(svg, r#"<g {attrs}>"#).unwrap();
    }
}

pub fn open_edge_g(index: usize, relation: &Relation, svg: &mut String) {
    #[cfg(feature = "svg-debug")]
    {
        let from = svg_utils::escape_xml(relation.from.as_str());
        let to = svg_utils::escape_xml(relation.to.as_str());
        let arrow = arrow_type_name(&relation.arrow);
        let mut attrs = format!(
            r#"data-dfy-kind="edge" data-dfy-index="{index}" data-dfy-from="{from}" data-dfy-to="{to}" data-dfy-arrow="{arrow}""#
        );
        append_source_line(&mut attrs, relation.span.start.line);
        writeln!(svg, r#"<g {attrs}>"#).unwrap();
    }
}

pub fn open_edge_label_g(index: usize, relation: &Relation, svg: &mut String) {
    #[cfg(feature = "svg-debug")]
    {
        let from = svg_utils::escape_xml(relation.from.as_str());
        let to = svg_utils::escape_xml(relation.to.as_str());
        writeln!(
            svg,
            r#"<g data-dfy-kind="edge-label" data-dfy-index="{index}" data-dfy-from="{from}" data-dfy-to="{to}">"#
        )
        .unwrap();
    }
}

pub fn open_group_g(group: &Group, svg: &mut String) {
    #[cfg(feature = "svg-debug")]
    {
        let id = svg_utils::escape_xml(group.id.as_str());
        let mut attrs = format!(r#"data-dfy-kind="group" data-dfy-id="{id}""#);
        append_source_line(&mut attrs, group.span.start.line);
        writeln!(svg, r#"<g {attrs}>"#).unwrap();
    }
}

pub fn close_g(svg: &mut String) {
    #[cfg(feature = "svg-debug")]
    writeln!(svg, "</g>").unwrap();
}
