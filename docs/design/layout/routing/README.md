# 边路由（独立 EdgeRouter 与原语）

> 状态：现行目标
> 代码：`crates/plotgram-router/`（`core` 原语 + `orthogonal` router + `verify` + `score`）
> 目标架构：[architecture.md](architecture.md) · 范围：[scope.md](scope.md)

横切夹：描述**与各布局核解耦**的边几何契约与正交 Router，避免把 OVG 搜索写进某个核的 Ink 文档里。

---

## 签名

节点与端口冻结之后，求正交折线：避障、少弯、确定性。注册名 `orthogonal`。

---

## 两种边几何模式

| 模式 | `EdgeGeometryMode` | 谁写边路径 |
|------|-------------------|------------|
| 内建 | `Builtin` | 布局核 Ink（Hier：Channel 拓扑 + 展开） |
| 独立路由 | `DeferToRouter` | 布局写节点与端口；**`EdgeRouter` 写 path** |

各核默认选哪种见该核 README；本夹只钉 Router 契约与原语。

---

## 基本逻辑（摘要）

```text
RouteScene（障碍 + 端子 + 可选组穿越许可）
  → L2 搜索拓扑（reduced OVG + A*，状态含方向）
  → L3 走廊 track 定序
  → L4 nudging 偏移
  → 正交折线（只写 path）
```

完整相序、参数、里程碑、夹具开发方式 → [architecture.md](architecture.md)。

---

## 写权（短表）

| 自由度 | 写者 |
|--------|------|
| 节点框 / 组框 | Layout Metric（或测试夹具） |
| 端口 | Layout Compose（或测试夹具） |
| 路径拓扑 / track / 偏移 | **本 Router** |
| 圆角 / 箭头缩进 | Render |

Router **不得**改端口或节点。纪律：[write-authority](../write-authority.md)。

---

## 能力范围 · 非目标

见 [scope.md](scope.md)。要点：可手写节点场景独立开发；不做 Sequence 主路径；不做均匀网格产品路径；组场景不支持则显式失败。

---

## 与内核的关系

| 核 | 关系 |
|----|------|
| Hierarchical | 默认 Builtin Channel；可选 Defer → 本 Router |
| Tree / Circular | 复杂正交走 Defer → 本 Router |
| Sequence | **禁止**本 Router 作主路径 |

共享：`route/core` 无策略原语。不共享：Channel vs OVG 搜索策略。

---

## 文档

| 文档 | 内容 |
|------|------|
| [architecture.md](architecture.md) | 契约、L2–L4、算法选型、夹具、里程碑 |
| [scope.md](scope.md) | 能力与非目标 |
| [ADR-006](../../adr/006-engine-io-and-crates.md) | `EdgeRouter` Trait / crate |
| [03 正交边路由](../../../reference/yfiles/03-正交边路由.md) | 算法证据 |

## 待写（相级下沉，按需）

- [ ] `phases/search-graph.md` — OVG / interesting lines
- [ ] `phases/track-and-nudge.md` — L3/L4
- [ ] `builtin-vs-router.md` — 与 Hier Channel 对照表（若 architecture §2 不够用再拆）
