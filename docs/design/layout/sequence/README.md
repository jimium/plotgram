# SequenceLayout

> 状态：占位（待展开）  
> 引擎注册名（目标）：`sequence`  
> 模板：[_template.md](../_template.md)

## 签名

参与者（生命线）轴 + 消息时间轴；**布局阶段产出边几何**（`produces_edge_geometry`）。不可并入 Hierarchical。

## 基本逻辑（草稿）

```text
参与者序（声明/约束）→ 生命线 x
消息时间序 = 边声明序（无 seq 字段）→ 消息 y
派生：激活条、回消息、笔记等几何
```

时序契约见 [model-boundary · 时序](../../model-boundary.md)：`LayoutContract.layout.name == "sequence"` 时边声明序即时间轴。

## 能力范围 · 非目标 · 典型域（摘要）

| | |
|--|--|
| **做** | 生命线、消息折线/水平段、创建/自调用、基本 notes |
| **不做** | Sugiyama 分层；用 Hier「模拟」消息轴 |
| **典型域** | sequence 图种（profile → 本核） |

## 边几何

内建为主；独立 EdgeRouter 不是本核主路径。

## 相关阅读

- [ADR-003](../../adr/003-edge-structural-fields.md)（边结构字段）  
- [model-boundary](../../model-boundary.md)  
- specs 视觉语言 sequence（若存在）

## 待写

- [ ] `architecture.md`（或并入 README）— 参与者序、消息 y、激活条写权  
- [ ] `scope.md` — fragment/alt/loop 等范围边界  
- [ ] group 在 sequence 中的弱需求与非目标
