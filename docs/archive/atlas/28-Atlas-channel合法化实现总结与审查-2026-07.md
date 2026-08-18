# 28 · Atlas channel 合法化实现总结与审查（2026-07-26）

> 实现对象：[`crates/tautcore-core/src/layout/atlas/channel/`](../../crates/tautcore-core/src/layout/atlas/channel/)（substrate / graph / search / derive / bundle）+ 探针门面 [`atlas/probe.rs`](../../crates/tautcore-core/src/layout/atlas/probe.rs) + [`atlas_probe`](../../crates/tautcore-eval/src/bin/atlas_probe.rs)
> 依据需求：[`27-Atlas-channel模块审查与改造需求`](27-Atlas-channel模块审查与改造需求-2026-07.md) 的 **L1–L8** 合法化改造
> 验收判据：27 号文 §7 **A1–A10**
> 重采数据：已回写 [`25-Atlas-相I可行率探针报告`](25-Atlas-相I可行率探针报告-2026-07.md) §8（本文 §4 为摘要）

---

## 0. 一页结论

**27 号文的两条 Blocker（F1 穿组词典序最优、F2 gate side 装饰化）已由构造消除，且经三重独立证据复核为 0。**

| 判定 | 内容 |
|---|---|
| ✅ **L1–L8 全部落地**（L7 部分留债） | 段模型 + 相交才 link、边界线 gate 配对、span_weight、容量输出化、探针三口径、穿透检查器、作用域掩码，一批交付 |
| ✅ **合法性红线三集归零** | 77 图 / 1144 边：A1 组穿透 = 0、A2 借道穿组 = 0（改造前 27）、A3 同 gate 双穿 = 0（改造前 116）；另 stress 集 8 条自环边按 L7-T5 显式排除（见 §6） |
| ✅ **表达能力在合法口径下仍 100%** | A4 合法单边可行率 265+180+699 全成功；L8 掩码的可行性副作用为零 |
| ✅ **A5 峰值 lane 三集全降** | product 10→9、stress 33→25、demo 25→21；stress-deep「lane 33 根因是 span_weight 恒 1」的诊断（27 号文 F4）被证实 |
| ✅ **性能反而更快** | channel 本体三集合计 ≈3.6 ms（改造前 ≈35 ms，预算 100 ms）；段数上升未兑现成成本，绕远消失反而省了搜索 |
| ✅ **独立代码审查无高/中问题** | CodeReview 子代理逐文件复核：不变量实现、确定性、占用可逆性均正面确认；唯一提示为组数 O(G²) 的构建期检查（当前规模合理） |
| ⚠️ **留债两项** | L7-T3（slots_per_side 语义）、T4（多源 Dijkstra）；T5（自环）已于同日追加修复（见 §6）；另探针仍是 flat 口径（生产接线需镜像真实分区布局） |

**含义**：channel 的「构造保证合法」这句话现在是真的——穿组与双穿在图上不可表达（L1/L2/L8），残余风险由 L6 检查器自反证兜底。25 号文 §6.1 被 27 号文推迟的「进入默认路径」判断，在新口径数据下恢复成立，可推进 23 号文 Stage 1 生产接线。

## 1. L1–L8 落地明细

按 27 号文 §8 实施次序 L1 → L2 → L8 → L6 → L3 → L4 → L5 单批交付：

| 层 | 内容 | 锚点 |
|---|---|---|
| L1 | 奇偶坐标切割算法 + `Track{line, ext}` 段模型；link 仅在几何相交处建立（构建期校验，跨线跳跃不可表达） | `substrate.rs` / `derive.rs` |
| L2 | `Gate{line, crossings}` 沿边界线逐对配对；G-inv-1/2/3 构建期拒绝；B7 退化轴（组占满一轴）不设 gate | `substrate.rs` / `derive.rs` |
| L8 | `ScopeMask`（`{None} ∪ chain(u) ∪ chain(v)`）在邻居松弛处硬过滤，Dijkstra 本体不动；同线直穿无关组不可表达 | `search.rs` |
| L6 | `verify_no_group_penetration()` 返回违规清单；探针每图先跑（A1 独立证据链） | `substrate.rs` |
| L3 | `span_weight` = ext 内 gap 数（空 cover 取 1），恒 ≥1；Q4 长度轴恢复有效，绕远不再免费 | `derive.rs` |
| L4 | `GateCapacity::{Unbounded, Fixed}`：生产恒 `Unbounded`（容量是输出不是约束）；`boundary_degree` 预算回环已删；`Fixed` 仅保留为诊断扫描通道 | `substrate.rs` / `graph.rs` |
| L5 | 探针三口径重写：口径一表达上界 / 口径二端口生产（cap=4）/ 口径三 gate 诊断附录 + 需求画像；A1–A3 红线每图输出 | `atlas_probe.rs` / `probe.rs` |
| L7 | T1（links → `BTreeSet`）、T2（随 `boundary_degree` 删除）、T6（注释修正）已做；T5（自环）同日追加修复；**T3 / T4 留债**（见 §6） | — |

### 1.1 实现期定案（27 号文未给数据结构的部分）

- **段的统一表示**：`line` = 所在线 gap 索引；`ext` = 可 link 的垂直延展**闭区间**，奇偶坐标编码（`2j` = gap j、`2j+1` = 第 j 列/层节点体），B8 开闭规则折算进区间端点，`covers_gap` / `covers_slot` 构造性保证。
- **空 cover**：只经 gate/端口的组内段（如单列组）用 `ext.0 > 偶坐标上界` 自然表达，无需特殊枚举值。
- **端口宿主解析**：取包含节点体奇坐标的段；贴组边节点因「边界线不被切」恒得根段，B7 退化组自动成立。
- **确定性**：`cut_line` 的 scope 归属按「深度降序 + GroupId 升序」平局；邻居按 `(to, via_key)` 排序；gate 占用 `BTreeSet` 去重——全链路无 HashMap 序依赖（AGENTS.md §2）。

### 1.2 探针期补充定案（重采中暴露，27 号文未预见，已回写 [`channel/README.md`](../../crates/tautcore-core/src/layout/atlas/channel/README.md)）

1. **组矩形嵌套树前提显式化**：切割模型要求任两组矩形「分离或有祖先关系」且矩内无非后代节点。derive 构建期拒绝（`OverlappingGroups` / `ForeignNodeInGroupRect`）。LayeredKernel flat 网格不做组感知布局，两种病态都会出现：探针门面（`probe.rs::sanitize`）确定性丢弃病态组，**含被丢空容器组的级联清理**（自叶向根不动点；修复 `demo.k8s-platform-stack` 整图 `EmptyGroup` 误判——4 个子组被丢后父容器成空组导致 30 边全灭）。生产接线（Legacy Adapter）镜像真实分区布局，不经此路径。三种病态各有单测钉死。
2. **探针端点口径 = 四侧候选**（24 号文 R5 `derive_node_ports` + R3 `route_candidates`）：B7 退化轴的组内边正解是**边界缝侧端口**，钉死单一侧对会人为制造同 gate 双穿（假 A3，曾残余 4 处）；四侧候选下三集 A1/A2/A3 恒 0。口径二为拿到获胜端口对做占用提交，手动展开候选循环并复刻平局规则（升序遍历 + 严格 `<`）。

## 2. 验收判据 A1–A10 结果

| # | 判据 | 目标 | 实测 | 结果 |
|---|---|---|---|---|
| A1 | `verify_no_group_penetration()` 全集通过 | 77 图全通过 | product 30 + stress 8 + demo 39 全 0 违规 | ✅ |
| A2 | 借道穿无关组 | **0** | 三集路径复核恒 0（改造前 27/1152 下界） | ✅ |
| A3 | 同 gate 双穿 | **0** | 三集恒 0（改造前 116/1152） | ✅ |
| A4 | 合法单边可行率 | ≥ 99% | 265/265 · 180/180 · 699/699 = **100%**，失败边为空（stress 8 条自环显式排除，见 §6 T5） | ✅ |
| A5 | 峰值 lane demand 下降（L3 效果度量） | 显著下降 | product 10→**9** · stress 33→**25** · demo 25→**21** | ✅ |
| A6 | 端口争用出数（生产口径） | 出数并逐图分析 | 全边成功、争用失败 0、峰值侧负载 ≤4（cap=4 未饱和） | ✅ |
| A7 | channel 本体耗时（三集合计） | ≤ 100 ms | **≈3.6 ms**（0.53 + 0.17 + 2.92） | ✅ |
| A8 | 确定性双跑 | 逐字段一致 | 三集报告各双跑，diff 逐字节一致 | ✅ |
| A9 | B1–B8 边界用例单测 | 全部钉死 | `tests.rs` 逐条覆盖（含 B7 宿主=边界缝根段、B8 构建期拒绝） | ✅ |
| A10 | L8 副作用边界 | 组横跨全列时外部边仍可行且不含组内段 | 单测钉死（走外框 Converged） | ✅ |

`cargo test -p tautcore-core`：**956 passed · 0 failed**（debug，AGENTS.md §9）。

## 3. 关键设计取舍

1. **「构造拒绝」优先于「事后检测」**：穿组的三种形态各由一层负责——跨线跳跃（L1 相交才 link）、边界穿越不配对（L2 gate 进出必落不同 gate）、同线直穿（L8 掩码）。L6 检查器只做自反证，不参与选路。
2. **gate 容量是输出**：27 号文 F3 指出「自适应容量 = 把容量设成需求」是同义反复；L4 后生产路径 gate 恒 `Unbounded`，crossing demand 作为需求画像输出给度量相撑空间。相 I 唯一硬争用只剩端口侧容量（A6）。
3. **探针病态消解放门面不放内核**：flat 网格的组交叠/不纯是探针口径的人造病态，derive 内核保持严格拒绝（生产接线也受同样保护），`probe.rs` 门面负责确定性 sanitize——内核不为探针弯腰。
4. **B7 退化轴不设 gate**：组占满一轴时边界缝就是「门」本身，组内边走边界缝侧端口（四侧候选自动选中），不引入自穿越的伪 gate。

## 4. 重采数据摘要（详表见 25 号文 §8）

### 4.1 合法性 + 表达（口径一）

| 集 | 图 | 边 | A1/A2/A3 | A4 | 丢病态组¹ |
|---|---|---|---|---|---|
| product | 30 | 265 | 0/0/0 | 100% | 7 |
| stress | 8 | 180² | 0/0/0 | 100% | 3 |
| demo | 39 | 699 | 0/0/0 | 100% | 35 |

¹ flat 口径 sanitize 丢弃的交叠/不纯组数；受影响图的 A2 检验力度打折，生产接线不受此限。
² 另 8 条自环边（dag×2 · lifelines×3 · transitions×3）按 L7-T5 从蓝图口径排除（channel 不建模自环，生产走节点旁小环），探针报告「标注」列与总注记披露。

### 4.2 需求画像（输出给度量相）

| 集 | 峰值 lane（旧→新） | 出处 | 峰值 gate crossing demand（旧→新） | 出处 |
|---|---|---|---|---|
| product | 10 → **9** | ecommerce-platform | 10 → **3** | ecommerce-platform / cloud-native |
| stress | 33 → **25** | layout-stress-deep | 9 → **0** | —（丢交叠组后无 gate） |
| demo | 25 → **21** | k8s-multi-cluster-federation | 18 → **7** | k8s-multi-namespace-overview |

gate 需求大幅下降是 L2 配对 gate 分段摊薄 + 四侧候选分散端点的叠加效果；固定容量诊断扫描 cap=1..8 三集全 100%，固定容量已不构成争用瓶颈（旧口径 cap=1 时 demo 仅 83.5%）。

### 4.3 性能（release，50 次中位数）

| 集 | channel 本体（旧→新） | route 单边均摊（旧→新） |
|---|---|---|
| product | 2.05 → **0.53 ms** | 6.35 → **1.35 µs/边** |
| stress | 0.99 → **0.17 ms** | 4.33 → **0.59 µs/边** |
| demo | 32.3 → **2.92 ms** | 41.9 → **3.53 µs/边** |

段模型让本体再缩 4–11 倍：span_weight 恢复长度轴后绕远路径提前被剪，转移表也因「相交才 link」变稀疏。blueprint（上游分层求解）仍占 98%+，与 25 号文 §7 结论一致。

## 5. 独立代码审查结论

CodeReview 子代理对 substrate / graph / search / derive / bundle / probe / atlas_probe 七个文件独立复核（不带实现者视角）：

**无高危 / 中危 / 低危确定性问题。** 正面确认清单：

- 奇偶坐标编码与 L1 定义严格对齐；`covers_gap` / `covers_slot` 构造性保证 B8 开闭规则，无边界 off-by-one。
- G-inv-1/2/3 均为构建期检查，非法基底根本建不出来。
- L6 检查器几何判定严密：scope 链正确排除祖先组，不误报合法的组内走线。
- `cut_line` 的 scope 平局规则（深度降序 + GroupId 升序）、邻居 `(to, via_key)` 排序、`route_candidates` 升序×升序 + 严格 `<`——全部确定性，无 HashMap 序依赖；`crates/tautcore-core` 内无 `std::time` 裸用（AGENTS.md §2/§6）。
- `Occupancy`：gate 用 `BTreeSet` 按边去重（同边同 gate 恒占 1 单位），commit/release 严格可逆；端口计数 `saturating_sub` 防下溢。
- `bundle.rs` 反向后缀 trie 与新段模型兼容，未受改造影响。
- 测试覆盖良好：B1–B8 逐条、G-inv、L6/L8、A10、确定性双跑、probe 三种病态消解。

**唯一提示（非缺陷）**：derive 的组矩形交叠检测与 probe 的 sanitize 均为 O(G²)（G = 组数）。当前图集 G ≤ 十几，开销可忽略；若未来出现数百组的图再考虑扫描线。属合理取舍，不建议现在优化。

## 6. 遗留债与后续

| 项 | 内容 | 去向 |
|---|---|---|
| L7-T3 | `slots_per_side` 语义仍是「预分配上限」而非按需生长 | Stage 1 生产接线时随端口真实需求一起定 |
| L7-T4 | 候选端点选路是 K×K 循环而非多源 Dijkstra | 性能远未到瓶颈（A7 余量 27 倍），按需再做 |
| L7-T5 | ~~自环（u == v）未定义通道语义，`start == goal` 返回单轨道平凡解~~ **已修（2026-07-26 追加）** | `route` 入口按「两端口同节点」显式判 `Infeasible`，不伪造平凡解；不同节点同宿主轨道（B7 退化组）仍合法。单测 `self_loop_same_node_is_infeasible`，958 tests passed。**修复同时曝光探针数据污染**：stress 集 8 条真自环边（dag×2 · lifelines×3 · transitions×3）此前靠平凡解假成功计入 A4 分母（188 含 8 条假读数）；修后探针蓝图口径显式排除并在报告披露，A4 恢复 180/180 = 100%。若未来建模自环走专门绕行构造 |
| flat 口径 | 探针网格不镜像 divide-conquer 真实几何；35+7+3 个病态组被 sanitize 丢弃，其内部边的 A2 检验力度打折 | Stage 1 接通生产 blueprint（Legacy Adapter 镜像真实分区）后复测——25 号文 §6.5 的既有风险提示，本轮不变 |
| 25 号文旧读数 | §0–§6 旧口径数字保留为改造前基线 | 已按 27 号文 §5.1 表结构在 25 号文 §8 重采回写 |

**下一步**：23 号文 Stage 1 生产接线（Legacy Adapter 出 blueprint → channel 选路 → 对拍现行路由），高压回归样本沿用 25 号文 §4.3 清单。

## 7. 变更清单与验证证据

| 文件 | 变更 |
|---|---|
| `channel/substrate.rs` | `Track{line, ext}` 段模型、`Gate{line, crossings}`、`GateCapacity`、L6 检查器、links → `BTreeSet` |
| `channel/derive.rs` | 奇偶坐标切割算法、相交建 link、L2 配对 gate、L3 span_weight、嵌套树/纯净性构建期拒绝 |
| `channel/search.rs` | `ScopeMask` + `route` / `route_candidates` / `route_node_sides` 掩码参数；T5 追加：`route` 入口同节点显式 `Infeasible` |
| `channel/graph.rs` | gate 按 crossings 展开、`Occupancy` 适配 `GateCapacity` |
| `channel/tests.rs` | 签名迁移 + B1–B8 / G-inv / L6 / L8 / A10 / 确定性 / 组交叠拒绝新单测 |
| `atlas/probe.rs` | sanitize：交叠/不纯组丢弃 + 空容器组级联清理（三种病态单测）；T5 追加：蓝图口径排除自环（单测） |
| `bin/atlas_probe.rs` | 三口径四侧候选重写（口径二手动候选循环复刻平局规则）；T5 追加：自环排除数披露（标注列 + 总注记） |
| `channel/README.md` / `mod.rs` | 分歧点表补 §1.2 两条定案，导出更新 |

验证命令（均可复现）：

```bash
cargo test -p tautcore-core                                                    # 958 passed
cargo run -p tautcore-eval --bin atlas_probe                                   # product：A1/A2/A3=0，265/265
cargo run -p tautcore-eval --bin atlas_probe -- --set benchmarks/sets/stress-probe-set.txt   # 180/180（8 条自环排除）
cargo run -p tautcore-eval --bin atlas_probe -- --set benchmarks/sets/demo-observe-set.txt   # 699/699
cargo run --release -p tautcore-eval --bin atlas_perf                          # A7：本体三集合计 ≈3.6 ms
```

确定性验证：三集探针各双跑，报告逐字节 diff 一致（A8）。
