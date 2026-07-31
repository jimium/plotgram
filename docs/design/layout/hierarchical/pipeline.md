# Hierarchical · 管线与写权

> 父页：[README.md](README.md)

## 1. 与经典 Sugiyama 的对应

| Sugiyama / yFiles | Atlas 相 | 产物 |
|-------------------|----------|------|
| P1 去环（FAS） | 组合 | 每条边是否反向 |
| P2 分层 | 组合 | `layer(v)` |
| P3 定序 + 虚节点 | 组合 | 层内 `order`；proper hierarchy |
| 端口 / 边组 / 通道拓扑 | 组合 | Plan：ports、gates、channels、bundles |
| P4 坐标 | 度量 | 节点框、主/次轴、track 像素、缝宽 |
| P5 边绘制 | Ink | `EdgePlacement.path`（展开，不发明） |

证据层逐步算法见 [01 Sugiyama](../../../reference/yfiles/01-sugiyama分层布局.md)。

## 2. 组合相子阶段（目标形态）

组策略是**收缩参数**，不是三条永久平行宇宙（Weak / StrongMacro / 无组 → 同一套产出类型）：

```text
I.1 Contraction   group policy → 收缩图 / 元数据
I.2 Ranking       layer assignment（含约束）
I.3 Ordering      crossing minimization
I.4 Gate          穿组/层界面闸门
I.5 Port          每边两端 side + along（或槽）
I.6 Channel       走廊拓扑；有界返工；非「Ink 里猜」
     ↓
   Plan（稳定序字段；禁止依赖 HashMap 遍历序）
```

度量相消费 Plan + `node_sizes`，一次写出节点几何与 track；Ink 只读 Metric / Plan。

## 3. 写权表（硬）

| 自由度 | 唯一写者 | 禁止 |
|--------|----------|------|
| 反向位、rank、order | 组合 | Ink / 事后 repair 改拓扑序 |
| 端口 side、along | 组合 | Ink `unwrap_or` 默认侧 |
| gate / channel 拓扑 | 组合 | 落笔发明 dogleg 穿组 |
| track 坐标、缝宽 | 度量 publish | Ink 用 bbox±ε 双写 |
| 节点 frame、组框 | 度量 | finalize 纯后验框作为真源（重建债） |
| 折点坐标 | Ink | 改 order / 挪节点「为了好画」 |
| label 带高度 | Demand→度量 | 仅事后避让且挤爆已发布间隙（目标债） |

判断句：若修法是「在 Ink 再加特判」，先问该自由度的写者是谁。

## 4. 确定性

- 邻接遍历、平局 tie-break、层内稳定序：显式排序或 `BTreeMap` / `IndexMap`。  
- 布局迭代不得依赖 `HashMap` key 序（[AGENTS.md](../../../../AGENTS.md) §2）。

## 5. 与重建代码的对照

| 设计相 | 重建 stub（当前） | v1 参考 |
|--------|-------------------|---------|
| Ranking | `rank.rs` 最长路式 | `kernel/layered` + NS 风格 |
| Ordering | 缺失 | `order.rs` weighted median |
| Ports / Channel | `ports.rs` 仅流向侧 | `atlas/channel/*` |
| Coordinate | `place.rs` 打包 | `kernel/coordinate/*` |
| Ink | `ink.rs` 单肘 | `atlas/ink.rs` + `ink_verify` |

重建目标：按本档相边界把 v1 能力迁入，而不是在 stub 上叠特判。
