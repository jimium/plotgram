# Circular · 相级设计索引

> 父页：[architecture.md](../architecture.md)
> 状态：目标契约细化；不记录实现进度

| 文档 | 回答的问题 |
|------|------------|
| [partition-and-order](partition-and-order.md) | BCC、割点归属、圈序、稳定序 |
| [backbone-and-ink](backbone-and-ink.md) | 分区圆半径、骨架 balloon、CircRoute、Ink |

## 共同格式

每篇写清：输入/输出、写者、稳定序、不变量、失败类别、下游只读字段。

## 禁止

- 按图种名分支分区策略；
- 在 Ink 挑选外弧或改半径；
- 嵌套 `layout: tree` 当骨架；
- 用声明序冒充谱序却声称 M1 完成。
