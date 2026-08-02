# 正交路由（Orthogonal EdgeRouter）

> 状态：M0 + M1 已落地
> 注册名：`orthogonal`
> 代码：`crates/plotgram-router/src/orthogonal/`（ovg + search + track）
> 姊妹页：[architecture.md](architecture.md) · [scope.md](scope.md) · [phases/group-crossing.md](phases/group-crossing.md)

轴对齐正交折线路由：避障、少弯、确定性。独立于布局核，可手写 `RouteScene` 夹具开发与验真。

---

## 基本逻辑

```text
RouteScene（障碍 + 端子 + 可选组穿越许可）
  → L2 搜索拓扑（reduced OVG + A*，状态含方向）
  → L3 走廊 track 定序
  → L4 nudging 偏移
  → 正交折线（只写 path）
```

完整相序、参数、里程碑、夹具开发方式 → [architecture.md](architecture.md)。

---

## 自由度分层

| 层 | 自由度 | 写者 | 本 Router |
|----|--------|------|-----------|
| L1 | 端口 side / along | Layout Compose（或夹具） | **只读** |
| L2 | 路径拓扑（经哪些走廊/格点） | Path search | **写** |
| L3 | 同走廊多边次序 | Track / channel order | **写** |
| L4 | 走廊内精确偏移 | Nudging | **写** |
| L5 | 圆角、箭头缩进 | Render / decorations | 不写 path 拓扑 |

L3 不得改 L2 走向；L4 不得推翻 L3 序。

---

## 里程碑

| 阶段 | 状态 | 目标 |
|------|------|------|
| M0 | **已落地** | OVG + A* 避障搜索；无组；单轮 |
| M1 | **已落地** | 两轮 shared；走廊 track 分离（均匀偏移）；规模门控；min_segment |
| M2 | **已落地** | 组穿越首期 + L4 VPSC nudging（[group-crossing](phases/group-crossing.md) · [track-and-nudge](phases/track-and-nudge.md)） |
| M3 | 部分 | Hier DeferToRouter 已通；FacadeVerifier / Tree 接入未做 |
| 后置 | 未做 | 增量路由、Bus、交叉进主搜 |

与 [architecture.md](architecture.md) §10 同步。

---

## 能力范围

见 [scope.md](scope.md)。要点：
- 可手写节点场景独立开发
- 不做 Sequence 主路径
- 不做均匀网格产品路径
- 组场景不支持则显式失败

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
| [phases/group-crossing.md](phases/group-crossing.md) | M2-group 穿越契约 + BoardFixture 字段 |
| [phases/track-and-nudge.md](phases/track-and-nudge.md) | L3 区间着色 + L4 VPSC nudging |
| [ADR-006](../../adr/006-engine-io-and-crates.md) | `EdgeRouter` Trait / crate |
| [03 正交边路由](../../../reference/yfiles/03-正交边路由.md) | 算法证据 |

## 待写（相级下沉，按需）

- [x] `phases/group-crossing.md` — 组穿越首期契约 + 夹具字段
- [x] `phases/track-and-nudge.md` — L3/L4
- [ ] `phases/search-graph.md` — OVG / interesting lines
- [ ] `builtin-vs-router.md` — 与 Hier Channel 对照表（若 architecture §2 不够用再拆）
