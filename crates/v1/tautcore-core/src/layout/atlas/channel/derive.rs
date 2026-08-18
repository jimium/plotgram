//! Substrate 推导 adapter（P0）：从「rank/order 网格 + 组矩形」推导通道图基底。
//!
//! **这是把通道图接到真实图的接缝**：上游（未来的 Legacy Adapter，23 号文 1.3）只需提供
//! 节点的 (rank, order)、组树与边列表——全部**无坐标**，本 adapter 负责生成
//! track / link / gate 拓扑。可行率探针据此在 product 集上测量「通道图粒度是否够细」。
//!
//! ## 推导规则（27 号文 L1/L2/L3）
//!
//! - **线与段**：全图每条 Cross 线（rank-gap `0..=rank_count`）/ Main 线
//!   （order-gap `0..=order_count`）被覆盖它的组边界**切成段**，每段独立注册为
//!   track。切点用奇偶坐标编码（`2j` = 垂直方向 gap `j`、`2j+1` = 第 `j` 列/层）：
//!   组在被切线上贡献两个切点——内部起点 `2·o0+1` 与右边界前 `2·(o1+1)`，
//!   全部切点排序去重后一次切完（B4）。边界缝归外段（B8）、共享缝独立一段（B6）、
//!   退化轴不产生组内段（B7）、零长段不注册（B5）全部由该编码自然产出。
//! - **link**：仅对**相交**的异向段建 link（互相覆盖对方所在线的 gap），
//!   同 scope 约束由 [`Substrate::link`] 构建期把守。
//! - **闸口**（L2）：组每侧至多一个 gate，绑定一条具体边界线；`crossings` 为
//!   跨该线紧邻的 (组内段, 组外段) 一一配对（遍历被切线：`MainLow/High` 遍历
//!   `og ∈ (o0, o1]`，`CrossLow/High` 遍历 `k ∈ (r0, r1]`）；配对为空不注册（B7 推论）。
//! - **span_weight**（L3）：= 段覆盖的 gap 数（`ext` 内偶坐标个数），空 cover 取 1。
//! - **容量**（L4）：生产路径恒 [`GateCapacity::Unbounded`]（穿越数是输出不是约束）；
//!   `gate_capacity_override` 仅供探针诊断扫描映射 [`GateCapacity::Fixed`]。

use super::search::ScopeMask;
use super::substrate::{
    GateCapacity, GateSide, GroupId, PortSide, PortSlotId, Substrate, SubstrateError, TrackId,
    TrackOrient,
};
use std::collections::{BTreeMap, BTreeSet};

/// 节点在基底中的槽位（无坐标，只有 rank/order 序号）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeSpec {
    pub rank: usize,
    pub order: usize,
}

/// 组规格：成员节点名 + 父组名（`None` = 顶层直属）。
///
/// gate 容量不再是输入（27 号文 L4）：生产路径 `Unbounded`，诊断扫描走
/// [`ChannelBlueprint::gate_capacity_override`]。
#[derive(Debug, Clone)]
pub struct GroupSpec {
    pub members: Vec<String>,
    pub parent: Option<String>,
}

/// 通道图蓝图：推导 Substrate 的无坐标输入契约。
#[derive(Debug, Clone, Default)]
pub struct ChannelBlueprint {
    /// 节点名 → 槽位。
    pub nodes: BTreeMap<String, NodeSpec>,
    /// 组名 → 规格。
    pub groups: BTreeMap<String, GroupSpec>,
    /// 边列表 (源节点名, 目标节点名)。
    pub edges: Vec<(String, String)>,
    /// 诊断用：强制所有组 gate 取精确容量 [`GateCapacity::Fixed`]。
    ///
    /// `None`（默认 / 生产路径）= [`GateCapacity::Unbounded`]——容量是输出
    /// （crossing_demand），不是可行性约束；`Some(c)` 仅供探针做
    /// 「固定容量 vs 争用成功率」扫描（L4/L5，不进生产结论）。
    pub gate_capacity_override: Option<u32>,
}

/// 推导期错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeriveError {
    /// 组成员引用了未声明节点。
    UnknownNodeInGroup { group: String, node: String },
    /// 组子树完全无节点（无直系成员且无含节点的子组），无法定矩形。
    EmptyGroup { group: String },
    /// 边引用了未声明节点。
    UnknownNodeInEdge { from: String, to: String },
    /// 两组矩形交叠且无祖先关系（切割模型要求组矩形构成嵌套树）。
    OverlappingGroups { a: String, b: String },
    /// 非后代节点落在组矩形内部（宿主段 scope 归属错乱，掩码必拒其出入）。
    ForeignNodeInGroupRect { group: String, node: String },
    /// 基底构建错误透传。
    Substrate(#[allow(dead_code)] String),
}

impl From<SubstrateError> for DeriveError {
    fn from(e: SubstrateError) -> Self {
        DeriveError::Substrate(e.to_string())
    }
}

/// 段引用（索引用轻量拷贝，避免查段属性时借用 Substrate）。
#[derive(Debug, Clone, Copy)]
pub struct SegmentRef {
    pub id: TrackId,
    /// 沿线延展（奇偶坐标闭区间，与 [`super::substrate::Track::ext`] 一致）。
    pub ext: (usize, usize),
    pub scope: Option<GroupId>,
}

impl SegmentRef {
    fn covers(&self, coord: usize) -> bool {
        self.ext.0 <= coord && coord <= self.ext.1
    }
}

/// 推导产物索引：「线 → 有序段列表」+ 节点归属，供探针挂接端口与路由。
#[derive(Debug, Clone)]
pub struct BlueprintIndex {
    /// Cross 线：rank-gap → 按 `ext` 升序的段列表。
    pub cross_lines: BTreeMap<usize, Vec<SegmentRef>>,
    /// Main 线：order-gap → 按 `ext` 升序的段列表。
    pub main_lines: BTreeMap<usize, Vec<SegmentRef>>,
    /// 组名 → GroupId（L8 掩码构建用）。
    pub group_ids: BTreeMap<String, GroupId>,
    /// 节点名 → 所属区域（`None` = 根；最深成员组）。
    pub node_region: BTreeMap<String, Option<String>>,
    /// 节点名 → 已挂接的端口 id（确定性排序；24 号文 R5）。
    ///
    /// 由 [`derive_node_ports`] 填充；手工挂接场景下为空。
    pub node_ports: BTreeMap<String, Vec<PortSlotId>>,
}

impl BlueprintIndex {
    /// Cross 线 `rg` 上覆盖第 `order` 列节点体位置（奇坐标 `2·order+1`）的段。
    ///
    /// 宿主规则（L1）：组内节点得组内段；退化轴 / 贴边界侧自动得边界缝外段（B7/B8）。
    pub fn cross_at(&self, rg: usize, order: usize) -> Option<TrackId> {
        self.cross_lines
            .get(&rg)?
            .iter()
            .find(|sg| sg.covers(2 * order + 1))
            .map(|sg| sg.id)
    }

    /// Main 线 `og` 上覆盖第 `rank` 层节点体位置（奇坐标 `2·rank+1`）的段。
    pub fn main_at(&self, og: usize, rank: usize) -> Option<TrackId> {
        self.main_lines
            .get(&og)?
            .iter()
            .find(|sg| sg.covers(2 * rank + 1))
            .map(|sg| sg.id)
    }

    /// 为边端点选端口轨道：跨 rank 用 Cross 缝，同 rank 用 Main 走廊。
    /// 返回 (源端口轨道, 目标端口轨道)。
    ///
    /// 段按节点体位置取（[`Self::cross_at`] / [`Self::main_at`]），节点自身相邻的
    /// gap 恒在全局线范围内，无需夹取；跨组仍经 gate（H2 由段模型保证）。
    pub fn edge_port_tracks(
        &self,
        blueprint: &ChannelBlueprint,
        from: &str,
        to: &str,
    ) -> Option<(TrackId, TrackId)> {
        let sf = blueprint.nodes.get(from)?;
        let st = blueprint.nodes.get(to)?;
        let src = if st.rank > sf.rank {
            self.cross_at(sf.rank + 1, sf.order)
        } else if st.rank < sf.rank {
            self.cross_at(sf.rank, sf.order)
        } else {
            // 同 rank 横向：取朝向目标的 order-gap
            let og = if st.order >= sf.order {
                sf.order + 1
            } else {
                sf.order
            };
            self.main_at(og, sf.rank)
        }?;
        let dst = if sf.rank > st.rank {
            self.cross_at(st.rank + 1, st.order)
        } else if sf.rank < st.rank {
            self.cross_at(st.rank, st.order)
        } else {
            let og = if sf.order >= st.order {
                st.order + 1
            } else {
                st.order
            };
            self.main_at(og, st.rank)
        }?;
        Some((src, dst))
    }

    /// 分配一个新端口槽 id（探针挂接端口用）。
    pub fn alloc_port_id(substrate: &Substrate) -> PortSlotId {
        substrate.alloc_port_id()
    }

    /// 节点归属作用域（`None` = 根；按 [`Self::node_region`] 最深成员组）。
    pub fn node_scope(&self, node: &str) -> Option<GroupId> {
        match self.node_region.get(node) {
            Some(Some(g)) => self.group_ids.get(g).copied(),
            _ => None,
        }
    }

    /// 为边 `(from, to)` 构建 L8 作用域掩码（探针/生产入口）：
    /// `{None} ∪ chain(scope(from)) ∪ chain(scope(to))`。
    pub fn scope_mask_for_edge(&self, substrate: &Substrate, from: &str, to: &str) -> ScopeMask {
        ScopeMask::for_scopes(substrate, self.node_scope(from), self.node_scope(to))
    }

    /// 解析节点某侧的宿主轨道（24 号文 R2，纯函数）。
    ///
    /// 约定（与 [`Self::edge_port_tracks`] 一致，在 README 写死）：节点槽位 `(rank, order)`，
    ///
    /// | side | 宿主轨道（相对节点 slot） |
    /// |------|------------------------------|
    /// | `MainLow`  | Cross 线 `rank` 上覆盖该列的段 |
    /// | `MainHigh` | Cross 线 `rank+1` 上覆盖该列的段 |
    /// | `CrossLow` | Main 线 `order` 上覆盖该层的段 |
    /// | `CrossHigh`| Main 线 `order+1` 上覆盖该层的段 |
    ///
    /// 全局线恒覆盖全图（根段全延展），故对合法蓝图恒有解；未知节点 → `None`，
    /// 调用方不得静默挂到错误 track。
    pub fn resolve_host_track(
        &self,
        bp: &ChannelBlueprint,
        node: &str,
        side: PortSide,
    ) -> Option<TrackId> {
        let slot = bp.nodes.get(node)?;
        match side {
            PortSide::MainLow => self.cross_at(slot.rank, slot.order),
            PortSide::MainHigh => self.cross_at(slot.rank + 1, slot.order),
            PortSide::CrossLow => self.main_at(slot.order, slot.rank),
            PortSide::CrossHigh => self.main_at(slot.order + 1, slot.rank),
        }
    }
}

/// 从蓝图推导 Substrate + 索引。
pub fn derive_substrate(bp: &ChannelBlueprint) -> Result<(Substrate, BlueprintIndex), DeriveError> {
    let mut s = Substrate::new();

    // 1. 组矩形 + GroupId 分配（按组名排序，确定性）
    // 组矩形覆盖**全部后代节点**（直系成员 + 子组成员）：纯容器组（自身无直属节点、
    // 只含子组，如 cloud-native 的 k8s ⊃ apps/platform）的矩形即其子树包围盒，
    // 不再因「无直属成员」误报 EmptyGroup。
    let children: BTreeMap<String, Vec<String>> = {
        let mut m: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (gname, spec) in &bp.groups {
            if let Some(p) = &spec.parent {
                m.entry(p.clone()).or_default().push(gname.clone());
            }
        }
        m
    };
    let mut group_rect: BTreeMap<String, (usize, usize, usize, usize)> = BTreeMap::new();
    let mut group_desc: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for gname in bp.groups.keys() {
        let descendants = collect_descendants(gname, bp, &children);
        if descendants.is_empty() {
            // 组子树完全无节点（无直系成员且无含节点的子组）→ 无法定矩形。
            return Err(DeriveError::EmptyGroup {
                group: gname.clone(),
            });
        }
        let (mut r0, mut r1) = (usize::MAX, 0usize);
        let (mut o0, mut o1) = (usize::MAX, 0usize);
        for m in &descendants {
            let slot = bp.nodes.get(m).ok_or_else(|| DeriveError::UnknownNodeInGroup {
                group: gname.clone(),
                node: m.clone(),
            })?;
            r0 = r0.min(slot.rank);
            r1 = r1.max(slot.rank);
            o0 = o0.min(slot.order);
            o1 = o1.max(slot.order);
        }
        group_rect.insert(gname.clone(), (r0, r1, o0, o1));
        group_desc.insert(gname.clone(), descendants.into_iter().collect());
    }
    // 组矩形必须构成嵌套树（L1 切割模型前提）：任两组或矩形分离、或有祖先关系。
    // flat 口径下兄弟组包围盒可能交叠（上游未做组感知布局），此处构建期显式拒绝
    // ——否则段 scope 归属歧义，L6 必报违规（A1）。flat 口径的消解见
    // `atlas::probe::derive_channel_blueprint`（丢弃交叠组并在探针报告标注）。
    let rect_names: Vec<&String> = group_rect.keys().collect();
    for i in 0..rect_names.len() {
        for j in i + 1..rect_names.len() {
            let (a, b) = (rect_names[i], rect_names[j]);
            if is_ancestor(a, b, &bp.groups) || is_ancestor(b, a, &bp.groups) {
                continue;
            }
            let ra = group_rect[a];
            let rb = group_rect[b];
            // 闭区间相交（共享任一 (rank, order) 槽即两组同争一格）
            if ra.0 <= rb.1 && rb.0 <= ra.1 && ra.2 <= rb.3 && rb.2 <= ra.3 {
                return Err(DeriveError::OverlappingGroups {
                    a: a.clone(),
                    b: b.clone(),
                });
            }
        }
    }
    // 组矩形纯净性（同为切割模型前提）：矩形内不得有非后代节点——否则该
    // 节点的宿主段 scope 落在别组，其边的 L8 掩码不含该组，选路必爆 A2/A3。
    for (gname, &(r0, r1, o0, o1)) in &group_rect {
        let desc = &group_desc[gname];
        for (nname, slot) in &bp.nodes {
            if r0 <= slot.rank
                && slot.rank <= r1
                && o0 <= slot.order
                && slot.order <= o1
                && !desc.contains(nname)
            {
                return Err(DeriveError::ForeignNodeInGroupRect {
                    group: gname.clone(),
                    node: nname.clone(),
                });
            }
        }
    }
    // 按深度升序注册组（父先于子），回填 parent GroupId；组矩形交给 L6 检查器
    let mut group_names: Vec<String> = bp.groups.keys().cloned().collect();
    group_names.sort_by_key(|g| (depth_of(g, &bp.groups), g.clone()));
    let mut group_id: BTreeMap<String, GroupId> = BTreeMap::new();
    for gname in &group_names {
        let spec = &bp.groups[gname];
        let parent = spec.parent.as_ref().map(|p| group_id[p]);
        let (r0, r1, o0, o1) = group_rect[gname];
        let gid = s.alloc_group_id();
        s.add_group(gid, parent, (r0, r1), (o0, o1))?;
        group_id.insert(gname.clone(), gid);
    }

    let rank_count = bp.nodes.values().map(|n| n.rank + 1).max().unwrap_or(0);
    let order_count = bp.nodes.values().map(|n| n.order + 1).max().unwrap_or(0);

    // 2. 切割注册段：每条线独立切（B1–B8 全部由奇偶编码自然产出）
    // 切割组三元组：(深度, gid, 沿线内部奇偶区间)——深者优先定 scope。
    let mut cross_lines: BTreeMap<usize, Vec<SegmentRef>> = BTreeMap::new();
    for k in 0..=rank_count {
        let mut cutters: Vec<(usize, GroupId, (usize, usize))> = Vec::new();
        for gname in &group_names {
            let (r0, r1, o0, o1) = group_rect[gname];
            // B1：组切开 rank-gap r0 < k ≤ r1 的 Cross 线
            if r0 < k && k <= r1 {
                cutters.push((
                    depth_of(gname, &bp.groups),
                    group_id[gname],
                    (2 * o0 + 1, 2 * o1 + 1),
                ));
            }
        }
        let segs = cut_line(&mut s, TrackOrient::Cross, k, 2 * order_count, &cutters);
        cross_lines.insert(k, segs);
    }
    let mut main_lines: BTreeMap<usize, Vec<SegmentRef>> = BTreeMap::new();
    for og in 0..=order_count {
        let mut cutters: Vec<(usize, GroupId, (usize, usize))> = Vec::new();
        for gname in &group_names {
            let (r0, r1, o0, o1) = group_rect[gname];
            // B2：组切开 order-gap o0 < og ≤ o1 的 Main 线
            if o0 < og && og <= o1 {
                cutters.push((
                    depth_of(gname, &bp.groups),
                    group_id[gname],
                    (2 * r0 + 1, 2 * r1 + 1),
                ));
            }
        }
        let segs = cut_line(&mut s, TrackOrient::Main, og, 2 * rank_count, &cutters);
        main_lines.insert(og, segs);
    }

    // 3. link：对每对异向段按「区间互含」建（按线索引扫，避免全平方）。
    // Cross 段覆盖的 order-gap 范围 = ext 内偶坐标 → 只扫这些 Main 线。
    for (k, csegs) in &cross_lines {
        for cs in csegs {
            let og_lo = (cs.ext.0 + 1) / 2;
            let og_hi = cs.ext.1 / 2;
            for og in og_lo..=og_hi {
                if let Some(msegs) = main_lines.get(&og) {
                    for ms in msegs {
                        // 互含 + 同 scope（组矩形嵌套树已在构建期钉死，此处
                        // scope 不等即正常的跨 scope 段对，须经 gate）
                        if ms.covers(2 * k) && ms.scope == cs.scope {
                            s.link(cs.id, ms.id)
                                .expect("derive: link intersecting same-scope segments");
                        }
                    }
                }
            }
        }
    }

    // 4. 边界线闸口（L2）：每组每侧至多一个，crossings 为空不注册（B7 推论）
    let capacity = match bp.gate_capacity_override {
        Some(c) => GateCapacity::Fixed(c),
        None => GateCapacity::Unbounded,
    };
    for gname in &group_names {
        let gid = group_id[gname];
        let (r0, r1, o0, o1) = group_rect[gname];
        // MainLow：边界线 rank-gap r0，遍历被切 Main 线 og ∈ (o0, o1]
        add_side_gate(
            &mut s, gid, GateSide::MainLow, r0, &main_lines, o0 + 1, o1, capacity,
        )?;
        // MainHigh：边界线 rank-gap r1+1
        add_side_gate(
            &mut s, gid, GateSide::MainHigh, r1 + 1, &main_lines, o0 + 1, o1, capacity,
        )?;
        // CrossLow：边界线 order-gap o0，遍历被切 Cross 线 k ∈ (r0, r1]
        add_side_gate(
            &mut s, gid, GateSide::CrossLow, o0, &cross_lines, r0 + 1, r1, capacity,
        )?;
        // CrossHigh：边界线 order-gap o1+1
        add_side_gate(
            &mut s, gid, GateSide::CrossHigh, o1 + 1, &cross_lines, r0 + 1, r1, capacity,
        )?;
    }

    // 5. 节点归属区域（最深成员组）
    let mut node_region: BTreeMap<String, Option<String>> = BTreeMap::new();
    for name in bp.nodes.keys() {
        node_region.insert(name.clone(), None);
    }
    // group_names 已按深度升序：子组覆盖父组
    for gname in &group_names {
        for m in &bp.groups[gname].members {
            node_region.insert(m.clone(), Some(gname.clone()));
        }
    }

    // 6. 校验边端点存在
    for (from, to) in &bp.edges {
        if !bp.nodes.contains_key(from) || !bp.nodes.contains_key(to) {
            return Err(DeriveError::UnknownNodeInEdge {
                from: from.clone(),
                to: to.clone(),
            });
        }
    }

    let index = BlueprintIndex {
        cross_lines,
        main_lines,
        group_ids: group_id,
        node_region,
        node_ports: BTreeMap::new(),
    };
    Ok((s, index))
}

/// 切一条线并注册段（B1–B8 的执行处）。
///
/// `cutters` = 切割该线的组 `(深度, gid, 沿线内部奇偶区间)`；每组贡献两个切点：
/// `interior.0`（= `2·o0+1`，左/顶边界之后）与 `interior.1 + 1`（= `2·(o1+1)`，
/// 右/底边界之前）。全部切点排序去重后一次切完（B4）；零长段自动消失（B5）；
/// 相邻组共享缝独立成段（B6）；退化轴两切点重合不产生组内段（B7）；
/// 边界缝 gap（偶坐标）恒落在外段内（B8）。
///
/// 每段 scope = 内部区间包含该段延展的**最深**切割组（深度降序、gid 升序定序，
/// 确定性），无命中 → 根。span_weight（L3）= 段覆盖 gap 数（偶坐标个数），
/// 空 cover（单列组的组内段）取 1——几何宽度下限一个槽。
fn cut_line(
    s: &mut Substrate,
    orient: TrackOrient,
    line: usize,
    full_hi: usize,
    cutters: &[(usize, GroupId, (usize, usize))],
) -> Vec<SegmentRef> {
    let mut cuts: BTreeSet<usize> = BTreeSet::new();
    for &(_, _, interior) in cutters {
        cuts.insert(interior.0);
        cuts.insert(interior.1 + 1);
    }
    let mut bounds: Vec<usize> = Vec::with_capacity(cuts.len() + 2);
    bounds.push(0);
    bounds.extend(cuts.iter().copied().filter(|&c| c > 0 && c <= full_hi));
    bounds.push(full_hi + 1);

    // scope 优先级：深度降序、gid 升序（确定性平局键）
    let mut by_depth: Vec<&(usize, GroupId, (usize, usize))> = cutters.iter().collect();
    by_depth.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));

    let mut segs = Vec::new();
    for w in bounds.windows(2) {
        let (a, b) = (w[0], w[1] - 1);
        if a > b {
            continue; // B5：零长段不注册
        }
        let scope = by_depth
            .iter()
            .find(|(_, _, interior)| interior.0 <= a && b <= interior.1)
            .map(|(_, gid, _)| *gid);
        // L3：span_weight = 覆盖 gap 数（[a, b] 内偶坐标个数），空 cover 取 1
        let lo_even = a + (a & 1);
        let hi_even = b - (b & 1);
        let gaps = if lo_even > hi_even {
            0
        } else {
            (hi_even - lo_even) / 2 + 1
        };
        let id = s.alloc_track_id();
        s.add_track(id, orient, scope, gaps.max(1) as f64, line, (a, b))
            .expect("cut_line: add segment");
        segs.push(SegmentRef {
            id,
            ext: (a, b),
            scope,
        });
    }
    segs
}

/// 注册组一侧的边界线 gate（L2 配对表）：遍历被该组切开的垂直线
/// `lines[lo..=hi]`，每条线上取跨 `boundary` 边界紧邻的 (组内段, 组外段) 对。
///
/// 内段判据：贴边界的组内段（Low 侧 `ext.0 == 2·boundary+1`，High 侧
/// `ext.1 == 2·boundary-1`）且 scope 恰为本组——嵌套组边界重合时最深子组
/// 拿走该段，本组该线不配对（穿越折入子组 gate）。外段判据：边界缝另一侧
/// 紧邻段且 scope 为本组祖先（B8：边界缝归外段）。配对为空不注册。
#[allow(clippy::too_many_arguments)]
fn add_side_gate(
    s: &mut Substrate,
    gid: GroupId,
    side: GateSide,
    boundary: usize,
    lines: &BTreeMap<usize, Vec<SegmentRef>>,
    lo: usize,
    hi: usize,
    capacity: GateCapacity,
) -> Result<(), DeriveError> {
    let mut crossings: Vec<(TrackId, TrackId)> = Vec::new();
    for line in lo..=hi {
        let Some(segs) = lines.get(&line) else {
            continue;
        };
        let low_side = matches!(side, GateSide::MainLow | GateSide::CrossLow);
        let inner = segs.iter().find(|sg| {
            sg.scope == Some(gid)
                && if low_side {
                    sg.ext.0 == 2 * boundary + 1
                } else {
                    boundary > 0 && sg.ext.1 == 2 * boundary - 1
                }
        });
        let outer = segs.iter().find(|sg| {
            s.is_ancestor_scope(sg.scope, gid)
                && if low_side {
                    sg.ext.1 == 2 * boundary
                } else {
                    sg.ext.0 == 2 * boundary
                }
        });
        if let (Some(i), Some(o)) = (inner, outer) {
            crossings.push((i.id, o.id));
        }
    }
    if crossings.is_empty() {
        return Ok(()); // B7 推论：退化轴无穿越线，不注册空 gate
    }
    let id = s.alloc_gate_id();
    s.add_gate(id, gid, side, boundary, crossings, capacity)?;
    Ok(())
}

/// 批量挂端口选项（24 号文 R5）。
#[derive(Debug, Clone)]
pub struct DerivePortsOptions {
    /// 是否挂接（`false` 时 [`derive_node_ports`] 不做任何事）。
    pub enabled: bool,
    /// 默认端口侧容量（对齐 `PORT_SIDE_CAPACITY`，建议 4）。
    pub default_capacity: u32,
    /// 要挂接的侧（默认四侧全挂；可只挂部分）。
    pub sides: Vec<PortSide>,
}

impl Default for DerivePortsOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            default_capacity: 4,
            sides: vec![
                PortSide::MainLow,
                PortSide::MainHigh,
                PortSide::CrossLow,
                PortSide::CrossHigh,
            ],
        }
    }
}

/// 为每个节点批量挂接侧端口（24 号文 R5）。
///
/// 对 `bp.nodes` 中每个节点、`options.sides` 中每一侧：[`BlueprintIndex::resolve_host_track`]
/// 成功 → 挂接 `slot_index = 0 .. default_capacity`（每槽 `capacity=1`，真多 slot）；
/// 失败（边界无该侧缝）→ **跳过该侧**（不整图失败）。
/// 挂接结果写入 `index.node_ports`（节点名 → 端口 id，按挂接序）。
///
/// 端口 id 由 `substrate.alloc_port_id()` 顺序分配；节点按名升序、侧按 `options.sides`
/// 序、槽按 `slot_index` 升序遍历，故挂接序与 id 分配均确定（AGENTS.md §2）。
pub fn derive_node_ports(
    substrate: &mut Substrate,
    bp: &ChannelBlueprint,
    index: &mut BlueprintIndex,
    options: &DerivePortsOptions,
) -> Result<(), DeriveError> {
    if !options.enabled {
        return Ok(());
    }
    let slot_n = options.default_capacity.max(1);
    for node in bp.nodes.keys() {
        let mut attached: Vec<PortSlotId> = Vec::new();
        for &side in &options.sides {
            if let Some(track) = index.resolve_host_track(bp, node, side) {
                for slot_index in 0..slot_n {
                    let id = substrate.alloc_port_id();
                    // 每槽独占一条边（capacity=1）；平行边靠不同 slot_index 区分
                    substrate.attach_port(id, node, side, slot_index, track, 1)?;
                    attached.push(id);
                }
            }
        }
        if !attached.is_empty() {
            index.node_ports.insert(node.clone(), attached);
        }
    }
    Ok(())
}

/// 收集组的全部后代节点（直系成员 + 子组成员，递归）。按节点名排序，确定性。
fn collect_descendants(
    gname: &str,
    bp: &ChannelBlueprint,
    children: &BTreeMap<String, Vec<String>>,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![gname.to_string()];
    while let Some(g) = stack.pop() {
        if let Some(spec) = bp.groups.get(&g) {
            out.extend(spec.members.iter().cloned());
        }
        if let Some(kids) = children.get(&g) {
            stack.extend(kids.iter().cloned());
        }
    }
    out.sort();
    out.dedup();
    out
}

fn depth_of(g: &str, groups: &BTreeMap<String, GroupSpec>) -> usize {
    let mut d = 0;
    let mut cur = groups.get(g).and_then(|s| s.parent.clone());
    while let Some(p) = cur {
        d += 1;
        cur = groups.get(&p).and_then(|s| s.parent.clone());
    }
    d
}

/// `anc` 是否为 `g` 的真祖先（沿 parent 链）。
fn is_ancestor(anc: &str, g: &str, groups: &BTreeMap<String, GroupSpec>) -> bool {
    let mut cur = groups.get(g).and_then(|s| s.parent.clone());
    while let Some(p) = cur {
        if p == anc {
            return true;
        }
        cur = groups.get(&p).and_then(|s| s.parent.clone());
    }
    false
}
