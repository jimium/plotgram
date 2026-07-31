# 布局写权纪律

> 现行设计尺子 · [`AGENTS.md`](../../../AGENTS.md) §1  
> 学 yFiles 的**理念与纪律**，不复刻全量布局栈。

Hier / 正交主路径只有两条硬约束：

1. **单写者** — 每个几何自由度有且只有一个写者  
2. **落笔零新决策** — Ink / 事后修只展开上游，不得发明

---

## 1. 最小完备闭环（Hier）

```text
① 拓扑    rank / order / 反转边              → 组合相
② 端口    每边两端 (node, side, 位置)         → 组合相（显式；不可借住 Ink）
③ 度量    坐标、缝宽、track                   → 度量相
④ 落笔    决策 → 几何                         → Ink（零新决策）
⑤ 标注    与几何联合求解，或至少进 Demand
```

各内核的相切分见 [`layout/`](README.md)；Hier 细节见 [hierarchical/pipeline](hierarchical/pipeline.md)。

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

---

## 3. 动手前四问

1. **写者是谁？** Plan / 度量相是否已有名义主人？  
2. **落笔是否在发明？** 猜中点 / 肘点 / Main 轴 → 缺决策，不是启发式。  
3. **是否多写者？** 间隙、track、ports、label 带是否一处 publish、他处只读？  
4. **是否事后硬修？** 不回写 Plan 的 repair 破坏自反证；正确性靠构造 + verify。

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
| **本文** | 尺子、判断句 |
| [`layout/`](README.md) | 各内核：逻辑、范围、典型域、相写权 |
| [`hierarchical/from-yfiles-reference`](hierarchical/from-yfiles-reference.md) | 参考文库对 Hier / Atlas 迁移的启发纪要 |
| [`archive/atlas/`](../../archive/atlas/README.md) | 历史总纲与债单（只读） |
