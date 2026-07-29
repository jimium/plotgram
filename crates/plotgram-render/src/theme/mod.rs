//! Theme system: loading, compilation, and resolved access.
//!
//! V2 themes are flat: no `diagrams` section. Visual variance is driven by `kind`.

pub mod compile;
pub mod schema;

use std::collections::BTreeMap;
use std::sync::OnceLock;

/// A fully compiled theme with all token references resolved.
#[derive(Debug, Clone)]
pub struct CompiledTheme {
    pub id: String,
    pub name: String,
    pub defaults: CompiledDefaults,
    pub kind_styles: BTreeMap<String, KindStyle>,
    pub tokens: schema::Tokens,
}

/// Resolved default styles.
#[derive(Debug, Clone)]
pub struct CompiledDefaults {
    pub canvas_background: String,
    pub title_fill: String,
    pub title_font_size: f64,
    pub node: KindStyle,
    pub edge: EdgeDefaults,
    pub group: GroupDefaults,
    pub typography: Typography,
}

/// Style entry for a kind (or the default node style).
#[derive(Debug, Clone)]
pub struct KindStyle {
    pub fill: String,
    pub stroke: String,
    pub stroke_width: f64,
    pub text_fill: String,
    pub font_size: f64,
    pub font_weight: Option<String>,
    pub shape: Option<String>,
    pub radius: Option<f64>,
    pub stroke_dasharray: Option<String>,
    pub stroke_linecap: Option<String>,
    pub stroke_linejoin: Option<String>,
    pub fill_opacity: Option<f64>,
    pub stroke_opacity: Option<f64>,
}

/// Edge default styles.
#[derive(Debug, Clone)]
pub struct EdgeDefaults {
    pub stroke: String,
    pub stroke_width: f64,
    pub text_fill: String,
    pub font_size: f64,
    pub arrow_fill: String,
    pub arrow_style: String,
    /// Dash pattern for `-->` response edges (theme-customizable).
    pub response_dasharray: String,
    pub stroke_linecap: Option<String>,
    pub stroke_linejoin: Option<String>,
    pub stroke_opacity: Option<f64>,
    pub label_bg: Option<String>,
    pub label_bg_opacity: f64,
}

/// Group default styles.
#[derive(Debug, Clone)]
pub struct GroupDefaults {
    pub fill: String,
    pub stroke: String,
    pub stroke_width: f64,
    pub text_fill: String,
    pub radius: f64,
    pub stroke_dasharray: Option<String>,
}

/// Typography tokens.
#[derive(Debug, Clone)]
pub struct Typography {
    pub font_family: String,
    pub title_size: f64,
    pub label_size: f64,
    pub small_size: f64,
}

// ─── Loading ──────────────────────────────────────────────────────

const BUILTIN_THEME_IDS: &[&str] = &[
    "common.clean-light",
    "common.clean-dark",
    "common.blueprint",
    "common.paper-ink",
    "common.github-light",
    "common.github-dark",
    "common.presentation",
    "common.floating-cards",
    "common.dual-channel",
    "common.okabe-ito",
    "mindmap.base",
    "mindmap.ink-dark",
    "mindmap.vivid-branches",
];

static THEME_CACHE: OnceLock<BTreeMap<&'static str, CompiledTheme>> = OnceLock::new();

fn theme_cache() -> &'static BTreeMap<&'static str, CompiledTheme> {
    THEME_CACHE.get_or_init(|| {
        let mut map = BTreeMap::new();
        for &id in BUILTIN_THEME_IDS {
            if let Some(json) = embedded_theme(id) {
                map.insert(id, compile::compile_theme(json, &|tid| embedded_theme(tid)));
            }
        }
        map
    })
}

/// Load and compile a theme by id. Falls back to `common.clean-light`.
pub fn load(theme_id: Option<&str>) -> CompiledTheme {
    let id = theme_id.unwrap_or("common.clean-light");
    let cache = theme_cache();
    cache
        .get(id)
        .or_else(|| cache.get("common.clean-light"))
        .expect("builtin theme common.clean-light missing")
        .clone()
}

/// Embedded theme JSON (include_str at compile time).
fn embedded_theme(id: &str) -> Option<&'static str> {
    match id {
        "common.clean-light" => Some(include_str!("themes/common.clean-light.json")),
        "common.clean-dark" => Some(include_str!("themes/common.clean-dark.json")),
        "common.blueprint" => Some(include_str!("themes/common.blueprint.json")),
        "common.paper-ink" => Some(include_str!("themes/common.paper-ink.json")),
        "common.github-light" => Some(include_str!("themes/common.github-light.json")),
        "common.github-dark" => Some(include_str!("themes/common.github-dark.json")),
        "common.presentation" => Some(include_str!("themes/common.presentation.json")),
        "common.floating-cards" => Some(include_str!("themes/common.floating-cards.json")),
        "common.dual-channel" => Some(include_str!("themes/common.dual-channel.json")),
        "common.okabe-ito" => Some(include_str!("themes/common.okabe-ito.json")),
        "mindmap.base" => Some(include_str!("themes/mindmap.base.json")),
        "mindmap.ink-dark" => Some(include_str!("themes/mindmap.ink-dark.json")),
        "mindmap.vivid-branches" => Some(include_str!("themes/mindmap.vivid-branches.json")),
        _ => None,
    }
}
