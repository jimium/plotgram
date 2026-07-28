//! `attributes.standard` 命名空间下的结构属性键常量。
//!
//! 与 [`super::style_attr_keys`] 对称：
//! - `standard_attr_keys`：结构/语义属性（layout、type、theme 等）
//! - `style_attr_keys`：`attributes.style` 下的视觉属性（fill、stroke 等）
//!
//! 按 DSL 元素层级分子模块，便于 parser / validation 按作用域引用。

pub mod diagram;
pub mod entity;
pub mod group;
pub mod relation;

pub use diagram::*;
pub use entity::*;
pub use group::*;
pub use relation::*;
