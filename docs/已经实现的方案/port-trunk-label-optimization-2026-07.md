# 端口 · 共干 · 标签归属：后续优化方案

> 日期：2026-07-16  
> 触发观感：`flowchart/n.user-auth`（DB 出入共端点）、`architecture/n.typical-microservice-architecture`（Prometheus 多箭）、`architecture/n.microservices`（有组 Postgres 未合流 / 标签歧义 / 客户端不对称）  
> 前置：[`congestion-remediation-plan-2026-07.md`](congestion-remediation-plan-2026-07.md)（S0–S5 / S4.x / S5.2b / S2.h ✅）  
> 手册：[`布局与路由核心手册-2026-07.md`](../总结经验/布局与路由核心手册-2026-07.md)  
> 开放项：[`开放项执行计划-2026-07.md`](../总结经验/开放项执行计划-2026-07.md)（O1 ✅ / O2 部分）  
> 状态：**A ✅ · B.1/B.2 ✅ · C ✅ · D ✅**；2026-07-16 二次复盘已收口语义角、走廊 stub 与 rank 缝

---

## 0. 一句话

S2.h 解决的是「挤」；本方案解决的是「端点选错 / 该合不合 / 标签无归属 / 列位不对称」。这不是单一 bug，而是 **多阶段局部最优叠加后的系统症状**——要按机制分层修，禁止图名特判。

---

## 1. 先回答：bug、设计缺陷、不正交，还是逻辑矛盾？

### 1.1 总判

| 类别 | 含义（本仓库语境） | 本轮问题里有没有 |
|------|-------------------|------------------|
| **实现 bug** | 代码意图与实际行为不符；门禁/注释说了 A，跑出来是 ¬A | 少数残留（O2.3 质心漂移属此类嫌疑） |
| **设计缺陷 / 范围缺口** | 意图明确，但覆盖面故意或遗漏地偏窄，导致合法场景未兑现 | **主因**（有组 FanIn 无写者、标签无强制归属） |
| **不正交（阶段/写权纠缠）** | 多模块各自合理，但无统一契约；后步覆盖前步或目标冲突 | **次主因**（选侧 vs slot vs lane；hub pack vs group-frame） |
| **逻辑矛盾** | 同一语义下两套规则互相否定，无法同时成立 | **基本没有**；监控「不合业务干」是权衡，不是自相矛盾 |

**结论**：当前观感问题 **主要不是实现写错**，也 **不是规则逻辑上无法共存**；而是：

1. **设计范围缺口**（该做的合流/归属没做全）；  
2. **管线不正交**（多阶段各优化局部目标，缺少「端点独占 / 合流许可 / 标签锚」的全局契约）；  
3. 外加少量 **实现未收口**（O2.3）。

监控外环与业务 FanIn 分流，属于 **显式产品权衡**，不要当成 bug 去「全部合并」。

### 1.2 逐条归类（对照三张图）

| 观感 | 样例 | 归类 | 说明 |
|------|------|------|------|
| 用户数据库出入边共端点 | `n.user-auth` | **不正交 + 设计缺口** | 正反向有 gap/offset（O1 ✅），但 **选侧阶段**仍可把请求/响应挤到同侧；slot 用 `is_from` 分桶禁止并线，却不保证「对边分置」。意图是错开，缺的是 **同节点双向对的侧向契约**。 |
| Prometheus 多箭「应合并」 | `n.typical-microservice` | **设计权衡（非 bug）** | S4 故意：Passive hub ≥3 → 外环、不进 S3 FanIn，避免与业务干线抢廊。若产品要「少箭头」，应另开 **监控局部 trunk**，而不是取消外环或当实现错误修。 |
| PostgreSQL 入边可合并却未合 | `n.microservices` | **设计范围缺口（S3.2b）** | 政策层 `edges_may_share_trunk` 允许；几何写者仅 **无组 architecture**。有组图「许可未兑现」——文档已标 S3.2b，不是跑偏。 |
| 「发布订单事件」看不出对应边 | 同上 | **设计缺口** | D 末有候选避让与条件引线，但 **无「归属不清则强制 leader」契约**；贪心放置在平行/近邻时会产生视觉歧义。 |
| 移动客户端怪折 / 与 Web 不对称 | 同上 | **不正交 + 实现未收口（O2）** | hub pack 意图对称；group-frame / 层平移可在其后改 x（~8px），路由只能跟着弯。属布局写权未冻结，不是路由器偏心。 |

### 1.3 为何容易误判成「实现 bug」

- 单看一张 SVG，像是「合并忘了写」「端点算错」。  
- 对照源码与 congestion / O* 文档后：多数是 **阶段目标不同** 或 **写者未覆盖某类图**。  
- 真正危险的是用「再 merge 一次 / 再 nudge 一次」当 bugfix——会破坏已收敛的 S3/S4 语义。

---

## 2. 问题升维（禁止降维）

| 手册母题 | 本轮实例 | **不是** |
|----------|----------|----------|
| 端口资源未排他 | DB 顶边出入共锚 | 再拉大 `NODE_GAP` |
| 有意合流未兑现 | 有组 `*→db` 并列竖干 | 无组 FanIn 再调系数 |
| 监控与业务争廊 | Prometheus 多箭外环 | 把监控并进 Postgres 干线 |
| 标签无最终归属写者 | 字漂在两缝之间 | router 内提前 resolve（会被 sanitize 丢） |
| 布局契约被后步扰动 | web/mobile 对 gateway 不对称 | 给 mobile 特判折点 |

---

## 3. 优化轨道（按机制，不按图名）

### 轨道 A — 端口侧 / 正反向同侧共锚 ✅

**目标**：同节点上请求↔响应不得视觉共锚；TB 堆叠对优先 **同侧切向错开**（对边分置往往不自然）。

**根因修正（执行后）**：user-auth 的选侧（Bottom↔Top）合理；共端点来自路径后处理把正反向 dock 压到同一切向。`enforce_reverse_pair_min_gap` 只保干线缝，不保落点。

| # | 任务 | DoD | 状态 |
|---|------|-----|------|
| A.1 | 谓词：正反向对 + 同节点同侧 | 单测覆盖 user-auth | ✅ |
| A.2 | `enforce_reverse_pair_dock_separation`：切向 ≥ max(parallel_gap, COMPACT_SLOT_PITCH) | DB/cache 顶边双锚可辨 | ✅ |
| A.3 | C 末 + D 末（sanitize 之后）双写，落点最终写者在 D | O1 trunk gap 不回归 | ✅ |

**不做**：为 `user-auth` 硬编码；强制把返回边改到 Left/Right。

**归类定位**：补 **落点契约**（不正交收口：干线缝 ≠ 锚点缝）。

---

### 轨道 B — 语义合流写者扩面（S3.2b 有组 FanIn）✅

**目标**：政策已允许的合流，在有组 architecture 上兑现；监控保持与业务干线分工。

| # | 任务 | DoD | 状态 |
|---|------|-----|------|
| B.1 | 去掉「有组跳过」门；穿模仍 degraded | `n.microservices` db FanIn 共干 | ✅ |
| B.1b | 合流后目标侧 **共锚**（同 trunk_x） | 双入边共箭头落点 | ✅ |
| B.2 | S4 外环后按目标端口侧做监控局部 trunk；相同文案去重 | 不抢业务廊、Prometheus 同侧少箭头 | ✅ |
| B.3 | FanOut 合流另切片 | 另切片 | 待开 |

**明确不做**：取消 `prefer_outer`；flowchart 默认开 trunk。

**二次复盘修正**：

- 合流写者不再无条件重造整条路径：同 rank / 同出侧 pendant 在源 stub 后对称合流；其它 FanIn 仅替换目标附近 suffix，保留既有走廊与绕障前缀。
- B.2 必须位于 S4 / S4.x 之后，否则外环重路由会覆盖监控 trunk。
- 同一显式 `MergeInterval` 上的相同标签只保留一份，避免共享干附近重复“上报指标”。

---

### 轨道 C — 标签归属契约 ✅

**目标**：读者无法用「最近路径」判断归属时，必须有可见锚（leader）。

| # | 任务 | DoD | 状态 |
|---|------|-----|------|
| C.1 | ±90° 旋转标签用轴对齐尺寸算可见引线长度 | 竖直字不再被「假贴线」吞掉 leader | ✅ |
| C.2 | Pass3：己边净空大 / 与邻边几乎等距 → 强制 `leader_to` | `n.microservices`「发布订单事件」有引线 | ✅ |
| C.3 | 写权仅 D 末 `assign_leader_lines` | 符合手册 P3.3 | ✅ |

**不做**：为消歧改折点拓扑。

---

### 轨道 D — 客户端↔hub 几何冻结（O2.3）✅

**目标**：pendant 对称 pack 之后，group-frame 不得留下稳定的 hub–client 质心差。

| # | 任务 | DoD | 状态 |
|---|------|-----|------|
| D.1 | 钉死写者：`resolve_all_sibling_overlaps` 后质心漂 | dump delta | ✅ |
| D.2 | `phase_d` 在 group_frame 后重跑 `align_client_nodes_to_hubs` + `center_group_hub_nodes` + expand | `\|cx(hub)-质心\|≤2px` | ✅ |
| D.3 | 与 A/B/C 文件集合已错开 | — | ✅ |

**归类定位**：**不正交收口**（frame 后重申布局契约）。

### 二次复盘 — 由渲染结果发现的契约缺口 ✅

| 症状 | 错误做法 | 收口 |
|------|----------|------|
| `spark→hive` Z 折消失后首段横着离开 Bottom 端口 | 走廊入口跳过 outward stub；sanitize 遇到 no-op 强制换另一角 | 入组走廊直接接边框，端点 final leg 仍负责 stub；撤销无障碍信息的强制换角 |
| 客户端 FanIn 用 4/5 点长度猜“第二弯” | 渲染器按 index / path len 推测合流 | SVG 消费 `RouteAnnotationSet.merge_intervals`；只有显式合流边界画直角 |
| CDN/WAF 用全图“同列 AABB”推开 | 会误推跨组、不同宏 rank 的无关节点 | 仅对同 leaf-group 的投影碰撞或正反向对，以 rank 为边界整带移动当前及后续层 |
| refine 再次吃掉层缝 | 仅在最终 guard 补救，重路由看到的是错误节点位置 | refine 候选若破坏 scoped rank gap 则整轮拒绝，保留原布局并走既有路由降级 |

---

## 4. 建议执行序与门禁

```text
A 端口侧  →  C 标签归属  →  B.1 有组 FanIn  →  D O2.3  →  (可选) B.2 监控局部 trunk
```

| 门禁 | 要求 |
|------|------|
| 样例 | T1 `n.user-auth`；T2 无组微服务；T3 `n.microservices`（有组） |
| collinear | `compare-collinear --allow-node-fp`（仅布局轴变时）vs 当前 latest |
| congestion | T1/T2 字段不恶化；T3 增断言：db FanIn merge_intervals / 无共锚 |
| 禁止 | 图名 `if`；路由后 nudge 节点；为消 warning 引入穿模 |

---

## 5. 与既有轨道关系

| 既有 | 关系 |
|------|------|
| congestion S1–S5 / S2.h | **地基**（缝与 stub）；本方案不重做拉缝 |
| S3 无组 FanIn | 已完成；本方案 **S3.2b** 扩有组 |
| S4 / S4.x 监控外环 | **保留**；B.2 是其上可选增强，非否定 |
| O1 共竖干 gap | ✅；A 补选侧，与 O1 正交 |
| O2 hub 对齐 | D 收口 O2.3 |
| collinear P4 全局 lane | 仍暂缓；A/B 后若仍多条 NeedsSeparation 再开 |

---

## 6. 成功标准（产品语言）

- **user-auth**：用户数据库顶边，查询与返回 **可分辨端点**（对边或明显错开）。  
- **typical-microservice**：Prometheus 保持外环；同目标侧在 S4 后局部合流，重复 trunk 标签去重且不穿业务干。  
- **microservices**：Postgres 入边近目标共干；「发布订单事件」有明确归属（引线或贴己边）；web/mobile→gateway 对称、无诡异短折。

---

## 7. 附录 — 管线写权提醒

```text
布局(BK / hub pack / edge_band_demand / SpaceBudget)
  → C: 选侧 → slot → route → lane → [S3 trunk] → [S4 monitor]
  → Annotation(merge_intervals) → sanitize → [S4.x escape] → [B.2 monitor local trunk]
D: snap → sanitize → reverse_pair gap → label resolve / leader
```

改 A 动选侧；改 B 动 S3 写者；改 C 动 D 末标签；改 D 动布局后处理。**勿在错误阶段修错误症状。**
