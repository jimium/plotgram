# Plan IR（Stage 1 交付 1.1 / 1.2）

整图离散决策的唯一中间表示：`Plan` 可序列化（serde）、可稳定指纹
（`Plan::fingerprint()`，手写 FNV-1a 64 规范编码，跨双跑/构建/平台一致）、
可逐字段 diff（`diff(a, b) -> PlanDiff`）。**旁路模块，不接线生产管线**——
生产路径仍走 `pipeline` → `recipes` → `routing`（23 号文 D1）。

## 与 `channel::Substrate` 的边界

Plan **不嵌入** `channel::Substrate`：后者是 `derive_substrate` 的运行态重产物
（段 links、端口容量、gate crossings），由 blueprint 确定性重建即可，序列化它
只会引入冗余与漂移面。Plan 只存：

- `SubstrateSketch { rank_count, order_count }`：rank×order 网格摘要
- `GroupScopeSpec`：组层次 + rank/order 覆盖闭区间（Adapter 分段所需）

与 channel 的对接是类型级复用：`record_route` 收录 `RouteOutcome`
（非 Converged 拒收且 Plan 不变），`detect_and_set_bundles` 直接调
`channel::detect_bundles` 写入 `Plan.bundles`（复用 `channel::Bundle`）。

## 确定性与相等口径

- 全字段 `BTreeMap` + 有序键：序列化 / 指纹 / diff 与插入顺序无关
- 指纹不用 std `DefaultHasher`（跨 Rust 版本无稳定承诺）
- **两套相等口径（混用会踩坑，钉死如下）**：
  - `==`（derive）：逐字段**结构相等**，含 `slot_id`、`provenance`、
    bundles 顺序；供 serde 往返、「拒收后不变」类断言
  - **决策口径**：`fingerprint()` / `semantic_eq()` / `diff()` 三者一致——
    `provenance`（溯源元数据）、`PortRef.slot_id`（基底重建可重编号，
    语义身份是 `(node, side, slot_index)`，24 号文 R1）、bundles 的 Vec
    顺序（指纹按 `(suffix, edges)` 规范序编码，diff 用集合差）均不参与。
    增量缓存用指纹判「决策是否变」与 `PlanDiff::is_empty` 不会互相误判
- `record_route` 成功即清空 `bundles`（channels 变更后旧合流失效，须重跑
  `detect_and_set_bundles`）
- `Plan::validate()`：轻量不变量（channels 有边必有 gates/provenance、组
  parent 不悬空且区间不倒置、端口只引用已知节点），Stage 1.3 Adapter
  的入口门槛
- `RouteOutcome.cost` 不进 Plan：拓扑 IR 只存决策，代价是求解过程量

## 留债

- **TODO(Stage1-1.3)** Legacy Adapter：从旧管线中间产物（`LayeredDraft`、
  `endpoint_map`、`LaneAssignment`、`MergeInterval`）反向构造 Plan
- **TODO(Stage1-1.4)** Ink 原型：`(Plan, 旧坐标) → 边几何`，与旧几何对拍
- **TODO(Stage1-I.5)** 端口策略：`ports` 字段本阶段由调用方填入
