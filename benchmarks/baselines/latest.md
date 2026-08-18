# Snapshot 2026-08-03-211000-v2-initial
tag: `v2-initial`
set_files: `benchmarks/sets/demo-observe-set.txt`, `benchmarks/sets/mech-set.txt`, `benchmarks/sets/product-regression-set.txt`, `benchmarks/sets/smoke-set.txt`, `benchmarks/sets/stress-probe-set.txt`

- samples: 49
  - smoke: 4
  - product: 18
  - demo: 20
  - stress: 4
  - mech: 3
- status: ok=49, parse-error=0, render-error=0

| role | file | status | det | node_ovl | edge_grp | label_ovl | crossings | edge_len | canvas | aspect | n/e | ms |
|---|---|---|:---:|---:|---:|---:|---:|---:|---:|---:|---|---:|
| smoke | `smoke.client-api-db.taut` | ok | ✓ | 0 | 0 | 0 | 0 | 80.2 | 18883.2 | 2.0 | 3/2 | 0 |
| smoke | `smoke.decision-loop.taut` | ok | ✓ | 0 | 0 | 0 | 0 | 333.8 | 36461.4 | 4.9 | 4/4 | 0 |
| smoke | `smoke.fan-out-four.taut` | ok | ✓ | 0 | 0 | 0 | 0 | 484.0 | 48307.2 | 3.1 | 5/4 | 0 |
| smoke | `smoke.flat-gateway-fanout.taut` | ok | ✓ | 0 | 0 | 0 | 0 | 286.9 | 31653.0 | 1.2 | 4/4 | 0 |
| product | `product.user-auth.taut` | ok | ✓ | 0 | 0 | 0 | 0 | 498.9 | 59209.2 | 1.2 | 5/8 | 1 |
| product | `product.typical-microservice-architecture.taut` | ok | ✓ | 0 | 0 | 0 | 2 | 2284.8 | 112841.6 | 1.6 | 8/16 | 1 |
| product | `product.three-tier.taut` | ok | ✓ | 0 | 0 | 0 | 0 | 171.3 | 19461.5 | 2.0 | 3/4 | 0 |
| product | `product.flat-rest-api.taut` | ok | ✓ | 0 | 0 | 0 | 0 | 450.5 | 74470.0 | 1.5 | 7/7 | 1 |
| product | `product.microservices.taut` | ok | ✓ | 0 | 0 | 0 | 0 | 892.3 | 96561.1 | 1.7 | 7/8 | 1 |
| product | `product.cdn-cache.taut` | ok | ✓ | 0 | 1 | 0 | 0 | 1073.5 | 105647.6 | 1.7 | 7/9 | 1 |
| product | `product.cloud-native.taut` | ok | ✓ | 0 | 10 | 3 | 0 | 2293.9 | 250199.0 | 2.0 | 12/14 | 1 |
| product | `product.ecommerce-platform.taut` | ok | ✓ | 0 | 1 | 0 | 0 | 2314.3 | 332946.4 | 1.0 | 18/19 | 1 |
| product | `product.message-queue-pipeline.taut` | ok | ✓ | 0 | 0 | 0 | 0 | 603.1 | 86214.4 | 1.1 | 7/7 | 1 |
| product | `product.monitoring-stack.taut` | ok | ✓ | 0 | 1 | 0 | 0 | 557.6 | 96815.4 | 1.2 | 7/6 | 1 |
| product | `product.narrow-corridor.taut` | ok | ✓ | 0 | 0 | 0 | 0 | 324.5 | 38526.3 | 1.2 | 6/5 | 0 |
| product | `product.linear-chain.taut` | ok | ✓ | 0 | 0 | 0 | 0 | 80.0 | 12956.4 | 4.3 | 3/2 | 0 |
| product | `product.refund-process.taut` | ok | ✓ | 0 | 0 | 0 | 0 | 1198.3 | 139831.6 | 1.6 | 10/11 | 1 |
| product | `product.swimlane-order-process.taut` | ok | ✓ | 0 | 0 | 0 | 0 | 288.7 | 64358.1 | 5.0 | 8/7 | 1 |
| product | `product.password-reset.taut` | ok | ✓ | 0 | 0 | 0 | 0 | 1361.3 | 189661.9 | 1.2 | 10/11 | 1 |
| product | `product.leave-approval-process.taut` | ok | ✓ | 0 | 0 | 0 | 0 | 1524.3 | 155449.2 | 1.1 | 10/11 | 1 |
| product | `product.self-loop-retry.taut` | ok | ✓ | 0 | 0 | 0 | 0 | 630.8 | 66934.4 | 1.7 | 7/8 | 0 |
| product | `product.symmetric-fanout.taut` | ok | ✓ | 0 | 0 | 0 | 0 | 591.4 | 77068.8 | 1.1 | 6/7 | 0 |
| demo | `demo.k8s-multi-namespace-overview.taut` | ok | ✓ | 0 | 11 | 3 | 10 | 11207.3 | 778448.0 | 2.4 | 26/37 | 3 |
| demo | `demo.k8s-multi-cluster-federation.taut` | ok | ✓ | 0 | 3 | 3 | 11 | 9790.6 | 491683.0 | 1.5 | 21/35 | 2 |
| demo | `demo.k8s-platform-stack.taut` | ok | ✓ | 0 | 5 | 5 | 4 | 5080.4 | 400664.0 | 1.6 | 23/30 | 1 |
| demo | `demo.k8s-tenant-isolation.taut` | ok | ✓ | 0 | 5 | 3 | 0 | 5548.7 | 510593.4 | 2.1 | 19/26 | 1 |
| demo | `demo.k8s-blue-green-release-topology.taut` | ok | ✓ | 0 | 7 | 0 | 11 | 7524.9 | 369921.9 | 1.4 | 19/30 | 2 |
| demo | `demo.hybrid-cloud-dr-topology.taut` | ok | ✓ | 0 | 3 | 2 | 3 | 4627.2 | 502671.8 | 1.6 | 17/24 | 1 |
| demo | `demo.data-lineage-platform.taut` | ok | ✓ | 0 | 0 | 1 | 2 | 3170.2 | 318641.6 | 1.3 | 16/20 | 1 |
| demo | `demo.ai-agent-docops-pipeline.taut` | ok | ✓ | 0 | 2 | 0 | 0 | 2821.5 | 422378.9 | 1.2 | 16/17 | 1 |
| demo | `demo.payment-clearing-platform.taut` | ok | ✓ | 0 | 1 | 0 | 1 | 2378.3 | 340899.9 | 1.1 | 16/17 | 1 |
| demo | `demo.supply-chain-control-tower.taut` | ok | ✓ | 0 | 1 | 0 | 0 | 2498.8 | 381500.4 | 2.6 | 16/18 | 1 |
| demo | `demo.mcp-server-cluster-architecture.taut` | ok | ✓ | 0 | 0 | 0 | 0 | 1284.9 | 159804.7 | 1.7 | 9/11 | 1 |
| demo | `demo.tautcore-core-mod-deps.taut` | ok | ✓ | 0 | 13 | 6 | 14 | 10593.6 | 901466.8 | 2.2 | 20/29 | 1 |
| demo | `demo.flat-realtime-recommendation.taut` | ok | ✓ | 0 | 0 | 0 | 1 | 1429.7 | 109034.2 | 1.5 | 9/10 | 1 |
| demo | `demo.aml-case-investigation.taut` | ok | ✓ | 0 | 0 | 0 | 0 | 1734.8 | 166407.9 | 4.2 | 13/15 | 1 |
| demo | `demo.k8s-incident-response.taut` | ok | ✓ | 0 | 0 | 0 | 2 | 3688.2 | 332586.8 | 2.1 | 19/24 | 1 |
| demo | `demo.change-approval-workflow.taut` | ok | ✓ | 0 | 0 | 0 | 0 | 1951.7 | 279206.8 | 2.9 | 16/18 | 1 |
| demo | `demo.pr-architecture-review.taut` | ok | ✓ | 0 | 0 | 0 | 2 | 2723.3 | 239537.6 | 1.6 | 13/15 | 1 |
| demo | `demo.ci-cd-security-pipeline.taut` | ok | ✓ | 0 | 3 | 0 | 2 | 6836.7 | 421624.1 | 4.8 | 23/29 | 1 |
| demo | `demo.e-commerce-order-fulfillment.taut` | ok | ✓ | 0 | 0 | 0 | 0 | 2069.4 | 245433.0 | 4.0 | 21/24 | 1 |
| demo | `demo.customer-refund-process.taut` | ok | ✓ | 0 | 0 | 0 | 0 | 3298.3 | 447937.1 | 1.6 | 23/27 | 1 |
| stress | `stress.layout-stress-nested.taut` | ok | ✓ | 0 | 2 | 1 | 0 | 1806.0 | 247580.1 | 1.4 | 11/13 | 1 |
| stress | `stress.layout-stress-flat-mesh.taut` | ok | ✓ | 0 | 0 | 0 | 7 | 3056.9 | 193661.1 | 1.2 | 11/20 | 1 |
| stress | `stress.layout-stress-dag.taut` | ok | ✓ | 0 | 0 | 0 | 1 | 2405.3 | 134434.8 | 3.6 | 11/18 | 1 |
| stress | `stress.layout-stress-yfiles-pipeline.taut` | ok | ✓ | 0 | 0 | 0 | 6 | 5230.7 | 256671.0 | 1.5 | 23/31 | 1 |
| mech | `mech.constrain-cross-group.taut` | ok | ✓ | 0 | 0 | 0 | 0 | 121.1 | 31604.4 | 2.5 | 4/3 | 0 |
| mech | `mech.constrain-flat-chain.taut` | ok | ✓ | 0 | 0 | 0 | 0 | 223.3 | 42666.1 | 1.7 | 5/4 | 1 |
| mech | `mech.constrain-sink.taut` | ok | ✓ | 0 | 0 | 0 | 0 | 240.0 | 26751.2 | 1.4 | 4/4 | 0 |

复跑: `./benchmarks/snapshot.sh`
对比: `./benchmarks/compare.sh <baseline.json> <current.json>`
