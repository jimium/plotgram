# 曲线路由（Curved EdgeRouter）

> 状态：MVP 已落地
> 注册名：`curved`
> 代码：`crates/plotgram-router/src/curved.rs`
> 约束：[写权纪律](../../layout/write-authority.md) · [routing README](../README.md) · [ADR-006](../../adr/006-engine-io-and-crates.md)

平滑曲线边路径：节点与端口冻结后，优先写出 [`EdgePath::Cubic`](../../../crates/plotgram-model/src/result.rs)；穿障则回退为平滑折线（`EdgePath::Polyline`）。独立于布局核，可手写 `RouteScene` 夹具开发。

> 成功路径写原生三次贝塞尔 IR；Render 画 SVG `C`。verify / score / ascii 可对 Cubic 做固定点数采样。

---

## 一句话目标

```text
RouteScene（障碍 + 端子）
  → 优先：端口出针三次贝塞尔 → EdgePath::Cubic
  → 穿障则：polyline 可见性折线 + Chaikin → EdgePath::Polyline
  → 只写 path
```

**不做**：均匀网格；改端口/节点；组穿越；力导向 bundling。

---

## 与相邻算法的边界

| | `straight` | `polyline` | **本 Router** |
|--|--|--|--|
| 路径形态 | 两点直线 | 任意角折线 | **Cubic（或折线回退）** |
| 避障 | 否 | 可见性图 | 贝塞尔试探 → 失败则复用 polyline |
| 组（首期） | 拒绝 | 拒绝 | **拒绝** |
| 典型场景 | 调试 | 自由折线 | 有机图、美化 |

对应 yFiles 产品面上的 curved / `OrganicEdgeRouter` 档（简化 MVP）。

---

## 算法（MVP）

1. **贝塞尔试探**：`P0/P3` = 端子；`P1/P2` 沿端口 `side` 外法向伸出 `arm`（`max(port_stub, 0.35·|P0P3|)`）；均匀采样固定点数做净空检查  
2. **净空**：采样折线相对 `spacing` 膨胀障碍（端点节点免检）  
3. **成功**：写出 `EdgePath::Cubic { start, end, controls: [P1,P2] }`（不再把采样点列塞进 IR）  
4. **回退**：调用 `PolylineEdgeRouter` 得避障折线，再做有限次 **Chaikin** 细分；若平滑后穿障则退回未平滑折线 → `EdgePath::Polyline`  
5. 端点必须等于端子锚点

参数复用 `OrthogonalRouteParams.spacing` / `port_stub`。

---

## 硬约束（继承 routing README R1–R6）

| 首期 | 行为 |
|------|------|
| 有组边界 / crossing | `UnsupportedRouteScene` |
| 无合法避障路径 | 硬失败（透传 polyline） |
| 确定性 | 固定采样数、固定 Chaikin 轮次、稳定边序 |

---

## 验真

- 端点附着；边集守恒；双跑 bit-identical  
- 不穿非豁免障碍（最终几何 / 采样点列）  
- **不做**正交 / octilinear 段向检查  

---

## 后置

- 组 gate；独立 `CurvedRouteParams`（tension / samples）  
- 交叉 / bundle；多段样条  

---

## 文档索引

| 文档 | 内容 |
|------|------|
| [routing/README](../README.md) | 独立 Router 总目录 |
| [polyline/](../polyline/) | 回退所用折线路由 |
