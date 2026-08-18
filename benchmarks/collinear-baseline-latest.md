# Collinear baseline 2026-07-20

- note: role-aware baseline: product-gate hard; stress/demo quality soft
- perf_runs: 5
- samples: 24
  - product: 18
  - stress: 6

| role | file | nodes | edges | exact_sev | tight_sev | exact_pairs | unrelated_trunk | lint_err | median_ms | det | node_fp |
|------|------|------:|------:|----------:|----------:|------------:|----------------:|---------:|----------:|:---:|---------|
| product | `product.three-tier.taut` | 3 | 4 | 0.0 | 0.0 | 0 | 0 | 0 | 0.27 | True | `3e259f98e91b` |
| product | `product.typical-microservice-architecture.taut` | 8 | 16 | 0.0 | 1048.5 | 0 | 0 | 0 | 9.18 | True | `48226cfc76f3` |
| product | `product.microservices.taut` | 7 | 8 | 0.0 | 0.0 | 0 | 0 | 0 | 1.83 | True | `c9677e8dcc89` |
| product | `product.cdn-cache.taut` | 7 | 9 | 12.0 | 0.0 | 1 | 0 | 3 | 3.97 | True | `07c6f49a65d4` |
| product | `product.cloud-native.taut` | 12 | 14 | 0.0 | 3665.9 | 0 | 0 | 0 | 15.0 | True | `4ab0e916c26b` |
| product | `product.ecommerce-platform.taut` | 18 | 19 | 0.0 | 478.1 | 0 | 0 | 0 | 45.68 | True | `7e475eead0e0` |
| product | `product.linear-chain.taut` | 3 | 2 | 0.0 | 0.0 | 0 | 0 | 0 | 0.14 | True | `5f5d07b14332` |
| product | `product.user-auth.taut` | 5 | 8 | 0.0 | 52.0 | 0 | 0 | 2 | 1.71 | True | `dbd04803f810` |
| product | `product.refund-process.taut` | 10 | 11 | 0.0 | 0.0 | 0 | 0 | 0 | 2.53 | True | `8b2529989567` |
| product | `product.swimlane-order-process.taut` | 8 | 7 | 0.0 | 0.0 | 0 | 0 | 0 | 1.4 | True | `7bb640584f7f` |
| product | `product.oauth-login.taut` | 4 | 8 | 0.0 | 0.0 | 0 | 0 | 0 | 0.1 | True | `4755c2fe5499` |
| product | `product.payment-gateway.taut` | 5 | 11 | 0.0 | 0.0 | 0 | 0 | 0 | 0.42 | True | `7a84ab95e0db` |
| product | `product.order-lifecycle.taut` | 8 | 8 | 0.0 | 0.0 | 0 | 0 | 0 | 1.18 | True | `04d6fc2e79ea` |
| product | `product.user-session.taut` | 7 | 9 | 0.0 | 0.0 | 0 | 0 | 0 | 2.78 | True | `693382c52243` |
| product | `product.payment-flow.taut` | 10 | 12 | 0.0 | 0.0 | 0 | 0 | 0 | 4.25 | True | `632ea0ef6e33` |
| product | `product.blog-schema.taut` | 5 | 5 | 0.0 | 0.0 | 0 | 0 | 0 | 0.55 | True | `2697cc2801e8` |
| product | `product.saas-schema.taut` | 9 | 10 | 0.0 | 0.0 | 0 | 0 | 2 | 7.57 | True | `d5b707a7ffd5` |
| product | `product.tech-stack.taut` | 10 | 9 | 0.0 | 0.0 | 0 | 0 | 0 | 0.54 | True | `0ca6f63c0463` |
| stress | `stress.layout-stress-nested.taut` | 11 | 13 | 0.0 | 861.3 | 0 | 0 | 1 | 8.17 | True | `a05a8e3f7d33` |
| stress | `stress.layout-stress-dense.taut` | 7 | 16 | 0.0 | 0.0 | 0 | 0 | 1 | 7.8 | True | `b0ebcb2adf99` |
| stress | `stress.layout-stress-dag.taut` | 11 | 18 | 238.0 | 82.4 | 1 | 0 | 0 | 4.98 | True | `ca3f4411f682` |
| stress | `stress.layout-stress-lifelines.taut` | 5 | 14 | 0.0 | 0.0 | 0 | 0 | 0 | 0.29 | True | `fe2fb3af3190` |
| stress | `stress.layout-stress-transitions.taut` | 9 | 14 | 56.0 | 0.0 | 2 | 0 | 0 | 3.02 | True | `e09a80637c63` |
| stress | `stress.layout-stress-deep.taut` | 63 | 62 | 0.0 | 0.0 | 0 | 0 | 0 | 82.86 | True | `ab55aa0d8694` |

复跑: `./benchmark-data/snapshot-collinear.sh`
对比: `./benchmark-data/compare-collinear.sh <baseline.json> <current.json>`
