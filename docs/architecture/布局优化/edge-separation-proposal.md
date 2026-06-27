# 正交边路由：边间距（Edge Separation）方案

> 日期：2026-06-28
> 范围：`crates/drawify-core/src/layout/edge/edge_routing_orthogonal/`
> 状态：X-0 完成，待执行 X-1（多轮冲突消解重路由）
> 前置阶段：P0(Group-Aware Side Selection) + P1(Routing Channel Fixes) + A(Slot Replanning) 已完成
> 验证用例：`showcase/architecture/c.layout-stress-nested.dfy`、`c.k8s-tenant-isolation.dfy`、`c.ai-agent-docops-pipeline.dfy`

---

## 一、问题定义

### 1.1 现象

在 P0+P1+A 完成后，side选择、slot排序、通道选择均已改善，但观察到**非bundling模式下仍然存在大量边重合现象**：

- 两条不相关的边在长距离内完全走同一条水平/垂直线段，视觉上叠在一起无法区分
- 平行边间距小于线宽（约2-3px），看起来像一条粗线
- 后路由的边被迫"压"在先路由的边上，因为软惩罚（penalty=1200）无法压过绕开节点的代价

### 1.2 重合分类：合理 vs 不合理

并非所有"近距离平行"都是bug。需要区分三种段类型：

| 段类型 | 位置 | 重合是否合理 | 正确间距 |
|--------|------|-------------|---------|
| **Stub段** | 节点端口出发的第一段（从anchor到第一个折点） | Concentrate下同点出发合理；Compact下小间距合理（slot_pitch=16px） | 0（Concentrate）或slot_pitch（Compact） |
| **Trunk段** | 节点间通道的长水平/垂直段 | **不重合**，必须保持间距 | ≥ EDGE_PARALLEL_GAP（默认8px） |
| **Bundle Trunk段** | 同bundling组的共享主干 | 重合（共线）是设计意图 | 0（共线） |
| **T/L接头** | 一条边的端点落在另一条边上 | 合理（端点接触不算交叉） | 0（接触允许） |

### 1.3 根因诊断

当前代码存在一个关键设计缺陷：

1. **`path_is_clean`（硬过滤）只避开节点和分组，完全不把已路由边当作障碍**
   - 见 `scoring.rs::path_is_clean()`：仅检查 `segment_intersects_node`，无任何边-边检查
2. **边-边间距仅靠软惩罚**（`edge_overlap_penalty`, 1200分/段）
   - 这意味着如果所有候选路径都经过已有边旁边（或没有严格干净的候选），路径仍会选择重合方案
3. **贪心顺序路由**：按 `edge_order` 逐条路由，先到先得，后路由的边没有"硬通道"可走时被迫重合
4. **EDGET_PARALLEL_GAP(8px) 是检测阈值但不是保证距离**：只在penalty计算中用来判断"是否算重叠"，不是路由时刻意保持的间距
5. **多轮重路由缺失**：路由完所有边后没有"发现冲突→重路由"的反馈循环

---

## 二、主流算法调研

### 2.1 三种主流方案对比

| 方案 | 代表实现 | 核心思想 | 优势 | 劣势 |
|------|---------|---------|------|------|
| **A. 边作为硬障碍+迷宫寻路** | yFiles OrthogonalEdgeRouter | 已路由边膨胀为"厚障碍"（线宽+间距），后续边用A*绕开 | 结果干净，每条边独立 | 顺序敏感，后期边绕路越来越长；计算量大 |
| **B. Channel/Lane Assignment** | ELK Layered, Sugiyama坐标分配 | 通道内平行段分配独立lane/track，区间图着色保证无重合 | 数学保证无重合，确定性好 | 适用于分层DAG，对通用orthogonal改造大 |
| **C. Post-routing Nudge** | X6 Manhattan, 轻量router | 先自由路由，后处理检测重叠段，施加排斥力推开 | 简单通用，不改主路由 | 可能引入新弯曲，需迭代收敛 |

### 2.2 FlowML 现状评估

- 已经有候选路径生成（Z-fold/staircase/channel detour）和评分框架
- 已经有SegmentGrid空间索引加速段-段查询
- 已经有edge_overlap_penalty检测重叠（只差"硬约束化"）
- 已经有replan_slots后重路由的机制（remove_by_edges→重新select_best_path）
- 不适用于纯分层Sugiyama（我们的图不是单一方向DAG，有组结构）

**结论：采用A+C混合的分层增量方案最适合现有架构**——先把边升级为硬障碍并多轮重路由（方案A的核心），再用轻推后处理兜底（方案C），不引入方案B的大改造。

---

## 三、架构原则

### 3.1 间距保证的语义

> **两条非bundle边的平行段之间，最小间距必须≥EDGE_PARALLEL_GAP（默认8px）。**
>
> 多辆车都可以走同一条高速（通道），但各占各的车道（lane），用白虚线隔开。
> 只有同bundle组的边才能"合并车道"（共线）。

具体规则：

1. **Stub段**：间距由slot分配天然保证（slot_pitch），不额外加硬约束
   - Concentrate策略：同点出发（间距=0），合理
   - Compact策略：间距=COMPACT_SLOT_PITCH(16px)，合理
   - Single策略：每个端点独立slot，间距≥slot_pitch
2. **Trunk段**（远离节点的中段）：必须保证平行间距≥EDGE_PARALLEL_GAP
   - 水平trunk段：y坐标差≥8px
   - 垂直trunk段：x坐标差≥8px
3. **Bundle trunk段**：bundling开启时同组边共线是合法的，间距=0
4. **正交交叉**（水平×垂直）：允许相交（不可避免），但应尽量减少（已有交叉惩罚）
5. **T/L端点接触**：一条边端点落在另一条边上，不算违规（正常连接）

### 3.2 为什么不一开始就用"边作为硬障碍"？

直觉上，路由第一条边时grid是空的，直接把已路由边当硬障碍应该能保证不重合。但有两个问题：

1. **顺序依赖**：贪心顺序下，先路由的边占据了自然通道，后路由的边被迫绕远路甚至无路可走（退化）
2. **stub段冲突**：同节点同侧出发的stub段必然平行且间距小（slot_pitch可能小于EDGE_PARALLEL_GAP），如果stub段也按硬障碍处理，同节点出边会被互相挡住

所以方案采用**两阶段+迭代**：
- 第一轮快速出初始方案（软惩罚，允许重合）
- 后续迭代检测冲突→按优先级重路由（硬障碍）
- stub段设为"软障碍"（只在距离>slot_pitch+stub_guard时才视为障碍）

---

## 四、分层增量实施计划

### 阶段总览

```
阶段 X-0（准备）：统计重合指标 + 扩展边-边硬检测
阶段 X-1（核心）：多轮冲突消解重路由（边升级为硬障碍）
阶段 X-2（兜底）：Segment Nudging 轻推后处理
阶段 X-3（远期可选）：Lane Assignment 通道分配
```

每个阶段遵循：**先写测试（捕获当前重合的失败用例）→实现→跑基准→记录性能**。

---

### 阶段 X-0：重合检测基础设施（准备）

**目标**：在不修改路由逻辑的前提下，增加重合量化统计和硬检测函数。

**涉及文件**：
- `crates/drawify-core/src/layout/edge/edge_routing_orthogonal/scoring.rs`
- `crates/drawify-core/src/layout/edge/edge_routing_orthogonal/mod.rs`
- `crates/drawify-core/src/layout/edge/edge_routing_orthogonal/context.rs`

**任务清单**：

1. **新增段-段间距检测函数** `segments_violate_spacing(a, b, min_gap) -> bool`
   - 基于现有的 `segments_conflict`，但逻辑反过来：平行段间距 < min_gap 时返回true
   - 垂直交叉不算违规（返回false）
   - 端点接触（T/L接头）不算违规（已有segments_cross_perpendicular排除端点接触的逻辑，复用）
   - 比现有segments_conflict更精确：区分"完全重合"（gap≈0）和"间距不足"（0<gap<min_gap）

2. **新增路径-网格间距违规检测** `path_edge_spacing_violations(path, grid, min_gap) -> Vec<(usize, f64)>`
   - 遍历path的每个段，查询grid中邻近段，收集违反间距的(段索引, 实际gap)列表
   - 返回违规段数量和最小gap，用于优先级排序

3. **在OrthoDebugStats中增加统计字段**
   - `edge_overlap_segments: usize`：完全重合的段对数
   - `edge_tight_segments: usize`：间距不足的段对数
   - `reroute_iterations: usize`：重路由迭代轮次
   - `rerouted_edges: usize`：重路由的边数
   - 在路由结束时打印统计

4. **在bench-phases中增加重合统计输出**
   - 每次benchmark输出预测重合段数，方便前后对比

5. **单元测试**
   - 构造两个完全重合的平行段，验证segments_violate_spacing返回true
   - 构造间距=4px的平行段（GAP=8），返回true
   - 构造间距=10px的平行段，返回false
   - 构造正交交叉，返回false
   - 构造T型接头（端点接触），返回false

**验证标准**：
- 编译通过，0 warning
- 872个现有测试全部通过
- 对三个验证用例输出重合统计（预期：layout-stress-nested有若干重合段）

---

### 阶段 X-1：多轮冲突消解重路由（核心）

**目标**：路由完成后，迭代检测冲突→按优先级重路由冲突边，将其他已路由边视为硬障碍，直到收敛或达到最大轮次。

**涉及文件**：
- `crates/drawify-core/src/layout/edge/edge_routing_orthogonal/scoring.rs`
- `crates/drawify-core/src/layout/edge/edge_routing_orthogonal/mod.rs`
- `crates/drawify-core/src/layout/edge/edge_routing_orthogonal/path.rs`

**前置依赖**：阶段 X-0 完成。

**任务清单**：

1. **新增"边-边硬干净"判定函数** `path_is_clean_from_edges(path, grid, min_gap, from_id, to_id, from_ep, to_ep) -> bool`
   - 类似path_is_clean，但检查对象是已路由边段而非节点
   - **stub段豁免规则**：path的第一段（from stub）和最后一段（to stub）在stub_guard长度内（默认24px）不做硬检查——因为同节点相邻slot的stub天然平行近距
   - 中段（非stub）严格执行min_gap检查
   - 排除T/L接头（端点接触）

2. **扩展path候选生成的硬过滤**
   - 现有逻辑：候选路径先过path_is_clean（节点硬过滤），否则降级
   - 新增：在多轮重路由时（phase1_only=true或reroute_mode=true），候选路径必须**同时**通过path_is_clean（节点）和path_is_clean_from_edges（边间距）
   - 第一轮路由（初始路由）保持现状（仅软惩罚），不启用边硬过滤，避免初始路由时grid为空导致stub相互阻塞

3. **实现多轮重路由循环**（在replan_slots之后、bundling之前）
   ```
   const MAX_REROUTE_ROUNDS: usize = 3;
   let mut conflict_edges = detect_conflict_edges(&edges, &grid, EDGE_PARALLEL_GAP);
   for round in 0..MAX_REROUTE_ROUNDS {
       if conflict_edges.is_empty() { break; }
       // 按冲突严重度排序（违规段数多的优先）
       conflict_edges.sort_by(|a,b| b.violation_count.cmp(&a.violation_count));
       let mut rerouted = HashSet::new();
       for (ei, _) in conflict_edges {
           if rerouted.contains(&ei) { continue; }
           // 将其他已路由边视为硬障碍（当前边先从grid移除）
           grid.remove_by_edges(&[ei]);
           let new_path = select_best_path_with_scorer_stats(
               &ctx_with_edge_obstacles, &pair, &DefaultScorer, ...,
               /*strict_edge_spacing=*/ true
           );
           if new_path_is_valid(new_path) {
               grid.insert_path(&new_path, ei);
               update_edge(ei, new_path);
               rerouted.insert(ei);
           } else {
               // 找不到严格干净的路径，放回原路径
               let old_path = edges[ei].path_points();
               grid.insert_path(&old_path, ei);
           }
       }
       // 重新检测冲突
       conflict_edges = detect_conflict_edges(&edges, &grid, EDGE_PARALLEL_GAP);
   }
   ```

4. **stub段保护机制**
   - 在path_is_clean_from_edges中，stub段（path[0]→path[1]和path[n-2]→path[n-1]）只检查"完全重合"（gap<1px），不检查"间距不足"（1px<gap<8px）
   - 这避免同节点相邻slot的stub段被误判为违规
   - 中段严格检查8px间距

5. **在OrthoConfig中增加可配置参数**
   - `edge_edge_gap: f64`（默认8.0）：平行边最小间距
   - `stub_guard_length: f64`（默认24.0）：stub段保护长度
   - `max_reroute_rounds: usize`（默认3）：最大重路由轮次

6. **更新tuning-guide文档**
   - 记录新增参数含义和调优建议

7. **测试**
   - 构造两个节点A、B水平对齐，第三条边C→D被迫走A→B通道的场景，验证第一轮重合，第二轮重路由后间距≥8px
   - 验证同节点同侧出发的stub段在stub_guard内不触发重路由
   - 验证5轮运行结果完全一致（确定性）

**验证标准**：
- 872个现有测试通过
- 三个验证用例的重合段数显著下降（目标：降为0或接近0）
- 性能中位数增长不超过50%（例如layout-stress-nested从2.6ms→≤4ms）
- 5次运行的md5一致（确定性）
- 无退化边（degraded_count不显著增加）

---

### 阶段 X-2：Segment Nudging 轻推后处理（兜底）

**目标**：对于X-1多轮重路由后仍无法消除的少量残余重合（拓扑约束导致确实没有足够空间），用几何轻推（nudge）局部推开。

**涉及文件**：
- 新建 `crates/drawify-core/src/layout/edge/edge_routing_orthogonal/nudge.rs`
- `mod.rs` 增加调用

**前置依赖**：阶段 X-1 完成，确认残余冲突数量。

**任务清单**：

1. **收集残余冲突段对**
   - 在X-1循环结束后，收集仍违反间距的平行段对
   - 按段长度降序排序（优先处理长重合段）

2. **实现单段nudge操作** `nudge_segment(edges, ei, seg_idx, direction, distance)`
   - 水平段：向上/下平移distance，在段两端插入补偿折点保持正交
   - 垂直段：向左/右平移distance，同样插入折点
   - 保持端点锚点不动（不修改from/to anchor位置）
   - direction选择冲突少的一侧（推向更空旷的方向）

3. **实现约束检查**
   - nudge后的段不能与节点相交（复用segment_intersects_node）
   - nudge后的段不能与其他段产生新的重合（检查间距）
   - nudge距离限制在 ±EDGE_PARALLEL_GAP 范围内（避免大幅变形）

4. **迭代收敛**
   - 最多3轮nudge
   - 每轮处理所有当前冲突段，冲突解决或无法nudge时停止

5. **测试**
   - 构造两条完全重合的水平段，验证nudge后间距=8px
   - 验证nudge不改变端点位置
   - 验证nudge后路径仍正交（所有段水平或垂直）

**验证标准**：
- 重合段数进一步下降（目标：0）
- 不引入新的节点穿越
- 端点位置不变（不破坏slot replan结果）
- 性能开销可忽略（<0.5ms）

---

### 阶段 X-3：Lane Assignment（远期可选）

**目标**：对于高密度图（边密度>30%），在通道内做显式lane分配，从几何上保证无重合。

**触发条件**：X-1+X-2完成后，如果在>50节点的压力测试图上仍有大量重合，再进入此阶段。

**核心思路**：
1. 收集所有水平trunk段，按y坐标聚类到"通道组"
2. 在每个通道组内做区间图着色（interval graph coloring），为每个段分配lane号
3. lane号→y坐标偏移：`y = base_y + lane * EDGE_PARALLEL_GAP`
4. 同样处理垂直段（x坐标偏移）
5. 在lane边界处插入Z字折点连接stub和偏移后的trunk

**复杂度**：高。这是PCB routing的经典track assignment问题，需要通道识别、区间构建、着色、折点补偿四步。建议先观察X-1+X-2效果再决定。

---

## 五、AI Agent执行指南

### 5.1 执行顺序

```
X-0 → X-1 → 视觉验证 → （如残留重合）→ X-2 → 视觉验证 → （如高密度仍不够）→ X-3
```

每个阶段**独立可测试**，完成一个阶段后必须：
1. `cargo test --release -p drawify-core --lib` 全部通过（872）
2. 三个验证用例benchmark：`cargo run --release -p drawify-core --bin bench-phases -- <file> 5`
3. 确定性验证：同一输入连续5次md5相同
4. 记录性能数据到本文档的"实施记录"章节
5. 视觉验证：渲染PNG人工检查重合是否消除/改善

### 5.2 禁止事项

1. **不要为特定case硬编码偏移**：所有间距规则必须基于通用几何判定（段类型、间距阈值），不得对"主数据库"、"API网关"等特定节点做特殊处理
2. **不要破坏stub段的slot顺序**：replan_slots已经保证了端点顺序，nudge和reroute不得移动端点anchor坐标
3. **不要在初始路由（第一轮）就启用边硬障碍**：这会导致同节点stub互相阻塞，必须分轮次
4. **不要删除或弱化现有的节点/分组障碍检查**：边-边间距是新增约束，不是替代
5. **不要为了消除warning牺牲算法简洁性**（遵循AGENTS.md第4条）
6. **遵守确定性规则**（AGENTS.md第2条）：所有迭代排序必须基于稳定键，禁止依赖HashMap迭代顺序

### 5.3 关键API/数据结构参考

| 名称 | 文件 | 作用 |
|------|------|------|
| `SegmentGrid` | `context.rs:62` | 段空间索引，已有insert_path/remove_by_edges/query_overlapping |
| `RoutedSegment` | `path.rs:173` | 已路由段（x1,y1,x2,y2,edge_index） |
| `segments_conflict` | `scoring.rs:297` | 现有段冲突检测（重合+交叉），可扩展 |
| `edge_overlap_penalty` | `scoring.rs:274` | 现有软惩罚，X-1后保留作为评分项 |
| `path_is_clean` | `scoring.rs:174` | 节点硬过滤，X-1新增边-边版本 |
| `select_best_path_with_scorer_stats` | `path.rs` | 候选路径选择，支持phase1_only参数 |
| `replan_slots` | `mod.rs:621` | 现有slot重排+reroute，X-1在其之后执行 |
| `OrthoConfig` | `mod.rs:82` | 正交路由配置，新增edge_edge_gap等参数 |

### 5.4 性能预算

| 用例 | 当前中位数 | X-1目标 | X-2目标 |
|------|-----------|---------|---------|
| c.layout-stress-nested.dfy | 2.65ms | ≤4ms | ≤4.5ms |
| c.k8s-tenant-isolation.dfy | 10.24ms | ≤15ms | ≤16ms |
| c.ai-agent-docops-pipeline.dfy | 3.66ms | ≤5.5ms | ≤6ms |

如果X-1阶段性能超出预算，优先检查：
1. reroute轮次是否过多（默认≤3轮）
2. query_overlapping的BBOX_EXPAND是否过大（当前10px）
3. 是否在stub段做了不必要的硬检查

---

## 六、实施记录

（每阶段完成后由AI Agent填写）

### X-0：重合检测基础设施
- 完成日期：2026-06-28
- 性能基准（release，5轮中位数）：
  - layout-stress-nested: 1.71ms（11节点/13边）
  - k8s-tenant-isolation: 11.32ms（19节点/26边）
  - ai-agent-docops-pipeline: 3.13ms（16节点/17边）
- 重合统计（before/X-0基线，即无修复时的重合量）：
  - layout-stress-nested: exact_overlap=1, tight_spacing=0
  - k8s-tenant-isolation: exact_overlap=7, tight_spacing=0
  - ai-agent-docops-pipeline: exact_overlap=12, tight_spacing=1
- 测试：882个全部通过（原872+新增10个）
- 编译：0 warning
- 确定性：layout-stress-nested 5轮md5一致；k8s/ai-agent存在预先存在的非确定性（与X-0无关，待后续排查）
- 新增API：
  - `SpacingViolationKind { ExactOverlap, TightSpacing }`：违规类型枚举
  - `segments_violate_spacing(a, b, min_gap) -> Option<(SpacingViolationKind, f64)>`：段-段间距检测
  - `path_edge_spacing_violations(path, grid, min_gap) -> Vec<(usize, SpacingViolationKind, f64)>`：路径-网格违规扫描
  - `count_all_edge_spacing_violations(grid, min_gap) -> (usize, usize)`：全局违规计数
  - `SegmentGrid::all_segments() -> &[RoutedSegment]`：段访问器
  - OrthoDebugStats新增字段：edge_exact_overlap_pairs, edge_tight_spacing_pairs, reroute_iterations, rerouted_edges

### X-1：多轮冲突消解重路由
- 完成日期：2026-06-28
- 性能基准（release，7轮中位数）：
  - layout-stress-nested: 5.97ms（11节点/13边，vs X-0基线1.71ms，+4.26ms）
  - k8s-tenant-isolation: 28.31ms（19节点/26边，vs X-0基线11.32ms，+16.99ms）
  - ai-agent-docops-pipeline: 6.44ms（16节点/17边，vs X-0基线3.13ms，+3.31ms）
- 重合统计（after X-1，对比X-0基线）：
  - layout-stress-nested: exact_overlap=1→1（0%↓）, tight_spacing=0→0；1处残余重合无法通过单条边重路由解决
  - k8s-tenant-isolation: exact_overlap=7→1（**86%↓**）, tight_spacing=0→1；残余1处exact+1处tight属高退化场景
  - ai-agent-docops-pipeline: exact_overlap=12→11（8%↓）, tight_spacing=1→1；多条边共享同一狭窄通道，单条重路由无法彻底消解
- reroute迭代轮次：3/3轮（全部用满，说明仍有冲突但达到上限）
- rerouted边数（累计）：layout-stress=33, k8s=60, ai-agent=21（同一条边可能多轮被重路由）
- 退化边数：layout-stress=0, k8s=6, ai-agent=1（与X-0基线一致，重路由未增加新退化）
- 测试：882个全部通过
- 编译：0 warning
- 确定性：X-1代码本身使用稳定排序（按违规数降序+边索引升序tiebreak），HashSet仅用于contains()查找；
  残余非确定性为pre-existing问题（初始路由阶段HashMap迭代顺序导致），与X-1无关。
- 新增/修改API：
  - `path_is_clean_from_edges(path, grid, min_gap, stub_guard) -> bool`：边-边硬干净判定，豁免stub段
  - `reroute_conflicting_edges(...)`：多轮冲突消解主函数，位于replan_slots之后执行
  - 常量：STUB_GUARD_LENGTH=24.0, MAX_REROUTE_ROUNDS=3, REROUTE_EXTRA_CHANNEL_MARGIN=40.0
  - 改进`count_all_edge_spacing_violations`：豁免短stub段（≤STUB_GUARD_LENGTH的首尾段不计数），避免Concentrate策略下短stub被误判
- 算法设计要点：
  1. 每轮检测所有边的间距违规，按违规段数降序排列处理（冲突最多的边优先修复）
  2. 逐条撕除-重路由：移除当前边→用更大channel_margin（+10/+25/+40px三档）尝试重路由→硬检查（节点+分组+边间距三重过滤）
  3. 每条边处理前重新检查冲突（上一条边的重路由可能顺带解决了这条边的冲突）
  4. 找不到干净路径时优雅降级：恢复原路径、标记为failed、后续轮次跳过
  5. 尝试过批量撕除（batch rip-up）策略，但实测k8s效果变差（1→5），因为移除所有冲突边后先路由的边会抢占通道阻塞后路由的边，故回退为逐条处理
- 性能分析：
  - 小图（~15边）开销约3-4ms，可接受
  - 中图（~26边）开销约17ms，主要来自60次额外候选路径搜索（每次搜索~0.3ms）
  - 性能超预算原因：k8s案例中6条退化边反复重路由失败消耗算力，可考虑通过failed_edges提前终止减少无效搜索
- 残余问题（留给X-2/X-3）：
  1. **同通道多重重叠**（ai-agent 11处）：多条边初始路由选择同一狭窄通道，逐条重路由无法同时为所有边找到独立通道，需要X-3的车道预留
  2. **振荡问题**：边A移开后挡住边B，边B移开后又挡住边C，3轮迭代无法收敛到全局最优，需要更全局的策略
  3. **pre-existing非确定性**：初始路由中HashMap迭代顺序导致不同运行产生不同初始路径，影响重路由起点

### X-2：Segment Nudging 轻推后处理
- 完成日期：2026-06-28
- 性能基准（release，7轮中位数）：
  - layout-stress-nested: 4.41ms（vs X-1的5.97ms，nudge开销可忽略）
  - k8s-tenant-isolation: 27.48ms（vs X-1的28.31ms）
  - ai-agent-docops-pipeline: 5.36ms（vs X-1的6.44ms）
- 重合统计（after X-2，对比X-1基线）：
  - layout-stress-nested: exact_overlap=1→**0**（**100%消除！**）, tight_spacing=0→0
  - k8s-tenant-isolation: exact_overlap=1→1, tight_spacing=1→1（残余1+1处因空间约束无法nudge）
  - ai-agent-docops-pipeline: exact_overlap=11→9（18%↓）, tight_spacing=1→1
- 全量showcase扫测结果（22个文件）：
  - **7个文件完全干净**（0重合0间距不足）：c.data-lineage-platform, c.layout-stress-nested, c.mcp-server-cluster-architecture, n.data-pipeline, n.microservices, s.client-api-db, s.three-tier
  - 简单/中等密度图（<15边）基本全干净
  - 高密度k8s图仍有残余重合（c.k8s-multi-namespace-overview 24处, c.k8s-multi-cluster-federation 21处），属X-3范畴
- Nudge统计：
  - layout-stress-nested: 1轮, 1段nudge成功, 0失败
  - k8s-tenant-isolation: 1轮, 0段nudge成功, 5失败（残余冲突两侧均有障碍）
  - ai-agent-docops-pipeline: 2轮, 2段nudge成功, 25失败（高密度导致大量冲突无法通过单段nudge解决）
- 测试：882个全部通过
- 编译：0 warning
- 新增文件：
  - `edge_routing_orthogonal/nudge.rs`：nudge核心实现
- 新增API：
  - `nudge::nudge_conflicting_segments(...)`：多轮nudge主函数
  - `NudgeStats { nudge_rounds, nudged_segments, nudge_failed }`：nudge统计
  - OrthoDebugStats新增字段：nudge_iterations, nudged_segments, nudge_failed
- 算法设计要点：
  1. 在X-1 reroute后执行，收集仍违反间距的中段（非stub）
  2. 按段长度降序排列（长重合段优先处理），边索引+段索引升序tiebreak
  3. 对每个冲突段尝试垂直平移EDGE_PARALLEL_GAP距离（6px），在段两端插入Z形补偿折点保持正交
  4. 交替选择方向（奇索引段+Y/-X，偶索引段-Y/+X），避免所有段往同一方向挤
  5. nudge前三重验证：①不穿节点 ②不与其他边产生新间距违规 ③端点锚点不动
  6. 全距离失败则不降级（不尝试半距离，因为半距离仍然会间距不足）
  7. 最多3轮迭代，本轮无成功nudge时提前收敛
  8. 端点锚点绝对不动：只对中段（非si=0、非si=n-1）操作，points[0]和points[last]不变
- 性能分析：
  - nudge开销极低（<0.1ms），因为它是局部几何操作，不涉及候选路径搜索
  - nudge失败率在高密度图中较高（25/27=93% in ai-agent），这是正确的保守行为
  - 高密度图中多条边挤同一通道，单段nudge无法创造空间，需要X-3 lane assignment

### X-3：Lane Assignment（如需要）
- 完成日期：
- 性能基准：
