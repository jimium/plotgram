# Gate baseline 2026-07-25-215707-phase5-rank-route-demand

- note: role-aware baseline: product-gate hard; stress/demo quality soft
- perf_runs: 5
- samples: 38
  - product: 30
  - stress: 8

| role | file | nodes | edges | exact_sev | tight_sev | exact_pairs | unrelated_trunk | lint_err | median_ms | det | node_fp |
|------|------|------:|------:|----------:|----------:|------------:|----------------:|---------:|----------:|:---:|---------|
| product | `product.three-tier.taut` | 3 | 4 | 76.6 | 0.0 | 1 | 0 | 0 | 1.03 | True | `409c160929da` |
| product | `product.typical-microservice-architecture.taut` | 8 | 16 | 149.6 | 3058.4 | 1 | 1 | 0 | 26.97 | True | `176213891ff4` |
| product | `product.flat-rest-api.taut` | 7 | 7 | 0.0 | 0.0 | 0 | 0 | 0 | 4.65 | True | `00629dd37273` |
| product | `product.microservices.taut` | 7 | 8 | 0.0 | 0.0 | 0 | 0 | 0 | 8.55 | True | `7292e26c585c` |
| product | `product.cdn-cache.taut` | 7 | 9 | 0.0 | 0.0 | 0 | 0 | 1 | 15.04 | True | `1ec91c438a92` |
| product | `product.cloud-native.taut` | 12 | 14 | 0.0 | 8477.9 | 0 | 0 | 3 | 107.28 | True | `32bfcc3a43e3` |
| product | `product.ecommerce-platform.taut` | 18 | 19 | 0.0 | 0.0 | 0 | 0 | 4 | 188.87 | True | `c6512c065cb3` |
| product | `product.message-queue-pipeline.taut` | 7 | 7 | 0.0 | 0.0 | 0 | 0 | 0 | 6.92 | True | `cff21f468242` |
| product | `product.monitoring-stack.taut` | 7 | 6 | 0.0 | 0.0 | 0 | 0 | 0 | 2.35 | True | `3431fe9e6c02` |
| product | `product.narrow-corridor.taut` | 6 | 5 | 0.0 | 304.0 | 0 | 0 | 0 | 1.88 | True | `727efe912725` |
| product | `product.linear-chain.taut` | 3 | 2 | 0.0 | 0.0 | 0 | 0 | 0 | 0.19 | True | `5f5d07b14332` |
| product | `product.user-auth.taut` | 5 | 8 | 0.0 | 0.0 | 0 | 0 | 0 | 4.13 | True | `2ff500c15fd3` |
| product | `product.refund-process.taut` | 10 | 11 | 0.0 | 0.0 | 0 | 0 | 0 | 6.2 | True | `2cf7f7dcbfeb` |
| product | `product.swimlane-order-process.taut` | 8 | 7 | 0.0 | 0.0 | 0 | 0 | 0 | 4.07 | True | `d76d07c0c203` |
| product | `product.password-reset.taut` | 10 | 11 | 0.0 | 0.0 | 0 | 0 | 0 | 6.1 | True | `5f551d70df89` |
| product | `product.leave-approval-process.taut` | 10 | 11 | 0.0 | 0.0 | 0 | 0 | 0 | 5.88 | True | `de694443efab` |
| product | `product.self-loop-retry.taut` | 7 | 8 | 0.0 | 0.0 | 0 | 0 | 0 | 1.84 | True | `9b057fac6aa8` |
| product | `product.symmetric-fanout.taut` | 6 | 7 | 0.0 | 0.0 | 0 | 0 | 0 | 2.22 | True | `e619983b061a` |
| product | `product.oauth-login.taut` | 4 | 8 | 0.0 | 0.0 | 0 | 0 | 0 | 0.09 | True | `4755c2fe5499` |
| product | `product.payment-gateway.taut` | 5 | 11 | 0.0 | 0.0 | 0 | 0 | 0 | 0.15 | True | `7a84ab95e0db` |
| product | `product.sms-verification.taut` | 5 | 11 | 0.0 | 0.0 | 0 | 0 | 2 | 0.15 | True | `fbfce343e7cb` |
| product | `product.jwt-refresh-token.taut` | 5 | 10 | 0.0 | 0.0 | 0 | 0 | 2 | 0.13 | True | `ac27d97f63bf` |
| product | `product.order-lifecycle.taut` | 8 | 8 | 0.0 | 0.0 | 0 | 0 | 0 | 4.04 | True | `779812fa7d44` |
| product | `product.user-session.taut` | 7 | 9 | 0.0 | 0.0 | 0 | 0 | 0 | 4.16 | True | `e36465488243` |
| product | `product.payment-flow.taut` | 10 | 12 | 0.0 | 0.0 | 0 | 0 | 0 | 6.52 | True | `aef8b19629bc` |
| product | `product.delivery-tracking.taut` | 7 | 7 | 0.0 | 0.0 | 0 | 0 | 0 | 3.0 | True | `9602a444d852` |
| product | `product.blog-schema.taut` | 5 | 5 | 0.0 | 0.0 | 0 | 0 | 0 | 0.49 | True | `8b27957af749` |
| product | `product.saas-schema.taut` | 9 | 10 | 0.0 | 0.0 | 0 | 0 | 1 | 8.77 | True | `339840f11b0e` |
| product | `product.crm-schema.taut` | 5 | 5 | 0.0 | 0.0 | 0 | 0 | 0 | 0.46 | True | `b0c07d7b30ef` |
| product | `product.tech-stack.taut` | 10 | 9 | 0.0 | 0.0 | 0 | 0 | 0 | 0.19 | True | `0ca6f63c0463` |
| stress | `stress.layout-stress-nested.taut` | 11 | 13 | 0.0 | 1152.2 | 0 | 0 | 0 | 54.49 | True | `cd410a5f8764` |
| stress | `stress.layout-stress-flat-mesh.taut` | 11 | 20 | 545.0 | 0.0 | 7 | 7 | 0 | 41.4 | True | `eafcc7491954` |
| stress | `stress.layout-stress-dense.taut` | 7 | 16 | 0.0 | 0.0 | 0 | 0 | 2 | 8.24 | True | `aad86723b619` |
| stress | `stress.layout-stress-dag.taut` | 11 | 18 | 0.0 | 10.0 | 0 | 0 | 0 | 11.65 | True | `b76aa0c4049b` |
| stress | `stress.layout-stress-yfiles-pipeline.taut` | 23 | 31 | 0.0 | 212.3 | 0 | 0 | 0 | 137.25 | True | `7dd4071ba827` |
| stress | `stress.layout-stress-lifelines.taut` | 5 | 14 | 0.0 | 0.0 | 0 | 0 | 2 | 0.27 | True | `fe2fb3af3190` |
| stress | `stress.layout-stress-transitions.taut` | 9 | 14 | 0.0 | 0.0 | 0 | 0 | 0 | 5.51 | True | `37c3dd25075f` |
| stress | `stress.layout-stress-deep.taut` | 63 | 62 | 0.0 | 0.0 | 0 | 0 | 0 | 1.25 | True | `5ff5efa1c4ca` |

复跑: `./benchmarks/snapshot.sh`
对比: `./benchmarks/compare.sh <baseline.json> <current.json>`
