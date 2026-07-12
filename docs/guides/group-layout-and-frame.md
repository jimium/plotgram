# Group 布局 Hint 与 Group Frame

组内排版与组间宏观几何的**唯一指南**：选项能力、典型配方、调节步骤与排错。

| 层级 | 写在哪里 | 管什么 |
|------|----------|--------|
| **L2 组内** | 每个 `group { layout: … }` | **框里面的节点**怎么排 |
| **L1 组间** | diagram 的 `group_frame` | **多个 group 框**怎么摆、是否等宽、间距与边框 |
| **L3 节点** | diagram `align` / `snap` | rank/layer 结构对齐；边与组框像素量化 |

```text
L1  Group Frame   — 组间：track 等宽、cross 对齐、gap、border
L2  Intra Frame   — 组内：group { layout: auto | horizontal | vertical | fan-out | fan-in | grid }
L3  Node Frame    — 节点：align（路由前）+ snap（路由后）
```

调节「group 框是否对齐」用 **L1**；调节「group 里节点怎么排」用 **L2**。

**上手建议**：先读 [§1 Architecture macro rank](#1-architecture-macro-rank必读)（避免把上下分层误当成左右排），再看 [§5 按场景选用](#5-按场景选用)。

> 设计规格：[group-frame-spec.md](../已经实现的方案/group-frame-spec.md)  
> DSL：[dsl-writing-manual §5.3 / §6.6](../specs/dsl/dsl-writing-manual.md) · [language-spec §4.6 / §7.3](../specs/dsl/language-spec.md)  
> 权威枚举：`attr_constants`（`group_layout` / `group_frame_track`）  
> 实现：`group_layout_hint.rs`（L2）、`layout/group_frame/`（L1）

Group Frame 在**主布局算法之后**执行（路由前、路由后各会幂等恢复一次），通过 DSL `config` 声明即可。

---

## 1. Architecture macro rank（必读）

架构图里「group 从上到下」和「`group_frame` 默认 Stack(水平)」**同时成立**，管的不是同一件事。

### 1.1 谁决定上下？——主布局的 macro rank

`layout: architecture`（两阶段）先根据 **group 之间的边** 给每个顶层 group 算 **macro rank**（宏观层号）：

- 数据/依赖大致从上游指向下游 → rank 增大  
- **rank 沿 Y 轴往下排**（上 = 小 rank，下 = 大 rank）  
- **同一 rank** 里若有多个 group，才在该行内沿 **X 轴左右排**

```text
示例 A：供应链链状（每层 1 个 group）——你看到的是「整列从上到下」

  rank 0   ┌──────────────┐
           │ 上游与计划    │
           └──────┬───────┘
  rank 1   ┌──────▼───────┐
           │ 运营与履约    │
           └──────┬───────┘
  rank 2   ┌──────▼───────┐
           │ 物流协同      │
           └──────┬───────┘
  rank 3   ┌──────▼───────┐
           │ 控制塔能力    │
           └──────────────┘

参考：showcase/architecture/c.supply-chain-control-tower.pgm
（未写 group_frame 时也是这样：上下由拓扑决定，不是 axis 写反了）
```

```text
示例 B：同一 rank 有多个 group ——「行内」才会左右并排

  rank 0   ┌────────┐  ┌────────┐
           │ 网关层  │  │ 边缘层  │     ← 同行，左右排
           └────┬───┘  └───┬────┘
  rank 1        └────┬─────┘
               ┌─────▼─────┐
               │  服务层    │           ← 下一行
               └─────┬─────┘
  rank 2       ┌─────▼─────┐
               │  数据层    │
               └───────────┘
```

**要点**：`group_frame` **不会**把示例 A 的四层竖链「掰成」一横排。上下顺序由 relation 拓扑决定。

### 1.2 那 `axis: horizontal` / 短名 `strips` 管什么？

在 architecture 默认里，`Stack(水平)` 的含义是：

| 说法 | 实际意思 |
|------|----------|
| ❌ 「所有 group 从左排到右」 | 误解 |
| ✅ 「**同一行（同 rank）** 内的兄弟按水平关系处理」 | 正确：等宽、水平间距、同行消重叠 |
| ✅ 「不同行之间仍保持主布局的上下关系」 | 正确：行间只做必要的竖直间距/消重叠 |

`track: equal`（`strips` 含此项）会把**同一 parent 下的兄弟 group** 拉成相同宽度——即使它们在不同 rank（不同行），也会形成「等宽竖条带」观感，而不是改成横排。

```text
strips / 默认 architecture 常见观感（链状分层）：

  ┌─────────────────────┐  ← 等宽
  │      上游与计划       │
  └─────────────────────┘
  ┌─────────────────────┐  ← 与上行同宽
  │      运营与履约       │
  └─────────────────────┘
           ⋮
```

### 1.3 和 flowchart / `direction` 的对比

| | architecture | flowchart |
|--|--------------|-----------|
| 层间主方向 | 主布局固定：**上→下**（macro rank） | 由 `direction` 决定（tb / lr） |
| `direction` | **不支持**（写了报错） | 支持 |
| `group_frame` `axis: horizontal` | 管**同行**兄弟；默认接近 `strips` | 常用于泳道左右并排（`lanes`） |
| `group_frame` `axis: vertical` | 少用；会按竖直 stack 语义整形 | 阶段上下堆（`stages`） |

> 详细选项仍见下文；场景短名见 [§5](#5-按场景选用)。

---

## 2. 何时使用

- 架构图顶层 group 宽度参差不齐，希望形成**等宽阶段条带**
- 泳道 / 分层图需要**左缘对齐**、**边框共线**
- 固定版式（如上二下一）需要 **matrix** 网格
- 组内节点排法不对：用 `layout: fan-out | grid | …` 覆盖自动推断

---

## 3. Group 级属性（含 L2 `layout`）

Group 标准属性只有三项：

| 属性 | 作用 |
|------|------|
| **`layout`** | 组内布局 hint（下文重点） |
| `border_style` | 边框线型：`solid` / `dashed` / `dotted`（纯视觉） |
| `color` | 背景色标签（主题/渲染消费） |

`layout` **仅 architecture 两阶段布局读取**；flowchart 分治路径目前不消费组内 `layout`。

### 3.1 写法

```plotgram
group backend "后端" {
    layout: fan-out
    entity gw "网关"
    entity a "服务 A"
    entity b "服务 B"
}
```

未写时等价于 `layout: auto`。

### 3.2 取值总表

| 值 | 别名 | 能力 | 典型用途 |
|----|------|------|----------|
| `auto` | （默认） | 按组内拓扑**自动推断**模式 | 大多数组；先不写，难看再显式覆盖 |
| `horizontal` | `h` | 成员**单行横排** | 3～5 个同级组件、工具链一行 |
| `vertical` | `v` | 成员**单列竖排** | 短链、网关→LB、存储栈 |
| `fan-out` | `fan_out`, `fanout` | 选一个 **hub**（出度≥2），其余在下游层展开；hub 常居中 | Agent 编排、API 网关扇出 |
| `fan-in` | `fan_in`, `fanin` | 选一个 **sink**（入度≥2），多源在上层、汇点在下 | 可观测性汇聚、多源→汇总 |
| `grid` | — | **规则网格**排布（行列） | 无内部边的同质节点（多库、多实例） |

解析入口：`parse_group_layout_hint` → `resolve_group_layout_mode`。

### 3.3 各选项详解

#### `auto`（默认）

引擎看组内边与度数，按优先级推断：

1. **fan-out**：某节点组内出度 ≥ 2  
2. **fan-in**：某节点组内入度 ≥ 2，且没有明显 fan-out  
3. **grid**：无内部边，且成员 ≥ 3（避免一行排太扁）  
4. **简单链**：成员 ≤ 2 → **vertical**（gateway→lb 等短栈）；成员 ≥ 3 → **horizontal**（避免与 architecture 组间上→下叠加成过高图）  
5. **Sugiyama**：其余有边的复杂拓扑（分层布局，非 DSL 字面量）

架构图还有**命名启发式**（仅当仍为 `auto` 时）：

- id 含 `public_subnet` → 倾向 `vertical`  
- id 含 `data_subnet` → 倾向 `grid`

**用途**：草稿阶段省事；拓扑清晰时结果通常够用。长链若仍要竖排，显式写 `layout: vertical`。

#### `horizontal` / `h`

- **能力**：忽略复杂分层，把直接成员排成一行。  
- **用途**：同级服务、同一层的多个对等组件。  
- **注意**：节点多时组会很宽，可能把整层条带撑高/撑宽；可改 `grid` 或拆组。

#### `vertical` / `v`

- **能力**：单列自上而下（或按流向）。  
- **用途**：管道式短链、入口栈（client → gateway → lb）、存储主从竖排。  
- **注意**：有横向 fan-out 时强制竖排会浪费高度、边更绕。

#### `fan-out` / `fan_out` / `fanout`

- **能力**：自动选 hub（组内出度最大且 ≥ 2；并列按 id 确定性打破）；子节点在下游层水平展开，hub 相对居中。  
- **用途**：「一个编排器连多个 worker」「网关连多个下游服务」。  
- **失败回退**：找不到合格 hub 时退回 `horizontal`。

#### `fan-in` / `fan_in` / `fanin`

- **能力**：自动选 sink（入度 ≥ 2，优先纯汇点出度=0）；多源在上层，sink 在下。  
- **用途**：metrics/logs/traces → grafana；多生产者 → 单一队列。  
- **失败回退**：找不到合格 sink 时退回 `horizontal`。

#### `grid`

- **能力**：按行列摆成矩阵，适合**彼此几乎无边**的同质节点。  
- **用途**：多数据库实例、多缓存分片、一排对等微服务且组内无依赖边。  
- **注意**：组内边很多时仍强制 grid 会忽略分层语义，边可能交叉；有边时更宜 `auto` / Sugiyama / `fan-*`。

### 3.4 选用建议

```text
组内有明显「一拖多」？     → fan-out
组内有明显「多归一」？     → fan-in
无边、≥3 个同类节点？     → grid（或交给 auto）
就是一条链？             → vertical
同级横排、数量不多？       → horizontal
不确定？                 → 先 auto，再按预览覆盖
```

`layout` **不改变** group 外框是否等宽、组间间距——那是 L1。

---

## 4. Diagram 级：`group_frame`（L1）

组间几何**只写** `group_frame`（旧 `group_sizing` 等已移除）。

### 4.1 推荐写法

```plotgram
diagram architecture {
    config {
        group_frame: stack {
            axis: horizontal
            track: equal
            cross: center
            gap: 40
            border: shared
            snap: true
        }
    }
}
```

| 写法 | 能力 | 用途 |
|------|------|------|
| `group_frame: stack { … }` | 对兄弟 group 做 L1 整形（等宽/对齐/间距；axis 语义见 §1 / §4.2） | 分层架构、泳道、阶段 |
| `group_frame: matrix { … }` | 顶层 group 填入 **二维网格** | 固定版式（如 2×2） |
| 场景短名 `strips` 等 | 展开为上表组合 | 日常优先；见 §5 |

未写 `group_frame` 时，按图表类型用算法默认（见 §4.4）。

### 4.2 `stack` 选项详解

#### `axis` — 堆叠主轴（L1 整形语义）

| 值 | 能力 | 用途 |
|----|------|------|
| `horizontal` / `h` | 把兄弟组当作**水平关系**处理：同行内左右间距/消重叠；`track: equal` 时拉齐宽度 | architecture 默认；`strips` / `lanes` |
| `vertical` / `v` | 把兄弟组当作**竖直关系**处理：上下堆叠与竖直间距 | flowchart 默认；`stages` |

**architecture 注意**：主布局已按 macro rank **上→下**排好层；此处的 `horizontal` **不会**把不同 rank 的 group 改成一横排，只影响**同行内**与等宽/对齐。详见 [§1](#1-architecture-macro-rank必读)。

只影响 **group 框**的 L1 整形，不改组内 `layout`。

#### `track` — 同级「轨道」尺寸（主轴方向上的框宽/高）

| 值 | 能力 | 用途 |
|----|------|------|
| `fit` | 每个 group 框贴合内容 | 草图、各组节点数差很大、不需要条带感 |
| `equal` / `uniform` | 同一 parent 下兄弟组拉齐到**最宽（或最高）者**；内容在 track 内居中 | **等宽阶段条带**（流水线、分层架构） |
| 数字（如 `320`） | 固定像素 track | 严格版式、对齐外部设计稿 |

**范围**：Equal 只拉齐**同一父级下的兄弟**，不会跨层强行同宽。

#### `cross` — 交叉轴对齐

主轴是水平时，cross 管**竖直方向**对齐（反之亦然）。

| 值 | 能力 | 用途 |
|----|------|------|
| `start` / `left` | 交叉轴起点对齐（水平堆叠时≈顶对齐；垂直堆叠时≈左对齐） | 泳道顶齐、左缘齐 |
| `center` | 交叉轴居中 | 架构图默认；条带内视觉平衡 |
| `end` / `right` | 交叉轴末端对齐 | 少见；右缘/底缘齐 |
| `stretch` | 交叉轴拉满可用空间 | 需要框在交叉轴上也撑满时 |

与 `track: equal` 配合时：主轴等宽 + 交叉轴对齐，条带最整齐。

#### `gap` — 组间净间距

| 能力 | 用途 |
|------|------|
| 相邻兄弟 group **外缘之间**的像素间距 | 加大 → 边路由更松、图更疏；减小 → 更紧凑 |

架构默认约 `40`；flowchart 通用 stack 约 `48`。常用调节区间 `40`～`80`。

#### `border` — 边框共线

| 值 | 能力 | 用途 |
|----|------|------|
| `none` | 不强制共线 | flowchart 默认；框随内容即可 |
| `shared` / `shared_lines` | 同级同侧边框尽量共线（左/上优先）；**只改框、不挪节点**（微调） | 架构条带「像尺子画出来」 |

#### `snap` — 组框像素量化

| 写法 | 能力 | 用途 |
|------|------|------|
| `true` | 开启，默认步长 8px | 与边折线量化一致，减少亚像素毛刺 |
| `false` | 关闭组框量化 | 调试几何时 |
| 数字（如 `8`） | 开启并指定步长 | 与设计网格对齐 |

也可继续用 diagram 级 `snap:`（边折线量化）；`group_frame { snap: … }` 可覆盖组框量化。

### 4.3 `matrix` 选项

```plotgram
config {
    group_frame: matrix {
        cols: 2
        rows: 2
        track: equal
        gap: 48
        cross: center
    }
}
```

| 选项 | 能力 | 用途 |
|------|------|------|
| `cols` / `rows` | 指定列/行；可只写一个，另一个按 `ceil(n/…)` | 固定「上二下一」类版式 |
| `gap` / `track` / `cross` / `snap` | 同 stack 语义，作用在网格单元格 | 单元格等宽、格内对齐 |

- 顶层 group 按布局后的 `(y, x)` **行优先**填入网格  
- **限制**：暂无 colspan；下排仅 1 个 group 时占左格  

### 4.4 算法默认（未写 `group_frame` 时）

| 图表 | arrangement | track | cross | gap | border | 你实际看到的 |
|------|-------------|-------|-------|-----|--------|--------------|
| **architecture** | `Stack(水平)` | **`equal`** | `center` | ~40 | `shared` | 层间仍 **上→下**（macro rank）；默认≈`strips` 的等宽竖条带 |
| **flowchart**（等） | `Stack(垂直)` | `fit` | `center` | ~48 | `none` | 组框倾向 **上→下** 堆；≈`stages` |

架构图默认「同级等宽 + 边框共线」，**不是**「所有 group 横排」。链状拓扑（如供应链控制塔）会呈现一列从上到下的等宽框。要内容贴合请写 `group_frame: fit` 或 `track: fit`。

### 4.5 架构图内置行为（无需 DSL）

| 行为 | 说明 |
|------|------|
| 单 group 行居中 | 某一层只有 1 个顶层 group 时，自动水平居中（`center_single_group_rows`） |
| 默认交叉轴居中 + 等宽 | 未写 `group_frame` 时 architecture 使用 `cross: center` + `track: equal` |
| 嵌套 group | 每个 parent 下的子 group 集合单独跑一遍 L1（同一 `GroupFrameSpec`） |
| Pin 保护 | `layout intent` 中 Pin 的节点在 Equal 拉宽时不会被平移 |

### 4.6 已移除的旧属性

以下 diagram 属性**已删除**，声明会校验报错，请一律改写为 `group_frame`：

| 已移除 | 请改写为 |
|--------|----------|
| `group_sizing: fit` | `track: fit` |
| `group_sizing: uniform` | `track: equal` |
| `group_arrangement: vertical` | `axis: vertical` |
| `group_arrangement: horizontal` | `axis: horizontal` |
| `group_gap: 60` | `gap: 60` |
| `group_align: center` | `cross: center` |
| `group_align: left` | `cross: start` |

**不要混淆**：

| 名字 | 层级 | 对象 |
|------|------|------|
| **macro rank**（主布局） | architecture 两阶段 | 按拓扑决定 group **在哪一行（上/下）** |
| group `layout` | L2 | 组内节点 |
| `group_frame` 的 `axis` / `cross` | L1 | 同行内怎么处理、是否等宽/对齐（**不改写 rank 上下序**） |
| diagram `direction` | 主布局流向轴 | **仅** flowchart / er / sugiyama(-v2) / mindmap；**architecture 不支持** |
| diagram `align` | L3 | 节点 rank/layer 结构对齐（与 group 框无关） |

> **`axis` ≠「整图左右流」**：architecture 下 `axis: horizontal` / `strips` 管的是同行与等宽条带，不是 `direction: left-to-right`。flowchart 的 `direction` 才改节点分层流向。详见 [§1](#1-architecture-macro-rank必读)。

---

## 5. 按场景选用

不必先背全套选项。先选下面 **5 个场景**之一；短名会展开成固定参数组合。需要微调时再写 `group_frame: <短名> { gap: 60 }` 覆盖单项，或改写成完整 `stack` / `matrix`。

| 场景 | 短名 | 一句话 | 展开后的关键参数 |
|------|------|--------|------------------|
| 分层条带 | `strips` | 等宽整齐的层/阶段框（architecture 层间仍上→下） | `axis: horizontal` · `track: equal` · `cross: center` · `border: shared` |
| 内容贴合 | `fit` | 不拉等宽；同行仍按水平关系处理 | `axis: horizontal` · `track: fit` |
| 水平泳道 | `lanes` | 多泳道并排，顶对齐、间距偏大 | `axis: horizontal` · `cross: start` · `gap: 80` |
| 纵向阶段 | `stages` | 阶段从上往下堆（flowchart） | `axis: vertical` · `track: fit` · `cross: center` |
| 固定网格 | `tiles` | 默认 2×2 格子版式 | `matrix` · `cols/rows: 2` · `track: equal` · `gap: 48` |

```plotgram
config {
    group_frame: strips          // 推荐：短名
    // group_frame: strips { gap: 60 }   // 短名 + 覆盖
}
```

> 架构图**未写** `group_frame` 时，默认效果接近 `strips`（等宽竖条带；**层序仍上→下**）。flowchart 默认接近垂直 `stages`，泳道请显式写 `lanes`。见 [§1](#1-architecture-macro-rank必读)。

### 5.1 `strips` — 分层条带

**何时用**：微服务分层、流水线阶段，希望同级 group **一样宽**、边框像尺子画的。

**architecture 下长什么样**：链状拓扑 → **一列等宽框从上到下**（不是横排）。同一 rank 有多个 group 时，该行内才会左右并排。

```plotgram
group_frame: strips
// 等价于：
group_frame: stack {
    axis: horizontal   // 同行按水平关系处理（≠ 整图改成左右流）
    track: equal       // 兄弟拉成一样宽 → 竖条带也同宽
    cross: center      // 交叉轴居中
    border: shared     // 同侧边框尽量共线
}
```

| 参数 | 作用 |
|------|------|
| `axis: horizontal` | 同行内水平间距/消重叠；配合 equal 做条带 |
| `track: equal` | 同级等宽（跨行也会同宽，形成竖条带） |
| `cross: center` | 交叉轴居中 |
| `border: shared` | 边框共线，更「图纸感」 |

常与 `edge_routing: orthogonal` 一起用。参考：[`c.supply-chain-control-tower.pgm`](../../showcase/architecture/c.supply-chain-control-tower.pgm)（竖向链）、[`c.ai-agent-docops-pipeline.pgm`](../../showcase/architecture/c.ai-agent-docops-pipeline.pgm)、[`n.data-pipeline.pgm`](../../showcase/architecture/n.data-pipeline.pgm)、[dsl-writing-manual §8.3](../specs/dsl/dsl-writing-manual.md#83-架构图分层服务使用-group_frame)。

### 5.2 `fit` — 内容贴合

**何时用**：各组节点数差很大，硬拉等宽会留大片空白。

```plotgram
group_frame: fit
// 等价于：
group_frame: stack {
    axis: horizontal   // 同行按水平关系；层间上下仍由主布局决定
    track: fit         // 框宽贴合内容，不拉齐
}
```

| 参数 | 作用 |
|------|------|
| `axis: horizontal` | 同行水平关系（同 `strips`） |
| `track: fit` | 每个框跟自己的内容走 |

### 5.3 `lanes` — 水平泳道

**何时用**：流程图多条泳道并排；要**顶对齐**、框之间疏一点。

```plotgram
diagram flowchart {
    config {
        direction: top-to-bottom   // 仅 flowchart 等支持；architecture 勿写
        group_frame: lanes
        // 等价于：
        // group_frame: stack {
        //     axis: horizontal
        //     cross: start
        //     gap: 80
        // }
    }
}
```

| 参数 | 作用 |
|------|------|
| `axis: horizontal` | 泳道左右并排（≠ `direction`） |
| `cross: start` | 顶对齐（水平堆叠时） |
| `gap: 80` | 泳道间距偏大，边更好走 |

参考：[`c.swimlane-order-process.pgm`](../../showcase/flowchart/c.swimlane-order-process.pgm)。

### 5.4 `stages` — 纵向阶段

**何时用**：流程图阶段从上往下划分（CI/CD、审批阶段）。

```plotgram
group_frame: stages
// 等价于：
group_frame: stack {
    axis: vertical
    track: fit
    cross: center
}
```

| 参数 | 作用 |
|------|------|
| `axis: vertical` | 组框上下堆 |
| `track: fit` | 高度/宽度随内容 |
| `cross: center` | 水平方向居中 |

### 5.5 `tiles` — 固定网格

**何时用**：就要 2×2 / 上二下一这类固定格子；默认 **2×2 等宽**。

```plotgram
group_frame: tiles
// 等价于：
group_frame: matrix {
    cols: 2
    rows: 2
    track: equal
    gap: 48
}

// 改成 3 列：
group_frame: tiles { cols: 3 }
```

| 参数 | 作用 |
|------|------|
| `cols` / `rows` | 网格列/行；短名默认都是 2 |
| `track: equal` | 单元格等宽 |
| `gap: 48` | 格间距 |

限制：暂无 colspan；下排仅 1 个 group 时占左格。

### 5.6 L1 + L2 一起写

短名只管**组间**；组内仍用 `group { layout: … }`。

```plotgram
diagram architecture {
    config {
        group_frame: strips
        edge_routing: orthogonal
    }

    group ingress "入口" {
        layout: vertical
        entity gw "网关"
        entity lb "负载均衡"
    }

    group services "服务" {
        layout: fan-out
        entity orch "编排"
        entity s1 "服务1"
        entity s2 "服务2"
    }

    group data "数据" {
        layout: grid
        entity db1 "主库"
        entity db2 "从库"
        entity cache "缓存"
    }
}
```

---

## 6. 调节步骤与排错

1. **先选场景短名**（`strips` / `fit` / `lanes` / `stages` / `tiles`），预览效果。  
2. **先定拓扑**：用 relation 表达分层与数据流；框无法单靠 DSL 覆盖错误拓扑。  
3. **微调一项**：常见只改 `gap`；要贴内容用 `fit` 或 `track: fit`。  
4. **调组内**：按组加 `layout: horizontal | fan-out | fan-in | grid | …`。  
5. **边框与量化**：`strips` 已含 `border: shared`；默认 `snap: true`。  
6. **预览**：`plotgram render your.pgm -o out.svg` 或 showcase 批量脚本。

| 现象 | 可能原因 | 调节 |
|------|----------|------|
| 架构图 group 全是上下排，不是左右 | **正常**：macro rank 上→下；每层 1 个 group 时就是竖列 | 见 [§1](#1-architecture-macro-rank必读)；左右并排需同 rank 多个 group |
| 写了 `strips` 仍是竖的 | `strips` 管等宽/同行，不改 rank 方向 | 改拓扑或接受竖条带 |
| 等宽后某层特别高 | 该组节点多或 `fan-out` 展开 | 拆组、改 `layout`、或减少组内 entity |
| 两层宽度仍不齐 | 两层不在同一 sibling 集合 | Equal 仅拉齐**同一 parent 下**的兄弟 |
| 拉宽后节点偏一侧 | 正常：Equal 会居中组内内容 | 检查 `cross`；Pin 节点不会被移动 |
| 矩阵顺序不对 | 格子顺序由布局后坐标决定 | 先调拓扑让大致顺序正确，再开 `tiles` / matrix |

### CLI 验证

```bash
cargo run -p plotgram-cli -- render showcase/architecture/c.ai-agent-docops-pipeline.pgm -o /tmp/out.svg
cargo run -p plotgram-cli -- lint showcase/architecture/c.ai-agent-docops-pipeline.pgm
```

Layout hints 中可查看 `GroupFrameReport`（是否 equalized、matrix_applied 等），见 [render-pipeline.md](render-pipeline.md)。

---

## 7. 相关文档

- [DSL 写作手册 §5 Group / §6.6 group_frame](../specs/dsl/dsl-writing-manual.md)  
- [语言规范 §4.6 / §7.3](../specs/dsl/language-spec.md)  
- [架构图视觉语言](../specs/visual-language/diagrams/architecture.md)  
- [group-frame-spec.md](../已经实现的方案/group-frame-spec.md) — 设计规格  
- [layout-intent.md](layout-intent.md) — Pin / Align 与 Group Frame 交互  
- [layout-lint.md](layout-lint.md) — 组重叠、节点溢出 group 等检查  
- [theme-and-style.md](theme-and-style.md) — 组边框线型等  
