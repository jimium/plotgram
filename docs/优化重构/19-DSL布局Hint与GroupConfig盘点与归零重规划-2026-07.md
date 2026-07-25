# 19 - DSL 布局 Hint / Group Config 盘点与归零重规划

> 日期：2026-07-24  
> 状态：盘点完成；重规划为架构建议（设计归零，迁移后删除旧语法）  
> 范围：当前 DSL / config 中所有影响布局与组几何的控制面；未来统一 Intent DSL 的保留/删除/映射  
> 关联：  
> - [`18-DiagramScheme-布局与路由总配方方案-2026-07.md`](./18-DiagramScheme-布局与路由总配方方案-2026-07.md)  
> - [`14-布局Recipe生命周期收敛方案-2026-07.md`](./14-布局Recipe生命周期收敛方案-2026-07.md)  
> - [`12-多图类型布局共享内核与独立配方架构-2026-07.md`](./12-多图类型布局共享内核与独立配方架构-2026-07.md)  
> - [`group-layout-and-frame.md`](../guides/group-layout-and-frame.md)（现状操作指南）  
> - [`language-spec.md`](../specs/dsl/language-spec.md) §4 / §7  

---

## 0. 一句话

> **第一部分如实盘点现状；第二部分把控制面归零为「Scheme + Intent + Frame」，旧 hint/config 语义迁移进 IR 后删除，禁止属性直接推最终几何。**

---

# 第一部分：当前 Hint 与 Group Config 盘点

当前布局相关控制面大致分成 **五层**，名称与文档中的 L1/L2/L3 对应，但实现上仍有交叉与图种特例。

```text
Diagram 级策略     layout / edge_routing / direction / align / snap / 算法 options
        │
L1 Group Frame     group_frame（组间几何唯一入口）
        │
L2 Intra Frame     group { layout: … }（组内排列 hint，主要 architecture）
        │
L3 Node Frame      align（路由前节点对齐）+ snap（路由后量化）
        │
节点尺寸 hint       style.width / style.height
```

另有一批**已删除**的旧组间糖属性（见 §1.6），说明本域已经历过一轮「多属性 → 单一入口」收敛。

---

## 1.1 Diagram 级布局 / 路由选择

| 属性 | 写法 | 作用 | 消费方 | 备注 |
|------|------|------|--------|------|
| `layout` | atom 或 `algo { options }` | 选择布局算法 | Layout registry / Recipe | 与图种默认绑定；architecture **不读** `direction` |
| `edge_routing` | atom 或 `algo { options }` | 选择边路由族 | Router registry | sequence **禁止**声明 |
| `direction` | `top-to-bottom` / `left-to-right` / `radial` | 布局流向轴偏好 | flowchart / er / sugiyama(-v2) / mindmap | 不支持的图种显式声明报错 |
| `theme` / `render_style` | atom | 主题与笔触 | 渲染 | **非布局**；盘点时列出以免与布局配置混淆 |
| `title` | string | 标题 | 渲染 | 非布局 |

### 布局算法可选值（摘要）

| 算法 | 典型图种 | 已知 options |
|------|----------|--------------|
| `flowchart` | flowchart 默认 | `group_padding` |
| `architecture` | architecture 默认 | `group_padding`, `padding` |
| `er` | ER | `group_padding` |
| `state` | state | `group_padding`, `padding`, `component_gap` |
| `mindmap` | mindmap | `padding`, `level_gap`, `branch_gap`, `node_gap`, `center_gap` |
| `sequence` | sequence | `group_padding`, `node_spacing`, `message_spacing` |
| `sugiyama-v2` / `sugiyama` | 通用/兼容 | `group_padding` |
| `force-directed` | — | `group_padding`, `padding`, `component_gap` |
| `circular` | — | `group_padding`, `padding`, `component_gap` |

### 边路由可选值（摘要）

| 路由 | 默认场景 | 已知 options |
|------|----------|--------------|
| `orthogonal` | flowchart / architecture | `slot_pitch`, `channel_margin` |
| `straight` | ER | — |
| `bezier` | — | `tension` |
| `spline` | — | — |
| `circular` | state | — |
| `organic` | mindmap | `tension`, `shoulder_ratio`, `depth_decay`, `curve_style`, `port_distribution` |

**问题**：`layout` 与 `edge_routing` 独立选择，缺少产品层「总方案」（见 doc18）；算法 options 暴露偏「旋钮」，对 AI agent 不友好。

---

## 1.2 L1：`group_frame`（组间宏观几何）

**定位**：组间几何的**唯一** DSL 入口（旧 `group_sizing` / `group_arrangement` / `group_gap` / `group_align` 已删）。

**写在**：diagram body / `config`。

**实现**：`layout/group_frame/`；主布局之后、路由前后可幂等恢复。

### 两种形态 + 场景短名

| 形态 | 用途 |
|------|------|
| `stack { … }` | 一维堆叠整形（等宽/对齐/间距/边框） |
| `matrix { … }` | 顶层 group 填入二维网格 |
| 短名 `strips` / `fit` / `lanes` / `stages` / `tiles` | 展开为固定参数组合；可再覆盖单项 |

短名展开：

| 短名 | 场景 | 展开要点 |
|------|------|----------|
| `strips` | 分层等宽条带 | `axis: horizontal`, `track: equal`, `cross: center`, `border: shared` |
| `fit` | 内容贴合 | `axis: horizontal`, `track: fit` |
| `lanes` | 水平泳道 | `axis: horizontal`, `cross: start`, `gap: 80` |
| `stages` | 纵向阶段 | `axis: vertical`, `track: fit`, `cross: center` |
| `tiles` | 固定网格 | `matrix { cols/rows: 2, track: equal, gap: 48 }` |

### `stack` 选项

| 选项 | 取值 | 含义 |
|------|------|------|
| `axis` | `horizontal`/`h`, `vertical`/`v` | 兄弟组按水平或竖直关系整形（**architecture 下 horizontal ≠ 整图左右流**） |
| `gap` | 正数 | 组间净间距（px） |
| `track` | `fit` / `equal`/`uniform` / 固定数值 | 主轴方向框宽(高)策略；`equal` 拉齐同级兄弟 |
| `cross` | `start`/`left`, `center`, `end`/`right`, `stretch` | 交叉轴对齐 |
| `border` | `none`, `shared`/`shared_lines` | 边框共线（改框、原则上不挪节点） |
| `snap` | bool / 步长 | 组框像素量化 |

### `matrix` 选项

| 选项 | 含义 |
|------|------|
| `rows` / `cols` | 网格行列 |
| `gap` / `track` / `cross` / `snap` | 同 stack 语义，作用于单元格 |

### 算法默认（未写时）

| 图种 | 倾向 |
|------|------|
| architecture | ≈ `strips`（等宽 + shared border；层间仍由 macro rank 上→下） |
| flowchart 等 | ≈ `stages`（竖直 stack + fit） |

### 架构图内置（无 DSL）

- 单 group 行水平居中  
- 嵌套 group 按 parent 子集分别跑 L1  
- Pin 节点在 Equal 拉宽时受保护（若有 layout intent pin）

**问题**：能力真实且有用，但与 `direction`、macro rank、group `layout` 易混淆；参数面偏大；`snap` 与 diagram 级 `snap` 双入口。

---

## 1.3 L2：Group 属性 `layout`（组内排列 hint）

**写在**：每个 `group { layout: … }`。

**消费方**：**仅 architecture 两阶段布局**读取；flowchart 分治路径目前**不消费**。

**解析**：`group_layout_hint.rs` → `parse_group_layout_hint` / `resolve_group_layout_mode`。

| 值 | 别名 | 行为摘要 |
|----|------|----------|
| `auto`（默认） | — | 按组内拓扑推断：fan-out → fan-in → grid → 短链 vertical / 长链 horizontal → 否则 Sugiyama |
| `horizontal` | `h` | 成员单行横排 |
| `vertical` | `v` | 成员单列竖排 |
| `fan-out` | `fan_out`, `fanout` | 选 hub（出度≥2），下游展开；失败回退 horizontal |
| `fan-in` | `fan_in`, `fanin` | 选 sink（入度≥2），多源汇入；失败回退 horizontal |
| `grid` | — | 规则网格；适合无内部边的同质节点 |

**architecture 命名启发式**（仅 `auto`）：id 含 `public_subnet` → vertical；含 `data_subnet` → grid。

同 group 上还有**非布局**属性：`border_style`、`color`（视觉），归零布局 DSL 时一般保留。

**问题**：

1. 只对 architecture 生效 → 控制面不统一。  
2. `auto` 含**图名/ id 子串启发式**，与「禁止图名特判」精神冲突，agent 难复现。  
3. 模式名是「排版动词」，不是「结构意图」；与将来 Intent IR 不对齐。

---

## 1.4 L3：`align` 与 `snap`

### `align`（节点结构对齐，路由前）

| 值 | 含义 |
|----|------|
| `true`（默认） | 各布局算法默认策略 |
| `false` / `off` | 完全关闭 |
| `rank` | 仅流向轴同层中心线对齐 |
| `layer` | 仅垂直于流向轴的重叠/间距修正 |
| `full` | rank + layer |

算法默认（`align: true` 时）：

| 布局 | 默认 |
|------|------|
| flowchart / sugiyama-v2 | rank ✅ + layer `OverlapOnly` |
| er | rank ✅ + layer `Off` |
| architecture | rank ✅ + layer `Centroid` |

**时序**：路由**之前**改节点坐标 → 影响路由输入。

**易混**：`group_frame.cross` 对齐的是 **group 框**；`align` 对齐的是 **节点**。

### `snap`（像素量化）

| 入口 | 对象 |
|------|------|
| diagram `snap: true/false` | 边路由后折线等量化（亦被描述为 group_frame sugar） |
| `group_frame { snap: … }` | 组框量化（可覆盖） |

**问题**：`align` 是后处理开关，不是语义意图；与 Coordinate Kernel「求解后少推点」方向张力大。`snap` 双入口应在归零时合并为 finalize 策略。

---

## 1.5 节点尺寸 hint 与其它

| 属性 | 作用 | 性质 |
|------|------|------|
| `style.width` / `style.height` | 布局用的节点尺寸 hint | 半布局 / 半视觉；多处复用 |
| entity `type` / `semantic` 等 | 形状与图标；间接影响尺寸与端口 | 语义属性，非布局 hint |
| relation `cardinality` 等 | 标签内容 | 非布局 |

算法 options 中的间距类（`group_padding`、`padding`、`*_gap`、`node_spacing` 等）本质是 **LayoutProfile 数值**，目前挂在 `layout: algo { … }` 下，与 `group_frame.gap` 并列，形成**多处调间距**。

---

## 1.6 已删除的旧组间属性（历史）

| 已删 | 迁移到 |
|------|--------|
| `group_sizing: fit/uniform` | `group_frame.track` |
| `group_arrangement: vertical/horizontal` | `group_frame.axis` |
| `group_gap` | `group_frame.gap` |
| `group_align` | `group_frame.cross` |

说明：**「多糖属性 → 单一入口」已被证明可行**；下一步是把 L1/L2/L3/算法 options 再升一层，统一到 Scheme + Intent。

---

## 1.7 现状问题小结

| # | 问题 |
|---|------|
| 1 | 控制面分散：scheme 级 / frame 级 / group layout / align / 算法 options / 尺寸 hint |
| 2 | 同一概念多入口：间距（padding/gap）、snap、对齐（cross vs align） |
| 3 | 图种特例：group `layout` 仅 architecture；direction 仅部分图种 |
| 4 | 存在 id 子串启发式（subnet 名） |
| 5 | 大量开关直接改几何或后处理，未统一 compile → Intent IR |
| 6 | 对 AI agent：选项多、语义叠床架屋、缺少「意图 → 可观测生效」闭环 |
| 7 | 与 doc18 DiagramScheme、doc14/16 写权模型尚未对齐 |

---

# 第二部分：归零重规划

## 2.0 原则

1. **设计归零，迁移后删除**——不先拆光再空想；新契约就绪并迁完 showcase 后再删旧语法。  
2. **Hint = 语义意图，不是坐标遥控器**——一律 compile 进 typed intent / profile；禁止 DSL 属性直接写最终 `x/y` 或折点。  
3. **少而稳，面向 agent**——可枚举、可冲突诊断、可 trace「生效/忽略」。  
4. **能推断则默认推断**——DSL 只做覆盖；去掉 id 子串特判。  
5. **与 Kernel/Recipe/Scheme 同向**——对接 doc12/14/16/18，不另起第三套写权。

---

## 2.1 目标控制面（四层）

```text
① DiagramScheme          整图策略：布局族 + 路由族 + tier（紧凑/标准/宽松）
② Structure Intent       结构意图：同层、顺序、侧置、主链、组内模式覆盖
③ Compose / Frame        组与宏观几何：条带/泳道/网格（吸收今日 group_frame）
④ Finalize Policy        收尾：像素量化等（吸收 snap；弱化或内化 align）
```

可选第五层（高级，默认不对 agent 开放）：

```text
⑤ Expert Profile knobs   group_padding / slot_pitch 等 → 仅 Scheme tier 或显式 expert 块
```

---

## 2.2 现有能力：保留 / 变形 / 删除

### A. 整图策略类

| 现状 |  disposition | 新形态 |
|------|--------------|--------|
| `layout` + `edge_routing` 分选 | **变形** | `scheme: flowchart-orthogonal` 等（doc18）；高级用户仍可显式覆盖并归一成临时 Scheme |
| `direction` | **保留语义，收窄入口** | 进入 Scheme / LayoutProfile 的 `flow_axis`；不支持的图种由 Scheme 直接不含该字段 |
| 算法名 `sugiyama-v2` 等裸选 | **降级为 expert / 内部** | 日常只暴露 Scheme；内部 Recipe id 可保留 |
| `theme` / `render_style` / `title` | **保留** | 非布局，不动 |

### B. L1 `group_frame`

| 现状 | disposition | 新形态 |
|------|--------------|--------|
| 场景短名 strips/fit/lanes/stages/tiles | **保留能力，改名空间** | `compose: strips` 或 `frame: lanes`（名称待定）；仍是预设 → ComposeSpec |
| `stack` / `matrix` 全选项 | **保留能力，收敛词汇** | 编译为 `ComposeIntent`（track/cross/gap/border/grid） |
| `axis` 与 macro rank 易混问题 | **文档 + 命名修正** | 避免再用「axis=整图方向」暗示；与 `flow_axis` 严格分词 |
| diagram `snap` vs frame `snap` | **合并** | 唯一 `finalize.snap`（或 Scheme 默认） |
| 等宽条带 / 泳道 / 阶段 / 瓷砖 | **必须保留** | 真需求；只换表达与编译路径 |

### C. L2 group `layout`

| 现状 | disposition | 新形态 |
|------|--------------|--------|
| `auto` 拓扑推断 | **保留为默认引擎行为** | 不再作为必须书写的 DSL；推断规则进 Recipe，**删除 id 子串启发式** |
| `horizontal` / `vertical` | **变形为 Structure/Compose 覆盖** | 如 `arrange: row \| column`（组级 intent） |
| `fan-out` / `fan-in` | **变形为结构角色覆盖** | `pattern: fan-out \| fan-in` 或自动识别 + `hub:` / `sink:` 覆盖 |
| `grid` | **变形** | `arrange: grid` |
| 「仅 architecture 生效」 | **删除该限制** | 凡两阶段/分治组内布局的图种统一消费同一 Intent |
| 别名丛（fan_out/fanout/…） | **删除** | 单一规范拼写 |

### D. L3 `align`

| 现状 | disposition | 新形态 |
|------|--------------|--------|
| `align: rank/layer/full/off` | **默认删除出 DSL** | 对齐目标进 Coordinate objective / 硬分离；由 Scheme 质量档隐式决定 |
| 调试用关闭对齐 | **可选 expert** | `finalize.node_align: off`（低频）或仅内部 flag |
| 路由前推点式 align | **实现侧收敛** | 能进 solver 的不进后处理；与手册「单一坐标写者」一致 |

### E. 间距与算法 options

| 现状 | disposition | 新形态 |
|------|--------------|--------|
| `group_padding` / `padding` / 各种 `*_gap` | **吸收进 LayoutProfile tier** | `density: compact \| standard \| spacious` → 映射数值表 |
| `group_frame.gap` | **保留为 Compose 局部覆盖** | 允许在 strips 上覆盖 gap；与 density 的优先级写死 |
| orthogonal `slot_pitch` / `channel_margin` | **进 RoutingProfile / DemandProbe** | 不对日常 DSL；由 scheme + density 推导 |
| organic 一堆曲线旋钮 | **进 RoutingProfile expert** | 日常只选 scheme（如 mindmap-organic） |

### F. 节点尺寸

| 现状 | disposition | 新形态 |
|------|--------------|--------|
| `style.width` / `style.height` | **保留** | 仍是尺寸约束/hint，编译为节点度量；与 Structure Intent 分开 |

### G. 明确删除（不迁语义或仅内部）

| 项 | 原因 |
|----|------|
| group id 子串 → layout 启发式 | 图名/命名特判 |
| 多套 snap 入口 | 合并为一 |
| 面向用户的裸算法 option 丛林 | 改为 tier + expert |
| 绝对坐标 / 逐边折点 DSL（若未来有人提） | 非自动布局合约 |
| 旧已删糖属性名复活 | 禁止 |

---

## 2.3 新架构下的实现路径

### 编译链（目标）

```text
DSL
  → Parse
  → DiagramScheme 解析/归一（doc18）
  → IntentCompiler
        StructureIntent[]  +  ComposeSpec  +  FinalizePolicy
  → LayoutRecipe.compile（吞掉 intents → CoordinateIntent / Layered policy / 组内 mode）
  → solve → DemandProbe → re-solve(≤2) → freeze
  → RoutingRecipe（RoutingProfile 来自 Scheme）
  → Finalize（snap 等）
```

**硬契约**：

- IntentCompiler **之后**不再存在「读 group.layout 字符串直接推点」的旁路。  
- Compose（原 group_frame）在 freeze 前完成组框策略；route 后只允许幂等恢复框，不挪节点（与现 L1 精神一致，写权钉死）。  
- `align` 类后处理尽量消失；残留仅 finalize / expert。

### 与现有模块映射

| 今日模块 | 明日归属 |
|----------|----------|
| `group_frame/` | `ComposeKernel` 或 CompositionKernel（doc12 已有 compose 位） |
| `group_layout_hint.rs` | Structure/Intra `ArrangeMode` 解析 + Recipe 内推断；删命名启发式 |
| `align` 后处理 | Coordinate objectives / MinSeparation；DSL 退出 |
| `layout: { group_padding }` | Scheme `LayoutProfile.density` 表 |
| `edge_routing: { slot_pitch }` | `RoutingProfile` + DemandProbe |
| `route_feedback` 推点 | 删除；改 DemandProbe（doc18） |

### 建议 DSL 形态（示意，非冻结语法）

```plotgram
diagram architecture {
    title: "微服务"
    config {
        scheme: architecture-orthogonal
        density: standard
        compose: strips { gap: 50 }    // 仅局部覆盖
    }

    group backend "后端" {
        arrange: fan-out               // 覆盖自动推断；无则 auto
        entity gw "网关"
        entity a "A"
        entity b "B"
    }

    // 结构意图示例（将来）
    // intent { same_rank: [a, b] }
    // intent { side: right, of: main, nodes: [audit] }
}
```

Agent 优先只学：`scheme`、`density`、`compose` 短名、`arrange` 覆盖、少量 `intent`。

---

## 2.4 分阶段落地（与 doc18 对齐）

| 阶段 | 做什么 | 退出判据 |
|------|--------|----------|
| **P0 规格** | 本文 + Intent/Compose 词汇表冻结（名称可改，分层不改） | 评审通过；与 doc18 无矛盾 |
| **P1 编译骨架** | IntentCompiler 空壳；旧 DSL 仍工作，但走「旧属性 → 临时 Intent」适配器 | 行为字节级或门禁可接受不变 |
| **P2 内化 align / density** | align 默认改由 solver；density 映射 padding/gap | showcase 可关 align 开关 |
| **P3 新 DSL 入口** | `scheme` / `compose` / `arrange` / `density` 可用 | 双轨：新旧皆可 |
| **P4 迁 showcase + 文档** | 全部示范图迁新语法；agent 提示词更新 | 旧属性零引用 |
| **P5 删除旧语法** | 解析器报错；删 `group_layout` 旧别名、双 snap、裸 option 日常入口 | language-spec 只留新合约 |

**前置**：doc16 路由冻结契约 + doc14 Recipe 生命周期基本收口后再开 P3 对外语法（P1–P2 可并行）。

---

## 2.5 Agent 友好性要求（验收）

1. **枚举短**：日常 ≤ 1 个 scheme + 1 个 density + 可选 compose 短名 + 可选 arrange。  
2. **冲突可诊断**：两个 intent 互斥时错误码 + 建议，不静默。  
3. **trace**：`picked_scheme`、生效 compose、arrange 来源（显式 / 推断）、被忽略的 intent。  
4. **无命名特判**：禁止 group id 子串驱动布局。  
5. **文档一张表**：Structure / Compose / Scheme 对照；不再维护 L1/L2/L3 与三套指南并行。

---

## 2.6 风险

| 风险 | 缓解 |
|------|------|
| 迁徙期 showcase 大面积失败 | P1 适配器保旧语法；P4 批量迁移 |
| 删 `align` 后个别图变差 | 先把对齐目标沉入 solver，再删 DSL；用 product-gate 对比 |
| Compose 改名导致生态成本 | 短名保留 strips/lanes/stages/tiles 语义；仅换属性根名 |
| 与写权改造抢进度 | P3 绑 doc18 L1 退出条件 |

---

## 2.7 结论

| 问题 | 结论 |
|------|------|
| 当前有什么？ | Diagram 策略、`group_frame`（L1）、group `layout`（L2）、`align`/`snap`（L3）、尺寸 hint、算法 options；组间旧糖已删 |
| 要不要归零？ | **要**——设计与词汇归零 |
| 立刻删光？ | **不要**——迁移后删除 |
| 保留什么？ | 等宽条带/泳道/阶段/网格能力、组内 fan/grid/行列模式、尺寸 hint、可视化属性、Scheme 化后的布局+路由绑定 |
| 去掉什么？ | 分散开关、双 snap、align 日常 DSL、id 启发式、面向用户的旋钮丛林、layout/routing 无总方案的分选（降为覆盖） |
| 新方式？ | **Scheme + Structure Intent + Compose + Finalize**，compile 进 Recipe/IR，DemandProbe 有界反馈，单一写权 |

---

## 附录 A：现状速查（一页）

| 层级 | DSL | 核心内容 |
|------|-----|----------|
| 策略 | `layout`, `edge_routing`, `direction`, options | 算法与流向、数值旋钮 |
| L1 | `group_frame` / strips|fit|lanes|stages|tiles | 组间等宽、对齐、间距、网格、边框、组框 snap |
| L2 | `group.layout` | auto/h/v/fan-out/fan-in/grid（主 architecture） |
| L3 | `align`, `snap` | 路由前节点对齐；路由后量化 |
| 节点 | `style.width/height` | 尺寸 hint |
| 已删 | `group_sizing` 等 | → `group_frame` |

## 附录 B：目标速查（一页）

| 层级 | 新控制面 | 吸收自 |
|------|----------|--------|
| Scheme | `scheme`, `density` | layout + edge_routing + 多数 options |
| Structure | `arrange` / `intent { … }` | group.layout、未来同层/侧置 |
| Compose | `compose: strips|…` | group_frame |
| Finalize | `finalize.snap`（可选） | snap；align 默认消失 |
| 尺寸 | `style.width/height` | 原样保留 |
