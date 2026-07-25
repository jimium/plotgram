# Phase 3 start 基线盘点（2026-07-25）

承接 Phase 2 LexAStar 切轨后状态。

## LOC（`edge_routing_orthogonal/*.rs`）

合计约 **14 010**（含 `orthogonal_tests.rs` 1529）。主要大块：`path_legacy` 1880、`lane_assignment` 1607、`stub_occupancy` 816、`sanitize` 770、`port_solver` 780、`semantic_trunk_merge` 692。

## 硬约束债（`25-Phase2-硬约束违反清单`）

| 种类 | 次数 |
|---|---:|
| H0EndpointBoundary | 81 |
| H0StubDirection | 69 |
| H1ThroughNode | 34 |
| H2GroupInterior | 1 |
| H3NonOrthogonal | 26 |

正交主债：H1=34。非正交族（sequence/er/mindmap）的 H0/H3 不作为 Phase 3 正交退出主判据。

## 质量基线

- Phase 2 末：`benchmarks/baselines/2026-07-25-201540-phase2-lex-astar.json`
- `latest.json` 已同步该快照
- Phase 3 退出：相对 Phase 0 / 产品可接受基线质量 ±10%；穿组=0、`det=true`
