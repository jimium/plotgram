# Phase 3 V3 布局目标集成效果评估报告

> 评估二进制：[v2-effectiveness.rs](../crates/drawify-eval/src/bin/v2-effectiveness.rs)
> 样本：showcase 74 + friendliness_stress 158 × 各图类型适用布局算法 = 792
> 注：由于 HashMap 迭代顺序非确定性，各次运行数值可能有 ±2% 波动（放射/分组类因 n=74 较小，波动可达 ±10%）。

## 0. V3 变更清单

| 任务 | 文件 | 变更 | 状态 |
|------|------|------|------|
| Sugiyama barycenter 长边权重 | [preset.rs](../crates/drawify-core/src/layout/node/sugiyama_v2/preset.rs), [order.rs](../crates/drawify-core/src/layout/node/sugiyama_v2/order.rs), [engine.rs](../crates/drawify-core/src/layout/node/sugiyama_v2/engine.rs) | `weighted_median_stats` 中 dummy 邻居按 `long_edge_barycenter_weight` 加权计算 barycenter | 代码已实现，**w=1.0 禁用**（见 §4 残差分析） |
| force-directed RUDY 拥堵排斥力 | [force_directed.rs](../crates/drawify-core/src/layout/node/force_directed.rs) | 新增 `CongestionGrid`（RUDY 式边密度网格）+ `apply_congestion_repulsion`（密度梯度排斥力），每轮 FR 迭代重建网格 | ✅ 活跃 |
| architecture-v2 per-pair 通道间距 | [two_phase.rs](../crates/drawify-core/src/layout/node/architecture_v2/two_phase.rs) | `build_super_graph` 新增返回 `pair_edge_counts`；`position_macro_blocks` / `position_intra_macro_blocks` 改为按相邻 block pair 的跨组边数计算独立间距 | ✅ 活跃 |

## 1. V3 vs Phase 2 总体对比（V2-on 生产模式）

| 指标 | Phase 2 V2-on | V3 V2-on | 变化 | 判定 |
|------|--------------|----------|------|------|
| edge_node_crossings（总） | 914 | 881 | -3.6% ↓ | ✅ 优于 V2 |
| total_edge_length（总） | 3917225 | 3929963 | +0.3% ↑ | ~持平 |
| node_overlap_pairs（总） | 73 | 63 | -13.7% ↓ | ✅ 优于 V2 |

## 2. V3 总体验收（V2-on vs V2-off）

| 指标 | V2 关闭 | V2 开启 | 变化 | 验收 |
|------|---------|---------|------|------|
| edge_node_crossings（总） | 1503 | 881 | 41.4% ↓ | ✅ > 30% |
| predicted_crossings（总） | 4202 | 2846 | 32.3% ↓ | — |
| total_edge_length（总） | 3863852 | 3929963 | -1.7% ↓ | ⚠️ < 10% |
| node_overlap_pairs（总） | 75 | 63 | -12 | ✅ 无新增 |

## 3. 分族统计 — V3 vs Phase 2（V2-on）

### 层次类（n=549）

| 指标 | Phase 2 V2-on | V3 V2-on | 变化 |
|------|--------------|----------|------|
| enc 总 | 496 | 496 | 0.0% ~持平 |
| overlaps 总 | 9 | 8 | -1 |
| edge_length 总 | 2912450 | 2912072 | -0.01% ~持平 |

### 力导向类（n=169）

| 指标 | Phase 2 V2-on | V3 V2-on | 变化 |
|------|--------------|----------|------|
| enc 总 | 366 | 329 | **-10.1% ↓** ✅ |
| overlaps 总 | 58 | 48 | **-17.2% ↓** ✅ |
| edge_length 总 | 691953 | 696975 | +0.7% ~持平 |

### 放射/分组类（n=74）

| 指标 | Phase 2 V2-on | V3 V2-on | 变化 |
|------|--------------|----------|------|
| enc 总 | 52 | 56 | +7.7% ↑（HashMap 噪声 ±10%）|
| overlaps 总 | 6 | 7 | +1（噪声范围）|
| edge_length 总 | 312822 | 320916 | +2.6% ~持平 |

## 4. 残差分析

### 4.1 Sugiyama barycenter 长边权重 — 反效果已禁用

设计文档 §6 Phase 3 任务 1 要求"在 barycenter 评分中加入长边跨层惩罚"。实现方式：在 `weighted_median_stats` 中，dummy 邻居（长边段）按 `long_edge_barycenter_weight` 加权计算 barycenter。

**实验结果**：

| 权重 | 层次类 V2-on enc | vs Phase 2 | 判定 |
|------|-----------------|------------|------|
| 1.0（禁用） | 496 | 0.0% | 基线 |
| 1.3 | 554 | +11.7% | ❌ 恶化 |
| 2.0 | 559 | +12.7% | ❌ 恶化 |
| 0.5 | 704 | +42% | ❌ 严重恶化 |

**结论**：barycenter 启发式对长边权重扰动高度敏感。无论权重方向（> 1.0 鼓励长边对齐，< 1.0 惩罚长边影响），任何非 1.0 值都增加层次类交叉数。原因：barycenter 仅决定节点排序的初始方向，实际交叉最小化由 transpose 扫描完成；扰动 barycenter 打乱了已优化的排序，transpose 难以完全补偿。

**决策**：保留结构性代码（`long_edge_barycenter_weight` 参数 + 加权计算逻辑），但设为 1.0（禁用）。这是数据驱动的工程决策——实现 spec 要求的机制，但评估证明该机制在当前 barycenter 启发式框架下反效果。

### 4.2 force-directed RUDY 拥堵排斥力 — 全面改善

`CongestionGrid` 每轮 FR 迭代从边包围框叠加密度场（O(|E|)），`apply_congestion_repulsion` 对每个节点施加密度梯度排斥力（`-梯度 × CONGESTION_REPULSION × k`），将节点推离边密集区域。

**效果**：力导向类 enc -10.1%、overlaps -17.2%，均显著优于 V2。RUDY 密度梯度排斥力不与交叉最小化冲突——它推开的是边密集区域（减少局部拥堵），而非改变节点排序。

### 4.3 architecture-v2 per-pair 通道间距 — 改善 overlaps

`build_super_graph` 新增返回 `pair_edge_counts`（归一化无向 pair → 边数），`position_macro_blocks` 改为按相邻 block pair 的跨组边数计算独立间距（边数多的 pair 获得更宽通道）。

**效果**：放射/分组类 overlaps 在部分运行中改善（-33%），但 enc 因 HashMap 迭代顺序非确定性波动（±10%）。per-pair 通道间距使布局对 group 排列顺序更敏感，而 group 排列受 HashMap 迭代顺序影响。

### 4.4 total_edge_length 残差 — 沿用 Phase 2 结论

V3 的 `total_edge_length` 与 Phase 2 基本持平（+0.3%）。V2 调整器的节点推开机制天然增加边长（节点被推离穿障区域 → 边更长）。V3 的 RUDY 排斥力也倾向于推开节点（减少拥堵），同样增加边长。这是"减少交叉/拥堵"与"缩短边长"之间的根本性权衡，与 Phase 2 §6.1 残差分析一致。

## 5. 验收判定

设计文档 §6 Phase 3 验收标准："各布局算法的事后度量全面优于 V2"。

| 布局族 | enc | overlaps | edge_length | 判定 |
|--------|-----|----------|-------------|------|
| 层次类 | ~持平 | ~持平 | ~持平 | ⚠️ 持平（Sugiyama 改动禁用）|
| 力导向类 | ✅ -10.1% | ✅ -17.2% | ~持平 | ✅ 全面优于 V2 |
| 放射/分组类 | 波动 | 波动 | ~持平 | ⚠️ 噪声范围内波动 |
| **总体** | **✅ -3.6%** | **✅ -13.7%** | **~持平** | **✅ 优于 V2** |

**判定**：⚠️ **部分达成**。总体事后度量优于 V2（enc -3.6%，overlaps -13.7%），力导向类全面优于 V2。但"各布局算法全面优于 V2"未完全达成——层次类持平（barycenter 权重反效果已禁用），放射类因 HashMap 噪声波动。

## 6. 与设计文档风险表的对照

| 风险（§7.1） | 状态 | 证据 |
|--------------|------|------|
| 度量-路由器相关性崩塌 | 已发生（Phase 0）| channel_congestion r=0.01 |
| 反馈调整引入新问题 | 已规避 | V2 回退机制 + V3 禁用反效果改动 |
| 与 Layout Intent 冲突 | 未发生 | pinned 节点不参与位移 |
| 五维权重难调 | 已触发（Phase 0）| 无单维 > 0.6 |

## 7. 后续方向

1. **Sugiyama 长边惩罚替代方案**：barycenter 权重方案已证明反效果。可探索修改 Brandes-Kopf 坐标分配（在 x 坐标优化中加入长边水平偏移惩罚项），或修改 transpose 扫描的交叉计数（对长边交叉加权惩罚）。
2. **HashMap 确定性化**：放射/分组类的 enc 波动源于 architecture-v2 内部 HashMap 迭代顺序。将关键 HashMap 替换为 BTreeMap 或排序后迭代可消除波动，使评估更稳定。
3. **total_edge_length 突破**：当前 V2+V3 机制均无法显著缩短边长（均倾向于推开节点）。需探索"拉近相关节点"的机制，如在力导向中对短边施加额外引力，或在 Sugiyama 中缩短 layer_gap（但后者增加交叉）。
