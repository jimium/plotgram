# Plotgram DSL 规范

> 版本：2.0-draft  
> 状态：语法契约草案（相对 [`language-spec.md`](language-spec.md) 的瘦身重设计）  
> 定稿选择：`kind` + `: shape`；无声明式样式；ER 内容扩展另文；自环默认禁、profile 可开

---

## 1. 语法总览

### 1.1 文件结构

```
<file> ::= [<doc_comment>] <diagram_declaration>
```

一个 `.pgm` 文件由可选的文档注释块和一个 `diagram` 声明组成：

```plotgram
// 文档注释（可选）

diagram flowchart {
    // 图表内容
}
```

### 1.2 图表类型

```
<diagram_type> ::= "flowchart" | "sequence" | "architecture"
                 | "state"     | "er"       | "mindmap"
```

图表类型是 DSL 层的 **profile 预设**。解析后展开为默认的 `layout` + `edge_routing` 及图种约束；**布局/路由引擎不接收 diagram type**，只认算法名与参数（见 §8）。

### 1.3 图表体

```
<diagram_body> ::= (<diagram_attribute>
                  | <node_declaration>
                  | <relation_declaration>
                  | <group_declaration>)*
```

- 元素顺序自由
- 同一 diagram 属性不可重复声明
- **无**顶层 `node_style` / `edge_style` / 其它声明式样式（v2 仅内联 `style.*`）

---

## 2. 标识符与字面量

### 2.1 Identifier

```
<identifier> ::= [a-z][a-z0-9_]*
```

- 小写字母开头，仅含小写字母、数字、下划线
- 长度 1–64
- 不允许连字符 `-` 和点号 `.`
- 用于：node id、group id、属性键、算法配置 option 键

### 2.2 Atom

```
<atom> ::= [a-z][a-z0-9_.-]*
```

- 小写字母开头，可含 `-` 和 `.`（不允许首尾或连续点号）
- 长度 1–64
- 无需引号；引号形式语义等价
- 用于：属性值（`kind`、`status`、算法名、主题 ID、形状名等）
- 合法性由引擎 / profile 后置校验，DSL 不枚举开放值

### 2.3 String

```
<string> ::= '"' <character>* '"'
```

- 双引号包裹，单行，最大 256 字符
- 转义：`\"` `\\` `\n`

### 2.4 Number

```
<number> ::= [0-9]+ ("." [0-9]+)?
```

### 2.5 Boolean

```
<boolean> ::= "true" | "false"
```

### 2.6 属性值

```
<attribute_value> ::= <string> | <atom> | <number> | <boolean> | <algorithm_config>
```

属性键：

```
<attribute_key> ::= <identifier>
                  | "style." <identifier>
                  | "meta." <identifier>
```

| 前缀 | 命名空间 | 说明 |
|------|----------|------|
| 无前缀 | standard | 语义属性（`kind`、`status`、`icon` 等） |
| `style.` | style | 内联视觉样式（`fill`、`stroke` 等） |
| `meta.` | meta | 自定义元数据（渲染器忽略） |

### 2.7 算法配置块

```
<algorithm_config> ::= <atom> ["{" <option_pair>* "}"]
<option_pair>      ::= <identifier> ":" <attribute_value>
```

- 简写：`layout: hierarchical`
- 带参数：`layout: hierarchical { direction: top-to-bottom }`
- `{ }` 内为自由 map，DSL 不枚举字段；各算法自行校验，未知 key 警告

---

## 3. 注释

仅支持单行注释：

```
<comment> ::= "//" [^\n]*
```

### 3.1 文档注释

`diagram` 关键字出现之前的连续 `//` 行为文档注释（可选），存入 AST：

```plotgram
// 用户认证流程
// 作者：平台团队

diagram flowchart {
    ...
}
```

- 必须从文件第一行开始（允许前导空白）
- 空行中断文档注释块
- 仅一组，不可重复

### 3.2 行注释

图表体内任意位置可使用 `//` 行注释，解析时丢弃：

```plotgram
node api "API 服务"   // 行尾注释
// 独立行注释
```

不支持块注释（`/* */`）。

---

## 4. Diagram 声明

### 4.1 语法

```
<diagram_declaration> ::= "diagram" <diagram_type> "{" <diagram_body> "}"
<diagram_attribute>   ::= <attribute_key> ":" <attribute_value>
```

### 4.2 图表属性（固定 5 个）

| 属性 | 类型 | 说明 |
|------|------|------|
| `title` | string | 图表标题 |
| `layout` | algorithm_config | 布局算法及参数 |
| `edge_routing` | algorithm_config | 边路由算法及参数；可与布局内建路由二选一或配合（见引擎注册表） |
| `theme` | atom | 颜色/字体主题 |
| `render_style` | atom | 笔触皮肤 |

- 所有属性可选；未声明时由 diagram type profile 提供默认值
- 同一属性不可重复声明
- 未识别的 diagram 级 key 产生警告，忽略

### 4.3 示例

```plotgram
diagram flowchart {
    title: "用户登录流程"
    layout: hierarchical { direction: top-to-bottom }
    edge_routing: orthogonal
    theme: common.clean-light
    render_style: standard

    node login "登录"
    node auth "认证"
    login -> auth
}
```

---

## 5. Node 声明

### 5.1 语法

```
<node_declaration> ::= "node" <identifier> [<string>] [":" <atom>] [<attribute_block>]
```

| 部分 | 含义 |
|------|------|
| `identifier` | 节点 ID（边引用） |
| `string` | 可选显示标签；省略时为无文字纯形状 |
| `: <atom>` | 可选**形状**（渲染几何）；覆盖 profile 默认 |
| `attribute_block` | 可选属性块 |

### 5.2 语义身份与形状

| 维度 | 写法 | 职责 |
|------|------|------|
| **kind** | 属性块 `kind: <atom>` | 语义身份（开放集）；驱动图标推断、profile 默认形状、图种约束 |
| **shape** | `: <atom>` 后缀 | 渲染形状；显式写出时覆盖 kind 推断的默认形状 |

- 未写 `kind`：由 profile 提供图种默认 kind（若有）
- 未写 `: shape`：由 `kind` + profile 推断形状；再缺省则 `rect`
- 形状封闭集（渲染器对未识别值回退 `rect`）：

```
rect, rounded_rect, circle, diamond, cylinder,
hexagon, stadium, person, parallelogram
```

`kind` 的合法子集、别名与图标映射由 profile / 视觉语言文档定义，**不**写进本语法规范。

### 5.3 属性块

```
<attribute_block> ::= "{" <attribute>* "}"
<attribute>       ::= <attribute_key> ":" <attribute_value>
```

属性为自由 map，DSL 不枚举字段。常见用法：

| key | 示例值 | 说明 |
|-----|--------|------|
| `kind` | `service`, `database`, `decision` | 语义身份 |
| `status` | `healthy`, `degraded`, `down` | 运行状态 |
| `icon` | `none`, 图标名 | 显式图标（可覆盖 kind 推断） |
| `style.fill` | `"#E3F2FD"` | 内联样式 |
| `meta.*` | 任意 | 自定义元数据（渲染器忽略） |

**内容扩展（ER 字段列表等）**：本草案**不**引入 `field "..."` 等非 `key: value` 语句。图种专用内容模型另文定义；在此之前勿在示例中使用。

### 5.4 示例

```plotgram
node login "用户登录"
node db "用户数据库" : cylinder { kind: database }
node api "API 服务" { kind: service, status: healthy }
node gw "网关" : hexagon { kind: gateway, style.fill: "#FFF3E0" }
node spacer : circle
```

---

## 6. Group 声明

### 6.1 语法

```
<group_declaration> ::= "group" <identifier> <string> "{" <group_body> "}"
<group_body>        ::= (<node_declaration>
                       | <relation_declaration>
                       | <group_declaration>
                       | <group_attribute>)*
<group_attribute>   ::= <attribute_key> ":" <attribute_value>
```

- `identifier` — group ID
- `string` — 显示标签（必填）
- body 可含：node、边、嵌套 group、以及与 diagram 属性同形的裸 `key: value`（组级属性）

组级属性**不**使用 `{ }` 包裹；与 node 属性块区分：node 的属性必须在节点声明尾部的 `{ }` 内。

### 6.2 规则

- 嵌套深度 DSL 不限制
- 内部边的两端必须都属于当前 group 的后代 node；跨组边写在顶层（或共同祖先 group）
- group ID 与 node ID 全局不重复
- group 不能作为边的端点

### 6.3 属性

常见 key：`layout`（组内布局 hint，非强制写者）、`style.*`、`meta.*`。引擎不认识的 key 忽略。

```plotgram
group compute "计算层" {
    layout: horizontal

    node spark "Spark"
    node flink "Flink"
}
```

### 6.4 示例

```plotgram
group frontend "前端" {
    node web "Web"
    node mobile "Mobile"
}

group backend "后端" {
    node api "API" { kind: service }
    node db "数据库" : cylinder { kind: database }

    api -> db "query"
}

web -> api
mobile -> api
```

---

## 7. Edge 声明

### 7.1 语法

```
<relation_declaration> ::= <identifier> <arrow> <identifier>
                           [<string>] [<label_marker>*] [<attribute_block>]
<arrow>                ::= "->" | "-->" | "<->"
<label_marker>         ::= ">" <string> | "<" <string>
```

### 7.2 箭头（仅 3 种）

| 箭头 | 语义 |
|------|------|
| `->` | 主动流向 |
| `-->` | 响应/返回 |
| `<->` | 双向 |

### 7.3 标签

- 中间标签：紧跟箭头后的 string
- 端点标签：`>"head"` 靠近目标，`<"tail"` 靠近源

### 7.4 示例

```plotgram
user -> api "请求"
api --> user "响应"
a <-> b "同步"
api -> db "查询" >"1" <"N"
api -> cache { status: degraded }
```

### 7.5 规则

- 两端必须是已声明的 **node**（不能是 group）
- 允许同一对 node 多条边
- **自环**（`a -> a`）：默认禁止；flowchart / state 等 profile 可显式允许（见 §8）

---

## 8. Profile 展开与引擎边界

```
.pgm
  → parse（AST 可保留 diagram_type，供诊断 / profile）
  → profile expand（默认 layout / edge_routing、kind 默认、自环策略、图种约束）
  → LayoutContract（算法名 + 参数 + 图模型）
  → layout / routing 引擎
```

- 引擎入口**不**按图名分支（禁图名特判）；差异只来自 contract / profile 已展开的字段
- 默认算法与约束表由引擎注册表维护；本规范只要求「有 profile、可展开」
- 示意（非封闭承诺，以实现注册表为准）：

| diagram_type | 默认 layout（示意） | 默认 edge_routing（示意） | 自环 |
|--------------|---------------------|---------------------------|------|
| flowchart | hierarchical | **None**（布局内建正交） | 可允许 |
| architecture | hierarchical | **None**（布局内建正交） | 禁止 |
| state | hierarchical 或 circular | **None**（依布局内建） | 可允许 |
| sequence | sequence | **None**（布局自带边几何） | 禁止 |
| mindmap | tree | **None**（依布局） | 禁止 |
| er | circular 等 | **None**（依布局） | 禁止 |

显式写 `edge_routing: orthogonal` 表示节点冻结后的**独立**路由器，与内建正交不是同一条路径（见 [`model-boundary.md`](../../design/model-boundary.md)）。

图种专用结构（时序消息语义、思维导图树、ER 字段表等）**另文**；本草案仅保证统一的 node / edge / group 骨架。

---

## 9. 样式

v2 **仅**支持内联 `style.*`（写在 node / edge / group 属性中）。

- 不提供 `node_style` / `edge_style` 顶层声明
- 不提供边上的 `line_style` 规则引用
- 批量主题交给 `theme` / `render_style` / 外部 StyleSheet（若有），不进本 DSL

```plotgram
node api "API" { kind: service, style.fill: "#E3F2FD", style.stroke: "#1976D2" }
api -> db "查询" { style.stroke: "#C62828", style.dashed: true }
```

---

## 10. 关键字（保留字）

不可用作 identifier：

```
diagram, node, group,
flowchart, sequence, architecture, state, er, mindmap,
true, false
```

箭头 token（`->` `-->` `<->`）与标签标记（`>` `<` 后接 string）不是 identifier。

---

## 11. 完整语法 BNF（汇总）

```
<file>                 ::= [<doc_comment>] <diagram_declaration>

<doc_comment>          ::= <comment_line>+        // 文件首、连续 //，空行中断
<comment_line>         ::= "//" [^\n]*

<diagram_declaration>  ::= "diagram" <diagram_type> "{" <diagram_body> "}"
<diagram_type>         ::= "flowchart" | "sequence" | "architecture"
                         | "state" | "er" | "mindmap"
<diagram_body>         ::= (<diagram_attribute>
                          | <node_declaration>
                          | <relation_declaration>
                          | <group_declaration>)*
<diagram_attribute>    ::= <attribute_key> ":" <attribute_value>

<node_declaration>     ::= "node" <identifier> [<string>] [":" <atom>] [<attribute_block>]
<group_declaration>    ::= "group" <identifier> <string> "{" <group_body> "}"
<group_body>           ::= (<node_declaration> | <relation_declaration>
                          | <group_declaration> | <group_attribute>)*
<group_attribute>      ::= <attribute_key> ":" <attribute_value>

<relation_declaration> ::= <identifier> <arrow> <identifier>
                           [<string>] [<label_marker>*] [<attribute_block>]
<arrow>                ::= "->" | "-->" | "<->"
<label_marker>         ::= ">" <string> | "<" <string>

<attribute_block>      ::= "{" <attribute>* "}"
<attribute>            ::= <attribute_key> ":" <attribute_value>
<attribute_key>        ::= <identifier> | "style." <identifier> | "meta." <identifier>
<attribute_value>      ::= <string> | <atom> | <number> | <boolean> | <algorithm_config>
<algorithm_config>     ::= <atom> ["{" <option_pair>* "}"]
<option_pair>          ::= <identifier> ":" <attribute_value>

<identifier>           ::= [a-z][a-z0-9_]*
<atom>                 ::= [a-z][a-z0-9_.-]*
<string>               ::= '"' <character>* '"'
<number>               ::= [0-9]+ ("." [0-9]+)?
<boolean>              ::= "true" | "false"
```

---

## 12. 语义约束汇总

| # | 约束 |
|---|------|
| 1 | 一个文件恰好一个 `diagram` |
| 2 | node / group id 全局唯一，且不互相撞名 |
| 3 | 边端点必须是已声明 node |
| 4 | group 不可作边端点 |
| 5 | 组内边两端须为该组后代 node |
| 6 | diagram 固定属性不可重复；未知 diagram key 警告忽略 |
| 7 | 自环默认非法；仅 profile 允许时合法 |
| 8 | 形状未识别 → 渲染回退 `rect` |
| 9 | 无声明式样式；仅 `style.*` / `theme` / `render_style` |

---

## 13. 完整示例

```plotgram
// 登录认证示意

diagram flowchart {
    title: "用户登录"
    layout: hierarchical { direction: top-to-bottom }
    theme: common.clean-light
    render_style: standard

    node start "开始" : circle { kind: start }
    node login "登录" { kind: process }
    node ok "成功？" : diamond { kind: decision }

    group auth "认证服务" {
        node api "API" { kind: service, status: healthy }
        node db "用户库" : cylinder { kind: database }
        api -> db "查询"
    }

    start -> login
    login -> api "提交"
    api --> login "结果"
    login -> ok
    ok -> start "重试"    // 若 profile 允许自环以外的回边；自环仍依 §7.5
}
```

---

## 附录 A：相对 language-spec 的主要变更

| 项 | 旧（0.4） | 新（2.0-draft） |
|----|-----------|-----------------|
| 节点关键字 | `entity` | `node` |
| 语义标注 | `entity[database]` / `type:` | `kind:` |
| 形状 | 多由 type 推断 / style.shape | `: shape` 显式覆盖 |
| 声明式样式 | `node_style` / `edge_style` | **删除**；仅内联 `style.*` |
| ER `field` | （旧亦弱） | **本草案不做**；另文 |
| 自环 | decision 例外 | 默认禁，profile 可开 |
| diagram → 引擎 | 易渗入图名 | profile 展开后引擎不收 type |

旧文档 [`language-spec.md`](language-spec.md) 在 2.0 落地前可作对照，**以实现本文件为准**。
