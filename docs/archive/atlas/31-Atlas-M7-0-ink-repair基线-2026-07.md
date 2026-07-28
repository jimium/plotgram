# Atlas M7-0：Ink repair 基线（2026-07-27）

> 原测量第三道 dogleg（`repair_ink_group_pierces` → `atlas_plan_distorted_edges`）。
> **M7 后期（2026-07-27）**：宽测 product∪stress Hier `distorted≡0` 后，**已删除** Atlas 第三道 repair。

## 宽测合计（删 repair 前）

| 图集 | Hier 实测 | 触发 repair 的图 | 失真边 |
|------|-----------|------------------|--------|
| `product-regression-set` | 18 | 0 | 0 |
| `stress-probe-set` | 4 | 0 | 0 |

## 结论

**product + stress Hier 总 distorted=0** → 第三道 dogleg 已删。

穿组证明链现为：L6 substrate → Ink 守 gate/裙边 → M7-2 `ink_verify` 硬 FAIL → lint `edge_crosses_group_interior`。

| 项 | 状态 |
|----|------|
| ~~M7 中期~~ | Ink 守 gate + 根走廊裙边 |
| ~~M7-2~~ | 几何硬门禁（gate/走廊/正交/端口侧/scope；仅 `PlanDistorted` 软） |
| ~~M7 后期~~ | **已落地**：删 `repair_ink_group_pierces`；`atlas_plan_distorted_edges` 生产恒空 |

## 明细（product TSV，删前快照）

```
path	groups	edges	distorted	distorted_eids	class	note
showcase/architecture/product.three-tier.pgm	0	4	0		no_groups	no groups → repair N/A
showcase/architecture/product.typical-microservice-architecture.pgm	0	16	0		no_groups	no groups → repair N/A
showcase/architecture/product.flat-rest-api.pgm	0	7	0		no_groups	no groups → repair N/A
showcase/architecture/product.microservices.pgm	2	8	0		clean	groups present, no dogleg
showcase/architecture/product.cdn-cache.pgm	2	9	0		clean	groups present, no dogleg
showcase/architecture/product.cloud-native.pgm	6	14	0		clean	groups present, no dogleg
showcase/architecture/product.ecommerce-platform.pgm	5	19	0		clean	groups present, no dogleg
showcase/architecture/product.message-queue-pipeline.pgm	4	7	0		clean	groups present, no dogleg
showcase/architecture/product.monitoring-stack.pgm	3	6	0		clean	groups present, no dogleg
showcase/architecture/product.narrow-corridor.pgm	3	5	0		clean	groups present, no dogleg
showcase/flowchart/product.linear-chain.pgm	0	2	0		no_groups	no groups → repair N/A
showcase/flowchart/product.user-auth.pgm	0	8	0		no_groups	no groups → repair N/A
showcase/flowchart/product.refund-process.pgm	0	11	0		no_groups	no groups → repair N/A
showcase/flowchart/product.swimlane-order-process.pgm	4	7	0		clean	groups present, no dogleg
showcase/flowchart/product.password-reset.pgm	0	11	0		no_groups	no groups → repair N/A
showcase/flowchart/product.leave-approval-process.pgm	0	11	0		no_groups	no groups → repair N/A
showcase/flowchart/product.self-loop-retry.pgm	0	8	0		no_groups	no groups → repair N/A
showcase/flowchart/product.symmetric-fanout.pgm	0	7	0		no_groups	no groups → repair N/A
```

### stress Hier（删前快照）

```
showcase/architecture/stress.layout-stress-nested.pgm	5	13	0		clean
showcase/architecture/stress.layout-stress-flat-mesh.pgm	0	20	0		no_groups
showcase/flowchart/stress.layout-stress-dag.pgm	0	18	0		no_groups
showcase/flowchart/stress.layout-stress-yfiles-pipeline.pgm	0	31	0		no_groups
```

## 复现

```bash
cargo run -p plotgram-eval --bin atlas_repair_baseline -- benchmarks/sets/product-regression-set.txt
cargo run -p plotgram-eval --bin atlas_repair_baseline -- benchmarks/sets/stress-probe-set.txt
# 删后 hints.atlas_plan_distorted_edges 恒空；探针仍可用于回归观测
```
