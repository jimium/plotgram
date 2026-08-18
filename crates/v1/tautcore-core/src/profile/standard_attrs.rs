//! Profile 与 validation/prepare 共享的 `attributes.standard` 属性键契约。
//!
//! 键列表与 [`crate::types::attr_schema`] 的 `ENTITY_ATTRS` / `RELATION_ATTRS` 保持同步，
//! 由 `schema_attrs_match_*` 测试保证一致性。新增属性只需在 schema 数组中添加一项，
//! 并在此处同步追加常量（测试会捕获遗漏）。

/// 所有图表类型下 Entity 允许的 standard 属性键。
///
/// ER 等图虽无 `default_entity_type`，仍允许显式 `type`（如 `database` 用于图标语义）；
/// 值域是否闭集校验由 [`super::DiagramProfile::restricts_entity_type_values`] 决定。
pub const STANDARD_ENTITY_ATTRS: &[&str] = &[
    crate::types::standard_attr_keys::entity::TYPE,
    crate::types::standard_attr_keys::entity::STATUS,
    crate::types::standard_attr_keys::entity::SEMANTIC,
    crate::types::standard_attr_keys::entity::ICON,
    crate::types::standard_attr_keys::entity::OWNER,
    crate::types::standard_attr_keys::entity::DESCRIPTION,
    crate::types::standard_attr_keys::entity::BRANCH_SLOT,
    crate::types::standard_attr_keys::entity::TREE_DEPTH,
];

/// 所有图表类型下 Relation 允许的 standard 属性键。
pub const STANDARD_RELATION_ATTRS: &[&str] = &[
    crate::types::standard_attr_keys::relation::STATUS,
    crate::types::standard_attr_keys::relation::LINE_STYLE,
    crate::types::standard_attr_keys::relation::CARDINALITY,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::attr_schema::{ENTITY_ATTRS, RELATION_ATTRS};

    /// 保证 `STANDARD_ENTITY_ATTRS` 与 `ENTITY_ATTRS` schema 同步。
    #[test]
    fn entity_attrs_match_schema() {
        let schema_keys: Vec<&str> = ENTITY_ATTRS.iter().map(|s| s.key).collect();
        assert_eq!(
            schema_keys.as_slice(),
            STANDARD_ENTITY_ATTRS,
            "STANDARD_ENTITY_ATTRS 与 ENTITY_ATTRS schema 不同步：新增属性需同时更新 schema 和此常量"
        );
    }

    /// 保证 `STANDARD_RELATION_ATTRS` 与 `RELATION_ATTRS` schema 同步。
    #[test]
    fn relation_attrs_match_schema() {
        let schema_keys: Vec<&str> = RELATION_ATTRS.iter().map(|s| s.key).collect();
        assert_eq!(
            schema_keys.as_slice(),
            STANDARD_RELATION_ATTRS,
            "STANDARD_RELATION_ATTRS 与 RELATION_ATTRS schema 不同步：新增属性需同时更新 schema 和此常量"
        );
    }
}
