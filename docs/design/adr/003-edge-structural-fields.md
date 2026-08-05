# ADR-003: Edge 结构一等字段与时序边序

> 状态：accepted  
> 日期：2026-07-30（修订 2026-08-05：移除 `edge_group`）  
> 关联：ADR-001、dsl-spec §7.4、[`model-boundary.md`](../model-boundary.md)

## 背景

自由 `AttrMap` 适合 `style.*` / `meta.*` / `variant`，不适合布局写权相关的自由度：端口若只埋在 attrs，引擎与 Ink 容易「猜」，违反单写者纪律。时序若再引入 `seq` 字段，与「声明序」双真源。

边合流曾用作者 `edge_group` id；与 yFiles AutomaticEdgeGrouping 不对齐，且强迫作者逐边标记。改为布局开关 `auto_edge_grouping`，算法按同源/同汇自动成组。

## 决策

1. **`Edge` 一等字段**：`from_port` / `to_port`（`Option<PortConstraint>`）、`critical`（`bool`）。
2. **DSL 键** `from_side` / `from_slot` / `to_side` / `to_slot` / … / `critical` 由解析器 **提升** 进字段（`Edge::lift_structural_attrs`），之后从 attrs 剥除；引擎只读字段。
3. **边合流**：`layout: hierarchical { auto_edge_grouping: true }`；**不**提供边级 `edge_group`（写了 → 解析错误）。
4. **决议端口** `PortRef` 写在 `EdgePlacement`，由布局组合相写入；Ink 只读。
5. **时序时间轴**：`layout: sequence` 时 = `Graph::edges_in_declaration_order()`；**不**设 `Edge::seq`。

## 后果

- model / dsl-spec / model-boundary 对齐；端口键可标为模型字段 `active`。
- ER 字段表、fragment 等仍不进本 IR；组间「框到框」见 ADR-004（`group_anchor`），不引入 group 端点。
- 增量 mental map 仍走 `prev` API，不改 `Graph` 形状。
- 将来若需要「只合部分扇出」的自定义组，再另开作者组 id（对齐 yFiles Custom Edge Grouping），与 auto 开关互斥策略一并设计。
