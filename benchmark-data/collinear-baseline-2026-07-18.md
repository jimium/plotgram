# Collinear baseline 2026-07-18

- note: 2026-07-18 raise: L3 architecture D-end exact stub repair + L5.1 through-node dogleg (shape-edit obstacle guard; refine through-only fallback; repair_through_edges_post_route). Correctness: group_interior not up (platform-stack 4→3, tenant 3→1). Quality: through↓ (ecommerce 3→0, federation 6→2); residual through/tight_sev/node_fp debt on some samples.
- perf_runs: 3
- samples: 10

| file | nodes | edges | exact_sev | tight_sev | exact_pairs | unrelated_trunk | lint_err | median_ms | det | node_fp |
|------|------:|------:|----------:|----------:|------------:|----------------:|---------:|----------:|:---:|---------|
| `c.layout-stress-nested.pgm` | 11 | 13 | 0.0 | 673.2 | 0 | 0 | 1 | 4.56 | True | `66f3e9f932be` |
| `c.cloud-native.pgm` | 12 | 14 | 0.0 | 3516.0 | 0 | 0 | 1 | 12.44 | True | `4ab0e916c26b` |
| `c.k8s-multi-cluster-federation.pgm` | 21 | 35 | 1345.0 | 8470.4 | 6 | 6 | 2 | 35.19 | True | `af36d479d066` |
| `c.k8s-multi-namespace-overview.pgm` | 26 | 37 | 506.3 | 7089.1 | 3 | 2 | 5 | 93.36 | True | `cd7e5467e649` |
| `c.ecommerce-platform.pgm` | 18 | 19 | 165.9 | 256.0 | 1 | 1 | 0 | 27.06 | True | `7e475eead0e0` |
| `c.hybrid-cloud-dr-topology.pgm` | 17 | 24 | 625.0 | 6782.5 | 1 | 1 | 3 | 26.18 | True | `5dc96286ef6e` |
| `c.layout-stress-dag.pgm` | 11 | 18 | 238.0 | 82.4 | 1 | 0 | 0 | 4.7 | True | `ca3f4411f682` |
| `c.aml-case-investigation.pgm` | 13 | 15 | 0.0 | 550.0 | 0 | 0 | 0 | 2.51 | True | `d7d20ebcbc3b` |
| `c.k8s-tenant-isolation.pgm` | 19 | 26 | 0.0 | 7782.4 | 0 | 0 | 5 | 45.42 | True | `70e50c14de73` |
| `c.k8s-platform-stack.pgm` | 23 | 30 | 1079.9 | 9862.0 | 4 | 4 | 10 | 56.53 | True | `5bc23174f549` |

复跑: `./benchmark-data/snapshot-collinear.sh`
对比: `./benchmark-data/compare-collinear.sh <baseline.json> <current.json>`
