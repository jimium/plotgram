# CircularLayout

> 状态：占位（待展开）  
> 引擎注册名（目标）：`circular`  
> 模板：[_template.md](../_template.md)

## 签名

节点置于圆周（或按连通分量多环）；强调环状对称与簇，而非全局层流。

## 基本逻辑（草稿）

```text
分量划分 → 环上序（可选）→ 半径/角度放置 → 边（常直线或浅弧）
```

state 图种可在 **Hierarchical（分层）** 与 **本核（环形）** 间由 profile / 显式 `layout` 选择，不是引擎内 `if state`。

## 能力范围 · 非目标 · 典型域（摘要）

| | |
|--|--|
| **做** | 单环、多分量环、基本环上间距 |
| **不做** | 有向分层主路径（→ [Hierarchical](../hierarchical/)）；代替 Tree 做思维导图 |
| **典型域** | state（环形路径）；部分环状关系图 |

## 边几何

默认直线/简单弧；正交需求走 `DeferToRouter`。

## 相关阅读

- [yFiles 布局与路由清单](../../../reference/yFiles-layouts-and-routing.md)（Circular 条）  
- [archive/atlas 21](../../../archive/atlas/21-Hierarchical统一内核与泳道语义-可行性与演进建议-2026-07.md) §1.1 内核集合

## 待写

- [ ] `architecture.md`（或并入 README）— 分量、环序、半径写权  
- [ ] `scope.md` — 与 Hier 双路径的 profile 约定  
- [ ] balloon / radial tree 与本核边界（避免与 Tree 重叠）
