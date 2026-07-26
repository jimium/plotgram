# Gate baseline 2026-07-27-025317-atlas-final

- note: raise product: Atlas Stage 7 final — Ink/Plan production path; expected node_fp + exact_sev + aesthetics shift vs frozen-routing baseline; residual lint.edge_crosses_group_interior: product.ecommerce-platform, product.swimlane-order-process (Ink geometry; channel L6/L8 construction still 0). raise stress (expected): Ink aesthetics/through-node on stress probes. Gates on; PipelineChoice::Legacy deleted.
- perf_runs: 5
- samples: 38
  - product: 30
  - stress: 8

| role | file | nodes | edges | exact_sev | tight_sev | exact_pairs | unrelated_trunk | lint_err | median_ms | det | node_fp |
|------|------|------:|------:|----------:|----------:|------------:|----------------:|---------:|----------:|:---:|---------|
| product | `product.three-tier.pgm` | 3 | 4 | 148.0 | 0.0 | 2 | 0 | 0 | 0.16 | True | `35a8add211e5` |
| product | `product.typical-microservice-architecture.pgm` | 8 | 16 | 67.8 | 0.0 | 2 | 1 | 4 | 1.17 | True | `e52c26140035` |
| product | `product.flat-rest-api.pgm` | 7 | 7 | 0.0 | 0.0 | 0 | 0 | 0 | 0.41 | True | `a94481ed0435` |
| product | `product.microservices.pgm` | 7 | 8 | 0.0 | 0.0 | 0 | 0 | 3 | 0.36 | True | `87c8656fb4f5` |
| product | `product.cdn-cache.pgm` | 7 | 9 | 116.0 | 0.0 | 1 | 1 | 11 | 0.7 | True | `b470b0d3f0dc` |
| product | `product.cloud-native.pgm` | 12 | 14 | 568.3 | 0.0 | 2 | 2 | 17 | 0.83 | True | `9f1425910192` |
| product | `product.ecommerce-platform.pgm` | 18 | 19 | 157.9 | 0.0 | 5 | 5 | 39 | 1.64 | True | `4dfc5081a575` |
| product | `product.message-queue-pipeline.pgm` | 7 | 7 | 0.0 | 0.0 | 0 | 0 | 11 | 0.5 | True | `a391c850bd41` |
| product | `product.monitoring-stack.pgm` | 7 | 6 | 0.0 | 0.0 | 0 | 0 | 3 | 0.47 | True | `10cbcad0a7a3` |
| product | `product.narrow-corridor.pgm` | 6 | 5 | 0.0 | 0.0 | 0 | 0 | 6 | 0.24 | True | `1989fe56a517` |
| product | `product.linear-chain.pgm` | 3 | 2 | 0.0 | 0.0 | 0 | 0 | 0 | 0.09 | True | `5f5d07b14332` |
| product | `product.user-auth.pgm` | 5 | 8 | 160.0 | 0.0 | 3 | 0 | 0 | 0.39 | True | `7e73d7dc735a` |
| product | `product.refund-process.pgm` | 10 | 11 | 62.0 | 0.0 | 2 | 0 | 0 | 0.79 | True | `06ccf934a754` |
| product | `product.swimlane-order-process.pgm` | 8 | 7 | 0.0 | 0.0 | 0 | 0 | 17 | 0.32 | True | `e59fd9a52386` |
| product | `product.password-reset.pgm` | 10 | 11 | 71.0 | 0.0 | 1 | 0 | 0 | 0.63 | True | `f240b507b08a` |
| product | `product.leave-approval-process.pgm` | 10 | 11 | 84.0 | 0.0 | 2 | 0 | 3 | 0.74 | True | `b701d0078d6b` |
| product | `product.self-loop-retry.pgm` | 7 | 8 | 60.0 | 0.0 | 1 | 0 | 0 | 0.33 | True | `1071d02c6a42` |
| product | `product.symmetric-fanout.pgm` | 6 | 7 | 123.0 | 0.0 | 4 | 0 | 0 | 0.3 | True | `6c0e669c73f9` |
| product | `product.oauth-login.pgm` | 4 | 8 | 0.0 | 0.0 | 0 | 0 | 0 | 0.11 | True | `4755c2fe5499` |
| product | `product.payment-gateway.pgm` | 5 | 11 | 0.0 | 0.0 | 0 | 0 | 0 | 0.17 | True | `7a84ab95e0db` |
| product | `product.sms-verification.pgm` | 5 | 11 | 0.0 | 0.0 | 0 | 0 | 2 | 0.16 | True | `fbfce343e7cb` |
| product | `product.jwt-refresh-token.pgm` | 5 | 10 | 0.0 | 0.0 | 0 | 0 | 2 | 0.17 | True | `ac27d97f63bf` |
| product | `product.order-lifecycle.pgm` | 8 | 8 | 0.0 | 0.0 | 0 | 0 | 0 | 0.57 | True | `90b40941e4b1` |
| product | `product.user-session.pgm` | 7 | 9 | 133.0 | 0.0 | 4 | 0 | 2 | 0.62 | True | `b62ff2faa7fa` |
| product | `product.payment-flow.pgm` | 10 | 12 | 105.0 | 0.0 | 2 | 0 | 3 | 1.17 | True | `ee82a81bbdb1` |
| product | `product.delivery-tracking.pgm` | 7 | 7 | 0.0 | 0.0 | 0 | 0 | 0 | 0.42 | True | `ab55fff8f38f` |
| product | `product.blog-schema.pgm` | 5 | 5 | 0.0 | 0.0 | 0 | 0 | 0 | 1.28 | True | `8b27957af749` |
| product | `product.saas-schema.pgm` | 9 | 10 | 0.0 | 0.0 | 0 | 0 | 0 | 13.2 | True | `339840f11b0e` |
| product | `product.crm-schema.pgm` | 5 | 5 | 0.0 | 0.0 | 0 | 0 | 0 | 0.5 | True | `b0c07d7b30ef` |
| product | `product.tech-stack.pgm` | 10 | 9 | 0.0 | 0.0 | 0 | 0 | 0 | 0.17 | True | `0ca6f63c0463` |
| stress | `stress.layout-stress-nested.pgm` | 11 | 13 | 156.2 | 0.0 | 3 | 3 | 23 | 1.66 | True | `f1115a4d1531` |
| stress | `stress.layout-stress-flat-mesh.pgm` | 11 | 20 | 268.6 | 0.0 | 3 | 3 | 7 | 3.46 | True | `b23fc64c97e8` |
| stress | `stress.layout-stress-dense.pgm` | 7 | 16 | 0.0 | 0.0 | 0 | 0 | 2 | 8.27 | True | `aad86723b619` |
| stress | `stress.layout-stress-dag.pgm` | 11 | 18 | 32.0 | 0.0 | 1 | 0 | 2 | 1.89 | True | `e35ed8be4120` |
| stress | `stress.layout-stress-yfiles-pipeline.pgm` | 23 | 31 | 2555.9 | 0.0 | 14 | 0 | 3 | 9.6 | True | `41e0d77281c0` |
| stress | `stress.layout-stress-lifelines.pgm` | 5 | 14 | 0.0 | 0.0 | 0 | 0 | 2 | 0.29 | True | `fe2fb3af3190` |
| stress | `stress.layout-stress-transitions.pgm` | 9 | 14 | 112.9 | 0.0 | 3 | 0 | 2 | 0.78 | True | `08e3557c70ec` |
| stress | `stress.layout-stress-deep.pgm` | 63 | 62 | 0.0 | 0.0 | 0 | 0 | 0 | 2.34 | True | `5ff5efa1c4ca` |

复跑: `./benchmarks/snapshot.sh`
对比: `./benchmarks/compare.sh <baseline.json> <current.json>`
