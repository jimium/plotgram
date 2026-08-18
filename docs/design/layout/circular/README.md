# CircularLayout

> 状态：M3 已落地（节点 `circle` 自定义分区；`single-cycle` 与 BCC 几何可区分）。默认仍是 `bcc-compact` + `spectral`。`automatic` / disk 仍 `Unsupported`
> 引擎注册名：`circular`
> 代码：`crates/tautcore-layout/src/layout/circular/`
> **目标架构真源**：[architecture.md](architecture.md)

## 签名

按连通结构把节点分成圆环（或盘），再把这些环摆成一棵树状骨架；强调**团块与环**，不是全局层流，也不是从根往外长的树。

## 基本逻辑

```text
分区（BCC / 单环 / 作者分区）
  → 每区圈序（谱序首选）
  → 每区半径与角坐标
  → 块割树骨架（balloon 几何，本核内建，不调用 layout: tree）
  → 边：区内弦或外弧；区际骨架边
```

完整管线、IR、参数与里程碑见 **[architecture.md](architecture.md)**。  
yFiles 类与取舍见 **[vs-reference.md](vs-reference.md)**。

## 能力范围 · 非目标 · 典型域

见 [scope.md](scope.md)。摘要：

| | |
|--|--|
| **做** | 单环、BCC 多环、圈序、区内直线/外弧、区际骨架 |
| **不做** | 有向分层（→ [Hierarchical](../hierarchical/)）；思维导图 / 组织树 / 径向树（→ [Tree](../tree/)） |
| **典型域** | 网络拓扑、社交子群、环形状态机（显式 `layout: circular`）；`profile: er` 默认本核 |

## 边几何

- **默认 Builtin**：区内弦（interior）；可选同区外弧（exterior）。
- **允许** `DeferToRouter`（与 Tree 同立场；与 Sequence 相反）：正交需求后接独立 router。
- Ink 只展开 `CircRoute` 骨架，不发明圈序或半径。

## 写权（本核）

| 自由度 | 写者 |
|--------|------|
| 分区、割点归属、块割树 | Compose |
| 圈序 | Compose |
| 半径、角、分区圆心 | Metric |
| 边骨架（弦 / 外弧 / 区际段） | Metric |
| path 点列 | Ink（只展开） |

纪律全文：[写权纪律](../write-authority.md)。

## 相关阅读

| 文档 | 用途 |
|------|------|
| **[architecture.md](architecture.md)** | 目标架构 |
| [deferred.md](deferred.md) | 后置能力契约（disk / automatic / from-sketch / …） |
| [vs-reference.md](vs-reference.md) | yFiles CircularLayout 对照 |
| [phases/](phases/README.md) | 分区/圈序 · 骨架/Ink |
| [05 §4](../../../reference/yfiles/05-树与径向布局.md) | Six–Tollis、谱序、BCC |
| [14 §3.2 / §5.1](../../../reference/yfiles/14-图论与优化工具箱.md) | Tarjan BCC、Fiedler |
| [产品条](../../../reference/yFiles-layouts-and-routing.md) | CircularLayout 能力边界 |
| 上游 | [Circular Layout](https://docs.yfiles.com/yfiles-html/dguide/circular_layout/) · [CircularLayout API](https://docs.yfiles.com/yfiles-html/api/CircularLayout.html) |
