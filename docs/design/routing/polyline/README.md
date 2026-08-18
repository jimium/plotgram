# 折线路由（Polyline EdgeRouter）

> 状态：MVP 已落地
> 注册名：`polyline`
> 代码：`crates/tautcore-router/src/polyline.rs`
> 约束：[写权纪律](../../layout/write-authority.md) · [routing README](../README.md) · [ADR-006](../../adr/006-engine-io-and-crates.md)

任意角折线避障路由：节点与端口冻结后，在障碍间隙中求可见性最短折线。独立于布局核，可手写 `RouteScene` 夹具开发。

---

## 一句话目标

```text
RouteScene（障碍 + 端子）
  → 障碍角点可见性图 + Dijkstra（欧氏长）
  → 任意角折线（只写 path）
```

**不做**：均匀网格搜索；改端口/节点；组穿越；track / nudge；交叉进主搜。

---

## 与相邻算法的边界

| | Hier Builtin `routing_style: polyline` | `straight` | `orthogonal` | **本 Router** |
|--|--|--|--|--|
| 谁发明走向 | Compose 写 `DirectOrVia` | 无（直连） | OVG + A* | **可见性图** |
| 段方向 | 展开已给点 | 任意（通常斜） | 轴对齐 | **任意角** |
| 避障 | Ink 不搜 | 否 | 是 | **是** |
| 组 | Plan 许可 | 拒绝 | M2 gate | **首期拒绝** |

`straight` = 本算法在「无障碍 / 直视可达」时的退化输出（两点）。

---

## L2 选型

- **主选**：障碍角点可见性图（vertices = inflated 角点 ∪ 端子 stub）
- **代价**：欧氏段长；平局按顶点稳定序
- **碰撞**：`spacing` 膨胀后的 AABB；端点所属节点免检
- **禁止**：均匀网格作产品搜索图（AGENTS.md）

端口出针：沿 `side` 走出 `port_stub`（复用 `RouteScene.params`），再进搜索图。

---

## 硬约束（继承 routing README R1–R6）

| 首期 | 行为 |
|------|------|
| 无碰撞自由路径 | 硬失败（不静默穿障） |
| 有 `group_boundaries` / `boundary_permissions` | `UnsupportedRouteScene` |
| 确定性 | 顶点按 `(x,y)` 排序；邻接与 Dijkstra 平局稳定 |

---

## 验真

- 端点附着端子；边集守恒；双跑 bit-identical
- 段不得穿非豁免障碍（`spacing` 膨胀）
- **不做**正交检查（斜线合法）

---

## 后置

- 组 gate 穿越
- 弯折惩罚 / 多轮 shared
- L3–L4 分离与 nudging
- 独立 `PolylineRouteParams`（现复用 `OrthogonalRouteParams.spacing` / `port_stub`）

---

## 文档索引

| 文档 | 内容 |
|------|------|
| [routing/README](../README.md) | 独立 Router 总目录 |
| [orthogonal/](../orthogonal/) | 正交对照（OVG / track） |
| Hier [ink-and-verification](../../layout/hierarchical/phases/ink-and-verification.md) §3 | Builtin polyline（DirectOrVia）— **不是**本 Router |
