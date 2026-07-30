# ADR-001: 图类型 / profile 不进入布局引擎

> 状态：accepted  
> 日期：2026-07-28（修订 2026-07-30）  
> 关联：dsl-spec §1.2 / §4、`plotgram-model::profile`

## 背景

V1 以图类型（flowchart / architecture / state / er / mindmap / sequence）为中心，每种图定制布局、路由、后处理管线。结果：6 条平行管线、大量重复逻辑、无法跨图种复用、特判堆积。

yFiles 的产品实践表明：一等公民是 Layout Algorithm + Edge Router，图类型只是推荐参数组合（profile）。

DSL 若写成 `diagram flowchart { … }`，位置上的 type 仍像语法一等公民，与「一切进属性块」及「引擎不收图种」不一致。

## 决策

1. **引擎不接收** profile / 图种名。只认 `LayoutContract` 中的 layout + edge_routing + 图模型。  
2. DSL 表面：`diagram { profile: <id> … }`——`profile` 是 **diagram 属性**，不是声明头位置参数。  
3. `profile:` 在解析层展开为默认 `layout` / `edge_routing` / 自环等；显式 `layout` / `edge_routing` **覆盖**预设。  
4. **禁止**位置写法 `diagram flowchart {`（不做糖、不双真源）。  
5. 可省略 `profile`：未写 `layout` 时默认按 `flowchart` 预设；只写 `layout` 则纯算法驱动。

封闭集：`flowchart` | `sequence` | `architecture` | `state` | `er` | `mindmap`。

## 含义

- `plotgram-engine` 禁止 `use` profile / `DiagramType` 做分支  
- `plotgram-model::profile` 仅供 DSL / 编排层展开  
- language-spec / 旧 `diagram <type>` 示例一律迁到 `profile:`  
- 算法表按 layout / router 名组织，不按图种列平行管线

## 备选方案

- **继续以图类型为中心**：每条管线独立演进。放弃原因：复杂度线性增长。  
- **图类型作为引擎内 hint**：放弃原因：特判换入口。  
- **属性键用 `type:`**：可用，但与「这是 profile 预设」的架构用语不一致；采用 **`profile:`**。  
- **保留 `diagram flowchart {` 为糖**：放弃原因：与「去掉位置 type」目标冲突，双真源。
