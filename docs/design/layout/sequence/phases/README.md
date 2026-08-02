# Sequence · 相级设计索引

> 父页：[architecture.md](../architecture.md)
> 状态：目标契约细化；不记录实现进度

| 文档 | 回答的问题 |
|------|------------|
| [axes](axes.md) | 生命线序、时间行、激活派生、x/y 预算 |
| [message-routing](message-routing.md) | Builtin 消息拓扑、附着点、穿越、Ink 展开 |

## 共同格式

每篇写清：输入/输出、写者、稳定序、不变量、失败类别、下游只读字段。

## 禁止

- 把 Channel / OVG 搜索拷进 sequence；
- 在 Ink 发明行号或异步斜率；
- 按 `Arrow` 种类分叉出多套 path 拓扑；
- 为 stub 保留 `HashMap` 序或双真源。
