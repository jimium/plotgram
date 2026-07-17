# 共线 / 重叠 / 合流方案 — 开发计划

> 日期：2026-07-15  
> 方案：[`collinear-problem-analysis-2026-07.md`](./collinear-problem-analysis-2026-07.md)  
> 手册：[`布局与路由核心手册-2026-07.md`](../../总结经验/布局与路由核心手册-2026-07.md)  
> 既有开放项：[`开放项执行计划-2026-07.md`](../../总结经验/开放项执行计划-2026-07.md)（O0 基线缺口、O6.5 与本计划 Phase 1 对齐）  
> 状态：**P0–P3 ✅**（2026-07-15）；拥堵修正 **S0–S5 + S4.x + S5.2b ✅**（见 [`congestion-remediation-plan-2026-07.md`](./congestion-remediation-plan-2026-07.md)）；其后可选 S2.2b / S3.2b；**P4 暂缓**

---

## 0. 目标与红线

**目标**：按方案 Phase 0→4 推进 Measure/Classify/Annotation/写权收敛；在每步可证明「质量不降、性能可控」。

**红线**：

1. 禁止图名特判；禁止空壳 `EdgeGeometryContract`；旁路注解优先。  
2. 正确性改动与大重构 **拆 PR**；不与 O2（pendant）同 PR 改同一热点文件。  
3. 验证一律 `cargo run -p plotgram-cli` / `cargo run -p plotgram-core --bin bench-phases`，不信陈旧 release binary。  
4. 仓库既有失败（开放项附录 A）先钉死，**不得与本计划失败混谈**；本计划只看「失败集是否扩大」。  
5. 性能：代表集 **median 退化 ≤10%**；单图 >100ms 的样例额外盯控；超阈值必须解释或回滚。

---

## 1. 监测体系（先于改行为）

现有能力与缺口：

| 能力 | 现状 | 本计划动作 |
|------|------|------------|
| Lint 摘要 + bench + SVG 确定性 | `benchmark-data/snapshot-phase0.sh` + `phase0-regression-set.txt` | **每 Phase 出口必跑**；归档 `benchmark-data/collinear-phaseN-YYYY-MM-DD.md` |
| OrthoDebugStats | `edge_exact_overlap_pairs` / `edge_tight_spacing_pairs` / reroute 计数 | Phase 0 纳入快照字段 |
| 全量重叠**严重度** | 开放项 O0.3 **缺口** | Phase 0 **必补**脚本（见 §1.1） |
| 节点坐标 hash | 开放项附录 B 代表图 | Phase 0 扩展为回归集节点中心 fingerprint |
| 路径写入归因 | 无 | Phase 0：可选 `RouteEditTrace`（feature / debug 开关，默认关） |

### 1.1 质量基线指标（Quality Gate）

每个 PR / Phase 出口对比 **同一 commit 钉下的基线快照**：

| 指标 | 定义 | 门禁 |
|------|------|------|
| `node_fp` | 全图节点 `(id, cx, cy)` 稳定序列 hash | **必须不变**（除非 Phase 明确授权且可解释） |
| `exact_sev` | 非 stub、经 Classify（或 Phase0 临时：非 stub 段）的 exact overlap：Σ `shared_length` | **不升**；Allowed 合流单独记账不进此桶 |
| `tight_sev` | 同上，gap &lt; `parallel_gap` 的 Σ `(gap_deficit × overlap_len)` | **不升** |
| `allowed_share_len` | 合法合流共享长度总和 | 可升；不可「误标 Allowed 掩盖 exact」 |
| `lint_hard` | `edge_through_node` / `edge_crosses_group_interior` / NodeOverlap 等 Error | **不升** |
| `unrelated_trunk` | lint `UnrelatedEdgeTrunkMerge` 计数（architecture） | Phase1+ 与 Classify 对齐后 **不升** |
| `degraded_edges` | Ortho / refine degraded 计数 | **不升**（或升则必须有 Degraded reason 分布说明） |
| `det` | 同输入连续 2 次 SVG hash | **必须一致** |

> 「严重度不升」优先于「违规条数」：一条长共干恶化比多条短碰更严重。

### 1.2 性能基线指标（Perf Gate）

| 指标 | 工具 | 门禁 |
|------|------|------|
| `median_ms` | `bench-phases <file> 5`（或 7） | 相对基线 **≤ +10%** |
| 回归集合计 median | `snapshot-phase0` 同款样例 | 合计 **≤ +10%** |
| 热点图 | `c.k8s-tenant-isolation`、`c.k8s-platform-stack`、`c.layout-stress-nested`、高边密度 architecture | 单图超阈 → 剖析 reroute 次数 / Classify 调用量后再合 |
| Trace 开销 | `RouteEditTrace` 开启时 | **默认关闭**；开启仅诊断，不进 release 热路径 |

**性能预算原则**（与手册 / AGENTS §4 一致）：

- Classify 必须 **O(段对数可索引)**：复用 `SegmentGrid`，禁止全量边×边无索引扫描进热路径。  
- `validate_route_edit` 只挂在「改形状」路径；严格共线删点不跑全量验证。  
- 新增守卫 **不得**无证据地再加一轮全图 enforce；先看 Phase 0 trace。

### 1.3 Phase 0 必交付工具

新建（建议落点，实现时可微调）：

```text
crates/plotgram-cli 或 plotgram-core bin：
  collinear-baseline   # 或扩展 plotgram lint / 新 subcommand
scripts / benchmark-data：
  snapshot-collinear.sh   # 包一层：质量表 + 调用 snapshot-phase0 性能段
  compare-collinear.sh    # 对比两份 JSON/MD：质量门禁 + perf 门禁，非零退出码
```

**Phase 0 出口产物**（钉死一份，后续 PR 对比）：

- `benchmark-data/collinear-baseline-YYYY-MM-DD.{md,json}`  
- 字段至少含：每样例 `node_fp`、`exact_sev`、`tight_sev`、`allowed_share_len`（Phase0 可先=0）、`lint_*`、`ortho exact/tight`、`median_ms`、`det`  
- 记录 `cargo test -p plotgram-core --lib` 失败集 ⊆ 开放项附录 A

---

## 2. 总序与 PR 切片

```text
P0  监测基建 + 钉基线          ← 无行为变化（或仅 debug 开关）
P1  Measure 收口 + Classify     ← 行为：报告/统计变准；路由几何默认不变
P2  Annotation + 验证钩子       ← 仅后处理回退变安全；几何可变但门禁卡住
P3  写权收敛（删重复 guard）    ← 须 P0 证明「谁在毁 gap」
P4  全局 lane（可选）           ← 仅数据证明需要时
```

| Phase | 建议 PR 数 | 可否与其它轨并行 |
|-------|------------|------------------|
| P0 | 1 | 可与文档/O2 并行 |
| P1 | 1–2（API + lint 接入可拆） | 对齐开放项 **O6.5**；勿与 O1 类正确性热点混改 |
| P2 | 1–2（Annotation 写入 / sanitize·snap 挂钩） | 勿与 architecture coordinate 大改同 PR |
| P3 | 1 | **依赖 P0 证据**；单独 PR |
| P4 | 0–N | 可选；另开里程碑 |

---

## 3. Phase 明细

### P0 — 监测与基线（开工门禁）

| # | 任务 | DoD |
|---|------|-----|
| P0.1 | 实现重叠严重度聚合（先几何 stub 豁免；Allowed 桶 Phase1 再接） | 脚本对回归集可复现 |
| P0.2 | `node_fp` + 确定性 + 接入/扩展 snapshot | JSON+MD 落盘 |
| P0.3 | 性能段：复用 `bench-phases`，写入同一 baseline 文件 | 与 `phase0-latest` 可对照 |
| P0.4 | （可选）`RouteEditTrace`：阶段标签 + 边 index + 前后点摘要；`PLOTGRAM_ROUTE_EDIT_TRACE=1` | 默认零开销 |
| P0.5 | 文档：本 baseline 路径写入方案/本计划「基线指针」 | 指针更新 |

**出口**：基线文件合并；`compare` 对自身 diff 为 0；**无算法行为变化**（P0.4 除外且默认关）。

**回滚**：脚本问题只修脚本，不改路由。

---

### P1 — Measure + Classify

| # | 任务 | DoD |
|---|------|-----|
| P1.1 | 抽稳 `SegmentPairMeasure`；`segments_violate_spacing` 等只产出 Measure | 单测：重合/紧间距/正交交/T 接 |
| P1.2 | `classify_segment_pair`：stub、正反向、`edges_may_share_trunk`、profile | 矩阵用例（方案 §2 产品表） |
| P1.3 | lint `UnrelatedEdgeTrunkMerge` + OrthoDebug 统计改走 Classify | 合法 bundle 不进 unrelated；门禁 `exact_sev`/`unrelated_trunk` |
| P1.4 | scoring / X-1 **先只读** Classify（或并行断言），默认不改选路 | `node_fp` 不变；`median_ms` ≤+5%（仅分类开销） |

**出口**：报告语义正确；**路由几何与 P0 baseline 的 `node_fp` 一致**；perf ≤+5%（P1 目标严于总阈值，因几乎无几何收益）。

**回滚**：Classify 仅用于 lint/stats 的 PR 可先合；一改热路径超 perf → 回退热路径接入。

---

### P2 — Annotation + `validate_route_edit`

| # | 任务 | DoD |
|---|------|-----|
| P2.1 | C 末旁路 `RouteAnnotation`（stub / lane / merge 区间）；不改 `EdgeLayout` | 单测可序列化关键字段 |
| P2.2 | `validate_route_edit`：穿障、stub、越 merge/lane 边界 | 失败回退单测 |
| P2.3 | 挂钩：`collapse_micro_jogs` / `merge_overshoot` / 量化后处理 | R12：无上下文不强行拉直；`lint_hard` 不升 |
| P2.4 | 全量门禁 | `node_fp` 不变或可解释；`exact_sev`/`tight_sev` 不升；`median_ms` ≤+10% |

**出口**：后处理更安全；严重度不升；性能达标。

**回滚**：验证失败率过高导致台阶暴增 → 收窄挂钩面（先只 overshoot / 量化），保留 P2.1。

---

### P3 — 写权收敛

**前置**：P0.4 或等价 dump 证明重复 `enforce_reverse_pair_min_gap` / sanitize 的必要性。

| # | 任务 | DoD |
|---|------|-----|
| P3.1 | 收敛正反向 gap：C 预修 + D 一次审计 | 代表图 gap 不回归；调用次数有文档 |
| P3.2 | 等价 simplify 与改形状路径分离；公共严格共线模块 | 删除重复实现前 fingerprint 对比 |
| P3.3 | 标签只在最终几何后一次 | 手册时序一致 |

**出口**：无「无证据连环 guard」；质量/perf 双门禁绿。

**回滚**：去掉某次 enforce 后 `exact_sev` 升 → 恢复该次并记入「证据：仍需保留」。

---

### P4 — 全局 lane（可选）

**触发**：P0/P1 数据说明高密度残余主要来自「同通道多条 `NeedsSeparation` trunk」，且 C 阶段 lane+X-1 已打满。

| # | 任务 | DoD |
|---|------|-----|
| P4.1 | 以 Annotation lane demand 做 track 分配设计评审 | 书面方案 + 复杂度 |
| P4.2 | 实现 + 不可行 → `Degraded` | 严重度降或持平；perf 单独预算（可放宽到 +15%，须评审） |

不做：最终任意 nudge 回潮。

---

## 4. 每 PR 合并检查单

```text
[ ] 对照方案 + 手册 §1–§3
[ ] 无图名特判；无 EdgeLayout 大换血
[ ] 钉死/对比 collinear-baseline（质量）
[ ] snapshot / bench-phases（性能，回归集 + 热点图）
[ ] node_fp 不变或变更可解释
[ ] exact_sev / tight_sev / lint_hard 不升
[ ] median_ms ≤ +10%（P1 目标 ≤ +5%）
[ ] 确定性 det = yes
[ ] cargo test -p plotgram-core --lib：失败集未扩大
[ ] 正确性 vs 重构拆分；未在 router 内开激进 merge_overshoot
[ ] 新热路径有复杂度说明（Grid / 无 N² 裸扫）
```

---

## 5. 样例集建议

**门禁集**（每次 PR）：`benchmark-data/phase0-regression-set.txt` 全文。

**热点加跑**（perf + 共干）：

- `showcase/architecture/c.k8s-tenant-isolation.pgm`
- `showcase/architecture/c.k8s-platform-stack.pgm`（若集内已有可只跑一次）
- `showcase/architecture/c.layout-stress-nested.pgm`
- `showcase/architecture/n.typical-microservice-architecture.pgm`（若存在）
- flowchart 代表：含同侧多入/出边的用户图（与开放项 auth 类单测互补）

**正确性钉扎单测**（已有则扩展，勿图名分支）：

- 正反向 trunk gap（O1 已有）
- `edges_may_share_trunk` true/false 对
- Classify 矩阵：Allowed stub / Allowed merge / NeedsSeparation / 正反向 Forbid

---

## 6. 风险与缓解

| 风险 | 缓解 |
|------|------|
| Classify 变热路径拖慢大图 | P1 先 lint-only；接入 reroute 前做调用计数；Grid 查询 |
| validate 过严 → 微折回退过多 | 分挂钩；度量「回退率」；R12 接受合法台阶 |
| Allowed 误标吞掉重合 | `allowed_share_len` 与 `exact_sev` 分桶；抽检 bundle 图 |
| 与 O2/group-frame 坐标漂移纠缠 | `node_fp` 门禁；不同 PR；漂移先归因再动路由 |
| 重复 baseline 脚本分裂 | collinear 快照 **扩展或包装** phase0，不复制第三套无文档工具 |

---

## 7. 进度表（勾选）

| Phase | 状态 | 基线指针 |
|-------|------|----------|
| P0 监测+钉基线 | **完成**（2026-07-15） | 已被 P1 基线接替；质量门禁字段已建立 |
| P1 Measure+Classify | **完成**（2026-07-15） | [`collinear-baseline-p1-2026-07-15.json`](../../../benchmark-data/collinear-baseline-p1-2026-07-15.json) |
| P2 Annotation+验证 | **完成**（2026-07-15） | [`collinear-baseline-p2-2026-07-15.json`](../../../benchmark-data/collinear-baseline-p2-2026-07-15.json) |
| P3 写权收敛 | **完成**（2026-07-15） | [`collinear-baseline-p3-2026-07-15.json`](../../../benchmark-data/collinear-baseline-p3-2026-07-15.json) / [`…-latest.json`](../../../benchmark-data/collinear-baseline-latest.json) |
| P4 全局 lane | 未触发 | — |

### P0 落地摘要

| 交付 | 路径 |
|------|------|
| 严重度聚合 | `crates/plotgram-core/src/layout/metrics/collinear.rs` |
| 采集 bin | `cargo run --release -p plotgram-core --bin collinear-baseline` |
| 样例集 | `benchmark-data/collinear-regression-set.txt` |
| 快照 | `./benchmark-data/snapshot-collinear.sh` |
| 门禁对比 | `./benchmark-data/compare-collinear.sh <base> <cur>` |

### P1 落地摘要

| 交付 | 路径 |
|------|------|
| Measure + Classify | `crates/plotgram-core/src/layout/edge/segment_pair.rs` |
| lint 改走 Classify | `layout/lint` → `NonSemanticTrunk` exact only |
| 基线分桶 | `exact_sev`/`tight_sev` = NeedsSeparation；`allowed_share_len` = 语义合流 |
| scoring 几何 | `classify_parallel_pair` 委托 `measure_segment_pair`（行为对齐） |

P0→P1 门禁：全样例 `node_fp` 不变；`exact_sev` 下降（合法合流入 `allowed_share_len`）；`unrelated_trunk` / lint error 不升；perf 持平。

### P2 落地摘要

| 交付 | 路径 |
|------|------|
| 旁路 Annotation | `crates/plotgram-core/src/layout/edge/route_annotation.rs`；C 末写入 `LayoutHints.route_annotations` |
| `validate_route_edit` | 端点 / stub / 正交 / 显式 merge 区间 / 可选穿障；严格共线可跳过 |
| 挂钩面（收窄后） | D：`merge_overshoot` sanitize；`snap_edge_waypoints` 量化后 simplify。router 内保守 sanitize **不**挂验证 |
| 穿障硬回退 | API+单测具备；**未**挂热路径（挂上会抬高 trunk 严重度）— P3 按证据再开 |

经验：校验注解必须取自**当前**几何（C 冻结的 start/end 经 space-budget/snap 后过期）；启发式 `protected_runs` 只观测、不硬回退。

P1→P2 门禁：`compare-collinear.sh` PASS（`node_fp` 不变；严重度/lint 不升；`median_ms` ≤+10%）。

### P3 落地摘要

| 交付 | 路径 / 结论 |
|------|-------------|
| P3.1 正反向 gap | **C** `phase_lane` 末预修 + **D** `pipeline` 一次审计；删除 4g sanitize 后重复 enforce。证据：去重后门禁绿，O1 单测仍过。D 审计改用 `parallel_gap_for_diagram`（architecture=12px） |
| P3.2 严格共线公共模块 | `layout/edge/common/collinear_simplify.rs`；正交 `simplify_path` 与 `grid_snap` 量化简化共用；改形状仍在 `collapse_micro_jogs` |
| P3.3 标签一次 | 去掉 router `phase_labels`；仅 pipeline 几何冻结后 `resolve_label_overlaps` |

调用次数文档：

```text
enforce_reverse_pair_min_gap:
  C  phase_lane 末          ← 预修（含 2 点直连）
  D  pipeline sanitize 后   ← 唯一审计
  ✗  phase_sanitize 后      ← 已删（无证据连环）

label resolve:
  ✗  router phase 5         ← 已删（会被 D sanitize 丢掉）
  D  pipeline 末            ← 唯一

sanitize:
  C  merge_overshoot=false  ← 保守
  D  merge_overshoot=true   ← 激进 + P2 验证钩子
```

P2→P3 门禁：质量 PASS；perf 持平（亚 10ms 样例改绝对 +5ms 容差，避免噪声误杀）。

---

## 8. 与开放项的关系

| 开放项 | 关系 |
|--------|------|
| O0.3 严重度脚本待补 | **由 P0.1 关闭缺口** |
| O6.5 scoring 分类统一 | **并入 P1**，完成后勾 O6.5 |
| O5.1 R12 文档 | P2 用验证钩子可执行化；仍不强行无障拉直 |
| O1 已完成 | P3 收敛 enforce 时不得破坏 O1 gap 单测 |
| O2 未完成 | 禁止与 P2/P3 同 PR 改 architecture 坐标路径 |

---

## 9. 建议执行序（更新 2026-07-15）

1. ~~P0 监测+钉基线~~ **完成**  
2. ~~P1 Measure+Classify~~ **完成**  
3. ~~P2 Annotation+验证~~ **完成**  
4. ~~P3 写权收敛~~ **完成**（见 `collinear-baseline-p3-*`）  
5. ~~拥堵修正 S0–S5~~ **完成**（见 congestion 计划）  
6. 可选：**S5.2b**（无组标签侧向余量）、**S2.2b / S3.2b**（有组）— 见 congestion §5.1；**S4.x** 已完成
7. **P4**（可选）：仅当数据仍证明同通道多条 `NeedsSeparation` trunk 时再开
