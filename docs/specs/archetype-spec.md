# Plotgram Archetype 规范

> 版本：1.0 | 状态：现行（规范已定；实现 `planned`）  
> 定位：**节点 archetype 展开糖**的单一真源——目录格式（CSV）、展开纪律、编译进二进制的约定。  
> 语法写法见 [`dsl/dsl-spec.md`](dsl/dsl-spec.md) §5；属性登记与封闭集见同文档 **§14**；主题**不**承载 archetype。

---

## 1. 动机与定位

节点外观的真值是三轴正交（dsl-spec §14）：

```text
shape × variant × icon
```

领域名（`database`、`gateway`…）不适合做 variant 槽位，也不应复活为 `kind`。  
**Archetype** 是建在三轴之上的**命名组合包**：把常用三元组预定义好，作者写一个名字，在上游展开成三轴后消失。

```text
archetype: database
  →  expand
shape: cylinder + variant: info  （icon 见 CSV；可为空）
```

| 是 | 不是 |
|----|------|
| 作者便利 / 展开糖 | 第四个被 resolve / 引擎读取的自由度 |
| 开放集领域名 → 三轴缺省 | 主题颜料键（那是 `variants`） |
| CSV 真源 → 编译进二进制 | 运行时读外部 CSV |

灵感接近 Bootstrap「组件类 = 工具类组合」，展开后只剩底层轴。

---

## 2. 写权纪律（硬约束）

与 AGENTS.md §1「每个自由度有且只有一个写者」对齐：

1. **展开时机**：parse 之后、profile expand 之中（或紧接其后）；**早于** `resolve_graph` 与布局引擎
2. **展开写者**：唯一——archetype 展开器；把包里有值的轴写入节点（见 §4「只填空」）
3. **展开后**：节点上可丢弃 `archetype` 属性，或保留仅供诊断；**render / theme / engine 不得按 archetype 名查表**
4. **主题 JSON**：禁止 `archetypes` 段；换主题只改变 `variants.info` 长什么样，不改变「database 默认是不是圆柱」
5. **引擎**：不读 `archetype`，不读展开前的包名

```text
.pgm
  → parse
  → archetype expand     ← 本规范；写 shape / variant / icon 缺省
  → profile expand       ← 布局默认、自环、可选图种默认 shape（若节点仍无 shape）
  → LayoutContract → engine
  → resolve_graph        ← 只认三轴 + style.* + 主题 variants
```

---

## 3. DSL 表面

属性键：`archetype`（atom）。写法与位置糖见 dsl-spec §5.1 / §5.5。

```plotgram
node db { label: "用户库", archetype: database }
node db "用户库" database
node db "用户库" database mysql
node db "" database
node db2 { label: "奇怪的库", archetype: database, shape: rounded_rect, variant: primary }
node gw { label: "网关", archetype: gateway, icon: none }
```

状态：`planned`（规范已定，无消费者代码前写了不生效）。键登记见 dsl-spec §14。

---

## 4. 展开规则

查表：`id = normalize(archetype)`（小写、trim；`-` → `_`，与 icon 键归一化一致）。

对每一轴：

| 轴 | 规则 |
|----|------|
| **shape** | 若节点**尚无**显式 `shape:`，且 CSV 该行 `shape` 非空 → 写入 shape |
| **variant** | 若节点**尚无**显式 `variant:`，且 CSV 该行 `variant` 非空 → 写入 variant |
| **icon** | 若节点**尚无**显式 `icon:`，且 CSV 该行 `icon` 非空 → 写入 icon |

要点：

- **只填空，不覆盖**：作者已写的轴优先（含位置糖写入的 `icon`）
- CSV 某轴为空 = 该轴不由本包提供（保持未填，交给后续 defaults / profile）
- 显式 `icon: none` 算「已写」，包不得再填 icon
- 未知 archetype 名：不报错，不展开（诊断模式可 warn）
- 包**不得**写入 `style.*`、颜色字面量或 `status`——只引用三轴合法值
- icon 与最终 shape 的兼容性在 resolve 阶段检查（部分 shape 会否决 icon，见 dsl-spec §14）

---

## 5. 目录真源：CSV

### 5.1 文件位置

```text
crates/plotgram-model/assets/archetypes.csv
```

仓库内**有且仅有一份** CSV 真源；清单以该文件为准（本文不复制全表，避免第二份真源）。

### 5.2 列定义

表头必须恰好为（顺序固定）：

```csv
id,shape,variant,icon,note
```

| 列 | 必填 | 说明 |
|----|------|------|
| `id` | 是 | archetype 名；atom 规则；全局唯一 |
| `shape` | 否 | 空 = 不设；非空必须 ∈ shape 封闭集（dsl-spec §14） |
| `variant` | 否 | 空 = 不设；非空必须 ∈ variant 封闭集（dsl-spec §14） |
| `icon` | 否 | 空 = 不设；非空必须是 icon id（**不写 alias**）或字面 `none` |
| `note` | 否 | 人类说明；不进运行时逻辑 |

### 5.3 校验（compile 期）

构建 / `build.rs`（或等价 codegen）读取 CSV 时必须：

1. 表头匹配；无重复 `id`
2. `shape` / `variant` 空或属于对应封闭集
3. `icon` 空、或 `none`、或存在于 icon 目录 id 集
4. 失败 → **编译失败**（不要静默跳过坏行）

### 5.4 现行条目（摘要）

以 `archetypes.csv` 为准。当前内置包括：`database` / `cache` / `queue` / `storage` / `gateway` / `external` / `service` / `client` / `decision` / `start` / `end` / `actor` / `root`。

说明：`database` 默认 `shape: cylinder` 且 **不**默认带 icon——DB 系图标与 `cylinder` 不兼容（resolve 会跳过）；需要品牌图标时显式 `icon:` 并通常同时 `shape: rounded_rect`，或写位置糖后再在块内改 shape。

---

## 6. 编译进二进制

### 6.1 产物

CSV → 静态表，链入二进制（推荐放 `plotgram-model`）：

```rust
// 示意；实现可调整
pub struct ArchetypeDef {
    pub id: &'static str,
    pub shape: Option<&'static str>,
    pub variant: Option<&'static str>,
    pub icon: Option<&'static str>,
}

pub static ARCHETYPES: &[ArchetypeDef] = &[ /* codegen */ ];

pub fn archetype_by_id(id: &str) -> Option<&'static ArchetypeDef>;
```

- **不**在运行时打开 CSV
- WASM / 本地同一份静态表
- `note` 列可不编进二进制，或仅编进诊断工具

### 6.2 确定性

查表与遍历必须稳定序（按 CSV 声明序或按 `id` 排序后固化）；禁止依赖 `HashMap` 迭代序做任何用户可见行为。

### 6.3 与 icon 资源的关系

Icon 字形仍由 `plotgram-render` 内嵌 SVG 提供；archetype 只存 **icon id 字符串**。CSV 校验不得引用不存在的 id。

---

## 7. 与 `kind` / 主题的边界

| 旧 / 其它 | Archetype |
|-----------|-----------|
| `kind: database` | `archetype: database`（展开后等价于显式三轴） |
| `kind_styles` / `variants` | **无关**；主题只定义 variant 颜料 |
| `KIND_ICON_MAP` | **删除**；图标来自 archetype 展开或显式 `icon:` |
| profile 默认 shape | 在 archetype 之后；仅当节点仍无 shape 时写入 |

---

## 8. 非目标

- 用户 `.pgm` 内自定义 archetype 表（可日后另议；v1 仅内置 CSV）
- 按 diagram_type 分多份 CSV
- archetype 驱动布局算法或端口
- 在 archetype 里写 fill/stroke 等颜料字面量

---

## 9. 待办（实现）

| # | 动作 |
|---|------|
| 1 | build.rs：读 CSV → codegen（文件已落地，待接线） |
| 2 | parse/profile：`archetype` 展开（只填空） |
| 3 | 表驱动 test：CSV 每行通过封闭集与 icon id 校验 |
| 4 | 消费者就绪后，dsl-spec §14 将 `archetype` 标为 `active` |
