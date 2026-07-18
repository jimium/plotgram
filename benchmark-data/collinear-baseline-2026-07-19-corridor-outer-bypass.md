# Collinear baseline 2026-07-19-corridor-outer-bypass

- note: 2026-07-19 raise: post-freeze corridor stick outer-bypass (solid exit border + buried-lane outer skirt + side-swap; main-chain outer_bypass=false). tenant b_worker→object_store group 1→0 stick=1; platform through 2→0; node_fp unchanged. Quality debt: tenant/ecommerce tight_sev↑ (outer path longer).
- perf_runs: 3
- samples: 10

| file | nodes | edges | exact_sev | tight_sev | exact_pairs | unrelated_trunk | lint_err | median_ms | det | node_fp |
|------|------:|------:|----------:|----------:|------------:|----------------:|---------:|----------:|:---:|---------|
| `c.layout-stress-nested.pgm` | 11 | 13 | 0.0 | 673.2 | 0 | 0 | 1 | 6.66 | True | `66f3e9f932be` |
| `c.cloud-native.pgm` | 12 | 14 | 0.0 | 3516.0 | 0 | 0 | 1 | 13.01 | True | `4ab0e916c26b` |
| `c.k8s-multi-cluster-federation.pgm` | 21 | 35 | 1345.0 | 8470.4 | 6 | 6 | 2 | 37.52 | True | `af36d479d066` |
| `c.k8s-multi-namespace-overview.pgm` | 26 | 37 | 506.3 | 1067.6 | 3 | 2 | 2 | 102.0 | True | `cd7e5467e649` |
| `c.ecommerce-platform.pgm` | 18 | 19 | 0.0 | 478.1 | 0 | 0 | 0 | 27.84 | True | `7e475eead0e0` |
| `c.hybrid-cloud-dr-topology.pgm` | 17 | 24 | 625.0 | 7132.8 | 1 | 1 | 2 | 28.26 | True | `5dc96286ef6e` |
| `c.layout-stress-dag.pgm` | 11 | 18 | 238.0 | 82.4 | 1 | 0 | 0 | 4.74 | True | `ca3f4411f682` |
| `c.aml-case-investigation.pgm` | 13 | 15 | 0.0 | 550.0 | 0 | 0 | 0 | 2.55 | True | `d7d20ebcbc3b` |
| `c.k8s-tenant-isolation.pgm` | 19 | 26 | 0.0 | 8341.1 | 0 | 0 | 0 | 55.0 | True | `ce79d54f4b90` |
| `c.k8s-platform-stack.pgm` | 23 | 30 | 477.5 | 8159.9 | 3 | 3 | 0 | 69.95 | True | `5bc23174f549` |

复跑: `./benchmark-data/snapshot-collinear.sh`
对比: `./benchmark-data/compare-collinear.sh <baseline.json> <current.json>`
