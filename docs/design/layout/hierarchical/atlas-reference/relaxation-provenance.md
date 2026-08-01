# Relaxation Ladder + 几何溯源

> 父页：[atlas-reference/README.md](README.md)  
> 对应新架构：[architecture.md §3.4 Diagnostics 与失败语义](../architecture.md)  
> Atlas 源：`crates/v1/plotgram-core/src/layout/atlas/{relaxation,provenance,provenance_check}.rs`

## 这是什么

Atlas 的失败语义有两块：**RelaxationLadder**（L0–L4 显式降级阶梯，每级带 Provenance）+ **Sourced\<T\>**（几何量溯源包装）。前者是新架构 §3.4 失败语义的直接前身，后者是「每个几何量带产地」的优雅设计——但 Atlas 实际用得很少，新实现应让它成为坐标系的底层类型。

## RelaxationLadder

```rust
pub enum RelaxLevel { L0=0, L1=1, L2=2, L3=3, L4=4 }

pub struct RelaxStep { pub level: RelaxLevel, pub provenance: Provenance }

pub struct RelaxationLadder { pub steps: Vec<RelaxStep> }
```

### 生产挂钩

| 级 | 做什么 | 触发条件 |
|----|--------|---------|
| L0 | 正常求解 | — |
| L1 | 缩 pitch 0.85 重解 Main LP | L0 `Infeasible \|\| !audit_passed` |
| L2 | 反向边序重跑 Phase I（不 skip，不热启动） | L1 仍不可行 |
| L3 | `inflate_layer_gaps` 启发式抬缝 | L2 仍不可行；`still_bad=true` 但继续出图 |
| L4 | Degraded（实际返回 None） | ChannelMetric 构建失败 |

### 关键不变量

- 模块头原话：「度量相不可行时按级显式放松，每级带 `Provenance`，**禁止静默压缩**」。
- **勿与 27 号文 L1–L8 合法化编号混淆**——这是失败语义阶梯，不是合法化编号。
- `RelaxLevel` `Ord` 派生，`highest()` 返回最大级；`is_pristine()` = 空 或 highest==L0。
- `push(level, producer, detail)` 追加 `RelaxStep`（带 `Provenance::with_detail`）。

## Sourced\<T\>（几何溯源）

```rust
pub struct Provenance {
    pub producer: &'static str,       // "模块路径:动作"，如 "metric/solve_axis:x"
    pub detail: Option<String>,
}

pub struct Sourced<T> {
    pub value: T,
    pub provenance: Provenance,
}
```

`Sourced<T>` 实现 `Deref` 到内层值（读取无感），`map` 变换值时产地改记为新写入者。

### 与 plan::Provenance 的分工

| 维度 | `plan::Provenance` | `provenance::Provenance`（本文） |
|------|--------------------|---------------------------------|
| 维度 | 边级离散决策溯源 | 几何量（坐标/尺寸/Demand）溯源 |
| 类型 | `enum { ChannelRoute, LegacyAdapter, Manual }` | `struct { producer: &'static str, detail: Option<String> }` |
| 用途 | Plan.channels 的边级溯源 | Stage 推进期的几何量产地 |

注释原话：「**与 `plan::Provenance` 分工**——plan 侧是边级离散决策溯源，本模块是几何量溯源，维度不同不可互换」。

### producer 用 `&'static str` 而非枚举

注释原话：「Stage 推进期写点频繁增删，枚举会把 atlas 子树与旧管线模块名耦合进类型系统」。新实现可保留这个设计，或用 typed Writer id。

## provenance_check.rs（Plan 溯源覆盖门禁）

```rust
pub fn assert_channel_provenance_coverage(plan: &Plan) -> Result<(), PlanError>;
pub fn missing_channel_provenance(plan: &Plan) -> Vec<EdgeId>;
pub fn channel_provenance_coverage_ratio(plan: &Plan) -> f64;  // 空 plan = 1.0
```

**关键不变量**（模块头原话）：「Hierarchical Ink 路径：`plan.channels` 的每条成功边必须有配套 `provenance`（及 `gates`）。**缺口不得静默——调用方应 `?` 或显式 Degraded，禁止在缺溯源时落笔**」。

`assert_channel_provenance_coverage` 在 `materialize_edges`（Ink）之前调用——跑 `missing_channel_provenance` + `Plan::validate`。

## 典型测试场景

| 测试名 | 验证什么 |
|--------|---------|
| `product_flowchart_channel_provenance_full` | 5 个 product flowchart 覆盖率 100% |
| `product_flowchart_no_group_penetration` | L6 + verify_route_scope 硬断言无穿组 |
| `product_post_ink_no_group_penetration_lint` | post-Ink EdgeCrossesGroupInterior = 0 |
| ladder 各级触发 | L0→L1→L2→L3 依次降级 |

## 不该照搬

1. **L4 没有真正出降级图**：`ladder.push(L4, "metric/channel", "build failed: {e}")` 后返回 `None`。新实现应让 L4 真正出降级图（带 `Provenance::Degraded` 标记），不返回 None。
2. **L3 `still_bad=true` 但继续出图**：静默降级。新实现应明确 L3 是「软偏好放宽」还是「硬约束放宽」，并报 `BudgetExceeded`。
3. **`Sourced<T>` 在主路径用得很少**：多是 `perf_log`，没有真正强制每个几何量带溯源。新实现应让 `Sourced<T>` 成为坐标系的底层类型，而非可选包装。
4. **`plan::Provenance::LegacyAdapter`** `[v1-coupled]`：旧管线反向构造的占位，新实现不需要。
5. **`producer: &'static str`** 没有 typed 保证：新实现可用 typed Writer id（新架构 §10.1 typed Writer），让产地是类型而非字符串。

## 新实现建议

### 失败语义（对应新架构 §3.4）

Atlas 的 L0–L4 阶梯映射到新架构 §3.4 的失败语义：

| Atlas RelaxLevel | 新架构失败类型 | 含义 |
|------------------|--------------|------|
| — | `InvalidInput` | 输入不合法（DSL/contract 阶段拒绝） |
| — | `Unsupported` | 当前实现不支持（如 Octilinear） |
| L1/L2 | `InfeasibleConstraint` | 硬约束不可行，走 relaxation ladder |
| L3 | `BudgetExceeded` | 软偏好超预算，降级出图 |
| L4 | `Degraded`（应真正出图） | 不可恢复，带 provenance 标记 |
| — | `InternalInvariant` | verifier 失败或 Plan 不变违反 |

### RelaxationLadder 保留 + 强化

- 保留 L0–L4 阶梯 + 每级 Provenance。
- L4 真正出降级图（带 `Provenance::Degraded` 标记），不返回 None。
- L3 明确报 `BudgetExceeded`，不静默 `still_bad=true`。
- ladder 常量（pitch scale、rip-up 轮数等）作为 profile 参数。

### Sourced\<T\> 成为底层类型

- 让 `Sourced<T>` 成为坐标系底层类型：所有坐标/尺寸/Demand 都带产地。
- `Deref` 让读取无感，`map` 让写入必显式改 producer。
- producer 用 typed Writer id（新架构 §10.1），不用 `&'static str`。
- 这样 verifier 报告能直接指「这条边路径的 x 来自 metric/solve_axis，但 plan.channels 写的是另一条 track」，写权违规立即可见。

### provenance_check 保留

- `assert_channel_provenance_coverage` 作为 Ink 前置 gate，保留。
- 新实现扩展到所有 typed Writer：每个写者写入 Plan 时必须带 provenance，Ink 前查覆盖率。
- 删除 `LegacyAdapter` 变体。

### 与 plan::Provenance 的关系

新实现可统一这两套溯源：

- Plan 级溯源 = 边级决策溯源（哪条边由哪个 Writer 写）。
- 几何量溯源 = 字段级产地（哪个坐标由哪个 Writer 写）。
- 两者都是 `Sourced<T>` 的实例，只是 `T` 不同。
