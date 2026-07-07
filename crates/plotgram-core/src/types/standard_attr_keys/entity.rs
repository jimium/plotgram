//! Entity 级 `attributes.standard` 键。

// 解析策略：`parse_atom_attribute_value`

/// 实体类型
pub const TYPE: &str = "type";
/// 状态（healthy / degraded / down / unknown）
pub const STATUS: &str = "status";
/// 全局语义标签（驱动图标推断）
pub const SEMANTIC: &str = "semantic";
/// 显式图标 id；`none` 表示不渲染图标
pub const ICON: &str = "icon";

// 解析策略：`parse_string_attribute_value`

/// 负责人
pub const OWNER: &str = "owner";
/// 描述
pub const DESCRIPTION: &str = "description";

// 解析策略：`parse_number_attribute_value`（mindmap 结构派生，DSL 可 override）

/// 分支槽位（mindmap：root 的第几个直接子树，0-based）
pub const BRANCH_SLOT: &str = "branch_slot";
/// 树深度（mindmap：距 root 的深度，root=0）
pub const TREE_DEPTH: &str = "tree_depth";
