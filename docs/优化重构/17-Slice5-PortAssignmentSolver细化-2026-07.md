# 17 — Slice 5（R5）PortAssignmentSolver 细化与可执行步骤

> 日期：2026-07-24
> 状态：**已细化，待落地**（Slice 4 已完成并 byte-identical，见 [16-R0 §10](./16-R0-写权地图-2026-07.md)）
> 来源：[路由 Slice 3-6 统一推进 plan] §Slice5（里程碑）+ [16-路由Recipe-Kernel与求解器架构改造方案] §6 / §20 R5
> 前置勘定：本文 §1 的写权时序表基于 Slice 4 落地后的真实代码（`edge_routing_orthogonal/`）逐函数签名核对。

本文把 plan 里程碑级的「Slice 5：PortAssignmentSolver 单一写者」细化为可执行步骤。核心结论：plan 初稿把「端口 side」与「端点 anchor/slot」笼统混为「端口写权」，但 Slice 4 落地后逐函数核对发现二者是**两批不同写者、不同依赖阶段**，据此把 Slice 5 收窄为 **port SIDE 单一写者**，anchor/slot 重排归入 Slice 6。

---

## 1. 写权时序勘定（Slice 4 后真实现状）

三元组 `from_side: Vec<Port>` / `to_side: Vec<Port>` / `endpoint_map: HashMap<(usize,bool), Endpoint>`。按「谁真正写 side」逐函数核对签名（`&mut [Port]` vs `&[Port]`）：

### 1.1 Port SIDE 写者（真正改 `from_side/to_side`）

**Pre-route 级联**（全在 `phase_port_slot`，Slice 4 后现居 `draft::compile`），执行序：

| # | 写者 | 位置 | 说明 |
|---|------|------|------|
| 1 | `choose_pair_sides_with_group` | `slot.rs` | 逐无向对几何初选 |
| 2 | `coordinate_port_sides` | `phases/port_slot.rs:571` | 同侧偏好协调（G8） |
| 3 | `relieve_overloaded_port_sides` | `phases/port_slot.rs:744` | env-gated，默认关 |
| 4 | **`solve_port_assignment`** | `port_solver.rs:49` | 约束求解器（默认开）；**从 `choose_pair_sides` 重新初始化并整体覆写** sides（局部搜索 swap） |
| 5 | `apply_feedback_side_overrides` | `phases/port_slot.rs:542` | 回环 feedback 覆写 |
| 6 | `apply_monitor_hub_escape_ports` | `feedback_side.rs:198` | S4 监控枢纽逃逸（`s4_monitor_corridor` 条件） |
| 7 | `align_fanin_target_sides` | `phases/port_slot.rs:411` | 仅 `to_side`，扇入对齐 |

> **关键观察**：`solve_port_assignment`（步 4）在函数内从 `choose_pair_sides_with_group` **重新初始化**，因此步 2/3（coordinate/relieve）的输出在 solver 默认开启时**被丢弃**。步 5/6/7 在 solver 之后再 mutate 其输出。→ 实际生效链 = `choose → local_search → feedback_override → monitor → fanin`，级联凌乱、写权分散，是本 Slice 的首要收敛目标。

**Post-route**：**唯一** side 写者 = `fix_reverse_stub_ports`（`stub_fix.rs:361`，经 `phase_stub_fix`←`phase_port_correction`）。路由完成后检测反向 stub，翻转端口（Bottom↔Top / Left↔Right）、重算 anchor、重路由，取最短候选。

### 1.2 Endpoint ANCHOR/slot 写者（**不改 side**，只改锚点切向 + 重路由）

以下全部 `sides: &[Port]`（只读），且是 **route-informed**（读实际路由几何才能决策）：

| 写者 | 位置 | 写内容 | route-informed 依据 |
|------|------|--------|--------------------|
| slot 分配 | `phases/port_slot.rs:200+` | `endpoint_map` 初版 anchor | 目标中心投影（pre-route） |
| `replan_slots` | `slot_replan.rs:16` | anchor 切向重排 + 重路由 | `compute_effective_exit_dir` 读路径 |
| `phase_straighten_align` | `phases/refine.rs:12` | anchor 对齐 + 重路由 | 读已路由几何 |
| `enforce_reverse_pair_dock_separation` | `phases/refine.rs:417` | dock 落点分离（几何） | 读路径 |
| `resolve_exact_stub_occupancy_post_route` | `stub_occupancy.rs:282` | exact 共柱修复（几何） | 读路径 |

### 1.3 确认的**非 side 写者**（只读 side，只写几何）

`reroute_conflicting_edges`（`conflict_reroute.rs:27`）、`phase_lane`（`phases/refine.rs:297`）—— 均 `sides: &[Port]`。

### 1.4 结论

- **port SIDE** = 纯 pre-route 决策（7 步级联）+ 1 处 post-route 翻转（`fix_reverse_stub_ports`）。
- **anchor/slot** = pre-route 初版 + 多处 **route-informed** 重排；天然属于 Slice 6（ResourceGraph / PathAssignmentSolver）职责。
- 把 anchor/slot 塞进 pre-route 的 PortAssignmentSolver 会把 Slice 6 的路径职责提前拖入 Slice 5，模糊 §24「Slice 5 端口 / Slice 6 路径」边界。→ **Slice 5 收窄为 SIDE-only**。

---

## 2. 范围决策（已与需求方确认）

- **Slice 5 = port SIDE 单一写者**：`from_side/to_side` 由唯一的 `PortAssignmentSolver` 写出；pre-route 7 步级联收敛为一个 solver；**删除**散落的 coordinate/relieve/feedback_override/monitor/fanin 作为独立 mutation pass（并入 solver 的约束/目标）。
- **post-route 翻转折叠进 solver 目标**：`fix_reverse_stub_ports` 的**端口翻转（side 重选）删除**；改由 solver 在 pre-route 阶段用几何预测「反向 stub」并纳入目标函数，力求消除翻转。**保留** geometry 相关 stub audit（几何层反向 stub 消毒 `sanitize.rs::fix_endpoint_reverse_stub` 不动）。
- **anchor/slot 重排（replan_slots / straighten_align / dock / exact stub occupancy）归入 Slice 6**：本 Slice **不动**这些函数（它们只读 side，行为不受 side 单一写者影响）。
- **属算法级重写**：走 §7 创新模式 + §8 门禁豁免；硬保穿组 + 确定性。

---

## 3. PortAssignmentSolver 接口设计

复用 / 演进已存在的 `port_solver.rs`。目标是让它成为 side 的**唯一决策点**，吸收当前散落在 `phase_port_slot` 的所有 side 决策 + 反向 stub 预测。

### 3.1 输入（消费 Draft 事实，不自行读 DiagramType）

```rust
/// PortAssignmentSolver 的确定性输入（全部来自 OrthogonalDraft / 其编译上游）。
pub(super) struct PortSolverInput<'a> {
    pub relations: &'a [Relation],
    pub nodes: &'a HashMap<String, NodeLayout>,
    pub group_ctx: &'a GroupRoutingContext,
    pub feedback: &'a FeedbackSideAssignment, // 回环 side 作为硬约束
    pub corridor_plan: &'a CorridorRoutePlan, // 反向 stub 几何预测用（组穿越 → 侧向可达性）
    pub obstacles: &'a PreparedObstacles,     // 反向 stub 几何预测用
    pub horizontal: bool,
    pub s4_monitor_corridor: bool,            // monitor-hub escape 作为硬约束条件
}
```

### 3.2 输出（单一写者产物）

```rust
/// 端口 side 的最终、唯一分配（本 Slice 不含 anchor；anchor 属 Slice 6）。
#[derive(Clone, Debug)]
pub struct PortAssignment {
    pub from_side: Vec<Port>,
    pub to_side: Vec<Port>,
}
```

> 复用现有 `PortAssignment`（`port_solver.rs:26`）。Slice 6 落地时再统一到 Slice 2 的 `EndpointAssignment`（side + anchor + lane）。

### 3.3 目标函数（在现有 `edge_cost` 基础上扩展）

现有 `edge_cost` = `bend_estimate·BEND_WEIGHT + conflict·CONFLICT_WEIGHT + angular·ANGULAR_WEIGHT`。新增：

- **反向 stub 惩罚（新）**：给定候选 side，用 `corridor_plan` + `obstacles` + 节点几何预测「端口出发后是否必须折返穿过节点投影平面」——即当前 `fix_reverse_stub_ports` 的 `collect_edges_to_check` 判据的 pre-route 几何近似。命中则加重罚，力求 pre-route 就避开需要 post-route 翻转的 side。
- **feedback / monitor-hub 硬约束**：回环 feedback hint 的 side、S4 monitor-hub escape 的 side 作为**固定约束**（求解时锁定，不参与 swap），取代当前「solve 后再 override」的时序。
- **fanin 对齐**：`align_fanin_target_sides` 的同宿对齐并入 solver 的软目标（同一 to 节点的入边 to_side 一致性奖励）。
- **同侧偏好 / 过载分流**：`coordinate_port_sides` / `relieve_overloaded_port_sides` 的同侧偏好与容量约束并入 conflict 项（已有二次容量惩罚 `count_port_conflicts`，扩展为覆盖 coordinate 的同侧偏好软目标）。

### 3.4 求解规模分档（保留现有）

- `n ≤ 50`：全量局部搜索（`local_search_optimize`）。
- `50 < n ≤ 120`：高度节点局部搜索（`local_search_high_degree`）。
- `n > 120`：几何规则 + 硬约束（不做 swap，保性能）。

### 3.5 确定性（§2 红线）

- 求解迭代顺序按 `edge_index` 升序；候选 side 遍历按固定 `[Top, Bottom, Left, Right]`；tie-break 按 `edge_index`。
- 禁 HashMap 迭代序驱动决策；degree 统计用 HashMap 但仅做只读查询，不驱动迭代。
- f64 成本比较用 `MIN_IMPROVEMENT` 阈值 + 显式 `partial_cmp`，接受判据带确定性 tie-break。

---

## 4. 可执行步骤（逐步验证，每步可回退）

> 每步：`cargo check -p plotgram-core` → `cargo test -p plotgram-core` → 必要时 `cargo run -p plotgram-cli -- render` 抽样。Slice 5 属算法级重写，**渲染输出会变**（非字节不变），门禁走 §8。

- **S5.1 — 扩展 solver 目标（反向 stub 预测 + 硬约束）**
  在 `port_solver.rs` 新增 `PortSolverInput`；把反向 stub 几何预测抽为 `predict_reverse_stub(ei, fs, ts, ...) -> bool`（复用 `stub_fix.rs::collect_edges_to_check` 的几何判据，pre-route 版）；把 feedback/monitor side 建为锁定约束；fanin/coordinate 并入目标。**加 feature flag `PLOTGRAM_PORT_SOLVER_V2`** 便于 A/B 与回退。solver 单测覆盖新目标。

- **S5.2 — solver 成为 pre-route 唯一 side 写者**
  `phase_port_slot` 里把步 2/3/5/6/7（coordinate/relieve/feedback_override/monitor/fanin）从独立 mutation pass 改为 solver 内部逻辑；`draft::compile` 的 `from_side/to_side` 仅由 `PortAssignmentSolver::solve` 产出。删除被吸收函数（§1 无向后兼容，直接删，不留转发层）。渲染抽样确认无 panic、穿组=0、det=true。

- **S5.3 — 删除 post-route side 翻转**
  从 `phase_port_correction`（`phases/port_correction.rs`）移除 `phase_stub_fix` 子阶段调用；删除 `fix_reverse_stub_ports` 的 **side 翻转写路径**（`from_side[ei]=/to_side[ei]=` + `endpoint_map.insert`）。`phase_port_correction` 只剩 `replan_slots` + `phase_straighten_align`（anchor 写者，不动，归 Slice 6）。保留 `sanitize.rs::fix_endpoint_reverse_stub` 几何消毒。清理 `stub_fix.rs` 中仅服务 side 翻转的死代码。

- **S5.4 — 单一写者测试 + solver 单测**
  加测试：断言 `from_side/to_side` 在整个 `route_edges_orthogonal_inner` 中**仅由 solver 写一次**，post-route 无 side mutation（可用「compile 后 side 快照 vs 渲染完成后 side」——但 side 不再随 edges 返回，改为在 draft 后立即冻结快照比对，或用 `debug_assert` 守卫）。solver 单测：反向 stub 预测命中、feedback 硬约束锁定、fanin 同侧奖励、确定性两次一致。

- **S5.5 — 门禁验证 + 重采基线（§8）**
  `benchmarks/compare.sh` 对比 Slice 4 基线：**硬保** 穿组（`edge_crosses_group_interior`）=0、det=true 全角色；product-gate 质量收敛为**目标**（力求 ≤ Slice 4；退化则记债 + 抬基线带 role note + 列残余样例）；stress 质量可 WARN。收敛后 `./benchmarks/snapshot.sh --tag slice5` 重采。用 `cargo run -p plotgram-cli` 核 binary（勿信陈旧 release）。

- **S5.6 — 更新 R0 文档**
  在 [16-R0](./16-R0-写权地图-2026-07.md) 追加 §11「Slice 5 已落地」：记录 solver 单一写者、被删函数清单、退出判据核对、残余样例、抬基线 note。

---

## 5. §7 创新模式登记（算法级重写必填三件事）

- **目标维度**：
  1. **架构正确性**：`from_side/to_side` 单一写者（pre-route solver 唯一写，post-route 零 side 写）。
  2. **质量（product-gate）**：减少端口冲突/弯折/交叉；`flipped_stub_edges` 目标 → 0（靠 pre-route 预测取代 post-route 翻转）。
- **可接受的临时退化范围**：
  - 过程中允许 product-gate 个别图交叉/弯折/ink **有界上升**（类似模拟退火），只要收敛时不劣于 Slice 4 基线，或显式抬基线带角色 note + 残余样例。
  - stress 集质量允许**更大**临时退化（默认 WARN，不阻挡）。
  - 节点坐标 diff **不要求**持平（§8 暂停「无退化要可量化」）。
- **退出（收敛）判据**：
  1. `from_side/to_side` 单一写者（测试/`debug_assert` 保证 post-route 无 side 写）；
  2. `fix_reverse_stub_ports` 的 side 翻转删除后，product-regression-set 上**不因缺翻转**新增穿组 / det 违规；
  3. 穿组=0、det=true **全角色硬保**；
  4. product-gate 严重度/质量收敛 ≤ Slice 4 基线，或显式抬基线（`raise product:` 带残余样例）。

## 6. §8 门禁豁免清单（本轮适用）

- product-gate / stress 质量轨**不阻挡**，无论退步多少都接受（对比观察，不作回滚依据）。
- **正确性仍硬**：穿组（`edge_crosses_group_interior`）+ 确定性（det=true）。
- 基线可自由重采（`snapshot.sh --tag slice5`），无需走「抬基线 note」流程。
- **卫生红线不豁免**：确定性（§2）、禁止图名特判、`cargo run -p plotgram-cli` 验真、WASM 禁 `std::time`、勿信陈旧产物。

## 7. Slice 6 边界影响（本 Slice 明确移交项）

以下**不在** Slice 5，移交 Slice 6（ResourceGraph + PathAssignmentSolver）统一收敛：

- anchor/slot 重排：`replan_slots`、`phase_straighten_align`（route-informed anchor 写者）。
- dock/落点分离：`enforce_reverse_pair_dock_separation`、`resolve_exact_stub_occupancy_post_route`。
- 冲突/车道几何：`reroute_conflicting_edges`、`phase_lane`、two-round / deferred OVG。

Slice 6 落地时把 `PortAssignment`（side）与 anchor/lane 统一到 Slice 2 的 `EndpointAssignment`，并把 route-informed anchor 重排纳入 PathAssignmentSolver 的一体求解。
