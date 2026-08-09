# hierarchical 样例

按结构能力分桶（见上级 [README](../README.md) 三层模型）：

| 目录 | 含义 |
|------|------|
| [`flat/`](flat/) | 无 group、无 PartitionGrid |
| [`group-weak/`](group-weak/) | `group_policy: weak`（默认）：软聚类，框由成员几何导出；全部聚类语义样例（含嵌套 / 无 label / 单成员 / 孤岛组等机制探针） |
| [`group-strong-macro/`](group-strong-macro/) | `group_policy: strong-macro` 基准：泳道严格分区 / 组级排序约束等需要宏收缩契约的样例；实现落地前只钉 Unsupported 契约（hier_eval 豁免） |
| [`fan/`](fan/) | 扇出 / 合流参数演示（`auto_edge_grouping`、`critical`） |
| [`styles/`](styles/) | `routing_style` 变体 |
| [`partition/`](partition/) | 真 PartitionGrid 占位（可空，等 M5） |

角色前缀仍为 `smoke.` / `product.` / `demo.` / `stress.` / `mech.`。

### `flat/` 夯实探针（M1）

| 文件 | 开关 / 主题 |
|------|-------------|
| [`smoke.auto-edge-grouping-on.pgm`](flat/smoke.auto-edge-grouping-on.pgm) | `auto_edge_grouping: true` |
| [`smoke.auto-edge-grouping-off.pgm`](flat/smoke.auto-edge-grouping-off.pgm) | 对照 off |
| [`smoke.critical.pgm`](flat/smoke.critical.pgm) | `critical` 边 |
| [`smoke.self-loop.pgm`](flat/smoke.self-loop.pgm) | 真自环 `a → a` |
| [`smoke.multi-rank-backedge.pgm`](flat/smoke.multi-rank-backedge.pgm) | 多 rank 回边 |

### `fan/` 开关对照

| 文件 | 开关 | 观感 |
|------|------|------|
| [`auto_edge_grouping.pgm`](fan/auto_edge_grouping.pgm) | `auto_edge_grouping: true` | 端总线 SharedPort→Trunk→Bus→Stub |
| [`critical.pgm`](fan/critical.pgm) | `critical` 边 | 关键路径权重 |

> 曾误引入的 `bus_routing` / `demo.bus_routing*.pgm` 已删除：对应的是 yFiles Layout Styles **demo** 面板名，不是 Hier API。
