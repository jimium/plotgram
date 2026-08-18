# Gate baseline 2026-07-23-120022-a5-side-approach-guard

- note: role-aware baseline: product-gate hard; stress/demo quality soft
- perf_runs: 3
- samples: 37
  - product: 30
  - stress: 7

| role | file | nodes | edges | exact_sev | tight_sev | exact_pairs | unrelated_trunk | lint_err | median_ms | det | node_fp |
|------|------|------:|------:|----------:|----------:|------------:|----------------:|---------:|----------:|:---:|---------|
| product | `product.three-tier.taut` | 3 | 4 | 0.0 | 0.0 | 0 | 0 | 0 | 0.9 | True | `3e259f98e91b` |
| product | `product.typical-microservice-architecture.taut` | 8 | 16 | 0.0 | 2289.4 | 0 | 0 | 0 | 10.14 | True | `4a4f75c01284` |
| product | `product.flat-rest-api.taut` | 7 | 7 | 0.0 | 0.0 | 0 | 0 | 0 | 0.94 | True | `d4e78ac57d33` |
| product | `product.microservices.taut` | 7 | 8 | 0.0 | 0.0 | 0 | 0 | 0 | 1.6 | True | `c9677e8dcc89` |
| product | `product.cdn-cache.taut` | 7 | 9 | 26.0 | 0.0 | 1 | 1 | 3 | 3.95 | True | `07c6f49a65d4` |
| product | `product.cloud-native.taut` | 12 | 14 | 0.0 | 5333.4 | 0 | 0 | 0 | 15.03 | True | `9f65c150ca8d` |
| product | `product.ecommerce-platform.taut` | 18 | 19 | 0.0 | 2412.0 | 0 | 0 | 0 | 33.16 | True | `7e475eead0e0` |
| product | `product.message-queue-pipeline.taut` | 7 | 7 | 0.0 | 0.0 | 0 | 0 | 0 | 1.48 | True | `d18fb9b4a109` |
| product | `product.monitoring-stack.taut` | 7 | 6 | 0.0 | 0.0 | 0 | 0 | 0 | 0.88 | True | `ae58ba68efb9` |
| product | `product.narrow-corridor.taut` | 6 | 5 | 0.0 | 924.0 | 0 | 0 | 0 | 1.32 | True | `40ceea55a4fa` |
| product | `product.linear-chain.taut` | 3 | 2 | 0.0 | 0.0 | 0 | 0 | 0 | 0.2 | True | `5f5d07b14332` |
| product | `product.user-auth.taut` | 5 | 8 | 0.0 | 52.0 | 0 | 0 | 2 | 2.27 | True | `dbd04803f810` |
| product | `product.refund-process.taut` | 10 | 11 | 8.0 | 0.0 | 1 | 0 | 0 | 4.09 | True | `59ef4634e5e8` |
| product | `product.swimlane-order-process.taut` | 8 | 7 | 0.0 | 0.0 | 0 | 0 | 0 | 0.79 | True | `66779b4cdad7` |
| product | `product.password-reset.taut` | 10 | 11 | 11.0 | 0.0 | 1 | 0 | 1 | 3.01 | True | `5255a3b5f303` |
| product | `product.leave-approval-process.taut` | 10 | 11 | 96.0 | 0.0 | 2 | 0 | 0 | 2.32 | True | `9d18947527bd` |
| product | `product.self-loop-retry.taut` | 7 | 8 | 0.0 | 0.0 | 0 | 0 | 0 | 0.8 | True | `e2b6a467c43d` |
| product | `product.symmetric-fanout.taut` | 6 | 7 | 0.0 | 0.0 | 0 | 0 | 0 | 0.92 | True | `f04c96299ba9` |
| product | `product.oauth-login.taut` | 4 | 8 | 0.0 | 0.0 | 0 | 0 | 0 | 0.1 | True | `4755c2fe5499` |
| product | `product.payment-gateway.taut` | 5 | 11 | 0.0 | 0.0 | 0 | 0 | 0 | 0.15 | True | `7a84ab95e0db` |
| product | `product.sms-verification.taut` | 5 | 11 | 0.0 | 0.0 | 0 | 0 | 0 | 0.15 | True | `fbfce343e7cb` |
| product | `product.jwt-refresh-token.taut` | 5 | 10 | 0.0 | 0.0 | 0 | 0 | 0 | 0.13 | True | `ac27d97f63bf` |
| product | `product.order-lifecycle.taut` | 8 | 8 | 0.0 | 0.0 | 0 | 0 | 0 | 0.95 | True | `cc7e1d0b40de` |
| product | `product.user-session.taut` | 7 | 9 | 0.0 | 0.0 | 0 | 0 | 0 | 2.4 | True | `693382c52243` |
| product | `product.payment-flow.taut` | 10 | 12 | 0.0 | 0.0 | 0 | 0 | 0 | 2.51 | True | `f450344eba6c` |
| product | `product.delivery-tracking.taut` | 7 | 7 | 0.0 | 0.0 | 0 | 0 | 0 | 0.7 | True | `5f0c91bb9cd4` |
| product | `product.blog-schema.taut` | 5 | 5 | 0.0 | 0.0 | 0 | 0 | 0 | 0.41 | True | `2697cc2801e8` |
| product | `product.saas-schema.taut` | 9 | 10 | 0.0 | 0.0 | 0 | 0 | 2 | 6.75 | True | `d5b707a7ffd5` |
| product | `product.crm-schema.taut` | 5 | 5 | 0.0 | 0.0 | 0 | 0 | 0 | 0.37 | True | `48dc1acfbfd1` |
| product | `product.tech-stack.taut` | 10 | 9 | 0.0 | 0.0 | 0 | 0 | 0 | 0.1 | True | `0ca6f63c0463` |
| stress | `stress.layout-stress-nested.taut` | 11 | 13 | 0.0 | 0.0 | 0 | 0 | 1 | 5.56 | True | `9a6a0e378a24` |
| stress | `stress.layout-stress-flat-mesh.taut` | 11 | 20 | 160.3 | 5283.9 | 3 | 1 | 0 | 17.94 | True | `d15044b37048` |
| stress | `stress.layout-stress-dense.taut` | 7 | 16 | 0.0 | 0.0 | 0 | 0 | 1 | 7.12 | True | `b0ebcb2adf99` |
| stress | `stress.layout-stress-dag.taut` | 11 | 18 | 112.0 | 45.6 | 1 | 0 | 0 | 5.67 | True | `551853faab1d` |
| stress | `stress.layout-stress-lifelines.taut` | 5 | 14 | 0.0 | 0.0 | 0 | 0 | 0 | 0.3 | True | `fe2fb3af3190` |
| stress | `stress.layout-stress-transitions.taut` | 9 | 14 | 40.0 | 0.0 | 1 | 0 | 0 | 2.96 | True | `eca06cfcc91b` |
| stress | `stress.layout-stress-deep.taut` | 63 | 62 | 0.0 | 0.0 | 0 | 0 | 0 | 1.13 | True | `ab55aa0d8694` |

复跑: `./benchmarks/snapshot.sh`
对比: `./benchmarks/compare.sh <baseline.json> <current.json>`
