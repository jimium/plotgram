# Gate baseline 2026-07-25-233904-group-first-class

- note: raise product: G4–G6 group-first-class exit — G-pre plain container; G0–G3 write ratchet; G4 RouteDemand→H-G3, orthosketch gutters-only, delete post_route expand (main=2: compute+canvas); G5 CoordinateProblem::build (prod inline=0); G6 DefaultScorer LexCost + penalty items≤13; debt: writes→1, shell-overflow→degraded, swimlane/table DSL.
- perf_runs: 1
- samples: 38
  - product: 30
  - stress: 8

| role | file | nodes | edges | exact_sev | tight_sev | exact_pairs | unrelated_trunk | lint_err | median_ms | det | node_fp |
|------|------|------:|------:|----------:|----------:|------------:|----------------:|---------:|----------:|:---:|---------|
| product | `product.three-tier.pgm` | 3 | 4 | 76.6 | 0.0 | 1 | 0 | 0 | 0.57 | True | `409c160929da` |
| product | `product.typical-microservice-architecture.pgm` | 8 | 16 | 149.6 | 3058.4 | 1 | 1 | 0 | 26.76 | True | `176213891ff4` |
| product | `product.flat-rest-api.pgm` | 7 | 7 | 0.0 | 0.0 | 0 | 0 | 0 | 4.66 | True | `00629dd37273` |
| product | `product.microservices.pgm` | 7 | 8 | 0.0 | 0.0 | 0 | 0 | 0 | 6.36 | True | `ccf03d9714fc` |
| product | `product.cdn-cache.pgm` | 7 | 9 | 0.0 | 144.0 | 0 | 0 | 0 | 5.15 | True | `5705a60a274f` |
| product | `product.cloud-native.pgm` | 12 | 14 | 305.6 | 356.1 | 6 | 2 | 2 | 93.27 | True | `799f7c05bb8a` |
| product | `product.ecommerce-platform.pgm` | 18 | 19 | 548.7 | 1083.0 | 3 | 3 | 3 | 246.15 | True | `12ecc4bad0f8` |
| product | `product.message-queue-pipeline.pgm` | 7 | 7 | 0.0 | 0.0 | 0 | 0 | 0 | 7.98 | True | `2dfc2e5f6787` |
| product | `product.monitoring-stack.pgm` | 7 | 6 | 0.0 | 0.0 | 0 | 0 | 0 | 8.55 | True | `5996029b9a0c` |
| product | `product.narrow-corridor.pgm` | 6 | 5 | 0.0 | 0.0 | 0 | 0 | 0 | 9.87 | True | `1989fe56a517` |
| product | `product.linear-chain.pgm` | 3 | 2 | 0.0 | 0.0 | 0 | 0 | 0 | 0.23 | True | `5f5d07b14332` |
| product | `product.user-auth.pgm` | 5 | 8 | 0.0 | 0.0 | 0 | 0 | 0 | 4.12 | True | `2ff500c15fd3` |
| product | `product.refund-process.pgm` | 10 | 11 | 0.0 | 0.0 | 0 | 0 | 0 | 6.97 | True | `2cf7f7dcbfeb` |
| product | `product.swimlane-order-process.pgm` | 8 | 7 | 0.0 | 0.0 | 0 | 0 | 0 | 7.84 | True | `e59fd9a52386` |
| product | `product.password-reset.pgm` | 10 | 11 | 0.0 | 0.0 | 0 | 0 | 0 | 6.3 | True | `5f551d70df89` |
| product | `product.leave-approval-process.pgm` | 10 | 11 | 0.0 | 0.0 | 0 | 0 | 0 | 6.28 | True | `de694443efab` |
| product | `product.self-loop-retry.pgm` | 7 | 8 | 0.0 | 0.0 | 0 | 0 | 0 | 1.88 | True | `9b057fac6aa8` |
| product | `product.symmetric-fanout.pgm` | 6 | 7 | 0.0 | 0.0 | 0 | 0 | 0 | 2.3 | True | `e619983b061a` |
| product | `product.oauth-login.pgm` | 4 | 8 | 0.0 | 0.0 | 0 | 0 | 0 | 0.12 | True | `4755c2fe5499` |
| product | `product.payment-gateway.pgm` | 5 | 11 | 0.0 | 0.0 | 0 | 0 | 0 | 0.17 | True | `7a84ab95e0db` |
| product | `product.sms-verification.pgm` | 5 | 11 | 0.0 | 0.0 | 0 | 0 | 2 | 0.16 | True | `fbfce343e7cb` |
| product | `product.jwt-refresh-token.pgm` | 5 | 10 | 0.0 | 0.0 | 0 | 0 | 2 | 0.15 | True | `ac27d97f63bf` |
| product | `product.order-lifecycle.pgm` | 8 | 8 | 0.0 | 0.0 | 0 | 0 | 0 | 4.16 | True | `779812fa7d44` |
| product | `product.user-session.pgm` | 7 | 9 | 0.0 | 0.0 | 0 | 0 | 0 | 4.14 | True | `e36465488243` |
| product | `product.payment-flow.pgm` | 10 | 12 | 0.0 | 0.0 | 0 | 0 | 0 | 6.58 | True | `aef8b19629bc` |
| product | `product.delivery-tracking.pgm` | 7 | 7 | 0.0 | 0.0 | 0 | 0 | 0 | 2.92 | True | `9602a444d852` |
| product | `product.blog-schema.pgm` | 5 | 5 | 0.0 | 0.0 | 0 | 0 | 0 | 0.52 | True | `8b27957af749` |
| product | `product.saas-schema.pgm` | 9 | 10 | 0.0 | 0.0 | 0 | 0 | 1 | 9.01 | True | `339840f11b0e` |
| product | `product.crm-schema.pgm` | 5 | 5 | 0.0 | 0.0 | 0 | 0 | 0 | 0.53 | True | `b0c07d7b30ef` |
| product | `product.tech-stack.pgm` | 10 | 9 | 0.0 | 0.0 | 0 | 0 | 0 | 0.19 | True | `0ca6f63c0463` |
| stress | `stress.layout-stress-nested.pgm` | 11 | 13 | 63.0 | 1248.0 | 1 | 1 | 0 | 73.38 | True | `2239a057036b` |
| stress | `stress.layout-stress-flat-mesh.pgm` | 11 | 20 | 545.0 | 0.0 | 7 | 7 | 0 | 41.78 | True | `eafcc7491954` |
| stress | `stress.layout-stress-dense.pgm` | 7 | 16 | 0.0 | 0.0 | 0 | 0 | 2 | 7.99 | True | `aad86723b619` |
| stress | `stress.layout-stress-dag.pgm` | 11 | 18 | 0.0 | 10.0 | 0 | 0 | 0 | 11.3 | True | `b76aa0c4049b` |
| stress | `stress.layout-stress-yfiles-pipeline.pgm` | 23 | 31 | 0.0 | 212.3 | 0 | 0 | 0 | 136.07 | True | `7dd4071ba827` |
| stress | `stress.layout-stress-lifelines.pgm` | 5 | 14 | 0.0 | 0.0 | 0 | 0 | 2 | 0.34 | True | `fe2fb3af3190` |
| stress | `stress.layout-stress-transitions.pgm` | 9 | 14 | 0.0 | 0.0 | 0 | 0 | 0 | 5.64 | True | `37c3dd25075f` |
| stress | `stress.layout-stress-deep.pgm` | 63 | 62 | 0.0 | 0.0 | 0 | 0 | 0 | 1.27 | True | `5ff5efa1c4ca` |

复跑: `./benchmarks/snapshot.sh`
对比: `./benchmarks/compare.sh <baseline.json> <current.json>`
