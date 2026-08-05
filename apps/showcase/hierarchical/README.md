# hierarchical 样例

按结构能力分桶（见上级 [README](../README.md) 三层模型）：

| 目录 | 含义 |
|------|------|
| [`flat/`](flat/) | 无 group、无 PartitionGrid |
| [`group/`](group/) | 有 group；含用 group 表达分区的旧「泳道」样例 |
| [`fan/`](fan/) | 扇出 / 合流参数演示（`auto_edge_grouping`、`critical`） |
| [`styles/`](styles/) | `routing_style` 变体 |
| [`partition/`](partition/) | 真 PartitionGrid 占位（可空，等 M5） |

角色前缀仍为 `smoke.` / `product.` / `demo.` / `stress.` / `mech.`。

### `fan/` 开关对照

| 文件 | 开关 | 观感 |
|------|------|------|
| [`auto_edge_grouping.pgm`](fan/auto_edge_grouping.pgm) | `auto_edge_grouping: true` | 端总线 SharedPort→Trunk→Bus→Stub |
| [`critical.pgm`](fan/critical.pgm) | `critical` 边 | 关键路径权重 |

> 曾误引入的 `bus_routing` / `demo.bus_routing*.pgm` 已删除：对应的是 yFiles Layout Styles **demo** 面板名，不是 Hier API。
