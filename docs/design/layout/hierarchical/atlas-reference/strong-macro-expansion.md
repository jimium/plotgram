# StrongMacro 展开序

> 父页：[atlas-reference/README.md](README.md)  
> 对应新架构：[architecture.md §8.2 Group policy](../architecture.md)  
> Atlas 源：`crates/v1/plotgram-core/src/layout/atlas/dialect/contraction/strong_macro/{mod,intra,phase_d,super_graph,macro_block,intra_builder}.rs`

## 这是什么

StrongMacro 是组策略的「先定语义舞台（group），再在框内摆节点」模式。Atlas 把它分成四步：**intra-rank → super-graph → macro-block → phase-D 回填**。新架构 §8.2 要求 StrongMacro「可递归求局部 Plan，但展开后必须归一为同一全局 Plan schema」——Atlas 的四步是这条件的具体实现形状。

## 四步流程

```
┌─────────────────────────────────────────────────────────────┐
│ Phase A: intra-rank（组内布局，递归）                         │
│   layout_intra_group_recursive(diagram, gid, group_tree, …)  │
│   叶子组: layout_intra_group(rank + layer + order + solve)   │
│   容器组: 递归子组 → IntraMacroBlock → super_graph_for_group │
│           → assign_super_macro_ranks → position_intra_macro  │
│           → compose_intra_layout_recursive                   │
│   输出: HashMap<group_id, IntraLayout>                       │
└─────────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│ Step 2: super-graph（宏观图）                                 │
│   build_super_graph(graph, group_map, reversed)              │
│   超级节点 = 顶层 group + @node:{node}                       │
│   超级边 = 跨超级节点的有效边（accumulate_super_edge）        │
│   输出: (super_members, super_edges,                         │
│         pair_edge_counts, edge_weights)                      │
└─────────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│ Step 3: macro-block（宏观块定位）                             │
│   build_macro_blocks: intra 装箱成 MacroBlock（组块+padding）│
│   assign_super_macro_ranks: 给超级节点分层                    │
│   position_macro_blocks_stacked: 逐 rank 纵向堆叠            │
│     行内 band_uniform_gap + extra_layer_gap                  │
│     Center 时 center_rank_rows                               │
│   输出: Vec<MacroBlock> + block_row                           │
└─────────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│ Step 4: phase-D 回填                                         │
│   expand_global_layout: ContractionMeta + expand_contraction │
│     Super 点 = 内容原点（block.x + padding.left/top）        │
│   nudge_intra_nodes_toward_cross_group_edges (Phase C+):     │
│     P2.1 y 对齐 + x 微调（跳过组内 hub）                     │
│   clamp_to_canvas + member_slots_from_blocks                 │
│   (legacy) phase_d_postprocess: seed→gutters→materialize     │
│   输出: nodes + groups + slots + sugiyama_ranks              │
└─────────────────────────────────────────────────────────────┘
```

## 核心数据结构

```rust
pub(super) struct MacroBlock {
    id: String,
    is_group: bool,
    width: f64, height: f64,
    x: f64, y: f64,
    intra: IntraLayout,   // [v1-coupled] kernel::common::divide_and_conquer::IntraLayout
}

pub enum RowAlign { Start, Center }

pub struct StrongExpandResult {
    pub nodes: HashMap<String, NodeLayout>,
    pub slots: std::collections::BTreeMap<String, crate::layout::atlas::plan::Slot>,
    pub sugiyama_ranks: HashMap<String, usize>,
    pub canvas_padding: f64,
}
```

## 关键不变量

- **「Super 点 = 内容原点」**：组块坐标 = `block.x + padding.left/top`，与 `compose_global_layout` 数值对齐。展开/收缩对称。
- **`member_slots_from_blocks`**：层归属来自同行对齐 `intra.layers`；同层 order 按 expand 后 x（确定性）。
- **过扁组回退 Grid**：`MIN_GROUP_ASPECT = 0.25`，Horizontal + members ≥ 3 + height < width * 0.25 时切 Grid。
- **band 内统一 gap**：同 RankBand 内取所有相邻 pair 的 `adaptive_group_gap` 最大值——避免行内 gap 不一致。
- **递归嵌套**：容器组 IntraLayout 包含所有后代节点局部坐标（相对容器组内容区原点）。

## Phase C+ nudge 常量

| 常量 | 值 | 含义 |
|------|-----|------|
| `CROSS_GROUP_NUDGE_BASE` | 16 | x 微调基础位移 |
| `CROSS_GROUP_NUDGE_MAX` | 48 | x 微调上限 |
| `CROSS_GROUP_NUDGE_WIDTH_RATIO` | 0.3 | 可用宽比例 |
| `CROSS_GROUP_NUDGE_DIST_RATIO` | 0.3 | 距离比例 |
| `CROSS_GROUP_Y_ALIGN_MAX` | 20 | y 对齐上限 |
| `CROSS_GROUP_Y_ALIGN_RATIO` | 0.5 | y 对齐比例 |

## adaptive gap 公式（macro_block.rs）

```rust
// gap = base + min(edge_count * scale, max_extra)
adaptive_group_gap(edge_count) = GROUP_GAP_X + min(edge_count * CROSS_EDGE_GROUP_GAP_SCALE, MAX_EXTRA_GROUP_GAP)
// CROSS_EDGE_GROUP_GAP_SCALE = 8, MAX_EXTRA_GROUP_GAP = 56

adaptive_vertical_rank_gap(from_rank, from_pair) =
    max(GROUP_GAP_Y + min(rank_cross_count * 12, 80),
        GROUP_GAP_Y + min(pair_cross_count * 10, 56))
```

## 典型测试场景

| 测试名 | 验证什么 |
|--------|---------|
| `strong_layout_core_*` | 四步流程端到端 |
| `r31_helpers_heights_gaps_and_delta_tops` | 层高 / base_gaps / apply_layer_tops 只扩不缩 |
| 嵌套组递归 | 容器组 IntraLayout 含全部后代，layers 反映宏观层级 |

## 不该照搬

1. **`nudge_intra_nodes_toward_cross_group_edges`**：post-solve 修改已求解坐标——组内节点本应在 intra 布局时就考虑跨组边方向（注释承认是「先定组框再微调组内节点」的反转步骤）。新实现应把跨组边方向作为 intra 布局的 objective，不在 post-solve 改。
2. **`phase_d_postprocess`**：seed → gutters → 单次 materialize，是 post-solve 修几何。新实现应把 corridor demand 提到布局主循环内联。
3. **`pub(super) use crate::layout::recipes::architecture::*`** `[v1-coupled]`：整个 strong_macro 与 `recipes::architecture` 深度耦合。新实现应通过 profile 参数化，不直接依赖 architecture recipes。
4. **`MacroBlock` 字段全 `pub(super)`，无封装**：`x`/`y` 可被任意改写。新实现应封装，坐标写入归单一写者。
5. **`layout_intra_group` 内 `diagram.diagram_type.clone()`** `[v1-coupled]` 传给 `resolve_group_layout_hint`——图种分支（虽封装在 hint 解析里）。新实现应让 mode 由 profile 注入。
6. **Sugiyama 委托 `intra_sugiyama::layout_intra_with_sugiyama_v2`** `[v1-coupled]`：组内布局走完全不同的算法栈。新架构 §8.2 要求「一个组合相」，组内/组外应共享同一算法栈。
7. **`compose_intra_layout_recursive` 的 layers 重建**：简化策略直接按 block y 顺序拼接 intra.layers，依赖宏观定位已保证 y 不重叠——脆弱。
8. **`intra_builder` 把 hub 居中 + client 对齐硬编码进 P1 objective** `[v1-coupled]`：architecture 图种语义。新实现应让 hub/client 概念由 profile 注入 objective，不写死。
9. **gap 全是经验常量**（`*12`, `*10`, `*8`, max 80/56/56）：无 principled 推导。新实现应从 corridor demand 推。
10. **`set_override_for_solve` / `clear_override_for_solve`** `[v1-coupled]`：全局可变状态传 GroupSizing——并发不安全。

## 新实现建议

- **保留四步流程的拓扑**（intra → super → macro → expand），但每步用独立 Writer。
- **intra 布局共享主算法栈**（NS + median + BK + VPSC），不另起 Sugiyama 委托。
- **跨组边方向作为 intra objective**，不在 post-solve nudge。
- **corridor demand 提到主循环**，不在 phase-D 后处理扩 gutter。
- **gap 从 corridor demand 推**，不用经验常量。
- **mode（Vertical/Horizontal/Grid/Sugiyama）由 profile 注入**，不读 `diagram_type`。
- **hub/client 等图种语义由 profile 注入 objective**，不写死在 builder。
- **`MacroBlock` 封装**，坐标写入归 macro-block Writer。
- **StrongMacro 产出归一到全局 Plan schema**（§8.2 第 4 条）：四步都写同一 Plan，不引入 IntraLayout 这个第二 IR。
