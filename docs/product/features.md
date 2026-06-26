# Drawify 功能特性设计

## 设计原则

所有功能特性遵循三条原则：

1. **为 AI 生成优化** — 语法少而精，减少 LLM 的选择空间
2. **机器可读优先** — AST 是一等公民，文本是序列化形式之一
3. **错误可修复** — 每个错误都是结构化的、可操作的

---

## 核心功能

### F1. 多图表类型支持

Drawify 使用统一的语法基础，通过 `diagram` 声明区分图表类型：

| 图表类型 | 关键字 | MVP 阶段 |
|----------|--------|----------|
| 流程图 | `flowchart` | 是 |
| 时序图 | `sequence` | 是 |
| 架构图 | `architecture` | 是 |
| 状态机图 | `state` | 否（P1） |
| ER 图 | `er` | 否（P1） |
| 思维导图 | `mindmap` | 否（P2） |

**语法示例（流程图）：**

```drawify
diagram flowchart {
    entity login "用户登录"
    entity auth "身份验证"
    entity dashboard "仪表盘"

    login -> auth "提交凭证"
    auth -> dashboard "验证通过"
    auth -> login "验证失败"
}
```

### F2. 语义优先的实体定义

实体（节点）只需要声明**是什么**，不需要声明**怎么画**：

```drawify
entity user "用户" {
    type: person
}

entity db "主数据库" {
    type: database
}

entity api "API 网关" {
    type: service
}
```

- `type` 是语义标签，渲染器根据 type 自动选择图标/形状
- Agent 不需要知道 "数据库用圆柱体" 这种视觉映射

### F3. 声明式关系

关系只需要表达**谁和谁有什么关系**：

```drawify
user -> api "发送请求"
api -> db "查询数据"
db --> api "返回结果"    // 虚线 = 响应/回调
```

**箭头语义固定，不允许变体：**

| 符号 | 语义 | 说明 |
|------|------|------|
| `->` | 主动流向 | 调用、发送、流转 |
| `-->` | 被动/响应 | 返回、回调、异步响应 |
| `<->` | 双向 | 双向通信、依赖 |

对比 Mermaid 的 `-->`, `---`, `-.->`, `==>`, `--text-->` 等十余种变体，Drawify 只有 3 种。

### F4. 结构化属性系统

使用 `key: value` 格式声明属性，所有属性都有明确的 schema：

```drawify
entity payment "支付服务" {
    type: service
    status: degraded
    owner: "支付团队"
    sla: "99.9%"
}
```

**属性设计原则：**
- 预定义属性集（`type`, `status`, `owner` 等），Agent 不需要发明属性名
- 自定义属性通过 `meta` 命名空间扩展，避免冲突
- 属性值是强类型的（string、enum、number），渲染器可以校验

### F5. 分组与层次

使用 `group` 关键字表达逻辑分组（子图），最多支持 2 层嵌套：

```drawify
group frontend "前端层" {
    entity web "Web 客户端"
    entity mobile "移动客户端"
}

group backend "后端层" {
    entity api "API 服务"
    entity worker "后台 Worker"
}

web -> api
mobile -> api
api -> worker
```

**限制：**
- 嵌套深度不超过 2 层（避免 LLM 生成深层嵌套结构）
- group 之间不能直接连线（只有 entity 可以）

---

## 错误反馈机制

### F6. 结构化错误

Drawify 的错误不是文本字符串，而是结构化对象：

```json
{
    "code": "E003",
    "severity": "error",
    "message": "关系引用了不存在的实体",
    "location": { "line": 12, "column": 5 },
    "context": {
        "referenced_entity": "payment_db",
        "available_entities": ["user", "api", "db"]
    },
    "suggestion": "请确认实体名拼写，或在图表中定义实体 'payment_db'"
}
```

**错误码体系（初期）：**

| 错误码 | 类型 | 说明 |
|--------|------|------|
| E001 | 语法错误 | 无法解析的语法结构 |
| E002 | 重复 ID | 实体或分组 ID 重复 |
| E003 | 引用缺失 | 关系引用了不存在的实体 |
| E004 | 属性非法 | 属性名或值不符合 schema |
| E005 | 类型不匹配 | 图表类型与使用的结构不兼容 |
| W001 | 孤立实体 | 存在无关系的实体（警告） |
| W002 | 冗余属性 | 属性存在但不影响渲染（警告） |

### F7. 增量修复建议

每次错误返回时附带修复建议，Agent 可以直接应用：

```json
{
    "code": "E003",
    "fix": {
        "action": "add_entity",
        "payload": { "id": "payment_db", "label": "支付数据库", "type": "database" }
    }
}
```

Agent 可以将 fix payload 直接合并到 AST，无需重新生成整段文本。

---

## 语义 Diff 与 Patch

### F8. AST Diff

Drawify 提供语义级别的图表比较：

```
+ entity cache "Redis 缓存" { type: cache }
~ entity api "API 服务" { status: healthy -> degraded }
- entity legacy "遗留系统"
+ relation: api -> cache "查询缓存"
```

**Diff 输出不是文本行比较**，而是结构化的变更列表：
- `+` 新增实体/关系
- `~` 属性变更
- `-` 删除实体/关系

### F9. AST Patch

支持将 Diff 结果应用为补丁，实现图表的程序化修改：

```json
{
    "patches": [
        { "op": "add", "path": "/entities/cache", "value": { "label": "Redis 缓存", "type": "cache" } },
        { "op": "modify", "path": "/entities/api/attributes/status", "value": "degraded" }
    ]
}
```

这使得 Agent 可以：
- 生成一个初版图表
- 根据用户反馈，只 patch 需要修改的部分
- 不需要重新生成整张图

---

## 渲染与输出

### F10. 多格式输出

| 输出格式 | 用途 | 实现方式 |
|----------|------|----------|
| SVG | 网页展示、文档嵌入 | 核心渲染器直接生成 |
| JSON | 前端自定义渲染 | AST 序列化 |
| PNG | 静态图片、社交分享 | SVG → 光栅化 |

### F11. 自动布局

渲染器内置智能布局算法，Agent 无需指定坐标：

- 流程图：基于层次布局（Sugiyama 算法）
- 时序图：线性时间轴布局
- 架构图：力导向 + 约束布局

布局可通过 `layout` 属性微调偏好（不指定坐标）：

```drawify
diagram flowchart {
    layout: top-to-bottom    // 或 left-to-right
    ...
}
```

### F12. 主题系统

支持通过属性控制渲染主题，而非内联样式：

```drawify
diagram flowchart {
    theme: "default"         // 预置主题
    ...
}
```

预置主题（MVP）：
- `default` — 清晰专业风格
- `dark` — 深色背景

---

## 集成能力

### F13. CLI 工具

```bash
# 解析并渲染
drawify render diagram.dfy -o output.svg

# 验证语法
drawify validate diagram.dfy

# 生成 JSON（供前端消费）
drawify export diagram.dfy --format json

# 语义 Diff
drawify diff old.dfy new.dfy
```

### F14. Web API

```
POST /render
Content-Type: application/json

{ "source": "diagram flowchart { ... }", "format": "svg" }

Response: { "output": "<svg>...</svg>", "warnings": [] }
```

```
POST /validate
Content-Type: application/json

{ "source": "diagram flowchart { ... }" }

Response: { "valid": false, "errors": [...] }
```

### F15. WASM 包

```javascript
import { renderSvg, validate } from 'drawify-wasm';

const svg = renderSvg(drawifySource);
const errors = validate(drawifySource);
```

---

## 功能优先级

| 功能 | 优先级 | 说明 |
|------|--------|------|
| F1 多图表类型（流程图） | P0 - MVP | 先做流程图一种 |
| F2 语义实体定义 | P0 - MVP | 核心语法 |
| F3 声明式关系 | P0 - MVP | 核心语法 |
| F4 结构化属性 | P0 - MVP | 核心语法 |
| F5 分组与层次 | P0 - MVP | 核心语法 |
| F6 结构化错误 | P0 - MVP | 核心差异化能力 |
| F10 多格式输出（SVG） | P0 - MVP | 先只做 SVG |
| F11 自动布局 | P0 - MVP | 必须有布局算法 |
| F7 增量修复建议 | P1 | 增强 Agent 体验 |
| F8 AST Diff | P1 | 差异化能力 |
| F9 AST Patch | P1 | 差异化能力 |
| F13 CLI | P1 | MVP 可先做 CLI |
| F14 Web API | P1 | 服务端集成 |
| F15 WASM | P1 | 浏览器集成 |
| F12 主题系统 | P2 | 后期丰富 |
| F1 时序图/架构图 | P1 | MVP 后扩展 |
| F1 状态机/ER图 | P2 | 更多图表类型 |
