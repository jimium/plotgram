//! 节点装饰图标库。
//!
//! 提供 catalog 注册表、semantic 词表、实体解析与 SVG 渲染辅助。

pub mod layout;
pub mod catalog;
pub mod registry;
pub mod render;
pub mod resolve;
pub mod semantic_resolve;
pub mod validate;

pub use catalog::{
    all_icons, icon_by_id, icon_by_key, known_icon_ids, normalize_key, IconCategory, IconDef,
    IconPlacement,
};
pub use registry::{is_known_semantic, known_semantic_count, normalize_semantic, semantic_to_icon_id};
pub use layout::apply_icon_to_node_size;
pub use render::{
    can_render, extra_node_width, layout_inside, render_entity_content, render_icon, render_inside,
    IconLayout,
};
pub use resolve::{node_shape_from_entity, resolve, ResolveOptions};
pub use semantic_resolve::SemanticResolveOptions;
pub use validate::validate_entity_semantic_icon;
