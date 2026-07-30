# ADR-003: Edge 结构一等字段与时序边序

> 状态：accepted  
> 日期：2026-07-30  
> 关联：ADR-001、dsl-spec §7.4、[`model-boundary.md`](../model-boundary.md)

## 背景

自由 `AttrMap` 适合 `style.*` / `meta.*` / `variant`，不适合布局写权相关的自由度：端口、边组若只埋在 attrs，引擎与 Ink 容易「猜」，违反单写者纪律。时序若再引入 `seq` 字段，与「声明序」双真源。

## 决策

1. **`Edge` 一等字段**：`from_port` / `to_port`（`Option<PortConstraint>`）、`edge_group`（`Option<String>`）。
2. **DSL 键** `from_side` / `from_slot` / `to_side` / `to_slot` / `edge_group` 由解析器 **提升** 进字段（`Edge::lift_structural_attrs`），之后从 attrs 剥除；引擎只读字段。
3. **决议端口** `PortRef` 写在 `EdgePlacement`，由布局组合相写入；Ink 只读。
4. **时序时间轴**：`layout: sequence` 时 = `Graph::edges_in_declaration_order()`；**不**设 `Edge::seq`。

## 后果

- model / dsl-spec / model-boundary 对齐；端口四键可标为模型字段 `active`（引擎端口决策实现前，未指定端仍由算法填；已提升的约束须被尊重或显式降级）。
- ER 字段表、fragment、接到 group 框等仍不进本 IR。
- 增量 mental map 仍走 `prev` API，不改 `Graph` 形状。
