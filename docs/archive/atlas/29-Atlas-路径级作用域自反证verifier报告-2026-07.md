# 29 · Atlas 路径级作用域自反证 verifier 报告（2026-07-26）

> **动机**：兑现 [22 号文 §8 风险台账第二项](22-Atlas下一代布局与路由架构-总纲-2026-07.md)——「『由构造保证』缺少证明义务，实际不成立」。
> **交付**：`crates/tautcore-core/src/layout/atlas/channel/verify.rs`（独立验证器）+ 探针 A2 语义升级 / A2b 新指标。
> **复现命令**（debug profile，AGENTS.md §9）：
> ```bash
> cargo test -p tautcore-core --lib                                                          # 972 passed
> cargo run -p tautcore-eval --bin atlas_probe                                               # product 集（默认）
> cargo run -p tautcore-eval --bin atlas_probe -- --set benchmarks/sets/stress-probe-set.txt
> cargo run -p tautcore-eval --bin atlas_probe -- --set benchmarks/sets/demo-observe-set.txt
> ```

## 0. 一页结论

| 结论 | 证据 |
|---|---|
| **作用域约束新增第四道防线：路径级独立自反证** | `verify_route_scope`——不复用 ScopeMask 代码路径，对已产出路径逐轨道、逐转移断言 |
| **A2 证据链升级：从「机制自证」到「独立作证」** | 探针旧 A2 复核用 `mask.allows()`（与被验机制同一代码给自己作证）；现改为独立验证器的 `ForeignScope` 计数 |
| **gate 一致性首次纳入恒 0 监控（A2b）** | 缺 gate / 配对不符 / 多余 gate / 未知轨道，此前无人验证 `RouteOutcome.gates` 与 `tracks` 的一致性 |
| **三集全绿，无回归** | 77 图 / 1144 边：A1=A2=A2b=A3 全 0，A4 可行率 100%（product 265/265 · stress 180/180 · demo 699/699），与 28 号文 §8 重采基线一致 |
| **单测 972 passed** | 新增 2 个 verifier 测试：真实嵌套组路由通过 + 手工构造五类违规逐一命中 |

**含义**：台账第二项在 channel 作用域这条线上闭环——「由构造保证」现在配齐了独立检查器，机制回归（借道复活、gate 记账错乱）会被探针恒 0 数字第一时间捕获，而不是等渲染肉眼发现。

## 1. 背景：为什么「机制在」不等于「性质成立」

22 号文 §8 台账第二/三项的教训来自真实事故：总纲曾承诺「穿组由构造保证不可表达」（H2），但 27 号文探针实测 ≥2.3% 路径借道穿组——机制只做了静态一半（构建期拒跨 scope link），动态搜索期仍可绕经无关组轨道。修复后（L8 ScopeMask）性质成立了，但**证明义务**并未完全兑现：

改造后作用域约束有三道防线，探针只独立验证了其中一道：

| 防线 | 机制 | 此前的验证方式 | 独立性 |
|---|---|---|---|
| L1 构建期 | 跨 scope link 直接不建 | 单测 | ✅ |
| L8 搜索期 | `ScopeMask` 硬过滤 `{None} ∪ chain(u) ∪ chain(v)` | 探针旧 A2 复核：对产出路径再跑一遍 `mask.allows()` | ❌ **机制自己作证**——若 `scope_chain`/掩码本身有 bug，复核会跟着一起错 |
| L6 基底静态 | `verify_no_group_penetration()`（段×组矩形） | 探针 A1 | ✅（但只验基底合法，不验单条路径的作用域） |

缺口：**没有一个与 ScopeMask 代码路径无关的检查器，对「这条具体路径」断言作用域性质**。这正是台账第二项定义的证明义务——凡写「由构造保证」，必须配一个不依赖该构造的独立检查器。

## 2. 设计：`verify_route_scope`

位置：[`channel/verify.rs`](../../crates/tautcore-core/src/layout/atlas/channel/verify.rs)（115 行）。

```rust
pub fn verify_route_scope(
    substrate: &Substrate,
    tracks: &[TrackId],
    gates: &[GateId],
    u_scope: Option<GroupId>,
    v_scope: Option<GroupId>,
) -> Vec<RouteScopeViolation>
```

### 2.1 独立性怎么保证

- **允许集重新推导**：沿 `substrate.group(g).parent` 链手动上溯构造 `{None} ∪ chain(u) ∪ chain(v)`，**不调用** `scope_chain` / `ScopeMask::for_scopes` / `mask.allows()`——与被验机制唯一共享的是 Substrate 数据本身（组的 parent 关系），这是被断言的事实源，不是被验的机制。
- **验的是产物不是过程**：输入取路径产物（轨道序列 + 闸口序列），对搜索器怎么算出来的一无所知。

### 2.2 验证三件事

1. **每条轨道 scope ∈ 允许集**——违者报 `ForeignScope`（借道穿组，A2 归零对象）；
2. **相邻轨道每次 scope 变化按顺序消耗一个 gate**，且该轨道对确实在 `gate.crossings` 的一一配对里（同 scope 转移是 link，不耗 gate）——违者报 `MissingGate` / `GateMismatch`；
3. **gates 无多余项**——违者报 `UnexpectedGate`。

另有 `UnknownTrack`（路径引用不存在的轨道；出现时转移检查失真，提前返回只报解析违规）。空路径（`Infeasible` 结果）自然通过，`gates` 也须为空。

### 2.3 签名取 `&[TrackId]` + `&[GateId]` 而非 `RouteOutcome`

有意为之：`Plan.channels` / `Plan.gates`（Plan IR，Stage 1 交付 1.1）存的正是这两个序列。将来 Plan 条目回放 / 增量更新后，可用**同一个验证器、同一个口径**复验持久化的决策——channel 现场产出与 Plan 存档两处一个证据链。

### 2.4 复杂度

O(路径长 × 单 gate 配对数)，逐边调用；探针全集（1144 边）总开销不可测量级，生产接线后也可常开为 debug 断言。

## 3. 探针接线（atlas_probe）

| 指标 | 变化 | 语义 |
|---|---|---|
| **A2 借道穿组**（既有） | 复核代码从 `mask.allows()` 换成独立验证器的 `ForeignScope` 计数 | 数字口径不变（仍是「含无关组轨道的边数」），**证据链升级为独立自反证** |
| **A2b gate 一致性违规**（新增） | `MissingGate` / `GateMismatch` / `UnexpectedGate` / `UnknownTrack` 任一命中的边数 | 首次监控 gates 记账与路径的一致性，恒 0 回归数字 |
| A1 / A3 | 不变 | L6 基底穿透 / 同 gate 双穿 |

总表、逐图明细表均增列 A2b；口径一（表达上界）每条 Converged 边都过验证器。

## 4. 单测证据（`channel/tests.rs`，972 passed）

| 测试 | 断言 |
|---|---|
| `verifier_passes_genuine_nested_group_route` | ① 真实嵌套组场景（inner ⊂ outer，I→X 穿 ≥2 gate）的 route 产出 → 违规为空；② 同一路径换 `(None, None)` 视角 → 必报 `ForeignScope`（证明验证器真的在看允许集，不是恒真）；③ 空路径通过 |
| `verifier_flags_borrowed_passage_and_gate_inconsistencies` | 手工基底（根轨道→组内轨道→根轨道，两端 scope 均为 None）：精确命中 `[ForeignScope, MissingGate, MissingGate]`；塞不匹配闸口 → `[GateMismatch, UnexpectedGate]`；未知轨道 → `[UnknownTrack]` |

第 ② 点是自反证测试的关键卫生位：验证器对**同一条合法路径**在不同视角下必须给出不同判决，排除「实现成恒空列表」的假绿。

## 5. 三集探针结果（2026-07-26）

| 图集 | 图数 | 边数 | A1 组穿透 | **A2 借道（独立自反证）** | **A2b gate 一致性** | A3 双穿 | A4 可行率 | 端口争用失败 |
|---|---|---|---|---|---|---|---|---|
| product | 30 | 265 | 0 | **0** | **0** | 0 | 265/265 = 100% | 0 |
| stress | 8 | 180 | 0 | **0** | **0** | 0 | 180/180 = 100% | 0 |
| demo | 39 | 699 | 0 | **0** | **0** | 0 | 699/699 = 100% | 0 |
| **合计** | **77** | **1144** | **0** | **0** | **0** | **0** | **100%** | **0** |

- 逐图明细全部 A1=A2=A2b=A3=0；可行率、端口争用与 28 号文 §8 基线完全一致——接入验证器**零行为变化**，纯增证据。
- A2 三集归零如今有两条独立证据链：L8 掩码（构造）+ 本验证器（自反证），二者代码路径不相交。

## 6. 防线全景（本文后）

| # | 阶段 | 机制 | 独立验证 |
|---|---|---|---|
| L1 | 构建期 | 跨 scope link 拒绝 | 单测 |
| L8 | 搜索期 | ScopeMask 硬过滤 | **本验证器（路径级自反证）** ← 新 |
| L6 | 基底静态 | `verify_no_group_penetration` | 探针 A1 |
| — | 路径产物 | `verify_route_scope` | 探针 A2/A2b 恒 0 |

## 7. 残余与下一步

1. **flat 口径检验力度折扣继续有效**（28 号文 §8.6.4）：探针门面丢弃的病态组（三集共 45 个）使部分图的 A2/A2b 检验力度打折；Stage 1 接通生产 blueprint 后需按真实分区几何复测。
2. **Plan 侧复验尚未接线**：签名已为 Plan 条目预留（§2.3），待 Plan IR 进入回放/增量路径时，将 `verify_route_scope` 作为 `record_route` 后置断言或回放前校验接入。
3. **台账其余项不在本文范围**：本文只闭环「作用域约束」这一条「由构造保证」；其余构造性承诺（如端口唯一键 P-inv-2、gate 配对完备性的基底级性质）如需同等待遇，另行立项。
