# Collinear baseline 2026-07-15 (P1)

- note: P1: Classify buckets; exact/tight=NeedsSeparation; lint Unrelated=NonSemanticTrunk exact only
- perf_runs: 3
- samples: 10

| file | nodes | edges | exact_sev | tight_sev | allowed_share | exact_pairs | unrelated_trunk | lint_err | median_ms | det | node_fp |
|------|------:|------:|----------:|----------:|--------------:|------------:|----------------:|---------:|----------:|:---:|---------|
| `c.layout-stress-nested.pgm` | 11 | 13 | 269.0 | 1856.0 | 0.0 | 1 | 1 | 0 | 5.73 | True | `e5b3507ff375` |
| `c.cloud-native.pgm` | 12 | 14 | 0.0 | 892.0 | 0.0 | 0 | 0 | 0 | 15.47 | True | `47c35a261bbf` |
| `c.k8s-multi-cluster-federation.pgm` | 21 | 35 | 4501.0 | 9113.3 | 1773.4 | 12 | 12 | 7 | 105.73 | True | `4ed8cdb2036d` |
| `c.k8s-multi-namespace-overview.pgm` | 26 | 37 | 30114.1 | 29485.5 | 12492.0 | 38 | 38 | 13 | 99.53 | True | `c0279d13cc9f` |
| `c.ecommerce-platform.pgm` | 18 | 19 | 583.0 | 1281.0 | 1662.9 | 1 | 1 | 1 | 34.83 | True | `a58baf93a9a2` |
| `c.hybrid-cloud-dr-topology.pgm` | 17 | 24 | 1810.0 | 4971.6 | 1029.3 | 4 | 4 | 5 | 34.4 | True | `a19899014381` |
| `c.layout-stress-dag.pgm` | 11 | 18 | 238.0 | 82.4 | 1.7 | 1 | 0 | 0 | 4.71 | True | `ca3f4411f682` |
| `c.aml-case-investigation.pgm` | 13 | 15 | 0.0 | 550.0 | 0.0 | 0 | 0 | 0 | 2.41 | True | `d7d20ebcbc3b` |
| `c.k8s-tenant-isolation.pgm` | 19 | 26 | 303.3 | 8608.9 | 242.7 | 1 | 1 | 14 | 62.0 | True | `866a0a1846af` |
| `c.k8s-platform-stack.pgm` | 23 | 30 | 1297.1 | 3320.5 | 30.7 | 4 | 4 | 10 | 67.18 | True | `e592859c027e` |

复跑: `./benchmark-data/snapshot-collinear.sh`
对比: `./benchmark-data/compare-collinear.sh <baseline.json> <current.json>`
