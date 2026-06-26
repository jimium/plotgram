# Drawify 错误模型与反馈机制

> 版本：0.1.0 | 状态：已实现（P0–P3）

本文档定义 Drawify 的结构化错误体系。错误反馈是 Drawify 区别于传统图表工具的核心能力之一——当 AI Agent 生成错误时，Drawify 不只返回"出错了"，而是返回**哪里错了、为什么错了、怎么修**。

实现位于 [`crates/drawify-core/src/error.rs`](../../crates/drawify-core/src/error.rs)，供 Agent / CLI / LSP 消费方参考。

---

## 1. 设计理念

### 1.1 错误是一等公民

传统图表工具的错误是文本字符串（甚至是空白页）。Drawify 的错误是**结构化对象**，可以被程序解析、理解和处理。

```
传统工具: 源文本 → 渲染失败 → "Error" / 空白页
Drawify:   源文本 → 解析失败 → 结构化错误（JSON）→ Agent 理解 → 修复 → 重试
```

### 1.2 错误驱动的自我修正闭环

```
AI Agent 生成 Drawify
    ↓
Drawify 解析/验证
    ↓ (失败)
返回结构化错误列表
    ↓
Agent 读取错误信息
    ↓
应用 fix 建议 或 根据上下文重新生成
    ↓
重新提交（目标：1 次重试内修复）
```

### 1.3 设计约束

| 约束 | 说明 |
|------|------|
| 每个错误必须有错误码 | 方便 Agent 分类处理 |
| 每个错误必须有位置信息 | Agent 能精确定位源文本 |
| 每个错误必须有修复建议 | Agent 知道怎么改 |
| 错误信息必须是 JSON | 可被程序直接解析 |
| 错误之间互不依赖 | 每个错误独立可理解 |
| 尽可能收集多个错误 | 一次返回所有错误，减少重试次数 |

---

## 2. 错误对象结构

### 2.1 DiagnosticError（单个错误）

```json
{
    "code": "E003",
    "severity": "error",
    "category": "validation",
    "message": "关系引用了不存在的实体 'payment_db'",
    "location": {
        "start": { "line": 12, "column": 5 },
        "end": { "line": 12, "column": 30 }
    },
    "context": {
        "referenced_entity": "payment_db",
        "available_entities": ["user", "api", "db"],
        "relation_index": 3
    },
    "suggestion": {
        "text": "请确认实体名拼写，或在图表中定义实体 'payment_db'",
        "fix": {
            "action": "add_entity",
            "payload": {
                "id": "payment_db",
                "label": "payment_db",
                "attributes": { "standard": { "type": { "$enum": "service" } }, "meta": {} }
            }
        }
    }
}
```

### 2.2 字段说明

| 字段 | 类型 | 必需 | 说明 |
|------|------|------|------|
| `code` | string | 是 | 错误码（如 `"E003"`），机器可识别 |
| `severity` | enum | 是 | 严重级别：`"error"` 或 `"warning"` |
| `category` | enum | 是 | 错误类别：`"parse"`, `"validation"`, `"render"`, `"patch"` |
| `message` | string | 是 | 人类可读的错误描述 |
| `location` | Span | 是 | 源文本中的位置范围（行列从 1 开始） |
| `context` | object | 否 | 与错误相关的上下文数据（帮助 Agent 理解错误） |
| `suggestion` | Suggestion | 否 | 修复建议和可选的自动修复 payload |

### 2.3 Suggestion（修复建议）

```json
{
    "text": "修复建议的自然语言描述",
    "fix": {
        "action": "修复动作类型",
        "payload": { /* 修复数据 */ }
    }
}
```

| 字段 | 说明 |
|------|------|
| `text` | 自然语言描述，供 Agent 理解修复方向 |
| `fix.action` | 修复动作类型（见 §5） |
| `fix.payload` | 修复所需的数据，可直接合并到 AST |

### 2.4 Rust 实现

所有错误码集中注册在 `ErrorCode` 枚举中，**编译期保证码唯一**，杜绝 `code: String` 时代的码冲突与拼写错误。

```rust
pub enum ErrorCode {
    // 解析错误
    E001, E002, E006, E007, E008, E009, E010,
    // 验证错误
    E003, E004, E005, E011, E012, E013, E014, E015, E016,
    // 渲染错误
    E101, E102,
    // 警告
    W001, W002, W003, W004, W005, W006, W007, W008, W009,
    // Patch 错误
    P001, P002, P003, P004,
}

pub struct DiagnosticError {
    pub code: ErrorCode,
    pub severity: Severity,        // Error | Warning
    pub category: Category,        // Parse | Validation | Render | Patch
    pub message: String,
    pub location: Span,            // start/end Position (line, column 从 1 开始)
    pub context: Option<serde_json::Value>,
    pub suggestion: Option<Suggestion>,
}
```

- `ErrorCode` 实现 `Serialize`（序列化为 `"E001"` 字符串）、`Display`、`PartialEq`、`Hash`。
- `as_str()` 返回规范码字符串；`category()` / `severity()` 由码自动推导。
- `DiagnosticError` 实现 `Serialize`（可直接作为 JSON API 响应）、`Display`（人类可读终端输出）。
- 每个错误码都有对应的便捷构造方法（如 `syntax_error`、`undefined_reference`、`invalid_enum_value` 等），自动填充 code / category / context / suggestion。

---

## 3. 错误码体系

### 3.1 编码规则

错误码格式：`{类型前缀}{三位数字}`

| 前缀 | 含义 |
|------|------|
| `E` | Error（错误，阻止渲染） |
| `W` | Warning（警告，不阻止渲染） |
| `P` | Patch Error（Patch 操作错误） |

### 3.2 解析错误（Parse Errors）

在将文本转换为 AST 的过程中产生的错误。

| 错误码 | 名称 | 构造方法 | 说明 | Fix Action |
|--------|------|----------|------|------------|
| E001 | SyntaxError | `syntax_error` | 无法识别的语法结构 | `replace_text` |
| E002 | DuplicateId | `duplicate_id` | 实体或分组 ID 重复 | `rename_entity` |
| E006 | UnterminatedString | `unterminated_string` | 字符串字面量未闭合 | `replace_text` |
| E007 | InvalidIdentifier | `invalid_identifier` / `hyphenated_identifier` | 标识符不符合命名规则（含连字符情形） | `replace_text` |
| E008 | UnexpectedToken | `unexpected_token` | 遇到了意外的 token | `replace_text`（单期望值时） |
| E009 | MissingDiagram | `missing_diagram` | 文件缺少 diagram 声明 | `replace_text` |
| E010 | MultipleDiagrams | `multiple_diagrams` | 文件包含多个 diagram 声明 | — |

#### E001 SyntaxError

**触发条件：** 文本中存在无法解析为任何合法语法构造的内容。

```json
{
    "code": "E001",
    "severity": "error",
    "category": "parse",
    "message": "无法解析的语法：在第 8 行遇到了意外的 '=>' 符号",
    "location": {
        "start": { "line": 8, "column": 5 },
        "end": { "line": 8, "column": 7 }
    },
    "context": {
        "unexpected": "=>",
        "expected": ["->", "-->", "<->"]
    },
    "suggestion": {
        "text": "Drawify 只支持三种箭头：-> (主动), --> (被动), <-> (双向)。请使用 -> 替代 =>",
        "fix": {
            "action": "replace_text",
            "payload": { "old": "=>", "new": "->" }
        }
    }
}
```

#### E002 DuplicateId

**触发条件：** 两个 entity 或 group 使用了相同的 identifier。

```json
{
    "code": "E002",
    "severity": "error",
    "category": "parse",
    "message": "实体 ID 'api' 重复定义（首次定义在第 5 行）",
    "location": {
        "start": { "line": 12, "column": 5 },
        "end": { "line": 12, "column": 28 }
    },
    "context": {
        "duplicate_id": "api",
        "first_defined_at": { "line": 5, "column": 5 }
    },
    "suggestion": {
        "text": "请将第二个 'api' 重命名为其他名称，如 'api_v2' 或 'api_backup'"
    }
}
```

#### E007 InvalidIdentifier

```json
{
    "code": "E007",
    "severity": "error",
    "category": "parse",
    "message": "无效的标识符 'API-Service'：标识符只能包含小写字母、数字和下划线",
    "location": {
        "start": { "line": 5, "column": 12 },
        "end": { "line": 5, "column": 23 }
    },
    "context": {
        "invalid_id": "API-Service",
        "rule": "[a-z][a-z0-9_]*"
    },
    "suggestion": {
        "text": "建议使用 'api_service' 替代 'API-Service'",
        "fix": {
            "action": "replace_text",
            "payload": { "old": "API-Service", "new": "api_service" }
        }
    }
}
```

### 3.3 验证错误（Validation Errors）

在 AST 语义验证阶段产生的错误。此时语法解析已成功，AST 已构建。

| 错误码 | 名称 | 构造方法 | 说明 | Fix Action |
|--------|------|----------|------|------------|
| E003 | UndefinedReference | `undefined_reference` | 关系引用了不存在的实体 | `add_entity` |
| E004 | InvalidAttribute | `invalid_attribute` | 属性名或值不符合 Schema | `rename_attribute` |
| E005 | StructureViolation | `structure_violation` | 违反结构约束（如 group 嵌套过深） | — |
| E011 | InvalidEnumValue | `invalid_enum_value` | 枚举属性值不在合法列表中（含 Levenshtein 相似度建议） | `replace_attribute_value` |
| E012 | GroupRelation | `group_relation` | group 之间直接建立关系 | — |
| E013 | SelfLoop | `self_loop_error` | 不允许的自环关系 | `remove_relation` |
| E014 | DuplicateStyleDecl | `duplicate_style_decl` | 同名 `node_style` 或 `edge_style` 重复声明 | — |
| E015 | InvalidEdgeStyleRef | `invalid_edge_style_ref` | relation 上使用 `style:` 引用边样式（应使用 `edge_style:`） | `replace_text` |
| E016 | StyleTypeMismatch | `style_type_mismatch` | `style.*` 属性值类型不匹配 | — |

#### E003 UndefinedReference

```json
{
    "code": "E003",
    "severity": "error",
    "category": "validation",
    "message": "关系引用了不存在的实体 'payment_db'",
    "location": {
        "start": { "line": 15, "column": 5 },
        "end": { "line": 15, "column": 25 }
    },
    "context": {
        "referenced_entity": "payment_db",
        "available_entities": ["user", "api", "db"],
        "relation_index": 3
    },
    "suggestion": {
        "text": "确认实体名拼写，或添加 entity payment_db 定义",
        "fix": {
            "action": "add_entity",
            "payload": {
                "id": "payment_db",
                "label": "payment_db",
                "attributes": { "standard": { "type": { "$enum": "service" } }, "meta": {} }
            }
        }
    }
}
```

#### E004 InvalidAttribute

```json
{
    "code": "E004",
    "severity": "error",
    "category": "validation",
    "message": "未知属性 'color'：不在预定义属性 Schema 中，且未使用 meta. 前缀",
    "location": {
        "start": { "line": 7, "column": 5 },
        "end": { "line": 7, "column": 16 }
    },
    "context": {
        "invalid_attribute": "color",
        "entity_id": "api",
        "valid_attributes": ["type", "status", "owner", "description"]
    },
    "suggestion": {
        "text": "如需自定义属性，请使用 meta. 前缀：meta.color。如果是预定义属性，请检查拼写",
        "fix": {
            "action": "rename_attribute",
            "payload": {
                "entity_id": "api",
                "old_key": "color",
                "new_key": "meta.color"
            }
        }
    }
}
```

#### E011 InvalidEnumValue

E011 的 `Suggestion.text` 会自动通过 **Levenshtein 距离**计算最相似的合法值：

```
✗ E011 [line 6:10] 属性 'type' 的值 'server' 不在合法枚举列表中
  合法值: service, database, person, client, queue, cache, gateway, storage, external, decision, process, start, end
  建议: 'server' 与 'service' 相似，是否应使用 'service'？
```

```json
{
    "code": "E011",
    "severity": "error",
    "category": "validation",
    "message": "属性 'type' 的值 'server' 不在合法枚举列表中",
    "location": {
        "start": { "line": 6, "column": 10 },
        "end": { "line": 6, "column": 16 }
    },
    "context": {
        "attribute": "type",
        "invalid_value": "server",
        "valid_values": ["service", "database", "person", "client", "queue", "cache", "gateway", "storage", "external", "decision", "process", "start", "end"]
    },
    "suggestion": {
        "text": "'server' 与 'service' 相似，是否应使用 'service'？",
        "fix": {
            "action": "replace_attribute_value",
            "payload": {
                "entity_id": "api",
                "attribute": "type",
                "old_value": "server",
                "new_value": "service"
            }
        }
    }
}
```

#### E005 StructureViolation

```json
{
    "code": "E005",
    "severity": "error",
    "category": "validation",
    "message": "group 嵌套深度超过 2 层：group 'deep' 嵌套在 'middle' 中，而 'middle' 已嵌套在 'outer' 中",
    "location": {
        "start": { "line": 15, "column": 5 },
        "end": { "line": 15, "column": 30 }
    },
    "context": {
        "group_id": "deep",
        "current_depth": 3,
        "max_depth": 2,
        "parent_chain": ["outer", "middle"]
    },
    "suggestion": {
        "text": "请将 group 'deep' 提升为 group 'middle' 的同级，或将其中的 entity 移到 'middle' 中"
    }
}
```

#### E014 DuplicateStyleDecl

```json
{
    "code": "E014",
    "severity": "error",
    "category": "validation",
    "message": "重复的 node_style 声明：'service' 已在第 5 行声明",
    "location": {
        "start": { "line": 12, "column": 5 },
        "end": { "line": 12, "column": 30 }
    },
    "context": {
        "decl_kind": "node_style",
        "selector": "service",
        "first_defined_at": { "line": 5, "column": 5 }
    },
    "suggestion": {
        "text": "请合并两处 node_style service 声明，或移除重复声明"
    }
}
```

#### E016 StyleTypeMismatch

```json
{
    "code": "E016",
    "severity": "error",
    "category": "validation",
    "message": "样式属性 'stroke_width' 的值类型不匹配：期望 Number，实际为 String",
    "location": {
        "start": { "line": 8, "column": 20 },
        "end": { "line": 8, "column": 30 }
    },
    "context": {
        "attribute": "stroke_width",
        "expected_type": "Number",
        "actual_type": "String"
    },
    "suggestion": {
        "text": "请使用数值类型，如 stroke_width: 2.0"
    }
}
```

### 3.4 警告（Warnings）

警告不阻止渲染，但提示潜在问题。

| 错误码 | 名称 | 构造方法 | 说明 |
|--------|------|----------|------|
| W001 | OrphanEntity | `orphan_entity` | 存在无关系的孤立实体 |
| W002 | RedundantAttribute | `redundant_attribute` | 属性存在但不影响当前图表类型的渲染 |
| W003 | SelfLoopWarning | `self_loop_warning` | 自环关系（type=decision 等豁免类型时仅为 warning） |
| W004 | EmptyDiagram | `empty_diagram` | diagram 体内无任何声明 |
| W005 | UnusedGroup | `unused_group` | group 已声明但内部无任何 entity |
| W006 | UnknownStyleSelector | `unknown_style_selector` | `node_style` 的 selector 不是当前 DiagramType 支持的 entity type |
| W007 | UnresolvedEdgeStyle | `unresolved_edge_style` | relation 的 `edge_style: <name>` 引用了不存在的声明 |
| W008 | UnknownSemantic | `unknown_semantic` | 未知图标语义 |
| W009 | UnknownIcon | `unknown_icon` | 未知图标名 |

#### W001 OrphanEntity

```json
{
    "code": "W001",
    "severity": "warning",
    "category": "validation",
    "message": "实体 'logger' 没有与任何其他实体建立关系",
    "location": {
        "start": { "line": 20, "column": 5 },
        "end": { "line": 20, "column": 25 }
    },
    "context": {
        "entity_id": "logger",
        "entity_label": "日志服务"
    },
    "suggestion": {
        "text": "孤立的实体不会在图表中显示有意义的连接。请检查是否遗漏了关系声明"
    }
}
```

### 3.5 渲染错误（Render Errors）

| 错误码 | 名称 | 构造方法 | 说明 |
|--------|------|----------|------|
| E101 | LayoutFailed | `layout_failed` | 布局算法无法为当前图结构生成有效布局 |
| E102 | RenderInternal | `render_internal` | 渲染器内部错误（格式不支持、编码失败等） |

### 3.6 Patch 错误（Patch Errors）

| 错误码 | 名称 | 构造方法 | 说明 |
|--------|------|----------|------|
| P001 | PathNotFound | `path_not_found` | Patch 目标路径不存在 |
| P002 | PathAlreadyExists | `path_already_exists` | Add 操作的目标路径已存在 |
| P003 | InvalidPatchValue | `invalid_patch_value` | Patch 值不符合目标路径的类型约束 |
| P004 | PatchConflict | `patch_conflict` | 多个 Patch 操作冲突 |

---

## 4. 错误响应格式

### 4.1 验证响应（Validate API）

```json
{
    "valid": false,
    "errors": [
        { "code": "E003", ... },
        { "code": "E004", ... }
    ],
    "warnings": [
        { "code": "W001", ... }
    ],
    "diagram": null
}
```

**验证成功时：**

```json
{
    "valid": true,
    "errors": [],
    "warnings": [],
    "diagram": { /* 完整 AST */ }
}
```

### 4.2 渲染响应（Render API）

**成功时：**
```json
{
    "success": true,
    "output": "<svg>...</svg>",
    "format": "svg",
    "warnings": []
}
```

**失败时：**
```json
{
    "success": false,
    "output": null,
    "errors": [
        { "code": "E003", ... }
    ],
    "warnings": []
}
```

### 4.3 CLI 文本输出

`DiagnosticError` 的 `Display` 输出格式：

```
✗ E003 [line 15:5] 关系引用了不存在的实体 'payment_db'
  可用实体: user, api, db
  建议: 确认实体名拼写，或添加 entity payment_db 定义
```

- 第一行：图标（✗/⚠）+ 错误码 + [line:col] + 消息
- 上下文行：自动从 `context` JSON 提取 `available_entities`、`valid_values`、`referenced_entity` 等字段
- 建议行：`建议: <suggestion.text>`

`drawify validate` 和 `drawify render` 在 text 模式下会显示源码片段，带 `^` 指示错误位置：

```
✗ E013 [line 36:5] 实体 'n9' 存在不允许的自环关系（仅 type: decision 允许自环）
  实体: n9
  建议: 请移除自环关系，或将实体 type 改为 decision
  │
  │     n9 -> n9 "等待异步 (自环)"
  │     ^^^^^^^^
```

### 4.4 CLI `--format json`

`drawify validate --format json` 输出结构化 JSON：

```json
{
  "errors": [ { "code": "E003", "severity": "error", "category": "validation", ... } ],
  "warnings": [ ... ],
  "total_errors": 2,
  "total_warnings": 1,
  "truncated": false,
  "valid": false
}
```

---

## 5. Fix Action 类型

### 5.1 自动修复动作

| action | 说明 | payload 结构 |
|--------|------|-------------|
| `add_entity` | 添加缺失的实体 | `{ id, label, attributes }` |
| `remove_entity` | 删除多余的实体 | `{ id }` |
| `rename_entity` | 重命名实体 ID | `{ old_id, new_id }` |
| `add_relation` | 添加关系 | `{ from, to, arrow, label }` |
| `remove_relation` | 删除关系 | `{ from, to }` |
| `replace_attribute_value` | 替换属性值 | `{ entity_id, attribute, old_value, new_value }` |
| `rename_attribute` | 重命名属性键 | `{ entity_id, old_key, new_key }` |
| `replace_text` | 替换源文本（用于解析错误的修复） | `{ old, new }` |
| `remove_group` | 删除分组 | `{ id }` |

### 5.2 Fix 应用流程

```
Agent 收到错误
    ↓
读取 suggestion.fix
    ↓
判断 fix.action 类型
    ↓
方式 A: 将 payload 作为 Patch 请求发送到 /patch
方式 B: 直接在源文本中应用 replace_text
    ↓
重新提交
```

### 5.3 已实现 Fix Action 对照表

以下错误码已补全 `suggestion.fix` payload，AI Agent 可直接消费进行自动修复：

| 错误码 | action | payload 示例 |
|--------|--------|-------------|
| E001 / E006 / E007 / E008 / E009 | `replace_text` | `{ "old": "=>", "new": "->" }` |
| E002 | `rename_entity` | `{ "old_id": "api", "new_id": "api_v2" }` |
| E003 | `add_entity` | `{ "id": "payment_db", "label": "", "attributes": {} }` |
| E004 | `rename_attribute` | `{ "entity_id": "api", "old_key": "color", "new_key": "fill" }` |
| E011 | `replace_attribute_value` | `{ "attribute": "type", "old_value": "server", "new_value": "service" }` |
| E013 | `remove_relation` | `{ "from": "api", "to": "api" }` |
| E015 | `replace_text` | `{ "old": "style:", "new": "edge_style:" }` |

---

## 6. 错误收集策略

### 6.1 多错误收集

解析器和验证器应尽可能收集**所有**错误，而不是遇到第一个错误就停止。

**解析阶段：**
- `parse()` 公共 API 始终走 `parse_with_diagnostics` fallback 路径
- 遇到语法错误时，尝试跳过当前语句，继续解析后续内容
- 恢复策略：跳过到下一个 `entity`、`group` 或关系声明的起始位置

**验证阶段：**
- 遍历所有语义约束，收集所有违反项
- 不依赖关系（每个约束独立检查）

### 6.2 错误优先级排序

`ValidationResult::sort()` 按以下优先级排序，`parse_prepare_validate` 在返回前自动调用：

1. 解析错误（category=parse）
2. 结构验证错误（E003、E005、E012、E013）
3. 属性验证错误（其他 validation errors）
4. 警告（W0xx）

同级错误按 `location.start.line` 升序排列。

### 6.3 错误数量限制

- 最多返回 **20 个错误**（`MAX_ERRORS`）
- 警告最多返回 **10 个**（`MAX_WARNINGS`）
- 超出时 `truncated = true`，`total_errors` / `total_warnings` 反映真实总数

```json
{
    "valid": false,
    "errors": [ /* 最多 20 个 */ ],
    "warnings": [ /* 最多 10 个 */ ],
    "truncated": true,
    "total_errors": 35,
    "total_warnings": 12
}
```

### 6.4 ValidationResult

```rust
pub struct ValidationResult {
    pub errors: Vec<DiagnosticError>,
    pub warnings: Vec<DiagnosticError>,
    pub total_errors: usize,    // 包含被截断的错误总数
    pub total_warnings: usize,
    pub truncated: bool,        // 是否发生了截断
}
```

---

## 7. LSP 兼容映射

Drawify 错误模型与 Language Server Protocol 的 Diagnostic 规范兼容：

| DrawifyError 字段 | LSP Diagnostic 字段 | 映射方式 |
|------------------|---------------------|----------|
| `location` | `range` | `start.line - 1`, `start.column - 1`（LSP 从 0 开始） |
| `severity` | `severity` | `"error"` → `1`, `"warning"` → `2` |
| `code` | `code` | 直接映射（string） |
| `message` | `message` | 直接映射 |
| — | `source` | 固定为 `"drawify"` |

`DiagnosticError::to_lsp()` 生成 LSP Diagnostic 协议兼容的 JSON：

```json
{
  "range": {
    "start": { "line": 14, "character": 4 },
    "end":   { "line": 14, "character": 29 }
  },
  "severity": 1,
  "code": "E003",
  "source": "drawify",
  "message": "关系引用了不存在的实体 'payment_db'"
}
```

可直接用于 LSP server / VS Code 扩展的 `publishDiagnostics`。

---

## 8. Rust 内部错误传播

### 8.1 DrawifyError

Rust 层面的 `Result<T, DrawifyError>` 传播链：

```rust
pub enum DrawifyError {
    Parse(Vec<DiagnosticError>),
    Prepare(Vec<DiagnosticError>),
    Render(Vec<DiagnosticError>),
    Patch(Vec<DiagnosticError>),
    Style(String),
}
```

- 每个变体携带完整的 `DiagnosticError` 列表，不再丢失错误码与上下文。
- `into_diagnostics()` 统一提取所有变体中的 `DiagnosticError`，便于上层 CLI / Server 处理。
- 便捷构造方法：`parse_msg`、`render_internal_msg`、`layout_failed_msg`、`patch_value_msg`。

### 8.2 Patch 错误结构化

`diff/patch.rs` 的 `apply_add` / `apply_remove` / `apply_modify` 返回 `Result<(), DiagnosticError>`：

| 场景 | 错误码 | 构造方法 |
|------|--------|----------|
| Remove/Modify 目标不存在 | P001 | `path_not_found` |
| Add 目标已存在 | P002 | `path_already_exists` |
| 值类型解析失败 / 缺少必需字段 | P003 | `invalid_patch_value` |
| 多操作冲突 | P004 | `patch_conflict` |

`PatchResult.errors` 类型为 `Vec<DiagnosticError>`，每个错误都带完整的 code / path / context。

---

## 9. 错误码完整索引

| 错误码 | 名称 | 类别 | 严重级别 | 构造方法 |
|--------|------|------|----------|----------|
| E001 | SyntaxError | parse | error | `syntax_error` |
| E002 | DuplicateId | parse | error | `duplicate_id` |
| E003 | UndefinedReference | validation | error | `undefined_reference` |
| E004 | InvalidAttribute | validation | error | `invalid_attribute` |
| E005 | StructureViolation | validation | error | `structure_violation` |
| E006 | UnterminatedString | parse | error | `unterminated_string` |
| E007 | InvalidIdentifier | parse | error | `invalid_identifier` |
| E008 | UnexpectedToken | parse | error | `unexpected_token` |
| E009 | MissingDiagram | parse | error | `missing_diagram` |
| E010 | MultipleDiagrams | parse | error | `multiple_diagrams` |
| E011 | InvalidEnumValue | validation | error | `invalid_enum_value` |
| E012 | GroupRelation | validation | error | `group_relation` |
| E013 | SelfLoop | validation | error | `self_loop_error` |
| E014 | DuplicateStyleDecl | validation | error | `duplicate_style_decl` |
| E015 | InvalidEdgeStyleRef | validation | error | `invalid_edge_style_ref` |
| E016 | StyleTypeMismatch | validation | error | `style_type_mismatch` |
| E101 | LayoutFailed | render | error | `layout_failed` |
| E102 | RenderInternal | render | error | `render_internal` |
| W001 | OrphanEntity | validation | warning | `orphan_entity` |
| W002 | RedundantAttribute | validation | warning | `redundant_attribute` |
| W003 | SelfLoopWarning | validation | warning | `self_loop_warning` |
| W004 | EmptyDiagram | validation | warning | `empty_diagram` |
| W005 | UnusedGroup | validation | warning | `unused_group` |
| W006 | UnknownStyleSelector | validation | warning | `unknown_style_selector` |
| W007 | UnresolvedEdgeStyle | validation | warning | `unresolved_edge_style` |
| W008 | UnknownSemantic | validation | warning | `unknown_semantic` |
| W009 | UnknownIcon | validation | warning | `unknown_icon` |
| P001 | PathNotFound | patch | error | `path_not_found` |
| P002 | PathAlreadyExists | patch | error | `path_already_exists` |
| P003 | InvalidPatchValue | patch | error | `invalid_patch_value` |
| P004 | PatchConflict | patch | error | `patch_conflict` |

---

## 10. 涉及文件

| 文件 | 说明 |
|------|------|
| `crates/drawify-core/src/error.rs` | ErrorCode 枚举、DiagnosticError、DrawifyError、Display、LSP |
| `crates/drawify-core/src/diff/types.rs` | `PatchResult.errors` → `Vec<DiagnosticError>` |
| `crates/drawify-core/src/diff/patch.rs` | 返回 `Result<(), DiagnosticError>`，使用 P001–P004 |
| `crates/drawify-core/src/dsl/parser/mod.rs` | `parse()` 始终走 fallback |
| `crates/drawify-core/src/pipeline/mod.rs` | 渲染错误保留原始错误码 |
| `crates/drawify-core/src/pipeline/prepare.rs` | `PipelineOutput` 新增 total_errors/truncated/sort |
| `crates/drawify-core/src/validation/mod.rs` | 验证入口与错误收集 |
| `crates/drawify-core/src/validation/common.rs` | self_loop 区分豁免(W003)/非豁免(E013) |
| `crates/drawify-core/src/validation/attrs.rs` | 使用 StylePropError 区分 E004/E016 |
| `crates/drawify-core/src/prepare/styles.rs` | 样式声明与选择器验证 |
| `crates/drawify-core/src/icons/validate.rs` | W008/W009 图标语义与名称验证 |
| `crates/drawify-core/src/types/style_attrs.rs` | StylePropError 枚举 |
| `crates/drawify-core/src/render/scene.rs` | layout 错误 → E101 |
| `crates/drawify-core/src/render/encode/*.rs` | 渲染错误 → E102 |
| `crates/drawify-cli/src/main.rs` | `--format json`、源码片段、`into_diagnostics` |
