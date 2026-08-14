# Plotgram DSL 规范

> 版本：2.7-draft  
> 状态：语法契约草案（相对 v1 `language-spec.md` 的瘦身重设计；该文档已删除）  
> 定稿选择：node / group / edge **规范形态**为声明头 + `{ … }`（`label` / `variant` / `style.*` 等进花括号）；node 另有三轴 + `archetype`；边箭头 `->` / `-->` / `<->` 保留语法；边端口 `side` + `slot`；无声明式样式；ER 另文；自环默认禁  
> 2.1：废弃 `kind`；三轴 + archetype  
> 2.2：node 废除 `: shape` 后缀；规范一切进 `{}`；§5.5 糖  
> 2.3：group 废除位置 string 标签；`label` / `variant` 进 `{}`；§6.5 糖  
> 2.4：edge 标签一律进 `{}`；废除 `>"` / `<"` 端点糖；§7.5 仅保留中点 string 糖；允许省略空 `{}`  
> 2.5：`group_anchor` 一等字段 + `@group` 组框边糖（ADR-004）  
> 2.6：废除 `diagram <type>` 位置；改为属性 `profile:`（ADR-001）  
> 2.7：`partition` / `cell_col` / `cell_row`（ADR-008 PartitionGrid）

**本文档定义**：DSL **语法形态**，以及 **属性注册表**（§14：写者/消费者/状态、shape / variant 封闭集）。  
archetype 展开与 CSV 见 [`archetype-spec.md`](archetype-spec.md)；视觉属性词表见 [`style-sheet-spec.md`](style-sheet-spec.md) §5。

---

## 1. 语法总览

### 1.1 文件结构

```
<file> ::= [<doc_comment>] <diagram_declaration>
```

一个 `.pgm` 文件由可选的文档注释块和一个 `diagram` 声明组成：

```plotgram
// 文档注释（可选）

diagram {
    profile: flowchart
    // 图表内容
}
```

### 1.2 Profile 预设

```
<profile_id> ::= "flowchart" | "sequence" | "architecture"
               | "state"     | "er"       | "mindmap"
```

`profile:` 是 DSL 层的 **算法/约束预设名**（写在 diagram 属性块内，见 §4）。解析后展开为默认的 `layout` + `edge_routing` 及图种约束；**布局/路由引擎不接收 profile / 图种名**，只认算法名与参数（见 §8、ADR-001）。

**算法默认 ≠ 约束默认**：未写 `profile:` 时，解析器仍可按 flowchart **算法**预设填充 `layout` / `edge_routing`（见 §4.2、§8 表），但**不**继承 flowchart 的约束包（如自环许可）。图种约束须作者**显式**写 `profile:` 才生效。

**已废弃**：`diagram flowchart { … }` 位置 type（须写成 `diagram { profile: flowchart … }`）。

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
- 词法层校验：`.` 不得出现在 atom 首尾，不得出现 `..`
- 无需引号；引号形式语义等价
- 用于：属性值（`archetype`、`variant`、`icon`、`status`、算法名、主题 ID、形状名等）
- 合法性由引擎 / profile 后置校验；`variant` / shape 为封闭集（见 §14），DSL 语法层不枚举

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
| 无前缀 | standard | 语义属性（`label`、`shape`、`archetype`、`variant`、`icon`、`status` 等） |
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
- `<algorithm_config>` 形态（带 `{ }` 参数块）**仅对** diagram / group 级的 `layout:` 与 `edge_routing:` 合法；其它属性的值只能是四种字面量之一（string / atom / number / boolean）

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

diagram {
    profile: flowchart
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
<diagram_declaration> ::= "diagram" "{" <diagram_body> "}"
<diagram_attribute>   ::= <attribute_key> ":" <attribute_value>
```

规范形态与 node/group 一致：**头只有 `diagram`，其余进花括号**。不做 `diagram flowchart {` 位置糖。

### 4.2 图表属性

| 属性 | 类型 | 说明 |
|------|------|------|
| **`profile`** | atom（§1.2 封闭集） | 算法与图种约束预设；展开 `layout` / `edge_routing` 及自环等约束；**须显式声明**才启用约束（隐式 flowchart 算法默认不含约束包，见 §1.2） |
| `title` | string | 图表标题 |
| `layout` | algorithm_config | 布局算法及参数（覆盖 profile 默认） |
| `edge_routing` | algorithm_config | 边路由算法及参数；可与布局内建路由二选一或配合（见引擎注册表） |
| `theme` | atom | 颜色/字体主题 |
| `render_style` | atom | 笔触皮肤 |

- 所有属性可选  
- 未写 `profile` 且未写 `layout` → **算法默认**按 flowchart 预设展开 `layout` / `edge_routing`（见 §8 表）  
- 写了 `profile` → 先灌该预设的算法与约束，再被显式 `layout` / `edge_routing` **覆盖**  
- 只写 `layout`、不写 `profile` → 允许（纯算法驱动）  
- **算法默认 ≠ 约束默认**：未写 `profile` 时，即使算法走 flowchart 预设，**profile 级约束**（如自环许可）仍不适用；须显式写 `profile: flowchart` 或 `profile: state` 等才启用对应约束（见 §7.8、§8）  
- 同一属性不可重复声明  
- 未识别的 diagram 级 key 产生警告，忽略  
- 未知 `profile` atom → **错误**（封闭集）

### 4.3 PartitionGrid 声明（ADR-008）

正交分区网格（泳道 / 矩阵）。**不是** `group`，也不是 Channel 走廊 `lane`。

```
<partition_declaration> ::= "partition" "{" <partition_body> "}"
<partition_body>        ::= (<partition_axis>)*
<partition_axis>        ::= ("column" | "row") <identifier> "{" <partition_axis_body> "}"
<partition_axis_body>   ::= (<partition_axis_attribute>)*
<partition_axis_attribute> ::= "label" ":" <string>
```

- 至多一个 diagram 级 `partition` 块。  
- `column` / `row` **声明序** = 几何轴序（稳定）。  
- 轴 `id` 与 node / group id **同一命名空间，禁止冲突**。  
- 仅 columns（或仅 rows）= 泳道；两者都有 = 矩阵。  
- **首期不做**：`swimlane` 关键字、`group` 自动成列。  
- **Parser**：块解析 **active**（model 类型 + parser 均已落地）。

```plotgram
diagram {
    profile: flowchart
    layout: hierarchical { direction: top-to-bottom }

    partition {
        column customer { label: "客户" }
        column sales    { label: "销售" }
        column warehouse { label: "仓库" }
    }

    node place_order {
        label: "下单"
        archetype: start
        cell_col: customer
    }
    node verify_order {
        label: "审核"
        cell_col: sales
    }

    place_order -> verify_order
}
```

矩阵示例（阶段 × 角色）另加 `row` 轴，节点同时写 `cell_col` 与 `cell_row`。

### 4.4 示例

```plotgram
diagram {
    profile: flowchart
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

纯算法、不绑预设名：

```plotgram
diagram {
    layout: hierarchical { direction: left-to-right }
    theme: common.clean-light

    node a { label: "A" }
    node b { label: "B" }
    a -> b
}
```

---

## 5. Node 声明

### 5.1 规范形态（Canonical）

**一切进属性块。** 位置标签、`: shape` 后缀均已废除。

```
<node_declaration> ::= "node" <identifier> <attribute_block>
<attribute_block>  ::= "{" <attribute>* "}"
<attribute>        ::= <attribute_key> ":" <attribute_value>
```

| 部分 | 含义 |
|------|------|
| `identifier` | 节点 ID（边引用）；**不是**显示文案 |
| `attribute_block` | 唯一的作者面；label / 三轴 / archetype / style.* / … 都写在这里 |

最小节点（无显示文案、无外观覆盖）：

```plotgram
node login {}
```

完整规范写法：

```plotgram
node db {
    label: "用户数据库"
    archetype: database
}

node odd {
    label: "异形库"
    archetype: database
    shape: rounded_rect
    variant: primary
}

node spacer {
    shape: circle
}
```

解析后 IR：`label` / `shape` 从属性块**提升**到 `Node` 字段（与 `Node.label` / `Node.shape` 对齐）；其余键留在 `attrs`。同一键不可在块内重复。

### 5.2 三轴：shape × variant × icon（+ archetype）

真值仍是三根正交轴；**写法全部是属性块键**：

| 维度 | 写法 | 职责 |
|------|------|------|
| **shape** | `shape: <atom>` | 渲染几何 |
| **variant** | `variant: <atom>` | 视觉变体（颜料）；查主题 `variants` |
| **icon** | `icon: <atom>` | 装饰图标 |
| **label** | `label: <string>` | 显示文案；省略 = 无文字纯形状 |
| **archetype** | `archetype: <atom>` | 展开糖：只填空写入 shape / variant / icon |

- 未写 `shape`：archetype 可填 → 否则主题 / profile 兜底
- 未写 `variant`：archetype 可填 → 否则 `default`
- 未写 `icon`：archetype 可填 → 否则无图标
- 未写 `label`：无显示文案（不默认用 id）
- `shape` **不是** `style.shape`（`style.shape` 静默忽略，见 §14）
- 封闭集与 cascade 真源：§14；archetype：[`archetype-spec.md`](archetype-spec.md)

**已废弃**：`kind`；位置 string 标签作规范形态；声明后缀 `: <shape>`。

### 5.3 属性块键（示例）

属性为自由 map，DSL 不枚举字段。**写者 / 消费者 / 状态以本文 §14 为真源**：

| key | 示例值 | 状态 | 说明 |
|-----|--------|------|------|
| `label` | `"用户库"` | `active`（模型字段） | 显示文案 |
| `shape` | `cylinder`, `diamond` | `active`（模型字段） | 几何；封闭集见 §14.6 |
| `role` | `entity`, `group_anchor` | `active`（模型字段） | 结构角色；见 §5.7 |
| `host_group` | group id | `active`（模型字段） | 仅 `group_anchor`；见 §5.7 |
| `side` / `slot` | 四向 / 非负整数 | `active`（模型字段） | 仅 `group_anchor` 贴框；提升为 `Node.anchor` |
| `cell_col` | atom（partition column id） | **`active`（模型字段）** | → `Node.partition_cell.column`；Hier 消费 **已落地**（四向 + Weak/Strong，见 partition-grid PG-4） |
| `cell_row` | atom（partition row id） | **`active`（模型字段）** | → `Node.partition_cell.row`；Hier 消费 **已落地**（PG-3 行区间 + PG-4 四向） |
| `archetype` | `database`, `gateway` | **`planned`** | 展开糖；纪律与目录见 [`archetype-spec.md`](archetype-spec.md) |
| `variant` | `primary`, `info`, … | **`planned`** | 颜料槽；封闭集见 §14.7 |
| `icon` | `none`, 图标名 | `active` | 装饰 |
| `status` | `healthy`, … | **`planned`** | 运行态；与 variant 正交 |
| `style.fill` | `"#E3F2FD"` | `active` | 内联颜料；词表见 style-sheet-spec §5 |
| `meta.*` | 任意 | — | 渲染器忽略 |

**内容扩展（ER 字段等）**：本草案不引入非 `key: value` 语句；另文。

### 5.4 规范形态示例

```plotgram
node login {
    label: "用户登录"
}

node db {
    label: "用户数据库"
    archetype: database
}

node db2 {
    label: "奇怪的库"
    archetype: database
    shape: rounded_rect
    variant: primary
}

node api {
    label: "API 服务"
    archetype: service
    status: healthy
}

node gw {
    label: "网关"
    archetype: gateway
    style.fill: "#FFF3E0"
}

node spacer {
    shape: circle
}
```

### 5.5 语法糖（便利层）

糖必须能**机械降到** §5.1 规范形态；parse 后 IR 与手写 `{}` 无法区分。  
下列为**已采纳**的轻糖；未列入的写法非法。

#### 5.5.1 省空块

| 糖 | 展开为 |
|----|--------|
| node id（无属性块） | node id + 空属性块 |

#### 5.5.2 位置：label → archetype → icon

位置参数**不是**三个互相独立的 `?`。在 `node <id>` 之后，若出现位置糖，必须先写 **string**（label），其后至多两个裸 atom，语义固定：

```text
node <id> <string>                         // ① 仅 label
node <id> <string> <archetype>             // ② label + archetype
node <id> <string> <archetype> <icon>      // ③ label + archetype + icon
```

可选再跟 `{ … }`；块内键与位置糖写入的键冲突 → **错误**。

| 糖 | 展开为 |
|----|--------|
| node id + string | 属性块仅含 label |
| node id + string + A | 属性块含 label 与 archetype: A |
| node id + string + A + I | 属性块含 label、archetype: A、icon: I |
| 上述任一行 + 属性块 | 先按上表展开，再合并块 |

上表用自然语言描述；对应规范形态示例：`node db "用户库"` → `node db { label: "用户库" }`；`node db "用户库" database` → `node db { label: "用户库", archetype: database }`。若位置 string 为 **空串** `""`，则 **不写 label**（见下「空 label」）。

**空 label**：位置 string 为 `""` 时，parse 后**忽略 label**（`Node.label = None`），与省略 `label:` 等价。仍占位，以便后面写 archetype / icon：

```plotgram
node spacer ""                          // → node spacer {}
node db "" database                     // → { archetype: database }（无 label）
node db "" database mysql               // → { archetype: database, icon: mysql }
```

**解析纪律（实现友好）**：

1. `node` `<id>` 之后：若下一 token 是 `{` 或语句结束 → 无位置糖  
2. 若下一 token 是 **string** → 进入本小节；再读 0～2 个 atom（第 1 = archetype，第 2 = icon）；再可选 `{`  
3. **禁止** `node <id> <atom>…`（无 string 却写裸 atom）——避免与 id 后直接跟块/换行的歧义，也避免实现上难切分
4. 位置糖的 0～2 个 atom 必须与 string **同一行**；换行即终止位置糖——避免下一条语句的裸标识符（如边源）被误读为 archetype

```plotgram
node login                                      // 省空块
node login "用户登录"                             // ①
node db "用户库" database                         // ②
node db "用户库" database mysql                   // ③
node db "用户库" database mysql { variant: primary }
node db "" database                             // 无文案 + archetype

// 非法：
// node db database
// node db database mysql
```

第 1 个裸 atom **只**是 `archetype`；第 2 个 **只**是 `icon`（可写 `none` 关掉默认图标）。`shape` / `variant` 不得出现在位置上，只进 `{ }`。

#### 5.5.3 明确不做

- 形状后缀（如 node id : cylinder）——已废除
- Mermaid 式括号形状（如 `[(label)]`）——不做
- 无 label string 的裸 archetype（如 node id database）——非法
- 块外再塞 `variant` / `shape`
- 第三个及以上裸 atom

更重的糖（如默认 `archetype` 与 id 同名）**未采纳**；需要时另修订本表。

### 5.6 Archetype

`archetype:` 为展开糖（不是第四轴）。**展开纪律、CSV 真源、编译进二进制**见 [`archetype-spec.md`](archetype-spec.md)。  
属性键登记见本文 §14；位置糖见 §5.5.2。

### 5.7 `group_anchor`（组框锚点节点）

> ADR-004：组间「框到框」边仍只连 **node**；用隐形锚点贴在 group 框上，不把 group 当作边端点。

#### 5.7.1 规范形态

```plotgram
group frontend {
    label: "前端"
    node web { label: "Web" }

    // 进阶手写：贴东侧的隐形锚点（通常由 §7.6 糖生成）
    node ga_fe_e {
        role: group_anchor
        host_group: frontend
        side: east
        // slot: 0          // 可选；同侧多锚时去重叠
    }
}
```

| 键 | 提升字段 | 规则 |
|----|----------|------|
| `role: group_anchor` | `Node.role` | 封闭集：`entity`（默认）/ `group_anchor`；未知 atom → 错 |
| `host_group` | `Node.host_group` | **必填**（且须是已声明 group id） |
| `side` | `Node.anchor.side` | **必填**；四向封闭集（同 §7.4） |
| `slot` | `Node.anchor.slot` | 可选；写了 `slot` 必须同时有 `side` |

- parse 后调用 `Node::lift_structural_attrs`（或 `Graph::lift_all_node_structural_attrs`）；上述键从 `attrs` **剥除**。
- `entity` 节点不得残留 `host_group` / `side` / `slot`。
- 锚点节点**必须是** `host_group` 的成员（直接子 node；糖展开时注入该组）。
- **不绘制**形体；逻辑尺寸由引擎取最小非零占位（禁止依赖 0×0）。
- **不参与** Sugiyama 叶层/序；几何 = 组框定稿 bbox + `side`/`slot` **派生**；Ink 禁止「发现是 anchor 再挪到框边」。
- 勿写 `label` / `shape` / `archetype` / `icon`（无业务外观）；写了由实现忽略或告警，不改变锚点语义。

#### 5.7.2 与边端口的关系

边仍可用 `from_side` / `to_side` 钉死端口。对锚点端，作者侧通常与 `Node.anchor.side` **一致**（糖展开时同步写入）。决议 `PortRef` 仍由布局组合相写出到 `EdgePlacement`。

---

## 6. Group 声明

### 6.1 规范形态（Canonical）

与 node 同一纪律：**组级属性一律写在花括号内**，以 `key: value` 形式与成员声明并列；**不再**在 id 后写位置 string。

```
<group_declaration> ::= "group" <identifier> "{" <group_body> "}"
<group_body>        ::= (<group_item>)*
<group_item>        ::= <group_attribute>
                      | <node_declaration>
                      | <relation_declaration>
                      | <group_declaration>
<group_attribute>   ::= <attribute_key> ":" <attribute_value>
```

| 部分 | 含义 |
|------|------|
| `identifier` | group ID（全局唯一，不与 node 撞名） |
| 花括号内 `key: value` | 组级属性：`label` / `variant` / `layout` / `style.*` / `meta.*` |
| 花括号内 `node` / `group` / 边 | 组成员（与组级属性可任意交错，见 §6.2） |

最小 group（无标题、无成员）：

```plotgram
group lane {}
```

完整规范写法：

```plotgram
group compute {
    label: "计算层",
    variant: muted
    layout: horizontal

    node spark { label: "Spark" }
    node flink { label: "Flink" }
}
```

解析后 IR：`label` 从组级属性**提升**到 `Group.label`（与 `Group::label` 对齐）；`variant` 留在 `attrs` 直至 resolve。组级 `key: value` 与成员声明的区分靠 token：以 `node` / `group` 或边模式开头的是成员，否则是组级属性。

**已废弃**：`group id "标签" { … }` 作为**规范**形态（仍允许为 §6.5 糖）。

### 6.2 组级属性

| 键 | 写法 | 职责 |
|----|------|------|
| **label** | `label: <string>` | 组标题；省略 = 无标题（不默认用 id） |
| **variant** | `variant: <atom>` | 视觉变体（颜料）；查主题 `variants`（§14.7 封闭集） |
| **layout** | `layout: <algorithm_config>` | 组内布局 hint（**`planned`**，引擎尚未读） |
| **style.\*** | `style.<prop>: …` | 内联颜料；词表见 style-sheet-spec §5 |
| **meta.\*** | `meta.<key>: …` | 渲染器忽略 |

group **没有** `shape` / `icon` / `archetype`（容器不是节点几何）。`variant` 与 node 共用同一封闭集，但 resolve 时只应用主题词表中**适用于 group** 的颜料字段（fill / stroke / radius …），见 style-sheet-spec §6.2。

- 未写 `variant` → `default`
- 未写 `label` → 无组标题
- 组级属性同一键不可重复；糖写入的 `label` 与块内 `label:` 冲突 → **错误**（同 §12 #12）

### 6.3 规则

- 嵌套深度 DSL 不限制
- 内部边的两端必须都属于当前 group 的后代 node；跨组边写在顶层（或共同祖先 group）
- group ID 与 node ID 全局不重复
- group **不能**作为边的端点（IR 层）；组框连线用 §5.7 锚点或 §7.6 `@group` 糖

### 6.4 示例

```plotgram
group frontend {
    label: "前端"
    variant: primary

    node web { label: "Web" }
    node mobile { label: "Mobile" }
}

group backend {
    label: "后端"
    variant: muted

    node api { label: "API", archetype: service }
    node db { label: "数据库", archetype: database }

    api -> db "query"
}

web -> api
mobile -> api
```

### 6.5 语法糖（便利层）

与 §5.5 同纪律：糖机械降到 §6.1；parse 后 IR 与手写 `{}` 无法区分。

#### 6.5.1 位置 label

| 糖 | 展开为 |
|----|--------|
| group id + string + 花括号体 | 在花括号体最前注入 label: string，再合并体中其余项 |
| group id + string（无体） | group id { label: string } |

```plotgram
group auth "认证服务" {
    node api { label: "API" }
}
// ≡
group auth {
    label: "认证服务"
    node api { label: "API" }
}
```

**空 label**：位置 string 为 `""` 时忽略 label（与省略 `label:` 等价），仍占位以便只写组级其它属性：

```plotgram
group frame "" {
    variant: muted
    node x { label: "X" }
}
```

**禁止**：`group id <atom>`（无 string 的裸 atom）——group 无 archetype 位置糖。

#### 6.5.2 明确不做

- ~~group id "标签" 作为规范写法~~（降为糖）
- group 上的 `shape` / `icon` / `archetype`
- 第三个及以上位置参数

---

## 7. Edge 声明

### 7.1 规范形态（Canonical）

**拓扑与箭头是语法；文案与颜料进属性块。** 废除 `>"` / `<"` 端点标签糖。

```
<relation_declaration> ::= <endpoint> <arrow> <endpoint> [<attribute_block>]
<endpoint>             ::= <identifier> | "@" <identifier>
<arrow>                ::= "->" | "-->" | "<->"
<attribute_block>      ::= "{" <attribute>* "}"
```

| 部分 | 含义 |
|------|------|
| `endpoint` | 源/目标：node id，或 `@group_id`（§7.6 糖，展开为 `group_anchor`） |
| `arrow` | 三种之一（见 §7.2） |
| `attribute_block` | 边上唯一作者面：`label` / `head_label` / `tail_label` / `variant` / 端口 / `style.*` / …；可省略空块（§7.5.1） |

最小边（无标签、无覆盖）：

```plotgram
a -> b
```

等价于 `a -> b {}`（见 §7.5.1）。

完整规范写法：

```plotgram
user -> api {
    label: "请求"
}

api --> user {
    label: "响应"
    variant: secondary
}

api -> db {
    label: "查询"
    head_label: "1"
    tail_label: "N"
    variant: muted
    style.stroke: "#888"
}
```

解析后 IR：`label` / `head_label` / `tail_label` 从属性块**提升**到 `Edge` 对应字段；`variant` 与端口键留在 `attrs` 直至 resolve / Plan。属性块内同一键不可重复。

**已废弃**：位置 string 中点标签作为**规范**形态（降为 §7.5 糖）；`>"head"` / `<"tail"` 端点标记；属性块内 `arrow:` / `source:` / `target:`（与语法双真源）。

### 7.2 箭头（语法，仅 3 种）

| 箭头 | 语义 |
|------|------|
| `->` | 主动流向 |
| `-->` | 响应/返回（未设 dash 时由主题补虚线，§14.4.2） |
| `<->` | 双向；默认带 `undirected` 非分层语义（糖，等价 `undirected: true`），显式 `undirected:` 属性优先（§14.4） |

箭头是**结构**字段，落在 `Edge::arrow`；**不得**用属性块 `arrow:` 覆盖。

### 7.3 属性块键

| 键 | 写法 | 职责 |
|----|------|------|
| **label** | `label: <string>` | 边中部标签；省略 = 无中点文案 |
| **head_label** | `head_label: <string>` | 靠近**目标**端标签（如 ER 基数 `1`） |
| **tail_label** | `tail_label: <string>` | 靠近**源**端标签（如 ER 基数 `N`） |
| **variant** | `variant: <atom>` | 视觉变体（颜料）；查主题 `variants`（§14.7 封闭集） |
| **from_side** / **to_side** | 见 §7.4 | 端口侧约束 → 提升为 `Edge.from_port` / `to_port`（**`active`**）；仅 FixedSide / FREE |
| **weight** | `weight: <number>` / `critical: true`（糖 = `weight: 2.0`） | 边权重 → 提升为 `Edge.weight: Option<f64>`（**`active`**）；`critical` 与 `weight` 同写 → 解析错误（`CriticalWeightConflict`） |
| **style.\*** | `style.<prop>: …` | 内联颜料；词表见 style-sheet-spec §5 |
| **meta.\*** | `meta.<key>: …` | 渲染器忽略 |

边**没有** `shape` / `icon` / `archetype` / `layout` / `seq` / `edge_group`。

- 未写 `variant` → `default`
- 三处标签互不推导；需要端点文案时显式写 `head_label:` / `tail_label:`
- 封闭集与 cascade：§14.4；`variant` resolve 见 style-sheet-spec §6.2
- 端口 / `weight`（`critical` 糖）经 parse 提升为一等字段后**不得**再留在 attrs 供引擎读取（见 ADR-003）
- 边合流不写边级键：开 `layout: hierarchical { auto_edge_grouping: true }`（见 §7.4.3）

### 7.4 端口（side）

边不仅连接「哪个 node」，还可钉死「节点的哪一侧」。同侧多边的沿边序由布局自动分配（`AlongSpec::Ordered`），**不**暴露边级 `*_slot` / ratio / 局部坐标 DSL。

#### 7.4.1 模型（IR / 引擎）

```
PortConstraint =                               // 作者钉死；落在 Edge.from_port / to_port
    | FixedSide   { side }                     // 边 DSL 仅此档
    | FixedOrder  { side, order: u32 }         // 仅 group_anchor（side + 可选 slot）
// 未写端口 = FREE（None）：侧别与档位全部由算法推断（+ shape port policy）

PortRef = { side: Side, along: AlongSpec }     // 已决议；落在 EdgePlacement
AlongSpec =
    | Ordered { order: u32, count: u32 }       // 组内相对序；Metric 按 (order+1)/(count+1) 展开
    | LocalOffset(Point)                       // 算法自用（如自环）；非边 DSL
Side    = north | south | east | west
```

| 字段 | 含义 |
|------|------|
| `side` | 锚在节点的哪条边（封闭四向） |
| `along` | 决议后的沿边规格；像素由 Metric 展开 |

**写权（硬纪律）：**

- 边作者约束只经 **`from_side` / `to_side`** 提升进 `Edge.from_port` / `to_port`。
- **决议** `PortRef` 由布局**组合相**写入 `EdgePlacement`；度量只算像素；**Ink 不得发明或改写端口**。
- DSL 未写 → 字段为 `None` → 算法推断并写入决议端口。

坐标系约定：`north/south/east/west` 相对**节点自身框**；画布 LTR/TTB 只影响默认推断策略。

#### 7.4.2 DSL 表面

> **状态：`active`（模型字段）** —— parse 须提升进 `Edge`；引擎组合相须读约束。

**默认：不写端口。** `a -> b` 合法。

需要钉侧时，写在属性块中（均为可选）：

| 属性键 | 类型 | 说明 |
|--------|------|------|
| `from_side` | atom（`north`/`south`/`east`/`west`） | 源端侧（FixedSide） |
| `to_side` | atom（同上） | 目标端侧 |

**已移除**（写了 → 解析错误）：`from_slot` / `to_slot` / `from_ratio` / `to_ratio` / `from_x`+`from_y` / `to_x`+`to_y` / `from_sides` / `to_sides`。

组锚点手写节点仍可用 `side` + 可选 `slot`（§5.7）；`@group` 糖只读对应 `*_side`，同 `(group, side)` 复用同一锚点。

规则：

1. 未写侧 → FREE。  
2. 侧 atom 必须是四向封闭集。  
3. 移除的键 → 校验错误（不静默忽略）。  
4. 提升后不得再留在 attrs 供引擎读取。

`Node.anchor` 可用 FixedSide / FixedOrder（`side` + 可选 `slot`）。

不引入 `@south` 箭头后缀；边端口只走 `from_side` / `to_side`。

#### 7.4.3 自动边合流（auto_edge_grouping）

一对多 / 多对一扇出的端口合流**不**用手写边级组 id。开布局开关即可：

```plotgram
layout: hierarchical { auto_edge_grouping: true }

hub -> a
hub -> b
hub -> c
```

| 参数 | 类型 | 说明 |
|------|------|------|
| `auto_edge_grouping` | bool | 同源扇出 / 同汇扇入：共享端口 + 干线 + 水平总线 + stub（yFiles bus）；默认 `false` |

- 对应 yFiles `automaticEdgeGrouping`：算法按公共源/汇自动成组，**不**要求作者逐边标记。
- DSL 键 `edge_group` **已移除**；写了 → 解析错误（提示改用本开关）。
- 勿使用已撤销的 `bus_routing`（Layout Styles demo 误映射；见 edge-parameters.md §2.4）。

#### 7.4.4 示例（正反边固定侧）

```plotgram
node a { label: "A" }
node b { label: "B" }

a -> b { label: "req" }
b --> a { label: "resp" }

a -> b {
    label: "req"
    from_side: south
    to_side: north
}
b --> a {
    label: "resp"
    from_side: north
    to_side: south
}
```

```
    ┌───────┐
    │   A   │
    └──┬─┬──┘  south（侧内序由算法分配）
       │ │
    ┌──┴─┴──┐
    │   B   │
    └───────┘
```

### 7.5 语法糖（便利层）

糖机械降到 §7.1；parse 后 IR 与手写 `{}` 无法区分。

#### 7.5.1 省空块

| 糖 | 展开为 |
|----|--------|
| `src arrow tgt`（无块） | `src arrow tgt {}` |

#### 7.5.2 位置：中点 label

| 糖 | 展开为 |
|----|--------|
| `src arrow tgt <string>` | `{ label: <string> }` |
| 上述 + `{ … }` | 先注入 `label`，再合并块；`label:` 冲突 → **错误** |

```plotgram
user -> api "请求"
// ≡
user -> api { label: "请求" }

api -> db "查询" { variant: muted }
// ≡ 先糖再合并块
```

**空 label**：位置 string 为 `""` 时忽略 `label`（与省略 `label:` 等价）。

**禁止**：

- `src arrow tgt <atom>`（无 string 的裸 atom）
- `>"…"` / `<"…"` 端点标记（用 `head_label:` / `tail_label:`）
- 属性块内 `arrow:` / `source:` / `target:`

端点标签（ER 基数等）**只**进属性块：

```plotgram
api -> db {
    label: "查询"
    head_label: "1"
    tail_label: "N"
}
```

#### 7.5.3 明确不做

- ~~`api -> db "查询" >"1" <"N"`~~（端点糖）
- ~~位置 string 作为规范中点标签~~（降为糖）
- 边上的 `shape` / `icon` / `archetype`

### 7.6 `@group` 组框边糖（ADR-004）

> 目标：作者可写「区到区」连线；parse **展开**为两个（或一侧）`group_anchor` node + 一条普通边。  
> IR **仍**遵守「边只连 node」；展开后与手写 §5.7 无法区分。

#### 7.6.1 语法

边端点前缀 `@` 表示「接到该 **group** 的框」，不是 node id：

```
<endpoint> ::= <identifier> | "@" <identifier>
```

- `@gid`：`gid` 必须是已声明的 **group** id  
- 无 `@`：必须是已声明的 **node** id（含既有 `group_anchor`）  
- 允许混合：`@frontend -> api`、`web -> @backend`

```plotgram
group frontend {
    label: "前端"
    node web { label: "Web" }
}
group backend {
    label: "后端"
    node api { label: "API" }
}

@frontend -> @backend {
    label: "调用"
    from_side: east
    to_side: west
}

web -> @backend {
    label: "直连框"
    to_side: north
}
```

#### 7.6.2 展开规则

对每个 `@gid` 端：

1. 取该端对应的侧：源端用 `from_side`，目标端用 `to_side`（**不**读 `*_slot`）。  
2. **`@` 端缺少对应 `*_side` → 解析/校验错误**（锚点 `side` 必填，不发明默认侧）。  
3. 在 group `gid` 内注入（或复用）一个 node：
   - `role: group_anchor`
   - `host_group: gid`
   - `side` = 上一步的侧（`@group` 糖不写 `slot`）
   - 合成 id：实现自定，须全局唯一；推荐稳定派生，例如 `ga_<gid>_<side>`，冲突时加边 id 后缀  
4. 同 `(host_group, side)` 的锚点 **可合并复用**（多条边共享同一框上侧）。  
5. 把边的该端改写为上述 node id；边属性中的 `from_side`/`to_side` **保留**并提升为 `Edge.from_port`/`to_port`（与锚点侧一致）。  
6. 边声明位置不变（通常在共同祖先 / 顶层）。

展开后须再跑 `lift_all_node_structural_attrs` + `lift_all_edge_structural_attrs`。

#### 7.6.3 明确不做

- 裸写 `frontend -> backend`（两端是 group id、无 `@`）——**非法**（避免与漏写 node 混淆）；必须 `@frontend -> @backend` 或手写锚点  
- 把 group 留在 IR 端点（`Endpoint::Group`）  
- 渲染期把线「贴」到组框而不生成锚点  
- `@` 用在非边端点位置

### 7.7 示例

```plotgram
user -> api "请求"
api --> user { label: "响应", variant: secondary }
a <-> b { label: "同步" }
api -> db {
    label: "查询"
    head_label: "1"
    tail_label: "N"
}
api -> cache { status: degraded }        // status：planned
db -> api {
    label: "回写"
    from_side: north
    to_side: south
}                                       // 端口 → Edge.from_port / to_port
```

结构键（端口 / `weight` / `role`…）parse 后提升为一等字段；引擎不读 attrs 中的同名残留。

### 7.8 规则

- 两端必须是已声明的 **node**（不能是 group）；`@group` 仅存在于糖面，展开后变为 node
- 允许同一对 node 多条边（靠解析器分配的稳定 `edge id` 区分；端口/slot 负责几何错开）
- **自环**（`a -> a`）：默认禁止；须**显式**写 `profile: flowchart` 或 `profile: state` 才允许（隐式 flowchart 算法默认不算；见 §4.2、§8）
- 端口属性遵守 §7.4.2；提升进 `Edge.from_port` / `to_port`；Ink 不得补端口
- **无** `seq` 属性；时序时间轴见 §8.1
- `group_anchor` 遵守 §5.7；组框边糖遵守 §7.6

---

## 8. Profile 展开与引擎边界

```
.pgm
  → parse（AST 可保留 profile id，供诊断 / 展开）
  → 展开 @group 糖（§7.6 → group_anchor nodes + 普通边）
  → lift Node 结构字段（role / host_group / side / slot）
  → lift Node 结构字段（role / host_group / side / slot / cell_col / cell_row）
  → validate_partition（有 cell 则须有 grid；轴 id 冲突检查）
  → lift Edge 结构字段（from_side… / weight（含 `critical` 糖）→ Edge 一等字段）
  → archetype expand（见 archetype-spec：只填空写入 shape / variant / icon）
  → profile expand（显式 `profile:` → 默认 layout / edge_routing、自环策略、图种约束；无 `profile:` 时仅算法字段走 flowchart 预设；显式 layout 覆盖）
  → LayoutContract（算法名 + 参数 + 图模型；**无** profile / 图种名）
  → layout / routing 引擎（组合相补全端口 → 度量 → 落笔）
```

- 引擎入口**不**按图名 / profile 名分支（禁图名特判）；差异只来自 contract 已展开的字段
- 默认算法与约束表由 profile 表 + 引擎注册表维护；本规范只要求「有 profile、可展开」
- **端口 / 边组 / group_anchor**：DSL 可选；提升后为一等字段（见 §5.7、§7.4、ADR-003/004）
- 示意（非封闭承诺，以实现注册表为准）：

| `profile` | 默认 layout（示意） | 默认 edge_routing（示意） | 自环 |
|-----------|---------------------|---------------------------|------|
| flowchart | hierarchical | **None**（布局内建正交） | 可允许 |
| architecture | hierarchical | **None**（布局内建正交） | 禁止 |
| state | hierarchical 或 circular | **None**（依布局内建） | 可允许 |
| sequence | sequence | **None**（布局自带边几何） | 禁止 |
| mindmap | tree | **None**（依布局） | 禁止 |
| er | circular 等 | **None**（依布局） | 禁止 |

上表「自环」列仅当 diagram **显式**写了对应 `profile:` 时生效；未写 `profile` 时自环一律禁止（§4.2）。

显式写 `edge_routing: orthogonal` 表示节点冻结后的**独立**路由器，与内建正交不是同一条路径（见 [`model-boundary.md`](../design/model-boundary.md)）。

### 8.1 时序：边声明序 = 时间轴

当 `layout` 为 `sequence`（含 profile 默认）：

- 消息时间序 = 图中边的**声明序**（顶层 `edges` 向量序，再按 group 声明序深度优先）。对应 model：`Graph::edges_in_declaration_order()`。
- **不**提供 `seq:` / `Edge::seq`；调整时间 = 调整 DSL 中边的书写顺序。
- 生命线、激活条等为 layout/render **派生几何**，不进入 `Graph`。
- 产品上消息写在顶层；组内消息非一等时序能力。

ER 字段表、fragment、tabular 等**另文**；本草案保证统一的 node / edge / group 骨架 + 端口/边组一等字段。

---

## 9. 样式

v2 **仅**支持内联 `style.*`（写在 node / edge / group 属性中）。

- 不提供 `node_style` / `edge_style` 顶层声明
- 不提供边上的 `line_style` 规则引用
- 批量主题交给 `theme` / `render_style` / 外部 StyleSheet（若有），不进本 DSL

可写哪些 `style.<prop>`：`<prop>` 取自 [`style-sheet-spec.md`](style-sheet-spec.md) §5 属性词表中标记为该元素**可内联**的属性（构造规则见 §14.9）。本文不列键名。

```plotgram
node api { label: "API", archetype: service, style.fill: "#E3F2FD", style.stroke: "#1976D2" }
api -> db {
    label: "查询"
    style.stroke: "#C62828"
    style.dashed: true
}
```

---

## 10. 关键字（保留字）

不可用作 identifier：

```
diagram, node, group, partition,
flowchart, sequence, architecture, state, er, mindmap,
true, false
```

`column` / `row` 仅在 `partition { … }` 块内为轴声明头（与 `node` / `group` 类似的结构关键字）；不作全局 identifier 禁词以外的额外保留——但轴 id 仍不得与 node/group 撞名。

箭头 token（`->` `-->` `<->`）不是 identifier。

---

## 11. 完整语法 BNF（汇总）

```
<file>                 ::= [<doc_comment>] <diagram_declaration>

<doc_comment>          ::= <comment_line>+        // 文件首、连续 //，空行中断
<comment_line>         ::= "//" [^\n]*

<diagram_declaration>  ::= "diagram" "{" <diagram_body> "}"
<diagram_body>         ::= (<diagram_attribute> ("," <diagram_attribute>)*
                          | <partition_declaration>
                          | <node_declaration>
                          | <relation_declaration>
                          | <group_declaration>)*
<diagram_attribute>    ::= <attribute_key> ":" <attribute_value>
                          // 含 profile: <profile_id>（§1.2 / §4.2）
<profile_id>           ::= "flowchart" | "sequence" | "architecture"
                         | "state" | "er" | "mindmap"
<partition_declaration> ::= "partition" "{" (<partition_axis>)* "}"
<partition_axis>       ::= ("column" | "row") <identifier> "{" (<partition_axis_attribute>)* "}"
<partition_axis_attribute> ::= "label" ":" <string>

<node_declaration>     ::= "node" <identifier> [<string> [<atom> [<atom>]]] [<attribute_block>]
                          // 规范：node id { … }
                          // 糖：省空块；或 string 后 0～2 atom = label / archetype / icon（§5.5）
<group_declaration>    ::= "group" <identifier> [<string>] "{" <group_body> "}"
                          // 规范：group id { label / variant / … ; members… }
                          // 糖：string 注入 label（§6.5）
<group_body>           ::= (<group_attribute> ("," <group_attribute>)*
                          | <node_declaration>
                          | <relation_declaration>
                          | <group_declaration>)*

<relation_declaration> ::= <endpoint> <arrow> <endpoint> [<string>] [<attribute_block>]
                          // 规范：src arrow tgt { label / variant / … }
                          // 糖：省略空块；或 string 注入 label（§7.5）；@group 端点见 §7.6
<endpoint>             ::= <identifier> | "@" <identifier>
                          // @gid = 组框端点糖；展开为 group_anchor（§7.6）
<arrow>                ::= "->" | "-->" | "<->"

<attribute_block>      ::= "{" <attribute_list> "}"
<attribute_list>       ::= <attribute> ("," <attribute>)*
<attribute>            ::= <attribute_key> ":" <attribute_value>
<attribute_key>        ::= <identifier> | "style." <identifier> | "meta." <identifier>
<attribute_value>      ::= <string> | <atom> | <number> | <boolean> | <algorithm_config>
<algorithm_config>     ::= <atom> ["{" <option_pair>* "}"]
<option_pair>          ::= <identifier> ":" <attribute_value>

注：`<algorithm_config>` 形态仅对 diagram / group 级 `layout:` / `edge_routing:` 合法（见 §2.7）。

属性块及 diagram / group 花括号体内的同级属性之间必须用逗号 `,` 分隔（`{ label: "A", archetype: start }`；`diagram { profile: flowchart, title: "T" }`）；无逗号写法不再合法。

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
| 3 | 边端点（展开后）必须是已声明 node |
| 4 | group 不可作 IR 边端点；组框连线用 `group_anchor` 或 `@group` 糖（§5.7 / §7.6） |
| 5 | 组内边两端须为该组后代 node |
| 6 | diagram 属性不可重复；未知 diagram key 警告忽略；`profile` 须为 §1.2 封闭集 |
| 7 | 自环默认非法；须显式 `profile: flowchart` / `state` 等允许自环的预设才合法（隐式 flowchart 算法默认不适用，§4.2） |
| 8 | 形状未识别 → 解析错误（封闭集见 §14.6；不静默回退） |
| 9 | 无声明式样式；仅 `style.*` / `theme` / `render_style` |
| 10 | 边端口：仅 `from_side`/`to_side`（→ FixedSide）或省略（→ FREE）；`*_slot`/`*_ratio`/`*_x,*_y`/`*_sides` 写了 → 错；提升为 `Edge.from_port`/`to_port` |
| 11 | 端口决议写者是布局组合相（`EdgePlacement` 上的 `PortRef`）；落笔不得发明 side/slot |
| 12 | node / group / edge 属性块内同一键不可重复；糖写入的 `label` 与块内 `label:` 冲突为错 |
| 13 | node 规范形态为 `node id { … }`；位置糖见 §5.5（须先有 string，禁止 `node id <atom>`） |
| 14 | group 规范形态为 `group id { … }`；组级属性与成员同块；位置 label 糖见 §6.5 |
| 15 | edge 规范形态为 `src arrow tgt { … }`；允许省略空 `{}`；中点 label 糖见 §7.5；`@group` 见 §7.6 |
| 16 | edge 禁止 `>"` / `<"` 端点糖；端点文案只认 `head_label:` / `tail_label:` |
| 17 | `layout: sequence` 时消息时间序 = 边声明序；禁止另立 `seq` 双真源（§8.1） |
| 18 | 边合流改布局开关 `auto_edge_grouping`；边级 `edge_group` 移除 |
| 19 | `role: group_anchor` 须有 `host_group` + `side`；提升为 `Node` 一等字段；锚点须为 host 组成员（§5.7） |
| 20 | `@gid` 端点须对应已声明 group；该端缺少 `*_side` → 错；禁止无 `@` 的裸 group id 作端点（§7.6） |
| 21 | `partition` 轴 id 与 node/group 不撞名；`cell_col`/`cell_row` 须引用已声明轴；无 grid 不得写 cell（§4.3 / ADR-008） |
| 21 | diagram 规范为 `diagram { … }`；预设用属性 `profile:`；禁止位置 `diagram flowchart {`（§4） |

---

## 13. 完整示例

```plotgram
// 登录认证示意

diagram {
    profile: flowchart
    title: "用户登录"
    layout: hierarchical { direction: top-to-bottom }
    theme: common.clean-light
    render_style: standard

    node start { label: "开始", archetype: start }
    node login { label: "登录" }
    node ok { label: "成功？", archetype: decision }

    group auth {
        label: "认证服务"
        variant: secondary

        node api { label: "API", archetype: service, status: healthy }
        node db { label: "用户库", archetype: database }
        api -> db { label: "查询" }
    }

    start -> login
    login -> api { label: "提交" }
    api --> login { label: "结果" }
    login -> ok
    ok -> start "重试"    // 回边（非自环）；自环须显式 profile（§7.8）
}
```

端口显式示例见 §7.4.4；上例故意不写端口，由 hierarchical 推断。

---


## 14. 属性注册表（Attribute Registry）

> 原独立文档 `attributes-spec.md`，已并入本规范。回答：可带哪些属性、写者/消费者、哪些是纸面（`planned`）。

### 14.1 范围与真源纪律

| 维度 | 真源 | 本文档的关系 |
|------|------|--------------|
| 语法形态（属性块写在哪、怎么写） | 本文 §5 / §6 / §7 | 同文档前半；本节不重复 BNF |
| 节点三轴（`shape` / `variant` / `icon`）与其它语义属性 | **本节（§14）** | 定义 |
| archetype 展开糖（CSV / 二进制） | [`archetype-spec.md`](archetype-spec.md) | 引用；本节只登记属性键 |
| shape 封闭集 | **§14.6** | 定义 |
| variant 封闭集 | **§14.7** | 定义 |
| icon 目录 | **§14.8** | 引用实现常量，不复制清单 |
| 视觉属性名与值域（`fill` / `stroke_width` …） | style-sheet-spec §5 属性词表 | 只给一条构造规则（§9），**不列键名** |

#### 14.1.1 纪律

> **属性是几何/视觉自由度的一种。** 与 AGENTS.md §1「每个自由度有且只有一个写者」同源：  
> 一个属性进入本表必须写明**写者**（谁产生它）与**消费者**（哪段代码读它）。  
> **没有消费者的属性只能标 `planned`**，且必须在 dsl-spec 对应处同步标注，避免作者按文档写了却无效果。

#### 14.1.2 状态取值

| 状态 | 含义 |
|------|------|
| `active` | 有消费者代码，写了就生效 |
| `planned` | 语法/模型上可承载，但**当前无任何消费者**；写了不报错也不生效 |

#### 14.1.3 当前实现基线

重建期尚无 `.pgm` 解析器（`plotgram-cli` 仅占位），`Graph` 由程序构造。因此本表的「消费者」列一律指向 **render / engine** 侧代码；「写者」列中的「DSL 作者」表示解析器落地后的来源。

引擎侧（`plotgram-engine`）当前**完全不读 attrs**——算法参数只经 `LayoutContract` 的 `AlgorithmRef.options` 进入（自由 map，见 [ADR-002](../design/adr/002-no-config-block-freeform-options.md)）。这是本表里多数 `planned` 项的直接原因。

**相对代码的漂移（1.1）**：本规范已将节点外观定为 **`shape` × `variant` × `icon` 三轴正交**，废弃 `kind` / `kind_styles`。`plotgram-render` 实现仍走 `kind` 查表与 `KIND_ICON_MAP` 推断；§10 跟踪迁移。规范优先于旧实现。

---

### 14.2 属性键的命名空间

沿用 §2.6 的三段划分，本节补充「谁消费」：

| 前缀 | 命名空间 | 消费者 | 未知键行为 |
|------|----------|--------|------------|
| 无前缀 | standard | §14.3–§14.5 列出的属性 | 忽略（不报错） |
| `style.` | style | `resolve.rs` 的三个 `apply_inline_*_styles` | 忽略 |
| `meta.` | meta | **无**（保留给外部工具链） | 忽略 |

`meta.*` 是明确的「渲染器不看」区：任何需要影响出图的属性都不得放在 `meta.` 下。

---

### 14.3 Node 属性

#### 14.3.0 三轴模型（正交）

节点外观由三根**互不推导、互不从属**的轴组成：

| 轴 | 写法 | 职责 | 值域 |
|----|------|------|------|
| **shape** | 属性 `shape: <atom>` | 几何形状 | 封闭集（§14.6） |
| **variant** | 属性 `variant: <atom>` | 视觉变体（颜料 preset） | 封闭集（§14.7） |
| **icon** | 属性 `icon: <atom>` | 装饰图标 | 开放集（§14.8） |

任意组合合法。领域实体名（`database`、`gateway`…）通过 **`archetype:`** 展开糖（见 [`archetype-spec.md`](archetype-spec.md)）或显式写在 **`icon:`** / `meta.*`；**不**作为 variant 枚举值。

显示文案是第四个常用键（非外观轴）：`label: <string>`。

**规范 DSL 形态**（§5.1）：一切进 `{}`。

```text
node db { label: "Orders DB", archetype: database }
node db2 { label: "Orders DB", shape: cylinder, variant: info, icon: database }
node api { label: "API", shape: rounded_rect, variant: primary }
node legacy { label: "ERP", shape: rounded_rect, variant: muted, icon: external }
```

| key | 值 | 状态 | 写者 | 消费者 |
|-----|----|------|------|--------|
| `label` | string | `active` | DSL 作者（或 §5.5 位置糖） | 提升为 `Node.label`；render 画文案 |
| `shape` | atom（封闭集，见 §14.6） | `active` | DSL 作者 / archetype / profile | 提升为 `Node.shape`；resolve 几何链 |
| `role` | atom：`entity` / `group_anchor` | **`active`（模型字段）** | DSL 作者（或 §7.6 糖） | 提升为 `Node.role`；见 §5.7 / ADR-004 |
| `host_group` | atom（group id） | **`active`（模型字段）** | DSL 作者（或 §7.6 糖） | 提升为 `Node.host_group`；仅 `group_anchor` |
| `side` | atom：`north`/`south`/`east`/`west` | **`active`（模型字段）** | DSL 作者（或 §7.6 糖） | 并入 `Node.anchor`；仅 `group_anchor` |
| `slot` | number（非负整数） | **`active`（模型字段）** | DSL 作者（或 §7.6 糖） | 并入 `Node.anchor.slot`；须有 `side` |
| `cell_col` | atom（column id） | **`active`（模型字段）** | DSL 作者 | 提升为 `Node.partition_cell.column`；parse/lift 已落地；Hier 消费 **已落地**（PG-0–PG-4，见 [partition-grid.md](../design/layout/hierarchical/phases/partition-grid.md)） |
| `cell_row` | atom（row id） | **`active`（模型字段）** | DSL 作者 | 提升为 `Node.partition_cell.row`；parse/lift 已落地；Hier 消费 **已落地**（PG-3 行区间 + PG-4 四向，见 [partition-grid.md](../design/layout/hierarchical/phases/partition-grid.md)） |
| `archetype` | atom（开放集；目录见 [`archetype-spec.md`](archetype-spec.md)） | **`planned`** | DSL 作者 | **展开器**（parse/profile）：只填空写入 shape / variant / icon；**render / theme / engine 不读** |
| `variant` | atom（封闭集，见 §14.7） | **`planned`**（规范已定；实现仍读 `kind`） | DSL 作者 / archetype 展开 | 主题 `variants` 查表（`resolve.rs`，待迁） |
| `icon` | atom：`none` / icon id / alias | `active` | DSL 作者 / archetype 展开 | `icons::resolve_icon`（`icons/mod.rs`） |
| `status` | atom：`healthy` / `degraded` / `down` … | **`planned`** | DSL 作者 | **无**。与 `variant` 正交：variant 是静态强调，status 是运行态 |
| `style.*` | 见 §14.9 | `active` | DSL 作者 | `apply_inline_node_styles` |
| `meta.*` | 任意 | — | DSL 作者 | 无（约定如此） |

**已废弃**：`kind`；声明后缀 `: shape`；把位置 string 当作规范标签写法（仍允许为糖，见 §5.5）。

`style.shape` **禁止生效**（词表未标可内联）；几何只用标准键 `shape:`。archetype **不是**第四轴，见 §14.3.4。

#### 14.3.1 `icon` 解析顺序

`icons::resolve_icon(node, final_shape)`（目标行为）：

```
icon: none                  → 无图标（硬短路）
icon: <id 或 alias>         → 命中则用；与 shape 不兼容 → 无图标
无 icon 属性                → 无图标
```

要点：

- **无从 variant / 其它属性推断 icon**——需要图标时显式写 `icon:`
- 键归一化：小写、`-` → `_`、trim（`normalize_key`）
- **形状兼容性是硬否决**：如 `database` 系图标在 `cylinder` 形状内不画（形状本身已表达语义），见各 `IconDef::incompatible_shapes`
- 显式 `icon:` 不兼容时**不**退回任何推断——显式意图优先于「画点什么」
- alias 存在跨 icon 重名（如 `github_actions` 同时是 `ci` 与 `github` 的别名，`bucket` 同时属 `storage` 与 `s3`）；`icon_by_key` 先精确匹 id，再按 `ICONS` 声明序取**首个**命中别名的图标。需要确定结果时写 icon id，不写 alias

实现基线仍含 `KIND_ICON_MAP`（`kind` → icon）；迁移完成后删除该表。

#### 14.3.2 shape 的决定链

形状写作属性块 `shape: <atom>`（§5）；解析提升为 `Node.shape`。有效链条：

```
defaults.node.shape        （主题全局兜底）
  → "rounded_rect"         （代码兜底，compile.rs::default_node_style）
  → profile expand 注入    （可选；图种惯例，如 flowchart decision → diamond）
  → archetype 填空         （若节点尚无 shape）
  → DSL shape:             （显式，最高）
  → icon 兼容性检查（用最终 shape）
```

**shape 与 variant 正交**：主题 `variants.*` **不得**含 `shape` 字段。几何写者是「主题 defaults / profile / archetype / DSL `shape:`」链；颜料写者是 `variants` + `style.*`。

`Profile` 当前结构只承载 layout / edge_routing / self_loop；若图种需要默认 shape，由 profile expand 写入节点字段，**不**经 variant 查表。

#### 14.3.3 variant 的决定链

```
省略 variant / 未知值     → 视作 default
variants[variant]         （主题；compile 期相对 defaults.node 物化颜料字段）
  → 若主题缺该键           → compile 期报错（主题作者问题）
  → node.attrs["style.*"] （最高，覆盖颜料）
```

variant **只**贡献 fill / stroke / font / dash / radius 等颜料；**不**贡献 shape、**不**推断 icon。

#### 14.3.4 Archetype（展开糖）

`archetype:` 在三轴之上提供命名组合包。完整纪律、CSV 列、编译进二进制见 [`archetype-spec.md`](archetype-spec.md)。

本表只强调与属性写权相关的三点：

1. 展开**只填空**：已有 `shape:` / `variant:` / `icon:` 不被覆盖
2. 展开后 resolve 只看见三轴；主题与引擎不按 archetype 名分支
3. 目录真源是 CSV → 静态表，不是主题 JSON

---

### 14.4 Edge 属性

| key | 值 | 状态 | 写者 | 消费者 |
|-----|----|------|------|--------|
| `label` | string | `active`（模型字段） | DSL 作者（或 §7.5 位置糖） | 提升为 `Edge.label`；render 画中点标签 |
| `head_label` | string | `active`（模型字段） | DSL 作者 | 提升为 `Edge.head_label`；靠近目标端 |
| `tail_label` | string | `active`（模型字段） | DSL 作者 | 提升为 `Edge.tail_label`；靠近源端 |
| `variant` | atom（封闭集，见 §14.7） | **`planned`** | DSL 作者 | 主题 `variants` → 边颜料（仅 edge 适用键）；见 style-sheet-spec §6.2 |
| `style.*` | 见 §14.9 | `active` | DSL 作者 | `apply_inline_edge_styles` |
| `from_side` | atom：`north`/`south`/`east`/`west` | **`active`（模型字段）** | DSL 作者（可选约束） | 提升为 `Edge.from_port`（FixedSide）；组合相读约束 |
| `to_side` | 同上 | **`active`（模型字段）** | DSL 作者 | 提升为 `Edge.to_port` |
| `from_slot` / `to_slot` / `from_ratio` / `to_ratio` / `from_x`+`from_y` / `to_x`+`to_y` / `from_sides` / `to_sides` | — | **`removed`** | — | 写了 → 解析错误；边端口只留 side（见 §7.4） |
| `weight` | `Option<f64>`（`critical: true` 糖 = `2.0`） | **`active`（模型字段）** | DSL 作者 | 提升为 `Edge.weight`；Hier 排序/对齐加权。`critical` 与 `weight` 同写 → 解析错误（`CriticalWeightConflict`） |
| `undirected` | boolean | **`active`（模型字段 `Edge.undirected`）** | DSL 作者或 `<->` 糖 | hierarchical rank 建图跳过（FAS/rank 不施加层级）；同 rank 端点由 Ink `intralayer` 路由为 side-link（跨轴直连或避让 U 形）；非 bool → 解析错误（`InvalidUndirected`） |
| `meta.*` | 任意 | — | DSL 作者 | 无 |

`source` / `target` / 箭头语义（`->` / `-->` / `<->`）是**语法**，落在 `Edge::source` / `target` / `arrow`；**不得**在属性块用 `source:` / `target:` / `arrow:` 覆盖。三处标签只经 `label` / `head_label` / `tail_label`（或 §7.5 中点糖）；**已废弃** `>"` / `<"` 端点标记。

**已废弃**：`>"head"` / `<"tail"` 端点糖；位置 string 作为规范中点标签（降为 §7.5 糖）。**不提供** `seq:`（时序用边声明序，§8.1）。

#### 14.4.1 端口 / 自动合流落地状态

| 项 | 状态 |
|----|------|
| `Edge.from_port` / `to_port`（FREE / FixedSide） | **已落地**（plotgram-model） |
| DSL 仅 `from_side`/`to_side` 校验与提升；其余边端口键硬拒绝 | **已落地** |
| 布局 `auto_edge_grouping`（同源/同汇自动合流）；边级 `edge_group` **已移除** | **已落地** |
| `Node.role` / `host_group` / `anchor`（group_anchor；可 FixedOrder） | **已落地**（plotgram-model；见 §5.7 / ADR-004） |
| `Graph.partition` / `Node.partition_cell`（ADR-008） | **已落地**（plotgram-model + `validate_partition`；`cell_*` lift 已接） |
| `partition { column/row … }` 块 parse | **已落地** |
| Hier 消费 PartitionGrid（连续块 / 层区间） | **已落地**（PG-0–PG-4；render 泳道底色后置） |
| `@group` 糖展开（§7.6） | **待 parser** |
| 布局组合相写入 `EdgePlacement` 的 `PortRef`（`side` + `along`：Ordered / LocalOffset） | **已落地**（Hierarchical Compose `ports.rs` 唯一写者；Metric 展开像素；Ink 零猜测） |
| Ink 零发明端口 / 锚点几何 | **纪律已定**；引擎实现时强制 |

作者已钉死的 `PortConstraint` 在组合相落地前不得被渲染层「猜侧」冒充已决议。

#### 14.4.2 `-->` 的虚线来自主题

`Arrow::Response` 在作者未设 dash 时套用主题 `edge.response_dasharray`（默认 `6,4`）。写者是 `resolve_edge`，非属性；作者用 `style.stroke_dasharray` / `style.dashed` 可先占位覆盖。

---

### 14.5 Group 属性

| key | 值 | 状态 | 写者 | 消费者 |
|-----|----|------|------|--------|
| `label` | string | `active`（模型字段） | DSL 作者（或 §6.5 位置糖） | 提升为 `Group.label`；render 画组标题 |
| `variant` | atom（封闭集，见 §14.7） | **`planned`** | DSL 作者 | 主题 `variants` → 组框颜料（仅 group 适用键）；见 style-sheet-spec §6.2 |
| `style.*` | 见 §14.9 | `active` | DSL 作者 | `apply_inline_group_styles` |
| `layout` | algorithm_config | **`planned`** | DSL 作者（组内布局 hint） | **无**。引擎不读 attrs |
| `meta.*` | 任意 | — | DSL 作者 | 无 |

group 的花括号同时承载**组级属性**与**成员**；组级键与 node 属性块同形（`key: value`），但**不**引入 `shape` / `icon` / `archetype`。

**已废弃**：`group id "标签"` 作为规范写法（降为 §6.5 糖）；组标题不再是 id 后的必填语法位。

---

### 14.6 Shape 封闭集

12 个，标准与 sketch 两条渲染路径**完全一致**（`shapes.rs::shape_svg` / `outline.rs::shape_outlines`）：

```
rect            rounded_rect    circle          diamond
cylinder        hexagon         stadium         person
parallelogram   document        cloud           subprocess
```

| 规则 | 说明 |
|------|------|
| 未识别值 | **解析错误**（封闭集，不静默回退） |
| 缺省值 | 见 §14.3.2 链条，最终兜底 `rounded_rect`（**不是** `rect`） |
| 半径语义 | `rect` 默认 `radius: 0`；`rounded_rect` 默认 `8`；主题 `defaults.node.radius` 可覆盖 |
| 与 icon | 部分 shape 会否决 icon，见 §14.3.1 |

`none` / `transparent` 不是 shape，是 `fill` 的取值（用于跳过 sketch hatch 填充）。

---

### 14.7 Variant 封闭集

`variant` 是**封闭集**：表达元素在图中的**视觉层级 / 强调**（node / group / edge 共用），不表达领域实体类型。灵感对齐 Bootstrap 的 theme color 槽位（`primary` / `secondary` …）：键名稳定，具体颜料由主题定义。

#### 14.7.1 v1 核心集（5 个）

| variant | 含义 | 典型用法 |
|---------|------|----------|
| `default` | 普通节点，无特殊强调 | 一般步骤 / 服务；**省略 `variant:` 等同此值** |
| `primary` | 图里最重要、最抢眼 | 核心服务、主路径、导图根；**主泳道标题区**；**主业务流边** |
| `secondary` | 重要但非 C 位 | 辅助服务、分支；**响应/回边**（常配 `-->`） |
| `muted` | 弱化、背景化 | 外部系统、过期组件、注释性节点；**泳道 / 组框背景**；**弱依赖边** |
| `info` | 中性强调（不偏正负） | 数据 / 基础设施类节点（常配 `icon:`） |

#### 14.7.2 规则

| 规则 | 说明 |
|------|------|
| 适用范围 | **node**、**group** 与 **edge** 共用 §14.7.1 封闭集；resolve 时各取词表中适用于该元素的颜料字段 |
| 未识别值 | 回退为 `default`，不报错（诊断模式可 warn） |
| 主题缺键 | 主题必须为上述 5 个槽位各提供样式块；缺一则 **compile 报错** |
| 块内容 | 与 `defaults.node` 同形的**颜料**字段；**禁止** `shape`；group resolve 忽略 node 专有项（如 `font` 若 group 无对应字段） |
| 与 status | `success` / `warning` / `danger` **不**进 v1 封闭集（预留给扩展或 `status` 映射，避免与运行态撞语义） |
| 与 appearance | `outline` / `subtle` **不是** variant 值；若需要，另立 `appearance`（本规范暂不引入） |

#### 14.7.3 与 archetype 的关系

旧 `kind` 领域词的一键便利由 **archetype** 承接（[`archetype-spec.md`](archetype-spec.md)），不再用「文档惯例手写三轴」代替内置目录。手写三轴始终合法，用于覆盖或未入库的组合。

---

### 14.8 Icon 目录

83 个图标（6 个分类：`people` / `databases` / `messaging` / `services` / `cloud` / `generic`），资源内嵌于 `plotgram-render/assets/glyphs/<category>/<id>.svg`。id 与别名以 `icons/catalog.rs::ICONS` 为准，本文档不复制清单（避免第二份真源）；浏览用 `assets/glyphs/index.html`。

---

### 14.9 `style.*`：唯一一条规则

> **`style.<prop>`** —— `<prop>` 取自 [`style-sheet-spec.md`](style-sheet-spec.md) §5 属性词表中**标记为该元素可内联**的属性。  
> 值类型、取值范围、cascade 优先级均以该词表为准，本文档不重复。

配套约束：

1. 无前缀语义键**不得**使用视觉属性名（不允许 `fill: red`，必须 `style.fill: red`）
2. 词表中未标「可内联」的属性写成 `style.*` 会被静默忽略（如 `style.shape`、`style.label_bg`）。几何用标准键 **`shape:`**，文案用 **`label:`**
3. 内联值是**字面量**，不解析 `{token.path}`——token 只在主题编译期展开

---

### 14.10 与其它规范的边界 · 迁移

```
dsl-spec §1–§13   语法形态（怎么写）
dsl-spec §14      属性注册表（写者/消费者/状态、shape / variant 封闭集）
archetype-spec    archetype 展开纪律、CSV、编译进二进制
style-sheet-spec  视觉属性词表 + 主题 JSON + cascade
```

一个属性只在**一处**被定义。跨文档只允许引用，不允许复制表格。

#### 14.10.1 从 `kind` 迁出（对照）

| 旧 | 新 |
|----|----|
| `kind: database` | **`archetype: database`**（或显式 `shape` / `variant` / `icon`） |
| 位置标签 + `: shape` | 规范：`label:` + `shape:` 进 `{}`；轻糖见 §5.5 |
| `kind_styles` | `variants`（键为 §14.7 封闭集；**无** `shape`） |
| `kind_styles.*.shape` | 删除；改走 §14.3.2 shape 链 |
| `KIND_ICON_MAP` | 删除；图标来自 archetype 展开或显式 `icon:` |
| `Node::kind()` | `Node::variant()`（或 attrs `variant`）；archetype 仅展开期存在 |
| `group id "标签"` | `group id { label: "标签", … }`；位置 string 为 §6.5 糖 |
| `a -> b "中点"`（规范）+ `>"1" <"N"` | `a -> b { label, head_label, tail_label }`；中点 string 为 §7.5 糖 |

#### 14.10.2 待办

| # | 动作 | 目的 |
|---|------|------|
| 1 | render：`kind_styles` → `variants`；`KindStyle` 删 `shape`；resolve 按 §14.3.2 / §14.3.3 | 兑现三轴 |
| 2 | 删 `KIND_ICON_MAP`；`resolve_icon` 只认显式 `icon:`（含 archetype 已写入的） | 去掉从属推断 |
| 3 | 主题 JSON 全部改写为 5 槽 `variants` | 与 §14.7 对齐 |
| 4 | 落地 archetype CSV + codegen + 展开器（见 [`archetype-spec.md`](archetype-spec.md) §9） | 一键领域便利 |
| 5 | model / dsl 示例：`kind` → `archetype` / 显式三轴 | 文档与代码一致 |
| 6 | 把 shape 封闭集提为常量 + 表驱动 test | §14.6 以常量为准 |
| 7 | variant 封闭集常量 + 主题 compile 缺键即错 | §14.7 机械锁定 |
| 8 | 引擎组合相：读 `PortConstraint` → 写 `EdgePlacement` 的 `PortRef`；尊重钉死约束 | 兑现 §7.4 写权 |
| 9 | `status` 要么给消费者，要么从本文语法节删除 | 消除纸面属性 |
| 10 | `variant` / `archetype` 消费者落地后标 `active` | 关闭 §14.1.3 漂移 |
| 11 | parser：边废除 `label_marker`；三标签提升；`group` / `edge` 位置 label 糖；调用 `lift_structural_attrs` | §6 / §7 / ADR-003 |
| 12 | resolve：`group.variant` / `edge.variant` cascade | style-sheet §6 |

---

## 附录 A：相对 language-spec 的主要变更

| 项 | 旧（0.4） | 新（2.4-draft） |
|----|-----------|-----------------|
| 节点关键字 | `entity` | `node` |
| 节点表面 | 位置标签 + 可选后缀 | **规范** `node id { label/shape/… }`；轻糖见 §5.5 |
| 组表面 | `group id "标签"` | **规范** `group id { label/variant/… }`；轻糖见 §6.5 |
| 边表面 | 中点 string + `>"1" <"N"` | **规范** `a -> b { label/head_label/tail_label/… }`；中点 string 糖见 §7.5 |
| 语义 / 视觉标注 | `entity[database]` / `type:` / 曾用 `kind:` | **`archetype:`** 或显式 **`variant:`** + **`icon:`**；废弃 `kind` |
| 形状 | 多由 type / kind 推断 / style.shape | 属性块 **`shape:`**；与 variant 正交；archetype 可填缺省 |
| 边端口 | 无（落笔易猜中点） | **`Edge.from_port`/`to_port`**；DSL 四键提升；决议在 `EdgePlacement` |
| 边组 / 总线 | 无 | 布局 **`auto_edge_grouping`**（无边级 `edge_group`） |
| 时序时间 | （图种特判） | **边声明序**；无 `seq` 字段（§8.1） |
| 声明式样式 | `node_style` / `edge_style` | **删除**；仅内联 `style.*` |
| ER `field` | （旧亦弱） | **本草案不做**；另文 |
| 自环 | decision 例外 | 默认禁；须显式 `profile:` 可开（隐式 flowchart 算法默认不含约束包） |
| diagram → 引擎 | 易渗入图名 | `profile:` 展开后引擎不收 profile / 图种名 |

旧文档 `language-spec.md`（v1，已删除）不再可对照，**以本文件为准**。

### A.1 2.0 → 2.1

| 项 | 2.0 | 2.1 |
|----|-----|-----|
| 节点外观 | `kind` + `: shape`（kind 可推 shape / icon） | `shape` × `variant` × `icon` 三轴；可选 `archetype` |
| 主题查表 | `kind_styles` | `variants`（见 style-sheet-spec 2.2） |
| 领域一键 | `kind: database` | `archetype: database`（CSV → 二进制，见 archetype-spec） |

### A.2 2.1 → 2.2

| 项 | 2.1 | 2.2 |
|----|-----|-----|
| 规范形态 | `node id "label" : shape { … }` | **`node id { label / shape / … }`** |
| 形状写法 | 声明后缀 `: shape` | 属性块 **`shape:`** |
| 标签写法 | 位置 string（规范） | 属性块 **`label:`**；位置 string 降为 §5.5 糖 |
| 空节点 | `node id` 或带后缀 | `node id {}`；糖允许省块 |

### A.3 2.2 → 2.3

| 项 | 2.2 | 2.3 |
|----|-----|-----|
| group 规范形态 | `group id "标签" { … }` | **`group id { label / variant / … ; members… }`** |
| group 标签 | id 后必填 string | 属性 **`label:`**；位置 string 降为 §6.5 糖 |
| group 颜料 | 仅 `style.*` + `defaults.group` | 可选 **`variant:`**（与 node 共用 §14.7 封闭集） |
| group 几何轴 | — | **无** shape / icon / archetype |

### A.4 2.3 → 2.4

| 项 | 2.3 | 2.4 |
|----|-----|-----|
| edge 规范形态 | `a -> b "标签" { … }`；`>"1" <"N"` 端点糖 | **`a -> b { label / head_label / tail_label / … }`** |
| edge 中点标签 | 位置 string（规范） | 属性 **`label:`**；位置 string 降为 §7.5 糖 |
| edge 端点标签 | `>"head"` / `<"tail"` | **`head_label:`** / **`tail_label:`**（废除端点糖） |
| 空边 | 隐式无块 | **`a -> b`** ≡ 空属性块 |
| edge 颜料 | 仅 `style.*` + `defaults.edge` | 可选 **`variant:`**（与 node 共用 §14.7 封闭集） |

### A.5 2.4 → 2.5

| 项 | 2.4 | 2.5 |
|----|-----|-----|
| 组间边 | 仅连组内 node | **`group_anchor`** + `@group` 糖（§5.7 / §7.6） |

### A.6 2.5 → 2.6

| 项 | 2.5 | 2.6 |
|----|-----|-----|
| diagram 头 | `diagram <type> { … }` | **`diagram { … }`** |
| 图种预设 | 位置 type | 属性 **`profile:`**（封闭集同旧 type） |
| 位置糖 | — | **不做** `diagram flowchart {` |
