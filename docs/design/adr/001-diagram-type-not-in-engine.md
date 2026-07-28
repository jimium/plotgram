# ADR-001: 图类型不进入布局系统

> 状态：accepted  
> 日期：2026-07-28

## 背景

V1 以图类型（flowchart / architecture / state / er / mindmap / sequence）为中心，每种图定制布局、路由、后处理管线。结果：6 条平行管线、大量重复逻辑、无法跨图种复用、特判堆积。

yFiles 的产品实践表明：一等公民是 Layout Algorithm + Edge Router，图类型只是推荐参数组合。

## 决策

`diagram <type>` 仅是 DSL 层的 profile 预设，不进入引擎。

## 含义

- 引擎（plotgram-engine）只认 **Layout Algorithm** + **Edge Router**，不认 diagram type
- `diagram flowchart` 在解析层展开为 `layout: hierarchical { ... }` + `edge_routing: orthogonal { ... }`
- 图类型 = 一组默认参数组合，不是独立代码路径
- 布局/路由算法对所有图种通用，通过参数差异化行为
- language-spec 中 §4.4/§4.5 的算法表需要重新设计，不再按图种列举

## 备选方案

- **继续以图类型为中心**：每条管线独立演进。放弃原因：复杂度线性增长，能力无法复用。
- **图类型作为引擎内 hint**：引擎接收 diagram type 做分支。放弃原因：本质仍是特判，只是换了入口。
