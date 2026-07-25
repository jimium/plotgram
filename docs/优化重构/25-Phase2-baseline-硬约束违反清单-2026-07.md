# Phase 2 baseline 硬约束违反清单（product-regression，H0 已对齐 E1）

> Phase 1 离线硬约束校验（H0–H3）。H4/H5 尚未逐边展开；H6 由签名确定性单测覆盖。

## 汇总

| 样本数 | 检查边数 | 违规总数 |
|---:|---:|---:|
| 30 | 265 | 216 |

## 按硬约束种类

| 种类 | 次数 |
|---|---:|
| H0EndpointBoundary | 81 |
| H0StubDirection | 74 |
| H1ThroughNode | 33 |
| H2GroupInterior | 1 |
| H3NonOrthogonal | 27 |

## 逐样本

### `showcase/architecture/product.three-tier.pgm` — signature=`67222b60d08b309c` — checked=4 violations=0

_无 H0–H3 违规_

### `showcase/architecture/product.typical-microservice-architecture.pgm` — signature=`9520c456f5f595a0` — checked=16 violations=7

| edge | from → to | kind | detail |
|---:|---|---|---|
| 1 | api_gateway → order_svc | H1ThroughNode | path crosses node interior |
| 2 | api_gateway → product_svc | H1ThroughNode | path crosses node interior |
| 3 | user_svc → postgres | H1ThroughNode | path crosses node interior |
| 5 | product_svc → postgres | H1ThroughNode | path crosses node interior |
| 6 | order_svc → redis | H1ThroughNode | path crosses node interior |
| 8 | order_svc → kafka | H1ThroughNode | path crosses node interior |
| 15 | kafka → prometheus | H1ThroughNode | path crosses node interior |

### `showcase/architecture/product.flat-rest-api.pgm` — signature=`c3d76c21f994297c` — checked=7 violations=1

| edge | from → to | kind | detail |
|---:|---|---|---|
| 5 | biz → redis | H1ThroughNode | path crosses node interior |

### `showcase/architecture/product.microservices.pgm` — signature=`d40c69bf75f13e7e` — checked=8 violations=3

| edge | from → to | kind | detail |
|---:|---|---|---|
| 0 | web → gateway | H1ThroughNode | path crosses node interior |
| 5 | order_svc → db | H1ThroughNode | path crosses node interior |
| 7 | mq → user_svc | H1ThroughNode | path crosses node interior |

### `showcase/architecture/product.cdn-cache.pgm` — signature=`99552f27b87146dc` — checked=9 violations=1

| edge | from → to | kind | detail |
|---:|---|---|---|
| 5 | edge_pop → origin_lb | H1ThroughNode | path crosses node interior |

### `showcase/architecture/product.cloud-native.pgm` — signature=`7c3878be0f6099ac` — checked=14 violations=6

| edge | from → to | kind | detail |
|---:|---|---|---|
| 4 | worker → config_center | H1ThroughNode | path crosses node interior |
| 7 | api → metrics | H1ThroughNode | path crosses node interior |
| 8 | worker → metrics | H1ThroughNode | path crosses node interior |
| 10 | api → traces | H1ThroughNode | path crosses node interior |
| 11 | metrics → grafana | H1ThroughNode | path crosses node interior |
| 13 | traces → grafana | H1ThroughNode | path crosses node interior |

### `showcase/architecture/product.ecommerce-platform.pgm` — signature=`1f3bc1ff850c66b7` — checked=19 violations=12

| edge | from → to | kind | detail |
|---:|---|---|---|
| 4 | app → lb | H1ThroughNode | path crosses node interior |
| 6 | api_gw → user_svc | H0StubDirection | to stub direction != Left |
| 8 | api_gw → order_svc | H1ThroughNode | path crosses node interior |
| 9 | api_gw → search_svc | H0StubDirection | from stub direction != Left |
| 9 | api_gw → search_svc | H0StubDirection | to stub direction != Left |
| 10 | payment_svc → alipay | H1ThroughNode | path crosses node interior |
| 11 | user_svc → mysql | H1ThroughNode | path crosses node interior |
| 12 | product_svc → mysql | H0StubDirection | from stub direction != Left |
| 12 | product_svc → mysql | H0StubDirection | to stub direction != Left |
| 15 | search_svc → es | H1ThroughNode | path crosses node interior |
| 16 | order_svc → mq | H1ThroughNode | path crosses node interior |
| 17 | mq → notify_svc | H1ThroughNode | path crosses node interior |

### `showcase/architecture/product.message-queue-pipeline.pgm` — signature=`f8f7ba7aa114fd9c` — checked=7 violations=3

| edge | from → to | kind | detail |
|---:|---|---|---|
| 0 | order_svc → kafka | H1ThroughNode | path crosses node interior |
| 4 | kafka → stat_svc | H1ThroughNode | path crosses node interior |
| 5 | point_svc → db | H1ThroughNode | path crosses node interior |

### `showcase/architecture/product.monitoring-stack.pgm` — signature=`324d6f057bf2badc` — checked=6 violations=1

| edge | from → to | kind | detail |
|---:|---|---|---|
| 0 | app → prom | H1ThroughNode | path crosses node interior |

### `showcase/architecture/product.narrow-corridor.pgm` — signature=`21ec496db57080de` — checked=5 violations=2

| edge | from → to | kind | detail |
|---:|---|---|---|
| 1 | mobile → api | H1ThroughNode | path crosses node interior |
| 4 | worker → db | H1ThroughNode | path crosses node interior |

### `showcase/flowchart/product.linear-chain.pgm` — signature=`e5a07e9fe4bd8b4d` — checked=2 violations=0

_无 H0–H3 违规_

### `showcase/flowchart/product.user-auth.pgm` — signature=`c96eddc9157cbdf8` — checked=8 violations=0

_无 H0–H3 违规_

### `showcase/flowchart/product.refund-process.pgm` — signature=`d08e5efe07e64b65` — checked=11 violations=1

| edge | from → to | kind | detail |
|---:|---|---|---|
| 4 | manual_review → review_result | H0StubDirection | to stub direction != Top |

### `showcase/flowchart/product.swimlane-order-process.pgm` — signature=`0d98f01ceb08c92e` — checked=7 violations=0

_无 H0–H3 违规_

### `showcase/flowchart/product.password-reset.pgm` — signature=`e8345c7af9a12732` — checked=11 violations=6

| edge | from → to | kind | detail |
|---:|---|---|---|
| 5 | send_link → click_link | H0StubDirection | to stub direction != Top |
| 6 | click_link → token_gate | H0StubDirection | to stub direction != Top |
| 7 | token_gate → set_new | H0StubDirection | from stub direction != Bottom |
| 7 | token_gate → set_new | H0StubDirection | to stub direction != Top |
| 9 | set_new → update_db | H0StubDirection | from stub direction != Bottom |
| 9 | set_new → update_db | H0StubDirection | to stub direction != Top |

### `showcase/flowchart/product.leave-approval-process.pgm` — signature=`0948e1c0cc01944b` — checked=11 violations=1

| edge | from → to | kind | detail |
|---:|---|---|---|
| 1 | fill_form → days_gate | H1ThroughNode | path crosses node interior |

### `showcase/flowchart/product.self-loop-retry.pgm` — signature=`22d470c4f3bcb9ae` — checked=8 violations=0

_无 H0–H3 违规_

### `showcase/flowchart/product.symmetric-fanout.pgm` — signature=`f0ab2d5f1808ab8e` — checked=7 violations=2

| edge | from → to | kind | detail |
|---:|---|---|---|
| 2 | review → reject | H0StubDirection | from stub direction != Bottom |
| 2 | review → reject | H0StubDirection | to stub direction != Top |

### `showcase/sequence/product.oauth-login.pgm` — signature=`9f62666a372848f9` — checked=8 violations=16

| edge | from → to | kind | detail |
|---:|---|---|---|
| 0 | user → browser | H0EndpointBoundary | from endpoint not on boundary of user |
| 0 | user → browser | H0EndpointBoundary | to endpoint not on boundary of browser |
| 1 | browser → auth | H0EndpointBoundary | from endpoint not on boundary of browser |
| 1 | browser → auth | H0EndpointBoundary | to endpoint not on boundary of auth |
| 2 | user → auth | H0EndpointBoundary | from endpoint not on boundary of user |
| 2 | user → auth | H0EndpointBoundary | to endpoint not on boundary of auth |
| 3 | auth → browser | H0EndpointBoundary | from endpoint not on boundary of auth |
| 3 | auth → browser | H0EndpointBoundary | to endpoint not on boundary of browser |
| 4 | browser → auth | H0EndpointBoundary | from endpoint not on boundary of browser |
| 4 | browser → auth | H0EndpointBoundary | to endpoint not on boundary of auth |
| 5 | auth → browser | H0EndpointBoundary | from endpoint not on boundary of auth |
| 5 | auth → browser | H0EndpointBoundary | to endpoint not on boundary of browser |
| 6 | browser → resource | H0EndpointBoundary | from endpoint not on boundary of browser |
| 6 | browser → resource | H0EndpointBoundary | to endpoint not on boundary of resource |
| 7 | resource → browser | H0EndpointBoundary | from endpoint not on boundary of resource |
| 7 | resource → browser | H0EndpointBoundary | to endpoint not on boundary of browser |

### `showcase/sequence/product.payment-gateway.pgm` — signature=`aa41c09ffa3cdc91` — checked=11 violations=22

| edge | from → to | kind | detail |
|---:|---|---|---|
| 0 | user → shop | H0EndpointBoundary | from endpoint not on boundary of user |
| 0 | user → shop | H0EndpointBoundary | to endpoint not on boundary of shop |
| 1 | shop → order_svc | H0EndpointBoundary | from endpoint not on boundary of shop |
| 1 | shop → order_svc | H0EndpointBoundary | to endpoint not on boundary of order_svc |
| 2 | order_svc → pay_gw | H0EndpointBoundary | from endpoint not on boundary of order_svc |
| 2 | order_svc → pay_gw | H0EndpointBoundary | to endpoint not on boundary of pay_gw |
| 3 | pay_gw → order_svc | H0EndpointBoundary | from endpoint not on boundary of pay_gw |
| 3 | pay_gw → order_svc | H0EndpointBoundary | to endpoint not on boundary of order_svc |
| 4 | order_svc → shop | H0EndpointBoundary | from endpoint not on boundary of order_svc |
| 4 | order_svc → shop | H0EndpointBoundary | to endpoint not on boundary of shop |
| 5 | user → pay_gw | H0EndpointBoundary | from endpoint not on boundary of user |
| 5 | user → pay_gw | H0EndpointBoundary | to endpoint not on boundary of pay_gw |
| 6 | pay_gw → bank | H0EndpointBoundary | from endpoint not on boundary of pay_gw |
| 6 | pay_gw → bank | H0EndpointBoundary | to endpoint not on boundary of bank |
| 7 | bank → pay_gw | H0EndpointBoundary | from endpoint not on boundary of bank |
| 7 | bank → pay_gw | H0EndpointBoundary | to endpoint not on boundary of pay_gw |
| 8 | pay_gw → user | H0EndpointBoundary | from endpoint not on boundary of pay_gw |
| 8 | pay_gw → user | H0EndpointBoundary | to endpoint not on boundary of user |
| 9 | pay_gw → order_svc | H0EndpointBoundary | from endpoint not on boundary of pay_gw |
| 9 | pay_gw → order_svc | H0EndpointBoundary | to endpoint not on boundary of order_svc |
| 10 | order_svc → shop | H0EndpointBoundary | from endpoint not on boundary of order_svc |
| 10 | order_svc → shop | H0EndpointBoundary | to endpoint not on boundary of shop |

### `showcase/sequence/product.sms-verification.pgm` — signature=`7e08a5f55e6d5867` — checked=11 violations=22

| edge | from → to | kind | detail |
|---:|---|---|---|
| 0 | user → app | H0EndpointBoundary | from endpoint not on boundary of user |
| 0 | user → app | H0EndpointBoundary | to endpoint not on boundary of app |
| 1 | app → auth | H0EndpointBoundary | from endpoint not on boundary of app |
| 1 | app → auth | H0EndpointBoundary | to endpoint not on boundary of auth |
| 2 | auth → otp_store | H0EndpointBoundary | from endpoint not on boundary of auth |
| 2 | auth → otp_store | H0EndpointBoundary | to endpoint not on boundary of otp_store |
| 3 | auth → sms | H0EndpointBoundary | from endpoint not on boundary of auth |
| 3 | auth → sms | H0EndpointBoundary | to endpoint not on boundary of sms |
| 4 | sms → user | H0EndpointBoundary | from endpoint not on boundary of sms |
| 4 | sms → user | H0EndpointBoundary | to endpoint not on boundary of user |
| 5 | user → app | H0EndpointBoundary | from endpoint not on boundary of user |
| 5 | user → app | H0EndpointBoundary | to endpoint not on boundary of app |
| 6 | app → auth | H0EndpointBoundary | from endpoint not on boundary of app |
| 6 | app → auth | H0EndpointBoundary | to endpoint not on boundary of auth |
| 7 | auth → otp_store | H0EndpointBoundary | from endpoint not on boundary of auth |
| 7 | auth → otp_store | H0EndpointBoundary | to endpoint not on boundary of otp_store |
| 8 | otp_store → auth | H0EndpointBoundary | from endpoint not on boundary of otp_store |
| 8 | otp_store → auth | H0EndpointBoundary | to endpoint not on boundary of auth |
| 9 | auth → app | H0EndpointBoundary | from endpoint not on boundary of auth |
| 9 | auth → app | H0EndpointBoundary | to endpoint not on boundary of app |
| 10 | app → user | H0EndpointBoundary | from endpoint not on boundary of app |
| 10 | app → user | H0EndpointBoundary | to endpoint not on boundary of user |

### `showcase/sequence/product.jwt-refresh-token.pgm` — signature=`dd64cb079471c019` — checked=10 violations=20

| edge | from → to | kind | detail |
|---:|---|---|---|
| 0 | user → spa | H0EndpointBoundary | from endpoint not on boundary of user |
| 0 | user → spa | H0EndpointBoundary | to endpoint not on boundary of spa |
| 1 | spa → api | H0EndpointBoundary | from endpoint not on boundary of spa |
| 1 | spa → api | H0EndpointBoundary | to endpoint not on boundary of api |
| 2 | api → spa | H0EndpointBoundary | from endpoint not on boundary of api |
| 2 | api → spa | H0EndpointBoundary | to endpoint not on boundary of spa |
| 3 | spa → auth | H0EndpointBoundary | from endpoint not on boundary of spa |
| 3 | spa → auth | H0EndpointBoundary | to endpoint not on boundary of auth |
| 4 | auth → store | H0EndpointBoundary | from endpoint not on boundary of auth |
| 4 | auth → store | H0EndpointBoundary | to endpoint not on boundary of store |
| 5 | store → auth | H0EndpointBoundary | from endpoint not on boundary of store |
| 5 | store → auth | H0EndpointBoundary | to endpoint not on boundary of auth |
| 6 | auth → spa | H0EndpointBoundary | from endpoint not on boundary of auth |
| 6 | auth → spa | H0EndpointBoundary | to endpoint not on boundary of spa |
| 7 | spa → api | H0EndpointBoundary | from endpoint not on boundary of spa |
| 7 | spa → api | H0EndpointBoundary | to endpoint not on boundary of api |
| 8 | api → spa | H0EndpointBoundary | from endpoint not on boundary of api |
| 8 | api → spa | H0EndpointBoundary | to endpoint not on boundary of spa |
| 9 | spa → user | H0EndpointBoundary | from endpoint not on boundary of spa |
| 9 | spa → user | H0EndpointBoundary | to endpoint not on boundary of user |

### `showcase/state/product.order-lifecycle.pgm` — signature=`50d69dc68ff6eb25` — checked=8 violations=2

| edge | from → to | kind | detail |
|---:|---|---|---|
| 1 | created → timeout | H0StubDirection | from stub direction != Bottom |
| 1 | created → timeout | H0StubDirection | to stub direction != Top |

### `showcase/state/product.user-session.pgm` — signature=`6bfe5ec131256014` — checked=9 violations=7

| edge | from → to | kind | detail |
|---:|---|---|---|
| 4 | active → idle | H0StubDirection | from stub direction != Bottom |
| 4 | active → idle | H0StubDirection | to stub direction != Top |
| 5 | idle → active | H0StubDirection | from stub direction != Top |
| 5 | idle → active | H0StubDirection | to stub direction != Bottom |
| 6 | idle → expired | H0StubDirection | from stub direction != Bottom |
| 6 | idle → expired | H0StubDirection | to stub direction != Top |
| 7 | active → logged_out | H1ThroughNode | path crosses node interior |

### `showcase/state/product.payment-flow.pgm` — signature=`623b68917fa6eb42` — checked=12 violations=0

_无 H0–H3 违规_

### `showcase/state/product.delivery-tracking.pgm` — signature=`d9efe797e49ec03e` — checked=7 violations=4

| edge | from → to | kind | detail |
|---:|---|---|---|
| 1 | pending → in_transit | H0StubDirection | from stub direction != Bottom |
| 1 | pending → in_transit | H0StubDirection | to stub direction != Top |
| 2 | in_transit → out_delivery | H0StubDirection | from stub direction != Bottom |
| 2 | in_transit → out_delivery | H0StubDirection | to stub direction != Top |

### `showcase/er/product.blog-schema.pgm` — signature=`96b423d8aca869f1` — checked=5 violations=14

| edge | from → to | kind | detail |
|---:|---|---|---|
| 0 | user → post | H3NonOrthogonal | non-axis-aligned segment |
| 0 | user → post | H0StubDirection | from stub direction != Bottom |
| 0 | user → post | H0StubDirection | to stub direction != Top |
| 1 | user → comment | H3NonOrthogonal | non-axis-aligned segment |
| 1 | user → comment | H0StubDirection | to stub direction != Top |
| 2 | post → comment | H3NonOrthogonal | non-axis-aligned segment |
| 2 | post → comment | H0StubDirection | from stub direction != Bottom |
| 2 | post → comment | H0StubDirection | to stub direction != Top |
| 3 | post → post_tag | H3NonOrthogonal | non-axis-aligned segment |
| 3 | post → post_tag | H0StubDirection | from stub direction != Bottom |
| 3 | post → post_tag | H0StubDirection | to stub direction != Top |
| 4 | tag → post_tag | H3NonOrthogonal | non-axis-aligned segment |
| 4 | tag → post_tag | H0StubDirection | from stub direction != Bottom |
| 4 | tag → post_tag | H0StubDirection | to stub direction != Top |

### `showcase/er/product.saas-schema.pgm` — signature=`a4eeb2d130049bec` — checked=10 violations=27

| edge | from → to | kind | detail |
|---:|---|---|---|
| 0 | tenant → user | H3NonOrthogonal | non-axis-aligned segment |
| 0 | tenant → user | H0StubDirection | from stub direction != Bottom |
| 0 | tenant → user | H0StubDirection | to stub direction != Top |
| 1 | tenant → project | H3NonOrthogonal | non-axis-aligned segment |
| 1 | tenant → project | H0StubDirection | to stub direction != Top |
| 2 | tenant → ws | H3NonOrthogonal | non-axis-aligned segment |
| 2 | tenant → ws | H0StubDirection | to stub direction != Top |
| 3 | user → user_role | H3NonOrthogonal | non-axis-aligned segment |
| 3 | user → user_role | H0StubDirection | to stub direction != Top |
| 3 | user → user_role | H2GroupInterior | path crosses unrelated group interior |
| 4 | role → user_role | H3NonOrthogonal | non-axis-aligned segment |
| 4 | role → user_role | H0StubDirection | from stub direction != Bottom |
| 4 | role → user_role | H0StubDirection | to stub direction != Top |
| 5 | role → role_perm | H3NonOrthogonal | non-axis-aligned segment |
| 5 | role → role_perm | H0StubDirection | from stub direction != Bottom |
| 5 | role → role_perm | H0StubDirection | to stub direction != Top |
| 6 | permission → role_perm | H3NonOrthogonal | non-axis-aligned segment |
| 6 | permission → role_perm | H0StubDirection | from stub direction != Bottom |
| 6 | permission → role_perm | H0StubDirection | to stub direction != Top |
| 7 | ws → project | H3NonOrthogonal | non-axis-aligned segment |
| 7 | ws → project | H0StubDirection | from stub direction != Bottom |
| 7 | ws → project | H0StubDirection | to stub direction != Top |
| 8 | user → audit_log | H3NonOrthogonal | non-axis-aligned segment |
| 8 | user → audit_log | H0StubDirection | to stub direction != Top |
| 9 | project → audit_log | H3NonOrthogonal | non-axis-aligned segment |
| 9 | project → audit_log | H0StubDirection | from stub direction != Bottom |
| 9 | project → audit_log | H0StubDirection | to stub direction != Top |

### `showcase/er/product.crm-schema.pgm` — signature=`be5a047d269bf557` — checked=5 violations=12

| edge | from → to | kind | detail |
|---:|---|---|---|
| 0 | account → contact | H3NonOrthogonal | non-axis-aligned segment |
| 0 | account → contact | H0EndpointBoundary | to endpoint not on boundary of contact |
| 0 | account → contact | H0StubDirection | from stub direction != Bottom |
| 0 | account → contact | H0StubDirection | to stub direction != Top |
| 1 | account → opportunity | H3NonOrthogonal | non-axis-aligned segment |
| 1 | account → opportunity | H0StubDirection | from stub direction != Bottom |
| 1 | account → opportunity | H0StubDirection | to stub direction != Top |
| 3 | sales_rep → account | H3NonOrthogonal | non-axis-aligned segment |
| 3 | sales_rep → account | H0StubDirection | from stub direction != Bottom |
| 3 | sales_rep → account | H0StubDirection | to stub direction != Top |
| 4 | sales_rep → opportunity | H3NonOrthogonal | non-axis-aligned segment |
| 4 | sales_rep → opportunity | H0StubDirection | to stub direction != Top |

### `showcase/mindmap/product.tech-stack.pgm` — signature=`ebe1da9e2ccd381f` — checked=9 violations=24

| edge | from → to | kind | detail |
|---:|---|---|---|
| 0 | stack → frontend | H3NonOrthogonal | non-axis-aligned segment |
| 0 | stack → frontend | H0StubDirection | from stub direction != Right |
| 0 | stack → frontend | H0StubDirection | to stub direction != Left |
| 1 | frontend → react | H3NonOrthogonal | non-axis-aligned segment |
| 1 | frontend → react | H0StubDirection | from stub direction != Right |
| 1 | frontend → react | H0StubDirection | to stub direction != Left |
| 2 | frontend → wasm | H3NonOrthogonal | non-axis-aligned segment |
| 2 | frontend → wasm | H0StubDirection | from stub direction != Right |
| 2 | frontend → wasm | H0StubDirection | to stub direction != Left |
| 4 | backend → rust | H3NonOrthogonal | non-axis-aligned segment |
| 4 | backend → rust | H0StubDirection | from stub direction != Right |
| 4 | backend → rust | H0StubDirection | to stub direction != Left |
| 5 | backend → postgres | H3NonOrthogonal | non-axis-aligned segment |
| 5 | backend → postgres | H0StubDirection | from stub direction != Right |
| 5 | backend → postgres | H0StubDirection | to stub direction != Left |
| 6 | stack → devops | H3NonOrthogonal | non-axis-aligned segment |
| 6 | stack → devops | H0StubDirection | from stub direction != Right |
| 6 | stack → devops | H0StubDirection | to stub direction != Left |
| 7 | devops → docker | H3NonOrthogonal | non-axis-aligned segment |
| 7 | devops → docker | H0StubDirection | from stub direction != Right |
| 7 | devops → docker | H0StubDirection | to stub direction != Left |
| 8 | devops → k8s | H3NonOrthogonal | non-axis-aligned segment |
| 8 | devops → k8s | H0StubDirection | from stub direction != Right |
| 8 | devops → k8s | H0StubDirection | to stub direction != Left |

