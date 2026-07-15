# Collinear baseline 2026-07-15

- note: P1: Classify buckets allowed_share_len; exact/tight = NeedsSeparation only
- perf_runs: 3
- samples: 10

| file | nodes | edges | exact_sev | tight_sev | exact_pairs | unrelated_trunk | lint_err | median_ms | det | node_fp |
|------|------:|------:|----------:|----------:|------------:|----------------:|---------:|----------:|:---:|---------|
| `c.layout-stress-nested.pgm` | 11 | 13 | 269.0 | 1856.0 | 1 | 1 | 0 | 5.31 | True | `e5b3507ff375` |
| `c.cloud-native.pgm` | 12 | 14 | 0.0 | 892.0 | 0 | 0 | 0 | 15.35 | True | `47c35a261bbf` |
| `c.k8s-multi-cluster-federation.pgm` | 21 | 35 | 4501.0 | 9113.3 | 12 | 12 | 7 | 98.93 | True | `4ed8cdb2036d` |
| `c.k8s-multi-namespace-overview.pgm` | 26 | 37 | 30114.1 | 29485.5 | 38 | 38 | 13 | 94.49 | True | `c0279d13cc9f` |
| `c.ecommerce-platform.pgm` | 18 | 19 | 583.0 | 1281.0 | 1 | 1 | 1 | 34.52 | True | `a58baf93a9a2` |
| `c.hybrid-cloud-dr-topology.pgm` | 17 | 24 | 1810.0 | 4971.6 | 4 | 4 | 5 | 32.12 | True | `a19899014381` |
| `c.layout-stress-dag.pgm` | 11 | 18 | 238.0 | 82.4 | 1 | 0 | 0 | 4.42 | True | `ca3f4411f682` |
| `c.aml-case-investigation.pgm` | 13 | 15 | 0.0 | 550.0 | 0 | 0 | 0 | 2.42 | True | `d7d20ebcbc3b` |
| `c.k8s-tenant-isolation.pgm` | 19 | 26 | 303.3 | 8608.9 | 1 | 1 | 14 | 58.53 | True | `866a0a1846af` |
| `c.k8s-platform-stack.pgm` | 23 | 30 | 1297.1 | 3320.5 | 4 | 4 | 10 | 67.01 | True | `e592859c027e` |

复跑: `./benchmark-data/snapshot-collinear.sh`
对比: `./benchmark-data/compare-collinear.sh <baseline.json> <current.json>`
