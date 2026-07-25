# 18 - DiagramScheme：布局与路由总配方方案

> 日期：2026-07-24  
> 状态：架构建议，待布局/路由 Recipe 收口后实施  
> 范围：产品层「一个总配方 = 布局 + 边路由 + profile」；路由需求有界反哺节点布局  
> 目标：对外体验对齐 yFiles「选布局方案即含边路由，且换路由风格可微调节点」；对内保持 Layout Kernel / Routing Kernel 分离，用 Coordinator 合成，禁止 route 后推点。  
> 前置阅读：  
> - [`12-多图类型布局共享内核与独立配方架构-2026-07.md`](./12-多图类型布局共享内核与独立配方架构-2026-07.md)  
> - [`14-布局Recipe生命周期收敛方案-2026-07.md`](./14-布局Recipe生命周期收敛方案-2026-07.md)  
> - [`16-路由Recipe-Kernel与求解器架构改造方案-2026-07.md`](./16-路由Recipe-Kernel与求解器架构改造方案-2026-07.md)  
> - [`11-统一坐标约束求解架构与执行计划-2026-07.md`](./11-统一坐标约束求解架构与执行计划-2026-07.md) §7（有界 layout–route feedback）  
> - [`布局与路由核心手册-2026-07.md`](../总结经验/布局与路由核心手册-2026-07.md)

---

## 0. 一句话方案

> **对外选一个 `DiagramScheme`（总配方）；对内仍是 LayoutRecipe + RoutingRecipe 两套 Kernel。路由通过 `RoutingDemandProbe → SpacingDemand → 有界 re-solve` 影响节点，正式 Router 只消费冻结节点，永不拥有节点写权。**

```text
DSL / UI：scheme = flowchart-orthogonal | flowchart-spline | architecture-orthogonal | …
                │
                ▼
         DiagramScheme
     ┌─────────┼─────────┐
     │         │         │
LayoutRecipe  Profiles  RoutingRecipe
     │         │         │
     └──── Coordinator ──┘
           │
  solve → DemandProbe → re-solve(≤2) → freeze → route → label
```

---

## 1. 动机

### 1.1 yFiles 的产品形态

在 yFiles 中：

1. 选择一种**布局方案**时，方案已包含边路由策略；
2. 可在 **profile** 中指定正交 / 曲线等路由风格与参数；
3. 切换边路由后，**节点布局会随之微调**，以便更好地适应所选路由（通道宽、端口密度、曲线净空等）。

这不是「一个超大求解器同时解节点与折线」，而是：

- 产品层把布局与路由绑成**一个方案**；
- 算法层用**有界反馈**让路由需求进入节点间距 / 通道预算。

### 1.2 我们当前的差距

| 维度 | 现状 | 目标 |
|------|------|------|
| 产品入口 | `layout_algo` 与 `edge_routing` 基本独立选择 | 一个 Scheme 同时绑定两者（仍允许显式覆盖） |
| 节点 ↔ 边 | 串行：先布局后路由；反馈多为推点 / 多写者补救 | DemandProbe → SpacingDemand → Coordinate re-solve |
| 路由写权 | Router / post-route 仍可能间接动节点 | Router 只改边；节点写权仅在 freeze 前的 Layout 链 |
| Profile | 散落在图种、OrthoRoutingProfile、pipeline hook | Scheme 级 LayoutProfile + RoutingProfile |

### 1.3 为什么现在记方案、稍后实施

布局侧 Recipe / Coordinate Kernel 与路由侧 Recipe / Solver 改造仍在进行（见 doc14、doc16）。**过早合成总配方会与写权清理打架。** 本文先冻结目标架构与时序契约，实施门槛见 §6。

---

## 2. 目标与非目标

### 2.1 目标

1. **总配方**：`DiagramScheme` 声明「用哪套布局 Recipe + 哪套路由 Recipe + 两套 profile」。
2. **换路由可影响节点**：同一布局族下，正交 / 样条 / 有机等通过不同 DemandProbe 产出不同 SpacingDemand，触发有界节点 re-solve。
3. **写权清晰**：节点仅 Layout 链可写；Router 消费 `FrozenNodeProduct`；label 在几何冻结后独立求解。
4. **DSL/UI 友好**：`scheme: flowchart-orthogonal` 一类入口；高级用户仍可拆开覆盖 layout / routing。
5. **与现有 Kernel/Recipe 同向**：不另起第三套算法，只在 Coordinator / 产品层合成。

### 2.2 非目标

1. **不**把节点坐标与折线路径放进同一个凸优化 / 统一目标函数。
2. **不**在正式 route 之后推节点（恢复旧 space-budget 多写者）。
3. **不**为压单图数字加图名特判。
4. **不**做无限 layout↔route 振荡；固定轮次 + degraded。
5. **首期不**做 yFiles 级交互式局部重布局（编辑器增量另案）。

---

## 3. 核心概念

### 3.1 三层对象

| 概念 | 职责 | 类比 |
|------|------|------|
| **DiagramScheme** | 产品总配方：绑定布局 Recipe、路由 Recipe、profiles、默认图种适用性 | yFiles Layout + EdgeRouter 方案 |
| **LayoutRecipe** | 编译图语义 → 节点布局产品；声明默认 `routing_profile` 倾向 | doc12 / doc14 |
| **RoutingRecipe** | 消费冻结节点 → 边几何；不写节点 | doc16 |

Coordinator 编排生命周期；Kernel 不感知 Scheme 名字。

### 3.2 Profile 分层

```text
DiagramScheme
  ├── LayoutProfile      # 层间距、对称、compact/standard/spacious tier、结构权重
  └── RoutingProfile     # family（ortho/spline/…）、通道/端口/label band、自环外带
```

规则：

- **数值与策略参数**进 profile；
- **是否走某条业务链**（如 architecture hub 居中）由 LayoutRecipe compile 成 typed intent，不堆布尔 hook（doc12 已强调）；
- RoutingProfile 可被 DSL 覆盖，但覆盖后仍走同一 DemandProbe 契约。

### 3.3 与「布局即选配方」的关系

doc14 写「对外 layout 即选配方」。本文升级为：

> **对外首选 Scheme；Scheme 未指定时，可由 `diagram_type + metrics` 自动挑默认 Scheme（含默认路由族）。**

`layout: flowchart` + `edge_routing: orthogonal` 仍可作为显式覆盖，内部归一成临时 Scheme。

### 3.4 Scheme 解析与归一规则

用户输入可能有多种形式，系统按以下**优先级链**归一为最终生效的 `(LayoutRecipeId, RoutingRecipeId, Profiles)`：

```text
优先级（高 → 低）：
  ① 显式 layout + 显式 edge_routing   → 组装临时 Scheme（两者均覆盖）
  ② 显式 layout + scheme.routing      → layout 覆盖，routing 取 scheme 默认
  ③ scheme 显式指定                   → 整体采用
  ④ diagram_type 默认 Scheme          → 自动选中
```

**归一细则：**

| 用户写法 | 解析结果 |
|----------|----------|
| `scheme: flowchart-orthogonal` | 直接命中注册表 |
| `layout: flowchart`（无 routing、无 scheme） | `diagram_type → 默认 Scheme`，routing 取该 Scheme 绑定值 |
| `layout: flowchart` + `edge_routing: spline` | 临时 Scheme = flowchart layout + spline routing + 对应 profiles |
| `scheme: flowchart-orthogonal` + `edge_routing: spline` | 显式 routing 覆盖 scheme 内 routing → flowchart layout + spline routing |
| 无任何指定 | `diagram_type → 默认 Scheme`（如 flowchart → `flowchart-orthogonal`） |

**默认 Scheme 映射表（首期静态）：**

| diagram_type | 默认 Scheme | 默认路由族 |
|---|---|---|
| flowchart | `flowchart-orthogonal` | orthogonal |
| architecture | `architecture-orthogonal` | orthogonal |
| state | `state-orthogonal` | orthogonal |
| er | `er-spline` | spline |
| mindmap | `mindmap-organic` | organic |
| sequence | `sequence-straight` | straight（自产边） |

> 注：sequence 布局自产边几何（`produces_edge_geometry() == true`），DemandProbe 阶段跳过。

**覆盖后仍走同一契约**：无论 Scheme 如何组装，DemandProbe → re-solve → freeze → route 的生命周期不变；仅 probe 内的估算参数随 RoutingProfile 变化。

---

## 4. 路由如何影响节点布局

这是对齐 yFiles「换 edge 路由会微调节点」的**唯一推荐机制**。

### 4.1 正确形态：DemandProbe，不是整管线路由

```text
LayoutRecipe.solve
  → NodeLayoutProduct（未冻结）
  → RoutingDemandProbe（只读、粗粒度、按 RoutingRecipe 族）
  → SpacingDemand / RouteDemand
  → LayoutRecipe.add_spacing_intents + re-solve（≤ 2 轮）
  → NodeFreeze
  → RoutingRecipe.compile/solve（正式几何）
```

`RoutingDemandProbe` **不**产出最终边折点；只估计：

- 端口容量 / 同侧密度；
- 自环外带；
- 邻层边通道带宽；
- 跨 scope 走廊容量；
- label 最小 band；
- （正交特有）侧廊 / 反向对最小 gap；
- （曲线特有）端点净空与弯曲半径预算。

输出统一为布局可吸收的 `SpacingDemand`（或等价 `CoordinateIntent`），带 `source` / `edge` provenance。

正式 Router 只在节点 freeze 之后运行（与 doc16 §16.2 一致）。

### 4.2 各路由族的典型 demand 差异

| 路由族 | 更可能推高的布局需求 |
|--------|----------------------|
| orthogonal | 同层通道宽、侧廊、端口槽位、反向对 gap、自环外带 |
| bezier / spline | 端点净空、更少强制通道、label band |
| organic | 局部排斥半径、障碍膨胀 |
| circular / radial | 扇区角间距、径向净空 |

差异只在 **Probe / Recipe compile**；Coordinate Kernel / 分层 Kernel **不**出现 `if routing_family`。

### 4.3 有界反馈契约（硬）

沿用 doc11 §7.3 / doc16 §16：

1. 最多 **1–2** 轮 demand → re-solve。
2. 每轮只增加或显式替换 spacing intent，不静默改目标。
3. **禁止** route 后直接推节点；不足则 `UnresolvedRoutingDemand`，由上层决定下一次完整 layout run 吸收或 degraded。
4. re-solve 后受影响边全部重路由（正式阶段）。
5. 第二轮仍压不住：保留最佳正确结果并标记 degraded，禁止振荡。
6. 确定性：probe 与 demand 枚举按稳定 id / 声明序排序。

### 4.4 错误形态（禁止）

| 错误做法 | 为何禁止 |
|----------|----------|
| 跑完整正交管线再推节点 | 多写者、与 freeze 契约冲突、易假回归 |
| 一个目标函数同时优化节点坐标与折线 | 尺度不同、难维护、确定性差 |
| refine / sanitize 偷偷挪节点补缝 | 写权失明；手册 ★1–2 |
| 按 showcase 图名加宽通道 | 违反无图名特判 |

### 4.5 Probe 估算策略（最小可行示例）

Probe 的核心原则：**只算“需要多少空间”，不算“具体怎么走”**。以下为各路由族的最小可行估算公式（L1 实现键点）：

#### orthogonal probe

```text
# 邻层通道带宽（最主要的布局影响因子）
channel_demand(layer_i, layer_j) =
    edges_between(i, j) × parallel_gap
    + max_port_density(i, j) × slot_pitch

# 侧廊需求（反向边 / feedback 边）
side_gutter_demand(node) =
    has_feedback_edges(node) ? reverse_pair_gap + margin : 0

# 自环外带
self_loop_band(node) =
    has_self_loop(node) ? loop_radius + clearance : 0

# 端口槽位（同侧多边）
port_slot_demand(node, side) =
    edges_on_side(node, side) × slot_pitch - node_extent(side)
    # 若 > 0，说明节点该侧边长不足以容纳所有端口，需拉大节点间距
```

#### spline / bezier probe

```text
# 端点净空（曲线起始段需要无遮挡空间）
endpoint_clearance(node) =
    max_edge_count_per_side(node) × curve_shoulder × 0.5

# label band（曲线中点附近标签带）
label_band_demand(edge) =
    has_label(edge) ? label_height + label_padding : 0

# 弯曲半径预算（曲线比正交需要更少的强制通道，但需要更大的层间距离）
bend_clearance = curvature_radius × 1.2  # 比正交通道窄，但比直线宽
```

#### organic probe

```text
# 局部排斥半径（有机曲线需要节点周围更大的净空）
repulsion_radius(node) =
    base_clearance × (1 + edge_degree(node) × degree_factor)

# 障碍膨胀（曲线绕行需要更大的障碍物间距）
obstacle_inflation = base_clearance × 1.5
```

**估算精度原则：**

- 允许过估 20–40%（布局略宽松可接受，过紧不可接受）；
- 禁止欠估导致正式路由时空间不足（产生穿模 / 重叠）；
- 若不确定，取较大值（宁宽勿紧）。

### 4.6 性能预算与 Early-Exit

re-solve 意味着 Coordinate Kernel 可能跑 2–3 次。性能约束：

| 图规模 | 单次 solve 目标 | 总管线目标（含 probe + re-solve） |
|--------|--------------|-------------------------------|
| ≤ 30 节点 | < 5ms | < 20ms |
| 30–100 节点 | < 20ms | < 60ms |
| 100–300 节点 | < 80ms | < 200ms |
| > 300 节点 | < 200ms | < 500ms |

> 注：以上为 debug profile 参考值；release 通常快 3–5×。WASM 环境无 perf_log 时不计时。

**Early-Exit 条件（跳过第二轮 re-solve）：**

1. **增量忽略**：第二轮 probe 产出的 `SpacingDemand` 与第一轮已吸收的 demand 差异 < ε（建议 ε = 2px），则不再 re-solve。
2. **无新增 demand**：probe 返回 `spacing.is_empty()`，直接 freeze。
3. **大图降级**：节点数 > 300 时，最多 1 轮 re-solve（而非 2 轮），避免三次 solve 的累计耗时。
4. **坐标收敛**：re-solve 后所有节点最大位移 < 1px，视为已收敛，不再迭代。

**Probe 本身的成本约束：**

- Probe 复杂度应为 O(E)（遍历边统计邻层计数 / 端口密度），不得出现 O(E²) 的边对比较；
- 禁止在 probe 内构建可见性图 / 跑 A* / 构建 OVG；
- 允许预排序（按层 / 按节点 id），但排序复杂度不计入 re-solve 轮次。

---

## 5. 目标生命周期

与 doc14 Phase R5、doc16 §16.1 对齐，产品入口换成 Scheme：

```text
Prepare(diagram)
  → 解析 / 选择 DiagramScheme
  → LayoutRecipe.compile(PreparedLayoutInput, LayoutProfile)
  → LayoutRecipe.solve
  → NodeLayoutProduct
  → RoutingDemandProbe(RoutingProfile / family)
  → 如需空间：SpacingDemand → add_spacing_intents → re-solve（≤ 2）
  → Group/Node hard audit
  → FrozenNodeProduct
  → RoutingRecipe.compile(FrozenNodeProduct, RoutingContract, RoutingProfile)
  → RoutingRecipe.solve / bounded repair（只改边）
  → RouteAuditor → FrozenRouteGeometry
  → LabelSolver
  → atomic canvas transform
```

**写权表（摘要）**

| 阶段 | 可写节点 | 可写边几何 |
|------|----------|------------|
| Layout solve / re-solve | ✅ | ❌（或仅占位） |
| DemandProbe | ❌（只读） | ❌ |
| Node freeze 之后 | ❌ | — |
| Routing solve / repair | ❌ | ✅ |
| Label | ❌ | 标签几何 only |
| Canvas transform | 刚体变换（唯一、原子） | 同左 |

---

## 6. 实施路线图

### L0 — 现状（进行中，不改本文目标）

- LayoutRecipe / Coordinate Kernel 收敛（doc14）。
- RoutingRecipe / Solver / 写权冻结（doc16）。
- 布局与路由仍可独立选择；feedback 逐步从推点改为 demand。

**退出**：正交路由主链走 Recipe；节点 freeze 后 Router 无节点写权；Demand/Spacing IR 可测。

### L1 — 契约加固（总配方前置条件）

1. `LayoutRecipe::routing_profile()` 稳定返回（doc12 / doc14 R1）。
2. 实现族相关 `RoutingDemandProbe` 接口（可先 orthogonal + straight/spline 两档）。
3. `route_feedback` 收口为 probe → spacing → re-solve；删除「正式 route 后再推节点」路径。
4. 文档与代码写权表一致（对照 doc16-R0）。

**退出**：换 `RoutingProfile` 中与间距相关的参数，能在 freeze 前观察到节点坐标按 demand 变化；正式 route 后节点坐标字节级不变（刚体 transform 除外）。

### L2 — DiagramScheme 产品层

1. 引入 `DiagramScheme` 注册表（静态表即可，无需动态插件首期）。
2. 预置方案示例：
   - `flowchart-orthogonal`
   - `flowchart-spline`（或 bezier）
   - `architecture-orthogonal`
   - `state-circular` / `mindmap-organic` 等按图种默认
3. DSL：`scheme: <name>`；缺省时 `diagram_type → 默认 Scheme`；显式 `layout` / `edge_routing` 覆盖并归一。
4. Trace：`picked_scheme`、layout/routing recipe 名、tier、主要 weights、demand 轮次。

**退出**：showcase / product-gate 可用 scheme 入口跑通；覆盖语法行为可测。

### L3 — 体验对齐（换路由微调节点）

1. 同一 LayoutRecipe 下切换 RoutingRecipe / RoutingProfile，DemandProbe 产出可区分的 SpacingDemand。
2. 对比基线：正交 vs 样条在通道密集图上节点间距差异可解释、确定性稳定。
3. 可选：`layout: auto` + metrics tier 与 Scheme 组合（衔接 doc14 R6）。

**退出**：产品说明可写「更换边路由风格时，布局会按通道/净空需求有界微调」；无无限反馈、无 route 后推点。

---

## 7. 接口草图（非冻结 API）

> 名称可在落地时调整；语义不得违背 §4–§5。

```rust
/// 产品总配方：绑定布局与路由，不实现算法。
pub struct DiagramScheme {
    pub id: &'static str,
    pub layout: LayoutRecipeId,
    pub routing: RoutingRecipeId,
    pub layout_profile: LayoutProfile,
    pub routing_profile: RoutingProfile,
    pub applicable: &'static [DiagramType],
}

pub trait RoutingDemandProbe {
    fn family(&self) -> GeometryFamily;

    /// 只读估计；不写 LayoutResult。
    fn probe(
        &self,
        diagram: &Diagram,
        nodes: &NodeLayoutProduct,
        profile: &RoutingProfile,
    ) -> DemandProbeReport;
}

pub struct DemandProbeReport {
    pub spacing: Vec<SpacingDemand>,
    pub unresolved_hints: Vec<UnresolvedRoutingDemand>,
    pub diagnostics: DemandProbeDiagnostics,
}

/// Coordinator 入口（示意）
pub fn run_scheme(
    diagram: &Diagram,
    scheme: &DiagramScheme,
) -> Result<LayoutResult, LayoutError>;
```

**SpacingDemand 与相关结构（补充）：**

```rust
/// 路由族向布局层提出的空间需求（原子单位）。
#[derive(Debug, Clone)]
pub struct SpacingDemand {
    /// 需求来源（哪条边 / 哪个节点触发）。
    pub source: DemandSource,
    /// 需求类型。
    pub kind: DemandKind,
    /// 需求的最小间距（px）。
    pub min_spacing: f64,
    /// 作用方向（水平 / 垂直 / 径向）。
    pub axis: DemandAxis,
    /// 受影响的节点对 / 层对（布局侧按此局部应用）。
    pub scope: DemandScope,
}

#[derive(Debug, Clone)]
pub enum DemandSource {
    /// 由某条边触发（声明序下标）。
    Edge(usize),
    /// 由某个节点触发（如自环、端口密度）。
    Node(String),
    /// 全局性（如图层通道）。
    Global,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DemandKind {
    /// 邻层通道带宽。
    ChannelBandwidth,
    /// 侧廊（反向边 / feedback）。
    SideGutter,
    /// 自环外带。
    SelfLoopBand,
    /// 端口槽位不足。
    PortSlotOverflow,
    /// 端点净空（曲线族）。
    EndpointClearance,
    /// 标签带。
    LabelBand,
    /// 排斥半径（有机族）。
    RepulsionRadius,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DemandAxis {
    Horizontal,
    Vertical,
    Radial,
    /// 沿布局主轴（由 LayoutRecipe 解释）。
    MainFlow,
}

#[derive(Debug, Clone)]
pub enum DemandScope {
    /// 作用于两个节点之间。
    NodePair(String, String),
    /// 作用于两层之间（层 id）。
    LayerPair(usize, usize),
    /// 作用于某节点的所有邻居。
    NodeNeighborhood(String),
    /// 全局。
    All,
}

/// Probe 诊断信息（写入 trace，不影响算法行为）。
#[derive(Debug, Clone, Default)]
pub struct DemandProbeDiagnostics {
    pub family: String,
    pub total_demands: usize,
    pub max_demand_px: f64,
    pub elapsed_us: u64,
    /// 是否触发了 early-exit。
    pub early_exit: bool,
    pub early_exit_reason: Option<String>,
}
```

**LayoutRecipe 吸收 demand 的接口（已有，补充语义）：**

```rust
impl LayoutRecipe {
    /// 将 probe 产出的 SpacingDemand 转化为 CoordinateIntent 并注入求解器。
    /// 布局 Kernel 不感知 demand 来自哪个路由族——只看到“某对节点/层需要 X px 间距”。
    pub fn add_spacing_intents(&mut self, demands: &[SpacingDemand]);

    /// 重新求解（增量式：仅调整受 demand 影响的局部坐标）。
    pub fn re_solve(&mut self) -> NodeLayoutProduct;
}
```

`LayoutRecipe` 继续保留 `add_spacing_intents` + `solve`（doc12）；Scheme 不替代 Recipe，只负责选型与 profile 组装。

---

## 8. 与既有文档的边界

| 文档 | 本文关系 |
|------|----------|
| doc11 | SpacingIntent / 有界 feedback 的坐标侧细则；本文引用不改写 |
| doc12 | Layout Kernel/Recipe；本文在其之上加 Scheme 产品层 |
| doc14 | LayoutRecipe 生命周期；R5 Coordinator 是 Scheme 的挂载点 |
| doc16 | RoutingRecipe 与 §16 时序契约；本文 L1/L3 依赖其完成 |
| 手册 | ★ 写权、确定性、禁图名特判；Scheme 实施同样适用 |

本文**不**重复正交 Solver 内部阶段设计，也不替代 LayoutRecipe 拆分计划。

---

## 9. 原则与红线

| # | 原则 |
|---|------|
| 1 | Scheme 是**合成与选型**，不是第三套算法内核 |
| 2 | 路由影响布局 **只**经 DemandProbe → SpacingDemand → re-solve |
| 3 | 正式 Router **零**节点写权 |
| 4 | Kernel 无 `DiagramType` / 无 Scheme 名分支；差异在 Recipe/Probe compile |
| 5 | 确定性迭代（`AGENTS.md` §2） |
| 6 | 禁止图名特判；应力图质量差记债或抬探针基线 |
| 7 | 验证用 `cargo run -p plotgram-cli`；日常不默认 `--release` |
| 8 | WASM 禁裸 `std::time::{Instant, SystemTime}` |

---

## 10. 风险与缓解

| 风险 | 缓解 |
|------|------|
| Probe 过粗，换路由节点几乎不动 | L3 用可控 fixture 校验 demand 灵敏度；按族补估计项 |
| Probe 过细 ≈ 半套路由，成本高 | 粗粒度带宽/容量估计；禁止在 probe 内跑完整 path solver |
| Scheme 与显式 layout/routing 冲突 | 归一规则写死：显式覆盖 > scheme > diagram 默认 |
| 与门禁「节点坐标不变」叙事冲突 | L3 起「换 routing profile」的对比集单独标注预期漂移；同 scheme 回归仍比坐标 |
| 过早实施干扰 doc16 写权清理 | **L2/L3 以 L1 退出判据为硬门槛** |
| 用户无感知 degraded，误以为布局已最优 | 见下方「Degraded 用户感知策略」 |

**Degraded 用户感知策略：**

当 probe + re-solve 后仍有未满足的路由空间需求时，系统需向用户透明报告：

| 严重度 | 条件 | 用户可见行为 |
|--------|------|----------------|
| silent | 所有 demand 已吸收，无 degraded | 无额外输出 |
| info | 有 `UnresolvedRoutingDemand` 但不影响正确性（仅美观） | trace 中记录 `demand_degraded: [...]`；playground 可在面板展示 |
| warning | 路由产生穿模 / 重叠但已用最佳结果 | DSL 诊断输出 warning（类似现有 `DiagnosticError` 机制） |
| error | 完全无法路由（极端密集 + 空间不足） | 返回错误，建议用户调整 scheme 或减少边数 |

实现要点：

- `DemandProbeReport.unresolved_hints` 携带未满足项，Coordinator 在正式路由后比对实际几何与 demand，确认是否真正 degraded；
- trace 输出示例：`[demand] round=2 degraded=[ChannelBandwidth(layer_1,layer_2): need 48px, got 36px]`；
- playground / CLI 可通过 `--trace` 或 verbose 模式查看；默认仅 warning 以上可见。

---

## 11. 建议落地顺序（摘要）

1. **先做完** doc16 正交 Recipe 主链 + 节点 freeze 契约（并行可写本文接口空壳，不接线）。
2. **L1**：DemandProbe + spacing re-solve 替换 route 后推点。
3. **L2**：`DiagramScheme` 注册表 + DSL `scheme:`。
4. **L3**：正交 vs 曲线 demand 差异可观测；文档与 playground 可选方案列表。

---

## 12. 验收一句话

用户选择（或自动选中）一个 `DiagramScheme` 后，系统按绑定的布局与路由配方运行；在节点冻结前，路由族相关的空间需求经最多两轮 re-solve 进入节点布局；冻结后边路由不再移动节点；切换正交 / 曲线等路由风格时，节点间距与通道预算有可解释、确定性的差异。

---

## 13. 端到端示例

以一个 5 节点 3 层 flowchart 为例，演示同一图在正交 vs 样条两种 Scheme 下的完整管线差异。

### 13.1 输入 DSL

```plotgram
diagram flowchart {
  scheme: flowchart-orthogonal   // 或 flowchart-spline
}

A["Start"] --> B["Process"]
A --> C["Validate"]
B --> D["Merge"]
C --> D
D --> E["End"]
```

图结构：3 层（rank0: A，rank1: B/C，rank2: D，rank3: E），5 条边。

### 13.2 正交 Scheme 管线 trace

```text
[scheme] resolved: flowchart-orthogonal
[layout] recipe=flowchart, direction=top-to-bottom
[layout] solve: 4 ranks, 5 nodes
  rank0: A(y=0)
  rank1: B(y=80), C(y=80)
  rank2: D(y=160)
  rank3: E(y=240)

[probe] family=orthogonal
[probe] channel_demand(rank0→rank1): 2 edges × 12px gap = 24px
[probe] channel_demand(rank1→rank2): 2 edges × 12px gap = 24px
[probe] port_slot_demand(B, bottom): 1 edge × 10px = 10px < node_width(100) → OK
[probe] port_slot_demand(D, top): 2 edges × 10px = 20px < node_width(100) → OK
[probe] side_gutter: 0 (no feedback edges)
[probe] self_loop_band: 0 (no self loops)
[probe] total_demands=2, max=24px, elapsed=3µs

[demand] round=1: SpacingDemand [
  { kind: ChannelBandwidth, scope: LayerPair(0,1), min_spacing: 24px, axis: MainFlow },
  { kind: ChannelBandwidth, scope: LayerPair(1,2), min_spacing: 24px, axis: MainFlow },
]
[demand] current layer_gap=80px > 24px → already satisfied, skip re-solve
[demand] early_exit=true, reason="all demands satisfied by initial layout"

[freeze] nodes frozen (5 nodes)
[route] orthogonal compile: 5 edges, 0 self-loops, 0 feedback
[route] orthogonal solve: 5 polylines, 0 violations
[audit] passed: no crossings, no group pierce
[label] 0 labels
[canvas] finalize: 220×300px
```

本例中初始布局的层间距（80px）已远超正交通道需求（24px），probe 触发 early-exit，无 re-solve。

### 13.3 样条 Scheme 管线 trace（对比）

```text
[scheme] resolved: flowchart-spline
[layout] recipe=flowchart, direction=top-to-bottom
[layout] solve: (same as above)

[probe] family=spline
[probe] endpoint_clearance(A, bottom): 2 edges × shoulder(16) × 0.5 = 16px
[probe] endpoint_clearance(D, top): 2 edges × shoulder(16) × 0.5 = 16px
[probe] label_band: 0 (no labels)
[probe] bend_clearance: curvature_radius(20) × 1.2 = 24px
[probe] total_demands=1, max=24px, elapsed=2µs

[demand] round=1: SpacingDemand [
  { kind: EndpointClearance, scope: NodeNeighborhood("A"), min_spacing: 16px, axis: MainFlow },
  { kind: EndpointClearance, scope: NodeNeighborhood("D"), min_spacing: 16px, axis: MainFlow },
]
[demand] current gap=80px > 16px → satisfied, skip re-solve
[demand] early_exit=true

[freeze] nodes frozen
[route] spline compile: 5 edges
[route] spline solve: 5 cubic beziers
[canvas] finalize: 220×300px
```

### 13.4 密集图示例（触发 re-solve）

假设 rank1 有 8 个节点，rank0→rank1 有 12 条边：

```text
[probe] family=orthogonal
[probe] channel_demand(rank0→rank1): 12 edges × 12px = 144px
[probe] port_slot_demand(A, bottom): 12 edges × 10px = 120px > node_width(100) → overflow=20px
[probe] total_demands=2, max=144px

[demand] round=1: SpacingDemand [
  { kind: ChannelBandwidth, scope: LayerPair(0,1), min_spacing: 144px, axis: MainFlow },
  { kind: PortSlotOverflow, scope: NodeNeighborhood("A"), min_spacing: 20px, axis: Horizontal },
]
[demand] current layer_gap=80px < 144px → re-solve needed
[re-solve] layer_gap(rank0→rank1): 80px → 148px (144 + 4px margin)
[re-solve] node A width intent: expand neighbors spacing by 20px
[re-solve] max displacement: 68px (rank1/2/3 all shifted down)

[probe] round=2: re-check
[probe] channel_demand(rank0→rank1): 144px ≤ current_gap(148px) → OK
[demand] early_exit=true, reason="all demands satisfied after round 1"

[freeze] nodes frozen
[route] orthogonal solve: 12 polylines in 148px channel → 0 violations
```

**对比：同一图用样条 Scheme：**

```text
[probe] family=spline
[probe] endpoint_clearance: 12 edges × 16 × 0.5 = 96px (分散到多个节点，单节点 max=32px)
[probe] bend_clearance: 24px
[demand] max=32px < current_gap(80px) → satisfied, no re-solve
```

→ 样条不需要 re-solve，节点保持原始布局；正交需要拉大层间距。**这就是「换路由风格微调节点」的可观测差异。**

### 13.5 小结

| 维度 | flowchart-orthogonal | flowchart-spline |
|------|---------------------|------------------|
| Probe 主要 demand | ChannelBandwidth (144px) | EndpointClearance (32px) |
| 是否触发 re-solve | 是（层间距 80→148） | 否 |
| 节点坐标差异 | rank1+ 下移 68px | 无变化 |
| 路由结果 | 12 条正交折线，无穿模 | 12 条贝塞尔曲线，无穿模 |
| 确定性 | 同一输入多次渲染字节一致 | 同左 |
