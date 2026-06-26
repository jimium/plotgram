# 语义 Diff 与 Patch 使用指南

Drawify 在 **RawDiagram**（parse 之后、prepare 之前）层提供结构化 diff 与 patch，用于 PR 审阅和 Agent 增量改图。

> 实现：`crates/drawify-core/src/diff2/`  
> 详细 API 注释：同目录 `README.md`

---

## 为什么不用文本 diff

| | 文本 diff | diff2 语义 diff |
|--|-----------|-----------------|
| 比较对象 | 字符行 | entity / relation / group |
| theme 展开 | 会产生噪音 | 在 prepare 之前，无派生字段 |
| Agent 消费 | 需重新解析整文件 | `ChangeSet` JSON 可直接 patch |

**不比较** `PreparedDiagram`（那是 theme 物化后的渲染态）。

---

## 闭环

```text
  A ──diff──▶ ChangeSet Δ
  │              │
  │              ▼ patch
  │              A' (RawDiagram)
  │              │
  │              ▼ prepare → validate
  │              PreparedDiagram
  ▼
  B          diff(A', B) 应为空（语义等价）
```

---

## CLI

### 生成变更集

```bash
drawify diff -o old.dfy -n new.dfy
drawify diff -o old.dfy -n new.dfy --format json > changes.json
```

文本模式：统计 `+` / `-` / `~` 并列出路径。

### 应用补丁

```bash
drawify patch base.dfy changes.json -o result.json
```

- 接受 `ChangeSet` 或 `Change[]` JSON
- 成功后对结果 **prepare**，输出 PreparedDiagram JSON
- 应用后建议再 `validate` 或 `render` 确认

---

## Rust API

```rust
use drawify_core::diff2::{self, ChangeSet};
use drawify_core::pipeline::{parse, prepare};
use drawify_core::prepare::StyleRequest;

let old = parse(&old_source)?;
let new = parse(&new_source)?;

let delta = diff2::diff(&old, &new);

let result = diff2::patch(&old, &delta);
if result.is_ok() {
    let prepared = prepare(result.diagram, &StyleRequest::default())?;
}

// 格式化回 DSL 文本（确定性排序，不保留注释）
let text = diff2::format(&raw);
```

### 核心类型

```rust
pub fn diff(old: &RawDiagram, new: &RawDiagram) -> ChangeSet;
pub fn patch(base: &RawDiagram, changes: &ChangeSet) -> PatchResult;
pub fn format(diagram: &RawDiagram) -> String;

pub enum ChangeOp { Add, Remove, Modify }
pub enum ChangeTarget { Diagram, Entity, Relation, Group, StyleDecl }
pub struct Change { op, path, old_value, new_value }
pub struct PatchResult { diagram, applied, errors }
```

---

## 比较范围

| 目标 | 身份键 | 比较内容 |
|------|--------|----------|
| Diagram | — | `diagram_type`、`attributes` |
| Entity | `id` | `label`、`group_id`、各属性命名空间 |
| Relation | `(from, to, label)` | `arrow`、属性（**label 变更 = 删旧增新**） |
| Group | `id` | `label`、`parent_id`、属性 |
| StyleDecl | `(kind, target)` | style 属性 |

**不比较**：`Span`、`entity_ids` / `child_group_ids` / `depth` 等派生字段。

---

## ChangeSet JSON 示例

```json
{
  "changes": [
    {
      "op": "add",
      "path": { "target": "entity", "id": "cache" },
      "new_value": { "id": "cache", "label": "Cache" }
    },
    {
      "op": "modify",
      "path": { "target": "entity", "id": "api", "attr_key": "label" },
      "old_value": "API",
      "new_value": "API Gateway"
    },
    {
      "op": "remove",
      "path": { "target": "relation", "id": "a->b::" },
      "old_value": { "from": "a", "to": "b", "arrow": "active" }
    }
  ]
}
```

`path.attr_key` 约定：`label`、`group_id`、`parent_id`、`arrow`、`standard/<key>`、`style/<key>`、`meta/<key>`。

---

## Agent 推荐流程

1. 读取基准图 `base.dfy`，`parse` 得 `RawDiagram`
2. 用 LLM 生成目标图或手工编辑得 `new.dfy`
3. `diff(base, new)` → `ChangeSet`（可审查、可裁剪）
4. `patch(base, Δ)` → 更新后的 `RawDiagram`
5. `prepare` + `validate` + `render` 验证
6. 可选：`format` 写回 `.dfy` 文本

避免让 Agent 直接输出整文件 DSL，可减少语法错误与无关 diff。

---

## 注意事项

- Patch 后必须能通过 `validate`；布局相关属性错误会在 prepare/validate 阶段暴露
- `format` 输出按 id/key **确定性排序**，不保留原文排版与注释
- Relation 的 `edge_style` 等是 DSL 关键字，不能写在 relation 属性块内作为普通 key

---

## 相关文档

- [drawify-cli.md](drawify-cli.md) — `diff` / `patch` 命令
- [render-pipeline.md](render-pipeline.md) — parse / prepare 阶段
- [specs/ast-spec.md](../specs/ast-spec.md) — AST 结构
