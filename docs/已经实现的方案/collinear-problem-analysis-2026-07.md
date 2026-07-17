# 正交路由：共线 / 重叠 / 合流 — 全局方案（压缩版）

> 日期：2026-07-15  
> 范围：正交路由 + `grid_snap` + pipeline 末尾 post-route  
> 状态：架构纲领，尚未实现  
> 执行计划：[`collinear-execution-plan-2026-07.md`](collinear-execution-plan-2026-07.md)（含质量/性能门禁）  
> 约束：对齐 [`布局与路由核心手册`](../总结经验/布局与路由核心手册-2026-07.md)；**禁止**新建空壳 `EdgeGeometryContract` 式大重构再修行为

---

## 0. 一句话

「共线」不是一类 bug。先分清**有意合流**与**非语义重叠**；几何只负责测量，语义负责裁决；会改路径形状的后处理必须过同一验证并失败回退。落地用**旁路注解 + 分类器 + 验证钩子**，不先换 `EdgeLayout`。

---

## 1. 问题语言（两方向合一）

| 名称 | 现象 | 默认 |
|------|------|------|
| 表示冗余 | 单边三点严格共线 | 应删（等价压缩） |
| 微台阶 / overshoot | 短 Z/L、冲过再折回 | 有上下文才改；否则可暂留（R12） |
| 非语义重合 / 紧间距 | 无共享许可的 trunk exact/tight | 分离或显式 `Degraded` |
| 有意合流 | 同 merge 语义共享 trunk | 允许，须有区间与分叉点 |
| 端口汇聚 | 同侧 stub 重合/近距 | 按 docking 允许 |
| T/L 接头 | 端点触边 | 通常允许 |

原则：

- **是否允许重合** ← 路由语义（merge / stub / corridor），不是「坐标碰巧相同」。
- **是否允许改折点** ← 改后整路径是否仍满足约束，不是「少一个点更好」。

旧因果纠错（勿回潮）：

- 删**严格共线**中点**不改变**折线覆盖，**不会**单独把 lane 拉回重合。
- 真风险：`collapse_micro_jogs` 换角、grid snap 改通道坐标、straighten/lane、末尾 `sanitize_ext`。
- `enforce_reverse_pair_min_gap` 调多次 = **写权不清**的症状，不是「simplify 必毁 gap」的证明。
- 叉积固定阈值：长段更严、短段更松；问题是量纲差、多套阈值，不是「长段过松」。

---

## 2. 产品合流规则（实现前写死）

共享许可**不得**从最终几何反推。按 profile 映射到现有 `edge_merge_policy` / docking：

| 场景 | Flowchart（`semantic_merge=false`） | Architecture（`semantic_merge=true`） |
|------|-------------------------------------|----------------------------------------|
| 同侧 stub | Concentrate：共享锚点；Compact：slot 间距 | 同左 |
| Trunk 共享 | **默认禁止**几何巧合共干；走廊 lane 宜独占 | 仅当 `edges_may_share_trunk` 为真（FanOut / FanIn / ParallelPair 等**组键交集**；`SuperEdgePair` **不**授 trunk） |
| 共享区间 | — | 仅声明的 trunk run；越过汇合/分叉边界后必须分车道 |
| 正反向对 | 始终 `Forbid` 共干；gap ≥ profile `parallel_gap` | 同左（`enforce_reverse_pair_min_gap` 窄域） |
| 入向箭头合并 | 优先 **stub / 端口汇聚**；长 trunk 合并不作为默认目标 | 允许同源/同宿语义 trunk 合流；分叉后 `Forbid` |
| 分叉/汇合点 | 无 trunk 共享则不要求 | Contract 须标出 merge 区间端点；清理器不得越过 |

「希望的合理共线」= 上表允许项；「不合理重叠」= 无许可的 exact/tight，或越过共享区间仍共干。

---

## 3. 架构断点（现状）

1. **`Polyline { points }` 吞掉意图**：后续只能猜 `index=1/len-2` 是 stub。  
2. **多阶段改同一份几何**：lane / sanitize / snap / sanitize_ext / gap guard，无统一「是否仍守承诺」入口。  
3. **测量与裁决混用**：`segments_violate_spacing` 只有几何；bundle/stub 规则散落在 caller。  
4. **末尾 sanitize 无障/无 lane/无 merge 上下文** → R12 只能「台阶可暂留」，不能承诺「更直且安全」。

---

## 4. 目标形态：旁路三件套（反空壳）

**禁止**：先替换 `EdgeLayout`、先建大而全的契约类型再修 V2/V3。  
**允许**：与现管线并行的旁路结构；行为改善可观测后再考虑内联。

```text
SegmentPairMeasure     （纯几何，已有 API 收口）
        +
classify_segment_pair  （唯一语义裁决入口）
        ↓
RouteAnnotation        （旁路：每边 stub/lane/merge 区间等）
        ↓
validate_route_edit    （仅挂在「会改形状」的后处理上）
```

### 4.1 测量 vs 裁决

- **Measure**：平行？投影重叠？gap？共享长度？端点关系？  
  ← `segments_violate_spacing` / `path_edge_spacing_violations` 停在这一层。  
- **Classify**：`Allowed | NeedsSeparation | Degraded(reason)`  
  ← 注入 profile、stub 保护区、`edges_may_share_trunk` / 多 `MergeGroup` 交集、corridor、正反向对。  
- 共享判定单位是 **段对 + 重叠区间 + 组键交集**，不是「每边一个 MergeId」。

### 4.2 RouteAnnotation（轻量旁路，C 阶段末冻结）

| 阶段 | 内容 |
|------|------|
| A/B 粗意图 | stub 保护区、merge **候选**组键、corridor demand（可空） |
| C 结束冻结 | 实际 lane/corridor 坐标、**已声明** merge 共享区间与分叉端点、`Degraded` |

折点不必塞 provenance 枚举。约束落在 **segment run**；折点只需边界角色（`Anchor` / `StubBoundary` / `ProtectedBoundary` / `FreeCorner`），可从 Annotation 推导。

### 4.3 `validate_route_edit`：何时必须跑

| 操作 | 验证 |
|------|------|
| 严格共线删点（覆盖范围不变） | **可跳过**（等价变换） |
| 换角 / `merge_overshoot` / 量化位移 / lane shift | **必须**；失败 → 回退 `before` |
| 标签避让 | 不改边几何 |

最少检查：端点与 stub 方向；正交连通；不穿节点/违规 group；不越过 lane/merge 保护区；不新增未解释的 `NeedsSeparation`。

---

## 5. 写权：单向收敛（非「单一写者」神话）

目标时序：

```text
布局冻结
 → 粗意图（slot / corridor demand / merge 候选）
 → 初始路由
 → C：冲突分类 + lane / unrelated-trunk / reroute 收敛 → 冻结 Annotation
 → D：一次 snap+repulse → 一次 contract-aware sanitize
 → 一次全局间距审计（含正反向窄域）+ 显式 Degraded
 → label resolve（最后写标签，不再改路径）
```

| 约束 | 最终写者 | 说明 |
|------|----------|------|
| lane / 非语义 trunk 分离 | C：`assign_lanes` + `separate_unrelated_trunk_overlaps` + X-1 | D 不得无验证挪受保护 trunk |
| 像素对齐 | D：`snap_and_repulse` 一次 | 变更当 RouteEdit |
| 形状消毒 | D：sanitize（微折须验证） | router 内 sanitize 保持保守 |
| 正反向 gap | D：一次审计；C 可预收敛 | **不**扩成万能重叠修复器 |
| 标签 | D 末 | sanitize 会重建 label → 必须在几何冻结后 |

有限空间下允许残余重叠，但必须是带原因的 `Degraded`，禁止偷偷破坏端口/穿模换「零重叠」。

---

## 6. 现有代码归并表（防叠第四套）

| 现有 | 归宿 |
|------|------|
| `segments_violate_spacing` / `path_edge_spacing_violations` | **Measure** 唯一几何入口 |
| `edges_may_share_trunk` / `MergeGroup` / profile | **Classify** 输入，不散落进低层几何 |
| stub / `STUB_GUARD_LENGTH` / docking | **Classify** 豁免与 Annotation stub 区 |
| `assign_lanes` | C 阶段 lane 写者；产出写入 Annotation |
| `separate_unrelated_trunk_overlaps` | C：architecture 非语义共干；经 Classify 触发 |
| X-1 `reroute_conflicting_edges` | C：`NeedsSeparation` 的重路由手段 |
| `enforce_reverse_pair_min_gap` | **窄域**正反向；收敛为 C 预修 + D 一次审计，勿泛化 |
| lint `UnrelatedEdgeTrunkMerge` 等 | 消费 **同一** Classify，禁止第二套语义 |
| scoring 软惩罚 | 可保留；硬裁决只认 Classify |
| `simplify_path` / grid 内 simplify | **公共** `collinear_simplify`（严格共线）；量化后放大 eps，禁止当路由器 |
| `collapse_micro_jogs` / `merge_overshoot` | 挂 `validate_route_edit`；失败回退（落实 R12） |
| 旧 nudge（主轴+Z） | 已废；**勿回潮** |

退役条件：Classify + Annotation 稳定后，删除「同语义多处 enforce」中**无证据支撑**的重复调用；**禁止**仅因「调用次数多」就删。

---

## 7. 迁移（每步可验收）

### Phase 0 — 证据

路径写入 trace（阶段、边、前后摘要、gap/穿障）；showcase：节点坐标 hash、exact/tight **严重度**、合法共享长度、Degraded 分布；稳定键排序。  
**验收**：能指出违规首次出现在 lane / snap / sanitize / guard 哪一步。

### Phase 1 — Measure + Classify

抽 `SegmentPairMeasure`；实现 `classify_segment_pair`；lint / scoring 统计 / reroute / debug 共用。  
**验收**：合法 bundle 不再报普通重合；非语义共干能指出区间与策略依据。

### Phase 2 — 最小 Annotation + 验证钩子

C 末写旁路 Annotation；仅给换角 / overshoot / 量化挂验证。  
**验收**：后处理不新引入穿模、越 merge/lane 边界；回退可观测；**仍不改 `EdgeLayout` 主结构**。

### Phase 3 — 写权收敛

等价 simplify 与「改形状」分离；gap 收敛到 C + D 一次审计；量化/标签各一次。  
**验收**：每类约束有最终写者；无「无条件连环 guard」。

### Phase 4 — 全局 lane（可选）

仅当 Phase 0 证明瓶颈是「同通道多条非共享 trunk」时，再以 Annotation 的 lane demand 做 track 分配；不可行则 `Degraded`，禁止任意最终 nudge。

---

## 8. 非目标

1. 图名/节点名特判。  
2. 消灭一切视觉共线（合流是产品能力）。  
3. 无上下文为「更直」穿障或毁 lane/merge。  
4. HashMap 序驱动决策。  
5. 导出层（drawio）反写布局几何。  
6. 靠抬高 `POST_QUANTIZE_SIMPLIFY_EPS` 消台阶。  
7. 把正反向 gap 扩成全局重叠万能药。  
8. 先建契约空壳再修行为（手册已废止项）。

---

## 9. 完成定义

不以「共线计数归零」为准，而以：

1. 每个 exact/tight trunk 可归为 `Allowed` / `NeedsSeparation` / `Degraded(reason)`；  
2. 有意合流有组键交集 + 共享区间，不靠几何巧合；  
3. 改形状的后处理统一验证、失败回退；等价 simplify 不改受约束几何；  
4. 写权单向；量化 / sanitize / gap 审计 / 标签职责清晰；  
5. showcase：节点坐标无意外漂移；重叠比**严重度**；确定、无图名分支。

---

## 参考落点

- `edge_routing_orthogonal/{simplify,sanitize,lane_assignment,scoring,corridor_route,run}.rs`
- `edge/edge_merge_policy.rs` · `grid_snap.rs` · `pipeline.rs` · `layout/lint/`
- `docs/总结经验/布局与路由核心手册-2026-07.md`
- `docs/已经实现的方案/edge-separation-proposal.md`
