# Drawify AST 数据结构定义

> 版本：0.1.0-draft | 状态：设计中

本文档定义 Drawify 的抽象语法树（AST）数据结构。AST 是 Drawify 的核心数据模型——解析器输出 AST，验证器消费 AST，渲染器消费 AST，Diff/Patch 操作 AST。

**AST 是 Drawify 的一等公民。** 文本只是 AST 的一种序列化形式。

### 存储层 vs 管线层

本文档定义 **`Diagram` 存储 schema**（serde、JSON、Patch 的数据形状）。管线阶段（Parser 产出 `RawDiagram` → `prepare()` → `PreparedDiagram`）由 [pipeline-spec.md](pipeline-spec.md) 定义：二者包装同一 `Diagram`，不重复字段。对外稳定契约是 **`PreparedDiagram`**；validate / render 等下游 API 应消费该类型。

---

## 1. 设计原则

| 原则 | 说明 |
|------|------|
| 可序列化 | AST 必须能无损地序列化为 JSON，也能从 JSON 反序列化 |
| 可寻址 | 每个节点都有唯一路径（JSON Pointer），支持 Diff/Patch 定位 |
| 不可变 | AST 在解析后不可变，修改通过 Patch 操作产生新 AST |
| 位置溯源 | 每个 AST 节点保留源文本中的位置信息（行号、列号） |
| 类型安全 | 所有字段都有明确的类型约束，不允许 any/dynamic 类型 |

---

## 2. 顶层结构

### 2.1 Diagram（图表根节点）

```rust
pub struct Diagram {
    /// 图表类型
    pub diagram_type: DiagramType,

    /// 图表级属性（layout, theme, title 等）
    pub attributes: Vec<DiagramAttribute>,

    /// 所有实体（扁平化存储，包括 group 内的）
    pub entities: Vec<Entity>,

    /// 所有关系
    pub relations: Vec<Relation>,

    /// 所有分组
    pub groups: Vec<Group>,

    /// 声明式样式规则（node_style / edge_style）
    pub style_decls: Vec<StyleDecl>,

    /// 源文本元信息
    pub source_info: SourceInfo,
}
```

**JSON 序列化示例：**

```json
{
    "diagram_type": "flowchart",
    "attributes": [
        { "key": "layout", "value": "top-to-bottom" },
        { "key": "title", "value": "用户认证流程" }
    ],
    "entities": [
        { "id": "api", "label": "API 服务", "attributes": { "standard": { "type": { "$enum": "service" } }, "style": {}, "meta": {} }, "group_id": null, "span": { "start": { "line": 5, "column": 5 }, "end": { "line": 5, "column": 30 } } }
    ],
    "relations": [
        { "from": "api", "to": "db", "arrow": "active", "label": "查询", "attributes": { "standard": {}, "style": {}, "meta": {} }, "span": { "start": { "line": 10, "column": 5 }, "end": { "line": 10, "column": 25 } } }
    ],
    "groups": [],
    "style_decls": [],
    "source_info": { "file": "diagram.dfy", "line_count": 15 }
}
```

### 2.2 DiagramType（图表类型枚举）

```rust
pub enum DiagramType {
    Flowchart,
    Sequence,
    Architecture,
    State,
    Er,
    Mindmap,
}
```

**JSON 序列化：** 使用小写字符串 `"flowchart"`, `"sequence"` 等。

---

## 3. Entity（实体节点）

### 3.1 结构定义

```rust
pub struct Entity {
    /// 实体唯一标识符（即语法中的 identifier）
    pub id: Identifier,

    /// 显示标签（人类可读名称）
    pub label: String,

    /// 属性集合
    pub attributes: AttributeMap,

    /// 所属 group ID（None 表示在顶层）
    pub group_id: Option<Identifier>,

    /// 源文本位置信息
    pub span: Span,
}
```

### 3.2 Identifier（标识符）

```rust
/// 标识符，保证符合 [a-z][a-z0-9_]* 规则
/// 使用 newtype pattern 保证构造时校验
pub struct Identifier(String);

impl Identifier {
    /// 创建标识符，不符合规则时返回错误
    pub fn new(s: &str) -> Result<Self>;

    /// 获取标识符字符串
    pub fn as_str(&self) -> &str;
}
```

**约束：**
- 符合正则 `[a-z][a-z0-9_]{0,63}`
- 不能是保留字（见 language-spec.md §11）
- 在 Diagram 范围内全局唯一

### 3.3 AttributeMap（属性集合）

```rust
/// 有序属性映射（保持声明顺序）
pub struct AttributeMap {
    /// 预定义属性
    pub standard: HashMap<String, AttributeValue>,

    /// 视觉样式属性（由 Expand Pass 物化；DSL 写 style.fill，AST 存 style["fill"]）
    pub style: HashMap<String, AttributeValue>,

    /// meta 命名空间自定义属性
    pub meta: HashMap<String, AttributeValue>,
}
```

**设计理由：** 将 `standard`、`style` 和 `meta` 属性分开存储，原因如下：

- 验证器对 `standard` 做 Schema 校验，对 `style` 做不同的校验规则
- Expand Pass 向 `attributes.style` 写入全部默认值（含 StyleSheet palette）；渲染阶段只读、不合并
- Diff/Patch 可以独立追踪样式变更

### 3.4 AttributeValue（属性值）

```rust
pub enum AttributeValue {
    /// 字符串值："some text"
    String(String),

    /// 数值：42, 3.14
    Number(f64),

    /// 布尔值：true, false
    Boolean(bool),

    /// 枚举值（不带引号的标识符）：service, healthy, degraded
    Enum(String),
}
```

**JSON 序列化：**

```json
// String
"some text"

// Number
42

// Boolean
true

// Enum
{ "$enum": "service" }
```

> Enum 类型在 JSON 中用 `$enum` 标记，以区分普通字符串。这使得反序列化时能还原原始类型。

### 3.5 Entity 完整 JSON 示例

```json
{
    "id": "api_gateway",
    "label": "API 网关",
    "attributes": {
        "standard": {
            "type": { "$enum": "gateway" },
            "status": { "$enum": "healthy" },
            "owner": "平台团队"
        },
        "style": {
            "fill": "#E3F2FD",
            "stroke": "#1976D2"
        },
        "meta": {
            "version": "2.1.0",
            "port": 8080
        }
    },
    "group_id": "backend",
    "span": {
        "start": { "line": 8, "column": 5 },
        "end": { "line": 12, "column": 6 }
    }
}
```

---

## 4. Relation（关系节点）

### 4.1 结构定义

```rust
pub struct Relation {
    /// 源实体 ID
    pub from: Identifier,

    /// 目标实体 ID
    pub to: Identifier,

    /// 箭头类型
    pub arrow: ArrowType,

    /// 关系标签（可选）
    pub label: Option<String>,

    /// 属性集合
    pub attributes: AttributeMap,

    /// 源文本位置信息
    pub span: Span,
}
```

### 4.2 ArrowType（箭头类型枚举）

```rust
pub enum ArrowType {
    /// ->  主动流向（调用、发送、流转）
    Active,

    /// --> 被动/响应流向（返回、回调、异步响应）
    Passive,

    /// <-> 双向关系（双向通信、依赖）
    Bidirectional,
}
```

**JSON 序列化：**

```json
// Active ->
"active"

// Passive -->
"passive"

// Bidirectional <->
"bidirectional"
```

### 4.3 Relation 完整 JSON 示例

```json
{
    "from": "client",
    "to": "gateway",
    "arrow": "active",
    "label": "HTTPS 请求",
    "attributes": {
        "standard": {},
        "style": {},
        "meta": {
            "protocol": "HTTPS",
            "port": 443
        }
    },
    "span": {
        "start": { "line": 15, "column": 5 },
        "end": { "line": 15, "column": 35 }
    }
}
```

---

## 5. Group（分组节点）

### 5.1 结构定义

```rust
pub struct Group {
    /// 分组唯一标识符
    pub id: Identifier,

    /// 显示标签
    pub label: String,

    /// 属性集合
    pub attributes: AttributeMap,

    /// 父 group ID（None 表示顶层 group）
    pub parent_id: Option<Identifier>,

    /// 嵌套深度（1 = 顶层 group，2 = 嵌套 group）
    pub depth: u8,

    /// 直接包含的 entity ID 列表（保持声明顺序）
    pub entity_ids: Vec<Identifier>,

    /// 直接包含的子 group ID 列表
    pub child_group_ids: Vec<Identifier>,

    /// 源文本位置信息
    pub span: Span,
}
```

**注意：** `entity_ids` 和 `child_group_ids` 仅存储**直接子节点**，不包含递归子节点。要获取全部子节点需要递归遍历。

### 5.2 Group JSON 示例

```json
{
    "id": "backend",
    "label": "后端层",
    "attributes": {
        "standard": {
            "style": { "$enum": "dashed" }
        },
        "meta": {}
    },
    "parent_id": null,
    "depth": 1,
    "entity_ids": ["api", "worker"],
    "child_group_ids": ["internal"],
    "span": {
        "start": { "line": 6, "column": 1 },
        "end": { "line": 15, "column": 2 }
    }
}
```

---

## 6. StyleDecl（样式声明）

### 6.1 结构定义

```rust
/// 样式声明（node_style / edge_style）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleDecl {
    /// 声明类型：节点样式 or 边样式
    pub kind: StyleDeclKind,

    /// 选择器名称
    /// - node_style: 对应 entity type（如 "service", "database"）
    /// - edge_style: 自定义名称（如 "error", "highlight"）
    pub selector: String,

    /// 样式属性集合
    pub properties: HashMap<String, AttributeValue>,

    /// 源文本位置
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StyleDeclKind {
    /// node_style <type> { ... }
    Node,
    /// edge_style <name> { ... }
    Edge,
}
```

### 6.2 语义说明

- `node_style` 按 `entity.type` 匹配，在 Expand Pass 中展开到对应 entity 的 `attributes.style`
- `edge_style` 通过 relation 上的 `edge_style: <name>` 显式引用，展开到 relation 的 `attributes.style`
- `style_decls` 在 `prepare()` 后清空（不变量 I1），声明式规则已物化为 per-entity/relation 的 `style` 属性

---

## 7. 管线阶段类型

### 7.1 RawDiagram / PreparedDiagram

```rust
/// Parser 产出。允许：含 `style_decls`、缺省 `entity.type`、仅部分 `attributes.style`。
pub struct RawDiagram(pub Diagram);

/// `prepare()` 产出。**对外稳定契约**。
/// 仅 `prepare`（或 `pub(crate)` 测试构造器）可构造此类型。
pub struct PreparedDiagram(pub Diagram);

impl RawDiagram {
    pub fn inner(&self) -> &Diagram { &self.0 }
}

impl PreparedDiagram {
    pub fn inner(&self) -> &Diagram { &self.0 }
    pub fn into_inner(self) -> Diagram { self.0 }
}
```

### 7.2 PreparedDiagram 不变量

`prepare` 返回前须满足（`debug_assert!` 或单元测试保障）：

| # | 不变量 | 说明 |
|---|--------|------|
| I1 | `style_decls.is_empty()` | 声明式规则已展开或本无 |
| I2 | 凡 `profile.default_entity_type` 非空的图表，每个 entity 均有 `standard["type"]` | Profile 默认值已补全 |
| I3 | 每个 entity/relation 的 `attributes.style` 已物化 | 含 palette 默认与用户覆盖 |
| I4 | （远期）`layout_decls` 已展开 | 与样式展开同一管线 |

下游 **validate / layout / render / diff** 的公开 API 签名使用 `&PreparedDiagram`，从类型上不可能传入未 prepare 的图。

### 7.3 API 分层策略

```rust
// 须区分阶段（管线边界）
parse()   -> RawDiagram
prepare() -> PreparedDiagram
validate(&PreparedDiagram)
render(&PreparedDiagram, ...)

// 可继续用 &Diagram（由调用方保证已 prepare，或仅作内部工具）
layout::compute(diagram: &Diagram)
node_style_from_attributes(entity: &Entity)
```

JSON 反序列化得到 `Diagram`，包装为 `RawDiagram(diagram)` 后走同一 `prepare`（幂等，已物化的图重复 prepare 结果不变）。

---

## 8. 位置信息

### 8.1 Span（文本范围）

```rust
pub struct Span {
    pub start: Position,
    pub end: Position,
}
```

### 8.2 Position（文本位置）

```rust
pub struct Position {
    /// 行号（从 1 开始）
    pub line: usize,

    /// 列号（从 1 开始，按字符计数）
    pub column: usize,
}
```

**设计理由：** 行号和列号从 1 开始（而非 0），与人类阅读习惯一致，也与 LSP（Language Server Protocol）的 Diagnostic 规范兼容。

### 8.3 SourceInfo（源文件信息）

```rust
pub struct SourceInfo {
    /// 源文件路径（可选，stdin 时为 None）
    pub file: Option<String>,

    /// 源文件总行数
    pub line_count: usize,
}
```

---

## 9. AST 寻址（JSON Pointer）

AST 中的每个节点都可以通过 JSON Pointer（RFC 6901）定位。这用于 Diff/Patch 操作。

### 9.1 寻址规则

| 节点类型 | 路径格式 | 示例 |
|----------|----------|------|
| Diagram 根节点 | `/` | `/` |
| 图表属性 | `/attributes/{index}` | `/attributes/0` |
| 实体 | `/entities/{id}` | `/entities/api_gateway` |
| 实体属性 | `/entities/{id}/attributes/{key}` | `/entities/api/attributes/type` |
| 关系 | `/relations/{index}` | `/relations/0` |
| 关系属性 | `/relations/{index}/attributes/{key}` | `/relations/0/attributes/status` |
| 分组 | `/groups/{id}` | `/groups/backend` |

### 9.2 寻址示例

给定以下 Drawify：

```drawify
diagram flowchart {
    entity api "API 服务" {
        type: service
        status: healthy
    }
    entity db "数据库" { type: database }
    api -> db "查询"
}
```

| 目标 | JSON Pointer |
|------|-------------|
| api 实体 | `/entities/api` |
| api 的 type 属性 | `/entities/api/attributes/type` |
| api 的 status 属性 | `/entities/api/attributes/status` |
| 第一条关系 | `/relations/0` |
| 关系的标签 | `/relations/0/label` |

---

## 10. AST Diff

### 10.1 DiffResult 结构

```rust
pub struct DiffResult {
    /// 变更列表
    pub changes: Vec<Change>,

    /// Diff 的元信息
    pub summary: DiffSummary,
}

pub struct DiffSummary {
    pub added_entities: usize,
    pub removed_entities: usize,
    pub modified_entities: usize,
    pub added_relations: usize,
    pub removed_relations: usize,
    pub modified_relations: usize,
}
```

### 10.2 Change（变更项）

```rust
pub enum Change {
    /// 新增节点
    Add {
        path: String,          // JSON Pointer
        value: serde_json::Value,
    },

    /// 删除节点
    Remove {
        path: String,
    },

    /// 修改节点
    Modify {
        path: String,
        old_value: serde_json::Value,
        new_value: serde_json::Value,
    },
}
```

### 10.3 Diff 输出示例

```json
{
    "changes": [
        {
            "type": "add",
            "path": "/entities/cache",
            "value": {
                "id": "cache",
                "label": "Redis 缓存",
                "attributes": { "standard": { "type": { "$enum": "cache" } }, "meta": {} }
            }
        },
        {
            "type": "modify",
            "path": "/entities/api/attributes/status",
            "old_value": { "$enum": "healthy" },
            "new_value": { "$enum": "degraded" }
        },
        {
            "type": "remove",
            "path": "/entities/legacy"
        },
        {
            "type": "add",
            "path": "/relations/3",
            "value": {
                "from": "api",
                "to": "cache",
                "arrow": "active",
                "label": "查询缓存"
            }
        }
    ],
    "summary": {
        "added_entities": 1,
        "removed_entities": 1,
        "modified_entities": 1,
        "added_relations": 1,
        "removed_relations": 0,
        "modified_relations": 0
    }
}
```

---

## 11. AST Patch

### 11.1 PatchRequest 结构

```rust
pub struct PatchRequest {
    pub patches: Vec<PatchOp>,
}

pub enum PatchOp {
    /// 新增节点
    Add {
        path: String,
        value: serde_json::Value,
    },

    /// 删除节点
    Remove {
        path: String,
    },

    /// 修改节点属性
    Modify {
        path: String,
        value: serde_json::Value,
    },

    /// 移动节点（修改关系指向等）
    Move {
        from_path: String,
        to_path: String,
    },
}
```

### 11.2 Patch 语义

| 操作 | 行为 | 失败时 |
|------|------|--------|
| `Add` | 在指定路径插入新节点 | 路径已存在 → 错误 |
| `Remove` | 删除指定路径的节点 | 路径不存在 → 错误 |
| `Modify` | 替换指定路径的值 | 路径不存在 → 错误 |
| `Move` | 从 from_path 移到 to_path | 任一不存在 → 错误 |

**事务性：** Patch 操作是原子性的——所有 op 全部成功，或全部回滚。

### 11.3 Patch 请求示例

```json
{
    "patches": [
        {
            "op": "add",
            "path": "/entities/cache",
            "value": {
                "id": "cache",
                "label": "Redis 缓存",
                "attributes": { "standard": { "type": { "$enum": "cache" } }, "meta": {} }
            }
        },
        {
            "op": "modify",
            "path": "/entities/api/attributes/status",
            "value": { "$enum": "degraded" }
        },
        {
            "op": "add",
            "path": "/relations/3",
            "value": {
                "from": "api",
                "to": "cache",
                "arrow": "active",
                "label": "查询缓存",
                "attributes": { "standard": {}, "meta": {} }
            }
        }
    ]
}
```

### 11.4 Patch 响应

```json
{
    "success": true,
    "applied": 3,
    "diagram": { /* 完整的更新后 AST */ }
}
```

**失败响应：**

```json
{
    "success": false,
    "applied": 0,
    "error": {
        "code": "P001",
        "message": "Patch 路径不存在",
        "path": "/entities/nonexistent",
        "index": 1
    }
}
```

---

## 12. 与 Rust 代码的映射

本文档中定义的 AST 结构对应 `drawify-core` crate 中的以下模块：

| AST 概念 | Rust 模块 | 文件 |
|----------|-----------|------|
| 全部结构 | `ast` | `crates/drawify-core/src/ast.rs` |
| Diagram | `ast::Diagram` | 同上 |
| Entity | `ast::Entity` | 同上 |
| Relation | `ast::Relation` | 同上 |
| Group | `ast::Group` | 同上 |
| ArrowType | `ast::ArrowType` | 同上 |
| AttributeMap | `ast::AttributeMap` | 同上 |
| StyleDecl | `ast::StyleDecl` | 同上 |
| StyleDeclKind | `ast::StyleDeclKind` | 同上 |
| RawDiagram | `ast::RawDiagram` | 同上 |
| PreparedDiagram | `ast::PreparedDiagram` | 同上 |
| Span / Position | `ast::Span`, `ast::Position` | 同上 |
| Diff / Patch | `diff` | `crates/drawify-core/src/diff.rs` |

所有结构体都 derive `Debug`, `Clone`, `Serialize`, `Deserialize`，以支持调试、克隆和 JSON 序列化。

---

## 13. 不变量（Invariants）

以下不变量在 AST 构造后始终成立（由解析器和验证器保证）：

| 编号 | 不变量 |
|------|--------|
| I01 | 所有 `entity.id` 在 Diagram 中全局唯一 |
| I02 | 所有 `group.id` 在 Diagram 中全局唯一 |
| I03 | entity.id 与 group.id 不重复 |
| I04 | `relation.from` 和 `relation.to` 指向的 entity 一定存在 |
| I05 | `entity.group_id` 指向的 group 一定存在（非 None 时） |
| I06 | `group.parent_id` 指向的 group 一定存在（非 None 时） |
| I07 | `group.depth` <= 2 |
| I08 | `group.entity_ids` 中的所有 ID 对应的 entity 的 `group_id` 等于该 group 的 ID |
| I09 | 所有 `Span` 的 `start` <= `end` |
| I10 | 所有 `Identifier` 符合 `[a-z][a-z0-9_]*` 规则 |
