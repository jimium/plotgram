# Gate baseline 2026-07-23-220326-b-family-closest-point-fix

- note: role-aware baseline: product-gate hard; stress/demo quality soft
- perf_runs: 5
- samples: 37
  - product: 30
  - stress: 7

| role | file | nodes | edges | exact_sev | tight_sev | exact_pairs | unrelated_trunk | lint_err | median_ms | det | node_fp |
|------|------|------:|------:|----------:|----------:|------------:|----------------:|---------:|----------:|:---:|---------|
| product | `product.three-tier.taut` | 3 | 4 | 0.0 | 0.0 | 0 | 0 | 0 | 0.3 | True | `3e259f98e91b` |
| product | `product.typical-microservice-architecture.taut` | 8 | 16 | 202.5 | 1245.2 | 1 | 1 | 0 | 13.45 | True | `61eadb19ff8e` |
| product | `product.flat-rest-api.taut` | 7 | 7 | 0.0 | 0.0 | 0 | 0 | 0 | 0.95 | True | `fa9ba0a40c68` |
| product | `product.microservices.taut` | 7 | 8 | 0.0 | 0.0 | 0 | 0 | 0 | 1.54 | True | `55bbadacaf81` |
| product | `product.cdn-cache.taut` | 7 | 9 | 0.0 | 0.0 | 0 | 0 | 2 | 4.45 | True | `5ed0180f6f68` |
| product | `product.cloud-native.taut` | 12 | 14 | 0.0 | 5242.7 | 0 | 0 | 0 | 16.14 | True | `e1deca206468` |
| product | `product.ecommerce-platform.taut` | 18 | 19 | 0.0 | 2412.0 | 0 | 0 | 0 | 37.87 | True | `e07104baea55` |
| product | `product.message-queue-pipeline.taut` | 7 | 7 | 0.0 | 0.0 | 0 | 0 | 0 | 1.41 | True | `d18fb9b4a109` |
| product | `product.monitoring-stack.taut` | 7 | 6 | 0.0 | 0.0 | 0 | 0 | 0 | 0.95 | True | `4641e245c6f9` |
| product | `product.narrow-corridor.taut` | 6 | 5 | 0.0 | 924.0 | 0 | 0 | 0 | 1.2 | True | `40ceea55a4fa` |
| product | `product.linear-chain.taut` | 3 | 2 | 0.0 | 0.0 | 0 | 0 | 0 | 0.16 | True | `5f5d07b14332` |
| product | `product.user-auth.taut` | 5 | 8 | 0.0 | 52.0 | 0 | 0 | 1 | 2.76 | True | `1fea184dc3bd` |
| product | `product.refund-process.taut` | 10 | 11 | 0.0 | 718.8 | 0 | 0 | 0 | 5.06 | True | `13d9f41f547e` |
| product | `product.swimlane-order-process.taut` | 8 | 7 | 0.0 | 0.0 | 0 | 0 | 0 | 0.95 | True | `66779b4cdad7` |
| product | `product.password-reset.taut` | 10 | 11 | 11.0 | 0.0 | 1 | 0 | 0 | 3.27 | True | `15e0a7832fdd` |
| product | `product.leave-approval-process.taut` | 10 | 11 | 96.0 | 0.0 | 2 | 0 | 1 | 2.78 | True | `ee22b6cb8427` |
| product | `product.self-loop-retry.taut` | 7 | 8 | 0.0 | 80.0 | 0 | 0 | 0 | 1.14 | True | `e2b6a467c43d` |
| product | `product.symmetric-fanout.taut` | 6 | 7 | 0.0 | 0.0 | 0 | 0 | 1 | 0.92 | True | `f04c96299ba9` |
| product | `product.oauth-login.taut` | 4 | 8 | 0.0 | 0.0 | 0 | 0 | 0 | 0.09 | True | `4755c2fe5499` |
| product | `product.payment-gateway.taut` | 5 | 11 | 0.0 | 0.0 | 0 | 0 | 0 | 0.15 | True | `7a84ab95e0db` |
| product | `product.sms-verification.taut` | 5 | 11 | 0.0 | 0.0 | 0 | 0 | 2 | 0.15 | True | `fbfce343e7cb` |
| product | `product.jwt-refresh-token.taut` | 5 | 10 | 0.0 | 0.0 | 0 | 0 | 2 | 0.15 | True | `ac27d97f63bf` |
| product | `product.order-lifecycle.taut` | 8 | 8 | 0.0 | 0.0 | 0 | 0 | 0 | 1.77 | True | `cc7e1d0b40de` |
| product | `product.user-session.taut` | 7 | 9 | 0.0 | 0.0 | 0 | 0 | 0 | 2.42 | True | `693382c52243` |
| product | `product.payment-flow.taut` | 10 | 12 | 0.0 | 0.0 | 0 | 0 | 0 | 2.65 | True | `f450344eba6c` |
| product | `product.delivery-tracking.taut` | 7 | 7 | 0.0 | 0.0 | 0 | 0 | 0 | 0.79 | True | `5f0c91bb9cd4` |
| product | `product.blog-schema.taut` | 5 | 5 | 0.0 | 0.0 | 0 | 0 | 0 | 0.48 | True | `3381fc011e35` |
| product | `product.saas-schema.taut` | 9 | 10 | 0.0 | 0.0 | 0 | 0 | 2 | 7.08 | True | `d5b707a7ffd5` |
| product | `product.crm-schema.taut` | 5 | 5 | 0.0 | 0.0 | 0 | 0 | 0 | 0.41 | True | `48dc1acfbfd1` |
| product | `product.tech-stack.taut` | 10 | 9 | 0.0 | 0.0 | 0 | 0 | 0 | 0.11 | True | `0ca6f63c0463` |
| stress | `stress.layout-stress-nested.taut` | 11 | 13 | 0.0 | 0.0 | 0 | 0 | 1 | 5.97 | True | `fd0b14b3bd65` |
| stress | `stress.layout-stress-flat-mesh.taut` | 11 | 20 | 222.9 | 3657.5 | 4 | 2 | 0 | 19.88 | True | `d15044b37048` |
| stress | `stress.layout-stress-dense.taut` | 7 | 16 | 0.0 | 0.0 | 0 | 0 | 1 | 7.77 | True | `b0ebcb2adf99` |
| stress | `stress.layout-stress-dag.taut` | 11 | 18 | 112.0 | 66.9 | 1 | 0 | 1 | 8.93 | True | `551853faab1d` |
| stress | `stress.layout-stress-lifelines.taut` | 5 | 14 | 0.0 | 0.0 | 0 | 0 | 2 | 0.28 | True | `fe2fb3af3190` |
| stress | `stress.layout-stress-transitions.taut` | 9 | 14 | 13.7 | 0.0 | 1 | 0 | 1 | 6.0 | True | `eca06cfcc91b` |
| stress | `stress.layout-stress-deep.taut` | 63 | 62 | 0.0 | 0.0 | 0 | 0 | 0 | 1.48 | True | `ab55aa0d8694` |

复跑: `./benchmarks/snapshot.sh`
对比: `./benchmarks/compare.sh <baseline.json> <current.json>`
