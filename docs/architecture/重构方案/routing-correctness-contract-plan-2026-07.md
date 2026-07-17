# 路由正确性契约 — 架构优化计划（2026-07）

> 日期：2026-07-17  
> 分支语境：`developv2`（L0 地板；L1.1 试修已回滚）  
> 触发：federation 跨组穿模优化反复「单图变好 → 全量门禁红 → 回滚」  
> 依据复盘：多写者无最终所有者、穿组/穿节点目标互相否决、走廊软提示、refine 第二路由器、有组图外廊能力错配  
> 手册：[`布局与路由核心手册-2026-07.md`](../../总结经验/布局与路由核心手册-2026-07.md)  
> 执行清单：[`布局路由待执行计划-2026-07.md`](../../总结经验/布局路由待执行计划-2026-07.md)（`L*`；本文件补 **契约层**，不替代 L*）  
> 受众：架构决策 + AI Agent 认领 `C*` 工作包

---

## 0. 一句话

先定 **正确性优先级与写权**，再改走廊几何；禁止在「穿组 / 穿节点 / ink·共线」未分层的目标函数下继续堆启发。

**非目标**：空壳 `EdgeGeometryContract` 大类型重构；推倒正交主路由；为单图好看抬基线却不记债。

---

## 1. 问题摘要（为何局部优化无效）

```text
corridor try_build
 → validated（常因 through 丢弃整条避组路径）
 → free-route / degraded（短 L 穿组）
 → straighten / x1–x3 / S3
 → refine push+reroute + fallback dogleg（可为消 through 毁掉已避组几何）
```

| 结构债 | 表现 | L1.1 教训 |
|--------|------|-----------|
| 多写者无所有者 | 后阶段可合法覆盖前阶段正确性 | router 避组外绕被 refine 改回穿组 L |
| 双目标互否 | clean∧avoid 同时硬要求 | 走廊因穿节点被丢 → free-route 穿组 |
| 走廊软提示 | 有 chain ≠ 必须走走廊 | `strict`/`prefer_outer` 调不动热点边 |
| refine 双职 | 推节点 + 第二路由器 | `node_fp` / sev 全图漂 |
| 外廊错配 | S4 外环绑 `groups.is_empty()` | 有组 federation 用不上外廊能力 |

---

## 2. 目标契约（最小集合）

以下五条是 **架构契约**，不是可选启发。落地后 L1 类几何优化才有稳定落点。

| ID | 契约 | 一句话 |
|----|------|--------|
| **P1** | 正确性优先级 | 跨 leaf：`不穿组 ≫ 不穿节点 ≫ 短/共线` |
| **P2** | 走廊硬契约 | 有可执行 chain → 走廊（或显式 Degraded）；禁静默 free-route 冒充成功 |
| **P3** | refine 写权收窄 | 已避组边默认冻结；fallback 不得为消 through 引入穿组 |
| **P4** | 门禁分层 | 正确性轨 vs 质量轨分判；债可显式，不可假 PASS |
| **P5** | 有组外廊 | 跨 leaf 外廊不绑死「无分组」；与 S4 monitor 解耦 |

---

## 3. 总序与依赖

```text
C0  契约文档 + 仪表板（本文件落地笔记）
 → C1  P1 优先级写入代码路径（validated / select / degraded 语义）
 → C2  P2 走廊可执行性（真邻接 + 外绕构造 + 失败→Degraded）
 → C3  P3 refine 写权（与 C1/C2 同 PR 或紧随；否则几何又被盖）
 → C4  P4 门禁/报告分层（可与 C3 后半并行）
 → C5  P5 有组外廊通用化（依赖 C1；可与 L1 几何同迭代）
 → 再开 L1.1 几何收口（在契约已生效的前提下）
```

**硬规则**：未完成 C1+C3 前，不得宣称「跨组穿模已修好」。  
**硬规则**：C2 若只做间隙清除不做失败可见性，仍算软提示，不算 P2 完成。

---

## 4. 工作包

### C0 — 契约基线与对照地板

| 字段 | 内容 |
|------|------|
| **目标** | 确认 L0 地板仍在；本计划与 `L*` / 手册交叉引用齐全 |
| **步骤** | 1) federation：`error=11 / through=9 / group=2` 2) 在待执行计划中挂「契约优先于 L1.1 重开」 3) 手册 §6 或 §3 增加「正确性优先级」指针 |
| **禁改** | 不改算法 |
| **DoD** | 三处文档互链；地板数字写入本节附录 |
| **出口** | → C1 |

### C1 — P1 正确性优先级（跨 leaf）

| 字段 | 内容 |
|------|------|
| **目标** | 代码路径显式实现：跨 leaf 时避组优先于避节点 |
| **落点** | `validated_corridor_path`；`select_best_path` / degraded 选用语义；必要时 lint advice 文案对齐 |
| **行为** | 1) 跨 leaf 走廊：`avoid_group` 成立即可接受（允许 through） 2) 同 leaf：仍倾向 clean 3) free-route 在存在「避组候选」时不得因更短而选穿组 |
| **禁改** | 不借机大拆文件；不引入图名分支 |
| **验证** | 单测：跨 leaf 避组脏路径胜于穿组净路径；federation 在 **SKIP_REFINE** 下 group 不劣于 L0 |
| **DoD** | 优先级有单测钉死；手册一句对照 |
| **出口** | → C2 / C3（C3 建议紧接，防 refine 打回） |

### C2 — P2 走廊硬契约

| 字段 | 内容 |
|------|------|
| **目标** | 「有 chain」变为「可执行且优先胜出，或显式失败」 |
| **落点** | `group/corridor.rs`（间隙无第三组；祖先容器跳过）；`corridor_route.rs`（多跳只外绕、选侧避组）；失败时 hints/`Degraded` 而非静默 free-route |
| **行为** | 1) 伪邻接不得进 BFS 2) 中间组不入内 3) `validated` 失败且无避组替代时：标记 degraded，日志可归因 |
| **禁改** | 不为消 warning 接受穿组走廊 |
| **验证** | 单测：left-mid-right 无 left-right 直达；federation SKIP_REFINE 下原热点组穿下降 |
| **DoD** | 走廊失败可观测；与 C1 语义一致 |
| **出口** | → C3（若未做）或 C5 |

### C3 — P3 refine 写权收窄

| 字段 | 内容 |
|------|------|
| **目标** | refine 不得成为「第二套可毁掉避组几何的路由器」 |
| **落点** | `refine/mod.rs`；`spline_fallback.rs` 硬门禁 |
| **行为** | 1) push+reroute 后：若边 **原先不穿组、现在穿组** → 恢复旧边几何 2) fallback **仅**处理当前已穿组边；已避组、仅 through 的边默认不进 dogleg 3) 候选硬拦：不得新增 `edge_crosses_group_interior` |
| **禁改** | 不关闭 refine 推节点能力（可另包收窄 scope）；不按图名跳过 |
| **验证** | 对照：同一输入 `PLOTGRAM_SKIP_REFINE` vs 开 refine，原热点组穿不得反弹；`node_fp` 变化可解释 |
| **DoD** | 写权测试或探针文档化；federation 开 refine 后 group ≤ SKIP_REFINE |
| **出口** | → C4；然后才允许宣称 L1.1 可重开 |

### C4 — P4 门禁分层

| 字段 | 内容 |
|------|------|
| **目标** | 正确性与质量分轨报告，避免「避组外绕」被 ink/共线同一把尺子打死 |
| **落点** | `compare-collinear.sh` / baseline note / 可选 `collinear-baseline` 输出字段 |
| **行为** | 1) **正确性轨**（硬）：`edge_crosses_group_interior`、关键 `error` 中的穿组类；跨 leaf 新增穿组 = FAIL 2) **质量轨**（软/可债）：`exact_sev`、`tight_sev`、`unrelated_edge_trunk_merge`、through-node（在组已安全时） 3) 抬质量基线必须 note：**正确性数字 + 残余债边** |
| **禁改** | 不削弱穿组硬失败；不把 through 永久开除正确性（同 leaf / 无组仍可硬） |
| **验证** | 文档示例：一次「group↓、sev↑」的 compare 输出如何读 |
| **DoD** | Agent 交付模板含两轨结论；本计划附录给样例 |
| **出口** | → C5 / L1.1 |

### C5 — P5 有组图外廊通用化

| 字段 | 内容 |
|------|------|
| **目标** | 跨 leaf 缺可用走廊时，外廊不依赖「无分组 + S4 monitor」 |
| **落点** | `run.rs` prefer_outer / corridor_boost 条件；与 `s4_monitor_corridor` 解耦；scoring 外环垫 |
| **行为** | `!same_leaf && (!has_executable_chain \|\| corridor_failed)` → prefer_outer（或等价）；有组 architecture 可走 |
| **禁改** | 不恢复「对已有可执行走廊的边盲目 prefer_outer」（易更脏） |
| **验证** | 有组代表图：无链跨 leaf 边 outer 候选增多；全量 compare 按 C4 两轨读 |
| **DoD** | S4 仍可专用于 monitor；通用跨 leaf 外廊有独立开关语义 |
| **出口** | → L1.1 几何收口 |

---

## 5. 与 `L*` 的关系

| 原计划 | 与本契约 |
|--------|----------|
| **L0** | 地板；C0 依赖 |
| **L1.0** | 证据仍有效；证明问题在契约而非「未开 strict」 |
| **L1.1** | **延后到 C1+C3（建议含 C2）之后**；几何改动在契约上重开 |
| **L1.2** | Degraded 可见性 ≈ C2 失败可见 + C4 债声明 |
| **L2–L7** | 可与 C4 文档工作并行；**勿与 C1–C3 同 PR 混改热点路由文件** |

```text
推荐日历序：
  C0 → C1 → C3 → C2 → C4 → C5 → L1.1 →（可选）L1.2
         └──────────┘ 可同迭代，但须同次验证
```

---

## 6. 验收总闸

任一 `C*` 合并前：

1. `unset CARGO_TARGET_DIR` + 新编 `./target/release/plotgram`  
2. federation lint 相对 L0：**group 不升**；through/error 若升须在质量轨声明债  
3. `compare-collinear`：正确性轨 PASS；质量轨 FAIL 只能「显式抬基线 + note」  
4. 无图名分支；无「为消 through 新增穿组」  

L0 对照（C0 附录）：

| 图 | error | through | group_interior | node_fp |
|----|-------|---------|----------------|---------|
| `c.k8s-multi-cluster-federation.pgm` | 11 | 9 | 2 | `af36d479d066e61b` |

基线文件：`benchmark-data/collinear-baseline-latest.json`（`date=2026-07-16`）。

契约落地后（2026-07-17，正确性轨）：

| 图 | group_interior (L0→现) | 质量债要点 |
|----|------------------------|------------|
| federation | 2→0 | exact_sev↑、trunk↑、node_fp 变（外绕加长） |
| ecommerce | 0→0 | through↑（避组优先） |
| platform-stack | 4→4 | sev↑ |
| namespace-overview | 1→0 | through↑ |

读法：`compare-collinear.sh` 先看正确性轨；质量轨 FAIL 时用 `--allow-quality-debt` 并 note 残余边，勿把 sev 上升当成穿组未修。

---

## 7. 反模式（禁止）

| ID | 禁止 |
|----|------|
| X1 | 只改 `strict`/`prefer_outer` 宣称修好跨组穿模 |
| X2 | 接受穿组走廊以换 through↓ |
| X3 | refine fallback 重写已避组边且无硬拦 |
| X4 | 用 exact_sev/trunk 否决「组安全但更长」的正确性改进且不分层 |
| X5 | 新建空壳大类型代替写权收敛（手册废止项） |
| X6 | 图名 / 文件名特判 federation |

---

## 8. 风险与债策略

| 风险 | 缓解 |
|------|------|
| 避组外绕加长 → sev/trunk↑ | C4 质量轨允许债；note 列边 |
| 禁 fallback 改避组边 → through 残留 | L1.2 / degraded；或后续「保组前提下的节点微绕」 |
| refine 少推边 → 局部更挤 | C3 仍允许对 **已穿组** 边 fallback；推节点 scope 另议 |
| C2 间隙清除改邻接图 → 他图回归 | 每包全量 compare；祖先跳过防 sibling 误杀 |

---

## 9. 文档维护

- 契约生效后：更新手册 §3.5 / §6 一句「跨 leaf 优先级 + refine 不得毁避组」。  
- `布局路由待执行计划`：L1.1 入口改为「先完成 C1–C3」。  
- 本文件状态：`草案 → 执行中 → 契约已落地（日期）`。

**当前状态**：契约已落地（2026-07-17）— C0–C4 生效；C2 间隙清除 / C5 全量 prefer_outer 保守暂缓（见 §8 债）。

### 落地摘要（2026-07-17）

| 包 | 状态 | 要点 |
|----|------|------|
| C0 | ✅ | L0 地板确认；文档互链 |
| C1 | ✅ | `validated` 跨 leaf 接受避组脏走廊；strict dirty 不再要求 clean；degraded 按 hits 优先避组 |
| C2 | ◐ | 多跳外绕 + 失败→degraded 计数；**激进间隙清除暂缓**（误杀 ecommerce 邻接） |
| C3 | ✅ | 有组图 **跳过 refine push**（防 group_frame 后新穿组）；fallback 仅穿组边；`PLOTGRAM_SKIP_REFINE` |
| C4 | ✅ | `compare-collinear.sh` 两轨 + `--allow-quality-debt` |
| C5 | ◐ | 语义保留（跨 leaf boost）；**有组 prefer_outer 未全开**（易引入穿组） |

**验收（相对 L0）**：正确性轨 PASS（federation group 2→0；namespace/tenant 不升）；质量轨 FAIL 须 `--allow-quality-debt` + note（sev/trunk/through/node_fp）。
