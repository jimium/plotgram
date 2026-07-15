# Collinear baseline 2026-07-15

- note: P1: Classify buckets allowed_share_len; exact/tight = NeedsSeparation only
- perf_runs: 3
- samples: 10

| file | nodes | edges | exact_sev | tight_sev | exact_pairs | unrelated_trunk | lint_err | median_ms | det | node_fp |
|------|------:|------:|----------:|----------:|------------:|----------------:|---------:|----------:|:---:|---------|
| `c.layout-stress-nested.pgm` | 11 | 13 | 269.0 | 1856.0 | 1 | 1 | 0 | 3.22 | True | `e5b3507ff375` |
| `c.cloud-native.pgm` | 12 | 14 | 0.0 | 128.0 | 0 | 0 | 0 | 15.87 | True | `47c35a261bbf` |
| `c.k8s-multi-cluster-federation.pgm` | 21 | 35 | 4513.0 | 8298.4 | 12 | 12 | 7 | 102.44 | True | `4ed8cdb2036d` |
| `c.k8s-multi-namespace-overview.pgm` | 26 | 37 | 29356.4 | 21958.7 | 39 | 38 | 15 | 96.14 | True | `7f66affd3879` |
| `c.ecommerce-platform.pgm` | 18 | 19 | 583.0 | 1169.5 | 1 | 1 | 2 | 42.77 | True | `79cada84c8e7` |
| `c.hybrid-cloud-dr-topology.pgm` | 17 | 24 | 1957.8 | 6434.6 | 5 | 5 | 5 | 33.07 | True | `a19899014381` |
| `c.layout-stress-dag.pgm` | 11 | 18 | 238.0 | 0.0 | 1 | 0 | 0 | 4.73 | True | `ca3f4411f682` |
| `c.aml-case-investigation.pgm` | 13 | 15 | 0.0 | 550.0 | 0 | 0 | 0 | 2.57 | True | `d7d20ebcbc3b` |
| `c.k8s-tenant-isolation.pgm` | 19 | 26 | 303.3 | 10664.1 | 1 | 1 | 14 | 60.17 | True | `866a0a1846af` |
| `c.k8s-platform-stack.pgm` | 23 | 30 | 1266.4 | 3086.3 | 3 | 3 | 10 | 68.71 | True | `b478db7611da` |

复跑: `./benchmark-data/snapshot-collinear.sh`
对比: `./benchmark-data/compare-collinear.sh <baseline.json> <current.json>`
