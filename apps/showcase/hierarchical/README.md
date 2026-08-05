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

| 文件 | `auto_edge_grouping` | `bus_routing` | 观感 |
|------|----------------------|---------------|------|
| [`auto_edge_grouping.pgm`](fan/auto_edge_grouping.pgm) | ✓ | ✗ | **有**端总线（Trunk/Bus/Stub） |
| [`demo.bus_and_grouping.pgm`](fan/demo.bus_and_grouping.pgm) | ✓ | ✓ | 可见效果来自 grouping；corridor 检测通常仍空 |
| [`demo.bus_routing.pgm`](fan/demo.bus_routing.pgm) | ✗ | ✓ | **当前无差分**（后缀长 &lt; 2，无 SharedCorridor） |
| [`demo.bus_routing_off.pgm`](fan/demo.bus_routing_off.pgm) | ✗ | ✗ | 与上一文件像素对照用 |
| [`critical.pgm`](fan/critical.pgm) | ✗ | ✗ | critical 边权重 |

`bus_routing` 要写出 `SharedCorridor`，需要 ≥2 条 Channel 路径共享 **长度 ≥ 2** 的 track 后缀；左右分列长边会走不同 Main，往往只共享末段 Cross（长 1）→ 检测不触发。拥塞搜索还会主动分散 Main，进一步降低共后缀概率。
