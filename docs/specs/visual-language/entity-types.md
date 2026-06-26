# 实体类型 (entity type) 标准

> 版本：0.3.0 | 状态：与实现同步

本文档定义 Drawify 中 `entity[type]` 的**图表内结构角色**、**全局语义标签**与视觉形状规则。图表级写作规范见 [diagrams/](./diagrams/) 目录；语法约束见 [language-spec.md](../language-spec.md)。

---

## 1. 工作原理

```
diagram 类型  +  entity type   →  形状 + theme 配色 + 布局规则
entity semantic（可选）         →  图标推断（全局词表）
entity icon（可选）             →  显式图标 / icon: none 压制推断
```

1. 用户在 `entity` 声明中通过方括号标注 `type`（`entity[type] id "label"`），或省略方括号使用图表默认 type。`type` 也可在属性块中通过 `type: xxx` 声明（推荐使用方括号语法糖）。
2. 引擎按 `diagram` 类型查 profile 闭集；非法 `type` 报错。
3. theme cascade 按 **原始 `type`** 查 `entity_types` 样式（不做归一化）。
4. 可选 `semantic` 驱动内侧图标推断；可选 `icon` 显式覆盖或 `icon: none` 关闭。

**无 `type` 时的默认值**（`apply_profile_defaults`）：

| 图表类型 | 默认 type |
|----------|-----------|
| `flowchart` | `process` |
| `sequence` | `participant` |
| `architecture` | `service` |
| `state` | `state` |
| `er` | 不补默认（表框语义） |
| `mindmap` | `branch` |

---

## 2. 各图表规范 type 列表

每个图表只允许下列 type（闭集校验）。**禁止**写其他图表的 type 或已废弃的跨图别名。

### 2.1 流程图 (`flowchart`)

`start`, `end`, `process`, `decision`, `service`, `database`, `person`, `client`, `gateway`, `queue`, `cache`, `storage`, `external`

### 2.2 时序图 (`sequence`)

`participant`, `actor`, `boundary`, `control`, `lifeline`, `database`

### 2.3 架构图 (`architecture`)

`frontend`, `backend`, `service`, `database`, `gateway`, `cache`, `queue`, `storage`, `external`

### 2.4 状态图 (`state`)

`initial`, `state`, `final`, `choice`

### 2.5 思维导图 (`mindmap`)

`root`, `main`, `branch`, `leaf`

### 2.6 ER 图 (`er`)

不限制 `type` 闭集；推荐 `type: database`。

---

## 3. 跨图表适用矩阵

图例：**✓** 该图表规范 type | **—** 不支持（校验报错） | **\*** ER 不限制

| type | flowchart | sequence | architecture | state | er | mindmap |
|------|-----------|----------|--------------|-------|-----|---------|
| **流程语义** |
| `start` | ✓ | — | — | — | * | — |
| `end` | ✓ | — | — | — | * | — |
| `process` | ✓ | — | — | — | * | — |
| `decision` | ✓ | — | — | — | * | — |
| **组件 / 基础设施** |
| `service` | ✓ | — | ✓ | — | * | — |
| `database` | ✓ | ✓ | ✓ | — | * | — |
| `person` | ✓ | — | — | — | * | — |
| `client` | ✓ | — | — | — | * | — |
| `gateway` | ✓ | — | ✓ | — | * | — |
| `queue` | ✓ | — | ✓ | — | * | — |
| `cache` | ✓ | — | ✓ | — | * | — |
| `storage` | ✓ | — | ✓ | — | * | — |
| `external` | ✓ | — | ✓ | — | * | — |
| **架构专用** |
| `frontend` | — | — | ✓ | — | * | — |
| `backend` | — | — | ✓ | — | * | — |
| **时序专用** |
| `participant` | — | ✓ | — | — | * | — |
| `actor` | — | ✓ | — | — | * | — |
| `boundary` | — | ✓ | — | — | * | — |
| `control` | — | ✓ | — | — | * | — |
| `lifeline` | — | ✓ | — | — | * | — |
| **状态专用** |
| `initial` | — | — | — | ✓ | * | — |
| `state` | — | — | — | ✓ | * | — |
| `final` | — | — | — | ✓ | * | — |
| `choice` | — | — | — | ✓ | * | — |
| **思维导图专用** |
| `root` | — | — | — | — | * | ✓ |
| `main` | — | — | — | — | * | ✓ |
| `branch` | — | — | — | — | * | ✓ |
| `leaf` | — | — | — | — | * | ✓ |

---

## 4. 类型目录（节选）

### 4.1 流程语义类（主图 `flowchart`）

| type | 语义 | 视觉（flowchart） |
|------|------|-------------------|
| `start` | 入口 | 体育场形，绿色系 |
| `end` | 终止 | 体育场形，红色系 |
| `process` | 普通步骤（**默认**） | 圆角矩形，灰色系 |
| `decision` | 条件分支（**唯一允许自环**） | 菱形，黄色系 |

### 4.2 组件与基础设施（`flowchart` / `architecture`）

| type | 语义 | 视觉要点 |
|------|------|----------|
| `service` | 微服务、可调用组件 | architecture 默认；圆角矩形 |
| `database` | 持久化存储 | 圆柱体（flowchart/arch） |
| `gateway` | API 网关 | architecture 为六边形 |
| `queue` | 消息队列 | architecture 倾斜矩形 |
| `cache` | 缓存层 | architecture 菱形 |
| `frontend` | 用户侧入口 | architecture 矩形，蓝色系 |
| `person` / `client` | 人 / 客户端 | **仅 flowchart**；架构图用 `frontend` |

### 4.3 时序 / 状态 / 思维导图专用 type

见 §2.2–§2.5；各图只使用本图规范名，例如状态图写 `initial` 而非 `start`，时序图写 `actor` 而非 `person`。

---

## 5. 全局语义 `semantic` 与图标 `icon`

与 `type` **正交**，跨图表共用同一词表（见 `icons/registry.rs`）。

| 字段 | 作用 | 校验 |
|------|------|------|
| `semantic` | 表达技术/角色语义，默认开启推断 → 内侧图标 | 未知值 **Warning**（W301） |
| `icon` | 显式指定图标 id；`icon: none` 强制不渲染 | 未知值 **Warning**（W302） |

**解析优先级**：`icon: none` → `icon: <id>` → `semantic` 推断 → 无图标。

**示例**（架构图）：

```drawify
entity[database] db "订单库" {
    semantic: mysql
}

entity[frontend] web "Web 端" {
    semantic: browser
}

entity[cache] cache "会话" {
    semantic: redis
    icon: none
}
```

- `type` 决定形状与 theme；`semantic` 决定图标（若形状兼容）。
- 圆柱体、人形、六边形等部分形状不渲染内侧图标（静默回退为居中标签）。

常用 `semantic` 与组件对应：

| semantic | 典型场景 |
|----------|----------|
| `mysql`, `postgres`, `redis`, `mongodb` | 数据存储 |
| `kafka`, `rabbitmq` | 消息队列 |
| `prometheus`, `grafana`, `loki`, `jaeger` | 可观测性 |
| `nginx`, `ingress` | 网关 / 入口 |
| `user`, `browser`, `mobile` | 用户 / 客户端 |
| `flink`, `spark`, `clickhouse` | 数据计算 |

完整词表以代码注册表为准。

---

## 6. 辅助属性 `status`

与 `type` / `semantic` 正交的**运行状态**标注：

| 值 | 语义 |
|----|------|
| `healthy` | 正常运行 |
| `degraded` | 降级/部分可用 |
| `down` | 不可用 |
| `unknown` | 状态未知 |

---

## 7. 选型速查

```
要画什么？
├─ 步骤/审批/分支        → flowchart：start / process / decision / end
├─ 消息时序              → sequence：participant / actor / boundary / control / database
├─ 系统组件/依赖         → architecture：frontend / service / database / gateway / ...
├─ 状态迁移              → state：initial / state / choice / final
├─ 表关系                → er：database + relation.cardinality
└─ 知识树/计划           → mindmap：root / main / branch / leaf

具体技术栈 / 品牌图标     → semantic: mysql / kafka / redis / ...
精确指定或关闭图标        → icon: mysql  或  icon: none
```

**三条原则**：

1. **`type` 必须用当前图表的规范枚举** — 校验严格，无别名、无隐式转换。
2. **`type` 表达结构角色，`semantic` 表达技术语义** — 箭头表达行为。
3. **一张图一种主要语义** — 架构图不写 `start`/`process`；跨图概念用 `semantic` 而非错用 `type`。

---

## 8. 视觉形状对照

| 形状 | 使用的 type（典型） |
|------|---------------------|
| 体育场形 | `start`, `end`（flowchart） |
| 圆角矩形 | `process`, `service`, `state`, `branch`, … |
| 矩形 | `frontend`, `participant`, ER 表框 |
| 圆柱体 | `database` |
| 菱形 | `decision`, `cache`, `choice` |
| 六边形 | `gateway`（architecture） |
| 人形 | `person`（flowchart） |
| 圆形 | `initial`, `final`（state）, `root`（mindmap） |
| 虚线边框矩形 | `external` |

图形风格由 `graphic-style` 与样式方案叠加，见 [style-sheet-spec.md](../style-sheet-spec.md)。

---

## 参见

- [视觉语言总览](./README.md)
- [各图表类型详解](./diagrams/)
- [language-spec.md §5.3](../language-spec.md) — 语法层属性
- `crates/drawify-core/src/profile/mod.rs` — 各图表 `entity_types` 与默认 type
- `crates/drawify-core/src/icons/registry.rs` — 全局 `semantic` 词表
