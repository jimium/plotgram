# 视觉拥堵修正计划：跨对共干 · FanIn · 层间距 · 监控外环

> 日期：2026-07-15  
> 触发样例：`showcase/flowchart/n.user-auth.pgm`、`showcase/architecture/n.typical-microservice-architecture.pgm`  
> 手册：[`布局与路由核心手册-2026-07.md`](../../总结经验/布局与路由核心手册-2026-07.md)  
> 共线纲领：[`collinear-problem-analysis-2026-07.md`](./collinear-problem-analysis-2026-07.md)  
> 共线基建：[`collinear-execution-plan-2026-07.md`](./collinear-execution-plan-2026-07.md)（P0–P3 已完成）  
> 状态：**S0–S5 ✅ · S4.x ✅**（2026-07-15）；其后可选 **S5.2b / S2.2b / S3.2b / collinear P4**（见 §5.1）

---

## 0. 一句话

这两张图的「乱 / 挤」不是同一类局部折点 bug，而是 **端口资源未排他 + 层通道预算不足 + 有意合流未兑现 + 长跨反馈抢道** 的系统症状。修正必须按手册：先定写权与语义，再动坐标；用 Classify / Annotation / 严重度门禁验收；禁止图名特判与全局 nudge。

---

## 1. 问题升维（对照手册母题）

### 1.1 观测（已钉死，非观感臆测）

| 样例 | 观感 | 可复现事实 |
|------|------|------------|
| user-auth 下部 | 四线打结 | `db→auth` 回程与 `auth→cache` 去程 **共竖线 x=184**；auth 底边 4 槽挤在中段；层缝 ~60px；`exact_sev≈36`；正反向水平 gap 大致达标 |
| typical-microservice | 整体拥挤 | 服务底→Redis 顶 gutter ~44px；三服务→Postgres 落点间距≈gap；`user→postgres`/`user→kafka` 出口 gap≈2；6 条 Prometheus 外环 → `edge_crossing=15`；`tight_sev≈310` |

### 1.2 映射到手册语言（禁止降维成「再 enforce 一次」）

| 手册母题 | 本问题实例 | **不是** |
|----------|------------|----------|
| **假设独占共享资源**（§1.2） | 赋锚/stub 未查跨边占用 → 跨对共干 | 正反向同对 gap 失效（V3a 已管同对） |
| **孤立决策** | 每边独立 L，群体在 auth 底/ Postgres 顶形成梳状 | 单边 `simplify` 删坏了 lane |
| **检测成功却不修复**（§2.1） | Classify 可标 `NeedsSeparation`，C 阶段未对 **跨对 stub** 收口 | 缺一次 sanitize |
| **契约 / 空间预算** | 层缝养不下 FanIn+标签+监控外环 | 「线画歪了」 |
| **有意合流未兑现**（共线方案 §2） | architecture FanIn 允许 trunk，实际多干并列 | flowchart 也要强制合流 |
| **R12** | 合法台阶可暂留；勿无障拉直救拥挤 | 靠 `merge_overshoot` 消梳齿 |

### 1.3 四条因果链（唯一允许的修复轴）

```text
A  端口 / stub 资源排他
   同侧出边的 stub 竖(横)坐标不得被无关边占用（跨对共干）

B  层通道预算（布局写权，路由前）
   rank 缝 / space_budget 须覆盖：max(扇出梳齿, 标签带, 反馈外环占位)

C  语义合流兑现（architecture）
   FanIn/FanOut 组键交集为真 → Annotation 声明 merge 区间 + 分叉 stub；
   禁止「许可了却仍多干并列」且无 Degraded

D  长跨 / feedback 侧通道
   Prometheus 类边走已有 feedback/corridor 策略；不得与业务 FanIn 主缝抢同一视觉干线
```

四条轴 **正交**；禁止用「扩大 `enforce_reverse_pair_min_gap`」一条轴吞掉 A–D。

---

## 2. 目标形态（高维，对接已有旁路三件套）

复用 P1–P3 资产，**不**新建 `EdgeGeometryContract`：

```text
布局冻结（含层 gutter 预算）
  → 粗意图：slot demand / FanIn merge 候选 / feedback 侧通道需求
  → 初始路由 + C：Classify 驱动
        · 跨对 stub 去冲突（轴 A）
        · 语义 merge 写入 Annotation（轴 C）
        · lane / unrelated-trunk（既有）
  → 冻结 RouteAnnotation
  → D：snap → sanitize(overshoot) → gap 审计 → label（P3 写权）
```

| 约束 | 最终写者 | 手册依据 |
|------|----------|----------|
| 节点坐标 / 层缝 | **布局**（Sugiyama / architecture coordinate + space_budget） | §1：动坐标只在路由前；§5 V3b 后禁止路由挪节点 |
| stub / 端口占用表 | C：slot_replan / 新增 stub-occupancy 收敛 | §1.2 赋锚查占用 |
| 正反向同对 gap | C 预修 + D 审计（P3.1 已定） | 手册 §3.1；**不**扩成跨对万能药 |
| 语义 trunk | C：merge 声明 + lane；D 不得无验证抹掉 | 共线方案 §2、§5 |
| 标签 | D 末一次 resolve | P3.3 / 手册 §3.1 |

---

## 3. 红线（手册 + AGENTS）

1. **禁止图名特判**（`user-auth` / `typical-microservice` 不得出现在 `if`）。用通用谓词：同侧多边、跨对 stub 冲突、FanIn 组键、feedback 边集。  
2. **禁止**把 `enforce_reverse_pair_min_gap` 泛化为全局重叠修复器（共线方案明确禁止）。  
3. **禁止**最终任意 nudge / 主轴+Z 回潮。  
4. **禁止**为消 lint 而穿模；检测为真 → 修复或显式 `Degraded(reason)`。  
5. **正确性 vs 大重构拆 PR**；不与 O2 pendant 同 PR 改同一热点。  
6. 验证：`cargo run -p plotgram-cli`；门禁对比 `collinear-baseline-latest`（及本计划代表集）。  
7. R12：无障不强行拉直侧通道台阶来「好看」。

---

## 4. 代表集与门禁

### 4.1 代表集（观感 + 机制）

| ID | 文件 | 盯的机制 |
|----|------|----------|
| T1 | `flowchart/n.user-auth.pgm` | 跨对 stub 共干、底边扇出 |
| T2 | `architecture/n.typical-microservice-architecture.pgm` | gutter、FanIn、监控外环 |
| R* | 既有 `collinear-regression-set.txt` | 防退化 |

可加 1–2 张「同构最小图」（单 hub 底边 2 正反向对；三源一宿 FanIn）作单测夹具——**仍无图名分支**。

### 4.2 Quality / Perf（继承 collinear）

| 指标 | 门禁 |
|------|------|
| `node_fp` | 轴 A/C/D **默认不变**；仅轴 B（层 gutter）允许变，且须列出移动节点集合与原因 |
| `exact_sev` / `tight_sev` | 代表集 **不升**；T1 期望跨对 exact **下降** |
| `allowed_share_len` | architecture FanIn 合流兑现后可升（须抽检非误标） |
| `lint_hard` / crossings | T2：`edge_crossing` 目标下降或持平；hard error 不升 |
| `unrelated_trunk` | 不升 |
| `median_ms` | ≤ +10%（亚 10ms 用绝对 +5ms，见 compare-collinear） |
| `det` | true |

验收叙事：先 T1/T2 机制指标，再全量 collinear 集；**不得**用「这两张图好看了」代替严重度。

---

## 5. 分阶段计划（可回滚）

### Phase S0 — 证据钉桩（无行为变化）

| # | 任务 | DoD |
|---|------|-----|
| S0.1 | ✅ 将 T1/T2 纳入 `benchmark-data/congestion-set.txt` | `congestion-baseline` 可跑 |
| S0.2 | ✅ `collect_stub_occupancy` / `find_stub_occupancy_conflicts` | T1 修复前可报 exact 共柱；修复后 `stub_exact_cross_pairs=0` |
| S0.3 | ✅ `estimate_layer_band_demands`（**面距** gutter） | T2：`layer_bands_with_deficit≥1`，`max_layer_deficit≈22` |
| S0.4 | ✅ 附录 A 指针 + 基线 JSON | 见 §11 |

**出口**：机制可机器复现。  
**对齐手册**：先追时序与写权，再猜启发（§1.1）。

---

### Phase S1 — 轴 A：跨对 stub / 端口占用排他（优先打 T1）

**问题**：正反向守卫只看无向对；跨对共干是 **共享资源未查占用**。

| # | 任务 | DoD |
|---|------|-----|
| S1.1 | ✅ C：`stub_occupancy.rs` 占用表 + 冲突检测 | 单测 `detects_cross_pair_shared_stub_column` |
| S1.2 | ✅ 仅修 **跨对 exact 共柱**（`gap<1`）；平移较大 `edge_index`；`!semantic_merge` 才写边 | T1：auth→cache 离 184；collinear vs P3 **PASS** |
| S1.3 | ✅ 复用既有 `NeedsSeparation`（segment_pair）；stub 修复后 T1 `exact_cross=0` | 未新建图名分支 classify |
| S1.4 | ✅ Annotation `stub_occupancy_from/to` | C 末 freeze 写入 |

**落地约束（相对初稿收窄，防回滚触发）**：

1. **architecture（`semantic_merge`）仅诊断**：C 改 stub 会反馈 space-budget → `node_fp` 漂移；T2 跨对仍可见（`stub_cross_pair_conflicts>0`），留给 S2/S3。  
2. **不做整侧贪心 pack**：只分离 exact 共柱，避免全图 lane 扰动。  
3. 正反向同对仍只走 `enforce_reverse_pair_min_gap`。

**写权**：只在 C（`phase_lane` 末）；D 不新开 stub 挪动。  
**不做**：扩大 reverse-pair enforce；为 T1 改节点坐标。  
**回滚已验证**：全量 pack / architecture 启用 resolve → `node_fp` 变；收窄后 P3→S1 collinear **PASS**。

**手册锚点**：§1.2「赋锚查占用」；§2.2「检测为真须改善或 degraded」；V2 残余「共竖干」— flowchart 侧已收口。

---

### Phase S2 — 轴 B：层通道预算（优先打 T2 拥挤，路由前）

**问题**：拥挤首先是 **布局缝养不下边+标签+外环**，不是折点算法单独能救。

| # | 任务 | DoD |
|---|------|-----|
| S2.1 | ✅ `layout/edge_band_demand.rs`：邻层边数 / FanIn·FanOut 梳齿 / 标签带宽上界 | 单测数值稳定 |
| S2.2 | ✅ architecture **无分组**路径 `assign_coordinates`：`gap = max(LAYER_GAP, demand)`；flowchart / 有组 two_phase 暂沿用既有密度/adaptive（避免大图 perf / tight 回退） | T2：`max_layer_deficit=0` |
| S2.3 | ✅ 仅布局写权（`strategy.compute` 内）；路由后不补竖缝 | T1 `node_fp` 不变；T2 `node_fp` 可变且可解释 |
| S2.4 | ✅ congestion + collinear vs S1 **PASS**（`--allow-node-fp` 仅轴 B 需要时） | exact/tight 不升 |

**落地约束**：

1. 共用公式 + 图种 `EdgeBandDemandProfile`；architecture 系数略强于 flowchart。  
2. **有组 two_phase / sugiyama_v2** 未强制叠 demand（实测叠上后 k8s 类样例 median_ms / tight 易升）；模块 API 已就绪，S2.2b 可按代表集再开。  
3. `compare-collinear.sh --allow-node-fp`：轴 B 允许 `node_fp` 变，仍卡严重度与 perf。

**写权**：布局 / `assign_coordinates`（无组 architecture）。  
**不做**：路由后推节点腾缝；图名加大 padding。  
**手册锚点**：§1 动坐标只在路由前；§7 space_budget ROI（竖缝 ≠ 水平 space_budget）。

---

### Phase S3 — 轴 C：FanIn/FanOut 语义合流兑现（architecture）

**问题**：产品表允许 trunk，实现仍多干并列 → 「许可未兑现」。

| # | 任务 | DoD |
|---|------|-----|
| S3.1 | ✅ C 末 `merge_intervals` + `freeze_route_annotations_with_merges` | 可序列化 |
| S3.2 | ✅ `semantic_trunk_merge.rs`：FanIn 共享竖直 approach trunk + 短分叉；失败 `Degraded(MergeInfeasible)` | T2 Postgres 共干（~同 x） |
| S3.3 | ✅ D 验证已挂（P2）；有 `merge_intervals` 时 overshoot/snap 不可无验证拆掉 | 钩子复用 |
| S3.4 | ✅ flowchart / 有组 architecture **不**启用写者 | T1 不变；collinear vs S2 **PASS** |

**落地约束**：

1. **仅无分组 architecture FanIn**（2–5 边）；FanOut / 有组大图留 **S3.2b**（避免 tight / 穿模）。  
2. 写权：C（lane 后、Annotation 冻结前）；障碍用 `segment_intersects_node`。  
3. flowchart：`semantic_merge=false`，整段跳过。

**手册锚点**：共线方案产品表；「检测为真须修复或 degraded」。

---

### Phase S4 — 轴 D：监控 / 长跨反馈让道 ✅

**问题**：外环与业务主缝抢道 → crossings 爆炸。

| # | 任务 | DoD | 状态 |
|---|------|-----|------|
| S4.1 | 监控枢纽（同目标被动入边 ≥3）并入延后路由集；**不**强制改 L/R 端口（顶置 hub 强行侧通道会增交叉） | 与 R1/R3 一致 | ✅ |
| S4.2 | `side_channel_gutter` API 已入 `edge_band_demand`；architecture 系数暂 0（强制占位会抬层缝虚高） | 预算钩子就绪 | ✅ 钩子 |
| S4.3 | 无组 architecture：外环候选 + 交叉硬过滤（prefer_outer）+ FanIn trunk 穿越惩罚；S3 后重路由监控边 | T2 `edge_crossing` 15→11 | ✅ |

**门控**：与 S3 相同——`semantic_merge && groups.is_empty()`；有组大图不启，避免 collinear 回归。  
**不做**：删监控边；为监控边单独画贝塞尔「绕开」（风格分裂）。  
**回滚**：侧通道导致画布过宽 → 降 demand，接受部分交叉并标记 degraded。

---

### Phase S5 — 标签带宽（几何冻结后）✅

**问题（T2 观感残留）**：FanIn 共干 / 监控让道后，中轴竖廊里「路由请求」「上报指标」「读写…」等字仍叠在干线上——**不是**再改路径能一刀切的，而是层缝标签预算不足。

| # | 任务 | DoD | 状态 |
|---|------|-----|------|
| S5.1 | architecture `label_band` 24→32、`label_per_edge` 2→4、`max_extra` 40→48 | T2 `max_layer_deficit=0`；轴 B `node_fp` 可变 | ✅ |
| S5.2 | D 末避让余量：试过全局 `DEFAULT_LABEL_PERP_OFFSET` 8→10 → `layout-stress-nested` 出现 `label_node_overlap` error，**已回滚** | 不引入 collinear `error_count` 上升 | ✅ 回滚；无组专用余量留 S5.2b |
| S5.3 | congestion T1/T2 + `compare-collinear --allow-node-fp` vs S4 | **PASS**；T2 `label_label_overlap` 4→2 | ✅ |

**写权**：仅布局（S2 系数）；C/D **不**为标签挪折点。  
**不做**：在 resolve 里改路径；全局抬 perp（已验证伤有组图）；图名加大 padding。  
**手册锚点**：sanitize 重建 label → 避让必须在最终几何之后（P3.3）。

**S5.2b（可选）**：若 T2 字叠仍不可读，再加 **无组 architecture** 专用候选侧向余量（`LabelPlacementConfig`），有组大图不启。

---

### §5.1 剩余 backlog（S5 之后）

按优先级；**禁止**与已完成阶段同 PR 混开大闸。

| ID | 内容 | 触发条件 | 风险 |
|----|------|----------|------|
| **S5.2b** | 无组 architecture 专用标签侧向余量 | T2 目视字仍糊 | 勿波及有组 nested |
| **S4.x** | 监控让道加强 ✅ | 虚线仍与业务实线共竖廊 / 穿模 | 见下方验收 |
| **S2.2b** | 有组 `two_phase` / sugiyama 叠 `edge_band_demand` | 有组 showcase 面距 deficit 仍高 | k8s 类 median_ms / tight 易升 |
| **S3.2b** | FanOut 合流；有组 FanIn 写者 | 有组图「许可未兑现」可机器复现 | 穿模 / tight |
| **collinear P4** | 同通道多条 `NeedsSeparation` 全局 lane | S1+S3+S5 后数据仍证同通道多干 | 扰动大；勿抢跑 |

### Phase S4.x — 监控让道加强 ✅

| # | 任务 | DoD | 状态 |
|---|------|-----|------|
| S4.x.1 | architecture `side_channel_*` 小开（0.4 / 12 / max20） | 无组侧廊占位；有组不消费 | ✅ |
| S4.x.2 | scoring：trunk 罚↑、外环 bonus↑；prefer_outer 时 obstacle/overlap 加重 | 监控边少抢 FanIn 竖廊 | ✅ |
| S4.x.3 | 同排侧廊被堵 → Top↔Bottom 逃逸端口；外环候选「离排再绕」+ 深 stub 收束 | sanitize 不再吞 U 形外环 | ✅ |
| S4.x.4 | sanitize 后 `force_outer_escape_path` 兜底；`reroute` 含穿入 moved 节点的边 | T2 `edge_through_node`→0 | ✅ |
| S4.x.5 | congestion + collinear vs S5 | **PASS**（`--allow-node-fp`） | ✅ |

**根因（T2 postgres→prometheus）**：外环候选本干净，但以「沿 Bottom 端口边横走」收束时，`ensure_outward_stub` 弹出平面折点 → simplify 与上游共线 → U 形被收成中轴竖爬穿 `order_svc`。修复是 **路径收束到 `PORT_CLEARANCE+NODE_OBSTACLE_PAD` 之外**（非全局放宽 stub 弹出——后者会伤有组大图）。

**明确不做**：全局改 `ensure_outward_stub` 弹出阈值；图名特判；有组启强制外环。

**明确暂缓**：再扩 `enforce_reverse_pair`；路由后 nudge 节点；无障拉直梳齿（R12）；全局抬 `DEFAULT_LABEL_PERP_OFFSET`。

---

## 6. PR 切片建议

| PR | 内容 | 依赖 |
|----|------|------|
| PR-S0 | 诊断 + 代表集 | 无 |
| PR-S1 | StubOccupancy + 跨对去冲突 | S0；collinear P1–P3 |
| PR-S2 | edge_band_demand + gutter | S0；**勿与 O2 同 PR** |
| PR-S3 | FanIn merge 兑现 + Annotation | S1（占用表） |
| PR-S4 | feedback 让道 | S2 预算 |
| PR-S5 | 仅调 label 带宽系数（若需要） | S2–S4 |

每 PR：手册 §1.3 DoD + `compare-collinear` + T1/T2 机制断言。

---

## 7. 明确不做什么（防回流）

| 诱惑 | 拒绝理由 |
|------|----------|
| 再挂一轮全图 `enforce_reverse_pair_min_gap` | 写权已收敛；管不了跨对；手册禁止泛化 |
| 对 user-auth 特判「auth 底边强制错开」 | 图名特判；应用 StubOccupancy 通用规则 |
| 路由后 nudge 节点腾缝 | 手册：路由后禁止挪节点；应走 S2 |
| 无障拉直梳齿 | R12 |
| 新建大契约壳再修 | 手册废止项；旁路 Annotation 已够 |
| P4 全局 lane 抢跑 | 仅当 S1+S3 后数据仍显示「同通道多条 NeedsSeparation」再开（collinear P4 触发条件） |

---

## 8. 与既有轨道的关系

| 轨道 | 关系 |
|------|------|
| collinear P0–P3 | **地基**（Measure/Classify/Annotation/写权）；本计划消费它们，不重做 |
| collinear P4 | **可选后续**；本计划 S1/S3 可能消除部分 P4 需求 |
| 手册 V2 残余 | S1 正式收口「共竖干 / 跨对」 |
| 手册 V3b / O2 | S2 改 coordinate/gutter 时拆 PR，避免纠缠 |
| 开放项 O5.2 | 若未来要消台阶，须障感知；本计划不依赖 |

---

## 9. 建议执行序

1. ~~**S0**~~ ✅  
2. ~~**S1**~~ ✅  
3. ~~**S2**~~ ✅  
4. ~~**S3**~~ ✅（无组 FanIn；有组 / FanOut = S3.2b）  
5. ~~**S4**~~ ✅（无组监控延后 + 外环/干线；侧 gutter 系数暂 0）  
6. ~~**S5**~~ ✅（architecture 标签带宽；全局 perp 试过已回滚）  
7. ~~**S4.x**~~ ✅（侧 gutter 小开 + 外环深 stub + 穿模兜底；T2 `edge_through_node`→0）  
8. 目视 T2：字仍糊 → **S5.2b**  
9. 有组大图喊挤 → **S2.2b** / **S3.2b**  
10. 数据仍证同通道多条 `NeedsSeparation` → collinear **P4**

---

## 10. 成功标准（产品语言）

- **T1**：auth 底边可逐条追踪。 ← **S1**  
- **T2**：服务↔数据面距达标（S2）；Postgres 入边语义共干（S3）；监控外环 `edge_crossing` 15→11（**S4**）  
- **T2 标签**：`label_label_overlap` 4→2（**S5**）；目视仍糊则开 S5.2b  
- **T2 穿模**：**S4.x** 后 `edge_through_node`→0；外环绕行后 `edge_crossing` 可回升（约 15，可接受）  
- **全局**：collinear S5→S4.x **PASS**（`--allow-node-fp`）

---

## 11. 附录 A — 基线指针（2026-07-15）

| 产物 | 路径 |
|------|------|
| congestion S4.x | `benchmark-data/congestion-baseline-s4x-2026-07-15.json` |
| collinear S4.x / latest | `benchmark-data/collinear-baseline-s4x-2026-07-15.json` → `collinear-baseline-latest.json` |
| congestion/collinear S5 | `*-s5-2026-07-15.json`（对比基线） |
| 监控/外环 | `feedback_side::monitor_hub_*`；`path::push_target_approach` / `force_outer_escape_path`；`scoring` trunk/outer；`run` S3 后 reroute + sanitize 后 escape |

**T2 Postgres FanIn（S3）**：三服务 approach 共享竖直干线，再分叉到 Compact 落点；`merge_intervals` 写入 Annotation。

**S5 系数**：architecture `label_band=32` / `label_per_edge=4` / `max_extra=48`。

**S4.x 系数**：`side_channel_scale=0.4` / `base=12` / `max=20`；外环收束深距 = `PORT_CLEARANCE + NODE_OBSTACLE_PAD + 8`。
