# 布局写权纪律

> 现行设计尺子 · [`AGENTS.md`](../../../AGENTS.md) §1
> 学 yFiles 的**理念与纪律**，不复刻全量布局栈。

Hier / 正交主路径的硬约束：

1. **单写者** — 每个几何自由度有且只有一个写者
2. **落笔零新决策** — Ink / 事后修只展开上游，不得发明
3. **多期待拉扯时优先目标函数** — 见 §2.2；不豁免前两条
---

## 1. 最小完备闭环（Hier）

```text
① 拓扑    rank / order / 反转边              → 组合相
② 端口    每边两端 (node, side, 位置)         → 组合相（显式；不可借住 Ink）
③ 度量    坐标、缝宽、track                   → 度量相
④ 落笔    决策 → 几何                         → Ink（零新决策）
⑤ 标注    与几何联合求解，或至少进 Demand
```

各内核的相切分见 [`layout/`](README.md)；Hier 细节见 [hierarchical/architecture](hierarchical/architecture.md)。

---

## 2. 两条纪律

| 纪律 | 含义 | 禁止 |
|------|------|------|
| **单写者** | 每个自由度唯一写者 | 端口 / track / 节点框多处赋值 |
| **落笔零新决策** | 下游只展开上游 | Ink 发明侧内位置、肘点拓扑、走廊坐标、穿组 dogleg |

**判断句**：若修法是「在 Ink 再加一个特判」，先问「这个自由度的写者应该是谁」。

| 可在 Ink | 必须上提 |
|----------|----------|
| 正交肘点展开、bundle 纯几何接合、orientation 转置 | 端口 along、track 真源、路径拓扑洞 |

### 2.1 唯一合法的反向影响：DemandBoard

下游机制可以在**预算阶段**向尚未执行的上游消费者贡献 typed 下界；这不是执行后的回调：

```text
producer 局部聚合（可 sum/count）
  → DemandBoard 对同一 key 仅 max 合并下界
  → consumer 执行前 freeze
  → freeze 后禁止再写该类需求
```

- 走廊 track → layer gap、端口数 → node min size、标签/组标题 → band/group min size 均走 Board；
- 覆盖赋值、消费者运行后补需求、路由阶段就地挪节点都违规；
- 两块必须同时占用的空间应由生产者先求和，不能误用 `max`；
- 具体 epoch/key 见 [Hier coordinate-and-demand](hierarchical/phases/coordinate-and-demand.md)。

### 2.2 目标函数写者（声明表的升级路径）

当同一自由度上多条视觉期待互相拉扯（共轴 vs 扇对称 vs 边直 vs 图宽），**先到先得的认领表 / 成员谓词 / 硬等式焊点**会很快撞上熔断：每张难看的图再加一个排除条件，没有可最小化的量，结果依赖遍历序。

**优先形态**（不替代单写者——求解器仍是该自由度的**唯一**写者）：

```text
可观测自由度 x
  → 定义 J(x)（软目标加权和）+ 尽量少的硬约束
  → typed 权重 / λ（bind + 进 params_hash）表达产品优先级
  → 初值 → 迭代（如加权中位）+ 投影（如 VPSC）→ 保留更优 snapshot
  → 可选终局 snap：只展开 J 已表达的规则，不发明新自由度
```

| 做 | 不做 |
|----|------|
| 硬约束只保留物理/拓扑必保项（分离、链共线…） | 把「好看」写成第三趟 ad-hoc 拉回 |
| 产品规则进 `J` 的项或权重（twin / 主臂 boost…） | 图名特判；Ink 抹坐标救对称 |
| 调不通 → 改 `J` / 权重 / 初值 | 再开一张 claimed 表或约束循环 continue |
| 旁路对比再切主路径（几何有意变化才改基线） | 半套声明表 + 半套求解器长期并存 |

**已落地范例**：Hier 次轴对称 — [`hierarchical/phases/symmetry-axis.md`](hierarchical/phases/symmetry-axis.md)（`J(x)` 取代 SymmetryPlan）。  
**可复用场景（有多目标拉扯时再考虑）**：Channel 轨序与嵌套、组框与全局减交叉、端口列与同脸序的折中（仍须先厘清写者；PortLane 等已有单一写者的不要叠第二套求解器）。

**判断句**：若修法是「往成员表再加一个谓词」，先问「`J(x)` 缺了哪一项，还是权重错了」。

---

## 3. 动手前四问

1. **写者是谁？** Plan / 度量相是否已有名义主人？
2. **落笔是否在发明？** 猜中点 / 肘点 / Main 轴 → 缺决策，不是启发式。
3. **是否多写者？** 间隙、track、ports、label 带是否一处 publish、他处只读？
4. **是否事后硬修？** 不回写 Plan 的 repair 破坏自反证；正确性靠构造 + verify。
5. **是否该上目标函数？** 认领表 / 硬焊点是否在堆谓词？多期待拉扯有无可观测的 `J`？
---

## 4. 优先级

- 有界返工、端点序、单一真源 **先于**「先上真 MCF / 加深全局优化」。
- 不为消 stress / lint 在 Ink 打补丁；禁止图名特判（[AGENTS.md](../../../AGENTS.md) §2）。
- **删冗余优于叠阶段**；不要用兼容层续命第二宇宙。
- **组合相最终应是一个**：weak/strong 是收缩参数，不是平行宇宙；输出类型不一致则组合相尚未存在。

---

## 5. 文档分工

| 文档 | 读什么 |
|------|--------|
| **本文** | 尺子、判断句、目标函数写者（§2.2） |
| [`hierarchical/expectations.md`](hierarchical/expectations.md) | Hier **视觉期待**：对称 / 主轴 / 平行 vs 侧绕 等 + 冲突裁定 |
| [`layout/`](README.md) | 各内核：逻辑、范围、典型域、相写权 |
| [`routing/orthogonal/architecture`](../routing/orthogonal/architecture.md) | 独立 EdgeRouter：只写 path；L2–L4；可夹具开发 |
| [`hierarchical/from-yfiles-reference`](hierarchical/notes/from-yfiles-reference.md) | 参考文库对 Hier 的启发纪要 |
| [`hierarchical/notes/anti-patterns.md`](hierarchical/notes/anti-patterns.md) | Hier 已否决路线 |
| [`archive/atlas/`](../../archive/atlas/README.md) | 历史总纲与债单（只读） |
