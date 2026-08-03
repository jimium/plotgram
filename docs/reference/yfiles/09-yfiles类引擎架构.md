# 09 · yFiles 类布局引擎的工程架构

> 归纳自 yFiles 公开 API 语义（`ILayoutAlgorithm`、`LayoutStageBase`、`ILayoutDataProvider`、`LayoutGraph` 适配层）、
> Eclipse ELK 的 `ILayoutProcessor` / `LayoutProcessorConfiguration`、OGDF 的 `ModuleOption` 体系、
> Graphviz 的 `gvlayout` 插件机制。
> 本篇不讨论具体算法，只讨论**如何把算法组织成可演化的引擎**。这是决定项目能走多远的部分。

---

## 1. 核心抽象：三层分离

```
┌─────────────────────────────────────────────┐
│ 视图 / 渲染模型（View）                       │
│  样式、主题、SVG/Canvas、交互、动画            │
└───────────────▲─────────────────────────────┘
                │  只读消费布局结果
┌───────────────┴─────────────────────────────┐
│ 布局模型（Layout Graph）                      │
│  纯几何：节点矩形、边折线、标签矩形、组框、端口   │
│  无样式、无语义、无图种概念                     │
└───────────────▲─────────────────────────────┘
                │  由适配层从业务模型投影
┌───────────────┴─────────────────────────────┐
│ 业务 / 语义模型（Domain Model）                │
│  DSL AST、图种、语义属性、主题                 │
└─────────────────────────────────────────────┘
```

**关键点：布局算法只看第二层。** 这是 yFiles 的 `LayoutGraph`（旧 API）/ `LayoutGraphAdapter`（新 API）的设计，也是 ADR-001「引擎不按 diagram type 分支」的架构基础。

**为什么必须这样：**
- 布局算法的正确性可以在纯几何层面测试（不需要构造完整 DSL）；
- 图种差异通过"投影时选择不同参数/约束"表达，而不是算法内 if；
- 换渲染后端（SVG/Canvas/WASM/PDF）不影响布局。

### 1.1 布局图（Layout Graph）的最小字段集
```rust
pub struct LayoutGraph {
    nodes: IndexMap<NodeId, NodeLayout>,   // 稳定序！
    edges: IndexMap<EdgeId, EdgeLayout>,
    groups: IndexMap<GroupId, GroupLayout>,
    // 派生索引（邻接表等）显式重建，不做隐式缓存
}
pub struct NodeLayout {
    bounds: Rect,                  // 唯一的位置与尺寸真源
    ports: Vec<PortLayout>,        // 相对节点的位置
    labels: Vec<LabelLayout>,
    parent: Option<GroupId>,
}
pub struct EdgeLayout {
    source: (NodeId, PortIdx), target: (NodeId, PortIdx),
    bends: Vec<Point>,             // 路由结果
    labels: Vec<LabelLayout>,
    reversed: bool,                // P1 的产物，渲染时还原
}
```
**必须用 `IndexMap`/`BTreeMap` 而非 `HashMap`**：所有迭代都会影响算法结果（barycenter 平局、路由顺序），HashMap 序会引入不确定性。这是本仓库已记录的踩坑。

### 1.2 "写权"如何在类型系统里表达
纯纪律靠不住，用类型收紧：
```rust
// 方案 A：阶段化所有权（推荐，Rust 友好）
pub struct Layered;    pub struct Ordered;    pub struct Positioned;
pub struct Graph<S> { /* ... */ _stage: PhantomData<S> }

impl Graph<Layered> {
    pub fn set_order(&mut self, ..) {}      // 只有此阶段能写序
    pub fn into_ordered(self) -> Graph<Ordered> { .. }
}
impl Graph<Ordered> {
    pub fn set_x(&mut self, ..) {}          // 只有此阶段能写 x
}
// 方案 B：字段私有 + 每相一个 &mut 视图结构体（更轻，改造成本低）
pub struct CoordWriter<'a>(&'a mut LayoutGraph);
impl<'a> CoordWriter<'a> { pub fn set_x(&mut self, n: NodeId, x: f64) {..} }
```
方案 B 成本低、见效快：给每个自由度一个 Writer 类型，只在对应相构造。**这会把"下游偷偷改上游"从代码审查问题变成编译错误。**

---

## 2. Stage 装饰器链（yFiles 最重要的架构模式）

yFiles 的 `ILayoutStage` 是一个装饰器：它包裹一个 core layout，在其前后做工作。

```rust
pub trait LayoutAlgorithm { fn apply(&self, g: &mut LayoutGraph); }

pub trait LayoutStage: LayoutAlgorithm {
    fn core(&self) -> &dyn LayoutAlgorithm;
}

// 典型 stage 的形态
impl LayoutAlgorithm for RemoveSelfLoopsStage {
    fn apply(&self, g: &mut LayoutGraph) {
        let saved = extract_self_loops(g);       // 前置：移除并记账
        self.core.apply(g);                       // 委托
        restore_self_loops(g, saved);             // 后置：还原/路由
    }
}
```

### 2.1 常见 stage 清单（几乎所有引擎都需要）
| Stage | 前置做什么 | 后置做什么 |
|---|---|---|
| `RemoveSelfLoops` | 摘掉自环 | 单独路由自环 |
| `HandleParallelEdges` | 平行边合并成一条 | 散开成弧/偏移折线 |
| `ComponentLayout` | 分解连通分量，逐个布局 | 装箱拼合（04 篇 §7） |
| `RemoveOverlaps` | — | VPSC/PRISM 消重叠 |
| `TreeReduction` | 图→生成树 | 非树边路由 |
| `OrientationStage` | 把 LR/RL/BT 变换成 TB | 反变换坐标（**关键**，见 §2.3） |
| `GroupHiding` / `Recursive` | 收缩组为超点 | 展开并偏移组内坐标 |
| `Labeling` | 预留标签空间 | 精细放置标签 |
| `EdgeRouting` | — | 替换所有边的路径 |
| `FixNode` | 记录锚点位置 | 整图平移使锚点不动 |
| `PortCalculation` | 分配端口 | 端口位置写回 |
| `NormalizeCoordinates` | — | 平移到 (0,0) / 加 margin |
| `MinimumSize` / `Fit` | — | 缩放/加白边以满足画布约束 |

**这套 stage 清单本身就是巨大的产品能力。** 每个 stage 都是 100–300 行，但组合起来覆盖了大量真实需求。

### 2.2 为什么装饰器优于"管线数组"
- 前置/后置成对出现（extract/restore），装饰器天然表达这种作用域；用平铺数组容易忘记还原或顺序错。
- 可嵌套：`Fix(Component(Orientation(Labeling(HierarchicCore))))`。
- 但**调试性差**（栈很深）。折中：保留装饰器语义，同时提供"扁平化视图"用于诊断输出（打印实际执行顺序与每段耗时）。

ELK 走的是另一条路：**声明式 processor 列表 + 依赖声明**（`LayoutProcessorConfiguration` 里每个 processor 声明"我在 phase X 之前/之后"），由框架拓扑排序生成执行序。优点是可按需启用（只有用了端口约束才插入端口 processor），缺点是配置系统本身有复杂度。
**建议：装饰器为主，配套一个"按特性启用 stage"的构建器**：
```rust
LayoutPipeline::builder(HierarchicCore::default())
    .with_orientation(dir)
    .with_self_loops()
    .with_parallel_edges()
    .with_groups_if(graph.has_groups())      // 按需
    .with_labeling_if(graph.has_labels())
    .with_routing(OrthogonalRouter::default())
    .with_normalize()
    .build()
```

### 2.3 Orientation Stage（方向变换）——**必须做，且必须只在这里做**
支持 TB / BT / LR / RL 四个方向最干净的方式：
1. 前置：把图坐标系变换成算法的标准方向（TB），即交换/取反节点尺寸与已有坐标；
2. 核心算法**只实现 TB**；
3. 后置：把所有坐标（节点、弯点、标签、组框、端口）反变换回去。

这是"图种差异不进引擎"纪律的最佳示范：**四个方向不是四套代码，是一个坐标变换**。
陷阱：
- 节点尺寸也要变换（LR 时宽高互换的是"算法看到的尺寸"）；
- 标签的锚点/对齐方式需要按方向重映射（不能盲目旋转，文字不能倒着写）；
- 端口的 side 枚举要一起映射（north 在 LR 下变成 west）。
把这三条写成一个 `Transform` 结构体统一处理，就不会漏。

---

## 3. 数据供给：LayoutData / DataProvider

算法需要"每个节点/边的额外信息"（层约束、端口约束、组、权重、标签偏好）。三种设计：

| 设计 | 形态 | 评价 |
|---|---|---|
| 把字段塞进 LayoutGraph | `node.layer_constraint: Option<..>` | 简单，但字段无限膨胀，且与算法耦合 |
| **DataProvider（yFiles 旧 API）** | `graph.get_data_provider(KEY) -> dyn Fn(Item)->Value` | 松耦合，可扩展；但类型不安全、KEY 字符串易错 |
| **LayoutData（yFiles 新 API / 推荐）** | 强类型结构体 `HierarchicLayoutData { layer_constraints, port_data, .. }` | 类型安全、可发现性好、IDE 友好 |

**建议：强类型 LayoutData + 每个算法自带其 Data 类型**。
```rust
pub struct HierarchicLayoutData {
    pub layer_constraints: Vec<LayerConstraint>,
    pub sequence_constraints: Vec<SequenceConstraint>,
    pub port_constraints: HashMap<EdgeId, (PortConstraint, PortConstraint)>,
    pub edge_weights: HashMap<EdgeId, f64>,   // 影响拉直优先
    pub critical_paths: Vec<Vec<EdgeId>>,
    pub partition_grid: Option<PartitionGrid>,
}
```
好处：算法签名 `fn apply(&self, g: &mut LayoutGraph, data: &HierarchicLayoutData)` 明确了它消费什么，测试也容易构造。

---

## 4. 参数体系（Options）

### 4.1 分层的参数来源
```
默认值 (algorithm defaults)
   ↓ 被覆盖
图种 profile（archetype/theme 展开的参数集）
   ↓ 被覆盖
图级 DSL 属性（用户在文件头写的）
   ↓ 被覆盖
元素级 DSL 属性（单个节点/边）
```
**profile 是"图种差异"的唯一合法出口**（ADR-001）：`flowchart` 与 `architecture` 的差别应完全体现为参数值差异（层间距、分组策略、端口策略、路由权重），而不是引擎里的 `if diagram_type == ...`。

### 4.2 参数设计守则
1. **物理量而非魔数**：`layer_spacing: f64`（点），不是 `spacing_level: 1|2|3`。
2. **可推导的不要暴露**：节点尺寸由内容推导，不要既有 `width` 又有 `auto_size` 还有 `padding` 互相打架。
3. **每个参数有单一效果点**：一个参数只被一相读取。若两相都读同一参数且效果耦合，那是设计问题。
4. **参数快照进诊断输出**：布局结果里带上"生效参数集"的哈希/JSON，便于复现 bug。
5. **规模门控作为显式参数**（`max_iterations`、`routing_mode_threshold`），并在输出里报告是否命中降级。

---

## 5. 确定性作为架构约束

必须做的事（每一条都对应真实事故）：
1. 所有 map/set 用 `IndexMap`/`BTreeMap`；如必须 HashMap，迭代前显式 `sort`。
2. 所有平局比较有 tie-break，最终 fallback 到稳定 ID（`u32` 序号，不是指针地址、不是字符串哈希）。
3. 随机性：显式 `seed: u64` 参数 + 自带确定性 PRNG（如 `SmallRng::seed_from_u64`），禁止 `thread_rng`。
4. 浮点：避免依赖累加顺序的归约（并行 sum 会变结果）；比较用带 epsilon 的稳定比较器。
5. 时间：不用 `Instant`（WASM 不支持且引入非确定），走项目的 perf 抽象（本仓库已有此红线）。
6. **测试**：同一输入连跑两次，断言 JSON 快照完全相同。这个测试能抓到 90% 的不确定性 bug。

---

## 6. 可观测性与诊断（低估的投入）

布局是"看起来不对但说不出哪不对"的典型领域。建议输出：
```json
{
  "phases": [ {"name":"layering","ms":3.2,"nodes":142,"dummies":88}, ... ],
  "metrics": {"crossings":14,"bends":63,"total_edge_len":18422,
              "node_overlaps":0,"label_overlaps":0,"area":[1200,860]},
  "gates": [{"name":"two_pass_routing","hit":true,"reason":"edges>500"}],
  "constraints": {"requested":42,"satisfied":40,
                  "dropped":[{"kind":"sequence","reason":"conflict with group"}]},
  "params_hash": "…"
}
```
- **每相耗时**让性能回归可定位；
- **约束满足报告**让"为什么我的约束没生效"可回答；
- **指标**接 11 篇的基准体系。

调试可视化（覆盖层）非常值得做：画出层边界、走廊/track、OVG、组框、端口位置、dummy 链。一次投入，长期省时间。

---

## 7. 与 v1 / 现有 crate 的对照建议

结合本仓库的 crate 划分（`model` / `parse` / `engine-api` / `engine` / `pipeline` / `render` / `content`）：

| 本篇概念 | 建议归属 |
|---|---|
| Domain Model（DSL AST、图种、主题） | `plotgram-model` + `plotgram-parse` |
| **Layout Graph（纯几何）** | `plotgram-engine-api`（类型）+ `plotgram-engine`（算法） |
| LayoutData（约束、权重） | `plotgram-engine-api` |
| Stage 链 / 装饰器 | `plotgram-compile` |
| Profile → 参数展开 | `plotgram-compile`（读 model，产出 engine 参数） |
| 文本测量 / content sizing | `plotgram-content`（measure 相，见 06 篇 §5） |
| VPSC / 公共求解器 | `plotgram-engine::solver` |
| 渲染 | `plotgram-render` |

三条最值钱的架构动作（按优先级）：
1. **Writer 类型化写权**（§1.2 方案 B）—— 把纪律变成编译期检查。
2. **VPSC 公共求解器**（07 篇）—— 一个求解器服务五处需求。
3. **Stage 装饰器链 + Orientation Stage**（§2）—— 四方向、自环、平行边、分量、消重叠一次性解决，且不污染核心算法。
