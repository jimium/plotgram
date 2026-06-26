# DSL 属性设计优化方案

> 状态：讨论中
> 创建：2026-06-19
> 范围：`AttributeValue` 类型、属性命名空间、属性 key 命名、parser/validation 策略

---

## 一、现状概览

### 1.1 属性命名空间（3 层）

`AttributeMap`（[ast.rs](../../crates/drawify-core/src/ast.rs)）包含三个命名空间：

| 命名空间 | DSL 写法 | 用途 |
|---|---|---|
| `standard` | `type: service` | 结构/语义属性 |
| `meta` | `meta.author: "xxx"` | 自由元数据 |
| `style` | `style.fill: "#xxx"` | 视觉样式属性 |

### 1.2 `AttributeValue` 变体（6 个）

```rust
pub enum AttributeValue {
    String(String),          // 引号字符串
    Number(f64),             // 数字
    Boolean(bool),           // true / false
    Atom(String),            // 无引号标识符（推荐用于枚举）
    Enum(String),            // 已废弃，语义等同 Atom
    Config { algo, options },// 算法配置块
}
```

### 1.3 各层级合法 standard 属性

**Diagram 级**（[diagram.rs](../../crates/drawify-core/src/types/standard_attr_keys/diagram.rs)）：

| key | 值类型 | 说明 |
|---|---|---|
| `title` | String | 标题 |
| `layout` | Atom | 布局方向（top-to-bottom / left-to-right / radial） |
| `layout_algo` | Atom/Config | 布局算法 |
| `edge_routing` | Atom/Config | 边路由算法 |
| `theme` | Atom | 主题 |
| `graphic_style` | Atom | 图形风格 |
| `group_sizing` | Atom | 分组尺寸策略 |
| `snap` | Boolean | 网格吸附开关 |

**Entity 级**（[entity.rs](../../crates/drawify-core/src/types/standard_attr_keys/entity.rs)）：

| key | 值类型 | 说明 |
|---|---|---|
| `type` | Atom | 实体类型 |
| `status` | Atom | 状态（healthy/degraded/down/unknown） |
| `semantic` | Atom | 语义标签 |
| `icon` | Atom | 图标 id |
| `owner` | String | 负责人 |
| `description` | String | 描述 |

**Group 级**（[group.rs](../../crates/drawify-core/src/types/standard_attr_keys/group.rs)）：

| key | 值类型 | 说明 |
|---|---|---|
| `style` | Atom | 边框线型（solid/dashed/dotted） |
| `color` | String | 分组颜色 |
| `layout` | Atom | 分组内布局算法 |

**Relation 级**（[relation.rs](../../crates/drawify-core/src/types/standard_attr_keys/relation.rs)）：

| key | 值类型 | 说明 |
|---|---|---|
| `status` | Atom | 状态 |
| `edge_style` | Atom | 边样式 |
| `cardinality` | String | 基数标注 |
| `head_label` | String | 箭头头标签（特殊处理） |
| `tail_label` | String | 箭头尾标签（特殊处理） |

---

## 二、问题诊断

### 问题 1：`Atom` / `Enum` 冗余变体

`Enum` 标记为废弃但仍在代码中存在。`as_atom()` 必须同时匹配两者，序列化泄漏历史（`{$atom:...}` vs `{$enum:...}`）。

- 位置：[ast.rs#L119](../../crates/drawify-core/src/ast.rs#L119)
- 影响：所有消费者需处理冗余分支

### 问题 2：String vs Atom 的心智负担

用户必须记住哪些 key 用引号、哪些不用，无规律可循：

| key | 值类型 | 写法 |
|---|---|---|
| `title` | String | `"xxx"` |
| `theme` | Atom | `builtin.clean-light` |
| `color` | String | `"#FF0000"` |
| `type` | Atom | `service` |
| `owner` | String | `"张三"` |
| `status` | Atom | `healthy` |

`color` 必须是 String 但 `theme` 必须是 Atom；`owner` 是 String 但 `status` 是 Atom。

### 问题 3：`style` 关键词严重重载

同一个词 `style` 在四个不同语境下含义不同：

| 语境 | 含义 | 位置 |
|---|---|---|
| `style: solid` | group 边框线型 | [group.rs#L8](../../crates/drawify-core/src/types/standard_attr_keys/group.rs#L8) |
| `style.fill: "#xxx"` | 内联视觉样式命名空间 | AttributeMap.style |
| `node_style service { ... }` | 声明式样式规则 | StyleDecl |
| `graphic_style: standard` | 图形风格 | diagram 级 |

### 问题 4：`layout` 语义歧义

- diagram 级 `layout: top-to-bottom` → 布局**方向**
- diagram 级 `layout_algo: sugiyama` → 布局**算法**
- group 级 `layout: grid` → 分组内布局**算法**

同一个 `layout` 在 diagram 和 group 层级含义不同，且 diagram 层还有个 `layout_algo` 来区分。

### 问题 5：diagram 属性的顺序约束

[parser/mod.rs#L322-L327](../../crates/drawify-core/src/dsl/parser/mod.rs#L322-L327) 强制 diagram 属性必须在 entity/relation/group 之前，否则报错。这是不必要的限制，用户写大图时很容易违反。

### 问题 6：`head_label` / `tail_label` 的 hack

[stmt.rs#L351-L358](../../crates/drawify-core/src/dsl/parser/stmt.rs#L351-L358) 中，这两个属性先被解析为 `attributes.standard`，然后又被 `remove` 出来提升为 `Relation` 的顶层字段。既不是纯粹的属性，也不是纯粹的语法结构。

### 问题 7：key→解析策略的硬编码 match

[expr.rs#L113-L133](../../crates/drawify-core/src/dsl/parser/expr.rs#L113-L133) 用一个巨大的 match 来决定每个 key 的值解析方式。新增属性需要同时改这里、改 validation、改 profile，三处分散。

### 问题 8：未知 key 的 fallback 过于宽松

属性块内未知 key 走 `parse_attribute_value()`（[expr.rs#L132](../../crates/drawify-core/src/dsl/parser/expr.rs#L132)），接受任意值类型，直到 validation 阶段才报错。Parser 阶段应该就能拒绝。

---

## 三、优化方案

### 方案 A+B：合并 `Atom` / `Enum` / `String` 为带 `quoted` 标记的单一变体

**目标**：消除 `Atom` / `Enum` / `String` 三个变体，统一为一个带来源标记的 `String` 变体；同时让所有文本类 key 同时接受引号和无引号两种写法。

**背景**：

当前 `AttributeValue` 有三个语义重叠的文本变体：

```rust
pub enum AttributeValue {
    String(String),   // 引号文本
    Atom(String),     // 无引号标识符
    Enum(String),     // 已废弃，语义等同 Atom
    // ...
}
```

三者内部都是 `String`，区别仅在"来源是否带引号"。但下游消费者真正关心的不是"带没带引号"，而是"值内容是否合法"。

**提议**：

合并为带 `quoted` 标记的单一变体：

```rust
pub enum AttributeValue {
    String { value: String, quoted: bool },   // 合并 Atom + Enum + String
    Number(f64),
    Boolean(bool),
    Config { algo: String, options: HashMap<String, AttributeValue> },
}
```

- `quoted: false` — 原始写法无引号（原 `Atom`），如 `type: service`
- `quoted: true` — 原始写法有引号（原 `String`），如 `title: "hello world"`

**Parser 改动**：

所有文本类 key 统一使用 `parse_text_value`，同时接受引号和无引号：

```rust
fn parse_text_value(&mut self) -> Option<AttributeValue> {
    match self.peek_kind().clone() {
        TokenKind::StringLit(s) => {
            self.advance();
            Some(AttributeValue::String { value: s, quoted: true })
        }
        TokenKind::Ident(_) => {
            let atom = self.read_atom_segment()?;
            Some(AttributeValue::String { value: atom, quoted: false })
        }
        _ => { /* error */ }
    }
}
```

这样 `title: hello` 和 `title: "hello"` 都合法，`type: service` 和 `type: "service"` 都合法。

**辅助方法简化**：

```rust
impl AttributeValue {
    /// 取文本值（替代原 as_atom + as_text）
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String { value, .. } => Some(value.as_str()),
            _ => None,
        }
    }

    pub fn is_quoted(&self) -> bool {
        match self {
            Self::String { quoted, .. } => *quoted,
            _ => false,
        }
    }

    pub fn algorithm_name(&self) -> Option<&str> {
        match self {
            Self::String { value, .. } => Some(value.as_str()),
            Self::Config { algo, .. } => Some(algo.as_str()),
            _ => None,
        }
    }
}
```

`as_atom()` + `as_text()` 合并为一个 `as_str()`。

**JSON 序列化**：

`quoted` 字段不参与 JSON 序列化——JSON 是数据交换格式，`quoted` 只对 DSL formatter 有意义：

```rust
// 序列化：忽略 quoted，输出裸字符串
// String { value: "service", quoted: false } → "service"
// String { value: "hello",   quoted: true  } → "hello"

// 反序列化：quoted 默认 true（安全兜底，formatter 保守加引号）
// "service" → String { value: "service", quoted: true }
```

往返保真度：

| 路径 | `quoted` 保真 | 说明 |
|---|---|---|
| DSL → AST → DSL | ✅ | parser 设置 quoted，formatter 读取 |
| DSL → AST → JSON → AST → DSL | ❌ | JSON 丢失 quoted，反序列化默认 true |
| JSON → AST → JSON | ✅ | 不涉及 quoted |

**Formatter 支持**：

未来实现 AST → DSL formatter 时，根据 `quoted` 决定是否加引号：

```rust
fn format_string(value: &str, quoted: bool) -> String {
    if quoted {
        format!("\"{}\"", value)
    } else {
        value.to_string()
    }
}
```

无需靠 `is_valid_atom()` 猜测，也不会把 `String("true")` 误输出为无引号的 `true`（会被 lexer 误认为 Boolean）。

**改动范围**：

| 范围 | 内容 | 量级 |
|---|---|---|
| `ast.rs` | 删除 `Atom`/`Enum` 变体、`serialize_atom`/`serialize_enum`；`String` 改为 struct variant；`as_atom`/`as_text` 合并为 `as_str` | 核心 |
| `parser/expr.rs` | `parse_value_for_standard_key` 简化：所有文本 key 走 `parse_text_value` | 中 |
| 全局 match 替换 | `AttributeValue::String(s)` → `String { value: s, .. }`；`Atom(s)` / `Enum(s)` → 同上 | 221 处 / 45 文件 |
| `as_atom()` / `as_text()` 调用 | 替换为 `as_str()` | 17 处 / 10 文件 |
| diff / 测试快照 | JSON 格式变化（`{$atom: "x"}` → `"x"`） | 需更新 |

**风险**：中。改动量大但模式统一，编译器能抓到所有遗漏。主要风险在 diff 引擎和测试快照的 JSON 格式变化。

---

### 方案 C：重命名消除歧义

**目标**：语义清晰，降低学习成本。

| 当前 | 提议 | 理由 |
|---|---|---|
| diagram `layout` | `direction` | 它就是方向，不是布局 |
| diagram `layout_algo` | `layout` | 这才是真正的布局算法 |
| group `style` | `border_style` | 避免与 `style.*` 命名空间冲突 |
| diagram `graphic_style` | `render_style` | 避免与 `style` 混淆 |
| relation `edge_style` | `line_style` | 与 group 的 `border_style` 对称 |

改后示例（结合方案 A+B 的 `quoted` 标记和方案 D 的 config block + title 位置参数）：

```dfy
diagram architecture "AI Agent 文档自动化管线" {
    config {
        direction: left-to-right      # 原 layout
        layout: sugiyama              # 原 layout_algo
        edge_routing: bezier { tension: 0.55 }
        theme: builtin.clean-light
        render_style: standard        # 原 graphic_style
        group_sizing: auto
        snap: true
    }

    group inputs "输入源" {
        border_style: dashed          # 原 style
        entity repo "代码仓库" { type: storage }
    }
    repo -> orchestrator "code context"
}
```

**改动范围**：
- `types/standard_attr_keys/*.rs`：修改常量值
- 全局：替换所有引用（含 .dfy 文件）

**风险**：中。需要批量更新所有 .dfy 示例文件和 benchmark。

---

### 方案 D：引入 `config` block 隔离 diagram 属性（替代原方案 D）

**目标**：将 diagram 级属性从 body 中隔离出来，消除 parser 歧义和顺序约束。

**现状问题**：

```dfy
diagram architecture {
    title: "..."           # ← ident: value，靠 lookahead 区分
    layout: top-to-bottom  # ← 同上
    group inputs "..." { ... }
    entity foo "Foo" { type: service }
    repo -> orchestrator "code context"   # ← ident -> ident
}
```

diagram body 中 `ident:`（属性）和 `ident ->`（关系）共存，parser 靠 `lookahead_is_attribute()` 逐 token 预判区分（[mod.rs#L460-L466](../../crates/drawify-core/src/dsl/parser/mod.rs#L460-L466)）。由此衍生：
- 顺序约束：属性必须出现在 entity/group/relation 之前（[mod.rs#L322-L327](../../crates/drawify-core/src/dsl/parser/mod.rs#L322-L327)）
- parser 脆弱：未来新增 `ident` 开头的语句类型会加剧歧义

**提议**：两部分改动——

**1. `title` 提升为 diagram 声明的位置参数**

entity 和 group 的 label 都是位置参数（`entity foo "Foo"`、`group inputs "输入源"`），但 diagram 的 title 却埋在 attributes 里。将 title 提升为位置参数，与 entity/group 对称：

```dfy
# 现状
diagram architecture {
    title: "AI Agent 文档自动化管线"
    ...
}

# 提议
diagram architecture "AI Agent 文档自动化管线" {
    ...
}
```

- title **可选**：无标题时省略字符串
- AST 变化：`Diagram` 新增 `title: Option<String>` 一等字段，删除 `Diagram::title()` 遍历 attributes 的 hack（[ast.rs#L638-L647](../../crates/drawify-core/src/ast.rs#L638-L647)）
- `title` 不再出现在 config block 中

**2. 引入 `config` block 承载剩余 diagram 级属性**

```dfy
diagram architecture "AI Agent 文档自动化管线" {
    config {
        direction: left-to-right
        layout: sugiyama
        edge_routing: bezier { tension: 0.55 }
        theme: builtin.clean-light
        render_style: standard
        group_sizing: auto
        snap: true
    }

    group inputs "输入源" { ... }
    entity foo "Foo" { type: service }
    repo -> orchestrator "code context"
}
```

**收益**：

| 问题 | 解决方式 |
|---|---|
| 顺序约束（问题 5） | config block 自然分组，无需强制顺序 |
| parser lookahead 歧义（问题 7 局部） | body 顶层只剩 `entity`/`group`/`node_style`/`edge_style`/关系，无 `ident:` 歧义 |
| 可读性 | 配置与内容分离，大图更清晰 |

**设计决策**：

1. **config block 是否可选？**
   - ✅ 可选：无 diagram 属性时省略 `config { }`

2. **config block 是否允许多次出现？**
   - ✅ 不允许：至多一个，出现多次报错

3. **config block 出现位置？**
   - ✅ 任意位置：`config { }` 可在 body 内任意位置

4. **block 名称？**
   - ✅ `config`

5. **title 是否作为 config block 的例外？**
   - ✅ title 不进 config block，提升为 diagram 声明的位置参数
   - 语法：`diagram <type> "<title>" { ... }`，与 `entity <id> "<label>"` / `group <id> "<label>"` 对称
   - title 可选：`diagram flowchart { ... }` 合法
   - AST：`Diagram.title: Option<String>` 一等字段，删除 `Diagram::title()` 遍历 hack

**改动范围**：
- `lexer.rs`：新增 `Config` 关键字 token
- `parser/mod.rs`：
  - `parse_diagram_type` 后新增可选 title 字符串解析
  - `parse_diagram_body_inner` 新增 `TokenKind::Config` 分支
  - 删除 `seen_non_attr` 逻辑和 `lookahead_is_attribute` 中对 diagram 属性的判断
- `parser/stmt.rs`：新增 `parse_config_block`，复用现有 `parse_diagram_attribute`
- `ast.rs`：`Diagram` 新增 `title: Option<String>` 字段，删除 `title()` 方法和 `diagram::TITLE` 常量
- `validation/common.rs`：删除 `diagram::TITLE` 的校验分支
- `types/standard_attr_keys/diagram.rs`：删除 `TITLE` 常量

**风险**：中。需改 lexer + parser + AST，下游需适配 `diagram.title` 字段读取方式。需批量更新 .dfy 文件。

**与原方案 D 的关系**：本方案从结构上消除了顺序约束的根因，原方案 D（仅删除 `seen_non_attr`）不再需要。若不采纳 config block，才回退到原方案 D。

---

### 方案 E：`head_label` / `tail_label` 提升为语法级

**目标**：消除 hack，让三个标签位置（mid / head / tail）对称，都是语法级一等公民。

**现状问题**：

一条 relation 有三个标签位置：

```dfy
a -> b "中间标签" {
    head_label: "头部标签"    # near `to` 端
    tail_label: "尾部标签"    # near `from` 端
}
```

- `label`（中间标签）是**语法级位置参数**，直接解析为 `Relation.label` 字段
- `head_label` / `tail_label` 走属性块 → 再被 `remove` 挖出来的 hack 路径（[stmt.rs#L351-L358](../../crates/drawify-core/src/dsl/parser/stmt.rs#L351-L358)）

Hack 数据流：

```
DSL: { head_label: "H" }
  → parse_attribute_block 存入 attributes.standard["head_label"]
    → parse_relation 做 attributes.standard.remove("head_label")
      → 移到 Relation.head_label 顶层字段
        → attributes.standard 中已缺失 head_label
```

这导致：
1. `head_label` / `tail_label` 是唯一被特殊对待的属性，其他属性（`status`、`line_style` 等）都留在 `attributes.standard`
2. 消费方两套路径：读 `head_label` 用 `relation.head_label`，读 `status` 用 `relation.attributes.standard.get("status")`
3. 未来 formatter 需要特殊处理：`label` 输出到箭头后，`head_label` 输出到 `{ }` 块

**提议**：用方向符号 `>` 和 `<` 提升为语法级

```dfy
# 现状（属性块 hack）
a -> b "中间标签" { head_label: "H", tail_label: "T" }

# 提议（方向符号）
a -> b "中间标签" >"H" <"T"
```

语义：
- `>"H"` — `>` 指向 `to` 端（head label），视觉上"向前"
- `<"T"` — `<` 指向 `from` 端（tail label），视觉上"向后"
- 两者都可选，顺序不限

更多示例：

```dfy
# 只有中间标签（最常见）
a -> b "mid"

# 只有 head label
a -> b "mid" >"H"

# 只有 tail label
a -> b "mid" <"T"

# 两者都有，顺序不限
a -> b "mid" >"H" <"T"
a -> b "mid" <"T" >"H"

# 无中间标签但有 head/tail
a -> b >"H" <"T"

# 仍有属性块用于其他属性
a -> b "mid" >"H" <"T" { status: healthy }
```

**收益**：

| 维度 | 效果 |
|---|---|
| **视觉直觉** | `>` 和 `<` 本身就是方向，不需要记 head/tail 哪个在哪端 |
| **对称性** | 和中间标签 `"mid"` 一样是裸字符串，只是多了方向前缀 |
| **简洁性** | 只想要 head：`a -> b "mid" >"H"`，一个符号搞定 |
| **hack 消除** | 不再需要 `attributes.standard.remove()` |
| **formatter 友好** | 三个标签都是位置参数，统一处理 |
| **无歧义** | lexer 中 `>` 和 `<` 当前不是独立 token（`<` 只出现在 `<->`），relation 上下文中不会混淆 |

**Lexer 改动**：

新增 `>` 和 `<` 作为独立 token（当前 `<` 仅出现在 `<->`，`>` 不存在）。在 relation 解析上下文中，读完 `to` 和可选的 mid label 后，遇到 `>` 或 `<` 即为 head/tail label：

```rust
// 伪代码
let head_label = if matches!(self.peek_kind(), TokenKind::Gt) {
    self.advance();
    let (s, _) = self.expect_string()?;
    Some(s)
} else { None };

let tail_label = if matches!(self.peek_kind(), TokenKind::Lt) {
    self.advance();
    let (s, _) = self.expect_string()?;
    Some(s)
} else { None };
```

**AST 变化**：

`Relation` 结构体保持 `head_label` / `tail_label` 顶层字段不变，但赋值来源从"属性块 remove"改为"语法级直接解析"。

**改动范围**：
- `lexer.rs`：新增 `Gt` / `Lt` token（或复用现有 `<` 逻辑）
- `parser/stmt.rs`：删除 `attributes.standard.remove()` hack，新增 `>` / `<` 解析
- `types/standard_attr_keys/relation.rs`：删除 `HEAD_LABEL` / `TAIL_LABEL` 常量
- `validation/common.rs`：删除 `head_label` / `tail_label` 的属性校验分支
- 全局：更新所有 .dfy 文件中的 `head_label:` / `tail_label:` 写法

**风险**：中。需改 lexer + parser，但 AST 结构不变。需批量更新 .dfy 文件。

---

### 方案 F：引入属性 schema 注册表 + 枚举值常量

**目标**：架构层面可扩展性，新增属性只需改一处；枚举值集中管理，消除魔法字符串。

**两部分改动**：

**1. 属性 schema 注册表**

将分散在 parser/validation/profile 三处的属性定义集中为声明式 schema：

```rust
struct AttrSchema {
    key: &'static str,
    scope: AttrScope,           // Diagram | Entity | Group | Relation
    value_type: AttrValueType,  // Text | Atom | Number | Boolean | AlgorithmConfig
    enum_values: Option<&'static [&'static str]>,  // 闭集校验，引用常量
}

const DIAGRAM_ATTRS: &[AttrSchema] = &[
    AttrSchema { key: "direction", scope: Diagram, value_type: Atom,
                 enum_values: Some(direction::ALL) },
    AttrSchema { key: "layout", scope: Diagram, value_type: AlgorithmConfig, enum_values: None },
    // ...
];
```

Parser 根据 schema 选择解析策略，validation 根据 schema 校验值域，profile 根据 schema 过滤合法 key。

**2. 枚举值常量文件**

将当前散落在 validation 代码中的硬编码字符串集中到一个常量文件，方便集中查看和引用：

```rust
// types/attr_constants.rs

pub mod entity_type {
    pub const SERVICE: &str = "service";
    pub const STORAGE: &str = "storage";
    pub const QUEUE: &str = "queue";
    pub const DATABASE: &str = "database";
    pub const PROCESSOR: &str = "processor";
    pub const EXTERNAL: &str = "external";
    pub const ACTOR: &str = "actor";

    pub const ALL: &[&str] = &[SERVICE, STORAGE, QUEUE, DATABASE, PROCESSOR, EXTERNAL, ACTOR];
}

pub mod status {
    pub const HEALTHY: &str = "healthy";
    pub const DEGRADED: &str = "degraded";
    pub const DOWN: &str = "down";
    pub const UNKNOWN: &str = "unknown";

    pub const ALL: &[&str] = &[HEALTHY, DEGRADED, DOWN, UNKNOWN];
}

pub mod direction {
    pub const TOP_TO_BOTTOM: &str = "top-to-bottom";
    pub const LEFT_TO_RIGHT: &str = "left-to-right";
    pub const RADIAL: &str = "radial";

    pub const ALL: &[&str] = &[TOP_TO_BOTTOM, LEFT_TO_RIGHT, RADIAL];
}
```

消费方引用常量，消除魔法字符串：

```rust
// 现状：魔法字符串散落各处
if entity_type == "service" { ... }

// 改后：常量引用
if entity_type == entity_type::SERVICE { ... }
```

**模块分工**：

| 模块 | 内容 | 示例 |
|---|---|---|
| `standard_attr_keys/` | key 名称常量 | `TITLE`, `LAYOUT`, `TYPE` |
| `attr_constants.rs` | value 枚举常量 | `entity_type::SERVICE`, `status::HEALTHY` |
| `attr_schema.rs` | schema 定义（key + scope + value_type + enum_values） | `AttrSchema { key, scope, ... }` |

**改动范围**：
- 新增 `types/attr_schema.rs`、`types/attr_constants.rs`
- 重构 `parser/expr.rs`、`validation/common.rs`、`validation/attrs.rs`、`profile/standard_attrs.rs`
- 全局：将硬编码字符串替换为常量引用
- WASM binding：暴露 schema 给 playground 前端（见下文）

**WASM 暴露 schema 作为 playground 唯一真源**：

schema 定义一次（Rust const），前端编辑器、parser、validation 都从同一来源读取，不会 drift。新增属性只需改 schema 一处，前端编辑器自动获得补全能力。

```rust
#[wasm_bindgen]
pub fn get_attr_schema(scope: &str) -> JsValue {
    // 返回该 scope 下所有合法属性
    // scope: "diagram" | "entity" | "group" | "relation"
    let schema = match scope {
        "diagram" => DIAGRAM_ATTRS,
        "entity" => ENTITY_ATTRS,
        "group" => GROUP_ATTRS,
        "relation" => RELATION_ATTRS,
        _ => return JsValue::NULL,
    };
    serde_wasm_bindgen::to_value(schema).unwrap()
}

#[wasm_bindgen]
pub fn get_enum_values(key: &str) -> Option<Vec<String>> {
    // 返回某个 key 的合法枚举值
    // 例如 get_enum_values("type") → ["service", "storage", ...]
}
```

前端编辑器两层校验分工：

| 层 | 数据来源 | 触发时机 | 职责 |
|---|---|---|---|
| **即时提示** | 缓存的 schema（启动时调 WASM 获取） | 每次按键 | key 补全、枚举值下拉、基本值类型检查 |
| **完整校验** | WASM parser | 防抖 300ms | 完整语法解析、语义校验、diagram-type 收窄 |

流程：

```
编辑器加载 → 调 WASM get_attr_schema → 缓存 schema 到 JS
    ↓
用户输入 key → 用缓存 schema 做自动补全（即时，无延迟）
    ↓
用户输入 value → 用缓存 enum_values 做下拉提示（即时）
    ↓
停顿 300ms → 调 WASM parser 做完整语法校验（防抖）
```

**风险**：高。跨层重构，需充分测试。但收益是长期可维护性。

---

## 四、优先级建议

| 优先级 | 方案 | 收益 | 改动范围 | 风险 |
|---|---|---|---|---|
| P0 | A+B. 合并文本变体为 `String { value, quoted }` | 消除技术债 + 统一解析 | ast.rs + 全局替换 221 处 | 中 |
| P0 | D. 引入 config block + title 位置参数 | 消除歧义 + 顺序约束 | lexer + parser + .dfy | 中 |
| P1 | C. 重命名消除歧义 | 语义清晰 | 常量定义 + 全局替换 | 中 |
| P2 | F. 属性 schema 注册表 | 可扩展性 | 跨层重构 | 高 |
| P2 | E. head/tail_label 正式化 | 消除 hack | parser/stmt.rs | 低~中 |

---

## 五、讨论记录

> 逐个方案讨论时在此追加结论。

### 方案 A+B：合并文本变体为 `String { value, quoted }` — 已确认

- 日期：2026-06-19
- 决策：采纳，合并原方案 A（删 Enum）和原方案 B（统一文本值解析）
  - 删除 `Atom`、`Enum` 变体，合并为 `String { value: String, quoted: bool }`
  - `quoted: false` 对应原 `Atom`（无引号），`quoted: true` 对应原 `String`（有引号）
  - Parser 统一使用 `parse_text_value`，所有文本类 key 同时接受引号和无引号
  - `as_atom()` + `as_text()` 合并为 `as_str()`
  - JSON 序列化忽略 `quoted`，输出裸字符串；反序列化 `quoted` 默认 `true`
- 效果：
  - 变体 6 → 4
  - 用户不再需要记住哪些 key 用引号
  - 未来 formatter 可根据 `quoted` 标记还原原始写法
- 改动量：221 处 match 替换 / 45 文件，17 处方法调用替换 / 10 文件
- 待办：diff 引擎和测试快照的 JSON 格式需更新（`{$atom: "x"}` → `"x"`）

### 方案 C：重命名消除歧义 — 已确认

- 日期：2026-06-19
- 决策：采纳，5 项重命名
  - diagram `layout` → `direction`
  - diagram `layout_algo` → `layout`
  - group `style` → `border_style`
  - diagram `graphic_style` → `render_style`
  - relation `edge_style` → `line_style`
- 示例已结合方案 A+B 和 D 的结构（config block + title 位置参数）
- 改动范围：常量定义 + 全局替换（含 .dfy 文件）

### 方案 D：引入 config block + title 位置参数 — 已确认

- 日期：2026-06-19
- 决策：采纳，5 项设计决策均确认
  - config block **可选**（无 diagram 属性时可省略）
  - **不允许**多次出现（至多一个，多次报错）
  - 出现位置**任意**
  - 关键字名称 **`config`**
  - **title 提升为位置参数**：`diagram <type> "<title>" { ... }`，不进 config block，与 entity/group 的 label 对称；title 可选
- 效果：
  - 替代原方案 D（仅删除 `seen_non_attr`），从结构上消除顺序约束和 parser 歧义
  - AST 变化：`Diagram` 新增 `title: Option<String>` 一等字段，删除 `title()` hack
- 待办：实现时需同步更新所有 .dfy 文件

### 方案 E：head/tail label 提升为语法级 — 已确认

- 日期：2026-06-19
- 决策：采纳选项 2（语法级），使用方向符号 `>` / `<`
  - `>"H"` — head label（near `to` 端），`>` 视觉上"向前"
  - `<"T"` — tail label（near `from` 端），`<` 视觉上"向后"
  - 两者都可选，顺序不限
  - 仍有属性块用于其他属性：`a -> b "mid" >"H" <"T" { status: healthy }`
- 效果：
  - 消除 `attributes.standard.remove()` hack
  - 三个标签位置（mid / head / tail）对称，都是语法级位置参数
  - formatter 友好，统一处理
- AST 变化：`Relation` 结构体不变，赋值来源从"属性块 remove"改为"语法级直接解析"
- 改动范围：lexer 新增 `Gt`/`Lt` token、parser 删除 hack 并新增方向符号解析、删除 `HEAD_LABEL`/`TAIL_LABEL` 常量、更新 .dfy 文件

### 方案 F：属性 schema 注册表 + 枚举值常量 — 已确认

- 日期：2026-06-19
- 决策：采纳，三部分改动
  1. **属性 schema 注册表**：将分散在 parser/validation/profile 三处的属性定义集中为声明式 `AttrSchema`
  2. **枚举值常量文件**：新增 `types/attr_constants.rs`，集中定义所有枚举值常量（`entity_type::SERVICE`、`status::HEALTHY`、`direction::TOP_TO_BOTTOM` 等），消费方引用常量而非魔法字符串
  3. **WASM 暴露 schema**：通过 `get_attr_schema(scope)` / `get_enum_values(key)` 暴露给 playground 前端编辑器，作为自动补全和即时校验的唯一真源
- 模块分工：
  - `standard_attr_keys/` — key 名称常量
  - `attr_constants.rs` — value 枚举常量
  - `attr_schema.rs` — schema 定义（key + scope + value_type + enum_values）
- playground 两层校验：
  - 即时提示（每次按键）：用缓存的 schema 做 key 补全、枚举值下拉
  - 完整校验（防抖 300ms）：调 WASM parser 做完整语法解析 + 语义校验
- 补充决策：diagram-type-specific 的枚举收窄（如 flowchart 不允许 radial）**不在 schema 层解决**，由 diagram type 自己的语义校验处理
- 改动范围：新增 `types/attr_schema.rs`、`types/attr_constants.rs`，重构 parser/validation/profile，WASM binding，全局替换硬编码字符串
- 风险：高（跨层重构），但收益是长期可维护性
