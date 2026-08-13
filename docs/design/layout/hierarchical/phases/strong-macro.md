# Hierarchical · StrongMacro

> 父页：[architecture](../architecture.md) §8.2  
> 组合相：[composition](composition.md) §2  
> 对照：[group-frame-d2](group-frame-d2.md)（Weak 框写者）  
> 否决：[notes/anti-patterns.md](../notes/anti-patterns.md)  
> 状态：**已落地**（顶层 + 嵌套；architecture profile 默认 strong 本轮不做）

## 1. 一句话

`group_policy: strong-macro`：先定组的语义舞台（macro block），再在组内用**同一套** Hier 栈摆节点；展开后与 Weak **同一**全局 `Plan` schema。Ink / Channel **不得**按 policy 分叉。

解决架构图痛点（层叠舞台、等宽条带），**不是** Weak 框进 VPSC，也不是第二套 `ArchitectureLayout`。

## 2. 与 Weak / Channel 的边界

| 工作 | 框写者 | 与本文 |
|------|--------|--------|
| Weak | Metric 组框变量 | 对照；compact / snap 仅 Weak |
| StrongMacro | **MacroBlockWriter** | 本文 |
| Channel | 展开后的同一 Plan | 禁止第二路由栈 |

禁止同一 policy 下 MacroBlock 再叠 VPSC 框。`channel/` / `ink/` 模块内禁止 `match group_policy`（orchestrator hook 除外）。

## 3. 管线

```text
Graph + group_policy: strong-macro
  → Intra（组树后序；局部 rank/order/`J(x)`；禁止读 diagram_type）
  → Super-graph（顶层组 ∪ 未入组 real；跨超节点边）
  → Macro-block（装箱 + 主轴堆叠 + 行内 gap；**唯一写组框**）
  → Expand（内容原点 + 全局稳定 key → 与 Weak 同一 PlanGraph）
  → 共享 Channel / Ink（`compute_channel_ink_tail`；`TailFrames::Fixed`）
```

| 自由度 | 写者 | 禁止 |
|--------|------|------|
| 组内 layer / order | 局部 Compose | 父层事后改组内序 |
| 组内节点坐标 | 局部 Metric | post-expand nudge |
| macro 块 / 组框 | MacroBlockWriter | finalize 重算包围盒；Ink 平移组 |
| 跨组边方向 | 局部 / 超图 objective 或 Demand | `nudge_intra_nodes_*` |
| Gate / 路径 | 展开后既有 Channel | Strong 专用路由栈 |

组内另起 Sugiyama、事后扩 gutter、全局 override 传 sizing：见 [anti-patterns](../notes/anti-patterns.md)。

## 4. 现行机制（合同，不是日记）

- **块级 FAS**：节点 FAS 后块聚合可再成环；超图上再跑 `greedy_fas`，翻转工作方向（`original_*` 不动）。  
- **走廊隔离**：块级翻转边的两端若与兄弟共享局部层，组内求解时移入底部独占层。  
- **嵌套**：后序 Block 树；每作用域独立块级 FAS + 行堆叠；SM-D 按 band 展开（跨行边 span ≥ 1）。空组跳过。  
- **跨组 Demand**：`MacroRowGap` / `MacroColGap`（`base + min((count−1)×edge_gap, 4×edge_gap)`）；`macro_align_weight`（默认 1；`0` = 居中回退）。Weak 不读 → Weak 几何不变。  
- **端口**：叶块 intra 的 side 喂局部 VPSC；expand 后全局 `assign_ports`，再 `reconcile_intra_port_sides` 写回同叶块边。  
- **穿组**：`verify_no_group_penetration`；Strong 硬门禁。外轨避让组包络（`canonical_group_obstacles`）；Weak 传空表。

## 5. 模块

```text
hierarchical/strong_macro/
  mod.rs          // layout → 与 Weak 同型 LayoutOutput
  intra.rs        // 后序局部求解
  super_graph.rs  // 超节点 / 超边 / 块级 FAS
  macro_block.rs  // 装箱 + 堆叠（框写者）
  expand.rs       // → 全局 PlanGraph + frames
```

内部 IR（`IntraResult` / `MacroBlock` / …）不泄漏到 Ink。

## 6. 验收

- `group_policy: strong-macro` 可跑 architecture / nested showcase。  
- Ink/Channel 源码无 `group_policy` 匹配分支。  
- 无 post-expand 节点 nudge；两次 run bit-identical。  
- 穿组 Strong 硬失败；组包含（子框 ⊆ 父框）硬门禁。

architecture profile **默认** strong：语料够了再评估（expectations 选型），本轮不做。
