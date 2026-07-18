# Collinear baseline 2026-07-18 (group-skirt)

- note: 2026-07-18 raise: aggressive post-freeze group skirt (union bbox + alt ports + corner); refine path unchanged. group_interior: tenant 3→1, multi-ns 2→0, platform 4→3; through also↓. Quality debt: multi-ns/platform tight_sev↑; tenant node_fp change = canvas finalize translate from longer skirts (relative layout at NodeFreeze unchanged).
- perf_runs: None
- samples: 10

| file | nodes | edges | exact_sev | tight_sev | exact_pairs | unrelated_trunk | lint_err | median_ms | det | node_fp |
|------|------:|------:|----------:|----------:|------------:|----------------:|---------:|----------:|:---:|---------|
| `c.layout-stress-nested.pgm` | 11 | 13 | 0.0 | 673.2 | 0 | 0 | 1 | None | None | `66f3e9f932be` |
| `c.cloud-native.pgm` | 12 | 14 | 0.0 | 3516.0 | 0 | 0 | 1 | None | None | `4ab0e916c26b` |
| `c.k8s-multi-cluster-federation.pgm` | 21 | 35 | 1345.0 | 8470.4 | 6 | 6 | 2 | None | None | `af36d479d066` |
| `c.k8s-multi-namespace-overview.pgm` | 26 | 37 | 506.3 | 9806.3 | 3 | 2 | 2 | None | None | `cd7e5467e649` |
| `c.ecommerce-platform.pgm` | 18 | 19 | 0.0 | 256.0 | 0 | 0 | 0 | None | None | `7e475eead0e0` |
| `c.hybrid-cloud-dr-topology.pgm` | 17 | 24 | 625.0 | 9763.1 | 1 | 1 | 2 | None | None | `5dc96286ef6e` |
| `c.layout-stress-dag.pgm` | 11 | 18 | 238.0 | 82.4 | 1 | 0 | 0 | None | None | `ca3f4411f682` |
| `c.aml-case-investigation.pgm` | 13 | 15 | 0.0 | 550.0 | 0 | 0 | 0 | None | None | `d7d20ebcbc3b` |
| `c.k8s-tenant-isolation.pgm` | 19 | 26 | 0.0 | 8071.0 | 0 | 0 | 1 | None | None | `ce79d54f4b90` |
| `c.k8s-platform-stack.pgm` | 23 | 30 | 477.5 | 11919.7 | 3 | 3 | 7 | None | None | `5bc23174f549` |

复跑: `./benchmark-data/snapshot-collinear.sh`
对比: `./benchmark-data/compare-collinear.sh <baseline.json> <current.json>`
