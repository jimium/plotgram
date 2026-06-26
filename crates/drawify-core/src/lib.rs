//! # Drawify Core
//!
//! Drawify 的核心库，提供解析、AST、验证、渲染和 Diff2/Patch 功能。
//! 被 CLI、Server 和 WASM 三个前端共享。
//!
//! 调用链遵循单向流水线,编排逻辑集中在 [`pipeline`] 模块:
//!
//! ```text
//! DSL 源码
//!   → parser::parse()           → RawDiagram
//!   → prepare::prepare()        → PreparedDiagram
//!   → validation::validate()    → PreparedDiagram (已校验)
//!   → scene::compute_layout()   → LayoutResult
//!   → scene::build_scene()      → ExportScene
//!   → encode::encode_scene()    → RenderOutput
//! ```

pub mod ast;
pub mod kinds;
pub mod profile;
pub mod diff2;
pub mod error;
pub mod interchange;
pub mod prepare;
pub mod graphic_style;
pub mod icons;
pub mod layout;
pub mod dsl;
pub mod pipeline;
pub mod render;
pub mod types;
pub mod theme;
pub mod validation;

pub use dsl::{lexer, parser};
pub use pipeline::{
    import_prepare_validate, parse, parse_prepare, parse_prepare_validate, render_bytes,
    render_json, render_output, render_text, render_with_style_json, run, PipelineOutput,
    PipelineResult, PrepareOutput,
};
pub use render::RenderFormat;
