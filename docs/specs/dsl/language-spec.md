# Drawify 语言语法与语义规范

> 版本：0.3.0 | 状态：与实现同步

本文档定义 Drawify 语言的完整语法规则和语义约束。所有语法设计决策均以**AI Agent 生成友好度**为首要考量。

---

## 1. 设计约束

### 1.1 语法设计原则

| 原则     | 说明            | 约束                       |
| ------ | ------------- | ------------------------ |
| 最小语法集  | 尽可能少的关键字和语法构造 | MVP 阶段关键字不超过 10 个        |
| 单一表达   | 每种语义只有一种合法写法  | 禁止语法变体（如多种箭头风格）          |
| 显式优于隐式 | 所有行为必须显式声明    | 禁止隐式类型转换、隐式默认值           |
| 有限嵌套   | 限制语法嵌套深度      | 最大嵌套 2 层（group 内 entity） |
| 可预测性   | 给定相同输入，解析结果唯一 | 禁止上下文相关的语法解析             |

### 1.2 对 LLM 的优化策略

- **Atom + String 分工** — 开放扩展的语义值用 atom（无引号），布局/标题等用引号字符串；合法性由执行层后置校验
- **固定结构** — entity/relation/group 的语法结构固定，不允许变形
- **无歧义分隔** — 使用 `{}`、`"`、`:` 等明确分隔符，避免空格/缩进敏感
- **即时可验证** — 每行语法独立可验证，不需要全文件上下文

---

## 2. 语法总览

### 2.1 文件结构

一个 `.dfy` 文件由一个可选的**文档注释块**和一个 `diagram` 声明组成：

```
<file> ::= [<doc_comment>] <diagram_declaration>
```

```drawify
// 文档注释块（可选，连续 // 行）
// 第二行

diagram flowchart {
    // 图表内容
}
```

### 2.2 图表类型关键字

```
<diagram_type> ::= "flowchart" | "sequence" | "architecture"
                 | "state"     | "er"       | "mindmap"
```

### 2.3 图表体（Diagram Body）

图表体由以下元素组成，顺序自由：

```
<diagram_body> ::= (<diagram_attribute> | <config_block>
                  | <entity_declaration> | <relation_declaration>
                  | <group_declaration> | <style_decl>)*
```

**约束：**

- 同一属性不能重复声明
- `config` 块最多出现一次

---

## 3. 标识符与字面量

### 3.1 标识符（Identifier）

```
<identifier> ::= [a-z][a-z0-9_]*
```

**规则：**

- 必须以小写字母开头
- 仅包含小写字母、数字、下划线
- 长度限制：1-64 个字符
- 同一作用域内唯一
- 不允许连字符 `-` 和点号 `.`

**示例：**

- `user` ✓
- `api_gateway` ✓
- `node1` ✓
- `User` ✗（大写）
- `1st_node` ✗（数字开头）
- `api-gateway` ✗（不允许连字符）

**设计理由：** 限制为小写 + 下划线，消除 LLM 在命名风格上的选择空间（不需要纠结 camelCase vs snake_case vs kebab-case）。

### 3.2 Atom 字面量（Atom Literal）

用于开放扩展的语义标记（如 `type`、`status`、`direction`），**无需引号**：

```
<atom> ::= <lowercase-letter> { <lowercase-letter> | <digit> | "_" | "-" | "." }
```

**规则：**

- 必须以小写字母开头
- 仅含小写字母、数字、下划线 `_`、连字符 `-`、点号 `.`
- 不允许首尾或连续的点号（如 `.foo`、`foo.`、`foo..bar`）
- 长度限制：1-64 个字符
- 解析为 AST 的 `AttributeValue::String(TextValue { quoted: false })`（开放字符串，非封闭枚举）
- 是否合法由执行层（profile / schema / 注册表）后置校验

**示例：**

- `service` ✓
- `sugiyama-v2` ✓
- `cross-link` ✓
- `common.clean-light` ✓（主题 ID 等分层命名）
- `inspired.dracula` ✓
- `"service"` ✓（引号形式也接受，语义等价）
- `Service` ✗（大写）
- `1st` ✗（数字开头）
- `.builtin` ✗（点号开头）

**与 Identifier 的区别：** Identifier 用于 entity id、属性键等，不允许连字符与点号；Atom 用于属性值，允许连字符与点号分段。

### 3.3 字符串字面量（String Literal）

```
<string> ::= '"' <character>* '"'
```

**规则：**

- 用双引号包裹
- 支持转义：`\"`, `\\`, `\n`
- 不允许换行（单行字符串）
- 最大长度：256 个字符

### 3.4 数值字面量（Number Literal）

```
<number> ::= [0-9]+ ("." [0-9]+)?
```

### 3.5 布尔字面量

```
<boolean> ::= "true" | "false"
```

### 3.6 算法配置块（Algorithm Config）

用于 `layout`、`edge_routing` 等需要算法名 + 可选参数的属性值：

```
<algorithm_config> ::= <atom> [ "{" <option_pair> { <option_pair> } "}" ]
<option_pair>      ::= <identifier> ":" <attribute_value>
```

- 可简写为单个 atom（如 `layout: sugiyama-v2`）
- 也可带 `{ }` 配置块（如 `layout: sugiyama-v2 { group_padding: 20 }`）
- 配置块内的 option key **不由语言规范枚举**；各算法自行定义并在布局阶段校验，未知 key 产生警告

---

## 4. Diagram 声明

### 4.1 语法

```
<diagram_declaration> ::= "diagram" <diagram_type> "{" <diagram_body> "}"
```

### 4.2 图表属性（Diagram Attributes）

图表级属性用于控制全局渲染行为。属性可以出现在图表体的任意位置（body 级属性），也可以集中在 `config` 块中：

```
<diagram_attribute> ::= <attribute_key> ":" <attribute_value>
<config_block>      ::= "config" "{" <diagram_attribute>* "}"
```

**`title` 属性**：作为 body 级属性直接写在图表体中（不放在 `config` 块内）。其他图表属性可放在 `config` 块中或 body 级别。

```drawify
diagram flowchart {
    title: "用户登录流程"

    config {
        direction: left-to-right
        layout: sugiyama-v2
        edge_routing: orthogonal
        theme: common.clean-light
        render_style: excalidraw
    }

    entity login "登录"
    entity auth "认证"
    login -> auth
}
```

### 4.3 图表属性一览

| 属性名            | 类型               | 可选值                                                       | 默认值                   | 说明                              |
| -------------- | ---------------- | --------------------------------------------------------- | --------------------- | ------------------------------- |
| `title`        | string           | 任意字符串                                                     | 无                     | 图表标题（body 级属性，不进 config 块）       |
| `direction`    | atom             | `top-to-bottom`, `left-to-right`, `radial`                | 由图表类型 profile 决定 | 布局方向偏好；仅支持 direction 的布局生效（见 §4.5）                          |
| `layout`       | atom 或配置块        | 见布局算法表                                                     | 由图表类型决定 | 布局算法及可选参数；支持 `friendliness` 选项：`off` \| `diagnose` \| `adjust`（默认 `adjust`） |
| `edge_routing` | atom 或配置块        | 见边路由算法表 | 由图表类型决定 | 边路由算法及可选参数；`orthogonal` 支持 `bundling: true` 启用边捆绑 |
| `theme`        | atom             | 内置主题 ID（见主题系统规范），如 `common.clean-light`、`common.blueprint`、`mindmap.vivid-branches` | 由图表类型 profile 决定      | 颜色/字体主题（对应 StyleSheet 的 `id` 字段） |
| `render_style`| atom             | `standard`, `excalidraw`, `cross-hatch`, `blueprint`, `spatial-clarity`, `neon-glow`, `stipple` | `standard` | 笔触皮肤（与 theme 分工：theme 管颜色，render_style 管绘制风格） |
| `group_frame`  | atom 或配置块        | `stack { ... }` \| `matrix { ... }` | 由算法默认决定 | **[新增]** Group Frame 统一配置块，统一控制组间排列/尺寸/对齐/间距/量化；旧属性 `group_sizing`/`group_arrangement`/`group_gap`/`group_align`/`snap` 保留为语法糖（见 §4.6） |
| `group_sizing` | atom             | `fit`, `uniform` | `fit` | 顶层分组宽度策略（`group_frame` sugar，建议直接使用 `group_frame`）；适用于含 group 的布局 |
| `snap`         | boolean          | `true`, `false` | `true` | 网格吸附开关（`group_frame` sugar，建议直接使用 `group_frame` 的 `snap` 选项） |
| `group_arrangement` | atom         | `vertical`, `horizontal` | `vertical` | group 间排列方向（`group_frame` sugar，建议直接使用 `group_frame: stack { axis: ... }`） |
| `group_gap`    | number           | 正数            | `60`                  | group 间距（像素）（`group_frame` sugar，建议直接使用 `group_frame` 的 `gap` 选项） |
| `group_align`  | atom             | `center`, `left` \| `start` | `center`             | group 间对齐方式（`group_frame` sugar，建议直接使用 `group_frame` 的 `cross` 选项） |

### 4.4 布局算法

`layout` 可选值：

| 值                   | 说明                         |
| ------------------- | -------------------------- |
| `flowchart`         | **流程图专属分层布局（默认）**；共享 sugiyama-v2 引擎；option: `group_padding`, `friendliness` |
| `er`                | **ER 图专属分层布局**；共享 sugiyama-v2 引擎；option: `group_padding`, `friendliness` |
| `state`             | **状态图专属布局**；共享 circular 引擎；option: `group_padding`, `padding`, `component_gap` |
| `architecture`      | **架构图分组分层布局（默认）**（原 `architecture-v2`）；option: `group_padding`, `padding` |
| `mindmap`           | 思维导图布局（默认）；option: `padding`, `level_gap`, `branch_gap`, `node_gap`, `center_gap` |
| `sequence`          | 时序图布局（布局阶段直接产出边几何，不支持 edge_routing）；option: `group_padding`, `node_spacing`, `message_spacing` |
| `sugiyama-v2`       | 通用 Sugiyama 分层布局（高级选项）；option: `group_padding`, `friendliness` |
| `sugiyama`          | Sugiyama 分层算法（旧版兼容）；option: `group_padding` |
| `force-directed`    | 分组感知力导向布局（FR + 分组引力）；option: `group_padding`, `padding`, `component_gap` |
| `circular`          | 自适应圆形布局（单圆 / 多连通分量）；option: `group_padding`, `padding`, `component_gap` |

### 4.5 边路由算法

`edge_routing` 可选值：

| 值                   | 说明              |
| ------------------- | --------------- |
| `orthogonal`        | 正交折线路由（flowchart/architecture 默认；options: `slot_pitch`, `channel_margin`, `bundling`） |
| `straight`          | 直线连接（ER 图默认） |
| `bezier`            | 贝塞尔曲线路由；options: `tension` |
| `spline`            | 障碍避让多段样条 |
| `circular`          | 弧形边路由（配合 `layout: circular`/`layout: state`；state 图默认） |
| `organic`           | **有机自然曲线**（mindmap 默认） |

> **时序图**（`diagram sequence`）不支持 `edge_routing`；消息路径由 `layout: sequence` 在布局阶段生成。显式声明 `edge_routing` 将报错。

> **注意：** `direction`、`theme`、`render_style`、`group_sizing` 使用 atom 字面量（无引号）。`layout`、`edge_routing` 可为 atom 或 `algo { options }` 配置块；配置块内的 option key 由各算法自行定义。多词算法/路由名用连字符分段，如 `sugiyama-v2`；主题 ID 含点号，如 `common.clean-light`、`mindmap.vivid-branches`。

**`direction` 布局支持矩阵：**

| 布局 / 图表类型 | 支持的 direction 值 | 不支持 direction |
| --- | --- | --- |
| `flowchart` / `er` / `sugiyama` / `sugiyama-v2` / `architecture` | `top-to-bottom`, `left-to-right` | `radial` |
| `mindmap` | `radial`, `top-to-bottom`, `left-to-right` | — |
| `sequence` / `state` / `force-directed` / `circular` | — | 不支持 `direction`，声明将报错 |

### 4.6 `group_frame` 统一配置块（新增）

`group_frame` 是组间宏观几何的统一配置入口，将原分散的 `group_sizing`、`group_arrangement`、`group_gap`、`group_align`、`snap` 整合为一个配置块。旧属性仍可使用（语法糖），新代码推荐直接使用 `group_frame`。

**语法：**

```
<group_frame_config> ::= <group_frame_arrangement>
<group_frame_arrangement> ::= "stack" "{" <stack_option>* "}" | "matrix" "{" <matrix_option>* "}"
```

**`stack` 选项（一维堆叠排列）：**

| 选项 | 类型 | 可选值 | 默认值 | 说明 |
| --- | --- | --- | --- | --- |
| `axis` | atom | `horizontal` / `h`, `vertical` / `v` | 由算法决定 | 堆叠轴方向 |
| `gap` | number | 正数 | 60 (flowchart) / 50 (architecture) | 组间净间距（像素） |
| `track` | atom/number | `fit`, `equal`/`uniform`, 固定数值 | `fit` | 主轴 track 尺寸策略；`equal` 同级等宽/等高 |
| `cross` | atom | `start`/`left`, `center`, `end`/`right`, `stretch` | `center` (flowchart) / `start` (architecture) | 交叉轴对齐方式 |
| `border` | atom | `none`, `shared`/`shared_lines` | `none` (flowchart) / `shared` (architecture) | 边框共线策略 |
| `snap` | boolean/number | `true`, `false`, 步长数值 | `true` (步长 8px) | 像素量化开关/步长 |

**`matrix` 选项（二期，二维网格排列）：**

| 选项 | 类型 | 可选值 | 默认值 | 说明 |
| --- | --- | --- | --- | --- |
| `rows` | number | 正整数 | 自动推断 | 网格行数 |
| `cols` | number | 正整数 | 自动推断（接近正方形） | 网格列数 |
| `gap` | number | 正数 | 60 | 组间间距 |
| `track` | atom/number | `fit`, `equal`/`uniform` | `fit` | 单元格尺寸策略 |
| `cross` | atom | `start`, `center`, `end` | `center` | 单元格内对齐 |
| `snap` | boolean/number |  | `true` | 像素量化 |

**示例：**

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

### 4.7 `group_sizing` 说明（兼容保留）

| 值 | 行为 |
| --- | --- |
| `fit` | 每个顶层 group 宽度贴合组内内容（默认） |
| `uniform` | 所有顶层 group 拉齐到最宽者；组内节点在拉宽后的框内**水平居中**，适合流水线/阶段类架构图 |

```drawify
diagram architecture {
    title: "数据仓 ETL 处理架构"
    config {
        group_sizing: uniform
    }
    ...
}
```

### 4.7 `group_arrangement` / `group_gap` / `group_align` 说明

这三个属性控制 **flowchart 含 group 时的分治布局**（每个 group 独立布局后，再按指定方向堆叠合并）。仅对 `diagram flowchart` 且存在 group 时生效；无 group 或其他图表类型声明这些属性不会报错但也不生效。

> **嵌套 group 限制**：flowchart 分治路径当前不支持嵌套 group。若 flowchart 声明了嵌套 group，子 group 的边界和标签会丢失，其内部节点会被当作顶层 group 的直接成员一起布局。如需嵌套 group 支持，请使用 `diagram architecture`（`architecture-v2` 布局已递归处理嵌套）。

| 属性 | 值 | 行为 |
| --- | --- | --- |
| `group_arrangement: vertical` | 默认 | group 自上而下排列（阶段划分图） |
| `group_arrangement: horizontal` | | group 从左到右排列（泳道图） |
| `group_gap` | 正数（默认 `60`） | 相邻 group 边界之间的像素间距 |
| `group_align: center` | 默认 | `vertical` 时各 group 水平居中对齐；`horizontal` 时各 group 垂直居中对齐 |
| `group_align: left` | | `vertical` 时各 group 左对齐；`horizontal` 时各 group 顶部对齐 |

```drawify
// 泳道图示例
diagram flowchart {
    title: "订单处理泳道"
    config {
        group_arrangement: horizontal
        group_gap: 40
        group_align: center
    }

    group customer "客户" {
        entity[start] order "下单"
        entity[process] pay "支付"
    }
    group warehouse "仓库" {
        entity[process] pick "拣货"
        entity[process] pack "打包"
    }

    order -> pay
    pay -> pick
    pick -> pack
}
```

### 4.8 约束

- 图表属性不是必需的
- 同一属性不能重复声明
- `config` 块最多出现一次

---

## 5. Entity 声明

### 5.1 语法

```
<entity_declaration> ::= "entity" ["[" <type_atom> "]"] <identifier> <string> [<attribute_block>]
```

- `[<type_atom>]` — **可选的类型标注**（语法糖）。`type` 是 entity 最高频的属性，使用方括号直接跟在 `entity` 关键字后指定，省去 `{ type: xxx }` 的写法
- `identifier` — 实体的程序化 ID（用于关系引用）
- `string` — 实体的显示标签（人类可读）
- `attribute_block` — 可选的属性块（用于 type 以外的其他属性）

**语法糖说明：**

- `entity[gateway] api "API 网关"` 等价于 `entity api "API 网关" { type: gateway }`
- 若 entity 不需要指定 type（使用图表默认 type），省略方括号：`entity login "用户登录"`
- 若同时指定 type 和其他属性，type 在方括号中，其他属性在 `{ }` 中：`entity[database] db "主库" { status: healthy }`
- 不允许同时在 `[ ]` 和 `{ }` 中指定 type（重复声明报错）

### 5.2 属性块（Attribute Block）

```
<attribute_block> ::= "{" <attribute>* "}"
<attribute>       ::= <attribute_key> ":" <attribute_value>
<attribute_key>   ::= <identifier> | "style." <identifier> | "meta." <identifier>
<attribute_value> ::= <string> | <atom> | <number> | <boolean> | <algorithm_config>
```

**命名空间路由：**

| 前缀       | 命名空间       | 说明                          |
| ---------- | ------------ | ----------------------------- |
| 无前缀     | `standard`   | 预定义语义属性（type, status 等） |
| `style.`   | `style`      | 内联视觉样式（fill, stroke 等） |
| `meta.`    | `meta`       | 自定义元数据（渲染器忽略）      |

**值类型分工：**

| 类别     | 语法       | 示例属性                                                                                                  |
| ------ | -------- | ----------------------------------------------------------------------------------------------------- |
| Atom   | 无引号 atom | `type`, `status`, `semantic`, `icon`, `direction`, `layout`, `edge_routing`, `render_style`, `theme`, `group_sizing`, `border_style`, group `layout` |
| String | 引号字符串    | `title`, `owner`, `description`, `cardinality`, group `color`                                                               |
| Number | 数值       | `style.stroke_width`, `style.width`, `style.height` 等                                                  |
| Boolean | 布尔      | `snap`, `style.dashed` 等                                                                               |

### 5.3 预定义属性 Schema

#### 通用属性（所有 entity 可用）

| 属性名           | 类型     | 说明   | 示例值                                                                               |
| ------------- | ------ | ---- | --------------------------------------------------------------------------------- |
| `type`        | atom   | 实体类型 | `service`, `database`, `person`, `queue`, `cache`, `gateway`, `client`, `storage` |
| `status`      | atom   | 运行状态 | `healthy`, `degraded`, `down`, `unknown`                                          |
| `semantic`    | atom   | 语义标记 | 驱动图标推断，如 `auth`, `payment`（开放值）                                                |
| `icon`        | atom   | 图标标记 | `none` 表示无图标；其他值匹配图标库（开放值）                                                  |
| `owner`       | string | 负责方  | `"平台团队"`                                                                          |
| `description` | string | 详细描述 | `"处理用户认证"`                                                                        |

#### `status` 枚举

| 值         | 语义     |
| --------- | ------ |
| `healthy` | 正常运行   |
| `degraded`| 降级     |
| `down`    | 宕机     |
| `unknown` | 未知状态   |

#### `type` 枚举完整列表

> 各图表类型允许的 `type` 子集、别名归一化与使用场景见 [实体类型标准](../visual-language/entity-types.md)。

| 值            | 语义       | 推荐渲染形状 | 适用图表类型 |
| ------------ | -------- | ------ | ---------- |
| `service`    | 微服务/后端服务 | 圆角矩形   | flowchart, architecture |
| `database`   | 数据库      | 圆柱体    | flowchart, sequence, architecture, er |
| `person`     | 人/用户角色   | 人形图标   | flowchart |
| `client`     | 客户端应用    | 矩形     | flowchart |
| `queue`      | 消息队列     | 队列形状   | flowchart, architecture |
| `cache`      | 缓存       | 菱形     | flowchart, architecture |
| `gateway`    | 网关       | 六边形    | flowchart, architecture |
| `storage`    | 文件/对象存储  | 文件夹形状  | flowchart, architecture |
| `external`   | 外部系统     | 虚线边框矩形 | flowchart, architecture |
| `decision`   | 决策节点     | 菱形     | flowchart |
| `process`    | 处理过程     | 矩形     | flowchart |
| `start`      | 流程起点     | 圆形     | flowchart |
| `end`        | 流程终点     | 双圆     | flowchart |
| `participant`| 时序图参与者  | 矩形     | sequence |
| `actor`      | 外部角色     | 人形图标   | sequence |
| `boundary`   | 边界对象     | 矩形     | sequence |
| `control`    | 控制对象     | 矩形     | sequence |
| `lifeline`   | 通用实体     | 矩形     | sequence |
| `frontend`   | 前端层     | 矩形     | architecture |
| `backend`    | 后端层     | 矩形     | architecture |
| `initial`    | 初始状态     | 圆形     | state |
| `state`      | 中间状态     | 圆角矩形   | state |
| `final`      | 终止状态     | 双圆     | state |
| `choice`     | 选择节点     | 菱形     | state |
| `root`       | 根节点     | 圆形     | mindmap |
| `main`       | 主分支     | 矩形     | mindmap |
| `branch`     | 分支节点    | 矩形     | mindmap |
| `leaf`       | 叶节点     | 矩形     | mindmap |

> **ER 图**（`diagram er`）不限制 `type` 值，接受任意 atom（开放集）。

### 5.4 自定义属性（meta 命名空间）

不在预定义 Schema 中的属性，必须以 `meta.` 为前缀：

```drawify
entity api "API 服务" {
    type: service
    meta.version: "2.1.0"
    meta.port: 8080
    meta.protocol: "gRPC"
}
```

**约束：**

- `meta.` 前缀后的 key 遵循 identifier 规则
- meta 属性的 value 类型不受 Schema 约束
- 渲染器忽略 meta 属性（仅供程序化消费）

### 5.5 Entity 示例

**最简形式（使用默认 type，无额外属性）：**

```drawify
entity login "用户登录"
```

**指定 type（语法糖形式，推荐）：**

```drawify
entity[gateway] api "API 网关"
entity[database] db "主数据库"
entity[start] begin "开始"
```

**指定 type 并带其他属性：**

```drawify
entity[database] db "主数据库" {
    status: healthy
    owner: "DBA 团队"
}
```

---

## 6. Relation 声明

### 6.1 语法

```
<relation_declaration> ::= <identifier> <arrow> <identifier>
                            [<string>] [<label_marker>*] [<attribute_block>]
<label_marker>         ::= ">" <string>    (head_label，靠近目标端)
                          | "<" <string>    (tail_label，靠近源端)
```

- 第一个 `identifier` — 源实体 ID
- `arrow` — 箭头类型（见下表）
- 第二个 `identifier` — 目标实体 ID
- `string` — 可选的关系标签（中间标签）
- `label_marker` — 可选的端点标签（`>"head"` 靠近目标，`<"tail"` 靠近源），顺序自由，可同时出现
- `attribute_block` — 可选的属性块

### 6.2 箭头类型（仅 3 种）

| 箭头    | 语义      | 使用场景              |
| ----- | ------- | ----------------- |
| `->`  | 主动流向    | 调用、发送请求、数据流转、流程推进 |
| `-->` | 被动/响应流向 | 返回结果、回调、异步响应      |
| `<->` | 双向关系    | 双向通信、依赖、数据同步      |

**设计理由：** 对比 Mermaid 的 `-->`, `---`, `-.->`, `==>`, `--text-->` 等 10+ 种变体，Drawify 只有 3 种固定语义。LLM 不需要在样式层面做选择。

### 6.3 Relation 属性

| 属性名           | 类型     | 可选值                                | 说明           |
| -------------- | ------ | ---------------------------------- | ------------ |
| `status`       | atom   | `healthy`, `degraded`, `down`, `unknown` | 运行状态（与 entity 共享枚举） |
| `line_style`   | atom   | 开放值；引用已声明的 `edge_style` 规则名        | 边线样式引用       |
| `cardinality`  | string | 任意字符串，如 `"1:N"`, `"0..1"`          | 基数标注（ER 图常用） |

### 6.4 Relation 示例

```drawify
// 最简形式
user -> api

// 带中间标签
user -> api "发送请求"

// 带端点标签
a -> b "mid" >"H" <"T"        // 中间标签 "mid"，目标端 "H"，源端 "T"

// 仅端点标签（无中间标签）
a -> b >"only head"

// 带属性
api -> db "查询数据" {
    status: degraded
    meta.latency: "200ms"
}

// 引用边样式规则
api -> db "查询" { line_style: error }

// ER 图基数标注
user -> post "发表" {
    cardinality: "1:N"
}
```

### 6.5 `line_style` 与 `edge_style` 声明

Relation 通过 `line_style: <name>` 引用已声明的 `edge_style` 规则：

```drawify
edge_style error {
    stroke: "#C62828"
    dashed: true
}

api -> db "查询" { line_style: error }
```

展开时，将 `edge_style error` 中定义的全部属性写入 `relation.attributes.style`（内联 `style.*` 仍优先）。

> **注意：** `edge_style` 是词法关键字，用于顶层声明（`edge_style <name> { ... }`），**不能**作为属性块中的 key。在 relation 属性块中引用边样式规则使用 `line_style: <name>`。

| 元素                | `style` 含义                            | 命名空间                                |
| ----------------- | ------------------------------------- | ----------------------------------- |
| Group             | 边框线型：`solid` / `dashed` / `dotted`    | `attributes.standard["border_style"]` |
| Relation          | 引用边样式规则：`line_style: error`           | `attributes.standard["line_style"]` |
| Entity / Relation | 内联视觉属性：`style.fill`, `style.stroke` 等 | `attributes.style`                  |

### 6.6 语义约束

- 关系两端的 entity 必须已经声明（前置声明要求）
- 允许同一对 entity 之间存在多条关系（不同方向或不同标签）
- 不允许自环关系（`a -> a`），除非 entity 的 type 为 `decision`
- Group 不能作为关系的端点

---

## 7. Group 声明

### 7.1 语法

```
<group_declaration> ::= "group" <identifier> <string> "{" <group_body> "}"
<group_body>        ::= (<entity_declaration> | <group_declaration> | <group_attribute> | <relation_declaration>)*
<group_attribute>   ::= <identifier> ":" <attribute_value>
```

### 7.2 约束

- **最大嵌套深度：2 层**（group 内可以有 group，但内层 group 不可再嵌套）
- group 内可以包含 entity、嵌套 group、group 属性、以及 **edge 连线（relation）**
- group 内 edge 的**两端端点必须都属于当前 group 的后代 entity**（含直接 entity 和子 group 的 entity）；跨 group 的连线必须声明在 diagram 顶层
- group 内的 entity ID 在全局唯一（不是 group 内唯一）
- group 自身不参与关系连线
- group 内声明的 edge 与顶层声明的 edge 语义完全等价，最终都进入 `diagram.relations` 列表

### 7.3 Group 属性

| 属性名           | 类型     | 可选值                                | 说明                                |
| -------------- | ------ | ---------------------------------- | --------------------------------- |
| `layout`       | atom   | `auto`, `horizontal`, `vertical`, `fan-out` | 组内节点布局；简写 `h` / `v`；别名 `fan_out`、`fanout`。**仅 `architecture-v2` 两阶段布局读取** |
| `border_style` | atom   | `solid`, `dashed`, `dotted`        | 边框样式                              |
| `color`        | string | 任意字符串，如 `"blue"`, `"red"`          | 分组背景色标签                           |

```drawify
group process "数据计算层" {
    layout: fan-out

    entity[queue] kafka "消息队列(Kafka)"
    entity[service] spark "批处理(Spark)"
    entity[service] flink "流计算(Flink)"

    // 组内 edge：两端 entity 都在本 group 内
    kafka -> spark "batch consume"
    kafka -> flink "stream consume"
}

group storage "数据存储层" {
    layout: vertical

    entity[database] hive "数仓(Hive)"
    entity[database] clickhouse "OLAP 引擎"
}
```

与图级 `group_sizing: uniform` 配合时，各层外框等宽，组内内容水平居中，形成整齐的阶段条带。

```drawify
group backend "后端层" {
    border_style: dashed

    entity[service] api "API 服务"
    entity[service] worker "Worker"

    api -> worker "dispatch job"
}
```

### 7.4 组内 Edge 与跨组 Edge

**组内 edge**（推荐写在 group 内）：两端都属于同一个 group 的后代 entity。这类连线表达模块内部的数据流，就近声明提升可读性。

**跨组 edge**（必须写在 diagram 顶层）：两端分属不同 group。顶层只保留跨模块的连线，使图的骨架一目了然。

```drawify
group frontend "前端" {
    entity web "Web"
    entity mobile "Mobile"
}

group backend "后端" {
    entity api "API"
    entity[database] db "Database"

    // 组内 edge 写在 group 内
    api -> db "query"
}

// 跨组 edge 写在顶层
web -> api
mobile -> api
```

---

## 8. 声明式样式规则

### 8.1 语法

```
<style_decl>      ::= <node_style_decl> | <edge_style_decl>
<node_style_decl> ::= "node_style" <identifier> "{" <style_property>* "}"
<edge_style_decl> ::= "edge_style" <identifier> "{" <style_property>* "}"
<style_property>  ::= <identifier> ":" <attribute_value>
```

### 8.2 示例

```drawify
diagram flowchart {
    title: "用户认证流程"
    config {
        direction: top-to-bottom
    }

    // 声明式样式规则：所有 type=service 的节点
    node_style service {
        fill: "#E3F2FD"
        stroke: "#1976D2"
        shape: rounded_rect
        stroke_width: 2.0
    }

    // 声明式样式规则：所有 type=database 的节点
    node_style database {
        fill: "#FFF3E0"
        stroke: "#E65100"
        shape: cylinder
    }

    // 声明式样式规则：名为 error 的边样式
    edge_style error {
        stroke: "#C62828"
        stroke_width: 2.5
        dashed: true
    }

    // 内联样式覆盖（优先级高于声明式规则）
    entity[service] api "API 服务" {
        style.fill: "#C8E6C9"    // 覆盖 node_style service 的 fill
    }

    entity[database] db "用户数据库"

    entity[cache] cache "Token 缓存" {
        style.shape: diamond      // 只覆盖 shape，其余从 StyleSheet entity_types 展开
    }

    api -> db "查询数据" { line_style: error }
    db --> api "返回结果"
}
```

### 8.3 选择器类型

| 选择器      | 语法                           | 匹配规则                                     |
| -------- | ---------------------------- | ---------------------------------------- |
| `ByType` | `node_style service { ... }` | 匹配所有 `type` 属性等于 `service` 的 entity      |
| `ByName` | `edge_style error { ... }`   | 被 relation 上 `line_style: error` 显式引用时展开 |

**不做组合选择器**（如 `[type=service AND status=degraded]`），避免 DSL 走向 CSS 选择器的复杂度。更复杂的场景交给 StyleSheet JSON。

与 `node_style` 不同，`edge_style` **不会自动应用到所有边**，必须在 relation 属性块中显式写 `line_style: <name>`。这是有意设计：边的样式差异大，批量隐式匹配容易误伤。

### 8.4 语法约束

- `node_style` / `edge_style` 声明可以出现在 diagram body 的任何位置（不限于最前面）
- 同名声明不允许重复（`node_style service` 只能出现一次）
- `node_style` 的 identifier 必须是当前 DiagramType profile 支持的 entity type 值
- `edge_style` 的 identifier 是自定义名称，遵循 identifier 规则
- relation 上使用 `line_style: <name>` 引用边样式规则（不是 `edge_style: <name>`，`edge_style` 是关键字不能用作属性键）

---

## 9. 注释

### 9.1 文档注释（Doc Comment）

文件开头的连续 `//` 行被捕获为**文档注释**，存入 `Diagram.doc_comment`：

```drawify
// 用户认证流程图
// 作者：平台团队

diagram flowchart {
    ...
}
```

**规则：**

- 必须出现在文件最前面（允许前导空白）
- 连续的 `//` 行组成一个文档注释块
- 空行中断文档注释块
- 文档注释不进入 token 流，不影响语法解析

### 9.2 行注释

```
<comment> ::= "//" <any_character>* <newline>
```

- 仅支持单行注释（`//`）
- 不支持块注释（`/* */`）—— 减少语法复杂度
- 行注释在解析时被丢弃，不进入 AST

```drawify
// 这是一个注释
entity api "API 服务"   // 行尾注释也允许
```

---

## 10. 空白与缩进

### 10.1 规则

- Drawify 是**空白不敏感**的语言
- 缩进仅为人类可读，不影响解析
- 元素之间以换行符分隔
- 多个空白字符等价于一个空格

### 10.2 Token 分隔

| Token 类型 | 分隔方式   |
| -------- | ------ |
| 关键字之间    | 空格或换行  |
| `{` `}`  | 空格或换行  |
| `:`      | 两侧空格可选 |
| `"`      | 紧贴内容   |

---

## 11. 完整语法 BNF

```
<file>                  ::= [<doc_comment>] <diagram_declaration>
<doc_comment>           ::= ("//" [^\n]* "\n")+

<diagram_declaration>   ::= "diagram" <diagram_type> "{" <diagram_body> "}"
<diagram_type>          ::= "flowchart" | "sequence" | "architecture"
                          | "state" | "er" | "mindmap"
<diagram_body>          ::= (<diagram_attribute> | <config_block>
                          | <entity_declaration> | <relation_declaration>
                          | <group_declaration> | <style_decl>)*

<config_block>          ::= "config" "{" <diagram_attribute>* "}"
<diagram_attribute>     ::= <identifier> ":" <attribute_value>

<entity_declaration>    ::= "entity" ["[" <atom> "]"] <identifier> <string> [<attribute_block>]
<attribute_block>       ::= "{" <attribute>* "}"
<attribute>             ::= <namespaced_key> ":" <attribute_value>
<namespaced_key>        ::= <identifier> ("." <identifier>)?

<relation_declaration>  ::= <identifier> <arrow> <identifier>
                            [<string>] [<label_marker>*] [<attribute_block>]
<arrow>                 ::= "->" | "-->" | "<->"
<label_marker>          ::= ">" <string> | "<" <string>

<group_declaration>     ::= "group" <identifier> <string>
                            "{" <group_body> "}"
<group_body>            ::= (<entity_declaration> | <group_declaration>
                          | <group_attribute>)*
<group_attribute>       ::= <identifier> ":" <attribute_value>

<style_decl>            ::= <node_style_decl> | <edge_style_decl>
<node_style_decl>       ::= "node_style" <identifier> "{" <style_property>* "}"
<edge_style_decl>       ::= "edge_style" <identifier> "{" <style_property>* "}"
<style_property>        ::= <identifier> ":" <attribute_value>

<attribute_value>       ::= <string> | <number> | <boolean> | <atom>
                          | <algorithm_config>
<algorithm_config>      ::= <atom> ["{" <option_pair>* "}"]
<option_pair>           ::= <identifier> ":" <attribute_value>

<identifier>            ::= [a-z][a-z0-9_]*
<atom>                  ::= [a-z][a-z0-9_.-]*
<string>                ::= '"' <character>* '"'
<number>                ::= [0-9]+ ("." [0-9]+)?
<boolean>               ::= "true" | "false"
<comment>               ::= "//" .* \n
```

---

## 12. 关键字保留字

以下标识符为保留字，不可用作 entity/group ID：

```
diagram, entity, group, relation,
flowchart, sequence, architecture, state, er, mindmap,
true, false, meta
```

以下为词法关键字，同样不可用作标识符：

```
node_style, edge_style, config
```

---

## 13. 语义约束汇总

以下约束在语法正确的基础上进一步校验：

| 编号  | 约束                                                        | 错误码  |
| --- | --------------------------------------------------------- | ---- |
| S01 | 文件有且仅有一个 diagram 声明                                       | E001 |
| S02 | entity ID 在全局唯一                                           | E002 |
| S03 | group ID 在全局唯一                                            | E002 |
| S04 | entity ID 与 group ID 不重复                                  | E002 |
| S05 | relation 引用的 entity 必须已声明                                 | E003 |
| S06 | 属性名在预定义 Schema 内，或以 `meta.` 开头                            | E004 |
| S07 | 枚举属性值必须在合法枚举列表中                                           | E004 |
| S08 | group 嵌套深度不超过 2 层                                         | E005 |
| S09 | group 不能作为 relation 的端点                                   | E005 |
| S10 | 不允许自环 relation（除 type=decision 外）                         | W003 |
| S11 | 存在无 relation 的 entity 时发出警告                               | W001 |
| S12 | diagram 体内无任何声明时发出警告                                      | W004 |
| S13 | 同名 `node_style` / `edge_style` 不允许重复声明                    | E014 |
| S14 | `node_style` 的 selector 必须是当前 DiagramType 支持的 entity type | W006 |
| S15 | `line_style` 引用名必须对应已声明的 `edge_style` 规则                  | W007 |
| S16 | `config` 块最多出现一次                                          | E005 |
| S17 | `direction` 仅在支持 direction 的布局中有效；在不支持的布局中声明将报错            | E004 |

---

## 14. 完整示例

```drawify
// 用户认证流程
diagram flowchart {
    title: "用户认证流程"
    config {
        direction: top-to-bottom
        layout: sugiyama-v2
        edge_routing: orthogonal
        theme: common.clean-light
        render_style: standard
    }

    // 声明式样式
    node_style service {
        fill: "#E3F2FD"
        stroke: "#1976D2"
        shape: rounded_rect
    }

    // 实体声明
    entity[client] client "移动客户端"

    entity[gateway] gateway "API 网关" {
        status: healthy
    }

    entity[service] auth "认证服务" {
        owner: "安全团队"
    }

    entity[database] db "用户数据库"

    entity[cache] cache "Token 缓存"

    // 关系声明
    client -> gateway "HTTPS 请求"
    gateway -> auth "转发认证请求"
    auth -> db "查询用户信息"
    db --> auth "返回用户记录"
    auth -> cache "存储 Token"
    cache --> auth "返回缓存结果"
    auth --> gateway "认证结果"
    gateway --> client "响应"
}
```

---

## 附录 A：语法对照（Mermaid → Drawify）

| Mermaid             | Drawify                                                      | 说明           |
| ------------------- | ------------------------------------------------------------ | ------------ |
| `graph TD`          | `diagram flowchart { config { direction: top-to-bottom } }` | 图表声明         |
| `A[节点名]`            | `entity a "节点名"`                                             | 节点声明         |
| `A((圆形))`           | `entity[start] a "圆形"`                                         | 形状通过 type 表达 |
| `A --> B`           | `a -> b`                                                     | 关系           |
| `A --文本--> B`       | `a -> b "文本"`                                                | 带标签关系        |
| `A -.-> B`          | `a --> b`                                                    | 虚线 → 被动流向    |
| `subgraph name`     | `group name "name" { }`                                      | 子图/分组        |
| `style A fill:#f9f` | `node_style service { fill: "#f9f" }` 或 `style.fill: "#f9f"` | 声明式规则或内联样式   |

---

## 附录 B：属性速查表

### Diagram 属性

| 属性 | 值类型 | 位置 | 说明 |
| --- | --- | --- | --- |
| `title` | string | body 级 | 图表标题 |
| `direction` | atom (enum) | body / config | `top-to-bottom` \| `left-to-right` \| `radial`；仅支持 direction 的布局生效 |
| `layout` | atom / config | body / config | 布局算法 |
| `edge_routing` | atom / config | body / config | 边路由算法 |
| `theme` | atom | body / config | 主题 ID |
| `render_style` | atom | body / config | 笔触皮肤 |
| `group_sizing` | atom (enum) | body / config | `fit` \| `uniform` |
| `snap` | boolean | body / config | 网格吸附开关 |
| `group_arrangement` | atom (enum) | body / config | `vertical` \| `horizontal`（仅 flowchart 含 group 时生效） |
| `group_gap` | number | body / config | group 间距像素（默认 `60`） |
| `group_align` | atom (enum) | body / config | `center` \| `left`（仅 flowchart 含 group 时生效） |

### Entity 属性

| 属性 | 值类型 | 说明 |
| --- | --- | --- |
| `type` | atom (profile-narrowed) | 实体类型 |
| `status` | atom (enum) | `healthy` \| `degraded` \| `down` \| `unknown` |
| `semantic` | atom | 语义标记（开放） |
| `icon` | atom | 图标标记（开放） |
| `owner` | string | 负责方 |
| `description` | string | 详细描述 |

### Group 属性

| 属性 | 值类型 | 说明 |
| --- | --- | --- |
| `layout` | atom (enum) | `auto` \| `horizontal` \| `vertical` \| `fan-out` |
| `border_style` | atom (enum) | `solid` \| `dashed` \| `dotted` |
| `color` | string | 背景色标签 |

### Relation 属性

| 属性 | 值类型 | 说明 |
| --- | --- | --- |
| `status` | atom (enum) | `healthy` \| `degraded` \| `down` \| `unknown` |
| `line_style` | atom | 边样式规则引用 |
| `cardinality` | string | 基数标注 |

### Entity 样式属性（`style.*`）

| 属性 | 类型 | 说明 |
| --- | --- | --- |
| `style.fill` | string | 填充色 |
| `style.stroke` | string | 边框色 |
| `style.stroke_width` | number | 线宽 |
| `style.stroke_dasharray` | string | 虚线 pattern |
| `style.shape` | atom | 节点形状：`rect`, `rounded_rect`, `circle`, `diamond`, `cylinder`, `hexagon`, `person`, `stadium` |
| `style.label_weight` | atom | 字重：`regular`, `medium`, `bold` |
| `style.width` | number | 布局 hint 宽度 |
| `style.height` | number | 布局 hint 高度 |
| `style.text_fill` | string | 文字颜色 |
| `style.font_size` | number | 字号 |
| `style.font_weight` | atom | 字重 |
| `style.radius` | number | 圆角半径 |
| `style.transform` | string | 变换 |

### Relation 样式属性（`style.*`）

| 属性 | 类型 | 说明 |
| --- | --- | --- |
| `style.stroke` | string | 线条颜色 |
| `style.stroke_width` | number | 线宽 |
| `style.stroke_dasharray` | string | 虚线 pattern |
| `style.dashed` | boolean | 简写虚线开关 |
| `style.label_color` | string | 边标签颜色 |
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
