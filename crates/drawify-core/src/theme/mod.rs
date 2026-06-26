//! # 主题系统
//!
//! 实现 `docs/specs/style-system/theme-inheritance-plan.md` 描述的新架构：
//! - **单层 `extends`** 子主题薄层化
//! - **`tokens.palette` + `{role.*}`** 类型级配色（compile 期展开）
//! - **`context_palettes` + `InstanceContext`** 实例级配色（prepare 期物化）
//! - **`CompiledTheme`** 单层查表（不保留运行时 cascade 热路径）
//! - **compile 期颜色函数** `lighten()` / `darken()`
//!
//! ## 模块结构
//!
//! | 模块 | 职责 |
//! |------|------|
//! | `schema` | 磁盘形态 + `CompiledTheme`、`CompiledRenderContext`、`InstanceContext`、`ThemeContext` |
//! | `loader` | `parse_style_sheet_json`、`validate_basic`、`StyleValidationError`、`is_valid_color` |
//! | `select` | `resolve_theme_id`、`ThemeIdResolver` |
//! | `merge` | `merge_style_sheets`（单层 `extends`） |
//! | `compile` | `compile_theme`、L1 展开、palette 展开、legacy 提升、`lighten`/`darken`、`validate_style_sheet` |
//! | `context_palette` | `InstanceContext`、`apply_context_palettes`、`materialize_*` |
//! | `builtin` | 缓存、`compiled_builtin_theme` |
//! | `context` | `ThemeContext` |
//! | `materialize` | `materialize_diagram_styles` |

pub mod builtin;
pub mod compile;
pub mod context;
pub mod context_palette;
pub mod loader;
pub mod materialize;
pub mod merge;
pub mod schema;
pub mod select;

// ─── 受控公开导出 ──────────────────────────────────────────────────

pub use schema::{
    CompiledContextPalette, CompiledDiagram, CompiledTheme, CompiledRenderContext,
    IndexRule, InstanceContext, ThemeContext,
    // 磁盘形态类型
    StyleBlock, StyleValue, StyleSheet, StyleTokens, NodeShape, StyleMeta,
    ElementDefaults, DiagramStyles, ContextPaletteDef, IndexRuleDef, ContextBindingDef,
    BranchPaletteEntry, PaletteRole,
    ResolvedStyleSheet, ResolvedDiagramStyles, ResolvedBranchPaletteEntry,
    KNOWN_DIAGRAM_TYPES, SUPPORTED_VERSIONS,
};

pub use loader::{parse_style_sheet_json, StyleValidationError, is_valid_color};
pub use select::{resolve_theme_id, ThemeIdResolver, is_internal_base};
pub use merge::merge_style_sheets;
pub use compile::{compile_theme, validate_style_sheet, lighten, darken};
pub use builtin::{compiled_builtin_theme, all_theme_ids, COMMON_THEME_IDS, MINDMAP_THEME_IDS};
pub use context_palette::{materialize_node, materialize_edge, materialize_group, derive_edge_context};
pub use materialize::materialize_diagram_styles;
