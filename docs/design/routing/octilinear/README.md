# 八方向路由（Octilinear EdgeRouter）

> 状态：MVP 已落地
> 注册名：`octilinear`
> 代码：`crates/plotgram-router/src/octilinear.rs`
> 约束：[写权纪律](../../layout/write-authority.md) · [routing README](../README.md) · [ADR-006](../../adr/006-engine-io-and-crates.md)

段方向限于 **水平 / 竖直 / 45° 对角**（八方向）的避障折线路由。独立于布局核，可手写 `RouteScene` 夹具开发。

---

## 一句话目标

```text
RouteScene（障碍 + 端子）
  → interesting-line 顶点 + 八方向线桶邻接
  → Dijkstra（欧氏长；边仅 octilinear）
  → 八方向折线（只写 path）
```

**不做**：均匀网格搜索；任意角段；Steiner all-pairs 可见性；改端口/节点；组穿越；track / nudge。

---

## 与相邻算法的边界

| | `orthogonal` | `polyline` | **本 Router** |
|--|--|--|--|
| 合法段 | 轴对齐 | 任意角 | **H / V / 45°** |
| 搜索图 | reduced OVG | 角点可见性 | interesting-line + 线桶 |
| 组（首期） | M2 gate | 拒绝 | **拒绝** |
| 典型场景 | 流程图 | 自由折线 | 电路图、地铁图 |

`orthogonal` ⊂ 本算法的合法段集合；本算法 ⊂ `polyline` 的合法段集合。

---

## L2 选型

- **顶点**：interesting X/Y 线交点（场景较小时）+ 角点 + 端子 stub 的 H/V/45° 射线采样；**禁止** Steiner all-pairs（会 O(V²) 卡死大夹具）
- **边**：按 H / V / 两条对角家族分桶，桶内相邻且可见则连边（度数 O(1)）
- **代价**：欧氏长；平局按顶点稳定序
- **禁止**：均匀网格作产品搜索图（AGENTS.md）；可见性全对枚举

```text
is_octilinear(a, b) ⇔  Δx≈0 ∨ Δy≈0 ∨ |Δx|≈|Δy|
```

端口出针：沿 `side` 走出 `port_stub`（复用 `RouteScene.params`）。

---

## 硬约束（继承 routing README R1–R6）

| 首期 | 行为 |
|------|------|
| 无碰撞自由八方向路径 | 硬失败 |
| 有组边界 / crossing 许可 | `UnsupportedRouteScene` |
| 确定性 | 顶点 `(x,y)` 排序；邻接与 Dijkstra 平局稳定 |

---

## 验真

- 端点附着；边集守恒；双跑 bit-identical
- 每段 `is_octilinear`；不穿非豁免障碍
- **不做**「仅正交」检查（45° 合法）

---

## 后置

- 组 gate；弯折惩罚进主搜；L3–L4
- interesting-line 对角闭包网格（大规模替代稀疏采样）
- 独立 `OctilinearRouteParams`

---

## 文档索引

| 文档 | 内容 |
|------|------|
| [routing/README](../README.md) | 独立 Router 总目录 |
| [polyline/](../polyline/) | 任意角对照 |
| [orthogonal/](../orthogonal/) | 正交对照 |
