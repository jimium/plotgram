# 布局共享语义

> 状态：占位  
> 目的：group / port / label 等**横切**契约，供各布局内核引用，避免在 hierarchical/tree/… 里各写一套漂移定义。

## 将覆盖

| 主题 | 要点 | 已有锚点 |
|------|------|----------|
| Group | 层次一等公民；框由布局写；组间边经 anchor（非 group 作端点） | [ADR-004](../../adr/004-group-anchor-nodes.md)、[model-boundary](../../model-boundary.md) |
| **Partition** | 正交网格（泳道/矩阵）；与 group/rank 三分 | [partition.md](partition.md)、[ADR-008](../../adr/008-partition-grid.md) |
| Port | `PortConstraint`（作者）→ `PortRef`（决议）；along 属组合相 | model `port`、[写权纪律](../write-authority.md) |
| Label | Demand 进缝宽；完整 integrated labeling 渐进 | Hier [scope](../hierarchical/scope.md) 记债 |
| Edge group | `edge_group` 合流/总线；由路由/Ink 消费 | [ADR-003](../../adr/003-edge-structural-fields.md) |
| **Decoration** | 布局派生几何（生命线/激活条等）；不进 Graph | [ADR-009](../../adr/009-layout-result-decorations.md) |

## 原则

- 内核文档只写「本核如何消费」；语义定义以本夹 + model/ADR 为准。  
- 不在这里写图种特判。

## 待写

- [x] `partition.md`  
- [ ] `group.md`  
- [ ] `port.md`  
- [ ] `label.md`
