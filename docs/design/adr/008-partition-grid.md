# ADR-008: PartitionGrid（正交分区）一等公民

> 状态：accepted  
> 日期：2026-08-01  
> 关联：ADR-001、ADR-004、[`layout/shared/partition.md`](../layout/shared/partition.md)、[`model-boundary.md`](../model-boundary.md)

## 背景

泳道 / 架构正交条带需要的是与流向**正交**的全局网格约束（yFiles `PartitionGrid`），不是 `group` 包含树。

用 group「演」泳道只能交差观感：无法保证列内连续块、全局 rank 跨列对齐、空列最小宽、严格轴序。Channel 的 `lane`（走廊 track）与业务泳道同名异义，更不能顶替。

需要在 **DSL + model** 先钉死契约，避免引擎日后用 Horizontal 堆叠打补丁。

## 决策

1. **引入 `PartitionGrid` 一等 IR**（挂在 `Graph` 上）：有序 `columns` / `rows`（`PartitionAxis`：id + 可选 label）。声明序 = 几何轴序（稳定）。
2. **节点 cell 一等字段** `Node.partition_cell`：`column` / `row` 均为可选 axis id（只写列 = 泳道；行列都写 = 矩阵）。
3. **DSL 显式声明**，不做 group→列的 model 语义：
   - diagram 级：`partition { column id { label: … }  row id { … } }`
   - node：`cell_col` / `cell_row`（atom）→ lift 进 `partition_cell`，并从 attrs 剥除
4. **与 group / rank 三分**：
   - rank = 沿流向；group = 嵌套包含；partition = 正交网格
   - 节点可同时属于某 group **且** 占据某 cell；**禁止**「group ⇒ 自动成列」作为 IR 语义
5. **id 空间**：partition 轴 id 与 node / group id **同一空间，禁止冲突**。
6. **校验**：无 grid 却写了 cell → 错；cell 引用未知轴 → 错；有 grid 但节点无 cell → **允许**（未分区自由区；Hier 算法未接线前可忽略其列约束）。
7. **引擎消费**：标 **`planned`** 直至 Hierarchical 组合/度量相接线；未实现前不得崩溃，**禁止**用 group Horizontal 冒充 PartitionGrid。
8. **ADR-001**：grid 在 `Graph` 内随 `LayoutContract` 进入引擎；不引入 profile/图种分支。

## 含义

| 层 | 影响 |
|----|------|
| **model** | `partition` 模块；`Graph.partition`；`Node.partition_cell` + lift/校验 |
| **dsl-spec** | partition 块 + `cell_col`/`cell_row`；§14 注册 |
| **parse** | 待接（本 ADR 不阻塞 model）；接上后 lift + 轴/节点 id 冲突检查 |
| **engine** | 消费前忽略；接线后：列→层内连续块，行→层区间（见 shared/partition） |
| **render** | 分区标题/带背景可后置；不发明 cell |

## 非目标（首期）

- `swimlane` / `table` 关键字糖
- `group { lane: true }` 自动展开成列（若将来做糖，必须展开为显式 cell，且标为糖）
- Hier 定序/坐标/VPSC 列区间的完整算法实现
- 改 showcase 强制改写为 cell（待 parse 后）

## 备选方案（未采用）

| 方案 | 放弃原因 |
|------|----------|
| 仅用 group + Horizontal 堆叠 | 非全局网格；跨列对齐失败 |
| cell 用整数下标 | id 更稳、与 DSL 声明对齐 |
| grid 放进 `layout` options | 结构数据应在 Graph；options 适合策略参数 |
| group 作列真源 | 偷换包含语义；与 ADR-004 组框职责纠缠 |

## 参考

- [`docs/reference/yfiles/08-分组泳道与端口约束.md`](../../reference/yfiles/08-分组泳道与端口约束.md) §2  
- [`docs/design/notes/v1-atlas-solver-vs-vpsc.md`](../notes/v1-atlas-solver-vs-vpsc.md)（度量零件；与本 ADR 正交）
