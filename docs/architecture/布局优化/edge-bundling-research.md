# Edge Bundling（边捆绑）研究方案

## 1. 问题背景

### 1.1 「意大利面」问题

当流程图、架构图中边数量增多时，独立路由的边会产生大量交叉和视觉混乱，形成「意大利面」（spaghetti）效果：

- 边与边交叉过多，难以追踪单条边的路径
- 相同流向的边分散在不同通道，视觉上无法感知整体流向
- 高密度区域（如公共总线、主干通道）边重叠但未真正合并，显得杂乱
- 长距离跨层边绕过障碍物时各自选择不同路径，增加视觉噪音

### 1.2 Edge Bundling 的目标

边捆绑（Edge Bundling）通过将几何上相似、方向相近的边「捆成一束」共享路径，在保持边端点连接正确的前提下：

1. **减少视觉交叉**：相似边合并到共同主干，减少交叉点数
2. **强化流向感知**：通过束的粗细和密度直观展示流量模式
3. **降低 Ink 量**：总绘制长度减少，图面更简洁
4. **突出主干通道**：高密度束对应主干通道，低密度束对应分支

---

## 2. 现有代码基础分析

### 2.1 当前已有的「类 bundling」能力

当前 orthogonal 路由已实现**节点级入口汇流**，但缺少**跨节点全局路径捆绑**：

| 特性 | 位置 | 范围 | 说明 |
|------|------|------|------|
| 平行边分离 | [parallel_edges.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/edge/common/parallel_edges.rs) | 同一节点对 | 同一对节点间多条边垂直偏移，是**分离**而非捆绑 |
| 端口并线分组 | [edge_routing_orthogonal/mod.rs#L268-L318](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/edge/edge_routing_orthogonal/mod.rs#L268-L318) | 节点端口 | 同节点同侧同线型的边共享锚点带 |
| Concentrate trunk+fork | [edge_routing_orthogonal/mod.rs#L365-L382](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/edge/edge_routing_orthogonal/mod.rs#L365-L382) | 节点附近 | ≥4 条边时，节点外 20px 共享一段 trunk 再分叉 |
| 路径重叠惩罚 | [edge_routing_orthogonal/mod.rs#L181](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/edge/edge_routing_orthogonal/mod.rs#L181) | 全局路由 | `EDGE_OVERLAP_PENALTY = 1200`，**惩罚重叠**，与捆绑目标相反 |

### 2.2 当前架构的关键集成点

边路由流水线（[mod.rs#L1158-L1222](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/mod.rs#L1158-L1222)）：

```text
节点布局完成
    ↓
friendliness V2 调整（节点位移）
    ↓
router.route() → 各边独立路由，生成 EdgeLayout
    ↓
refine（穿障修正，推开节点）            ← route 之后立即执行
    ↓
V2 路由后验证：baseline vs v2 择优      ← post_route_select
    ↓
grid_snap（waypoint 量化）
    ↓
repulse_edges_from_group_borders（group 边框排斥）
    ↓
finalize_canvas_bounds
```

**关键观察**：`route()` 之后**立即**执行 `refine`（[L1184-L1186](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/mod.rs#L1184-L1186)），中间无插入间隙；V2 模式下还会路由两次并择优（[L1189-L1201](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/mod.rs#L1189-L1201)）；`grid_snap` 和 `repulse_edges_from_group_borders`（[L1203-L1222](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/mod.rs#L1203-L1222)）都会修改 waypoint 坐标。

**Bundling 插入点：`repulse_edges_from_group_borders` 之后、`finalize_canvas_bounds` 之前**（[L1222](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/mod.rs#L1222) 之后），理由：
1. **必须在 V2 择优之后**：否则两次路由（baseline + v2）都要 bundling，浪费性能；且择优比较的是未 bundling 的原始指标，bundling 后指标变化可能导致择优翻转
2. **必须在 refine 之后**：refine 会推开节点修正穿障，若 bundling 在 refine 之前，refine 推开节点后主干坐标可能穿障，且 bundling 重写的路径可能重新穿障而 refine 已跑过无法修正
3. **必须在 grid_snap + group repulse 之后**：这两个操作会量化 waypoint、推开 group 边框附近的边，破坏 bundling 主干的精确坐标。bundling 应作为**最后一步几何后处理**，输出后不再有任何几何修改
4. **不侵入各路由器内部逻辑**：straight/bezier/spline/orthogonal 均可受益（曲线 bundling 为远期可选）
5. **可通过配置开关控制启用/禁用/强度**

### 2.3 PathGeometry 数据结构

当前路径几何（[mod.rs#L137-L153](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/mod.rs#L137-L153)）：

```rust
pub enum PathGeometry {
    Straight { start, end },
    Bezier { start, end, controls: [(f64,f64); 2] },
    Polyline { points: Vec<(f64,f64)> },  // orthogonal 输出此类型
}
```

捆绑需要处理的主要是 `Polyline`（正交折线），Bezier/Straight 可先采样为折线再处理。

### 2.4 与现有汇流合并逻辑的关系：哪些保留、哪些删掉

当前 orthogonal 路由的「汇流合并和公线」实际上是**三层递进**的逻辑，职责不同，不能一刀切全删：

| 层级 | 位置 | 作用 | 去留 | 理由 |
|------|------|------|------|------|
| **L1 端口侧选择 + 锚点分组** | [mod.rs#L254-L318](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/edge/edge_routing_orthogonal/mod.rs#L254-L318) | 决定边从哪侧进/出；按 `(arrow_type, line_style, is_from)` 分子组 | **必须保留** | 端口侧选择是路由起点，没有路径就没法 bundling；分组约束（不同箭头/线型不并线、出边入边不混）是有语义的，bundling 也必须遵守 |
| **L2 DockingStrategy 锚点分布** | [slot.rs#L11-L27](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/edge/edge_routing_orthogonal/slot.rs#L11-L27) | Single(1条)/Compact(2-3条紧凑)/Concentrate(4+条合并到一点) | **保留但弱化** | Single 是基础设施；Compact 对 2-3 条边足够好用（轻量、无需 bundling）；Concentrate 的「锚点合并到一点」可保留作为入口预处理（让 bundling 合入段更整齐），但不需要它做「合并」这件事本身 |
| **L3 Concentrate trunk+fork** | [mod.rs#L363-L382](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/edge/edge_routing_orthogonal/mod.rs#L363-L382) | 节点外 20px 共享一段短 trunk 再分叉 | **直接删掉** | 是新 Edge Bundling 的真子集且能力极弱：trunk 只有 20px 形同虚设、无方向感知、所有边同点分叉易交叉、Ink 节省可忽略 |

**L3 与新 Bundling 的能力对比**：

| 维度 | 旧 Concentrate trunk+fork | 新 Edge Bundling |
|------|--------------------------|-----------------|
| 共享段长度 | 固定 20px，刚出节点就分叉 | 跨节点共享长主干，几十到几百像素 |
| 决策范围 | 只看单节点单侧的边数 | 全局看路径相似性、方向、区域 |
| 分叉位置 | 所有边在同一个点分叉，易扇形交叉 | 每条边按目标位置在主干不同位置有序分叉 |
| Ink 节省 | ~20px × (n-1)，几乎可忽略 | 目标节省 15%~25% 总 Ink |
| 方向感知 | 无，进来的边全往一个点凑 | 有，反向边不会捆在一起 |

**约束传递**：L1 分组的语义约束（`arrow_type` 不同不并线、`line_style` 不同不并线、出边入边不混）需要**传递给 bundling 的兼容性评估**，作为硬条件——不满足的边对 compatibility 直接为 0 分。

**推荐的分层架构**：

```
L1: 端口侧选择 + 锚点分组（保留）
      ↓
L2: DockingStrategy（保留，简化 Concentrate 为「锚点合并到一点」，去掉 trunk+fork）
      ↓
   正交路由（保留不变，输出 Polyline）
      ↓
L4: Edge Bundling（新增，完全替代旧 L3）
      - 兼容性评估（含 arrow_type/line_style 硬约束）
      - 确定性聚类
      - 主干通道分配 + 分叉点计算
      - 路径重写 + 穿障回退
```

**长远可选项**：等 Bundling 稳定后，L2 的 Concentrate 也可以进一步收掉——所有边都用 Single 模式均匀分布，「入口是否汇聚」完全交给 Bundling 决策，路由层更纯粹。但这是后续优化，不在第一期范围内。

### 2.5 与现有管线的精确集成契约

为避免实现阶段的歧义，本节明确 bundling 与各现有子系统的交互契约：

| 契约项 | 规约 | 理由 |
|--------|------|------|
| **插入点** | `repulse_edges_from_group_borders` 之后、`finalize_canvas_bounds` 之前 | 见 §2.2，bundling 是最后一步几何后处理 |
| **V2 择优** | bundling 在 `post_route_select` 择优之后执行，**只对最终选中的结果做一次** | 避免双倍开销；避免指标翻转 |
| **lane 处理** | bundling **覆盖** lane 分离：同 bundle 内的边强制共享主干，原 lane 信息在主干段失效；分叉段恢复各自路径 | lane 是路由阶段的反重叠手段，bundling 主干段的重叠是故意的（见 §4.9）。但 `parallel_edges.rs` 的同节点对平行边分离**仍保留**——它处理的是同一节点对的多条边，与跨节点 bundling 正交 |
| **L3 共存策略** | P0 起通过 feature flag（`orthogonal.concentrate_trunk_fork: false`）**关闭 L3**，让 bundling 在干净基准上开发和验证；P5 仅删除死代码 | L3 与新 bundling 共存会出现"双重 bundling"（L3 先做 20px trunk，bundling 再做长 trunk），干扰效果评估和 Ink 节省计算 |
| **Ink 节省基准** | 基准 = L3 关闭后的纯单边路径长度（即 `router.route()` + `refine` + `grid_snap` 的输出） | 保证基准干净，不受 L3 干扰 |
| **后续几何修改** | bundling 输出后，**禁止**修改 `PathGeometry` / waypoint；**允许**修改 `EdgeLabelLayout`（见 §4.10） | 保证主干坐标稳定，同时 label 能锚定新路径 |
| **标签布局** | bundling **之后**执行完整后置 label 流水线（§4.10，默认 **SegmentAware** 独占段锚定）；`BundlingResult.edge_roles` + `trunk_keepouts` 供 label 层查询 |  label 关联未合并段，避免悬空与主干堆叠 |

---

## 3. Edge Bundling 算法调研

### 3.1 主流算法分类

| 算法 | 发表 | 类型 | 适用场景 | 复杂度 | 确定性 |
|------|------|------|----------|--------|--------|
| **HEB** (Hierarchical Edge Bundling) | Holten 2006 | 层次引导 | 有树/层次结构的图（包依赖、组织架构） | O(E·P) | 是 |
| **FDEB** (Force-Directed Edge Bundling) | Holten & van Wijk 2009 | 力导向物理模拟 | 地理图、一般图（节点位置固定） | O(I·E·C·P) | 否 |
| **SBEB** (Skeleton-Based Edge Bundling) | Ersoy et al. 2011 | 图像骨架 | 大图、高密度图 | O(R²+E·P) | 是 |
| **MINGLE** | Gansner et al. 2011 |  Ink 节省优化 | 多层图、大规模图 | O(E log E) | 是 |
| **FFTEB** | Lhuillier et al. 2017 | FFT 加速密度场 | 实时交互、百万级边 | O(E·P + R² log R) | 是 |
| **CUBu** | van der Zwan et al. 2016 | GPU 加速 | 实时 bundling | GPU 并行 | 是 |

> **注**：P = 每条边控制点/采样点数，I = 力模拟迭代轮次，C = 兼容边对数量，R = 渲染分辨率

### 3.2 算法核心思想对比

#### FDEB（力导向边捆绑）——最通用的基线

- 每条边采样为 P 个控制点（通常 P = 20~40）
- 控制点之间施加两种力：
  - **弹簧力**（spring）：相邻控制点间保持原始边长度，防止过度拉伸
  - **静电力**（electrostatic）：兼容边（方向相近、距离近）的控制点之间相互吸引
- 迭代求解，模拟退火降温，控制点逐步聚拢形成束
- **兼容性判定**（避免不相关边被错误捆绑）：
  - 角度兼容：两边方向夹角 < 阈值
  - 尺度兼容：两边长度比 < 阈值
  - 位置兼容：两边距离 < 阈值
  - 可见性兼容：两边连线不穿过节点

#### MINGLE（Ink 节省优化）——工程化首选

- 目标函数：最小化总 Ink（绘制长度），即最大化共享路径
- 自底向上合并：
  1. 将每条边视为初始 bundle
  2. 反复计算每对 bundle 合并后的 Ink 节省量
  3. 合并节省量最大的一对，直到无正收益
- 合并时通过加权平均重新计算 bundle 的路径
- **优点**：O(E log E) 接近线性、确定性强、Ink 节省量可量化
- **缺点**：贪心可能陷入局部最优；对长距离边的中间段捆绑效果一般

#### HEB（层次边捆绑）——层次结构专用

- 需要额外的层次树（ad hoc tree / hierarchy）作为引导骨架
- 边的两个端点在树中的 LCA（最近公共祖先）路径即为捆绑骨架
- 控制点沿树路径插值扭曲，自然汇聚到树的骨架上
- **优点**：语义明确、结果可解释、计算快
- **缺点**：需要层次结构；对 drawify 的流程图/架构图，层次结构来自 Sugiyama rank 或 group 嵌套

### 3.3 适合 drawify 场景的算法选择

drawify 的场景特点：
1. **正交折线为主**（流程图、架构图使用 orthogonal 路由）
2. **需要确定性**（[AGENTS.md](file:///Users/jimichan/zaprt-projects/flowml/AGENTS.md) 要求不得依赖 HashMap key 排序驱动迭代）
3. **图规模中等**（建议规模 ≤ 320 节点、边数约 1.5×节点数 = ~500 边）
4. **存在层次信息**（Sugiyama rank、group 分组边框）
5. **有明确的通道概念**（正交路由的水平/垂直间隙）

**推荐方案：分层混合策略**

| 层级 | 算法 | 作用 |
|------|------|------|
| 节点端口附近 | 现有 Concentrate trunk+fork（已实现） | 入口/出口汇流 |
| 边对相似度聚类 | 类似 FDEB 兼容性测试（确定性版本） | 识别可捆绑的边组 |
| 路径段合并 | MINGLE 式 Ink 优化的正交化版本 | 共享水平/垂直主干段 |
| 分叉/合入点计算 | 类似 HEB 的骨架路径 | 确定分叉位置 |

---

## 4. 正交边捆绑（Ortho-Bundling）设计方案

由于 drawify 主流是正交折线（水平/垂直段构成），我们不需要 FDEB 那样的曲线控制点物理模拟，而是设计**正交友好的离散路径段合并算法**。

### 4.1 核心思想：通道共享 + 分叉控制

正交边的特征：路径由水平段和垂直段交替组成，折点（turning point）在网格上。

**Bundle 的直观形式**：

```
  无 bundling：                    有 bundling：
  A ──┐            B ──┐          A ──┐
      │                │              │
      └──→ X          └──→ X         ├──→ X  ← 共享主干段（bundle trunk）
  C ──┐            D ──┐              │
      │                │          C ──┘
      └──→ Y          └──→ Y         │
                                     └──→ Y
```

多条从左侧进入、目标在右侧的边，先共享一段水平主干（bundle trunk），再在合适位置分叉到各自目标。

### 4.2 算法流水线

```
Step 1: 路径分段与特征提取
    ↓
Step 2: 边兼容性评估（构建兼容图）
    ↓
Step 3: 边聚类（将兼容边分组为 bundle candidates）
    ↓
Step 4: 通道分配（为 bundles 分配共享主干通道）
    ↓
Step 5: 分叉点计算（确定每条边从主干分叉的位置）
    ↓
Step 6: 路径重写（生成合并后的折线路径）
    ↓
Step 7: 重叠惩罚豁免 + 微调
```

### 4.3 Step 1：路径分段与特征提取

对每条 Polyline 路径，分解为有向段（segments）：

```rust
struct PathSegment {
    edge_index: usize,
    axis: Axis,               // Horizontal / Vertical
    start: (f64, f64),
    end: (f64, f64),
    direction: Direction,     // 沿轴的正/负方向
    length: f64,
    layer: Option<usize>,     // 所属通道层（y 坐标 for H-seg, x for V-seg）
}
```

提取每条边的高层特征：
- 起点端口 / 终点端口
- 主要流向（基于 from/to 节点的相对位置：`→`、`←`、`↓`、`↑`）
- 起点所在区域 / 终点所在区域（group 或 rank 编号）

### 4.4 Step 2：边兼容性评估

参考 FDEB 的兼容性度量，针对正交边调整。

两条边 e1、e2 **兼容**当且仅当：

| 度量 | 条件 | 权重 | 说明 |
|------|------|------|------|
| **L1 语义约束** | `arrow_type` 相同 且 `line_style` 相同 且 流向同向（同为出边或同为入边，按端点 is_from 判断） | 硬条件 | 从 L1 分组传递而来（§2.4），不满足直接 compatibility = 0 |
| **方向兼容** | 端到端方向（from→to 向量）夹角 ≤ 60° | 硬条件 | 正交边端到端方向主要是 0°/90°/180°/270°；60° 阈值意味着 90° 的边对**不兼容**（水平流 vs 垂直流不能捆） |
| **区域兼容** | 起点同 rank 区间 且 终点同 rank 区间 | 硬条件 | 复用 Sugiyama rank 作为层次区域，比纯几何距离更语义化（接近 HEB 思想）；无 rank 信息时退化为"起点同 group 或同侧" |
| **流向兼容** | 同向（非反向） | 硬条件 | 一条 from→to 与一条 to→from 不捆 |
| 尺度兼容 | `min(len1, len2) / max(len1, len2) ≥ 0.3` | 加分项 | 边长度差距过大无意义 |
| 位置兼容 | 两条边的最小距离 ≤ bundle_gap（默认 60px） | 加分项 | 几何邻近度 |
| lane 兼容 | 同 lane 优先 | 加分项 | 见下方"加速策略" |
| **label 兼容** | `SegmentAware`：仅检查独占段几何可行性（§4.10.4）；`Conservative`：双方不同 `label` 禁止合并 | 软/硬条件 | 智能策略下 label 不阻碍捆绑 |

**反例（不兼容）**：
- 一条从左到右、一条从右到左（反向，不能捆在同侧）
- 一条从上层节点到底层节点、一条从下层到上层（方向相反）
- 边长度差距过大（长边与超短边捆绑无意义）
- 一条水平流（→）、一条垂直流（↓）——端到端方向 90°，超过 60° 阈值

兼容性评分公式：
```
compatibility(e1, e2) = w_angle * angle_score
                      + w_region * region_score
                      + w_scale * scale_score
                      + w_distance * distance_score
```
阈值：`compatibility ≥ threshold`（默认 0.5）且所有硬条件满足，才认为可捆绑。

**加速策略（避免 O(E²) 全量比对）**：
1. **按 (from_rank, to_rank, arrow_type, line_style) 分桶**：只有同桶的边对才可能兼容（硬条件预筛），把 O(E²) 降到 O(Σ bucket_size²) ≈ O(E·k)
2. **同 lane 优先**：现有路由已计算 `lane[i]`（[mod.rs#L243](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/edge/edge_routing_orthogonal/mod.rs#L243)），同 lane 的边天然同向同区域，可优先评估；跨 lane 边对仍可兼容（lane 不是硬条件），但需额外几何验证
3. **空间索引**：对每个桶内的边按起点坐标建 grid bucketing，只比对相邻 grid 的边对

### 4.5 Step 3：边聚类

使用**确定性连通分量聚类**（不用 HashMap 迭代顺序）：

1. 将所有边作为图节点
2. 兼容边对之间连边（权重 = compatibility 分数）
3. 按权重从高到低处理边对，用并查集（Union-Find）合并连通分量
4. 聚类大小 ≥ 2 且 ≤ `max_bundle_size`（默认 8）时形成 bundle candidate
5. 超过 8 条边的 bundle 按目标区域子分（避免过粗的束）

聚类结果：每个 bundle 包含一组边，有唯一确定的「代表路径」（组内最中心的那条边的路径）。

### 4.6 Step 4：通道分配（Trunk 定位）

对每个 bundle，确定其共享主干通道：

1. **主轴选择**：
   - 如果 bundle 内多数边为水平方向为主（长水平段）→ 主干为水平通道
   - 如果多数边为垂直方向为主 → 主干为垂直通道
   - 混合方向：选起点到终点的主方向（from 集群 → to 集群）

2. **主干坐标**：
   - 取组内所有边对应方向段的坐标中位数（水平主干取 y 中位数，垂直取 x 中位数）
   - 量化到网格（8px 网格，与现有 grid_snap 一致）
   - 不同 bundles 的主干之间保持 `bundle_gap`（默认 12px = 3px × 4 条平行线间距）

3. **主干范围**：
   - 起点：所有边在主轴方向投影的 min 坐标（外扩 `trunk_margin` = 24px）
   - 终点：所有边在主轴方向投影的 max 坐标（外扩 `trunk_margin`）

### 4.7 Step 5：分叉点计算

对 bundle 内每条边，计算其在主干上的「合入点」（entry）和「分叉点」（exit）：

```
              entry_i       exit_i
                ↓             ↓
  ──────────────┬─────────────┬──────────  主干 trunk
   from_i ──────┘             └──────→ to_i
```

- **合入点 entry_i**：边的 from 端在主干上的最近投影点，向 from 方向外扩 `fork_distance`（默认 16px，见 §7.2）
- **分叉点 exit_i**：边的 to 端在主干上的最近投影点，向 to 方向外扩 `fork_distance`

分叉点排序：
- 合入点按边的 from 节点在主轴垂直方向的坐标排序（确定性排序）
- 分叉点按边的 to 节点坐标排序
- 确保分叉点不重叠，最小间距 `fork_spacing`（默认 8px，见 §7.2；与 `bundle_gap`=12px 区分：前者是同 bundle 内相邻分叉点间距，后者是不同 bundle 主干间距），不足时均匀错开

### 4.8 Step 6：路径重写

将每条边的路径重写为：

```
from_anchor
    → stub 段（端口向外 16px，现有逻辑）
    → 合入段（从 stub 终点到 entry_i，1~2 个折点）
    → 主干共享段（entry_i → exit_i，与其他边完全重合）
    → 分叉段（从 exit_i 到 to 端 stub 起点，1~2 个折点）
    → stub 段（到 to_anchor）
```

合并后路径性质：
- 保持正交（所有段为水平或垂直）
- 端点（from_anchor、to_anchor）不变，端口不变
- 主干段坐标完全相同，视觉上合并为一条粗线（可通过线宽 + 透明度叠加产生束的视觉效果）

### 4.9 Step 7：后处理

1. **重叠白名单**：`EDGE_OVERLAP_PENALTY`（[scoring.rs#L234](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/edge/edge_routing_orthogonal/scoring.rs#L234)）在**路由阶段**计算，bundling 是后处理不会再调用 scoring，故无需"豁免"。真正需要确认的是：**是否存在后置 overlap 检测**会惩罚 bundling 产生的故意重叠。若有，需为 bundle 主干段加白名单（按 edge_index 对标记）；若无，本项无操作。

2. **节点避让**：检查主干段是否穿过节点，若穿过则推移主干（或放弃该 bundle 回退原路径）。注意：refine 已在 bundling 之前跑过，bundling 重写的路径若穿障**无法再调用 refine**，必须自行检测并回退。

3. **标签后置**：不在此步处理；完整流程见 §4.10（bundling 完成后由 layout 管线统一调用）。

4. **空 bundle 回退**：若 bundle 合并后总 Ink 节省 < `min_ink_saving`（默认 10%），回退到原路径。**Ink 节省基准** = L3 关闭后的纯单边路径长度（见 §2.5 契约），保证基准干净。

5. **自环/短边过滤**：自环边（from == to）和超短边（路径长度 < 3 × `fork_distance`）直接跳过 bundling，不进入聚类。

### 4.10 边标签后置布局

> **背景**：当前 orthogonal 路由器在 `route()` 内部完成标签初定位与 `resolve_label_overlaps`（[mod.rs#L479-L528](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/edge/edge_routing_orthogonal/mod.rs#L479-L528)）。bundling 重写 `PathGeometry` 后，若沿用旧 `labels`，会出现**悬空**（label 不在新路径上）或**堆叠**（bundle 内多条边的中段 label 落在同一几何位置）。本节定义 bundling 启用时的 label 契约与算法。

#### 4.10.1 管线变更：路由「占位」+ bundling 后「重算」

启用 bundling 时，orthogonal 路由对 label 的处理改为两阶段：

| 阶段 | 时机 | 行为 |
|------|------|------|
| **路由占位** | `router.route()` 内 | 仍调用 `build_edge_labels` 填充 `edge.labels`（供路由期障碍估算等），但**不**调用 `resolve_label_overlaps`；或仅做轻量占位（`labels` 文本保留，`center` 可置零） |
| **后置重算** | `EdgeBundler::apply()` 之后 | 调用 `relayout_edge_labels_after_bundling()`（§4.10.2），基于**新 path** 重算全部 label |

未启用 bundling 时，行为与现有一致（路由内完成 label 放置 + 避障）。

**不变量**：后置 label 流程**只修改** `EdgeLayout.labels`（含 `center` / `rotation` / `leader_to`），**不修改** `PathGeometry` 与端口。

#### 4.10.2 后置 label 流水线

```
EdgeBundler::apply()  →  BundlingResult（含 bundles + trunk 避让区）
        ↓
Step A: 清除旧 label 几何（保留 text，重置 center/leader_to/rotation）
        ↓
Step B: 按 label 类型 + 策略初定位（§4.10.3 / §4.10.4 SegmentAware）
        ↓
Step C: resolve_label_overlaps（复用现有 label_avoidance）
        ↓
Step D: assign_leader_lines（复用现有逻辑，基于新 path）
        ↓
Step E: bundle 主干禁放区二次校验（label 不得压在 trunk 带内，见 §4.10.5）
```

**Step B 细节**——依 `label_bundle_policy` 分支（§4.10.3）：

- **`SegmentAware`（默认，§4.10.4）**：中段 `label` 锚定在**该边独占段**（合入段/分叉段/stub），不占用共享主干；`head_label`/`tail_label` 同理优先近端独占段。
- **`Stagger`**：中段 `label` 在共享主干上错开 `t`（§4.10.3 策略二）。
- **`Conservative` / 未进 bundle**：沿全路径 `parse_label_t` + `build_edge_labels`，与现网一致。

```rust
// 伪代码：edge_bundling/label_placement.rs
match config.label_bundle_policy {
    LabelBundlePolicy::SegmentAware => {
        place_labels_on_exclusive_segments(edge_i, rel, &path, &roles, &bundling);
    }
    LabelBundlePolicy::Stagger => {
        let middle_t = adjust_middle_t_for_bundle(i, parse_label_t(rel), &bundling);
        build_edge_labels(rel, middle_t, (0.0, 0.0), |t| point_at_path_t(&path, t))
    }
    _ => build_edge_labels(rel, parse_label_t(rel), (0.0, 0.0), |t| point_at_path_t(&path, t)),
}
```

- **`rotation`**：在选定独占段上用 `tangent_angle_at_t` 重算（段内局部 t，非全路径 t）。

**Step C/D**：直接复用 [label_avoidance.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/edge/common/label_avoidance.rs) 的 `resolve_label_overlaps` 与 `assign_leader_lines`，无需 fork 新避障算法。

#### 4.10.3 label 策略总览

| 策略 | 捆绑前门控 | 中段 `label` 锚点 | 适用 |
|------|-----------|------------------|------|
| **`SegmentAware`（默认）** | 不因 label 文本禁止合并；仅当**所有独占段均放不下 label** 时拒绝并入 | 合入段 / 分叉段 / stub 等**非共享段**（§4.10.4） | 推荐：可捆又可读 |
| `Conservative` | 双方有不同 `label` → 不捆绑 | 全路径 t（与现网同） | 最保守、零行为变化 |
| `Stagger` | 不禁止合并 | 共享主干上错开 t | 独占段极短时的兜底 |
| `ForkOnly` | 有 `label` 的边永不进 bundle | 全路径 t | 调试 |

配置项 `label_bundle_policy`，默认 **`SegmentAware`**。

**策略一：捆绑前门控（`Conservative` / `Stagger` 时生效）**

在 Step 3 聚类合并边对 `(e1, e2)` 前，检查中段 label 冲突：

| 条件 | 动作 |
|------|------|
| 仅一方有 `label` | 允许合并 |
| 双方都有 `label` 且文本相同 | 允许合并 |
| 双方都有 `label` 且文本不同 | **禁止合并**（compatibility 置 0） |
| bundle 内已有 label 边数 ≥ `max_labeled_edges_per_bundle`（默认 2） | 拒绝继续并入带 `label` 的边 |

> `SegmentAware` **不走上表**：不同文本的边允许同束，label 靠独占段自然分离（§4.10.4）。

**策略二：bundle 内主干 t 错开（`label_bundle_policy = Stagger` 时）**

对 bundle `B` 内带 `label` 的边，按 `edge_index` 升序分配中段 t：

```
t_i = trunk_t_center + (rank_i - (n-1)/2) * label_t_spacing
```

- `trunk_t_center`：共享主干在整条 path 上的参数中点（由 `entry_i`/`exit_i` 投影到 path 算得）
- `rank_i`：边在 bundle 内按 `edge_index` 的序号 0..n-1
- `label_t_spacing`：默认 `0.08`，钳制在 `[0.15, 0.85]` 内
- 若错开后仍超出可用区间 → 回退到 §4.10.4 独占段定位，或束外侧 + leader

确定性：排序键 = `(bundle_id, edge_index)`。

#### 4.10.4 独占段感知定位（SegmentAware）

bundling 后每条边的路径可分解为五类区段（与 §4.8 重写形态一一对应）：

```
from_anchor ──[FromStub]──┬──[MergeLeg]──┬──[Trunk 共享]──┬──[ForkLeg]──┬──[ToStub]── to_anchor
                          entry_i                      exit_i
```

| 区段 | 是否独占 | label 关联语义 |
|------|---------|---------------|
| `FromStub` / `ToStub` | 是（每边独立） | 近端口短腿；适合 `tail_label` / `head_label` |
| `MergeLeg` | 是 | 近源侧；适合 `label_position: start` 或流向起点语义 |
| `ForkLeg` | 是 | 近目标侧；适合 `label_position: end` 或流向终点语义 |
| `Trunk` | **否**（bundle 内重合） | **默认禁止**放任何 label；仅 `Stagger` 或降级时可用 |

**数据结构**（`path_rewrite` 输出，写入 `BundlingResult`）：

```rust
/// 边路径上的半开区间 [t_start, t_end) 及折线点索引
pub struct SegmentSpan {
    pub role: SegmentRole,
    pub point_start: usize,
    pub point_end: usize,
    pub t_start: f64,
    pub t_end: f64,
    pub length: f64,
}

pub enum SegmentRole {
    FromStub, MergeLeg, Trunk, ForkLeg, ToStub,
}

/// 每条边（含未捆绑边：仅 Trunk 为空或 Trunk=全路径）
pub struct EdgePathRoles {
    pub edge_index: usize,
    pub spans: Vec<SegmentSpan>,   // 按 path 顺序，确定性排序
}
```

**Step B′：为每个 label 选独占段并求段内锚点**

对边 `e` 的每个 label（`label` / `head_label` / `tail_label`）：

1. **候选集**：`spans` 中 `role != Trunk` 且 `length ≥ min_label_segment_len`（默认 `label_metrics(text).0 + 8`，或配置 `min_exclusive_segment_for_label`）的段。
2. **DSL 偏好**（与 `label_position` 对齐）：

| 标签字段 | `label_position` 暗示 | 优先段（降序） |
|---------|---------------------|---------------|
| `tail_label` | start | `FromStub` → `MergeLeg` |
| `label` | start / `t≤0.35` | `MergeLeg` → `FromStub` |
| `label` | end / `t≥0.65` | `ForkLeg` → `ToStub` |
| `label` | middle / 默认 | 自动评分（见下） |
| `head_label` | end | `ToStub` → `ForkLeg` |

3. **中段 `label` 自动评分**（`middle` 且未指定明确 start/end 时）：

```
score(seg) = w_len * (seg.length / path_total_len)
           + w_pref * dsl_alignment(seg.role)
           + w_side * bundle_side_bonus(seg, edge_index_in_bundle)
           - w_occ * occupied_by_sibling_label(seg)
```

- `bundle_side_bonus`：bundle 内按 `edge_index` 奇偶交替偏好 `MergeLeg` / `ForkLeg`，使相邻边的 label 自然分列主干两侧，降低共线重叠。
- `occupied_by_sibling_label`：同 bundle 内已有 label 占用的段（同 `role` 且 t 区间 IoU > 0）扣分。
- 权重默认：`w_len=0.4, w_pref=0.35, w_side=0.25`；确定性：同分取 `role` 枚举序，再取 `edge_index` 小者。

4. **段内锚点**：在选中段的 `[t_start, t_end]` 上取 `t_seg = 0.5`（或 DSL 映射到段内相对位置），调用 `point_at_path_t(path, t_global)` 得 `center`；`rotation` 用该 `t_global` 处切线。

5. **bundle 级分配（可选增强）**：对同 bundle 内多条带 `label` 的边，在各自 top-2 候选段上做一次**确定性贪心匹配**——按 `edge_index` 升序依次为每条边选分最高且与同束已选段空间不冲突的段；冲突判定用 label bbox 投影到段轴上的 1D 区间是否重叠。

**捆绑前门控（SegmentAware 专用）**

合并边对前仅检查**几何可行性**，不比较 label 文本：

| 条件 | 动作 |
|------|------|
| 双方均无 `label` | 允许 |
| 至少一方有 `label`，且双方合并后每条有 label 的边存在 `length ≥ min_label_segment_len` 的独占段 | 允许 |
| 某条边的 `MergeLeg`+`ForkLeg` 均短于阈值（极端短束） | 拒绝该边对合并，或整束回退 `Stagger` |

**回退链**（确定性、逐级）：

```
SegmentAware 找不到合法独占段
    → 同边尝试另一独占段角色
    → bundle 内改 Stagger（主干错开 t）
    → 束外侧偏移 + leader_to（Step E）
    → 标记 label_conflict（debug hints），不回退 path
```

**示例**（`SegmentAware`，3 条不同 label 同束）：

```
  A「请求」──┐
  B「响应」──┼──════ 共享主干 ═══──→ X
  C「超时」──┘
     ↑            ↑
  label 在       label 在
  MergeLeg       ForkLeg
  （源侧独占）    （目标侧独占）
```

三条边可**同时捆绑**；label 落在各自未合并的腿上，无需主干错开，也不触发 Conservative 的「不同文本禁止合并」。

#### 4.10.5 主干禁放区（Trunk Keep-out Zone）

`BundlingResult` 为每个 bundle 输出禁放矩形/折线段集合，供 Step E 查询：

```rust
/// 主干段外扩 label_trunk_pad（默认 8px）的避让带
pub struct TrunkKeepout {
    pub bundle_id: usize,
    /// 外扩后的轴对齐条带（水平主干 → 薄矩形；垂直主干 → 薄矩形）
    pub zones: Vec<(f64, f64, f64, f64)>,
}
```

**Step E 规则**：

1. 中段 `label` 的 bbox 与任一 `TrunkKeepout` 相交 → 沿主干法向推开至条带外（优先上/右，与现有 AABB 推开一致）
2. 推开后距 path 最近点 > `DEFAULT_LEADER_LINE_THRESHOLD` → 设置 `leader_to`（与现有引线语义一致）
3. `head_label` / `tail_label` 默认允许压在 stub 上，**不**受 trunk keepout 约束（它们本就在分叉附近）

此区域同时供渲染层可选查询（避免装饰元素压在束上），与 §2.5 原「标签避让区」合一。

#### 4.10.6 与 Ink 回退的交互

§4.9 的 `min_ink_saving` 回退**仅比较路径长度**，不把 label 位移计入。但若 bundle 回退原因是「主干穿障」，对应边的 label 无需特殊处理（路径恢复后后置 label 自然重算）。

若 Step E 后某 bundle 内仍有多 label 严重重叠（迭代耗尽），**不回退 bundling 路径**，而是对该 bundle 标记 `label_conflict: true` 写入 debug hints，并按 §4.10.4 回退链降级（`Stagger` → leader → 仅标记）。

#### 4.10.7 模块与集成

新增 `edge_bundling/label_placement.rs`，导出：

```rust
pub fn relayout_edge_labels_after_bundling(
    diagram: &Diagram,
    edges: &mut [EdgeLayout],
    bundling: &BundlingResult,
    config: &BundlingConfig,
);
```

layout 管线（§7.3）在 bundling 之后调用；`finalize_canvas_bounds` 之前完成。

`place_labels_on_exclusive_segments` 与 `EdgePathRoles` 分解逻辑放在 `label_placement.rs`；`SegmentSpan` 在 `path_rewrite` 完成时一并写入 `BundlingResult::edge_roles: Vec<EdgePathRoles>`。

#### 4.10.8 示例

```
无 bundling：              SegmentAware（3 条不同 label，可同束）：

  A ──「请求」──→ X       A ──「请求」──┐
  B ──「响应」──→ X       B ──「响应」──┼──═ 主干 ═──→ X
  C ──「超时」──→ X       C ──「超时」──┘
                           ↑ 源侧 MergeLeg   ↑ 源侧 MergeLeg … 目标侧 ForkLeg
```

`Conservative` 下 A/B/C **不会**同束；`SegmentAware` 下同束且 label 各据独占段。

`Stagger`（兜底）：中段 label 仍在主干上错开 t，用于独占段过短时。

---

## 5. 曲线边的 Bundling（Bezier/Spline）——远期可选

> **本节为远期方案，不在 P0~P5 核心范围内**。第一期仅实现 orthogonal bundling（见 §9 阶段规划 P7 可选）。§7.3 集成代码的 `router.name() == "orthogonal"` 条件判断即为此约束的体现。

对于 bezier、spline 等曲边路由，未来可采用**简化版 FDEB**：

1. 每条贝塞尔曲线采样为 P = 16 个控制点
2. 兼容性评估同 Step 2（角度、位置、尺度）
3. 迭代 6~10 轮：
   - 每个控制点受到其 K 个最近兼容控制点的吸引（K=3）
   - 弹簧力保持边的原始长度
   - 端点固定不动
4. 用变形后的控制点重新拟合贝塞尔/多段样条

考虑到 drawify 主要使用 orthogonal，曲线 bundling 列为 P7 可选优先级。

---

## 6. 渲染层效果增强

Bundling 不仅改变路径几何，还可以通过渲染增强束的视觉效果：

| 技术 | 效果 |
|------|------|
| **线宽累加** | bundle 处的线宽 = 基线宽 × √(n_edges)，高密度束更粗 |
| **透明度叠加** | 边线使用低 alpha（如 0.3），叠加后高密度束颜色更深 |
| **颜色渐变** | 沿束方向渐变，指示流向（类似 G6 航线图例子） |
| **束间隙** | 同一 bundle 内的平行线留 2~3px 间隙而非完全重叠，仍可区分各条边（可选） |
| **高亮联动** | hover 一条边时，同 bundle 的其他边一起高亮 |

线宽和透明度的处理可以在 SVG 渲染阶段基于 bundle 元数据动态计算，不需要修改几何数据。

---

## 7. 模块设计

### 7.1 新增模块结构

```
crates/drawify-core/src/layout/edge/
├── common/
│   └── ...
├── edge_bundling/           ← 新增模块
│   ├── mod.rs
│   ├── compatibility.rs     # Step 2: 兼容性评估（含 label 门控）
│   ├── clustering.rs        # Step 3: 确定性聚类
│   ├── trunk.rs             # Step 4-5: 主干通道与分叉点
│   ├── path_rewrite.rs      # Step 6: 路径重写 + EdgePathRoles 分解
│   ├── label_placement.rs   # §4.10: bundling 后置 label（含 SegmentAware）
│   └── types.rs             # Bundle / SegmentSpan / TrunkKeepout
├── edge_routing.rs
├── edge_routing_bezier.rs
├── edge_routing_orthogonal/
└── ...
```

### 7.2 核心数据结构

> **确定性约定**：按 [AGENTS.md](file:///Users/jimichan/zaprt-projects/flowml/AGENTS.md) 要求，所有映射使用 `Vec` 按 `edge_index` 排序存储，**不使用 `HashMap`**，避免任何迭代顺序依赖。

```rust
/// 边捆绑配置
pub struct BundlingConfig {
    /// 是否启用边捆绑（默认 true for orthogonal）
    pub enabled: bool,
    /// 兼容性阈值（0.0~1.0，越高捆绑越保守）
    pub compatibility_threshold: f64,
    /// 同束最大边数（默认 8，超过子分）
    /// 依据：线宽累加 √(n) 后，8 条边的束宽度 = 基线宽 × 2.83，
    ///       视觉上仍可辨识为"束"而非"色块"；超过 8 条按目标区域子分
    pub max_bundle_size: usize,
    /// 束间最小间距（像素，默认 12px）——不同 bundle 的主干之间
    pub bundle_gap: f64,
    /// 分叉点距节点的最小距离（像素，默认 16px）
    pub fork_distance: f64,
    /// 分叉点之间的最小间距（像素，默认 8px）——同 bundle 内相邻分叉点
    pub fork_spacing: f64,
    /// 最小 Ink 节省比例，低于则不合并（默认 0.1）
    pub min_ink_saving: f64,
    /// bundle 内最多允许多少条带中段 label 的边（默认 2，见 §4.10.3）
    pub max_labeled_edges_per_bundle: usize,
    /// 主干禁放区外扩（像素，默认 8）
    pub label_trunk_pad: f64,
    /// bundle 内中段 label 的 t 间距（默认 0.08，仅 Stagger 策略）
    pub label_t_spacing: f64,
    /// 独占段最小长度（像素）；低于此值的段不可锚 label（默认由 label_metrics 推导）
    pub min_exclusive_segment_for_label: f64,
    /// label 与 bundling 的协同策略（默认 SegmentAware，见 §4.10.3–4）
    pub label_bundle_policy: LabelBundlePolicy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelBundlePolicy {
    /// 中段 label 锚在合入/分叉等独占段（默认，§4.10.4）
    SegmentAware,
    /// 双方有不同 label 时不捆绑
    Conservative,
    /// 允许捆绑，主干上错开 t
    Stagger,
    /// 有 label 的边永不进 bundle
    ForkOnly,
}

/// 一个 bundle（一束边）
pub struct EdgeBundle {
    /// 束 ID（确定性，按组内最小 edge_index 编号）
    pub id: usize,
    /// 包含的边索引列表（已排序，升序）
    pub edges: Vec<usize>,
    /// 主干段（主轴方向）
    pub trunk_axis: Axis,
    pub trunk_start: (f64, f64),
    pub trunk_end: (f64, f64),
    /// 每条边的合入/分叉点（按 edge_index 升序排序，与 edges 对齐）
    pub entry_points: Vec<(f64, f64)>,
    pub exit_points: Vec<(f64, f64)>,
}

/// Bundling 结果（附加在 LayoutHints 中供渲染使用）
pub struct BundlingResult {
    pub bundles: Vec<EdgeBundle>,
    /// edge_index → 所属 bundle_id（None 表示未捆绑）
    /// 按 edge_index 索引（vec[edge_index] = Option<bundle_id>），无需 HashMap
    pub edge_to_bundle: Vec<Option<usize>>,
    /// 总 Ink 节省量（像素）
    pub total_ink_saved: f64,
    /// 每条边的路径区段分解（§4.10.4 SegmentAware）
    pub edge_roles: Vec<EdgePathRoles>,
    /// 主干禁放区，供 §4.10.5 后置 label 与渲染查询
    pub trunk_keepouts: Vec<TrunkKeepout>,
}
```

**主干段渲染语义**：bundle 内多条边的主干段**几何坐标完全相同**（重合），渲染时采用**重叠绘制 + alpha/线宽叠加**（见 §6），而非几何合并（dedup）。理由：
1. 实现简单：每条边仍绘制完整路径，无需特殊处理主干段
2. 支持 hover 联动：每条边是独立 SVG 元素，可单独高亮
3. 视觉效果：低 alpha（0.3）叠加后高密度束颜色更深，线宽 × √(n) 体现束粗细

### 7.3 与现有管线集成

按 §2.2 和 §2.5 契约，bundling 插入在 `repulse_edges_from_group_borders` 之后、`finalize_canvas_bounds` 之前（[mod.rs#L1222](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/mod.rs#L1222) 之后）：

```rust
// ……V2 择优后的最终 result（post_route_select 已执行）……

if grid_snap::should_snap(algo) {
    // grid_snap + repulse_edges_from_group_borders（现有逻辑，L1203-L1222）
    // ...
}

// ── 新增：边捆绑后处理（最后一步 path 几何后处理）──
// 必须在 V2 择优 + refine + grid_snap + group repulse 之后执行
if plan.edge_bundling.enabled && router.name() == "orthogonal" {
    let bundler = EdgeBundler::new(plan.edge_bundling);
    result = bundler.apply(diagram, result);
    // bundling 之后禁止修改 PathGeometry（§2.5 契约）

    // ── 后置 label 重算（§4.10）──
    if let Some(ref bundling) = result.hints.bundling {
        relayout_edge_labels_after_bundling(
            diagram,
            &mut result.edges,
            bundling,
            &plan.edge_bundling,
        );
    }
}

// finalize_canvas_bounds（现有逻辑）
```

配置通过 `edge_routing: orthogonal { bundling: true }` DSL 扩展。

**L3 feature flag**：P0 起通过 `orthogonal.concentrate_trunk_fork: false`（默认 false）关闭 L3，避免与新 bundling 双重叠加（见 §2.5 契约）。

---

## 8. 确定性保证

按照 [AGENTS.md](file:///Users/jimichan/zaprt-projects/flowml/AGENTS.md) 的要求：

1. **不依赖 HashMap 迭代顺序**：
   - 聚类前先按 edge_index 排序边
   - 并查集合并按权重降序、权重相同按最小 edge_index 升序
   - 分叉点按坐标排序而非插入顺序遍历

2. **所有数值计算确定性**：
   - 不使用随机数（FDEB 原算法的随机初始化需要替换为确定性初始化：用中位数/平均值）
   - 通道坐标量化到 8px 网格

3. **排序稳定性**：
   - 所有 sort 使用 `sort_by` + 全序 key（含 tiebreaker）
   - tiebreaker 使用 edge_index 保证稳定

---

## 9. 实现阶段规划

| 阶段 | 内容 | 预估工作量 |
|------|------|-----------|
| **P0** | 数据结构 + compatibility 评分（含 L1 语义约束/rank/lane 硬条件 + **label 门控占位**）+ 单元测试；**新增 L3 feature flag `concentrate_trunk_fork: false` 默认关闭**（见 §2.5 契约） | 1~2 天 |
| **P1** | 确定性聚类 + 主干通道分配 + 简单分叉（**水平 + 垂直主干**，architecture 类图多为垂直层间流向，需同步支持） | 2~3 天 |
| **P2** | 路径重写 + 节点避让 + Ink 节省回退 + **`EdgePathRoles` / `TrunkKeepout` 输出** | 2~3 天 |
| **P3** | 正交全方向支持（混合方向 + L 形/Z 形主干）+ 后处理 | 2 天 |
| **P4** | DSL 配置项 + 集成到流水线（§7.3）+ **后置 label 流水线**（§4.10，含 **SegmentAware**）+ orthogonal 路由内 label 避障改为 bundling 开关控制 + 端到端测试（bundling 默认关闭）+ **性能指标验证**（见 §10.1） | 1.5~2.5 天 |
| **P5** | **删除 L3 死代码**：移除 `fork_points` / `concentrate_from` / `concentrate_to` 及相关豁免逻辑；Concentrate 模式简化为「锚点合并到一点」 | 0.5~1 天 |
| **P6**（可选） | 渲染增强（线宽/透明度累加）| 1~2 天 |
| **P7**（可选） | Bezier/Spline 曲线 bundling（见 §5 远期方案） | 3~5 天 |
| **P8**（可选，远期） | 进一步收掉 L2 Concentrate：所有边用 Single 模式，入口汇聚完全交给 Bundling | 1~2 天 |

总计约 10~16 个工作日完成核心功能（P0~P5）。

> **关于 L3 关闭时机**：P0 起即通过 feature flag 默认关闭 L3（不是 P5 才关），让 bundling 在干净基准上开发和验证。P5 仅是删除已被 flag 关闭的死代码，无行为变化。这样避免了 P0~P4 期间 L3 与新 bundling "双重 bundling" 的干扰（见 §2.5 契约）。
>
> **关于垂直主干提前**：原方案 P1 只做水平主干、P3 才做垂直。但 §10.2 的 architecture 类图（多层架构）多为垂直层间流向，P1 完成时无法验证这类高价值场景。故 P1 同步支持水平 + 垂直，P3 只处理混合方向。

---

## 10. 验证与测试方案

### 10.1 量化指标

- **Ink 节省率**：`(原始总路径长度 - 捆绑后总路径长度) / 原始总长度`
  - 目标：典型架构图 ≥ 15%，密集流程图 ≥ 25%
- **交叉数变化**：统计边对交叉点数
  - 目标：交叉数减少 ≥ 20%
- **视觉捆绑度**：共享段长度占总路径长度比例
  - 目标：≥ 10%
- **bundling 阶段耗时**：单独测量 `EdgeBundler::apply` 耗时
  - 目标：≤ 路由总耗时（route + refine + grid_snap）的 20%
  - E=500 边时绝对值目标：< 50ms（避免交互渲染卡顿）

### 10.2 测试用例

单元测试：
- compatibility 评分：同向边/反向边/短边长边各种组合
- **label 门控**：Conservative 下双方不同 `label` → compatibility=0；SegmentAware 下仅短独占段拒绝
- **SegmentAware**：`label.center` 落在 `role != Trunk` 的段上；`t` 在该段 `[t_start,t_end]` 内
- **bundle 侧交替**：3 边同束时 MergeLeg/ForkLeg 分配不撞车
- **t 错开**（Stagger 兜底）：3 条同 bundle 边的 `middle_t` 单调
- 聚类：3~8 条边的简单场景，验证 bundle 正确形成
- 分叉点排序：确保分叉点不重叠、顺序正确
- 路径重写：重写后路径端点不变、无穿障
- **后置 label**：bundling 后 `label.center` 落在 new path 上（距离 < 2px）；`leader_to` 指向新 path

集成测试（基于现有 benchmark 图集）：
- bipartite-5x5（二分图，天然适合水平捆绑）
- architecture 类图（多层架构，有主干通道）
- dense-n20（密集图，验证不引入额外交叉）
- wide-L4P6（宽图，验证长水平主干）
- **labeled-bundle-3**（新建：3 条同向边各带不同 label；SegmentAware 同束 + 独占段锚定；Conservative 对照不同束）

对比测试：
- bundling on/off 输出 SVG 对比
- 交叉数、Ink 量自动测量

### 10.3 渐进式发布策略

1. 初始版本：配置默认 `bundling: false`，通过 DSL 手动开启测试
2. 收集 benchmark 结果，调整默认参数（compatibility_threshold、bundle_gap 等）
3. 在 architecture 图类型默认开启（该类型最受益）
4. 扩大到 flowchart，观察无明显副作用后默认开启

---

## 11. 风险与应对

| 风险 | 影响 | 应对 |
|------|------|------|
| 捆绑后路径穿过节点 | 视觉错误 | 后处理穿障检测，不通过则回退该 bundle（注意：refine 已在 bundling 前跑过，bundling 穿障无法再调用 refine，见 §4.9） |
| 过度捆绑导致路径丢失个体特征 | 无法追踪单条边 | `max_bundle_size=8` 限制（依据见 §7.2）；保留分叉段间隙；hover 高亮联动 |
| 增加 bundling 阶段耗时 | 大图性能下降 | 兼容性评估经分桶加速后从 O(E²) 降到 O(E·k)（见 §4.4 加速策略）；只对 degree ≥ 2 节点的边做聚类；性能目标见 §10.1 |
| 标签位置错乱 | 标签重叠、悬空或压在束上 | §4.10：**SegmentAware** 默认把 label 锚在 MergeLeg/ForkLeg/stub；`TrunkKeepout` 禁主干；避障耗尽时 Stagger → leader 回退链 |
| 与 refine/grid_snap 冲突 | 坐标不再对齐 | **已通过插入点选择解决**：bundling 在 grid_snap + group repulse 之后执行（见 §2.2、§7.3），是最后一步几何后处理，输出后无几何修改 |
| V2 择优指标翻转 | 选错基线 | bundling 在 `post_route_select` 择优之后执行，只对最终结果做一次（见 §2.5 契约） |
| L3 与新 bundling 双重叠加 | 效果评估失真 | P0 起 feature flag 默认关闭 L3（见 §2.5 契约、§9 阶段规划） |

---

## 12. 参考资料

1. Holten, D. (2006). Hierarchical Edge Bundles: Visualization of Adjacency Relations in Hierarchical Data. *EuroVis*.
2. Holten, D., & van Wijk, J. J. (2009). Force-Directed Edge Bundling for Graph Visualization. *Computer Graphics Forum*, 28(3).
3. Gansner, E. R., Koren, Y., & North, S. (2011). Topological Fisheye Views for Visualizing Large Graphs.
4. Ersoy, O., Hurter, C., Paulovich, F., Cantareiro, G., & Telea, A. (2011). Skeleton-Based Edge Bundling for Graph Visualization. *IEEE TVCG*.
5. AntV G6 Edge Bundling 插件实现：<https://g6.antv.antgroup.com/>
6. 现有 orthogonal 路由中的 Concentrate 模式：[edge_routing_orthogonal/mod.rs#L61-L65](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/edge/edge_routing_orthogonal/mod.rs#L61-L65)
