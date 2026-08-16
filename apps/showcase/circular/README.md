# circular 样例

按 **分区政策**分桶（见上级 [README](../README.md) 与 [circular 架构](../../../docs/design/layout/circular/architecture.md)）。

目录名 = 布局内核注册名 `circular`。径向树仍是 `layout: tree` 的 placer，**禁止**再建 `radial/` 布局目录。

| 目录 | 对应政策 | 状态 |
|------|----------|------|
| [`cycle/`](cycle/) | `partitioning: single-cycle` | **M0**；分量内全体一圈 |
| [`bcc/`](bcc/) | `bcc-compact` / `bcc-isolated` | **M1–M2**；多圈 + 割点桥 |
| [`custom/`](custom/) | 节点 `circle:` | **M3**；作者分区覆盖 BCC |

裸 `layout: circular` 走默认 `bcc-compact` / `spectral`。单环样例须显式 `partitioning: single-cycle`。节点 `circle:` 在默认或 `partitioning: custom` 下生效（DSL 不能写节点键 `partition:`，那是图级 PartitionGrid 保留字）。`automatic` 与 disk 仍 `Unsupported`。

环状拓扑优先谱序；`declaration` 按声明序绕圈。`bfs` 从两端同时展开，密环上弦会穿心。

### `cycle/`

| 文件 | 主题 |
|------|------|
| [`smoke.ring.pgm`](cycle/smoke.ring.pgm) | 六点环（`order: declaration`），点在一圈、相邻不叠 |
| [`smoke.two-triangles.pgm`](cycle/smoke.two-triangles.pgm) | 与 `bcc/smoke.two-rings` 同一拓扑，挤成一圈 |
| [`mech.exterior.pgm`](cycle/mech.exterior.pgm) | 密圈对角走外弧，邻接仍弦 |
| [`mech.self-loop.pgm`](cycle/mech.self-loop.pgm) | 自环短弧在节点外侧 |

### `bcc/`

| 文件 | 主题 |
|------|------|
| [`smoke.two-rings.pgm`](bcc/smoke.two-rings.pgm) | 两三角共割点；两圈 + 桥，不是一个巨圆 |
| [`smoke.isolated-cut.pgm`](bcc/smoke.isolated-cut.pgm) | `bcc-isolated`：割点夹在两圆之间 |
| [`product.two-cliques.pgm`](bcc/product.two-cliques.pgm) | 两团经桥相连；balloon 骨架把分区圆分开 |

### `custom/`

| 文件 | 主题 |
|------|------|
| [`smoke.two-groups.pgm`](custom/smoke.two-groups.pgm) | 6 环拆成 `circle: left` / `right` 两圈 |
