# PartitionGrid（正交分区）

> 状态：现行契约（model/DSL 已定；Hier 引擎消费 **已落地**，PG-0–PG-4；render 泳道底色后置）  
> ADR：[008-partition-grid](../../adr/008-partition-grid.md)  
> 实施方案：[hierarchical/phases/partition-grid.md](../hierarchical/phases/partition-grid.md)（PG-0–PG-4）  
> Model：`plotgram_model::partition`  
> 证据：[08 分组·泳道·端口](../../../reference/yfiles/08-分组泳道与端口约束.md) §2

## 1. 功能定位

| 概念 | 相对流向 | 职责 |
|------|----------|------|
| Sugiyama **rank** | **沿**流向 | 层号 |
| **group** | 树状包含 | 嵌套、组框、跨组边 |
| **PartitionGrid** | **正交**于流向 | 全局列/行格子；节点占 cell |

- **泳道** = 只用 columns（默认 TB 流向时列沿 x），或只用 rows（视 orientation）。  
- **矩阵** = rows + columns（如阶段 × 角色）。  
- **不是** Channel 走廊里的 track/`lane`。

```text
TB 流向（rank 沿 y）

  col_a   col_b   col_c     ← PartitionGrid.columns（声明序）
  ┌─────┬─────┬─────┐
  │  ●  │  ●──┼──●  │       ← 同一全局 rank 跨列对齐
  │  │  │     │  │  │
  │  ●  │  ●  │  ●  │
  └─────┴─────┴─────┘
```

## 2. 与 group 的关系

- **正交叠加**：节点可同时是某 group 成员 **且** `partition_cell` 非空。  
- **禁止** model 语义「group ⇒ 列」。group Horizontal 堆叠 ≠ PartitionGrid。  
- 将来若有泳道糖，parse 必须展开为显式 `column` + `cell_col`，IR 仍只有 grid。

## 3. 引擎约束契约（消费时）

| 相 | 约束 |
|----|------|
| P2 分层 | 若使用 **rows**：行 → 全局层区间；层跨列共享同一套 rank（不能每列各排各的） |
| P3 定序 | **columns** → 层内连续块（与 group 边界 dummy **同构机制**，政策不同） |
| P4 坐标 | 每列/行有区间变量；空轴保留最小尺寸（标题带）；VPSC/分隔约束写宽高 |
| P5 / Ink | 可跨分区走边；不发明 cell |

未指派 cell 的节点：允许存在；接线前可忽略分区约束；接线后策略（自由区 vs 报错）由 Hier profile 参数决定，默认宽松。

## 4. 写权

| 自由度 | 写者 |
|--------|------|
| 轴声明序、轴 id/label | DSL → model（作者） |
| `partition_cell` | DSL → lift（作者）；引擎只读 |
| 列/行像素区间 | 度量相（已落地：列带 VPSC；行带 = 主轴层堆叠 y 并集） |
| 组框 | 仍由 group 度量路径写；与列区间可对齐但真源不混 |

## 5. DSL / Model 摘要

见 ADR-008 与 dsl-spec（`partition { column … }`，`cell_col` / `cell_row`）。
