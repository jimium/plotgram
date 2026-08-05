# Hierarchical · 边参数支持研究（对照 yFiles Edges 分组）

> 日期：2026-08-05  
> 状态：设计研究；第一批（routing_style 补齐）与第二批（auto_edge_grouping / critical）已落地（见 §4 落地摘要），其余批次实施前单独细化验收  
> 对照：yFiles HierarchicalLayout 的 Edges 配置面板  
> 依据：[architecture.md](architecture.md) · [roadmap.md](roadmap.md) · [写权纪律](../write-authority.md)  
> 相关：[layout-diagnostics 指南](../../../guides/layout-diagnostics.md)

---

## 0. 一句话结论

yFiles 的 8 个 Edges 参数，我们**按写权归位**后没有一个需要新造「第二套布局器」：
2 个已有雏形（routing_style 枚举、回边 dummy 链），3 个是 Compose/Ink 层能力
（backloop / grouping / bus），2 个是 Metric/Demand 层能力
（critical path / min edge length），1 个依赖 Channel（segment 最短约束）。
**不做**的是把任何参数做成 Ink 特判或图种分支。

---

## 1. 现状盘点（MVP + 阶段 A/B/C 之后）

| 既有能力 | 位置 | 与本文关系 |
|----------|------|-----------|
| `routing_style` 四值枚举已 bind | `params.rs` | `orthogonal` / `polyline` / `curved` 三档已落地；`octilinear` 仍硬失败（依赖 Channel） |
| dummy 链正交 Ink（中点双折点 jog） | `ink/route.rs` | 回边、跨层边共用同一展开；无走廊概念 |
| FAS 反转 + 回边走廊侧别（G3） | `compose/cycle.rs` / `ports.rs` | 回边「能画」，但走 rank 方向、折点无界 |
| 自环 | `ink/selfloop.rs` | 已支持 |
| `Edge.critical` 结构字段 | `plotgram-model/graph.rs` | 作者关键路径标记 |
| 布局 `auto_edge_grouping` | `HierarchicalParams` | **自动**同源/同汇合流（§2.3）；无边级 `edge_group` |
| `HierarchicalLayoutData.edge { min_span?, weight?, priority? }` | architecture.md §9.2 | critical 已落地（`Edge.critical` → `RealEdge.critical`，P3/P4 消费）；min length 仍待 DemandBoard |
| DemandBoard | architecture.md §3.3 | 目标协议、代码未立——层间距 demand 回写的唯一合法通道 |
| Channel / track / rip-up | roadmap D₁ | 未实现——segment 约束、bus 干线、边优先级抢占都依赖它 |
| diagram 级 `edge_routing:` 独立 EdgeRouter | `plotgram-router` | 五路由器齐备；与内建 `routing_style` **正交**（architecture.md §5.3） |

---

## 2. 逐参数分析

每个参数回答三问：**映射到什么概念**、**写者是谁**、**现在能不能做**。

### 2.1 Routing Style —— 已有框架，缺非正交实现（polyline/curved 已落地）

yFiles：边路径几何风格（Orthogonal / Polyline / Octilinear / Curved）。

- **映射**：`params.routing_style`，已 bind。四值语义与 yFiles 一致。
- **写者**：`orthogonal` = Channel 拓扑 + Ink 正交展开（主路径）；
  `polyline` = Compose 明写 `RouteTopology::Polyline(DirectOrVia)`，Ink 只展开
  （architecture.md §5.3）；`curved` = 正交骨架上做圆滑（Bezier/圆角曲线），
  属 Ink 落笔变换，不改变拓扑；`octilinear` 需 45° 骨架，**依赖 Channel**。
- **判断**：polyline 与 curved 可在无 Channel 的前提下先做（见 §4 第一批）——
  **已落地**；octilinear 明确后置（roadmap §6 已列）。
- **交互约束**：diagram 级 `edge_routing:` 若显式声明，独立 EdgeRouter 接管，
  内建 `routing_style` 被忽略（现状即如此）。两者同时显式声明时的冲突语义
  **已定死并落地**：显式 `edge_routing` 优先 + diagnostics warning
  （engine run.rs 判定，不硬失败——`edge_routing: none` 是合法用法）。

### 2.2 Backloop Routing —— 缺走廊，不是缺开关

yFiles：回边（指向更上层的边）绕节点侧面/外侧，少穿层、少压正向边。

- **映射**：回边走**跨轴外侧走廊**。我们的回边已有 dummy 链 + G3 侧别，
  但走的是 rank 方向、折点随层跨度增长；yFiles 的 backloop 观感 =
  走廊分配 + track 隔离，即 **Channel（D₁）的能力**，不是一个布尔开关能伪造的。
- **写者**：Channel search 决定回边走哪条走廊（L2）；track 次序归
  TrackOrderWriter（L3）。Ink 不得为回边发明绕行（写权纪律）。
- **判断**：**不支持为布尔参数**。DSL 侧先不引入 `backloop_routing` 键；
  能力随 D₁ 落地后**默认开启**（回边自动走外侧走廊），无需用户开关——
  若将来确需降级行为，再加参数，bind 之前必须有消费者。

### 2.3 Automatic Edge Grouping —— Compose 聚类 + Bus 几何（已落地）

yFiles：同源/同汇的多条边在端口附近合并成一股（共享端口 + 公共干线 +
水平总线 + stub）。开关名：`automaticEdgeGrouping`。

- **映射**：`auto_edge_grouping: bool`（图级参数）。**不**提供边级
  `edge_group`；**不**再用 `cluster_pitch` 在源端口散开出口。
- **写者**：
  - **Compose**：同 `(node, North|South)` FREE 端 → `PortGroup` + `BusPrefix`
    （拓扑：SharedPort → Trunk → Bus → Stub）
  - **Metric**：同簇同 `PortPoint`；写确定性 `bus_y`（层间 trunk 长度）
  - **Ink**：只接合共享干线 / 总线 / stub，不发明合流点
- **边界**：相邻层扇出即可得到图二观感；跨多层 track 级 Bundle 仍归
  Channel / 日后 `bus_routing`。
- **判断**：**已落地**（bus-style v1）。

### 2.4 Automatic Bus Routing —— Channel track 级长程干线

yFiles 独立 `BusRouter` / Channel 上的长程共享 track，**不是** Hier
`automaticEdgeGrouping` 的相邻层扇出（那已由 §2.3 覆盖）。

- **映射**：日后 `bus_routing`（可选），语义 = 跨层 / 拥塞走廊上的
  Bundle track 干线。
- **写者**：Channel track 分配 + Ink trunk/stub 展开；Compose 写 Bundle
  拓扑事实。
- **判断**：依赖 D₁ Channel；与 §2.3 相邻层 bus 几何不冲突。

### 2.5 Highlight Critical Path —— 边权重进排序/坐标目标（已落地）

yFiles：让关键路径更直、更优先；是**布局偏好**，不是渲染描边。

- **映射**：两件事严格分开——
  1. **布局侧**：`HierarchicalLayoutData.edge { priority? / weight? }`
     （目标 IR 已设计）。关键边权重进 median ordering（拉直、少交叉）与
     cross-axis 对齐目标（端点列对齐），Channel 阶段 priority 进 rip-up
     排序（architecture.md §6.3 已有 `edge priority` 词条）。
  2. **渲染侧**：描边高亮是渲染层/style 的事，与布局无关，不在本文范围。
- **DSL 形态**：元素级标记（如 `edge a -> b { critical }` lift 进 typed
  `HierarchicalLayoutData`），**不**做图级布尔——「哪条是关键路径」是作者
  事实，算法不应猜。自动推断（最长路径等）明确不做：不可验证语义不进引擎。
- **判断**：消费点在 P3（ordering）与 P4（坐标），与阶段 A 的「长边更直」
  目标函数是同一批代码路径——搭车实施 ROI 最高。可观测验收：标记边相对
  未标记边的 bend 数 / 列偏移 delta。**已落地**（`Edge.critical` 一等字段；
  P3 链段权重 ×2、P4 VPSC desired 权重 ×2，见 §4 第二批落地摘要）。

### 2.6 / 2.7 Minimum First / Last Segment Length —— Channel 转弯约束

yFiles：边离开源 / 进入目标前的最短直线段，避免贴节点急拐。

- **映射**：`min_first_segment` / `min_last_segment`（数值，默认关 = 0）。
  语义 = 「第一个转弯 track 距源端口 ≥ X」「最后一个转弯距目标端口 ≥ X」。
- **写者**：转弯是 L2 拓扑决策 → **Channel search 的约束/代价**
  （禁近端转弯 track 或加代价）。Ink 现在的中点双折点 jog 若被该参数推着挪
  折点位置，就是「Ink 发明决策」——**禁止**。
- **判断**：**D₁ 前置条件**。在 Channel 落地前，这两个键进
  `bind` 的 unsupported 硬失败列表（与 `edge_gap` 同待遇）。
- **替代观察**：贴节点急拐的观感问题，短期内靠 `layer_gap` 与阶段 A 的
  拉直目标缓解，不用特判模拟。

### 2.8 Minimum Edge Length —— DemandBoard 回写层间距

yFiles：整条边总长下限；短跨（相邻层直连）被撑开，影响层间距。

- **映射**：`min_edge_length`（数值，默认关）。对跨 k 层的边，总长 ≈
  Σ层间距 + 跨轴折线段；相邻层直连边的下限实际约束的是**其间的层间距**。
- **写者**：层间距的写者是 P4 坐标求解；下游需求回写只能走 **DemandBoard**
  （architecture.md §3.3）：Ink/Channel 在预算阶段把「该层间带至少需要 X」
  报上板，合并用 `max`（确定性），唯一参数写者在坐标求解前消费。
  **禁止** finalize 后挪节点撑边长。
- **判断**：DemandBoard 代码未立，此参数与 Board 同批落地（小能力，
  Board 的第一批真实消费方之一，正好验证协议）。Channel 前可先做
  「层间距下限 demand」的最简版：`min_edge_length` 直接换算为相邻层间带
  需求（忽略跨轴折线），完整换算等 Channel。

---

## 3. 支持矩阵（总览）

| yFiles 参数 | 我们的参数名 | 写者/相 | 依赖 | 优先级 | 状态 |
|-------------|--------------|---------|------|--------|------|
| Routing Style | `routing_style`（已有） | Ink 展开 / Compose 拓扑 | polyline/curved 无硬依赖；octilinear 依赖 Channel | ★★ 第一批 | **已落地**（orthogonal/polyline/curved） |
| Backloop Routing | 不做开关 | Channel（D₁） | D₁ | D₁ 随附（默认行为） | 待 D₁ |
| Automatic Edge Grouping | `auto_edge_grouping` | Compose + Metric + Ink | 无（Channel 前做 bus v1） | ★★ 第二批 | **已落地**（bus-style） |
| Automatic Bus Routing | `bus_routing`（Channel track） | Channel + Ink | D₁ | 第四批 | 待 D₁ |
| Highlight Critical Path | 边级 `critical: bool` | P3 ordering / P4 坐标 | 无 | ★★ 第二批（搭阶段 A） | **已落地** |
| Min First Segment | `min_first_segment` | Channel 搜索约束 | D₁ | 第三批（bind 前硬失败） | 待 D₁ |
| Min Last Segment | `min_last_segment` | Channel 搜索约束 | D₁ | 第三批（同上） | 待 D₁ |
| Min Edge Length | `min_edge_length` | DemandBoard → P4 | DemandBoard | 第三批（随 Board） | 待 DemandBoard |

---

## 4. 分阶段实施

与 roadmap 的 A/B/C/D/E 对齐；每批**开工前**再细化验收 fixture。

### 第一批 · 边风格补齐（无新基建）——已收口

**目标**：`routing_style` 四值里补上产品最常用的两档。

1. `polyline`：Compose 写 `RouteTopology::Polyline(DirectOrVia)`——
   相邻层直连不插 dummy，跨层经 dummy 列；Ink 直连折线展开。
2. `curved`：以 orthogonal 骨架为输入，Ink 落笔层做圆滑
   （圆角 → Bezier 输出）。拓扑零变化、折点只减不增。
3. `routing_style` × `edge_routing` 冲突语义定死（显式 EdgeRouter 优先 +
   warning 进 diagnostics）。

**验收**：polyline/curved fixture 进 hier_eval（硬不变量全绿）+ 快照；
orthogonal 几何零变化；`octilinear` 仍硬失败（明示后置）。

**落地摘要**：

- `polyline`：Ink 直接把 waypoints（源锚点 → dummy 中心 → 目标锚点）直线
  相连，不做 bend 插入 / 正交归一（`ink/route.rs`）。
- `curved`：受控限制——仅当两端端口的 canonical side 均为 North/South
  （主轴向）时生效，发射单段 `EdgePath::Cubic`，控制点沿端口法线外推
  `clamp(跨度/3, 24, layer_gap+node_gap)`；含 East/West 端口（G3 无链回边）
  的边硬失败，指向 roadmap D₁。自环恒走 polyline。
- 冲突 warning：engine `run.rs` 判定显式 `edge_routing` + 显式非默认
  `routing_style` → `LayoutDiagnostics.warnings`。
- Ink 内部 `InkPath { Polyline, Cubic }` 枚举，orientation-out 时映射到
  `EdgePath`；模型层 `EdgePath::Cubic` + `samples()` 24 点采样就绪。

### 第二批 · 端口合流与关键路径（Compose / 目标函数）——已收口

**目标**：一对多/多对一的观感 + 主流程视觉主干。

1. `auto_edge_grouping: bool`：Compose 自动聚类同源/同汇边 → `PortGroup` +
   `BusPrefix`；Metric 同锚点 + `bus_y`；Ink 接合干线/总线/stub。
2. 边级 critical 标记 lift 进 `HierarchicalLayoutData.edge.priority/weight`；
   P3 median 权重与 P4 对齐目标消费。

**验收**：fan-out/fan-in fixture 呈 yFiles bus 观感（单出口 + 单干线 +
水平总线）；标记 critical 的链 bend 数不劣于未标记基线。

**落地摘要**：

- **聚类**（compose/ports.rs）：`auto_edge_grouping=true` 时，同一
  `(node, North|South)` 上的 FREE 端合并为一簇并写出 `BusPrefix`。
- **锚点**：同簇成员共享同一 `PortPoint`（无 pitch 散开）。
- **bus_y**（metric/bus.rs）：`port.y + sign(side) * trunk`，
  `trunk = clamp(0.35·layer_gap + …, 12, 32)`。
- **Ink**：`[shared_port, (shared_x, bus_y), (target_x, bus_y), target]`。
- **无效组合**：`auto_edge_grouping=true` × `routing_style=octilinear`；
  键 `cluster_pitch` / `edge_grouping` / `edge_group` 硬失败。
- **验收落地**：`fan/auto_edge_grouping.pgm`、`flat/smoke.fan-out-four.pgm`；
  集成测试断言同源簇成员 `samples()[0]` 相等。

### 第三批 · Demand 与 Channel 前置参数

**目标**：立 DemandBoard，把「参数必须被消费」纪律覆盖到边长/段长域。

1. DemandBoard 最小实现（architecture.md §3.3 协议）。
2. `min_edge_length`：相邻层间带需求最简版（完整换算待 Channel）。
3. `min_first_segment` / `min_last_segment` 进 bind  unsupported 列表
   （占位声明，消费者在 D₁）。

**验收**：min_edge_length fixture（相邻层直连被撑开，撑开量进快照）；
unsupported 键硬失败测试。

### 第四批 · Channel 能力（随 D₁）

实施切片与写权见 [phases/channel-d1.md](phases/channel-d1.md)。参数 ↔ 子里程碑：

| 参数 / 行为 | 子里程碑 | 说明 |
|-------------|----------|------|
| `edge_gap` | **D1.0** | track pitch；LayerGap demand 真消费；恢复 bind |
| 关 grouping 分轨（修假 bus） | **D1.0** | TrackOrder；非参数开关 |
| 回边外侧走廊（默认，无布尔开关） | D1.2 | Channel L2；D1.1 可先占位代价 |
| `min_first_segment` / `min_last_segment` | D1.2 | 搜索约束；此前 bind unsupported |
| `bus_routing` | D1.2 | track 干线 + 支线；依赖 grouping / Bundle |
| grouping → `BundlePlan`；critical → rip-up | D1.2 | BusPrefix 升格；priority 进边选择序 |

---

## 5. 纪律（贯穿所有批次）

1. **参数能 bind 就必须被消费**——否则 bind 硬失败（现状 `edge_gap` /
   `group_sizing` / `group_align` 的待遇）；新参数进 `hash()` canonical 串
   与 params_hash 归因体系。
2. **写权归位**——拓扑归 Channel/Compose，像素归 Metric/VPSC，落笔归 Ink；
   任何「Ink 加特判让边好看」的修法先问写者是谁。
3. **无图种分支**——BPMN 的 critical path、电路图的 bus 都是参数表差异，
   不是新布局器（ADR-001）。
4. **软放宽进 diagnostics**——若某参数在特定图上无法满足而放宽
   （如 bus 在空间不足时降级为普通分组），必须写 `relaxations`，不得静默。
5. **不做**：布尔 backloop 开关（伪能力）、自动关键路径推断（不可验证语义）、
   octilinear 在 Channel 前的近似实现。
