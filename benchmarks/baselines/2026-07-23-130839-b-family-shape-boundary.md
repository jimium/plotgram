# Gate baseline 2026-07-23-130839-b-family-shape-boundary

- note: role-aware baseline: product-gate hard; stress/demo quality soft
- perf_runs: 5
- samples: 37
  - product: 30
  - stress: 7

| role | file | nodes | edges | exact_sev | tight_sev | exact_pairs | unrelated_trunk | lint_err | median_ms | det | node_fp |
|------|------|------:|------:|----------:|----------:|------------:|----------------:|---------:|----------:|:---:|---------|
| product | `product.three-tier.taut` | 3 | 4 | 0.0 | 0.0 | 0 | 0 | 0 | 0.32 | True | `3e259f98e91b` |
| product | `product.typical-microservice-architecture.taut` | 8 | 16 | 0.0 | 585.3 | 0 | 0 | 0 | 16.22 | True | `a021056c4312` |
| product | `product.flat-rest-api.taut` | 7 | 7 | 0.0 | 0.0 | 0 | 0 | 0 | 1.03 | True | `d4e78ac57d33` |
| product | `product.microservices.taut` | 7 | 8 | 0.0 | 0.0 | 0 | 0 | 0 | 2.28 | True | `c9677e8dcc89` |
| product | `product.cdn-cache.taut` | 7 | 9 | 0.0 | 361.4 | 0 | 0 | 3 | 4.63 | True | `07c6f49a65d4` |
| product | `product.cloud-native.taut` | 12 | 14 | 0.0 | 6709.4 | 0 | 0 | 0 | 16.04 | True | `9f65c150ca8d` |
| product | `product.ecommerce-platform.taut` | 18 | 19 | 0.0 | 3634.8 | 0 | 0 | 0 | 37.11 | True | `7e475eead0e0` |
| product | `product.message-queue-pipeline.taut` | 7 | 7 | 0.0 | 0.0 | 0 | 0 | 0 | 1.56 | True | `d18fb9b4a109` |
| product | `product.monitoring-stack.taut` | 7 | 6 | 0.0 | 0.0 | 0 | 0 | 0 | 0.88 | True | `ae58ba68efb9` |
| product | `product.narrow-corridor.taut` | 6 | 5 | 0.0 | 924.0 | 0 | 0 | 0 | 1.05 | True | `40ceea55a4fa` |
| product | `product.linear-chain.taut` | 3 | 2 | 0.0 | 0.0 | 0 | 0 | 0 | 0.16 | True | `5f5d07b14332` |
| product | `product.user-auth.taut` | 5 | 8 | 0.0 | 52.0 | 0 | 0 | 2 | 2.05 | True | `dbd04803f810` |
| product | `product.refund-process.taut` | 10 | 11 | 42.6 | 0.0 | 1 | 0 | 0 | 5.57 | True | `db0669feabef` |
| product | `product.swimlane-order-process.taut` | 8 | 7 | 0.0 | 0.0 | 0 | 0 | 0 | 3.15 | True | `f0d13f78e7cf` |
| product | `product.password-reset.taut` | 10 | 11 | 50.0 | 0.0 | 1 | 0 | 0 | 5.59 | True | `f6131319614e` |
| product | `product.leave-approval-process.taut` | 10 | 11 | 64.0 | 0.0 | 1 | 0 | 1 | 6.54 | True | `8c5eabcd8d58` |
| product | `product.self-loop-retry.taut` | 7 | 8 | 0.0 | 0.0 | 0 | 0 | 0 | 0.76 | True | `e2b6a467c43d` |
| product | `product.symmetric-fanout.taut` | 6 | 7 | 50.0 | 0.0 | 1 | 0 | 0 | 0.83 | True | `f04c96299ba9` |
| product | `product.oauth-login.taut` | 4 | 8 | 0.0 | 0.0 | 0 | 0 | 0 | 0.1 | True | `4755c2fe5499` |
| product | `product.payment-gateway.taut` | 5 | 11 | 0.0 | 0.0 | 0 | 0 | 0 | 0.15 | True | `7a84ab95e0db` |
| product | `product.sms-verification.taut` | 5 | 11 | 0.0 | 0.0 | 0 | 0 | 0 | 0.15 | True | `fbfce343e7cb` |
| product | `product.jwt-refresh-token.taut` | 5 | 10 | 0.0 | 0.0 | 0 | 0 | 0 | 0.14 | True | `ac27d97f63bf` |
| product | `product.order-lifecycle.taut` | 8 | 8 | 0.0 | 216.0 | 0 | 0 | 0 | 1.5 | True | `9fd4d91624d8` |
| product | `product.user-session.taut` | 7 | 9 | 0.0 | 0.0 | 0 | 0 | 0 | 2.12 | True | `693382c52243` |
| product | `product.payment-flow.taut` | 10 | 12 | 0.0 | 0.0 | 0 | 0 | 0 | 2.64 | True | `9cb742ac08fb` |
| product | `product.delivery-tracking.taut` | 7 | 7 | 0.0 | 0.0 | 0 | 0 | 0 | 0.65 | True | `ad9326db81e5` |
| product | `product.blog-schema.taut` | 5 | 5 | 0.0 | 0.0 | 0 | 0 | 0 | 0.42 | True | `2697cc2801e8` |
| product | `product.saas-schema.taut` | 9 | 10 | 0.0 | 0.0 | 0 | 0 | 2 | 6.51 | True | `d5b707a7ffd5` |
| product | `product.crm-schema.taut` | 5 | 5 | 0.0 | 0.0 | 0 | 0 | 0 | 0.36 | True | `48dc1acfbfd1` |
| product | `product.tech-stack.taut` | 10 | 9 | 0.0 | 0.0 | 0 | 0 | 0 | 0.1 | True | `0ca6f63c0463` |
| stress | `stress.layout-stress-nested.taut` | 11 | 13 | 0.0 | 1221.3 | 0 | 0 | 0 | 5.87 | True | `9a6a0e378a24` |
| stress | `stress.layout-stress-flat-mesh.taut` | 11 | 20 | 0.0 | 1049.9 | 0 | 0 | 0 | 21.67 | True | `f4a3dcb34a3d` |
| stress | `stress.layout-stress-dense.taut` | 7 | 16 | 0.0 | 0.0 | 0 | 0 | 1 | 7.09 | True | `b0ebcb2adf99` |
| stress | `stress.layout-stress-dag.taut` | 11 | 18 | 0.0 | 45.6 | 0 | 0 | 0 | 9.09 | True | `08dd00ffd794` |
| stress | `stress.layout-stress-lifelines.taut` | 5 | 14 | 0.0 | 0.0 | 0 | 0 | 0 | 0.28 | True | `fe2fb3af3190` |
| stress | `stress.layout-stress-transitions.taut` | 9 | 14 | 0.0 | 0.0 | 0 | 0 | 1 | 9.36 | True | `0f1a0601bfdf` |
| stress | `stress.layout-stress-deep.taut` | 63 | 62 | 0.0 | 0.0 | 0 | 0 | 0 | 1.1 | True | `ab55aa0d8694` |

复跑: `./benchmarks/snapshot.sh`
对比: `./benchmarks/compare.sh <baseline.json> <current.json>`
