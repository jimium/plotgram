# Drawify DSL 写作手册

> 实用指南 | 基于语言规范 v0.3.0

本手册是 Drawify DSL 的实践写作指南，帮助你在不同场景下快速写出正确的 `.dfy` 文件。语法规范见 [language-spec.md](./language-spec.md)。

---

## 1. 快速入门

### 1.1 最小可用图表

```drawify
diagram flowchart {
    title: "Hello World"

    entity a "开始"
    entity b "结束"

    a -> b
}
```

### 1.2 文件骨架

一个 `.dfy` 文件的标准结构：

```drawify
// 文档注释（可选，文件开头连续 // 行）

diagram <类型> {
    title: "<标题>"                    // body 级属性

    config {                           // 可选，集中放置图表属性
        direction: <方向>
        layout: <布局算法>
        edge_routing: <边路由>
        theme: <主题>
        render_style: <笔触皮肤>
        group_frame: <组框架配置>      // [新增] 统一组间排列配置
    }

    // 声明式样式规则（可选）
    node_style <type> { ... }
    edge_style <name> { ... }

    // 分组（可选）
    group <id> "<标签>" { ... }

    // 实体声明（type 写在方括号中，可选）
    entity[<type>] <id> "<标签>" { ... }

    // 关系声明
    <from> -> <to> "<标签>"
}
```

### 1.3 三条核心规则

1. **先声明 entity，再写 relation** — relation 引用的 entity 必须已声明
2. **title 在 body 级，其他图表属性在 config 块** — title 不进 config
3. **ID 用小写+下划线，值用 atom 或字符串** — ID 不允许连字符，atom 值允许连字符和点号

---

## 2. 图表类型选择

### 2.1 六种图表类型

| 类型 | 关键字 | 适用场景 | 默认布局 | 默认边路由 |
| --- | --- | --- | --- | --- |
| 流程图 | `flowchart` | 决策流程、审批流、状态转换 | `flowchart` (sugiyama-v2) | `orthogonal` |
| 时序图 | `sequence` | 交互时序、消息传递 | `sequence` | 无（布局生成） |
| 架构图 | `architecture` | 系统架构、分层服务 | `architecture` | `orthogonal` |
| 状态图 | `state` | 状态机、状态转换 | `state` (circular) | `circular` |
| ER 图 | `er` | 数据模型、表关系 | `er` (sugiyama-v2) | `straight` |
| 思维导图 | `mindmap` | 知识结构、脑图 | `mindmap` | `organic` |

### 2.2 选择建议

- **业务流程** → `flowchart`（有决策分支、开始/结束节点）
- **API 交互** → `sequence`（有参与者、消息往返）
- **微服务架构** → `architecture`（有分层、分组、服务依赖，支持嵌套 group）
- **状态机** → `state`（有初始/终止/选择节点）
- **数据库设计** → `er`（有表、外键、基数关系）
- **知识梳理** → `mindmap`（有树形层级）

---

## 3. Entity 写法

### 3.1 基本形式

```drawify
entity <id> "<显示标签>"
```

- `id`：小写字母 + 数字 + 下划线，如 `user_service`、`db1`
- 显示标签：双引号字符串，如 `"用户服务"`

### 3.2 带 type

`type` 决定渲染形状，是最常用的 entity 属性，使用 `entity[<type>]` 方括号语法在 entity 关键字后直接指定：

```drawify
entity[database] db "用户数据库"
entity[gateway] api "API 网关"
entity[person] user "用户"
```

### 3.3 带状态

```drawify
entity[database] db "主数据库" {
    status: healthy
}

entity[cache] cache "缓存" {
    status: degraded
}
```

`status` 可选值：`healthy`、`degraded`、`down`、`unknown`

### 3.4 带描述和负责人

```drawify
entity[service] auth "认证服务" {
    owner: "安全团队"
    description: "处理用户认证和授权"
}
```

### 3.5 带 semantic 和 icon

```drawify
entity[service] api "API 服务" {
    semantic: auth
    icon: shield
}
```

- `semantic`：驱动图标推断（开放值，如 `auth`、`payment`、`storage`）
- `icon`：直接指定图标名；`icon: none` 表示不显示图标

### 3.6 带 meta 自定义属性

```drawify
entity[service] api "API 服务" {
    meta.version: "2.1.0"
    meta.port: 8080
    meta.protocol: "gRPC"
}
```

meta 属性不参与渲染，仅供程序化消费。

### 3.7 各图表类型的可用 type

| 图表类型 | 可用 type 值 |
| --- | --- |
| `flowchart` | `service`, `database`, `person`, `client`, `queue`, `cache`, `gateway`, `storage`, `external`, `decision`, `process`, `start`, `end` |
| `sequence` | `participant`, `actor`, `boundary`, `control`, `lifeline`, `database` |
| `architecture` | `frontend`, `backend`, `service`, `database`, `gateway`, `cache`, `queue`, `storage`, `external` |
| `state` | `initial`, `state`, `final`, `choice` |
| `mindmap` | `root`, `main`, `branch`, `leaf` |
| `er` | 开放集（任意 atom 值均可） |

---

## 4. Relation 写法

### 4.1 三种箭头

```drawify
a -> b      // 主动流向：调用、发送、推进
a --> b     // 被动/响应：返回、回调、异步响应
a <-> b     // 双向关系：双向通信、数据同步
```

### 4.2 带标签

```drawify
user -> api "发送请求"
api --> user "返回响应"
```

### 4.3 带端点标签

```drawify
a -> b "中间标签" >"目标端" <"源端"
a -> b >"仅目标端"
```

- `>"text"` — 靠近目标端的标签
- `<"text"` — 靠近源端的标签
- 顺序自由，可同时出现

### 4.4 带属性

```drawify
api -> db "查询数据" {
    status: degraded
    line_style: error
}

user -> post "发表" {
    cardinality: "1:N"
}
```

### 4.5 关系属性速查

| 属性 | 类型 | 说明 | 示例 |
| --- | --- | --- | --- |
| `status` | atom | 运行状态 | `status: degraded` |
| `line_style` | atom | 引用 `edge_style` 规则 | `line_style: error` |
| `cardinality` | string | 基数标注（ER 图） | `cardinality: "1:N"` |

### 4.6 关系约束

- entity 必须先声明
- 允许同一对 entity 之间多条关系
- 不允许自环（`a -> a`），除非 entity type 为 `decision`
- group 不能作为关系端点

---

## 5. Group 写法

### 5.1 基本形式

```drawify
group <id> "<标签>" {
    // group 属性（可选）
    layout: <布局>
    border_style: <边框样式>
    color: "<颜色标签>"

    // entity 声明（type 写在方括号中，可选）
    entity[<type>] <id> "<标签>" { ... }

    // 组内 edge 连线（两端 entity 必须都在本 group 内）
    <from> -> <to> "<标签>"

    // 嵌套 group（最多 1 层）
    group <id> "<标签>" { ... }
}
```

### 5.2 分层架构示例

推荐将**组内 edge** 就近写在 group 内部，**跨组 edge** 保留在顶层，使模块边界一目了然：

```drawify
group frontend "前端层" {
    entity[frontend] web "Web 应用"
    entity[frontend] mobile "移动端"
}

group backend "后端层" {
    layout: horizontal
    border_style: dashed

    entity[service] api "API 服务"
    entity[service] worker "Worker"

    // 组内 edge：api -> worker 都在 backend 内
    api -> worker "dispatch"
}

group data "数据层" {
    entity[database] db "主数据库"
    entity[cache] cache "缓存"
}

// 跨组 edge 在顶层声明
web -> api
mobile -> api
api -> db
api -> cache
```

### 5.3 Group 属性

| 属性 | 可选值 | 说明 |
| --- | --- | --- |
| `layout` | `auto`, `horizontal`, `vertical`, `fan-out` | 组内布局（简写 `h`/`v`，别名 `fan_out`/`fanout`） |
| `border_style` | `solid`, `dashed`, `dotted` | 边框线型 |
| `color` | 任意字符串 | 背景色标签 |

### 5.4 Group 约束

- 最大嵌套 2 层（group 内可以有 group，但不可再嵌套）
- group 内可以声明 entity、嵌套 group、group 属性，以及**组内 edge 连线**
- 组内 edge 的**两端端点必须都属于当前 group 的后代 entity**（含直接 entity 和子 group 的 entity）
- 跨 group 的 edge 必须声明在 diagram 顶层
- group 不参与关系连线
- **flowchart 分治路径不支持嵌套 group**：若 flowchart 声明了嵌套 group，子 group 的边界和标签会丢失，其内部节点会被当作顶层 group 的直接成员一起布局。如需嵌套 group，请使用 `diagram architecture`

---

## 6. 图表属性配置

### 6.1 title（body 级属性）

```drawify
diagram flowchart {
    title: "用户认证流程"
    ...
}
```

title 直接写在 body 中，不放在 config 块内。

### 6.2 config 块

其他图表属性集中在 `config` 块中：

```drawify
diagram flowchart {
    title: "用户登录流程"

    config {
        direction: left-to-right
        layout: sugiyama-v2 { friendliness: adjust }
        edge_routing: orthogonal { bundling: 1.0 }
        theme: common.clean-light
        render_style: excalidraw
        group_frame: stack {
            axis: horizontal
            gap: 40
            track: equal
        }
    }

    ...
}
```

### 6.3 属性速查

| 属性 | 类型 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `direction` | atom | 由图表类型 profile 决定 | `top-to-bottom` / `left-to-right` / `radial`；仅 flowchart/er/sugiyama 支持 tb+lr，mindmap 支持 radial+tb+lr；其他布局不支持 direction |
| `layout` | atom/config | 由图表类型决定 | 布局算法，可带配置块；支持 `friendliness: off/diagnose/adjust`（默认 `adjust`） |
| `edge_routing` | atom/config | 由图表类型决定 | 边路由算法，可带配置块；`orthogonal` 支持 `bundling` 启用边捆绑 |
| `theme` | atom | 由 profile 决定 | 主题 ID，如 `common.clean-light`、`common.blueprint`、`mindmap.vivid-branches` |
| `render_style` | atom | `standard` | `standard` / `excalidraw` / `cross-hatch` / `blueprint` / `spatial-clarity` / `neon-glow` / `stipple` |
| `group_frame` | atom/config | 由算法默认决定 | **[推荐]** Group Frame 统一配置块，统一控制组间排列/尺寸/对齐/间距/量化 |
| `group_sizing` | atom | `fit` | 顶层分组宽度策略（`group_frame` sugar，建议用 `group_frame`） |
| `snap` | boolean | `true` | 网格吸附（`group_frame` sugar，建议用 `group_frame` 的 `snap` 选项） |
| `group_arrangement` | atom | `vertical` | group 间排列方向（`group_frame` sugar，建议用 `group_frame: stack { axis: ... }`） |
| `group_gap` | number | `60` | group 间距像素（`group_frame` sugar，建议用 `group_frame` 的 `gap` 选项） |
| `group_align` | atom | `center` | group 间对齐方式（`group_frame` sugar，建议用 `group_frame` 的 `cross` 选项） |

### 6.4 布局算法选项

`layout` 可选值及常用 option：

| 值 | 说明 | 常用 option |
| --- | --- | --- |
| `flowchart` | **流程图专属分层布局（默认）**；共享 sugiyama-v2 引擎 | `group_padding`, `friendliness` |
| `er` | **ER 图专属分层布局**；共享 sugiyama-v2 引擎 | `group_padding`, `friendliness` |
| `state` | **状态图专属布局**；共享 circular 引擎 | `group_padding`, `padding`, `component_gap` |
| `architecture` | **架构图分组分层布局（默认）** | `group_padding`, `padding` |
| `mindmap` | 思维导图布局（默认） | `padding`, `level_gap`, `branch_gap`, `node_gap`, `center_gap` |
| `sequence` | 时序图布局（不支持 edge_routing） | `group_padding`, `node_spacing`, `message_spacing` |
| `sugiyama-v2` | 通用 Sugiyama 分层布局（高级选项） | `group_padding`, `friendliness` |
| `force-directed` | 分组感知力导向布局 | `group_padding`, `padding`, `component_gap` |
| `circular` | 自适应圆形布局 | `group_padding`, `padding`, `component_gap` |

**friendliness 模式：**
- `off`：关闭路由友好度评估
- `diagnose`：仅诊断并报告问题，不调整
- `adjust`（默认）：诊断并自动调整路由

### 6.5 边路由算法选项

`edge_routing` 可选值：

| 值 | 说明 | 常用 option |
| --- | --- | --- |
| `orthogonal` | 正交折线路由（flowchart/architecture 默认） | `slot_pitch`, `channel_margin`, `bundling` |
| `straight` | 直线连接（ER 图默认） | — |
| `bezier` | 贝塞尔曲线路由 | `tension` |
| `spline` | 障碍避让多段样条 | — |
| `circular` | 弧形边路由（state 图默认） | — |
| `organic` | **有机自然曲线**（mindmap 默认） | — |

**边捆绑（Edge Bundling）：**
正交路由支持 `bundling` 选项（0.0~1.0，默认 0.0 关闭），启用后相似路径的边会共享主干段，减少视觉"意大利面"效应：

```drawify
config {
    edge_routing: orthogonal {
        bundling: 1.0    // 完全启用边捆绑
    }
}
```

> **时序图**（`diagram sequence`）不支持 `edge_routing`；消息路径由布局阶段生成。

### 6.6 `group_frame` 统一配置块（推荐）

`group_frame` 是组间宏观几何的统一配置入口，将原分散的 `group_sizing`、`group_arrangement`、`group_gap`、`group_align`、`snap` 整合为一个配置块。旧属性仍可使用（语法糖），新代码推荐直接使用 `group_frame`。

**`stack` 一维堆叠排列（最常用）：**

| 选项 | 类型 | 可选值 | 默认值 | 说明 |
| --- | --- | --- | --- | --- |
| `axis` | atom | `horizontal` / `h`, `vertical` / `v` | 由算法决定 | 堆叠轴方向 |
| `gap` | number | 正数 | 60 (flowchart) / 50 (architecture) | 组间净间距（像素） |
| `track` | atom/number | `fit`, `equal`/`uniform`, 固定数值 | `fit` | 主轴尺寸策略；`equal` 同级等宽/等高 |
| `cross` | atom | `start`/`left`, `center`, `end`/`right`, `stretch` | `center` (flowchart) / `start` (architecture) | 交叉轴对齐方式 |
| `border` | atom | `none`, `shared`/`shared_lines` | `none` (flowchart) / `shared` (architecture) | 边框共线策略 |
| `snap` | boolean/number | `true`, `false`, 步长数值 | `true` (步长 8px) | 像素量化开关/步长 |

**架构图（等宽水平分层）示例：**

```drawify
diagram architecture {
    title: "微服务架构"
    config {
        group_frame: stack {
            axis: horizontal
            gap: 50
            track: equal
            cross: start
            border: shared
        }
    }
    // ...
}
```

**流程图（垂直阶段划分）示例：**

```drawify
diagram flowchart {
    title: "CI/CD 流水线"
    config {
        group_frame: stack {
            axis: vertical
            gap: 80
            cross: center
        }
    }
    // ...
}
```

**泳道图（水平排列）示例：**

```drawify
diagram flowchart {
    title: "订单处理泳道"
    config {
        group_frame: stack {
            axis: horizontal
            gap: 40
            cross: center
        }
    }
    // ...
}
```

---

## 7. 样式系统

### 7.1 三层样式优先级

从低到高：

1. **Theme 主题** — 全局颜色/字体（通过 `theme` 属性选择）
2. **node_style / edge_style 声明** — 按 type 匹配的批量样式
3. **内联 style.* 属性** — 单个 entity/relation 的样式覆盖

### 7.2 node_style 声明

按 entity `type` 批量匹配：

```drawify
node_style service {
    fill: "#E3F2FD"
    stroke: "#1976D2"
    shape: rounded_rect
    stroke_width: 2.0
}

node_style database {
    fill: "#FFF3E0"
    stroke: "#E65100"
    shape: cylinder
}

node_style cache {
    fill: "#E8F5E9"
    stroke: "#2E7D32"
    shape: diamond
}
```

### 7.3 edge_style 声明 + line_style 引用

声明命名边样式，在 relation 中通过 `line_style` 引用：

```drawify
edge_style error {
    stroke: "#C62828"
    stroke_width: 2.5
    dashed: true
}

edge_style success {
    stroke: "#2E7D32"
    stroke_width: 2.0
}

api -> db "查询" { line_style: error }
db --> api "返回" { line_style: success }
```

> **注意：** 在 relation 属性块中使用 `line_style: <name>` 引用，**不是** `edge_style: <name>`。`edge_style` 是关键字，只能用于顶层声明。

### 7.4 内联样式覆盖

在 entity/relation 的属性块中用 `style.*` 覆盖：

```drawify
entity[service] api "API 服务" {
    style.fill: "#C8E6C9"     // 覆盖 node_style service 的 fill
    style.shape: hexagon      // 覆盖 shape
}

api -> db "查询" {
    line_style: error
    style.stroke_width: 3.0   // 覆盖 edge_style error 的 stroke_width
}
```

### 7.5 Entity 样式属性一览

| 属性 | 类型 | 说明 |
| --- | --- | --- |
| `style.fill` | string | 填充色 |
| `style.stroke` | string | 边框色 |
| `style.stroke_width` | number | 线宽 |
| `style.stroke_dasharray` | string | 虚线 pattern |
| `style.shape` | atom | `rect` / `rounded_rect` / `circle` / `diamond` / `cylinder` / `hexagon` / `person` / `stadium` |
| `style.label_weight` | atom | `regular` / `medium` / `bold` |
| `style.width` | number | 布局 hint 宽度 |
| `style.height` | number | 布局 hint 高度 |
| `style.text_fill` | string | 文字颜色 |
| `style.font_size` | number | 字号 |
| `style.font_weight` | atom | 字重 |
| `style.radius` | number | 圆角半径 |
| `style.transform` | string | 变换 |

### 7.6 Relation 样式属性一览

| 属性 | 类型 | 说明 |
| --- | --- | --- |
| `style.stroke` | string | 线条颜色 |
| `style.stroke_width` | number | 线宽 |
| `style.stroke_dasharray` | string | 虚线 pattern |
| `style.dashed` | boolean | 虚线开关 |
| `style.label_color` | string | 标签颜色 |
| `style.text_fill` | string | 文字颜色 |
| `style.font_size` | number | 字号 |
| `style.label_bg` | string | 标签背景色 |
| `style.label_bg_opacity` | number | 标签背景透明度 |
| `style.label_border` | string | 标签边框色 |
| `style.label_border_width` | number | 标签边框宽度 |
| `style.label_border_radius` | number | 标签圆角 |
| `style.label_padding` | number | 标签内边距 |
| `style.label_font_size` | number | 标签字号 |
| `style.label_font_weight` | atom | 标签字重 |
| `style.label_position` | atom | 标签位置 |
| `style.label_rotation` | number | 标签旋转角度 |

---

## 8. 实战配方

### 8.1 流程图：决策分支

```drawify
diagram flowchart {
    title: "用户登录决策"
    config {
        direction: top-to-bottom
        layout: flowchart { friendliness: adjust }
    }

    entity[start] start "开始"
    entity[process] input "输入凭证"
    entity[decision] check "验证凭证"
    entity[end] success "登录成功"
    entity[end] fail "登录失败"

    start -> input
    input -> check
    check -> success "通过"
    check -> fail "拒绝"
}
```

### 8.2 时序图：请求-响应

```drawify
diagram sequence {
    title: "API 调用时序"

    entity[actor] client "客户端"
    entity[participant] server "服务端"
    entity[participant] db "数据库"

    client -> server "请求"
    server -> db "查询"
    db --> server "数据"
    server --> client "响应"
}
```

### 8.3 架构图：分层服务（使用 group_frame）

```drawify
diagram architecture {
    title: "微服务架构"
    config {
        render_style: blueprint
        group_frame: stack {
            axis: horizontal
            track: equal
            gap: 50
            cross: start
            border: shared
        }
        edge_routing: orthogonal { bundling: 1.0 }
    }

    group gateway "网关层" {
        layout: horizontal

        entity[gateway] lb "负载均衡"
        entity[gateway] api "API 网关"
    }

    group service "服务层" {
        layout: horizontal

        entity[service] auth "认证服务"
        entity[service] user "用户服务"
        entity[service] order "订单服务"
    }

    group data "数据层" {
        layout: horizontal

        entity[database] db "主数据库"
        entity[cache] cache "缓存"
        entity[queue] queue "消息队列"
    }

    lb -> api
    api -> auth
    api -> user
    api -> order
    auth -> db
    user -> db
    user -> cache
    order -> queue
}
```

### 8.4 ER 图：表关系

```drawify
diagram er {
    title: "博客数据模型"
    config {
        direction: left-to-right
    }

    entity[database] user "用户表" {
        meta.pk: "id"
        meta.fields: "username\nemail\ncreated_at"
    }

    entity[database] post "文章表" {
        meta.pk: "id"
        meta.fields: "title\ncontent\nuser_id"
    }

    entity[database] comment "评论表" {
        meta.pk: "id"
        meta.fields: "content\npost_id\nuser_id"
    }

    user -> post "发表" { cardinality: "1:N" }
    user -> comment "评论" { cardinality: "1:N" }
    post -> comment "包含" { cardinality: "1:N" }
}
```

### 8.5 状态图：状态机

```drawify
diagram state {
    title: "订单状态机"

    entity[initial] pending "待支付"
    entity[state] paid "已支付"
    entity[state] shipped "已发货"
    entity[state] delivered "已签收"
    entity[final] cancelled "已取消"

    pending -> paid "支付"
    paid -> shipped "发货"
    shipped -> delivered "签收"
    pending -> cancelled "取消"
    paid -> cancelled "退款"
}
```

### 8.6 思维导图

```drawify
diagram mindmap {
    title: "产品知识图谱"
    config {
        direction: left-to-right
    }

    entity[root] product "产品"
    entity[main] design "设计"
    entity[main] dev "开发"
    entity[main] ops "运维"

    entity[branch] ui "UI/UX"
    entity[branch] research "用户研究"

    entity[branch] frontend "前端"
    entity[branch] backend "后端"

    entity[branch] ci "CI/CD"
    entity[branch] monitor "监控"

    product -> design
    product -> dev
    product -> ops
    design -> ui
    design -> research
    dev -> frontend
    dev -> backend
    ops -> ci
    ops -> monitor
}
```

### 8.7 泳道流程图（使用 group_frame）

```drawify
diagram flowchart {
    title: "订单处理泳道"
    config {
        group_frame: stack {
            axis: horizontal
            gap: 40
            cross: center
        }
    }

    group customer "客户" {
        entity[start] order "下单"
        entity[process] pay "支付"
    }
    group warehouse "仓库" {
        entity[process] pick "拣货"
        entity[process] pack "打包"
    }
    group shipping "配送" {
        entity[process] ship "发货"
        entity[end] deliver "送达"
    }

    order -> pay
    pay -> pick
    pick -> pack
    pack -> ship
    ship -> deliver
}
```

### 8.8 带边捆绑的密集流程图

当图中边很多容易交叉时，启用 `bundling` 让相似边共享主干：

```drawify
diagram flowchart {
    title: "微服务调用全景"
    config {
        direction: left-to-right
        edge_routing: orthogonal {
            bundling: 1.0
            slot_pitch: 30
        }
    }

    entity[gateway] gateway "API Gateway"

    group services "业务服务" {
        layout: vertical
        entity[service] user "用户服务"
        entity[service] order "订单服务"
        entity[service] pay "支付服务"
        entity[service] notify "通知服务"
    }

    group data "数据层" {
        layout: vertical
        entity[database] mysql "MySQL"
        entity[cache] redis "Redis"
        entity[queue] mq "消息队列"
    }

    gateway -> user
    gateway -> order
    gateway -> pay
    user -> mysql
    user -> redis
    order -> mysql
    order -> mq
    pay -> mysql
    pay -> mq
    mq -> notify
}
```

---

## 9. 常见错误与避坑

### 9.1 ID 命名错误

```drawify
// ✗ 错误：ID 含连字符
entity api-gateway "API 网关"

// ✓ 正确：用下划线
entity api_gateway "API 网关"
```

### 9.2 title 放错位置

```drawify
// ✗ 错误：title 在 config 块内
diagram flowchart {
    config {
        title: "流程图"
        direction: top-to-bottom
    }
}

// ✓ 正确：title 在 body 级
diagram flowchart {
    title: "流程图"
    config {
        direction: top-to-bottom
    }
}
```

### 9.3 edge_style 用作属性键

```drawify
// ✗ 错误：edge_style 是关键字，不能用作属性键
edge_style error { stroke: "#C62828" }
api -> db { edge_style: error }

// ✓ 正确：用 line_style 引用
edge_style error { stroke: "#C62828" }
api -> db { line_style: error }
```

### 9.4 group 内写 relation

```drawify
// ✗ 错误：relation 不能在 group 内声明
group backend "后端" {
    entity api "API"
    entity db "DB"
    api -> db        // 语法错误
}

// ✓ 正确：relation 在顶层声明
group backend "后端" {
    entity api "API"
    entity db "DB"
}
api -> db            // 在顶层声明
```

### 9.5 group 嵌套过深

```drawify
// ✗ 错误：超过 2 层嵌套
group a "A" {
    group b "B" {
        group c "C" {   // 第三层，报错
            entity x "X"
        }
    }
}

// ✓ 正确：最多 2 层
group a "A" {
    group b "B" {
        entity x "X"
    }
}
```

### 9.6 未声明的 entity 引用

```drawify
// ✗ 错误：api 未声明
user -> api "请求"

// ✓ 正确：先声明再引用
entity api "API 服务"
user -> api "请求"
```

### 9.7 type 值不在当前图表类型范围内

```drawify
// ✗ 错误：sequence 图不支持 type: service
diagram sequence {
    entity[service] api "API"
}

// ✓ 正确：用 sequence 支持的 type
diagram sequence {
    entity[participant] api "API"
}
```

### 9.8 direction 值不在枚举内或布局不支持

```drawify
// ✗ 错误：from_center 不是合法值
config {
    direction: from_center
}

// ✗ 错误：sequence 不支持 direction
diagram sequence {
    config {
        direction: left-to-right
    }
}

// ✓ 正确：flowchart 支持 direction
config {
    direction: top-to-bottom
}
```

### 9.9 重复声明 config 块

```drawify
// ✗ 错误：config 块只能出现一次
diagram flowchart {
    config { direction: top-to-bottom }
    config { theme: common.clean-light }
}

// ✓ 正确：合并到一个 config 块
diagram flowchart {
    config {
        direction: top-to-bottom
        theme: common.clean-light
    }
}
```

### 9.10 group 作为关系端点

```drawify
// ✗ 错误：group 不能作为 relation 端点
group backend "后端" {
    entity api "API"
}
backend -> api

// ✓ 正确：用 entity ID 作为端点
group backend "后端" {
    entity api "API"
}
api -> db
```

### 9.11 主题前缀错误

```drawify
// ✗ 错误：旧前缀 builtin. 已弃用
config {
    theme: builtin.clean-light
}

// ✓ 正确：使用 common. 前缀
config {
    theme: common.clean-light
}
```

---

## 10. 编写清单

写 `.dfy` 文件时，按以下清单检查：

- [ ] 文件以 `diagram <类型> {` 开头，以 `}` 结尾
- [ ] `title` 在 body 级（不在 config 块内）
- [ ] `config` 块最多出现一次
- [ ] 所有 entity ID 小写 + 下划线，不含连字符
- [ ] 所有 entity 在 relation 引用前已声明
- [ ] `type` 值在当前图表类型的允许范围内
- [ ] `direction` 值为 `top-to-bottom` / `left-to-right` / `radial`，且当前布局支持 direction
- [ ] `status` 值为 `healthy` / `degraded` / `down` / `unknown`
- [ ] `border_style` 值为 `solid` / `dashed` / `dotted`
- [ ] group 嵌套不超过 2 层
- [ ] relation 不在 group 内声明
- [ ] group 不作为 relation 端点
- [ ] 引用边样式用 `line_style: <name>`（不是 `edge_style: <name>`）
- [ ] `node_style` 的 selector 是当前图表类型支持的 entity type
- [ ] 主题 ID 使用 `common.` 前缀（如 `common.clean-light`）
- [ ] 组间排列优先使用 `group_frame` 配置块（而非旧的 `group_sizing`/`group_arrangement` 等）
- [ ] 边密集的图考虑启用 `edge_routing: orthogonal { bundling: 1.0 }` 减少交叉
