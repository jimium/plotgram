# ADR-004: 组间边经由 `group_anchor` 隐形节点（方案 A）

> 状态：accepted  
> 日期：2026-07-30  
> 关联：ADR-001、ADR-003、dsl-spec §6（group 不作边端点）、[`model-boundary.md`](../model-boundary.md)、写权纪律

## 背景

架构图常需要「区域 ↔ 区域」的连线观感。规范坚持 **group 不能作边端点**（边只连 node），跨组边的合法形态是连到各组**内部** node。

若让 `Edge.source/target` 直接指向 group：

- IR 要引入 `Endpoint::Node | Group`
- 组框是包络几何，边锚在**派生 bbox** 上，与叶节点分层的写权纠缠
- 端口、穿组、路由都要第二套锚点规则

若只在 **SVG 渲染期**把线画到组框：落笔/渲染发明几何，违反「落笔零新决策」。

需要一种仍遵守「边只连 node」、又能表达「贴组框连线」的结构化办法。

## 决策

采用 **方案 A 的具象化：`group_anchor` 隐形节点**。

1. **边永远只连 node**；不引入 group 端点。
2. 对「接到某 group 的某侧」引入专用角色节点 **`group_anchor`**（名称以实现为准，语义如下）：
   - **归属**：必填 `host_group`（所属 group id）
   - **侧**：必填 `side`（`north|south|east|west`，与边端口同一封闭集）；可选 `slot`
   - **可视**：render **不绘制**（或等价 invisible）
   - **逻辑尺寸**：采用**最小非零**占位（禁止依赖 0×0，以免路由/碰撞退化）；不是参与内容排版的普通框
3. **几何写权**：
   - 组框 bbox 的写者不变（度量/组几何）
   - anchor 的中心（或锚点）= **由 `host_group` 定稿框 + side + slot 唯一派生**；在组框定稿之后计算
   - **禁止** Ink / sanitize / 渲染「发现是 anchor 再挪到框边」
4. **布局参与**：
   - `group_anchor` **不单独占 Sugiyama 叶层/序**（收缩进 host，或仅在组框定稿后挂载）
   - 不得当普通 leaf 参与 rank/order，以免撑歪图
5. **DSL**：
   - 作者可手写 anchor（dsl-spec §5.7）
   - **推荐糖**：`@group_id` 端点（dsl-spec §7.6）；parse 展开为 `group_anchor` + 普通边
6. **产品范围**：用于区到区连线；区内服务之间仍连真实业务 node。不做 UML、不做表格端点。

## 含义

| 层 | 影响 |
|----|------|
| **model** | `Node.role` / `host_group` / `anchor` 一等字段 + `lift_structural_attrs`（**已落地**） |
| **engine** | Hier/度量：识别 anchor，跳过叶分层；组框后派生坐标；边端口决议与普通边相同 |
| **render** | 跳过 anchor 的形体绘制；边仍按 `EdgePlacement` 画 |
| **dsl-spec** | §5.7 规范；§7.6 `@group` 糖（parser 待实现） |
| **测试** | 断言：组框移动后 anchor 随动；anchor 不出现在 leaf rank 快照；Ink 无「贴边」特判分支 |

## 备选方案（未采用）

| 方案 | 放弃原因 |
|------|----------|
| **B. Group 作边端点** | IR + 双套锚点 + 写权更重；与当前「边只连 node」契约冲突 |
| **仅渲染期贴框** | 落笔/渲染发明坐标；自反证与增量困难 |
| **0×0 节点** | 易触发路由/数值退化；改为最小非零逻辑尺寸 |
| **普通 node + Ink 特判贴边** | 多写者、假装修好；明确禁止 |

## 非目标

- 内容块（框内字段列表）——见 ADR-005；与 anchor 正交 
- 完整 UML 时序 fragment、表格泳道格子  
- 让 anchor 可被用户当业务节点拖拽改层（anchor 不是业务实体）
