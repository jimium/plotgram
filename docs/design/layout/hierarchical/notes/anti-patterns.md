# Hierarchical · 踩过的坑

> 父页：[architecture](../architecture.md) · 尺子：[write-authority](../../write-authority.md)  
> 本文只登记**已实测否决或已拆掉的路线**。现行契约在 `phases/`；yFiles 真值见 [mech.layout-styles.yfiles.md](mech.layout-styles.yfiles.md)。

审查时先看 [architecture §13](../architecture.md) 的通则，再看下表：表里是「试过、回退、不要再试」。

---

## 1. 从 v1 Atlas 拆掉的壳

v1 源码仍在 `crates/v1/tautcore-core/src/layout/atlas/`（只读）。可借鉴的形状（奇偶坐标、构建期切割、构造 + 独立 verifier）已进现行 Channel；下面三条**不要搬回来**。

| 勿做 | 为何 |
|------|------|
| 事后修几何 | 度量入口改 along / lane；Ink 平移根廊；组框定后再 nudge 组内点；post-Ink 刚体推组。每层不信任上游，正确性无法自反证。 |
| 图种分支 | `preset_for(diagram)` / 把 hub-client 写进 P1 objective。违反 ADR-001。 |
| 三路径壳 | `solve_atlas_flat/weak/strong` + 三套 metric tail。Weak / Strong 是收缩**参数**，同一 Plan schema。 |
| 组内另起 Sugiyama | 共享主栈（rank / order / `J(x)`）。跨组边进 objective 或 Demand，禁止 `nudge_intra_nodes_*`。 |
| 全局可变 override 传 sizing | 并发不安全；走 params。 |

可保留的形状：走廊 `ext` 奇偶格（`2j = gap`，`2j+1 = 节点体`）让切割由编码产出；Substrate 构建期拒穿组，搜索层独立复证（不复用 ScopeMask 代码路径）。

---

## 2. 次轴 / 端口（已实测）

| 勿做 | 实测 | 该谁写 |
|------|------|--------|
| `SymmetryPlan` + `claimed` 认领表 | 树上精确、DAG 一出现扇心可偏 220px；再加谓词是熔断点 | `J(x)` + 少量硬约束 + typed 权重 |
| 主臂 / twin `desired` 1e6 绝对锁 | n19 粘在 n18 下，而不是 n0/n1 脊 | 相对项进 J；IPSEP 主路径不 1e6 |
| 「最短跨唯一」主臂身份（boost + snap 吸叶到 hub 轴） | n19 仍粘在 n18 下（n18→n19 span=1 唯一最短）；审批扇靠 exclusive / λ_sym / PortLane 已够 | 不恢复该资格；扇叶不进 exclusive 1:1 |
| 凡 dummy 三点加权 / 最长唯一通路赢家 | 长侧枝（n18→n25、check→approved）压过短脊；n19 会偏向 n24 而非 n0 | exclusive 才加 ψ(L)；扇出按 mass 分流，不赢家通吃 |
| 给菱形「是」加 through-hop / 把 1:1 改成 `κ=1/(down·up)` 写进 J 或 FanPack 插值 | 与最短跨同类。J：`check→finance` 与菱形「是」同 κ，order-approval 脊裂 2–4px；兄弟归一化等于认领 through-hop。FanPack 按 κ 插值：desired 被层内分离吃掉，software-release 脊跨度仍 ~156px | 1:1 与扇出是同一自由度的两端；incoming 茎权必须明显大于 outgoing 半 1:1，否则伤 D2。不加第三趟 |
| 重心方案 C：不铺扇出 hub 孩子 + mass 过半直通叶贴父轴 | software-release 脊跨度仍 ~154px（分离吃掉 desired）；D2 `check` 偏 1px；n19 离 n1 更远 | 不恢复；重心不能当第三趟 snap |
| 直通节点沿唯一父上溯加 `|x_c−x_anc|`（最多 3 层） | 流水线脊跨度仍 ~156px：`unit_test` 已被右侧 notify 回边 dummy 廊顶在分离下限（cx 152 vs dummy 212），祖先列 191 不可行。mech：`n1` 的唯一父链是 `n19/n18`，上溯把脊拉向短支（n0/n1 裂开，n19−n1 ~17px） | 唯一父 ≠ 主链。跨层直度不能当臂身份；dummy 占列是 P3/链列的事，不是再加 P4 项。勿与 `w_end` 长边端点项一起再试 |
| 混合扇短边权 + 单 dummy 由扇 hub 写列 | n13−n11 仅 90→84px（yFiles 31）；dummy 可贴 n11，短叶仍停在廊墙外侧。稍宽的「混合扇」谓词会把 n19 粘回 n18、n1 粘到 n0 | 短叶列不是再加认领谓词能写的；廊墙在 VPSC 硬分离。勿恢复 `fan_short_hop_boost` / `unique_short_side_leaf` |
| 加大 `λ_sym`（1→4）加强扇心 | n19 跟孩子走：n19−n1 从 ~0 裂到 51px；n19 偏向 n0/n24 中点而非 n1。D2 脊仍焊。门禁绿但丢茎流刚换来的 n19≈n1 | 扇心就是孩子重心。加力 = 下定心，会离开 1:1 脊。n19 不是 keep_j（父 n18 已扇出）。勿再调 `lambda_sym` 默认 |
| Dummy 在 VPSC 里不当实节点（触及 dummy 的对 extra=`edge_gap`，宽仍 0） | n13−n11 仅 90→76px（yFiles 31）——dummy 仍夹在短叶和父列之间，两侧各省 8px 不够。D3 FanPack 漂 ~5px；n19−n1 裂 ~5px | 不是「虚节点宽=edge_gap」（那条另否决）。实–虚分离不是把 dummy 从层内公民里请出去；廊墙还在。下一刀是 P3 链块 / 源 hub 写列 |
| 扇出 `center_h` 用加权中位 | 两点时重孩子通吃，等于又认领主臂 | 加权重心；等质量回退中位 |
| 两端等权折中写长链列 | 廊停在两端中间（e18 把 L2 整层顶开 ~90px） | ≥2 dummy：约束层链恒等；desired 层单写者（一非叶端，或两端 hub 时本层更靠边缘的那端） |
| port-anchor 放进 IPSEP 迭代 | 整图无界左漂 | 只在终局 snap 写 dummy desired |
| 长边 hub snap 回子心 | 撤销 J 刚拉直的线性段（n5 离开 e30） | IPSEP 下连 dummy 的 hub、1:1 茎上的扇出 hub **保持 J** |
| 扇入去 FanPack 父节点 | 1:1 茎被拽开，汇点看起来贴 median 父 | 只铺 `down_deg ≥ 2` 的叶 |
| 两端都是 hub 的茎强焊 | n6–n10：子贴父拖散簇；父贴子撞分离 | 该子保持 J |
| 悬挂汇点叶焊进 1:1 茎 | 撤销 D 的「叶跟廊」（n14） | 叶跟 dummy 列，不反向拽廊 |
| 让带 dummy 的扇入汇点也 `center_h` | ticket-triage / 主臂门禁红 | 长边 hub 的扇入汇点保持 J（n12 偏左是已知缺口，改 J 而不是撤这条） |
| 虚节点宽 = `edge_gap` | crossings / 折数涨、撑画布；R1+R2 之后再试仍否决 | 先有 track/走廊预算再议 ρ；失败时先查下降步是否把一层映到少数 x |
| 端点直度项 `w_end` 拉跨层两端 | 折从一端搬到另一端；max_bends 升 | P3 先让链能整块换侧；P4 不要用端点项掩盖错列 |
| order 给 reverse dummy 加远真实端 bary 偏置 | 量纲错（跨层 order 下标）；crossings 升 | 链块级 sift；不要单元素偏置 |
| Compose 槽序用远真实端 `layer_order` | 跨层不可比；e29 倒在 n11 最左，P4 把廊锚错槽，e6×e29 交叉 | 邻层邻接元（含 dummy）；dummy 与叶左右反了是 P3 的事 |
| 内分带外扩到脸上重叠的对端 | 共享脸扇贴圆角 | 内分带 `[1/(2n), 1−1/(2n)]`；追逐不超过 `min(半 inset, 半 port_pitch)` |
| 单槽脸**无界**滑口省折 | 箭头扎在盒子角上（`fan-out-four`） | 有界漂移仍归 PortLane（n=1 带塌成中心，只开 `min(宽/4, pitch/2)`）；窗口外的 Δx 才是 cross 轴。勿把砸角写成「单槽不动」——见 [incomplete-policy](incomplete-policy.md) |
| `forward_gutter_side` 掩盖错列 | e18 走到与 yFiles 相反的一侧 | 改 P4 列，不在下游加第三趟侧别 |

声明表熔断的判断句（写权 §2.2）：修法若是「再加一个谓词 / continue」，先问 `J` 缺哪一项。

---

## 3. Channel / Ink / 组

| 勿做 | 为何 |
|------|------|
| Ink 扫框后改折线拓扑（`clear_main_x` / stub 在像素期二选一走廊） | L2 是 Channel 的自由度；Ink 越权会让 TrackOrder 与几何脱节。根因曾是 Substrate 不建模节点占位——补在 Channel，不要在 L5 发明 |
| Ink `append_bend` 用 `(from.y+to.y)/2` | 同层跨边共 mid_y，假 bus。归 TrackOrder |
| 静默 `channel-group-fallback` 且不进 `relaxations` | 回退后等同无组；必须可观测 |
| Weak 框由 finalize `union+pad` 当真源 | 边穿过事后才出现的 pad 带。Weak 框 = Metric；Strong 框 = MacroBlockWriter |
| 同一 policy 下 MacroBlock 再叠 VPSC 框 | 双写者 |
| 用 group Horizontal 冒充泳道 | ADR-008：泳道是 PartitionGrid |
| per-edge 递减试验降 fallback 次数 | 无效已撤回；根因在构造（组通道 / 基片），不要再调这一档 |

---

## 4. 仍开放、但已知走不通的捷径

这些是**当前缺口**，不是可以再试一遍的补丁：

- **扇出下的短链贴父槽**（n13 相对 n11 仍偏左；n19 已离开 n18，剩余是贴 n0/n1 脊约 30px）：P3 廊已在短链右侧。再对调父脸槽序、再让带 dummy 的扇入汇点 `center_h`、再在单 dummy 链上拉扇入端跟廊、再恢复最短跨主臂身份、再把 dummy–real 分离改成 `edge_gap`，会打 D3 / 上游轴继承 / 把 n19 粘回去。廊墙是层内序，不是分离常数。下一刀是 P3 链块 / 源 hub 写列。
- **dense 层整层 `node_gap` 取等**：投影在吃 slack。不要加宽 dummy，先查下降步。
- **大件齐了还差十几折 / 一对交叉**：先读 [incomplete-policy](incomplete-policy.md)（PortLane / TrackOrder 政策是否写窄），不要第一刀去拧 `J` / Channel / Ink。
- **穿组构造清零、label/loop reserve**：见 [roadmap](../roadmap.md)，不是再开一张 claimed 表。
