//! theme schema：磁盘形态（`StyleSheet`、`ContextPaletteDef` 等）+ compile 产物（`CompiledTheme`、`CompiledRenderContext`）+ 运行时上下文类型。
//!
//! 严格遵循 `docs/specs/style-system/style-sheet-spec.md` §3 / §6 / §11 定义。
//! 本模块仅包含纯数据结构，不依赖 `crate::ast`。

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::types::GraphicStyleId;
use crate::icons::ResolveOptions;

// ─── StyleValue ────────────────────────────────────────────────────

/// 样式属性值。支持字符串、数值、布尔和数值数组（用于虚线 pattern）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StyleValue {
    String(String),
    Number(f64),
    Boolean(bool),
    Array(Vec<f64>),
}

impl StyleValue {
    /// 尝试获取字符串值。
    pub fn as_str(&self) -> Option<&str> {
        match self {
            StyleValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// 尝试获取数值。
    pub fn as_number(&self) -> Option<f64> {
        match self {
            StyleValue::Number(n) => Some(*n),
            StyleValue::String(s) => s.parse::<f64>().ok(),
            _ => None,
        }
    }

    /// 尝试获取布尔值。
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            StyleValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    /// 将值转为字符串表示（用于 token 引用解析后的回退）。
    pub fn to_resolved_string(&self) -> Option<String> {
        match self {
            StyleValue::String(s) => Some(s.clone()),
            StyleValue::Number(n) => Some(if *n == (*n as i64) as f64 {
                (*n as i64).to_string()
            } else {
                format!("{n:.2}")
            }),
            StyleValue::Boolean(b) => Some(b.to_string()),
            StyleValue::Array(arr) => Some(
                arr.iter()
                    .map(|n| {
                        if *n == (*n as i64) as f64 {
                            (*n as i64).to_string()
                        } else {
                            format!("{n:.2}")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(","),
            ),
        }
    }
}

// ─── StyleMeta ─────────────────────────────────────────────────────

/// 样式表元信息。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyleMeta {
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

// ─── StyleTokens ───────────────────────────────────────────────────

/// 设计 token 集合（spec §5）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StyleTokens {
    #[serde(default)]
    pub colors: BTreeMap<String, String>,
    #[serde(default)]
    pub typography: BTreeMap<String, StyleValue>,
    #[serde(default)]
    pub strokes: BTreeMap<String, StyleValue>,
    #[serde(default)]
    pub radius: BTreeMap<String, StyleValue>,
    #[serde(default)]
    pub spacing: BTreeMap<String, StyleValue>,
    #[serde(default)]
    pub effects: BTreeMap<String, StyleValue>,
    /// 按 entity type 的语义配色（`{role.*}` 引用源）。
    #[serde(default)]
    pub palette: BTreeMap<String, PaletteRole>,
}

/// `tokens.palette.<role>` 的结构：fill / stroke / text_fill / edge_stroke。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PaletteRole {
    #[serde(default)]
    pub fill: Option<String>,
    #[serde(default)]
    pub stroke: Option<String>,
    #[serde(default)]
    pub text_fill: Option<String>,
    #[serde(default)]
    pub edge_stroke: Option<String>,
}

impl StyleTokens {
    /// 按 `{category.key}` 语法查找 token 值。
    ///
    /// 返回 `Some(resolved_string)` 或 `None`（引用无效时）。
    pub fn resolve_ref(&self, reference: &str) -> Option<String> {
        // 期望格式: {category.key}
        let trimmed = reference.trim();
        if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
            return None;
        }
        let inner = &trimmed[1..trimmed.len() - 1];
        let (category, key) = inner.split_once('.')?;
        match category {
            "colors" => self.colors.get(key).cloned(),
            "typography" => self.typography.get(key).and_then(|v| v.to_resolved_string()),
            "strokes" => self.strokes.get(key).and_then(|v| v.to_resolved_string()),
            "radius" => self.radius.get(key).and_then(|v| v.to_resolved_string()),
            "spacing" => self.spacing.get(key).and_then(|v| v.to_resolved_string()),
            "effects" => self.effects.get(key).and_then(|v| v.to_resolved_string()),
            _ => None,
        }
    }

    /// 判断字符串是否为 token 引用（`{category.key}` 格式）。
    pub fn is_token_ref(value: &str) -> bool {
        let trimmed = value.trim();
        trimmed.starts_with('{') && trimmed.ends_with('}') && trimmed.contains('.')
    }
}

// ─── StyleBlock ────────────────────────────────────────────────────

/// 一组视觉属性键值对（spec §6）。
///
/// 用于 `defaults.node`、`diagrams.flowchart.entity_types.service` 等。
pub type StyleBlock = BTreeMap<String, StyleValue>;

// ─── ElementDefaults ───────────────────────────────────────────────

/// 第一层：全局兜底视觉默认值（spec §4.2）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ElementDefaults {
    #[serde(default)]
    pub canvas: StyleBlock,
    #[serde(default)]
    pub title: StyleBlock,
    #[serde(default)]
    pub node: StyleBlock,
    #[serde(default)]
    pub edge: StyleBlock,
    #[serde(default)]
    pub group: StyleBlock,
}

// ─── DiagramStyles ─────────────────────────────────────────────────

/// 第二层 + 第三层：图表命名空间样式（spec §4.2）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DiagramStyles {
    #[serde(default)]
    pub node: Option<StyleBlock>,
    #[serde(default)]
    pub edge: Option<StyleBlock>,
    #[serde(default)]
    pub group: Option<StyleBlock>,
    #[serde(default)]
    pub title: Option<StyleBlock>,
    /// 第三层：按 `entity.type` 覆盖（spec §4.2）。
    #[serde(default)]
    pub entity_types: BTreeMap<String, StyleBlock>,
    /// 边类型覆盖（spec §4.3）。
    #[serde(default)]
    pub edge_kinds: BTreeMap<String, StyleBlock>,
    /// Mindmap（及未来类似图）：按 branch_slot 索引的调色板条目。
    #[serde(default)]
    pub branch_palettes: Vec<BranchPaletteEntry>,
    /// 按 tree_depth 索引的边线宽（depth 0/1/2/…）；缺省则不分 depth。
    #[serde(default)]
    pub edge_depth_stroke_width: Vec<f64>,
    /// 实例级 context palettes（spec §4.5）。
    #[serde(default)]
    pub context_palettes: BTreeMap<String, ContextPaletteDef>,
}

/// `context_palettes.<id>` 的磁盘 JSON 形态（spec §4.5）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ContextPaletteDef {
    #[serde(default)]
    pub entries: Vec<StyleBlock>,
    #[serde(default)]
    pub index: IndexRuleDef,
    #[serde(default)]
    pub bindings: Vec<ContextBindingDef>,
}

/// `index` 字段的 JSON 定义。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct IndexRuleDef {
    pub from: String,
    #[serde(default)]
    pub wrap: bool,
    #[serde(default)]
    pub cap: Option<usize>,
}

/// `bindings[]` 的单条绑定规则。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ContextBindingDef {
    pub target: String, // "entity" | "edge" | "group"
    #[serde(default)]
    pub types: Vec<String>,
    pub fields: BTreeMap<String, String>, // style_key → entry_key
}

/// 分支配色板条目（spec §mindmap / contextual token）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BranchPaletteEntry {
    #[serde(default)]
    pub fill: Option<StyleValue>,
    #[serde(default)]
    pub stroke: Option<StyleValue>,
    #[serde(default)]
    pub edge_stroke: Option<StyleValue>,
}

// ─── StyleSheet ────────────────────────────────────────────────────

/// 完整的视觉主题 JSON（spec §3）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StyleSheet {
    pub version: String,
    pub id: String,
    pub name: String,
    /// 单层继承：指向基座 theme_id（基座不得有 `extends`）。
    #[serde(default)]
    pub extends: Option<String>,
    #[serde(default)]
    pub meta: StyleMeta,
    #[serde(default)]
    pub tokens: StyleTokens,
    #[serde(default)]
    pub defaults: ElementDefaults,
    /// 键为 DiagramType 小写名（如 `"flowchart"`），非 DiagramType 枚举。
    #[serde(default)]
    pub diagrams: BTreeMap<String, DiagramStyles>,
}

// ─── NodeShape ─────────────────────────────────────────────────────

/// 节点几何形态枚举（spec §6.3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeShape {
    #[serde(rename = "rect")]
    Rect,
    #[serde(rename = "rounded_rect")]
    RoundedRect,
    #[serde(rename = "circle")]
    Circle,
    #[serde(rename = "diamond")]
    Diamond,
    #[serde(rename = "cylinder")]
    Cylinder,
    #[serde(rename = "hexagon")]
    Hexagon,
    #[serde(rename = "person")]
    Person,
    #[serde(rename = "stadium")]
    Stadium,
    #[serde(rename = "parallelogram")]
    Parallelogram,
    #[serde(rename = "document")]
    Document,
    #[serde(rename = "cloud")]
    Cloud,
    #[serde(rename = "subprocess")]
    Subprocess,
}

impl NodeShape {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rect => "rect",
            Self::RoundedRect => "rounded_rect",
            Self::Circle => "circle",
            Self::Diamond => "diamond",
            Self::Cylinder => "cylinder",
            Self::Hexagon => "hexagon",
            Self::Person => "person",
            Self::Stadium => "stadium",
            Self::Parallelogram => "parallelogram",
            Self::Document => "document",
            Self::Cloud => "cloud",
            Self::Subprocess => "subprocess",
        }
    }

    /// 默认节点形状。
    pub fn default_shape() -> Self {
        Self::RoundedRect
    }
}

impl FromStr for NodeShape {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "rect" => Ok(Self::Rect),
            "rounded_rect" => Ok(Self::RoundedRect),
            "circle" => Ok(Self::Circle),
            "diamond" => Ok(Self::Diamond),
            "cylinder" => Ok(Self::Cylinder),
            "hexagon" => Ok(Self::Hexagon),
            "person" => Ok(Self::Person),
            "stadium" => Ok(Self::Stadium),
            "parallelogram" => Ok(Self::Parallelogram),
            "document" => Ok(Self::Document),
            "cloud" => Ok(Self::Cloud),
            "subprocess" => Ok(Self::Subprocess),
            other => Err(format!("unknown node shape: '{other}'")),
        }
    }
}

impl fmt::Display for NodeShape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ─── ResolvedStyleSheet ────────────────────────────────────────────

/// 解析完成后的 StyleSheet（token 引用已展开）。
///
/// 这是旧 `resolve` 模块的产出，保留供历史代码参考；新路径直接使用 `CompiledTheme`。
#[derive(Debug, Clone)]
pub struct ResolvedStyleSheet {
    pub id: String,
    pub name: String,
    /// 已展开的 token 集合（供 canvas/title 等渲染查询）。
    pub tokens: StyleTokens,
    /// 第一层全局兜底（token 引用已解析）。
    pub defaults: ElementDefaults,
    /// 第二层 + 第三层（token 引用已解析）。
    pub diagrams: BTreeMap<String, ResolvedDiagramStyles>,
}

/// 解析后的图表样式。
#[derive(Debug, Clone)]
pub struct ResolvedDiagramStyles {
    pub node: StyleBlock,
    pub edge: StyleBlock,
    pub group: StyleBlock,
    pub title: StyleBlock,
    /// 第三层：entity type → 已解析样式块。
    pub entity_types: BTreeMap<String, StyleBlock>,
    /// 边类型 → 已解析样式块。
    pub edge_kinds: BTreeMap<String, StyleBlock>,
    /// 已展开 sheet 级 token 的分支调色板。
    pub branch_palettes: Vec<ResolvedBranchPaletteEntry>,
    /// 边 depth 线宽（数值，无 token）。
    pub edge_depth_stroke_width: Vec<f64>,
}

/// 解析后的分支调色板条目（palette 内的 `{colors.*}` 等 sheet 级 token 已展开）。
#[derive(Debug, Clone, Default)]
pub struct ResolvedBranchPaletteEntry {
    pub fill: Option<StyleValue>,
    pub stroke: Option<StyleValue>,
    pub edge_stroke: Option<StyleValue>,
}

// ─── 已知图表类型键名 ──────────────────────────────────────────────

/// StyleSheet `diagrams` 中合法的键名集合。
pub const KNOWN_DIAGRAM_TYPES: &[&str] = &[
    "flowchart",
    "sequence",
    "architecture",
    "state",
    "er",
    "mindmap",
];

/// 支持的规范版本。
pub const SUPPORTED_VERSIONS: &[&str] = &["0.2"];

// ─── CompiledTheme（compile 产物） ────────────────────────────────

/// compile 后的主题：L1 已展开，无魔法字符串；L2 palette entries 已展开为字面量。
#[derive(Debug, Clone)]
pub struct CompiledTheme {
    pub id: String,
    pub name: String,
    pub canvas: StyleBlock,
    /// 全局 node 默认（来自 `defaults.node`，已展开 token）。
    /// 当 diagram 不存在时作为 fallback。
    pub node_default: StyleBlock,
    /// 全局 edge 默认（来自 `defaults.edge`，已展开 token）。
    pub edge_default: StyleBlock,
    /// 全局 group 默认（来自 `defaults.group`，已展开 token）。
    pub group_default: StyleBlock,
    /// 全局 title 默认（来自 `defaults.title`，已展开 token）。
    pub title: StyleBlock,
    /// 全局 `group_nest` palette（从 `group_default` 合成）。
    ///
    /// 当 diagram 不存在或 diagram 未定义 `group_nest` 时作为 fallback，
    /// 保证旧路径 `group_style_by_depth` 的 depth 递进行为在所有 diagram type 上等价。
    pub group_nest: CompiledContextPalette,
    pub diagrams: BTreeMap<String, CompiledDiagram>,
}

/// compile 后的图表样式。
#[derive(Debug, Clone)]
pub struct CompiledDiagram {
    // ── L1 类型级 ──
    pub node_default: StyleBlock,
    pub nodes: BTreeMap<String, StyleBlock>,
    pub edge_default: StyleBlock,
    pub edges: BTreeMap<String, StyleBlock>,
    pub group_default: StyleBlock,
    pub title: StyleBlock,

    // ── L2 实例级 ──
    pub context_palettes: BTreeMap<String, CompiledContextPalette>,
}

/// compile 后的 context palette：entries 已展开为字面量。
#[derive(Debug, Clone)]
pub struct CompiledContextPalette {
    pub entries: Vec<StyleBlock>,
    pub index: IndexRule,
    pub bindings: Vec<ContextBindingDef>,
}

/// compile 后的下标规则。
#[derive(Debug, Clone)]
pub enum IndexRule {
    BranchSlot { wrap: bool },
    TreeDepth { cap: usize },
    GroupDepth { cap: usize },
}

impl CompiledTheme {
    /// L1 查询：节点类型级块（已含 defaults + diagrams.node + entity_types 三层预合并）。
    pub fn node_block(&self, diagram: &str, entity_type: Option<&str>) -> &StyleBlock {
        let diag = self.diagrams.get(diagram);
        match (diag, entity_type) {
            (Some(d), Some(t)) => d.nodes.get(t).unwrap_or(&d.node_default),
            (Some(d), None) => &d.node_default,
            (None, _) => &self.node_default,
        }
    }

    /// L1 查询：边类型级块。
    pub fn edge_block(&self, diagram: &str, edge_kind: Option<&str>) -> &StyleBlock {
        let diag = self.diagrams.get(diagram);
        match (diag, edge_kind) {
            (Some(d), Some(k)) => d.edges.get(k).unwrap_or(&d.edge_default),
            (Some(d), None) => &d.edge_default,
            (None, _) => &self.edge_default,
        }
    }

    /// L1 查询：group 默认块。
    pub fn group_block(&self, diagram: &str) -> &StyleBlock {
        self.diagrams
            .get(diagram)
            .map(|d| &d.group_default)
            .unwrap_or(&self.group_default)
    }

    /// L1 查询：画布块。
    pub fn canvas_block(&self) -> &StyleBlock {
        &self.canvas
    }

    /// L1 查询：标题块。
    pub fn title_block(&self, diagram: &str) -> &StyleBlock {
        self.diagrams
            .get(diagram)
            .map(|d| &d.title)
            .unwrap_or(&self.title)
    }
}

// ─── InstanceContext（prepare 期派生） ────────────────────────────

/// per-图元的实例上下文，用于 L2 context_palette 下标查表。
#[derive(Debug, Clone, Default)]
pub struct InstanceContext {
    pub branch_slot: Option<usize>,
    pub tree_depth: Option<usize>,
    pub group_depth: Option<usize>,
}

// ─── ThemeContext（prepare 侧） ───────────────────────────────────

/// prepare 的 `materialize_diagram_styles` 通过此结构获取 `CompiledTheme`、当前 diagram type、
/// 以及 `root_id`（用于边的 `InstanceContext` 派生）。
pub struct ThemeContext<'a> {
    pub compiled: &'a CompiledTheme,
    pub diagram_type: &'a str,
    pub root_id: Option<&'a crate::ast::Identifier>,
}

// ─── CompiledRenderContext（render 侧） ───────────────────────────

/// render 的 `RenderRequest::resolve_context()` 迁移后构建此结构。
pub struct CompiledRenderContext {
    pub compiled: CompiledTheme,
    pub graphic_style: GraphicStyleId,
    pub icon_resolve: ResolveOptions,
}

impl CompiledRenderContext {
    pub fn graphic_painter(&self) -> &'static dyn crate::graphic_style::GraphicStylePainter {
        crate::graphic_style::painter_for(self.graphic_style)
    }
}
