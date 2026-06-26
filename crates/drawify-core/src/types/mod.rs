//! 跨模块共享的类型与常量定义。
//!
//! ## 属性元数据（三层对称）
//!
//! | 模块 | 职责 | 示例 |
//! |------|------|------|
//! | [`standard_attr_keys`] | key 名称常量 | `TYPE`、`STATUS`、`DIRECTION` |
//! | [`attr_constants`] | value 枚举值常量 | `entity_type::SERVICE`、`status::HEALTHY` |
//! | [`attr_schema`] | schema 定义（key + scope + value_type + enum_values） | `AttrSchema { key, ... }` |
//!
//! ## 属性键常量（对称命名）
//!
//! | 模块 | 命名空间 | 职责 |
//! |------|----------|------|
//! | [`standard_attr_keys`] | `attributes.standard` | 结构/语义属性（layout、type、theme…） |
//! | [`style_attr_keys`] | `attributes.style` | 视觉样式属性（fill、stroke、shape…） |
//!
//! ## 其他
//!
//! - [`diagram_type`]：语言判别式
//! - [`graphic_style_id`] / [`style_attrs`]：样式标识与校验

pub mod attr_constants;
pub mod attr_schema;
pub mod diagram_type;
pub mod graphic_style_id;
pub mod standard_attr_keys;
pub mod style_attr_keys;
pub mod style_attrs;

pub use attr_schema::{AttrScope, AttrSchema, AttrValueType};
pub use diagram_type::DiagramType;
pub use graphic_style_id::GraphicStyleId;
