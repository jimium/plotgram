# diff2 — Intent Diff

在 **parse 之后、prepare 之前** 的语义层（[`RawDiagram`]）上工作的差异比较、补丁应用与格式化模块。

与现有 `crate::diff`（面向 `PreparedDiagram` 的渲染态比较）分离。本模块表达作者真正写进 DSL 的意图，而不是 theme 展开、默认值补全等派生结果。

## 适用场景

- **PR 审阅**：结构化展示两份 DSL 的语义差异，比文本 diff 更精确
- **Agent 增量改图**：基于 `ChangeSet` 精确修改图结构，而非整图重写

> 物化后的有效态（`PreparedDiagram`）比较不在本模块范围内。

## 架构

```
diff2/
├── mod.rs      模块根，导出公开 API
├── types.rs    变更类型定义（ChangeSet / Change / ChangePath / ...）
├── diff.rs     Diff：比较两份 RawDiagram，输出 ChangeSet
├── patch.rs    Patch：将 ChangeSet 应用到基础 RawDiagram
├── format.rs   Formatter：将 RawDiagram 还原为 DSL 文本
└── tests.rs    测试套件（49 个测试）
```

### 公开 API

```rust
// 比较
pub fn diff(old: &RawDiagram, new: &RawDiagram) -> ChangeSet;

// 应用
pub fn patch(base: &RawDiagram, changes: &ChangeSet) -> PatchResult;

// 格式化
pub fn format(diagram: &RawDiagram) -> String;

// 类型
pub enum ChangeOp { Add, Remove, Modify }
pub enum ChangeTarget { Diagram, Entity, Relation, Group, StyleDecl }
pub struct ChangePath { target, id, attr_key }
pub struct Change { op, path, old_value, new_value }
pub struct ChangeSet { changes: Vec<Change> }
pub struct PatchResult { diagram, applied, errors }
```

## 闭环

三者形成闭环：`diff(A, B)` 得到变更集 `Δ`，`patch(A, Δ)` 再 `format`，应得到与 `B` 语义等价的新 DSL。

```text
    A ──diff──▶ Δ
    │           │
    │           ▼ patch
    │           A'
    │           │
    │           ▼ format
    │           DSL 文本
    │           │
    │           ▼ parse
    │           A''
    │           │
    ▼           ▼
    B ◀── diff 为空 ── A''
```

**约束：**

- Patch 产出的 `RawDiagram` 需通过 `prepare() → validate()` 全链路无 errors
- Formatter 保证语义 round-trip，**不保留**原文注释与排版
- 输出顺序按确定性排序（属性 / 元素按 key / id 排序），便于 diff 工具比较

## Diff 能力

### 比较范围

| 目标 | 身份键 | 比较内容 |
|------|--------|----------|
| Diagram | — | `diagram_type`、`attributes`（按 key） |
| Entity | `id` | `label`、`group_id`、`standard`、`style`、`meta` |
| Relation | `(from, to, label)` 三元组 | `arrow`、`standard`、`style`、`meta` |
| Group | `id` | `label`、`parent_id`、`standard`、`meta` |
| StyleDecl | `(kind, target)` | `style` 属性 |

### 不比较的内容（非语义）

- `Span` / `SourceInfo`（源码位置）
- `StyleSource`（RawDiagram 中均为 `Inline`）
- `group.entity_ids` / `group.child_group_ids` / `group.depth`（派生字段，从 `entity.group_id` 和 `group.parent_id` 推导）

### Relation 身份规则

label 是 relation 身份的一部分：

- **label 变更** → Remove 旧 relation + Add 新 relation
- **arrow 变更** → Modify（只改 `arrow` 字段）

### ChangePath attr_key 约定

| attr_key | 含义 |
|----------|------|
| `diagram_type` | 图类型变更 |
| `label` | entity / group / relation 的标签 |
| `group_id` | entity 所属分组（`null` 表示移出分组） |
| `parent_id` | group 的父分组（`null` 表示顶层） |
| `arrow` | relation 箭头类型 |
| `standard/<key>` | standard 命名空间属性 |
| `style/<key>` | style 命名空间属性 |
| `meta/<key>` | meta 命名空间属性 |

## Patch 能力

### 应用流程

1. 克隆基础 `Diagram`
2. 逐条应用 `Change`（Add / Remove / Modify）
3. 重建派生字段：`group.entity_ids`、`group.child_group_ids`、`group.depth`

即使部分变更失败，也返回已应用部分的结果。调用方应检查 `PatchResult::is_ok`。

### 子属性 Add/Remove

Add / Remove 操作的 `attr_key` 为 `Some` 时，表示子属性增删（如 `standard/owner`），等价于对应命名空间 map 的 insert / remove。Patch 内部将此类操作委托给 Modify 逻辑处理。

### Group membership 重建

Patch 完成后自动调用 `rebuild_group_membership`：

- `entity.group_id` 是权威来源
- `group.entity_ids` 从 `entity.group_id` 反推
- `group.child_group_ids` 从 `group.parent_id` 反推
- `group.depth` 沿 `parent_id` 链计算

## Formatter 能力

### 输出结构

```text
diagram <type> {
    <diagram 属性, 按 key 排序>

    <style_decls, 按 (kind, target) 排序>

    <顶层 groups, 按 id 排序, 递归包含 entity / 子 group>

    <顶层 entities (group_id=None), 按 id 排序>

    <relations, 按 (from, to, label) 排序>
}
```

- 4 空格缩进
- 各 section 之间空行分隔
- 属性块内按 `standard → style → meta` 顺序，各命名空间内按 key 排序
- 无属性的 entity / relation 不输出 `{}`
- `Config` 值输出为多行块：`algo {\n    key: value\n}`

### 值格式化

| AttributeValue | 输出 |
|----------------|------|
| `String(s)` | `"s"`（含转义） |
| `Number(n)` | 整数 `N` / 小数 `N.M` |
| `Boolean(b)` | `true` / `false` |
| `Atom(s)` / `Enum(s)` | `s`（裸 atom） |
| `Config { algo, options }` | `algo {\n    key: value\n}` |

## ChangeSet JSON 格式

`ChangeSet` 支持 serde 序列化 / 反序列化，可作为持久化格式或 API 传输。

```json
{
  "changes": [
    {
      "op": "add",
      "path": { "target": "entity", "id": "c" },
      "new_value": { "id": "c", "label": "C" }
    },
    {
      "op": "modify",
      "path": { "target": "entity", "id": "a", "attr_key": "label" },
      "old_value": "旧标签",
      "new_value": "新标签"
    },
    {
      "op": "remove",
      "path": { "target": "relation", "id": "b->c::调用" },
      "old_value": { "from": "b", "to": "c", "arrow": "active", "label": "调用" }
    }
  ]
}
```

### AttributeValue JSON 标记格式

`AttributeValue` 序列化时使用标记对象区分变体：

| 变体 | JSON |
|------|------|
| `String(s)` | `"s"` |
| `Number(n)` | `123` / `1.5` |
| `Boolean(b)` | `true` / `false` |
| `Atom(s)` | `{"$atom": "s"}` |
| `Enum(s)` | `{"$enum": "s"}` |
| `Config { algo, options }` | `{"$config": {"algo": "...", "options": {...}}}` |

> Patch 内部使用自定义 `json_to_attribute_value` 还原这些格式，因为 `AttributeValue` 的 serde derive 只有 `serialize_with`、没有对应的 `deserialize_with`。

## 技术约束

- **`edge_style` / `node_style` 是词法关键字**，不能作为属性块中的 key（如 `edge_style: error` 在 relation 属性块中会解析失败）。顶层声明 `edge_style error { ... }` 不受影响。
- **RawDiagram 中所有 `StyleSource` 均为 `Inline`**，theme 展开 / palette 物化在 `prepare` 阶段才发生。
