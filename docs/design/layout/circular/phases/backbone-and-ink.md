# Circular · 骨架与 Ink

> 父页：[architecture.md](../architecture.md) · 对照：[vs-reference.md](../vs-reference.md)
> 证据：[05 §3 balloon / §4.2](../../../../reference/yfiles/05-树与径向布局.md)

本文钉死 Metric 写半径 / 分区圆心 / `CircRoute`，以及 Ink 如何展开。圈序已在 Plan 冻结。

---

## 1. 分区圆（CYCLE）

| | |
|--|--|
| **输入** | 圈序、`NodeSizes`、Demand |
| **输出** | 局部坐标：圆心在原点，节点角 `θ`、半径 `r`、框 |
| **写者** | `ShapeWriter`（该分区） |

角：从 `THETA0 + rotation` 起按权重切满 `2π`。权重 = `max(extent, ε)`。不得均分后再在 Ink 把大节点挤开。

半径：对每对圈上相邻（含首尾）

```text
r ≥ (extent_i + extent_j + node_gap) / (2 · sin(Δθ/2))
```

`Δθ ≥ π` 时跳过正弦约束，只保留 `min_radius` 与 Demand。与 Tree radial 的 `PI_SAFE` 同一理由。

框：中心 = `polar_point(origin, r, θ)`，尺寸 = Demand 后的节点尺寸，轴对齐（**不**旋转框）。

局部原点任意；调度器稍后把分区圆平移到骨架位置。

---

## 2. 骨架放置

| | |
|--|--|
| **输入** | `backbone` 树、每区已算的外接半径 `R_p`（圆 r + 节点外延） |
| **输出** | 每区圆心的全局坐标 |
| **写者** | `BackboneWriter` |

算法（balloon，对**分区**不是对作者节点）：

```text
fn place_disk(p):
    对每个子分区 q: r_q = R_q + gap
    二分 D s.t. Σ 2·asin(r_q / D) ≤ 2π - reserved
    把子圆心放在半径 D 的环上
    非根：reserved > 0，缺口朝向父
```

`place_children_on_common_radius = true`：同一父的子共用同一个 `D`。  
根分区：`reserved = 0`。

平移：每个分区的局部节点框 / 区内边随圆心平移。禁止再改 θ。

区际重叠：子盘与父盘、兄弟盘的圆盘（半径 `R_p`）不得相交（ε）。失败 → `InternalInvariant`（二分应付得住；付不住说明 lo/hi 错）。

**禁止**：对作者节点调 Tree `BalloonPlacer`；禁止 `layout: tree` 嵌套。

---

## 3. `CircRoute`

| 角色 | 默认骨架 |
|------|----------|
| 区内 + interior | `Chord`：两端框边界朝向对方中心 |
| 区内邻接 + exterior | 仍 `Chord`（yFiles：邻接走弦） |
| 区内非邻接 + exterior | `ExteriorArc`：弧半径 = 分区 r + Demand `ExteriorSep`；`t0,t1` = 两端 θ，取短弧 |
| 区际 | `Spoke`：从本区框边界指向对方分区圆心方向，落到对端框 |
| 自环 | 短弧，圆心在节点外侧；不占用圈序 |
| Parallel | M2：弦法向偏移；M1 可与主边重合并 warning |

`automatic` 未落地前不得在 RouteWriter 里按交叉数改角色。

外弧 `t0→t1` 必须与圈序方向一致或取较短者，但**同一分区所有外弧同一取向**（例如一律顺时针），避免交错外弧互穿。取向 = 圈序方向。

---

## 4. Ink

| | |
|--|--|
| **输入** | `CircRoute` + 框 |
| **输出** | `EdgePath` |
| **写者** | `InkWriter` |
| **零新决策** | 不改 θ、r、外弧集合 |

- `Chord` / `Spoke` → 两点折线。
- `ExteriorArc` → 按固定角步长（如 π/12）采样；步数 = `ceil(|Δθ| / 步长)`，与半径无关的拓扑。
- `DeferToRouter` → 空 path。

端子必须落在对应框边界上（verify）。

---

## 5. 分量装箱

每个弱连通分量一个 AABB（含外弧占用）。按分量声明序水平拼接，缝 = `component_gap`。不要对分量再跑一遍 BCC。

---

## 6. 失败类别

| 情况 | 类别 |
|------|------|
| 半径 / 角非有限 | `InternalInvariant` |
| 兄弟分区圆重叠 | `InternalInvariant` |
| 外弧用于区际边 | `InternalInvariant` |
| `disk` 等未实现 style | bind 阶段 `Unsupported` |
| Defer 却写出 path | Ink verifier |
