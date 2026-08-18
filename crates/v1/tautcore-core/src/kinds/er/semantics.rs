//! ER 图共享逻辑：实体列解析、节点尺寸、关系基数。
//!
//! 供 `render`、`validate` 与 `layout`（sugiyama ER 节点尺寸）复用。

use crate::ast::{AttributeValue, Entity, Relation};
use crate::layout::geometry::Point;

const HEADER_HEIGHT: f64 = 28.0;
const ROW_HEIGHT: f64 = 16.0;
const PADDING_Y: f64 = 8.0;
const MIN_WIDTH: f64 = 120.0;
const MAX_WIDTH: f64 = 240.0;
const CHAR_WIDTH: f64 = 7.2;
const SIDE_PADDING: f64 = 24.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErColumnKind {
    PrimaryKey,
    ForeignKey,
    Field,
}

#[derive(Debug, Clone)]
pub struct ErColumn {
    pub name: String,
    pub kind: ErColumnKind,
}

/// 解析实体应展示的列（主键、外键、普通字段）。
pub fn entity_columns(entity: &Entity) -> Vec<ErColumn> {
    let mut columns = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let mut push = |name: String, kind: ErColumnKind| {
        let key = name.to_lowercase();
        if name.is_empty() || !seen.insert(key) {
            return;
        }
        columns.push(ErColumn { name, kind });
    };

    if let Some(pk) = meta_string(entity, "pk").or(standard_string(entity, "pk")) {
        push(pk, ErColumnKind::PrimaryKey);
    }
    if let Some(fk) = meta_string(entity, "fk").or(standard_string(entity, "fk")) {
        push(fk, ErColumnKind::ForeignKey);
    }

    if let Some(fields) = meta_string(entity, "fields").or(standard_string(entity, "attributes")) {
        for line in fields.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(rest) = line.strip_prefix("fk.") {
                push(rest.to_string(), ErColumnKind::ForeignKey);
            } else {
                push(line.to_string(), ErColumnKind::Field);
            }
        }
    }

    columns
}

/// 根据表名与列数计算节点尺寸。
pub fn entity_node_size(entity: &Entity) -> (f64, f64) {
    let columns = entity_columns(entity);
    let label_chars = entity.label.chars().count() as f64;
    let max_col_chars = columns
        .iter()
        .map(|c| c.name.chars().count())
        .max()
        .unwrap_or(0) as f64;

    let content_chars = label_chars.max(max_col_chars);
    let width = (content_chars * CHAR_WIDTH + SIDE_PADDING).clamp(MIN_WIDTH, MAX_WIDTH);

    let body_rows = columns.len().max(1) as f64;
    let height = HEADER_HEIGHT + body_rows * ROW_HEIGHT + PADDING_Y;

    (width, height)
}

pub const ER_HEADER_HEIGHT: f64 = HEADER_HEIGHT;
pub const ER_ROW_HEIGHT: f64 = ROW_HEIGHT;

/// 关系基数 `(from端, to端)`，优先读 `cardinality` 属性，其次从 label 前缀解析。
pub fn relation_cardinality(relation: &Relation) -> Option<(String, String)> {
    if let Some(card) = standard_string_relation(relation, "cardinality") {
        if let Some(pair) = parse_cardinality(&card) {
            return Some(pair);
        }
    }
    relation
        .label
        .as_deref()
        .and_then(parse_cardinality_from_label)
}

/// 关系语义标签（去掉 label 中的基数前缀）。
pub fn relation_semantic_label(relation: &Relation) -> Option<String> {
    let label = relation.label.as_deref()?;
    if let Some((_, rest)) = split_cardinality_prefix(label) {
        let trimmed = rest.trim();
        if trimmed.is_empty() {
            return None;
        }
        return Some(trimmed.to_string());
    }
    Some(label.to_string())
}

pub fn is_valid_cardinality(value: &str) -> bool {
    parse_cardinality(value).is_some()
}

fn meta_string(entity: &Entity, key: &str) -> Option<String> {
    entity
        .attributes
        .meta
        .get(key)
        .and_then(attribute_as_string)
}

fn standard_string(entity: &Entity, key: &str) -> Option<String> {
    entity
        .attributes
        .standard
        .get(key)
        .and_then(attribute_as_string)
}

fn standard_string_relation(relation: &Relation, key: &str) -> Option<String> {
    relation
        .attributes
        .standard
        .get(key)
        .and_then(attribute_as_string)
}

fn attribute_as_string(value: &AttributeValue) -> Option<String> {
    match value {
        AttributeValue::String(s) => Some(s.value.clone()),
        _ => None,
    }
}

fn parse_cardinality(value: &str) -> Option<(String, String)> {
    let value = value.trim();
    let (left, right) = value.split_once(':')?;
    let left = left.trim();
    let right = right.trim();
    if left.is_empty() || right.is_empty() {
        return None;
    }
    if !is_card_side(left) || !is_card_side(right) {
        return None;
    }
    Some((left.to_string(), right.to_string()))
}

fn is_card_side(side: &str) -> bool {
    matches!(side, "1" | "N" | "M" | "0" | "0..1" | "0..N" | "1..N")
        || side.chars().all(|c| c.is_ascii_digit())
}

fn parse_cardinality_from_label(label: &str) -> Option<(String, String)> {
    split_cardinality_prefix(label).map(|(card, _)| card)
}

fn split_cardinality_prefix(label: &str) -> Option<((String, String), &str)> {
    let label = label.trim();
    let (head, rest) = label.split_once(char::is_whitespace).unwrap_or((label, ""));
    parse_cardinality(head).map(|card| (card, rest))
}

/// 沿折线路径按弧长比例取点，返回坐标与切线角度（弧度）。
pub fn point_along_path(path: &[Point], t: f64) -> Option<(Point, f64)> {
    if path.len() < 2 {
        return None;
    }
    if path.len() == 2 {
        let p1 = path[0];
        let p2 = path[1];
        let angle = (p2.y - p1.y).atan2(p2.x - p1.x);
        return Some((Point::new(p1.x + (p2.x - p1.x) * t, p1.y + (p2.y - p1.y) * t), angle));
    }

    let mut segments: Vec<(f64, f64, f64, f64, f64)> = Vec::new();
    let mut total = 0.0;
    for w in path.windows(2) {
        let len = hypot(w[1].x - w[0].x, w[1].y - w[0].y);
        segments.push((w[0].x, w[0].y, w[1].x, w[1].y, len));
        total += len;
    }
    if total < 1e-6 {
        return Some((path[0], 0.0));
    }

    let target = (t.clamp(0.0, 1.0)) * total;
    let mut acc = 0.0;
    for (x1, y1, x2, y2, len) in segments {
        if acc + len >= target {
            let local = if len < 1e-6 { 0.0 } else { (target - acc) / len };
            let x = x1 + (x2 - x1) * local;
            let y = y1 + (y2 - y1) * local;
            let angle = (y2 - y1).atan2(x2 - x1);
            return Some((Point::new(x, y), angle));
        }
        acc += len;
    }

    let last = path[path.len() - 1];
    let prev = path[path.len() - 2];
    let angle = (last.y - prev.y).atan2(last.x - prev.x);
    Some((last, angle))
}

fn hypot(dx: f64, dy: f64) -> f64 {
    (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AttributeMap, Identifier, Span, TextValue};

    fn span() -> Span {
        Span::dummy()
    }

    fn entity_with_meta(pk: &str, fk: Option<&str>, fields: Option<&str>) -> Entity {
        let mut attributes = AttributeMap::default();
        attributes
            .meta
            .insert("pk".to_string(), AttributeValue::String(TextValue::quoted(pk.to_string())));
        if let Some(fk) = fk {
            attributes
                .meta
                .insert("fk".to_string(), AttributeValue::String(TextValue::quoted(fk.to_string())));
        }
        if let Some(fields) = fields {
            attributes.meta.insert(
                "fields".to_string(),
                AttributeValue::String(TextValue::quoted(fields.to_string())),
            );
        }
        Entity {
            id: Identifier::new("user").unwrap(),
            label: "User".to_string(),
            attributes,
            group_id: None,
            span: span(),
        }
    }

    #[test]
    fn entity_columns_reads_meta_namespace() {
        let entity = entity_with_meta("id", Some("tenant_id"), Some("name\nemail"));
        let cols = entity_columns(&entity);
        assert_eq!(cols.len(), 4);
        assert_eq!(cols[0].kind, ErColumnKind::PrimaryKey);
        assert_eq!(cols[1].kind, ErColumnKind::ForeignKey);
    }

    #[test]
    fn entity_node_size_grows_with_columns() {
        let small = entity_with_meta("id", None, None);
        let large = entity_with_meta("id", Some("fk"), Some("a\nb\nc\nd"));
        let (_, h1) = entity_node_size(&small);
        let (_, h2) = entity_node_size(&large);
        assert!(h2 > h1);
    }

    #[test]
    fn relation_cardinality_from_attribute_and_label() {
        let mut attrs = AttributeMap::default();
        attrs.standard.insert(
            "cardinality".to_string(),
            AttributeValue::String(TextValue::quoted("1:N".to_string())),
        );
        let rel = Relation {
            from: Identifier::new("a").unwrap(),
            to: Identifier::new("b").unwrap(),
            arrow: crate::ast::ArrowType::Active,
            label: Some("拥有".to_string()),
            head_label: None,
            tail_label: None,
            attributes: attrs,
            span: span(),
        };
        assert_eq!(
            relation_cardinality(&rel),
            Some(("1".to_string(), "N".to_string()))
        );

        let rel2 = Relation {
            from: Identifier::new("a").unwrap(),
            to: Identifier::new("b").unwrap(),
            arrow: crate::ast::ArrowType::Active,
            label: Some("N:M 关联".to_string()),
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: span(),
        };
        assert_eq!(
            relation_cardinality(&rel2),
            Some(("N".to_string(), "M".to_string()))
        );
        assert_eq!(relation_semantic_label(&rel2), Some("关联".to_string()));
    }

    #[test]
    fn point_along_path_interpolates() {
        let path = vec![Point::new(0.0, 0.0), Point::new(100.0, 0.0)];
        let (p, _) = point_along_path(&path, 0.5).unwrap();
        assert!((p.x - 50.0).abs() < 0.01);
    }
}
