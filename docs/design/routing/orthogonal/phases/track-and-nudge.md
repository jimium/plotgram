# Track 定序与 VPSC Nudging（L3 / L4）

> 状态：**已落地**（L3 = `interval_color`；L4 = `vpsc`）
> 代码：`orthogonal/track.rs` · `orthogonal/nudge.rs`
> 零件：`plotgram_algo::interval_color` · `plotgram_algo::vpsc`
> 证据：[03 正交边路由](../../../../reference/yfiles/03-正交边路由.md) §通道内定序 / Nudging

---

## 相序

```text
走廊检测（共线重叠段）
  → L3a color_intervals（track 色）
  → L3b 垂向偏好重排（交叉感知空间序）
  → L4 nudge_track_coords / VPSC（精确坐标；相邻 ≥ spacing）
  → shift_on_line（碰撞则保留原 path）
```

纪律：L4 **不得**改 L3 track 序；Nudging「想穿到另一侧」→ 上提 L2/L3。

---

## L3 · 区间着色 + 交叉感知序

1. **着色**（`color_intervals`, gap=0）：重叠区间不同色。  
2. **空间序**（L3b）：按各色成员的 enter/leave **垂向偏好**（邻接折点的平均垂直坐标）对 colour → spatial rank 重排；低偏好在前。  
   - 解决「仅着色不够」：色号不是几何上下/左右。  
   - 参考 yFiles 03 §3.2 / Wybrow 偏序思想的轻量实现（偏好排序；完整约束图环检测后置）。

平局：偏好相同 → colour id。

---

## L4 · VPSC

```text
vars[t].desired = backbone
vars[t].weight = 1
constraints:  x[t+1] − x[t] ≥ spacing    (t = 0 .. k-2)
```

等权 ⇒ 束居中于 backbone（最小总平方位移）。VPSC 异常时回退为居中均匀铺开（同几何族，确定性）。

---

## 与 M1 均匀偏移的差异

| | M1 `k × spacing` | 现行 L3+L4 |
|--|------------------|------------|
| track 下标 | `edge_order` 序 | `interval_color`（可复用非重叠） |
| 偏移 | 第 0 条钉死，其余 `±k·gap` | VPSC 对称推开 |
| 零件 | 无 | algo `interval_color` + `vpsc` |

---

## 非目标（本相）

- 交叉感知的 track 拓扑序（进入/离开偏序）— 后置
- 端口对齐 / 标签避让写进同一 VPSC — 后置
- Demand 回写层间距 — Hier 侧
