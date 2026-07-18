# A1 DoD 增量：router `path_is_clean` ↔ lint 穿障判定一致率笔记（2026-07）

> 配套文档：`布局路由算法重构方案-2026-07.md`（TD-1 / TD-2 / A1 / B1 / A3）。
> 本笔记是 A1「统一 router 节点级穿障 primitive」的验收增量，用 CLI lint JSON 对回归样例集抽样，
> 按方案步骤 0 定义的**两类因**分别标注差异来源。

## 1. 结论速览

- A1 已把 router 侧节点级穿障判定收敛为单一 primitive `edge::common::geom_obstacle::segment_pierces_node`
  （等价于 `Rect::from(nl).expanded(pad).segment_crosses_interior(a, b, EPS)`，EPS=0.1）。
  `scoring::segment_intersects_node` 与 `lane_assignment::segment_hits_node` 内层均委托于它。
- **一致率笔记的核心判定**：抽样表明，最终几何上残留的 `edge_through_node` / `edge_crosses_group_interior`
  **不是** router 节点级 pad 不一致造成的（该来源已被 A1 消除），而是分裂为下列两类因。
- **A1 单独是「必要非充分」**：它只保证 router 内部各调用点用同一把尺子；要让 router 的 clean 结论与
  lint 最终裁决闭环，仍需 A3（折线冻结点后用 lint primitive 单点复校，`repair_through_edges_post_route`
  收敛为唯一响应者）+ B1（分组中点-vs-穿越分歧）。

## 2. 抽样方法与口径限制

- 样例：`benchmark-data/collinear-regression-set.txt`（10 张：8 architecture + 2 flowchart）。
- 数据源：`plotgram lint --format json --profile all <file>`，统计 `edge_through_node`、
  `edge_crosses_group_interior`、`edge_on_group_border` 三类 violation 计数；router 侧退化信号取自
  `[perf]` 诊断行（`x3_lane_assignment ... failed`、`s3_semantic_trunk degraded=N`、
  `d_stub_exact_post_route degraded=N`、`d_through_repair`）。
- **口径限制（重要）**：lint 门禁真值走 `refine::segment_intersects_node`（pad=0.5、eps=0.5），
  与 router 的 `segment_pierces_node`（pad ∈ {0.0, NODE_OBSTACLE_PAD=18.0}、eps=0.1）**阈值不同**。
  因此 CLI 层面只能做**种群级（population-level）**归因，不能读出逐边的严格 clean↔lint 布尔一致率——
  后者需在 router 内埋点导出 accept-time 判定，属 A3 的复校闭环，不在 M1（纯结构重构）范围。

## 3. 抽样结果（最终几何上的 lint 裁决）

| 图 | edge_through_node | edge_crosses_group_interior | edge_on_group_border | router 退化信号（perf） |
|---|---|---|---|---|
| c.layout-stress-nested | 0 | 0 | 0 | — |
| c.cloud-native | 1 | 0 | 0 | trunk rewritten=2；x3 failed |
| c.k8s-multi-cluster-federation | 2 | 0 | 0 | x3 failed |
| c.k8s-multi-namespace-overview | 4 | 0 | 0 | x3 failed |
| c.ecommerce-platform | 0 | 0 | 0 | — |
| c.hybrid-cloud-dr-topology | 2 | 1 | 0 | trunk rewritten=2；d_through_repair |
| c.layout-stress-dag (flow) | 0 | 0 | 0 | — |
| c.aml-case-investigation (flow) | 0 | 0 | 0 | — |
| c.k8s-tenant-isolation | 4 | 1 | 0 | trunk degraded=1；stub degraded=2；d_through_repair |
| c.k8s-platform-stack | 7 | 3 | 0 | trunk degraded=3；x3 failed=32；d_through_repair |
| **合计** | **20** | **5** | **0** | — |

关键观察：
- `edge_on_group_border` 全为 0；violation 集中在 `edge_through_node`（20）与 `edge_crosses_group_interior`（5）。
- **凡出现 through/group violation 的图，均伴随 router 侧退化信号**（trunk degraded/rewritten、
  x3 lane failed、stub degraded、d_through_repair 触发）；三张零 violation 图（stress-nested、
  ecommerce、两张 flowchart）router 侧亦无显著退化。→ violation 与 router 自知的退化强相关，
  而非 router「以为干净、lint 却报脏」的静默错判。

## 4. 两类因归因

### 因 (a)：同一几何下的判定差异（A1 / B1 域）
- **节点级 pad 差异**：A1 前 scoring 与 lane 各写一份（虽字节等价，仍是双份维护风险）；A1 后**已单源**。
  与 lint 的差异只剩「router eps=0.1/pad∈{0,18} vs lint eps=0.5/pad=0.5」这一**阈值口径差**，
  属可解释的固定偏置，不是逻辑分歧。
- **分组中点-vs-穿越**：lint 用段中点落域 `point_in_rect_interior(mid, gl)`（`lint/mod.rs:502`），
  router 用线段穿内部 `segment_crosses_interior`（`scoring.rs:301`）。这是**真算法分歧**，
  是 `edge_crosses_group_interior`（5 例）与部分 `edge_through_node` 落差的结构性来源。→ **B1** 处理，M1 不动。

### 因 (b)：几何被 router 校验后又被后续阶段改写（TD-1）
- router 在 route 阶段用 clean 判定接受折线，但 `s3_semantic_trunk`（rewritten/degraded）、
  `d_stub_exact_post_route`（degraded）、lane 移位等**后续写者**会改写几何；lint 跑在最终几何上，
  于是出现「route 时判干净、最终被改脏」的落差。
- 证据：k8s-platform-stack（trunk degraded=3、rewritten=4）、k8s-tenant-isolation（trunk degraded=1、
  stub degraded=2）等高 violation 图均在 route 后有改写/退化动作，且 `d_through_repair` 已触发仍有残留
  → 属改写后无法修复的真实退化，需 **A3**：折线冻结点后用 **lint primitive** 单点复校，
  `repair_through_edges_post_route` 收敛为该复校的唯一响应者。

## 5. 对 A1 验收的意义

- A1 的 DoD（node_fingerprint 不变 + `compare-collinear` PASS）已满足：本次改造是等价委托，
  快照对比无 node_fp / 严重度变化，正确性轨与质量轨均 PASS。
- 本笔记补齐 A1 DoD 增量：**残留 violation 已定位到 (a) B1 分组分歧 + 阈值口径差、(b) TD-1 后写改写**，
  与 router 节点级 pad 收敛**无关**——即 A1 达成了「router 内部单一尺子」的目标，未引入也未消除行为层面的落差。
- 后续闭环顺序建议：B1（统一分组判定语义）→ A3（冻结后单点复校 + repair 唯一响应）。

## 附：复现命令

```bash
for f in $(grep -v '^#' benchmark-data/collinear-regression-set.txt | grep -v '^$'); do
  ./target/release/plotgram lint --format json --profile all "$f" 2>/dev/null \
    | python3 -c "import sys,json,collections;d=json.load(sys.stdin);c=collections.Counter(v['rule'] for v in d.get('violations',[]));print(c.get('edge_through_node',0),c.get('edge_crosses_group_interior',0),c.get('edge_on_group_border',0),sep='\t')"
done
```
