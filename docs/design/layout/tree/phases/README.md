# Tree · 相级设计索引

> 父页：[architecture.md](../architecture.md)
> 状态：目标契约细化；不记录实现进度

| 文档 | 回答的问题 |
|------|------------|
| [subtree-placer](subtree-placer.md) | `ISubtreePlacer` 调用序、connector、SubtreeShape 合并、写权 |

## 共同格式

每篇写清：输入/输出、写者、稳定序、不变量、失败类别、下游只读字段。

## 禁止

- 按图种名分支 placer；
- 在 Ink 发明总线轨或子树相对位置；
- 为四向各写一套 `place_subtree`；
- 用包围盒贪心冒充 Buchheim 却声称 M1 完成。
