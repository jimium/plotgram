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

> 设计规格：[group-frame-spec.md](../已经实现的方案/group-frame-spec.md)  
> DSL：[dsl-writing-manual §5.3 / §6.6](../specs/dsl/dsl-writing-manual.md) · [language-spec §4.6 / §7.3](../specs/dsl/language-spec.md)  
> 权威枚举：`attr_constants`（`group_layout` / `group_frame_track`）  
> 实现：`group_layout_hint.rs`（L2）、`layout/group_frame/`（L1）

Group Frame 在**主布局算法之后**执行（路由前、路由后各会幂等恢复一次），通过 DSL `config` 声明即可。

---

## 1. 何时使用

- 架构图顶层 group 宽度参差不齐，希望形成**等宽阶段条带**
- 泳道 / 分层图需要**左缘对齐**、**边框共线**
- 固定版式（如上二下一）需要 **matrix** 网格
- 组内节点排法不对：用 `layout: fan-out | grid | …` 覆盖自动推断

---

## 2. Group 级属性（含 L2 `layout`）

Group 标准属性只有三项：

| 属性 | 作用 |
|------|------|
| **`layout`** | 组内布局 hint（下文重点） |
| `border_style` | 边框线型：`solid` / `dashed` / `dotted`（纯视觉） |
| `color` | 背景色标签（主题/渲染消费） |

`layout` **仅 architecture 两阶段布局读取**；flowchart 分治路径目前不消费组内 `layout`。

### 2.1 写法

```plotgram
group backend "后端" {
    layout: fan-out
    entity gw "网关"
    entity a "服务 A"
    entity b "服务 B"
}
```

未写时等价于 `layout: auto`。

### 2.2 取值总表

| 值 | 别名 | 能力 | 典型用途 |
|----|------|------|----------|
| `auto` | （默认） | 按组内拓扑**自动推断**模式 | 大多数组；先不写，难看再显式覆盖 |
| `horizontal` | `h` | 成员**单行横排** | 3～5 个同级组件、工具链一行 |
| `vertical` | `v` | 成员**单列竖排** | 短链、网关→LB、存储栈 |
| `fan-out` | `fan_out`, `fanout` | 选一个 **hub**（出度≥2），其余在下游层展开；hub 常居中 | Agent 编排、API 网关扇出 |
| `fan-in` | `fan_in`, `fanin` | 选一个 **sink**（入度≥2），多源在上层、汇点在下 | 可观测性汇聚、多源→汇总 |
| `grid` | — | **规则网格**排布（行列） | 无内部边的同质节点（多库、多实例） |

解析入口：`parse_group_layout_hint` → `resolve_group_layout_mode`。

### 2.3 各选项详解

#### `auto`（默认）

引擎看组内边与度数，按优先级推断：

1. **fan-out**：某节点组内出度 ≥ 2  
2. **fan-in**：某节点组内入度 ≥ 2，且没有明显 fan-out  
3. **grid**：无内部边，且成员 ≥ 3（避免一行排太扁）  
4. **vertical**：简单链（A→B→C）  
5. **Sugiyama**：其余有边的复杂拓扑（分层布局，非 DSL 字面量）

架构图还有**命名启发式**（仅当仍为 `auto` 时）：

- id 含 `public_subnet` → 倾向 `vertical`  
- id 含 `data_subnet` → 倾向 `grid`

**用途**：草稿阶段省事；拓扑清晰时结果通常够用。若推断与意图不符，再改成显式值。

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

### 2.4 选用建议

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

## 3. Diagram 级：`group_frame`（L1）

组间几何**只写** `group_frame`（旧 `group_sizing` 等已移除）。

### 3.1 推荐写法

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
| `group_frame: stack { … }` | 兄弟 group **一维堆叠**（沿 axis） | 分层架构、阶段流水线、泳道（改 axis） |
| `group_frame: matrix { … }` | 顶层 group 填入 **二维网格** | 固定版式（如 2×2） |

未写 `group_frame` 时，按图表类型用算法默认（见 §3.4）。

### 3.2 `stack` 选项详解

#### `axis` — 堆叠主轴

| 值 | 能力 | 用途 |
|----|------|------|
| `horizontal` / `h` | 兄弟组沿**水平**排开（左→右） | 架构图默认语义：阶段/层从左到右 |
| `vertical` / `v` | 兄弟组沿**垂直**排开（上→下） | 流程图阶段划分、纵向泳道内容 |

只影响 **group 框之间**的相对位置，不改组内 `layout`。

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

### 3.3 `matrix` 选项

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

### 3.4 算法默认（未写 `group_frame` 时）

| 图表 | arrangement | track | cross | gap | border |
|------|-------------|-------|-------|-----|--------|
| **architecture** | `Stack(水平)` | **`equal`**（`track: fit` 可退回） | `center` | ~40 | `shared` |
| **flowchart**（等） | `Stack(垂直)` | `fit` | `center` | ~48 | `none` |

架构图默认就是「同级等宽条带」；要内容贴合请写 `group_frame: stack { track: fit }`。

### 3.5 架构图内置行为（无需 DSL）

| 行为 | 说明 |
|------|------|
| 单 group 行居中 | 某一层只有 1 个顶层 group 时，自动水平居中（`center_single_group_rows`） |
| 默认交叉轴居中 + 等宽 | 未写 `group_frame` 时 architecture 使用 `cross: center` + `track: equal` |
| 嵌套 group | 每个 parent 下的子 group 集合单独跑一遍 L1（同一 `GroupFrameSpec`） |
| Pin 保护 | `layout intent` 中 Pin 的节点在 Equal 拉宽时不会被平移 |

### 3.6 已移除的旧属性

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
| group `layout` | L2 | 组内节点 |
| `group_frame` 的 `axis` / `cross` | L1 | group 框怎么排、怎么对齐 |
| diagram `direction` | 主布局流向轴 | **仅** flowchart / er / sugiyama(-v2) / mindmap；**architecture 不支持** |
| diagram `align` | L3 | 节点 rank/layer 结构对齐（与 group 无关） |

> **`axis` ≠ `direction`**：`group_frame { axis: horizontal }` 控制**组框**从左到右堆叠；flowchart 的 `direction: left-to-right` 控制**节点分层流向**。架构图要「层从左到右」请写 `group_frame`，不要写 `direction`（写了会报错）。

---

## 4. 典型配方

### 4.1 流水线 / 分层（等宽条带）

```plotgram
config {
    group_frame: stack {
        axis: horizontal
        track: equal
        cross: center
        gap: 50
        border: shared
    }
}
```

参考：[`c.ai-agent-docops-pipeline.pgm`](../../showcase/architecture/c.ai-agent-docops-pipeline.pgm)、[`n.data-pipeline.pgm`](../../showcase/architecture/n.data-pipeline.pgm)。

拓扑由 **relation** 决定层级；`track: equal` 把**同级**顶层 group 拉成相同宽度。

### 4.2 微服务三层 + 正交路由

```plotgram
config {
    group_frame: stack {
        axis: horizontal
        track: equal
        cross: center
        gap: 50
        border: shared
    }
    edge_routing: orthogonal
}
```

见 [dsl-writing-manual §8.3](../specs/dsl/dsl-writing-manual.md#83-架构图分层服务使用-group_frame)。

### 4.3 泳道（水平堆叠 group）

```plotgram
diagram flowchart {
    config {
        direction: top-to-bottom   // 仅 flowchart 等支持；architecture 勿写
        group_frame: stack {
            axis: horizontal       // 组框左右排（≠ direction）
            cross: start
            gap: 80
        }
    }
}
```

参考：[`c.swimlane-order-process.pgm`](../../showcase/flowchart/c.swimlane-order-process.pgm)。架构图同类版式只保留 `group_frame`，不要加 `direction`。

### 4.4 仅贴合内容

`group_frame: stack { track: fit }`（或 flowchart 默认）：每个 group 宽度随内容，适合组内节点数差异大的草图。

### 4.5 L1 + L2 一起写

```plotgram
diagram architecture {
    config {
        group_frame: stack {
            axis: horizontal
            track: equal
            cross: center
            gap: 48
            border: shared
        }
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

## 5. 调节步骤与排错

1. **先定拓扑**：用 relation 表达分层与数据流；框无法单靠 DSL 覆盖错误拓扑。  
2. **开等宽**：架构图默认已是 `track: equal`；要贴内容用 `track: fit`。  
3. **调组内**：按组加 `layout: horizontal | fan-out | fan-in | grid | …`。  
4. **调间距**：`gap` 加大可减轻边路由拥挤；架构图常用 `48`～`60`。  
5. **边框与量化**：`border: shared` + 默认 `snap: true`。  
6. **预览**：`plotgram render your.pgm -o out.svg` 或 showcase 批量脚本。

| 现象 | 可能原因 | 调节 |
|------|----------|------|
| 等宽后某层特别高 | 该组节点多或 `fan-out` 展开 | 拆组、改 `layout`、或减少组内 entity |
| 两层宽度仍不齐 | 两层不在同一 sibling 集合 | Equal 仅拉齐**同一 parent 下**的兄弟 |
| 拉宽后节点偏一侧 | 正常：Equal 会居中组内内容 | 检查 `cross`；Pin 节点不会被移动 |
| 矩阵顺序不对 | 格子顺序由布局后坐标决定 | 先调拓扑让大致顺序正确，再开 matrix |

### CLI 验证

```bash
cargo run -p plotgram-cli -- render showcase/architecture/c.ai-agent-docops-pipeline.pgm -o /tmp/out.svg
cargo run -p plotgram-cli -- lint showcase/architecture/c.ai-agent-docops-pipeline.pgm
```

Layout hints 中可查看 `GroupFrameReport`（是否 equalized、matrix_applied 等），见 [render-pipeline.md](render-pipeline.md)。

---

## 6. 相关文档

- [DSL 写作手册 §5 Group / §6.6 group_frame](../specs/dsl/dsl-writing-manual.md)  
- [语言规范 §4.6 / §7.3](../specs/dsl/language-spec.md)  
- [架构图视觉语言](../specs/visual-language/diagrams/architecture.md)  
- [group-frame-spec.md](../已经实现的方案/group-frame-spec.md) — 设计规格  
- [layout-intent.md](layout-intent.md) — Pin / Align 与 Group Frame 交互  
- [layout-lint.md](layout-lint.md) — 组重叠、节点溢出 group 等检查  
- [theme-and-style.md](theme-and-style.md) — 组边框线型等  
