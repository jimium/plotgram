# TreeLayout

> 状态：占位（待展开）  
> 引擎注册名（目标）：`tree`  
> 模板：[_template.md](../_template.md)

## 签名

树形（或可抽成树的）层次结构；子树递归放置。支持方向性（LTR/TTB 等）与径向等 **profile**，不是按 mindmap 图种单开管线。

## 基本逻辑（草稿）

```text
根选定 → 子树度量（优选尺寸）→ 子树放置器（placer）→ 边几何（常由 placer 风格决定）
```

非树边：预处理（忽略 / 反馈为约束 / 换核），本核不假装是通用有向图布局。

## 能力范围 · 非目标 · 典型域（摘要）

| | |
|--|--|
| **做** | 单根树、子树紧凑/正交/分层放置、子树朝向与边序 |
| **不做** | 一般有向图分层（→ [Hierarchical](../hierarchical/)）；时序消息轴（→ [Sequence](../sequence/)） |
| **典型域** | mindmap；组织树、目录树、部分数据流树 |

## 边几何

多数由 subtree placer 决定（直线 / 正交折线等）；复杂正交可 `DeferToRouter`。

## 相关阅读

- [05 树与径向](../../../reference/yfiles/05-树与径向布局.md)
- [yFiles TreeLayout 产品条](../../../reference/yFiles-layouts-and-routing.md)

## 待写

- [ ] `pipeline.md` — placer 写权、根选择、非树边策略  
- [ ] `scope.md` — radial vs 正交树 profile 表  
- [ ] 与重建 `layout/tree` 模块对齐后补代码链
