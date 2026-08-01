# Ink Verifier：violation 分类 + hard/soft 分割

> 父页：[atlas-reference/README.md](README.md)  
> 对应新架构：[architecture.md §11 InkVerifier](../architecture.md)  
> Atlas 源：`crates/v1/plotgram-core/src/layout/atlas/ink_verify.rs` · `ink.rs`

## 这是什么

Ink 阶段把 Plan + Metric 展开成折线后，verifier 独立复验折线是否忠实于 Plan。Atlas 的 `verify_ink_vs_plan` 是新架构 §11 InkVerifier 的直接前身，violation 分类与 hard/soft 分割可直接借鉴。

## violation 枚举（完整）

```rust
pub enum InkPlanViolation {
    PlanDistorted(EdgeId),                          // soft：M7 repair 已知失真
    MissingGeometry(EdgeId),                        // hard：缺几何 / 点数 < 2
    NonOrthogonal(EdgeId),                          // hard：非轴对齐段
    PortSideMismatch { edge: EdgeId, is_from: bool, side: PortSide },  // hard：端点不落声称侧
    CorridorMiss { edge: EdgeId, track_index: usize },  // hard：折线未命中 track 走廊中心线
    GateMiss { edge: EdgeId, gate: GateId },         // hard：折线未命中 gate 边界线
    Scope(EdgeId, RouteScopeViolation),              // hard：拓扑作用域违规
}
```

## hard / soft 归类

`partition_violations -> (geom_count, distorted_count)`：

| Violation | 归类 | 触发条件 |
|-----------|------|---------|
| `PlanDistorted` | **soft**（不计入 `hard_geom_count`） | 边在传入的 `distorted` 集合中 |
| `MissingGeometry` | hard | `edges.get(eid)` None 或点数 < 2 |
| `NonOrthogonal` | hard | `dx > 0.5 && dy > 0.5` |
| `PortSideMismatch` | hard | `port_on_side` false |
| `CorridorMiss` | hard | `polyline_hits_h/v_line` false |
| `GateMiss` | hard | `polyline_hits_gate_line` false |
| `Scope` | hard | `verify_route_scope` 非空 |

`hard_geom_count` = 上述 6 个 hard 之和。

## 双命中规则（关键）

`polyline_hits_h_line` / `polyline_hits_v_line` 同时承认两种命中：

1. **折点命中**：折线有顶点落在 track 中心线上
2. **正交段穿越**：一段正交段跨过 track 中心线（不必有折点）

这与 ink.rs 的 `push_gate_boundary_point` + `simplify_collinear_preserving` 同轴：ink 在 gate/裙边走廊处保留见证点不被共线简化吃掉，verifier 用同样的「折点或穿越」口径承认命中。**新实现的 InkVerifier 必须保留这个双命中口径**，否则会把 ink 合法产出的「无折点穿越」误报为 CorridorMiss。

## 容差

`TOL = CORRIDOR_LANE_PITCH` `[v1-coupled]`（与 lane pitch 同量级，覆盖 side_order 散布与浮点噪声）。

- `port_on_side`：沿侧方向允许 TOL 散布，法向必须贴边
- 命中判定：`|coord - line| <= TOL`

## 关键算法

```
verify_ink_vs_plan(plan, substrate, nodes, edges, track_coords, distorted, groups):
  1. 构造 InkContext + apply_track_coords
  2. 逐 EdgeId:
     · distorted.contains(eid) → PlanDistorted，跳过几何项
     · 否则跑 verify_route_scope 收 Scope 违规
     · edges.get(eid) None 或点数 < 2 → MissingGeometry
     · has_non_orthogonal → NonOrthogonal
     · port_on_side 检查 from/to → PortSideMismatch
     · 逐 track：Cross 算 lane_centers + skirt_root_cross_y
                 Main 算 lane_centers + skirt_root_main_x
                 polyline_hits_h/v_line 不命中 → CorridorMiss
     · 逐 gate：polyline_hits_gate_line 不命中 → GateMiss
  3. 返回 Vec<InkPlanViolation>
```

## M5 调用约束

注释原话：**调用方须保证节点与边已在规范空间（orientation 之前调用）**。

新架构 §11 InkVerifier 应明确：orientation 变换是 Ink 之后的 Stage，verifier 在 canonical 空间运行。

## 典型测试场景

| 测试名 | 验证什么 |
|--------|---------|
| `verify_table_driven_cases` | faithful(0 违规) / diagonal(几何违规) / port mismatch / distorted skips geom |
| `verify_gate_line_miss_and_hit` | 缺折点 → GateMiss；竖直段穿越 gate 线 → 无 GateMiss（双命中规则） |
| `apply_track_coords_main_and_cross_do_not_clobber` | Cross/Main 不互相覆盖 |
| `push_gate_boundary_and_skirt_root_main` | 组内 x 裙到外侧 margin |

## 不该照搬

1. **`PlanDistorted` 软放过**——这是「M7 repair 改 Plan + verifier 自我豁免」的产物。新实现应：M7 不允许改 Plan，或改了就报 `InternalInvariant`，不在 verifier 里开软放过口子。
2. **verifier 内复用 `skirt_root_*`**——与 ink.rs 同源问题：验证逻辑与生成逻辑都依赖裙边规则，裙边规则变则两边都要改。新实现应：ink 不做裙边（根走廊落位上提到 Metric），verifier 只验「折线命中 track 中心线」，不验「裙边偏移」。
3. **`TOL = CORRIDOR_LANE_PITCH` 硬编码**——新实现应作为 InkVerifier 参数，不全局常量。
4. **验证逻辑分散**——`verify_no_group_penetration`（substrate）、`verify_route_scope`（channel/verify）、`verify_ink_vs_plan`（ink_verify）分散三处。新架构 §11 应集中到分相 Verifier（PlanVerifier / MetricVerifier / InkVerifier / FacadeVerifier），但每相 verifier 内部保持独立证明器风格（不复用构造代码）。

## 新实现建议

- 保留 violation 七类 + hard/soft 分割 + 双命中规则。
- 删除 `PlanDistorted`——M7 不改 Plan，或改了报 InternalInvariant。
- ink 不做裙边（`skirt_root_*`），根走廊落位由 Metric 写；verifier 只验「命中」。
- TOL 作为 InkVerifier 参数。
- 把 `verify_route_scope` 作为 InkVerifier 的子检查（Scope violation），但保留其「独立推导允许集、不复用 ScopeMask」的设计。
- 新架构 §11 的四相 Verifier 都应采用「构造保证 + 独立证明」双层范式：构造路径保证不变量，verifier 独立复证，两者代码路径分离。
