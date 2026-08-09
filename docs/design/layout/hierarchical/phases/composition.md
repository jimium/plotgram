# Hierarchical · 组合相

> 父页：[architecture](../architecture.md) §4  
> 输入输出：[contracts-and-ir](contracts-and-ir.md)

组合相写全部离散事实，产出唯一 `Plan`。Metric 不得补 layer/order/port/gate/route topology。

## 1. 输入投影

先把 Graph 与 `HierarchicalLayoutData` 投影为 canonical 工作图：

- real node / edge 使用稳定声明序；
- group tree 计算每个节点的 scope path；
- PartitionGrid 只读 axis/cell 作者事实；
- fixed port、rank/order、edge min-span、alignment set 转为 typed constraint；
- 自环与平行边由 Stage 提取为 facts，不在普通 DAG 核里混特判。

输入投影不产生几何坐标。

## 2. Contraction

### Weak

组边界形成连续块与 scope 约束；可对组作超节点收缩以决定组间相对位置，但组内/组外边必须保留完整 crossing 元数据。

### StrongMacro

1. 按组树后序生成局部工作图；
2. 局部 Plan 只在局部 key 空间求解；
3. 将组作为 macro node 进入父层；
4. 展开时重映射为全局稳定 `NodeKey/GateKey/SegmentKey`；
5. 最终仍产出与 Weak 完全相同的 `Plan` schema。

禁止让后续 Metric/Ink 根据 policy 选择两套字段。

## 3. Cycle removal

主选 Greedy-FAS（ELS 骨架）+ **环 reroot**（环入口规则，[architecture.md](../architecture.md) §3.1）。输出：

```text
EdgeId → { working_source, working_target, reversed }
```

规则：

- self-loop 已由 Stage 提取；
- `undirected` 边不进 FAS、永不被反转（见 §5.3）；
- 平局按 `(score, declaration_index, EdgeId)`；
- **声明序靠前的节点，布局上优先靠上**：反转集确定后，若 0 号节点（声明最前）非工作源点且恰有一条未反转原始入边，则把切边旋转到该入边（同时取消一条现有反转，经无环校验，候选按最小边序）；旋转不改反转数，ELS 上界保留；条件不满足时保持 ELS 结果；
- bidirectional/response 仍以 Graph 的 source→target 作为声明方向参与工作图，渲染语义保留在 original edge；
- 作者若提供 hard preferred direction，与 FAS 冲突时报告不可行或按明确的 soft priority 计费，不得静默翻转硬约束。

## 4. Ranking

Network Simplex 目标：

```text
min Σ edge_weight(e) × span(e)
subject to layer(v) - layer(u) ≥ min_span(e)
```

还需注入：

- group 连续层 / macro 展开约束；
- PartitionGrid 映射到 main axis 的 band 区间；
- 显式 rank/rank_range；
- label/strong-port 需要的逻辑跨度下界。

`undirected` 边不施加 rank 约束：longest-path / NS 建图均跳过它，端点 rank 仅由其它边决定（见 §5.3）。

输出 layer 必须归一为从 0 开始的稠密层；空层若是显式 band/title 需求的一部分，可作为命名虚层保留，否则删除。

## 5. Properify 与强端口投影

### 5.1 长边

每条跨度大于 1 的 working edge 生成稳定 dummy 链：

```text
Virtual { owner: edge_id, kind: LongEdge, ordinal: layer_offset }
```

Plan 最终保留 dummy chain 与 original edge 的映射，Ink 不重新推导。

### 5.2 强端口

FIXED_ORDER / FIXED_RATIO / FIXED_POS 可能改变交叉计数。Ordering 前把它们投影成 port dummy / endpoint order constraint。  
该步骤不写最终 `PortPlan`，只生成排序约束；唯一 PortWriter 在 Ordering 后冻结决议。

### 5.3 Undirected 边（非分层）

语义对齐 yFiles `HierarchicLayouter` 的 `UNDIRECTED_EDGES_DPKEY`：边**不施加 rank 层级**，但仍路由、仍渲染。DSL 面为 `undirected: boolean`（dsl-spec §14.4），`<->` 箭头是糖（显式属性优先）。

- `assign_ranks` 之后、`properify` 之前由 `split_intra_layer` 分流：span == 0 移入 `RealGraph.intra_layer`（对 ordering / properify / channel 不可见）；span ≥ 1 规范化 working 方向朝下（仅交换 `working_*`，`reversed` 恒 false，永不产生回边），properify 的 `r1 > r0` 不变量由此保持；
- span == 0 边由 Ink 专用写者 `intralayer.rs` 展开为 side-link（跨轴水平直连，或经层缝的避让 U 形），镜像 self-loop 的旁路模式（端点 East/West 中点 port，LocalOffset 真实记录）。

## 6. Ordering

主循环：

```text
initial stable order
  → median down sweep
  → transpose
  → median up sweep
  → transpose
  → crossing count
  → best snapshot
```

最低规则：

- real-real / real-virtual / virtual-virtual 权重默认 1/2/8；
- group 与 cross-axis partition band 都投影为连续块约束；
- fixed order/port endpoint order 是 hard precedence；
- median 无定义时保留前一轮 order；
- tie-break tuple 固定为 `(median, previous_order, declaration_index, NodeKey)`；
- snapshot 比较固定为 `(weighted_crossings, total_span, lexicographic_order)`。

若连续块 precedence 成环：

1. 作者硬约束环 → `InfeasibleConstraint`；
2. 仅软 alignment 偏好造成 → 按 priority、id 固定顺序放宽并记录 diagnostics。

## 7. Port finalize

Ordering 冻结后：

- FIXED_* 校验并映射到 original source/target；
- FREE 端口按对侧 `(layer, order, EdgeId)` 排序；
- side 候选先比较合法性，再比较预计弯折/逆向出针，最后按 Side 固定枚举序；
- 写出唯一 `PortPlan`；
- 端口数与 port group 生成 MetricBudget。

详见 [ports-and-channel](ports-and-channel.md)。

## 8. Gate 与 RouteTopology

根据两端 scope path 的 LCA 生成 crossing 序列，再交给 Channel 搜索。  
每边最终必须恰有一种：

- `Orthogonal(ChannelPath)`；
- `Polyline(DirectOrVia)`；
- `Deferred(terminals + boundary permissions)`。

不存在“无 topology，等 Ink 自己连”的合法状态。

## 9. PlanVerifier

Freeze 前至少检查：

1. working 非自环边满足 `layer(v) > layer(u)`；
2. dummy chain 每段跨度为 1；
3. 每层 order 唯一、稠密、满足 hard precedence；
4. original/working 端点映射可逆；
5. 每边 source/target PortPlan 完整；
6. gate crossing 与 scope path 一致；
7. route topology 从 original source terminal 连通到 target terminal；
8. 所有稳定 key 唯一且序列化双跑一致。
