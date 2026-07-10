# Phase 0 回归基线快照

- 日期: 2026-07-10
- 默认行为: architecture `group_frame` = **Fit**（Equal 需显式 `group_sizing: uniform`）— *本快照在 Phase 1 默认改 Equal 之前采集*
- I1–I3 / corridor A1+A2: 已落地（见 layout-routing-optimization-proposal-2026-07.md）
- 样例集: `benchmark-data/phase0-regression-set.txt`
- 复跑: `./benchmark-data/snapshot-phase0.sh`

## Lint 摘要

| 样例 | errors | warnings | edge_through_node | edge_crosses_group_interior | sibling_width_ratio |
|------|-------:|---------:|------------------:|----------------------------:|--------------------:|
| `c.layout-stress-nested` | 0 | 10 | 0 | 0 | 0 |
| `c.cloud-native` | 7 | 6 | 2 | 1 | 1 |
| `c.k8s-multi-cluster-federation` | 12 | 54 | 12 | 0 | 1 |
| `c.k8s-multi-namespace-overview` | 4 | 56 | 4 | 0 | 1 |
| `c.ecommerce-platform` | 1 | 6 | 0 | 0 | 0 |
| `c.hybrid-cloud-dr-topology` | 7 | 30 | 5 | 0 | 0 |
| `c.layout-stress-dag` | 0 | 1 | 0 | 0 | 0 |
| `c.aml-case-investigation` | 0 | 0 | 0 | 0 | 0 |

## bench-phases（布局+路由中位数，3 轮）

| 样例 | nodes | edges | groups | median_ms | min_ms | max_ms |
|------|-------:|------:|-------:|----------:|-------:|-------:|
| `c.layout-stress-nested` | 11 | 13 | 5 | 11.63 | 10.43 | 12.98 |
| `c.cloud-native` | 12 | 14 | 6 | 9.38 | 9.34 | 9.39 |
| `c.k8s-multi-cluster-federation` | 21 | 35 | 5 | 45.22 | 45.07 | 45.27 |
| `c.k8s-multi-namespace-overview` | 26 | 37 | 6 | 102.31 | 101.67 | 102.70 |
| `c.ecommerce-platform` | 18 | 19 | 5 | 24.68 | 24.55 | 24.92 |
| `c.hybrid-cloud-dr-topology` | 17 | 24 | 4 | 29.41 | 28.97 | 29.48 |
| `c.layout-stress-dag` | 11 | 18 | 0 | 5.75 | 5.60 | 5.79 |
| `c.aml-case-investigation` | 13 | 15 | 0 | 3.27 | 3.27 | 3.48 |

## 确定性（连续两次 SVG hash）

| 样例 | hash1 == hash2 |
|------|:--------------:|
| `c.layout-stress-nested` | yes |
| `c.cloud-native` | yes |
| `c.k8s-multi-cluster-federation` | yes |
| `c.k8s-multi-namespace-overview` | yes |
| `c.ecommerce-platform` | yes |
| `c.hybrid-cloud-dr-topology` | yes |
| `c.layout-stress-dag` | yes |
| `c.aml-case-investigation` | yes |

## 说明

- 本快照为后续 Phase 1–5 对照基线；**采集时不改变算法行为**。
- 与历史单图基线 `baseline.md`（tenant-isolation）并存。
- 回链: `docs/architecture/重构方案/render-layout-routing-baseline-2026-07.md`
