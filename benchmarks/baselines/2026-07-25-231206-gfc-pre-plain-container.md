# Gate baseline 2026-07-25-231206-gfc-pre-plain-container

- note: role-aware baseline: product-gate hard; stress/demo quality soft
- perf_runs: 1
- samples: 38
  - product: 30
  - stress: 8

| role | file | nodes | edges | exact_sev | tight_sev | exact_pairs | unrelated_trunk | lint_err | median_ms | det | node_fp |
|------|------|------:|------:|----------:|----------:|------------:|----------------:|---------:|----------:|:---:|---------|
| product | `product.three-tier.pgm` | 3 | 4 | 76.6 | 0.0 | 1 | 0 | 0 | 0.61 | True | `409c160929da` |
| product | `product.typical-microservice-architecture.pgm` | 8 | 16 | 149.6 | 3058.4 | 1 | 1 | 0 | 27.76 | True | `176213891ff4` |
| product | `product.flat-rest-api.pgm` | 7 | 7 | 0.0 | 0.0 | 0 | 0 | 0 | 4.62 | True | `00629dd37273` |
| product | `product.microservices.pgm` | 7 | 8 | 0.0 | 0.0 | 0 | 0 | 0 | 3.17 | True | `d6a7e6f9a72e` |
| product | `product.cdn-cache.pgm` | 7 | 9 | 0.0 | 0.0 | 0 | 0 | 0 | 6.71 | True | `99909912825c` |
| product | `product.cloud-native.pgm` | 12 | 14 | 0.0 | 1368.0 | 0 | 0 | 6 | 103.67 | True | `c29847255b28` |
| product | `product.ecommerce-platform.pgm` | 18 | 19 | 0.0 | 1134.2 | 0 | 0 | 3 | 171.0 | True | `430b8f85588c` |
| product | `product.message-queue-pipeline.pgm` | 7 | 7 | 0.0 | 0.0 | 0 | 0 | 0 | 7.42 | True | `2dfc2e5f6787` |
| product | `product.monitoring-stack.pgm` | 7 | 6 | 0.0 | 0.0 | 0 | 0 | 0 | 8.17 | True | `f89171bb7b37` |
| product | `product.narrow-corridor.pgm` | 6 | 5 | 0.0 | 304.0 | 0 | 0 | 0 | 2.16 | True | `1989fe56a517` |
| product | `product.linear-chain.pgm` | 3 | 2 | 0.0 | 0.0 | 0 | 0 | 0 | 0.23 | True | `5f5d07b14332` |
| product | `product.user-auth.pgm` | 5 | 8 | 0.0 | 0.0 | 0 | 0 | 0 | 4.03 | True | `2ff500c15fd3` |
| product | `product.refund-process.pgm` | 10 | 11 | 0.0 | 0.0 | 0 | 0 | 0 | 6.35 | True | `2cf7f7dcbfeb` |
| product | `product.swimlane-order-process.pgm` | 8 | 7 | 0.0 | 0.0 | 0 | 0 | 0 | 8.09 | True | `e59fd9a52386` |
| product | `product.password-reset.pgm` | 10 | 11 | 0.0 | 0.0 | 0 | 0 | 0 | 6.17 | True | `5f551d70df89` |
| product | `product.leave-approval-process.pgm` | 10 | 11 | 0.0 | 0.0 | 0 | 0 | 0 | 6.3 | True | `de694443efab` |
| product | `product.self-loop-retry.pgm` | 7 | 8 | 0.0 | 0.0 | 0 | 0 | 0 | 1.82 | True | `9b057fac6aa8` |
| product | `product.symmetric-fanout.pgm` | 6 | 7 | 0.0 | 0.0 | 0 | 0 | 0 | 2.5 | True | `e619983b061a` |
| product | `product.oauth-login.pgm` | 4 | 8 | 0.0 | 0.0 | 0 | 0 | 0 | 0.12 | True | `4755c2fe5499` |
| product | `product.payment-gateway.pgm` | 5 | 11 | 0.0 | 0.0 | 0 | 0 | 0 | 0.18 | True | `7a84ab95e0db` |
| product | `product.sms-verification.pgm` | 5 | 11 | 0.0 | 0.0 | 0 | 0 | 2 | 0.17 | True | `fbfce343e7cb` |
| product | `product.jwt-refresh-token.pgm` | 5 | 10 | 0.0 | 0.0 | 0 | 0 | 2 | 0.14 | True | `ac27d97f63bf` |
| product | `product.order-lifecycle.pgm` | 8 | 8 | 0.0 | 0.0 | 0 | 0 | 0 | 4.04 | True | `779812fa7d44` |
| product | `product.user-session.pgm` | 7 | 9 | 0.0 | 0.0 | 0 | 0 | 0 | 4.18 | True | `e36465488243` |
| product | `product.payment-flow.pgm` | 10 | 12 | 0.0 | 0.0 | 0 | 0 | 0 | 6.57 | True | `aef8b19629bc` |
| product | `product.delivery-tracking.pgm` | 7 | 7 | 0.0 | 0.0 | 0 | 0 | 0 | 3.09 | True | `9602a444d852` |
| product | `product.blog-schema.pgm` | 5 | 5 | 0.0 | 0.0 | 0 | 0 | 0 | 0.7 | True | `8b27957af749` |
| product | `product.saas-schema.pgm` | 9 | 10 | 0.0 | 0.0 | 0 | 0 | 1 | 8.83 | True | `339840f11b0e` |
| product | `product.crm-schema.pgm` | 5 | 5 | 0.0 | 0.0 | 0 | 0 | 0 | 0.54 | True | `b0c07d7b30ef` |
| product | `product.tech-stack.pgm` | 10 | 9 | 0.0 | 0.0 | 0 | 0 | 0 | 0.16 | True | `0ca6f63c0463` |
| stress | `stress.layout-stress-nested.pgm` | 11 | 13 | 55.0 | 424.0 | 1 | 1 | 0 | 63.89 | True | `681114312d32` |
| stress | `stress.layout-stress-flat-mesh.pgm` | 11 | 20 | 545.0 | 0.0 | 7 | 7 | 0 | 43.1 | True | `eafcc7491954` |
| stress | `stress.layout-stress-dense.pgm` | 7 | 16 | 0.0 | 0.0 | 0 | 0 | 2 | 8.66 | True | `aad86723b619` |
| stress | `stress.layout-stress-dag.pgm` | 11 | 18 | 0.0 | 10.0 | 0 | 0 | 0 | 11.5 | True | `b76aa0c4049b` |
| stress | `stress.layout-stress-yfiles-pipeline.pgm` | 23 | 31 | 0.0 | 212.3 | 0 | 0 | 0 | 140.37 | True | `7dd4071ba827` |
| stress | `stress.layout-stress-lifelines.pgm` | 5 | 14 | 0.0 | 0.0 | 0 | 0 | 2 | 0.34 | True | `fe2fb3af3190` |
| stress | `stress.layout-stress-transitions.pgm` | 9 | 14 | 0.0 | 0.0 | 0 | 0 | 0 | 5.55 | True | `37c3dd25075f` |
| stress | `stress.layout-stress-deep.pgm` | 63 | 62 | 0.0 | 0.0 | 0 | 0 | 0 | 1.36 | True | `5ff5efa1c4ca` |

复跑: `./benchmarks/snapshot.sh`
对比: `./benchmarks/compare.sh <baseline.json> <current.json>`
