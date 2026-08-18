# 20 · 图种 profile 与参数映射

> 核心矛盾：**不同图种确实需要不同的布局行为**，但引擎里不能出现 `if diagram_type ==`（[ADR-001](../../design/adr/001-diagram-type-not-in-engine.md)）。
> 这一矛盾唯一的解法是：**把"图种差异"完整地表达为参数**，并让参数域设计得足够表达力，
> 使得任何图种差异都能落在参数上，而不需要新分支。
>
> 本篇给出：参数域的设计方法、覆盖层级、回写协议、参数域够不够的判定方法，以及各图种 profile 的具体取值。
> yFiles 的产品结构（`HierarchicLayout` + `LayoutData` + 各种 `*Descriptor`）本质就是一个精心设计的参数域 —— 值得逐项对照。

---

## 1. 为什么"参数化"是唯一正解

历史教训（V1 的六条平行管线）与业界共识一致：

| 做法 | 复杂度 | 新图种成本 | 跨图种复用 |
|---|---|---|---|
| 每图种一条管线 | $O(\text{图种} \times \text{特性})$ | 一整条管线 | 无 |
| 引擎内 hint 分支 | 同上，只是分支位置换了 | 每处特性加一个分支 | 名义上有 |
| **参数化 profile** | $O(\text{图种} + \text{特性})$ | 一张参数表 | 完全 |

**判断句**：如果加一个图种需要改引擎代码，说明参数域不够；正确反应是**扩展参数域**（一个新参数服务所有图种），
而不是加一个分支（一个分支只服务一个图种）。

---

## 2. 参数域的三条设计准则

### 准则 1：参数必须是"机制维度"，不是"图种名的别名"
```
❌ layout.style: "er_style"          // 这是图种名换了个马甲
✅ layout.node_order_source: Declaration | Barycenter | Sketch
✅ layout.self_loop_placement: Side(Right) | Corner | Inline
```
检验方法：**这个参数的每个取值，是否至少有两个图种可能用到？** 若某取值只服务一个图种，它大概率是伪参数。
（允许例外：确实只有一个图种需要该取值，但机制本身是通用的 —— 如 `sequence` 的连续时间主轴。）

### 准则 2：参数必须落在明确的自由度上
每个参数要能回答"它影响哪个自由度的哪个写者"。参数表应按**相**组织，不按图种组织：

| 相 | 参数示例 |
|---|---|
| measure | `min_node_size`、`padding`、`label_wrap_width`、`font` |
| P1 去环 | `cycle_removal`（GreedyFAS / DFS / None）、`preferred_direction` |
| P2 分层 | `layering`（NetworkSimplex / LongestPath / CoffmanGraham / FromSketch）、`min_layer_span`、`max_layer_width` |
| P3 定序 | `order_source`（Declaration / Barycenter / Median / Sketch）、`sweeps`、`use_sifting`、`edge_weight_{real,dummy,label}` |
| P4 坐标 | `coord`（BrandesKopf / VPSC / Priority）、`node_gap`、`layer_gap`、`align_priority` |
| 端口 | `default_port_policy`、`port_side_policy`、`port_distribution` |
| 路由 | `router`（Orthogonal / Polyline / Straight / Bus）、`bend_penalty`、`corner_radius`、`min_segment`、`nudge_gap` |
| 标签 | `edge_label_placement`、`node_label_position`、`reserve_space` |
| 组 | `group_padding`、`group_title_height`、`group_order_policy` |
| 全局 | `orientation`（TB/BT/LR/RL）、`component_arrangement`、`budget/*`（降级阈值） |

### 准则 3：参数要有正交性
两个参数不应互相隐含。例如：
```
❌ router: "orthogonal_with_bundling"      // 混了两个维度
✅ router: Orthogonal + bundling: Ordered
```
正交化的收益：$n$ 个维度各 $k$ 个取值 → $k^n$ 种组合免费获得，而代码只有 $nk$ 处。

---

## 3. 覆盖层级（四层，从弱到强）

```
1. 引擎默认值（algorithm defaults）        —— 每个算法自己的合理默认
2. profile 预设（解析层展开）              —— flowchart/sequence/er/... 的推荐组合
3. 图级显式属性（diagram { ... }）         —— 用户对整图的覆盖
4. 元素级属性（node/edge/group 上的属性）  —— 最强，逐元素
```

规则：
- **后者覆盖前者，逐字段（不是整块替换）**；
- profile 只在**解析层/编排层**展开，引擎收到的是已展开的完整参数（ADR-001 的直接推论）；
- 引擎侧的参数结构体应是 **完全确定的**（无 `Option` 的"未定"状态），把"未定→默认"的解析全部在展开时做完。
  这样引擎里不存在"这个参数没给，我猜一下"的逻辑。
- 元素级参数应有明确的作用域语义（组上的 `node_gap` 是否影响子节点？—— 需明确回答并写进 spec）。

### 3.1 参数展开的实现形态
```rust
// 解析/编排层
pub struct LayoutContract {
    pub layout: LayoutParams,      // 全部字段有值
    pub routing: RoutingParams,
    pub overrides: ElementOverrides, // 逐元素的稀疏覆盖（IndexMap<Id, PartialParams>）
}
```
- 引擎读 `overrides` 时用 `params_for(node)` 之类的查询函数，内部做"元素级 → 图级"的回退；
- **`params_hash` 进诊断输出**（09 篇）：让"结果变了"能归因到"参数变了"还是"代码变了"。这条极其实用。

---

## 4. 回写协议（下游需求 → 上游参数）

写权纪律允许的唯一"反向影响"是**参数回写**，且必须在上游相执行**之前**完成。
需要一个显式的机制而不是散落的赋值：

```rust
/// 由下游相在"预算阶段"贡献需求，上游相在执行前汇总
pub struct DemandBoard {
    layer_gap_demand: BTreeMap<(LayerIdx, LayerIdx), f64>,  // 走廊需求
    node_size_demand: BTreeMap<NodeId, Size>,               // 端口数/标签
    node_gap_demand:  BTreeMap<(NodeId, NodeId), f64>,       // 标签宽度
}
```

已知的回写项（应全部走 DemandBoard）：

| 需求来源 | 回写目标 | 时机 |
|---|---|---|
| 层间边数 × track 间距 | `layer_gap[i][i+1]` | P4 坐标之前 |
| 端口数量 × 最小端口间距 | 节点最小宽/高 | measure 之后、P4 之前 |
| 边标签尺寸 | 层间距 / 节点间距（或 label dummy 尺寸） | P2 dummy 化时 |
| 节点标签尺寸 | 节点尺寸 | measure 相内 |
| 自环数量与尺寸 | 节点有效包围盒 | P4 之前 |
| 组标题与 padding | 组框最小尺寸 | 组坐标之前 |
| 消息标签宽度（序列图） | 生命线间距 | 次轴坐标之前 |
| 嵌套激活条深度 | 生命线宽度 | 同上 |

**纪律**：
1. DemandBoard 只允许 `max` 合并（需求是下界），不允许覆盖 —— 保证合并顺序无关（确定性）；
2. 汇总后写入参数的动作**只有一个写者**（参数写者）；
3. 若某个需求在上游已执行后才产生 → 是设计错误，说明相顺序需调整，不是"就地挪一下"。

---

## 5. 各 profile 的具体取值（对照表）

ADR-001 的封闭集：`flowchart | sequence | architecture | state | er | mindmap`。
下表是**参数取值**，不是代码分支 —— 引擎对每一行的处理完全相同。

| 参数 | flowchart | architecture | state | er | mindmap | sequence |
|---|---|---|---|---|---|---|
| `orientation` | TB | TB 或 LR | LR | LR | LR（双向） | TB（主轴） |
| `layering` | NetworkSimplex | NetworkSimplex | NetworkSimplex | NetworkSimplex | TreeDepth | EventPartialOrder |
| `order_source` | Median+Transpose | Median+Transpose | Median+Transpose | Median | Declaration | Declaration→MinLA |
| `coord` | BrandesKopf | BrandesKopf+VPSC | BrandesKopf | BrandesKopf | TreeNodePlacer | PrefixSum |
| `router` | Orthogonal | Orthogonal | Polyline/Curved | Orthogonal | Curved/Polyline | Straight(水平) |
| `corner_radius` | 中 | 中 | 大（状态用圆润） | 0（工程感） | — | 0 |
| `default_port_policy` | FREE | FIXED_SIDE | FREE | **FIXED_POS**（字段级） | FREE | — |
| `self_loop_placement` | Side | Side | **Corner（显著）** | Side | — | **Inline（2 行回环）** |
| `edge_label_placement` | 中点上方 | 中点 | **靠源端**（转移条件） | 端点附近（基数） | 无 | 中点上方 |
| `group_support` | 有 | **强（嵌套+泳道）** | 有（复合状态） | 弱 | 无 | 片段容器 |
| `bundling` | 无 | Ordered（可选） | 无 | 无 | 无 | 无 |
| `node_gap / layer_gap` | 中/中 | 大/大 | 中/大 | 小/中 | 小/中 | 由标签决定 |
| 特有机制 | — | 组边界穿越 | 初/终态形状 | crow-foot 箭头 | 左右分支平衡 | 主轴离散化 |

**注意最后一行**："特有机制"仍必须是参数化的能力（如 `mindmap` 的左右分支 = `tree.balance: TwoSided`，
`sequence` 的主轴 = `major_axis: Discrete{...}`），而不是图种专属代码路径。

---

## 6. 判定"参数域够不够"的方法

这是本篇最实用的部分。三个可执行的检验：

### 检验 1：新图种落地测试
选一个尚未支持的图种（如 BPMN、SBGN、甘特、数据血缘），尝试**只用参数表达它**。
写出参数取值表，然后问：哪些格填不出来？
- 填不出来的格 = 缺失的参数维度（应新增参数）；
- 填得出来但"需要代码支持某个取值" = 缺失的机制（应新增机制，服务所有图种）。

已知的检验结果（来自 10 / 18 / 19 篇）：
| 目标图种 | 缺什么 | 应新增的**通用**能力 |
|---|---|---|
| BPMN | 泳道 + 主流程直线优先 | `swimlane`（=PartitionGrid）、`straighten_path: Option<PathSpec>` |
| SBGN | 严格形状语义 + 反应节点 | 纯样式层，无需布局参数 |
| 甘特 | 连续时间主轴 + 区间行分配 | `major_axis: Continuous`、`row_assignment: IntervalColoring` |
| 数据血缘 | 列级端口 + 总线 | `port_granularity: Field`、`bundling: Bus`（18 篇） |
| EDA 原理图 | 引脚固定 + 器件翻转 | `node_flip: Allowed`、`port_policy: FixedPos` |
| 网络拓扑 | 固定坐标 + 纯路由 | `layering: Fixed`、`coord: Fixed`（即 PartialLayout） |

**这张表就是路线图**：每一行右列的能力都服务多个图种，值得做；没有一行需要"图种分支"。

### 检验 2：反向审计（grep 测试）
```
在 tautcore-engine 里 grep：diagram_type | profile | DiagramType
                            "flowchart" | "sequence" | "er" | "mindmap"
期望结果：0 命中
```
可以做成 CI 检查（一个几行的脚本 + `#[deny]`/`grep` gate）。**这是 ADR-001 唯一的可执行守卫。**
建议同时禁止引擎依赖 `tautcore-model::profile`（crate 依赖层面的硬约束，比 grep 更强）。

### 检验 3：参数正交性审计
对参数表两两检查：
- 是否存在"设了 A 就必须设 B"的隐含依赖？→ 应合并为一个枚举，或明确文档化为约束并在展开层校验；
- 是否存在无效组合（如 `router: Straight` + `corner_radius: 8`）？→ 展开层应**报诊断**（而非静默忽略），
  静默忽略会让用户以为参数生效了。

---

## 7. 参数的文档化与演进

- **单一真源**：参数注册表放在 dsl-spec §14（属性注册表）。代码里的默认值应从注册表生成或与之做一致性测试
  （一个"注册表 vs 代码默认值"的单测，防止两处漂移）。
- **无向后兼容**（AGENTS.md 红线）→ 参数可自由重命名删除，但**每次改动要同步注册表 + profile 表 + 基准报告**。
- 参数变更应体现在 `params_hash`：基准对比时能区分"参数变了"与"算法变了"。
- **不要提供"万能逃生舱"参数**（如 `extra_options: Map<String, String>`）——
  见 [ADR-002](../../design/adr/002-no-config-block-freeform-options.md)：自由形式选项会变成隐藏的图种分支，绕过整个纪律。

---

## 8. 与 yFiles 的对照（作为参数域完备性的参考）

yFiles 的参数结构值得逐项对照检查自己有没有漏（以下为其公开 API 的概念，非源码）：

| yFiles 概念 | 本项目对应 |
|---|---|
| `HierarchicLayout` 的 `LayoutOrientation` | `orientation` + Orientation Stage（09 篇） |
| `NodeLayoutDescriptor`（每节点的层对齐、最小距离、端口分布） | 元素级 overrides |
| `EdgeLayoutDescriptor`（每边的最小长度、路由风格、优先级、可否分组） | 元素级 overrides |
| `LayoutData`（强类型的逐元素数据映射） | `ElementOverrides` + IndexMap |
| `PortConstraint` / `PortCandidate` | 端口五档模型（08 篇） |
| `LayerConstraintData` / `SequenceConstraintData` | 层/序约束（07 篇） |
| `PartitionGridData` | swimlane / PartitionGrid |
| `*Stage`（各种装饰器） | Stage 装饰器链（09 篇） |
| `EdgeRouter` 的 `scope` / `Grid` / `PenaltySettings` | 路由参数组（`bend_penalty` 等） |
| `LayoutExecutor`（动画、视口、边缓存） | 17 篇的 Transition Planner |

**这张对照表的用法**：右列为空的行 = 参数域的空洞。
当前已知空洞：`EdgeLayoutDescriptor` 级别的**逐边路由优先级**（哪条边优先占好走廊）尚未参数化 ——
它是 18 篇"拆线重布"的输入，建议随路由优化一起补上。

---

## 9. 落地清单（给本项目）

1. **参数表按相组织**（§2 准则 2 的表），一次写全，作为 dsl-spec §14 的骨架。
2. **展开层实现四层覆盖**（§3），引擎侧参数结构体**无 Option**。
3. **DemandBoard 机制**（§4）替代散落的回写赋值，只允许 max 合并。
4. **CI 加 ADR-001 守卫**（§6 检验 2）：引擎不依赖 profile crate + grep 图种名 0 命中。
5. **`params_hash` 进诊断输出**，基准报告里展示。
6. **无效参数组合报诊断**，不静默忽略。
7. **按 §6 检验 1 做一次"新图种落地测试"**（建议用 BPMN 与甘特），把缺口列成参数域的 TODO ——
   这是判断"参数域够不够"最快的一次性投入。
