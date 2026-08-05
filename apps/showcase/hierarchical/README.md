# hierarchical 样例

按结构能力分桶（见上级 [README](../README.md) 三层模型）：

| 目录 | 含义 |
|------|------|
| [`flat/`](flat/) | 无 group、无 PartitionGrid |
| [`group/`](group/) | 有 group；含用 group 表达分区的旧「泳道」样例 |
| [`fan/`](fan/) | 扇出 / 合流参数演示（`auto_edge_grouping`、`bus_routing`、`critical`） |
| [`styles/`](styles/) | `routing_style` 变体 |
| [`partition/`](partition/) | 真 PartitionGrid 占位（可空，等 M5） |

角色前缀仍为 `smoke.` / `product.` / `demo.` / `stress.` / `mech.`。

### `fan/` 开关对照

| 文件 | `auto_edge_grouping` | `bus_routing` |
|------|----------------------|---------------|
| [`auto_edge_grouping.pgm`](fan/auto_edge_grouping.pgm) | ✓ | ✗ |
| [`demo.bus_routing.pgm`](fan/demo.bus_routing.pgm) | ✗ | ✓ |
| [`demo.bus_routing_off.pgm`](fan/demo.bus_routing_off.pgm) | ✗ | ✗（对照） |
| [`demo.bus_and_grouping.pgm`](fan/demo.bus_and_grouping.pgm) | ✓ | ✓ |
| [`critical.pgm`](fan/critical.pgm) | ✗ | ✗（critical 边） |
