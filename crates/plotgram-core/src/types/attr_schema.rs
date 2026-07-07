//! 属性 schema 注册表。
//!
//! 将分散在 parser / validation / profile 三处的属性定义集中为声明式 schema，
//! 作为属性元数据的唯一真源。新增属性只需改 schema 一处。
//!
//! - key：属性键名（引用 [`super::standard_attr_keys`] 常量）
//! - value_type：值类型（决定 parser 解析策略与 validation 校验方式）
//! - enum_values：闭集枚举值（引用 [`super::attr_constants`] 常量），`None` 表示开集

use super::attr_constants;
use super::standard_attr_keys::{diagram, entity, group, relation};

/// 属性所属的作用域。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AttrScope {
    Diagram,
    Entity,
    Group,
    Relation,
}

/// 属性值类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AttrValueType {
    /// 带引号的文本字符串
    Text,
    /// atom（小写标识符，可含连字符/点号）
    Atom,
    /// 数字
    Number,
    /// 布尔值
    Boolean,
    /// 算法配置块（`algo` 或 `algo { key: value }`）
    AlgorithmConfig,
}

/// 属性 schema 定义。
#[derive(Debug, serde::Serialize)]
pub struct AttrSchema {
    pub key: &'static str,
    pub scope: AttrScope,
    pub value_type: AttrValueType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<&'static [&'static str]>,
}

impl AttrSchema {
    const fn new(
        key: &'static str,
        scope: AttrScope,
        value_type: AttrValueType,
        enum_values: Option<&'static [&'static str]>,
    ) -> Self {
        Self {
            key,
            scope,
            value_type,
            enum_values,
        }
    }
}

/// Diagram 级属性 schema。
pub const DIAGRAM_ATTRS: &[AttrSchema] = &[
    AttrSchema::new(
        diagram::TITLE,
        AttrScope::Diagram,
        AttrValueType::Text,
        None,
    ),
    AttrSchema::new(
        diagram::DIRECTION,
        AttrScope::Diagram,
        AttrValueType::Atom,
        Some(attr_constants::direction::ALL),
    ),
    AttrSchema::new(
        diagram::LAYOUT,
        AttrScope::Diagram,
        AttrValueType::AlgorithmConfig,
        None,
    ),
    AttrSchema::new(
        diagram::EDGE_ROUTING,
        AttrScope::Diagram,
        AttrValueType::AlgorithmConfig,
        None,
    ),
    AttrSchema::new(
        diagram::RENDER_STYLE,
        AttrScope::Diagram,
        AttrValueType::Atom,
        None,
    ),
    AttrSchema::new(
        diagram::THEME,
        AttrScope::Diagram,
        AttrValueType::Atom,
        None,
    ),
    AttrSchema::new(
        diagram::GROUP_SIZING,
        AttrScope::Diagram,
        AttrValueType::Atom,
        Some(attr_constants::group_sizing::ALL),
    ),
    AttrSchema::new(
        diagram::GROUP_GAP,
        AttrScope::Diagram,
        AttrValueType::Number,
        None,
    ),
    AttrSchema::new(
        diagram::GROUP_ALIGN,
        AttrScope::Diagram,
        AttrValueType::Atom,
        Some(attr_constants::group_align::ALL),
    ),
    AttrSchema::new(
        diagram::GROUP_ARRANGEMENT,
        AttrScope::Diagram,
        AttrValueType::Atom,
        Some(attr_constants::group_arrangement::ALL),
    ),
    AttrSchema::new(
        diagram::SNAP,
        AttrScope::Diagram,
        AttrValueType::Boolean,
        None,
    ),
];

/// Entity 级属性 schema。
pub const ENTITY_ATTRS: &[AttrSchema] = &[
    AttrSchema::new(
        entity::TYPE,
        AttrScope::Entity,
        AttrValueType::Atom,
        None, // diagram-type-specific 收窄由 profile 处理
    ),
    AttrSchema::new(
        entity::STATUS,
        AttrScope::Entity,
        AttrValueType::Atom,
        Some(attr_constants::status::ALL),
    ),
    AttrSchema::new(entity::SEMANTIC, AttrScope::Entity, AttrValueType::Atom, None),
    AttrSchema::new(entity::ICON, AttrScope::Entity, AttrValueType::Atom, None),
    AttrSchema::new(entity::OWNER, AttrScope::Entity, AttrValueType::Text, None),
    AttrSchema::new(
        entity::DESCRIPTION,
        AttrScope::Entity,
        AttrValueType::Text,
        None,
    ),
    AttrSchema::new(
        entity::BRANCH_SLOT,
        AttrScope::Entity,
        AttrValueType::Number,
        None,
    ),
    AttrSchema::new(
        entity::TREE_DEPTH,
        AttrScope::Entity,
        AttrValueType::Number,
        None,
    ),
];

/// Group 级属性 schema。
pub const GROUP_ATTRS: &[AttrSchema] = &[
    AttrSchema::new(
        group::BORDER_STYLE,
        AttrScope::Group,
        AttrValueType::Atom,
        Some(attr_constants::group_border_style::ALL),
    ),
    AttrSchema::new(group::COLOR, AttrScope::Group, AttrValueType::Text, None),
    AttrSchema::new(
        group::LAYOUT,
        AttrScope::Group,
        AttrValueType::Atom,
        Some(attr_constants::group_layout::ALL),
    ),
];

/// Relation 级属性 schema。
pub const RELATION_ATTRS: &[AttrSchema] = &[
    AttrSchema::new(
        relation::STATUS,
        AttrScope::Relation,
        AttrValueType::Atom,
        Some(attr_constants::status::ALL),
    ),
    AttrSchema::new(
        relation::LINE_STYLE,
        AttrScope::Relation,
        AttrValueType::Atom,
        None,
    ),
    AttrSchema::new(
        relation::CARDINALITY,
        AttrScope::Relation,
        AttrValueType::Text,
        None,
    ),
];

/// 按 scope 名称获取属性 schema 列表。
///
/// `scope`: `"diagram"` | `"entity"` | `"group"` | `"relation"`
pub fn schema_for_scope(scope: &str) -> Option<&'static [AttrSchema]> {
    match scope {
        "diagram" => Some(DIAGRAM_ATTRS),
        "entity" => Some(ENTITY_ATTRS),
        "group" => Some(GROUP_ATTRS),
        "relation" => Some(RELATION_ATTRS),
        _ => None,
    }
}

/// 按 key 查找某 scope 下的 schema。
pub fn find_schema(scope: AttrScope, key: &str) -> Option<&'static AttrSchema> {
    let list = match scope {
        AttrScope::Diagram => DIAGRAM_ATTRS,
        AttrScope::Entity => ENTITY_ATTRS,
        AttrScope::Group => GROUP_ATTRS,
        AttrScope::Relation => RELATION_ATTRS,
    };
    list.iter().find(|s| s.key == key)
}

/// 获取某 key 的枚举值列表（跨所有 scope 查找）。
pub fn enum_values_for_key(key: &str) -> Option<&'static [&'static str]> {
    DIAGRAM_ATTRS
        .iter()
        .chain(ENTITY_ATTRS.iter())
        .chain(GROUP_ATTRS.iter())
        .chain(RELATION_ATTRS.iter())
        .find(|s| s.key == key)
        .and_then(|s| s.enum_values)
}
