# Collinear baseline 2026-07-16

- note: 2026-07-16 raise: orthogonal refine no-spline; federation residual edge_through/group known
- perf_runs: 5
- samples: 10

| file | nodes | edges | exact_sev | tight_sev | exact_pairs | unrelated_trunk | lint_err | median_ms | det | node_fp |
|------|------:|------:|----------:|----------:|------------:|----------------:|---------:|----------:|:---:|---------|
| `c.layout-stress-nested.pgm` | 11 | 13 | 324.0 | 0.0 | 1 | 1 | 1 | 6.03 | True | `9b35e2eca507` |
| `c.cloud-native.pgm` | 12 | 14 | 0.0 | 1234.0 | 0 | 0 | 0 | 19.35 | True | `3a98d4ab2c45` |
| `c.k8s-multi-cluster-federation.pgm` | 21 | 35 | 1345.0 | 12995.0 | 6 | 6 | 11 | 30.12 | True | `af36d479d066` |
| `c.k8s-multi-namespace-overview.pgm` | 26 | 37 | 37424.9 | 14237.6 | 41 | 40 | 4 | 117.33 | True | `f339ccaaaaf5` |
| `c.ecommerce-platform.pgm` | 18 | 19 | 565.0 | 2642.1 | 8 | 8 | 0 | 37.24 | True | `08b6a869ba07` |
| `c.hybrid-cloud-dr-topology.pgm` | 17 | 24 | 994.0 | 4575.6 | 2 | 2 | 5 | 24.0 | True | `a8bb3bfae4ba` |
| `c.layout-stress-dag.pgm` | 11 | 18 | 238.0 | 82.4 | 1 | 0 | 0 | 4.6 | True | `ca3f4411f682` |
| `c.aml-case-investigation.pgm` | 13 | 15 | 0.0 | 550.0 | 0 | 0 | 0 | 2.45 | True | `d7d20ebcbc3b` |
| `c.k8s-tenant-isolation.pgm` | 19 | 26 | 0.0 | 6451.6 | 0 | 0 | 14 | 37.98 | True | `70e50c14de73` |
| `c.k8s-platform-stack.pgm` | 23 | 30 | 1337.1 | 9373.7 | 3 | 3 | 13 | 49.66 | True | `5bc23174f549` |

复跑: `./benchmark-data/snapshot-collinear.sh`
对比: `./benchmark-data/compare-collinear.sh <baseline.json> <current.json>`
