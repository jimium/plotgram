# 边路由（与布局的衔接）

> 状态：占位  
> 代码：`crates/plotgram-engine/src/route/`（`core` 原语 + `orthogonal` 等）

## 两种模式

| 模式 | `EdgeGeometryMode` | 谁写边路径 |
|------|-------------------|------------|
| 内建 | `Builtin` | 布局核 Ink（Hier：Channel 拓扑 + 展开） |
| 独立路由 | `DeferToRouter` | 布局写节点（+ 通常含端口决议）；`EdgeRouter` 写 path |

布局内核文档描述本核默认选哪一种；本夹描述**契约与原语**，避免与某个核绑死。

## 将覆盖

- 正交无策略原语（`route/core`）：锚点、肘线等  
- 独立 `orthogonal` router 的输入假设（节点冻结、端口是否已决议）  
- 与写权纪律的关系：router **不得**为好画而改 rank/order/节点框；缺决策应打回布局组合/度量相

## 相关阅读

- [ADR-006](../../adr/006-engine-io-and-crates.md)  
- [03 正交边路由](../../../reference/yfiles/03-正交边路由.md)  
- Hier [architecture](../hierarchical/architecture.md)

## 待写

- [ ] `builtin-vs-router.md`  
- [ ] `orthogonal.md`
