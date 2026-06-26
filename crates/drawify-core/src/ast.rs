//! Drawify 抽象语法树 (AST) 定义
//!
//! AST 是 Drawify 的核心数据模型。文本只是 AST 的一种序列化形式。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::types::standard_attr_keys::diagram;

// ─── Position & Span ───────────────────────────────────────────────

/// 源文本中的位置（行号、列号均从 1 开始）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

impl Position {
    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

/// 源文本中的范围
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start: Position,
    pub end: Position,
}

impl Span {
    pub fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    pub fn dummy() -> Self {
        Self {
            start: Position::new(0, 0),
            end: Position::new(0, 0),
        }
    }
}

/// 源文件元信息
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourceInfo {
    pub file: Option<String>,
    pub line_count: usize,
}

// ─── Identifier ────────────────────────────────────────────────────

/// 标识符，保证符合 [a-z][a-z0-9_]* 规则
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Identifier(String);

impl Identifier {
    pub fn new(s: &str) -> Result<Self, String> {
        if s.is_empty() {
            return Err("identifier cannot be empty".into());
        }
        if s.len() > 64 {
            return Err(format!("identifier '{}' exceeds 64 characters", s));
        }
        let first = s.chars().next().unwrap();
        if !first.is_ascii_lowercase() {
            return Err(format!(
                "identifier '{}' must start with a lowercase letter",
                s
            ));
        }
        if !s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
            return Err(format!(
                "identifier '{}' can only contain lowercase letters, digits, and underscores",
                s
            ));
        }
        if RESERVED_WORDS.contains(&s) {
            return Err(format!("'{}' is a reserved word", s));
        }
        Ok(Self(s.to_string()))
    }

    pub fn new_unchecked(s: &str) -> Self {
        Self(s.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub const RESERVED_WORDS: &[&str] = &[
    "diagram", "entity", "group", "relation", "flowchart", "sequence", "architecture", "state",
    "er", "mindmap", "true", "false", "meta",
];

use crate::types::DiagramType;

// ─── AttributeValue ────────────────────────────────────────────────

/// 文本值（原 `String` + `Atom` + `Enum` 合并）。
/// `quoted` 标记原始写法：`true` = 引号字符串，`false` = 无引号 atom。
/// JSON 序列化/反序列化只处理 `value`，`quoted` 不参与。
/// `PartialEq` 忽略 `quoted`——`service` 和 `"service"` 语义相等。
#[derive(Debug, Clone)]
pub struct TextValue {
    pub value: String,
    pub quoted: bool,
}

impl TextValue {
    pub fn quoted(value: impl Into<String>) -> Self {
        Self { value: value.into(), quoted: true }
    }

    pub fn unquoted(value: impl Into<String>) -> Self {
        Self { value: value.into(), quoted: false }
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }
}

impl std::ops::Deref for TextValue {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl std::fmt::Display for TextValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.value)
    }
}

impl PartialEq for TextValue {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl PartialEq<&str> for TextValue {
    fn eq(&self, other: &&str) -> bool {
        &self.value == other
    }
}

impl PartialEq<str> for TextValue {
    fn eq(&self, other: &str) -> bool {
        self.value == other
    }
}

impl Serialize for TextValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.value)
    }
}

impl<'de> Deserialize<'de> for TextValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(Self { value, quoted: false })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AttributeValue {
    String(TextValue),
    Number(f64),
    Boolean(bool),
    /// 算法名 + 可选配置块，如 `bezier { tension: 0.55 }`。
    #[serde(serialize_with = "serialize_config")]
    Config {
        algo: String,
        options: HashMap<String, AttributeValue>,
    },
}

/// Atom 字面量词法约束：`[a-z][a-z0-9_.-]*`（不允许首尾或连续 `.`）
pub fn is_valid_atom(s: &str) -> bool {
    if s.is_empty() || s.len() > 64 {
        return false;
    }
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    if !chars.all(|c| {
        c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-' || c == '.'
    }) {
        return false;
    }
    !s.starts_with('.') && !s.ends_with('.') && !s.contains("..")
}

impl AttributeValue {
    /// 读取文本值（原 `as_atom` + `as_text` 合并）。
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(tv) => Some(tv.value.as_str()),
            _ => None,
        }
    }

    /// 读取算法配置中的算法名（简写文本或 config 块均可）。
    pub fn algorithm_name(&self) -> Option<&str> {
        match self {
            Self::String(tv) => Some(tv.value.as_str()),
            Self::Config { algo, .. } => Some(algo.as_str()),
            _ => None,
        }
    }

    /// 读取算法配置块内的 options；简写文本形式返回 `None`。
    pub fn algorithm_options(&self) -> Option<&HashMap<String, AttributeValue>> {
        match self {
            Self::Config { options, .. } => Some(options),
            _ => None,
        }
    }

    /// 是否为引号字符串。
    pub fn is_quoted(&self) -> bool {
        match self {
            Self::String(tv) => tv.quoted,
            _ => false,
        }
    }
}

#[derive(Serialize)]
struct ConfigPayload<'a> {
    algo: &'a str,
    options: &'a HashMap<String, AttributeValue>,
}

fn serialize_config<S>(
    algo: &str,
    options: &HashMap<String, AttributeValue>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeMap;
    let payload = ConfigPayload { algo, options };
    let mut map = serializer.serialize_map(Some(1))?;
    map.serialize_entry("$config", &payload)?;
    map.end()
}

// ─── StyleSource / StyleAttribute ──────────────────────────────────

/// 物化后 `style.*` 属性的来源（便于诊断与 Diff）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StyleSource {
    #[default]
    Unknown,
    Inline,
    Expanded { decl_target: String },
    Palette {
        style_sheet_id: String,
        entity_type: String,
        branch_slot: Option<usize>,
    },
    Token { key: String },
}

/// 带溯源信息的样式属性。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StyleAttribute {
    pub value: AttributeValue,
    #[serde(default)]
    pub source: StyleSource,
}

/// `attributes.style` 容器；序列化时仅输出 value（兼容旧 JSON）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StyleMap(HashMap<String, StyleAttribute>);

impl StyleMap {
    pub fn get(&self, key: &str) -> Option<&AttributeValue> {
        self.0.get(key).map(|a| &a.value)
    }

    pub fn get_attr(&self, key: &str) -> Option<&StyleAttribute> {
        self.0.get(key)
    }

    pub fn insert(&mut self, key: String, value: AttributeValue) {
        self.insert_with_source(key, value, StyleSource::Unknown);
    }

    pub fn insert_with_source(
        &mut self,
        key: String,
        value: AttributeValue,
        source: StyleSource,
    ) {
        self.0.insert(key, StyleAttribute { value, source });
    }

    /// `or_insert` 语义：仅当 key 不存在时写入。
    pub fn or_insert_with_source(
        &mut self,
        key: String,
        value: AttributeValue,
        source: StyleSource,
    ) {
        self.0
            .entry(key)
            .or_insert(StyleAttribute { value, source });
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.0.keys()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &StyleAttribute)> {
        self.0.iter()
    }

    pub fn iter_values(&self) -> impl Iterator<Item = (&String, &AttributeValue)> {
        self.0.iter().map(|(k, v)| (k, &v.value))
    }

    pub fn remove(&mut self, key: &str) -> Option<StyleAttribute> {
        self.0.remove(key)
    }
}

impl Serialize for StyleMap {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let flat: HashMap<&str, &AttributeValue> =
            self.0.iter().map(|(k, v)| (k.as_str(), &v.value)).collect();
        flat.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for StyleMap {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let flat = HashMap::<String, AttributeValue>::deserialize(deserializer)?;
        Ok(Self(
            flat.into_iter()
                .map(|(k, v)| {
                    (
                        k,
                        StyleAttribute {
                            value: v,
                            source: StyleSource::Unknown,
                        },
                    )
                })
                .collect(),
        ))
    }
}

// ─── AttributeMap ──────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AttributeMap {
    #[serde(default)]
    pub standard: HashMap<String, AttributeValue>,
    #[serde(default)]
    pub meta: HashMap<String, AttributeValue>,
    /// 内联样式属性（`style.fill`, `style.stroke` 等）。
    ///
    /// DSL 写 `style.fill: "#xxx"`，Parser 将 key 存为 `"fill"`（去掉 `style.` 前缀）。
    /// prepare 将 StyleSheet palette、声明式规则一并物化到此 map。
    /// 渲染器通过 `node_style_from_attributes` / `edge_style_from_attributes` 消费。
    #[serde(default)]
    pub style: StyleMap,
}

// ─── DiagramAttribute ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagramAttribute {
    pub key: String,
    pub value: AttributeValue,
    pub span: Span,
}

// ─── Entity ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: Identifier,
    pub label: String,
    pub attributes: AttributeMap,
    pub group_id: Option<Identifier>,
    pub span: Span,
}

// ─── ArrowType ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArrowType {
    /// -> 主动流向
    Active,
    /// --> 被动/响应流向
    Passive,
    /// <-> 双向关系
    Bidirectional,
}

// ─── Relation ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub from: Identifier,
    pub to: Identifier,
    pub arrow: ArrowType,
    /// 边中段标签（默认标签，位于路径中点附近）
    pub label: Option<String>,
    /// 箭头头部标签（靠近 `to` 端）
    #[serde(default)]
    pub head_label: Option<String>,
    /// 箭头尾部标签（靠近 `from` 端）
    #[serde(default)]
    pub tail_label: Option<String>,
    pub attributes: AttributeMap,
    pub span: Span,
}

// ─── Group ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: Identifier,
    pub label: String,
    pub attributes: AttributeMap,
    pub parent_id: Option<Identifier>,
    pub depth: u8,
    pub entity_ids: Vec<Identifier>,
    pub child_group_ids: Vec<Identifier>,
    pub span: Span,
}

// ─── Pipeline Stage Newtypes ───────────────────────────────────────

/// Parser 产出。允许：缺省 `entity.type`、仅部分 `attributes.style`。
///
/// 必须经 `prepare()` 转换为 `PreparedDiagram` 后才能传给下游。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RawDiagram(pub Diagram);

impl RawDiagram {
    pub fn inner(&self) -> &Diagram {
        &self.0
    }

    pub fn into_inner(self) -> Diagram {
        self.0
    }
}

/// `prepare()` 产出。对外稳定契约。
///
/// 不变量（由 `prepare` 保证）：
/// - 凡 `profile.default_entity_type` 非空的图表，每个 entity 均有 `standard["type"]`
/// - 每个 entity/relation 的 `attributes.style` 已物化（Step 3+ 后）
/// - `layout_plan` 在构造时解析，供 layout / validate 复用
///
/// ## `layout_plan` 生命周期
///
/// 1. **创建** — `new(diagram)` / `prepare()` 末尾调用 `LayoutPlan::resolve(diagram, profile)`
/// 2. **读取** — `layout_plan()` 供 `validate_layout_plan_warnings` 与 `compute_layout_with_plan` 使用
/// 3. **失效** — 修改 diagram 的 `layout_algo`、`edge_routing` 或其配置块后 plan 过时
/// 4. **刷新** — `refresh_layout_plan()`；patch 路径使用 `PreparedDiagram::new(diagram)` 重建
/// 5. **序列化** — `Serialize` 仅输出 diagram；`Deserialize` 后自动重新 resolve
///
/// 下游 validate / layout / render 的公开 API 签名使用 `&PreparedDiagram`。
#[derive(Debug, Clone)]
pub struct PreparedDiagram {
    diagram: Diagram,
    layout_plan: crate::layout::LayoutPlan,
}

impl PreparedDiagram {
    pub fn new(diagram: Diagram) -> Self {
        let profile = crate::profile::profile_for(&diagram.diagram_type);
        let layout_plan = crate::layout::LayoutPlan::resolve(&diagram, profile);
        Self {
            diagram,
            layout_plan,
        }
    }

    pub fn with_layout_plan(diagram: Diagram, layout_plan: crate::layout::LayoutPlan) -> Self {
        Self {
            diagram,
            layout_plan,
        }
    }

    pub fn inner(&self) -> &Diagram {
        &self.diagram
    }

    pub fn layout_plan(&self) -> &crate::layout::LayoutPlan {
        &self.layout_plan
    }

    pub fn into_inner(self) -> Diagram {
        self.diagram
    }

    pub fn into_parts(self) -> (Diagram, crate::layout::LayoutPlan) {
        (self.diagram, self.layout_plan)
    }

    /// diagram 属性变更后重新解析 layout plan。
    pub fn refresh_layout_plan(&mut self) {
        let profile = crate::profile::profile_for(&self.diagram.diagram_type);
        self.layout_plan = crate::layout::LayoutPlan::resolve(&self.diagram, profile);
    }
}

impl serde::Serialize for PreparedDiagram {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.diagram.serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for PreparedDiagram {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let diagram = Diagram::deserialize(deserializer)?;
        Ok(PreparedDiagram::new(diagram))
    }
}

impl std::ops::Deref for PreparedDiagram {
    type Target = Diagram;
    fn deref(&self) -> &Self::Target {
        &self.diagram
    }
}

impl AsRef<Diagram> for PreparedDiagram {
    fn as_ref(&self) -> &Diagram {
        &self.diagram
    }
}

// ─── StyleDecl ─────────────────────────────────────────────────────

/// DSL 声明式样式：`node_style service { fill: "#xxx" }` 或 `edge_style error { stroke: "#xxx" }`。
///
/// 这是 Cascade 第 4 层（DSL 声明覆盖 palette），优先级高于 theme 的 entity_types / edge_kinds，
/// 但低于内联 `style.*`（第 5 层）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleDecl {
    /// `node_style` 或 `edge_style`。
    pub kind: StyleDeclKind,
    /// 目标名称：entity_type（如 `"service"`）或 edge_kind（如 `"error"`）。
    pub target: String,
    /// 声明的样式属性。
    pub style: HashMap<String, AttributeValue>,
    /// 源码位置。
    #[serde(skip, default = "Span::dummy")]
    pub span: Span,
}

/// 声明式样式的种类。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StyleDeclKind {
    Node,
    Edge,
}

// ─── Diagram ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagram {
    pub diagram_type: DiagramType,
    pub attributes: Vec<DiagramAttribute>,
    pub entities: Vec<Entity>,
    pub relations: Vec<Relation>,
    pub groups: Vec<Group>,
    /// DSL 声明式样式列表：`node_style service { ... }` / `edge_style error { ... }`。
    #[serde(default)]
    pub style_decls: Vec<StyleDecl>,
    /// 文件开头连续 `//` 行组成的文档注释。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc_comment: Option<String>,
    pub source_info: SourceInfo,
}

impl Default for Diagram {
    fn default() -> Self {
        Self {
            diagram_type: DiagramType::Flowchart,
            attributes: Vec::new(),
            entities: Vec::new(),
            relations: Vec::new(),
            groups: Vec::new(),
            style_decls: Vec::new(),
            doc_comment: None,
            source_info: SourceInfo::default(),
        }
    }
}

impl Diagram {
    pub fn new(diagram_type: DiagramType, source_info: SourceInfo) -> Self {
        Self {
            diagram_type,
            attributes: Vec::new(),
            entities: Vec::new(),
            relations: Vec::new(),
            groups: Vec::new(),
            style_decls: Vec::new(),
            doc_comment: None,
            source_info,
        }
    }

    /// 查找实体 by ID
    pub fn find_entity(&self, id: &str) -> Option<&Entity> {
        self.entities.iter().find(|e| e.id.as_str() == id)
    }

    /// 查找分组 by ID
    pub fn find_group(&self, id: &str) -> Option<&Group> {
        self.groups.iter().find(|g| g.id.as_str() == id)
    }

    /// 获取图表标题（从 diagram 级 `title` 属性读取）。
    pub fn title(&self) -> Option<&str> {
        for attr in &self.attributes {
            if attr.key == diagram::TITLE {
                if let Some(v) = attr.value.as_str() {
                    return Some(v);
                }
            }
        }
        None
    }

    /// 获取 effective direction（委托 `resolve_effective_direction`）。
    ///
    /// 返回值优先级：AST 显式声明 → profile 默认 → 硬编码 `top-to-bottom` 兜底。
    /// 推荐布局代码直接调用 `resolve_effective_direction()` 获取 `Option<&str>`，
    /// 此方法仅为向后兼容保留。
    pub fn direction(&self) -> &str {
        crate::layout::resolve_effective_direction(self)
            .unwrap_or(crate::types::attr_constants::direction::TOP_TO_BOTTOM)
    }

    /// 仅返回 AST 中显式声明的 `direction` 值，不包含任何 fallback。
    ///
    /// 用于校验层判断用户是否显式写了 `direction`，
    /// 以及 `resolve_effective_direction()` 的输入。
    pub fn direction_attr(&self) -> Option<&str> {
        for attr in &self.attributes {
            if attr.key == diagram::DIRECTION {
                if let Some(v) = attr.value.as_str() {
                    return Some(v);
                }
            }
        }
        None
    }

    /// 读取数值型图级属性（支持 Number；也容许 String 形式的数字）
    pub fn number_attr(&self, key: &str) -> Option<f64> {
        for attr in &self.attributes {
            if attr.key == key {
                return match &attr.value {
                    AttributeValue::Number(n) => Some(*n),
                    AttributeValue::String(tv) => tv.value.trim().parse().ok(),
                    _ => None,
                };
            }
        }
        None
    }
}
