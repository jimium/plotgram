//! 基底（Substrate）：主轴 × 交叉轴张成的离散槽位骨架（22 号文 §3.1，27 号文 L1/L2）。
//!
//! **全程无几何坐标**——只有 id、拓扑关系与抽象权重。合法性在构建期拒绝，
//! 而非路由期检测：跨 scope 直连轨道会返回 [`SubstrateError::CrossScopeConnection`]，
//! 不相交的段不能 link（[`SubstrateError::NonIntersectingLink`]），跨组转移只能经
//! [`Gate`]，因此「穿组」在本模型中不可表达（H2 由构造保证；动态面见 `search::ScopeMask`）。
//!
//! ## 段（Segment）维（27 号文 L1）
//!
//! 一条 track **不是一条完整的线，而是被组边界切成的一段**：
//! - `line`：所在线的 gap 索引（Cross 段 = rank-gap，Main 段 = order-gap）；
//! - `ext`：沿线延展，**奇偶坐标**闭区间——`2j` 表示垂直方向的 gap `j`，
//!   `2j+1` 表示第 `j` 列/层的节点体。边界缝归外段（L1.1 B8）：
//!   组内段的 `ext` 以奇坐标（列/层）开头结尾，不含边界 gap 的偶坐标，
//!   因此「内段与边界缝 link」在相交判定下天然不成立。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// 轨道（段）ID。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TrackId(pub u32);

/// 组 ID。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GroupId(pub u32);

/// 闸口 ID。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct GateId(pub u32);

/// 端口槽 ID。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PortSlotId(pub u32);

/// 边 ID（声明序下标；合流检测 / 全局协调用）。
pub type EdgeId = usize;

/// 轨道走向（相对基底轴，不是屏幕方向；bend 计数依据）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackOrient {
    /// 沿主轴（rank 方向）延伸，如层内/组间纵向走廊。
    Main,
    /// 沿交叉轴（order 方向）延伸，如层间缝。
    Cross,
}

/// 组边界侧（主轴两端 + 交叉轴两端）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GateSide {
    /// 主轴低端（rank 小的一侧）。
    MainLow,
    /// 主轴高端（rank 大的一侧）。
    MainHigh,
    /// 交叉轴低端（order 小的一侧）。
    CrossLow,
    /// 交叉轴高端（order 大的一侧）。
    CrossHigh,
}

impl GateSide {
    /// 该侧 gate 配对段的走向（G-inv-1，27 号文 L2）：
    /// `MainLow/High` gate 被 **Main** 走向段穿越（穿 rank 边界），
    /// `CrossLow/High` gate 被 **Cross** 走向段穿越（穿 order 边界）。
    pub fn crossing_orient(self) -> TrackOrient {
        match self {
            GateSide::MainLow | GateSide::MainHigh => TrackOrient::Main,
            GateSide::CrossLow | GateSide::CrossHigh => TrackOrient::Cross,
        }
    }
}

/// 端口侧（无几何，与基底轴对齐；24 号文 R1）。
///
/// 与 [`GateSide`] **同构**（`From`/`Into` 互转，避免两套枚举漂移）：
/// - `MainLow`  ≈ 流向起点侧（TB 布局≈ Top）
/// - `MainHigh` ≈ 流向终点侧（≈ Bottom）
/// - `CrossLow` ≈ 交叉轴低端（≈ Left）
/// - `CrossHigh`≈ 交叉轴高端（≈ Right）
///
/// **禁止**在本层引入屏幕方向依赖；与 `direction`/`Port` 的映射留给 Dialect / Adapter。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum PortSide {
    /// 主轴低端（≈ Top）。
    MainLow,
    /// 主轴高端（≈ Bottom）。
    MainHigh,
    /// 交叉轴低端（≈ Left）。
    CrossLow,
    /// 交叉轴高端（≈ Right）。
    CrossHigh,
}

impl PortSide {
    /// 该侧端口宿主轨道的相容走向（P-inv-4）：
    /// `MainLow/High` 挂 **Cross** 轨道（层间缝进出），`CrossLow/High` 挂 **Main** 轨道（走廊进出）。
    pub fn required_orient(self) -> TrackOrient {
        match self {
            PortSide::MainLow | PortSide::MainHigh => TrackOrient::Cross,
            PortSide::CrossLow | PortSide::CrossHigh => TrackOrient::Main,
        }
    }
}

impl From<GateSide> for PortSide {
    fn from(s: GateSide) -> Self {
        match s {
            GateSide::MainLow => PortSide::MainLow,
            GateSide::MainHigh => PortSide::MainHigh,
            GateSide::CrossLow => PortSide::CrossLow,
            GateSide::CrossHigh => PortSide::CrossHigh,
        }
    }
}

impl From<PortSide> for GateSide {
    fn from(s: PortSide) -> Self {
        match s {
            PortSide::MainLow => GateSide::MainLow,
            PortSide::MainHigh => GateSide::MainHigh,
            PortSide::CrossLow => GateSide::CrossLow,
            PortSide::CrossHigh => GateSide::CrossHigh,
        }
    }
}

/// 轨道 = **段**：一条线被组边界切开后的一段，可容纳边段的一条带（27 号文 L1）。
///
/// **lane 数是输出（Demand），不是输入上限**——路由后由占用数导出，
/// 交由度量相把两侧撑开（B1 公理）。
#[derive(Debug, Clone)]
pub struct Track {
    pub id: TrackId,
    pub orient: TrackOrient,
    /// 所属作用域：`None` = 顶层；`Some(g)` = 组 g 内部。
    pub scope: Option<GroupId>,
    /// 抽象长度权重（Q4 计价用；L3：编码真实槽位跨度，恒 ≥ 1）；无量纲，非像素。
    pub span_weight: f64,
    /// 所在线的 gap 索引（Cross 段 = rank-gap，Main 段 = order-gap）。
    pub line: usize,
    /// 沿线延展，奇偶坐标闭区间：`2j` = 垂直方向 gap `j`，`2j+1` = 第 `j` 列/层节点体。
    pub ext: (usize, usize),
}

impl Track {
    /// 段是否覆盖垂直方向的 gap `g`（相交判定用；偶坐标 `2g` 落在 `ext` 内）。
    pub fn covers_gap(&self, g: usize) -> bool {
        let c = 2 * g;
        self.ext.0 <= c && c <= self.ext.1
    }

    /// 段是否覆盖第 `j` 列/层的节点体位置（端口宿主解析用；奇坐标 `2j+1`）。
    pub fn covers_slot(&self, j: usize) -> bool {
        let c = 2 * j + 1;
        self.ext.0 <= c && c <= self.ext.1
    }
}

/// 组作用域（支持任意深度嵌套）。
///
/// `ranks` / `orders` 为组矩形（成员节点槽位包围盒，闭区间），
/// 供 [`Substrate::verify_no_group_penetration`]（L6）做无穿组自反证。
#[derive(Debug, Clone)]
pub struct GroupScope {
    pub id: GroupId,
    pub parent: Option<GroupId>,
    /// rank 跨度 `[r0, r1]`（闭区间）。
    pub ranks: (usize, usize),
    /// order 跨度 `[o0, o1]`（闭区间）。
    pub orders: (usize, usize),
}

/// 闸口容量（27 号文 L4）：生产路径恒 `Unbounded`——容量是**输出**
/// （相 I 记 `crossing_demand`，相 II 把边界段撑开）；`Fixed` 仅供诊断扫描。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateCapacity {
    /// 无硬容量（生产路径）：穿越数作为 Demand 输出给度量相。
    Unbounded,
    /// 精确容量（诊断扫描，探针 `gate_capacity_override`）。
    Fixed(u32),
}

/// 闸口：组**一条具体边界线**上的跨 scope 转移区（27 号文 L2）。
///
/// `crossings` 是一一配对列表而非笛卡尔积：每对 `(inner, outer)` 是**同一条线**
/// 被该边界切开后紧邻的两段（G-inv-2）。穿越该边界的每条边占 1 单位，
/// 与具体走哪对无关（共享 [`GateCapacity`] 池）。
#[derive(Debug, Clone)]
pub struct Gate {
    pub id: GateId,
    pub group: GroupId,
    pub side: GateSide,
    /// 被穿越的边界线：`MainLow`=rank-gap `r0`、`MainHigh`=rank-gap `r1+1`、
    /// `CrossLow`=order-gap `o0`、`CrossHigh`=order-gap `o1+1`。
    pub line: usize,
    /// 跨该边界线的段对：(组内段, 组外段)，一一配对而非笛卡尔积。
    pub crossings: Vec<(TrackId, TrackId)>,
    /// 容量池（L4：生产 `Unbounded`，诊断 `Fixed`）。
    pub capacity: GateCapacity,
}

/// 稳定节点标识（24 号文方案 A：与 [`super::derive::ChannelBlueprint`] 的节点名键一致）。
pub type NodeKey = String;

/// 端口槽：边端点在基底上的挂接点，唯一出入口是其宿主轨道。
///
/// `capacity` 为端口侧容量（22 号文 I.5 `PORT_SIDE_CAPACITY` 硬约束）：
/// 同一挂接点可容纳的边数上限，由节点边长 / 最小端口间距离散化。
///
/// **语义身份**（24 号文 R1）：`(node, side, slot_index)` 三元组唯一标识一个端口，
/// 使上层（I.5 / Legacy Adapter）能把「已定侧」或「候选侧」交给通道图。
#[derive(Debug, Clone)]
pub struct PortSlot {
    pub id: PortSlotId,
    /// 宿主轨道（唯一出入口）。
    pub track: TrackId,
    /// 端口侧容量（0 = 不限；默认建议 4）。
    pub capacity: u32,
    /// 所属节点（稳定标识）。
    pub node: NodeKey,
    /// 端口侧。
    pub side: PortSide,
    /// 同侧离散槽（无几何错开；平行边区分用，默认 0）。
    pub slot_index: u32,
}

/// L6 违规项：段 `track` 的延展与无关组 `group` 的内部相交。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PenetrationViolation {
    pub track: TrackId,
    pub group: GroupId,
}

/// 基底构建错误：非法结构在构建期显式拒绝。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubstrateError {
    UnknownTrack(TrackId),
    UnknownGroup(GroupId),
    UnknownPort(PortSlotId),
    DuplicateTrack(TrackId),
    DuplicateGroup(GroupId),
    DuplicateGate(GateId),
    DuplicatePort(PortSlotId),
    /// 同一 `(node, side, slot_index)` 重复注册（P-inv-2）。
    DuplicatePortSlot {
        node: NodeKey,
        side: PortSide,
        slot_index: u32,
    },
    /// 端口侧与宿主轨道走向不相容（P-inv-4）。
    PortSideMismatch {
        side: PortSide,
        orient: TrackOrient,
    },
    /// 零长/倒置延展（L1.1 B5：零长段不注册）。
    InvalidExtent { track: TrackId },
    /// 自环或重复 link。
    InvalidLink { a: TrackId, b: TrackId },
    /// 同向轨道相交被拒绝：平行轨道无交点，转移只发生在 Main × Cross 交口。
    ParallelLink { a: TrackId, b: TrackId },
    /// 段延展不相交被拒绝（L1「相交才 link」；B8 内段与边界缝天然不相交）。
    NonIntersectingLink { a: TrackId, b: TrackId },
    /// 跨 scope 直连被拒绝——跨组转移只能经 Gate（H2 由构造保证）。
    CrossScopeConnection { a: TrackId, b: TrackId },
    /// Gate 配对段 scope 不合法：inner 必须属该组，outer 必须属祖先 scope。
    InvalidGateScope { gate: GateId, group: GroupId },
    /// Gate 配对违反 G-inv-1/G-inv-2：走向与 side 不符 / 不同线 / 不隔边界相邻。
    GatePairMismatch {
        gate: GateId,
        inner: TrackId,
        outer: TrackId,
    },
    /// 同一 `(group, side)` 重复注册 gate（G-inv-3）。
    DuplicateGateSide { group: GroupId, side: GateSide },
    /// Gate 配对列表为空。
    EmptyGate { gate: GateId },
    /// 候选端点选路时某一端候选集为空（24 号文 R3）。
    EmptyCandidates,
}

impl fmt::Display for SubstrateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTrack(t) => write!(f, "unknown track {t:?}"),
            Self::UnknownGroup(g) => write!(f, "unknown group {g:?}"),
            Self::UnknownPort(p) => write!(f, "unknown port {p:?}"),
            Self::DuplicateTrack(t) => write!(f, "duplicate track {t:?}"),
            Self::DuplicateGroup(g) => write!(f, "duplicate group {g:?}"),
            Self::DuplicateGate(g) => write!(f, "duplicate gate {g:?}"),
            Self::DuplicatePort(p) => write!(f, "duplicate port {p:?}"),
            Self::DuplicatePortSlot {
                node,
                side,
                slot_index,
            } => write!(f, "duplicate port slot ({node}, {side:?}, {slot_index})"),
            Self::PortSideMismatch { side, orient } => write!(
                f,
                "port side {side:?} requires {:?} track, got {orient:?}",
                side.required_orient()
            ),
            Self::InvalidExtent { track } => {
                write!(f, "track {track:?} has empty/inverted extent (B5: no zero-length segment)")
            }
            Self::InvalidLink { a, b } => write!(f, "invalid link {a:?} - {b:?}"),
            Self::ParallelLink { a, b } => {
                write!(f, "parallel link {a:?} - {b:?} rejected: tracks never intersect")
            }
            Self::NonIntersectingLink { a, b } => {
                write!(f, "link {a:?} - {b:?} rejected: segment extents do not intersect")
            }
            Self::CrossScopeConnection { a, b } => {
                write!(f, "cross-scope link {a:?} - {b:?} rejected: use a Gate")
            }
            Self::InvalidGateScope { gate, group } => {
                write!(f, "gate {gate:?} scope invalid for group {group:?}")
            }
            Self::GatePairMismatch { gate, inner, outer } => write!(
                f,
                "gate {gate:?} pair ({inner:?}, {outer:?}) violates G-inv-1/2 (orient/line/adjacency)"
            ),
            Self::DuplicateGateSide { group, side } => {
                write!(f, "duplicate gate for ({group:?}, {side:?}) (G-inv-3)")
            }
            Self::EmptyGate { gate } => write!(f, "gate {gate:?} has empty crossings"),
            Self::EmptyCandidates => write!(f, "empty candidate endpoint set"),
        }
    }
}

/// 基底：确定性容器（[`BTreeMap`]，AGENTS.md §2）。
#[derive(Debug, Clone, Default)]
pub struct Substrate {
    tracks: BTreeMap<TrackId, Track>,
    groups: BTreeMap<GroupId, GroupScope>,
    gates: BTreeMap<GateId, Gate>,
    ports: BTreeMap<PortSlotId, PortSlot>,
    /// `(node, side, slot_index)` 唯一性索引（P-inv-2 构建期拒绝重复）。
    port_keys: BTreeSet<(NodeKey, PortSide, u32)>,
    /// `(group, side)` 唯一性索引（G-inv-3 构建期拒绝重复）。
    gate_keys: BTreeSet<(GroupId, GateSide)>,
    /// 同 scope 轨道相交关系（无向，存归一化 (min, max)；T1：有序集合，O(log L) 查重）。
    links: BTreeSet<(TrackId, TrackId)>,
}

impl Substrate {
    pub fn new() -> Self {
        Self::default()
    }

    // ─── ID 分配器（供推导 adapter 使用，取当前最大 id + 1，确定性）───

    pub fn alloc_track_id(&self) -> TrackId {
        TrackId(self.tracks.keys().next_back().map_or(0, |k| k.0 + 1))
    }
    pub fn alloc_group_id(&self) -> GroupId {
        GroupId(self.groups.keys().next_back().map_or(0, |k| k.0 + 1))
    }
    pub fn alloc_gate_id(&self) -> GateId {
        GateId(self.gates.keys().next_back().map_or(0, |k| k.0 + 1))
    }
    pub fn alloc_port_id(&self) -> PortSlotId {
        PortSlotId(self.ports.keys().next_back().map_or(0, |k| k.0 + 1))
    }

    /// 注册组作用域；`parent = None` 表示顶层直属。
    /// `ranks` / `orders` 为组矩形（闭区间），供 L6 无穿组自反证。
    pub fn add_group(
        &mut self,
        id: GroupId,
        parent: Option<GroupId>,
        ranks: (usize, usize),
        orders: (usize, usize),
    ) -> Result<(), SubstrateError> {
        if self.groups.contains_key(&id) {
            return Err(SubstrateError::DuplicateGroup(id));
        }
        if let Some(p) = parent {
            if !self.groups.contains_key(&p) {
                return Err(SubstrateError::UnknownGroup(p));
            }
        }
        self.groups.insert(
            id,
            GroupScope {
                id,
                parent,
                ranks,
                orders,
            },
        );
        Ok(())
    }

    /// 注册段（L1）：`line` 为所在线 gap 索引，`ext` 为沿线奇偶坐标闭区间。
    /// `span_weight` 为 Q4 抽象权重（L3：应编码真实槽位跨度，恒 ≥ 1）。
    ///
    /// 零长/倒置延展在构建期拒绝（B5：零长段不注册）。
    pub fn add_track(
        &mut self,
        id: TrackId,
        orient: TrackOrient,
        scope: Option<GroupId>,
        span_weight: f64,
        line: usize,
        ext: (usize, usize),
    ) -> Result<(), SubstrateError> {
        if self.tracks.contains_key(&id) {
            return Err(SubstrateError::DuplicateTrack(id));
        }
        if ext.0 > ext.1 {
            return Err(SubstrateError::InvalidExtent { track: id });
        }
        if let Some(g) = scope {
            if !self.groups.contains_key(&g) {
                return Err(SubstrateError::UnknownGroup(g));
            }
        }
        self.tracks.insert(
            id,
            Track {
                id,
                orient,
                scope,
                span_weight,
                line,
                ext,
            },
        );
        Ok(())
    }

    /// 声明两条**同 scope、异向、几何相交**的段可互相转移（L1「相交才 link」）。
    ///
    /// 三道构建期防线：
    /// - 跨 scope 拒绝 → 穿组不可表达（H2 静态面）；
    /// - 同向拒绝 → 转移只在 Main × Cross 交口发生，折点 = link 转移（H3）；
    /// - 不相交拒绝 → 段延展必须覆盖对方所在线（B8：组内段与边界缝的偶坐标
    ///   不在其 `ext` 内，link 无法旁路 gate）。
    pub fn link(&mut self, a: TrackId, b: TrackId) -> Result<(), SubstrateError> {
        let ta = self
            .tracks
            .get(&a)
            .ok_or(SubstrateError::UnknownTrack(a))?;
        let tb = self
            .tracks
            .get(&b)
            .ok_or(SubstrateError::UnknownTrack(b))?;
        if a == b {
            return Err(SubstrateError::InvalidLink { a, b });
        }
        if ta.scope != tb.scope {
            return Err(SubstrateError::CrossScopeConnection { a, b });
        }
        if ta.orient == tb.orient {
            return Err(SubstrateError::ParallelLink { a, b });
        }
        if !ta.covers_gap(tb.line) || !tb.covers_gap(ta.line) {
            return Err(SubstrateError::NonIntersectingLink { a, b });
        }
        let key = if a < b { (a, b) } else { (b, a) };
        if !self.links.insert(key) {
            return Err(SubstrateError::InvalidLink { a, b });
        }
        Ok(())
    }

    /// 注册边界线闸口（L2）：`crossings` 为跨 `line` 边界的 (组内段, 组外段) 配对列表。
    ///
    /// 构建期校验三条不变量（27 号文 L2）：
    /// - **G-inv-1**：配对段走向与 `side` 相符（[`GateSide::crossing_orient`]）；
    /// - **G-inv-2**：每对同线（`inner.line == outer.line`）、隔边界相邻——
    ///   Low 侧 `outer.ext.1 == 2*line` 且 `inner.ext.0 == 2*line+1`，
    ///   High 侧 `inner.ext.1 == 2*line-1` 且 `outer.ext.0 == 2*line`（B8：边界缝归外段）；
    /// - **G-inv-3**：同一 `(group, side)` 只允许一个 gate。
    ///
    /// scope 约束沿用：inner 必须属 `group`；outer 必须属 `group` 的**某个祖先** scope
    /// （嵌套组边界可能与祖父辈的段重合，而非直接父）。
    pub fn add_gate(
        &mut self,
        id: GateId,
        group: GroupId,
        side: GateSide,
        line: usize,
        crossings: Vec<(TrackId, TrackId)>,
        capacity: GateCapacity,
    ) -> Result<(), SubstrateError> {
        if self.gates.contains_key(&id) {
            return Err(SubstrateError::DuplicateGate(id));
        }
        if crossings.is_empty() {
            return Err(SubstrateError::EmptyGate { gate: id });
        }
        if !self.groups.contains_key(&group) {
            return Err(SubstrateError::UnknownGroup(group));
        }
        if self.gate_keys.contains(&(group, side)) {
            return Err(SubstrateError::DuplicateGateSide { group, side });
        }
        let orient = side.crossing_orient();
        for &(inner, outer) in &crossings {
            let ti = self
                .tracks
                .get(&inner)
                .ok_or(SubstrateError::UnknownTrack(inner))?;
            let to = self
                .tracks
                .get(&outer)
                .ok_or(SubstrateError::UnknownTrack(outer))?;
            if ti.scope != Some(group) || !self.is_ancestor_scope(to.scope, group) {
                return Err(SubstrateError::InvalidGateScope { gate: id, group });
            }
            // G-inv-1：走向匹配 side；G-inv-2：同线 + 隔边界相邻（B8 边界缝归外段）
            let adjacent = match side {
                GateSide::MainLow | GateSide::CrossLow => {
                    to.ext.1 == 2 * line && ti.ext.0 == 2 * line + 1
                }
                GateSide::MainHigh | GateSide::CrossHigh => {
                    line > 0 && ti.ext.1 == 2 * line - 1 && to.ext.0 == 2 * line
                }
            };
            if ti.orient != orient || to.orient != orient || ti.line != to.line || !adjacent {
                return Err(SubstrateError::GatePairMismatch {
                    gate: id,
                    inner,
                    outer,
                });
            }
        }
        self.gate_keys.insert((group, side));
        self.gates.insert(
            id,
            Gate {
                id,
                group,
                side,
                line,
                crossings,
                capacity,
            },
        );
        Ok(())
    }

    /// 挂接节点端口槽到宿主轨道（24 号文 R1/R2）。
    ///
    /// 构建期防线：
    /// - P-inv-1：`track` 必须存在；
    /// - P-inv-2：同一 `(node, side, slot_index)` 不得重复注册；
    /// - P-inv-3：`id` 不得重复；
    /// - P-inv-4：宿主 `track.orient` 必须与 `side` 相容（[`PortSide::required_orient`]）。
    ///
    /// `capacity` 为端口侧容量（0 = 不限）。
    pub fn attach_port(
        &mut self,
        id: PortSlotId,
        node: &str,
        side: PortSide,
        slot_index: u32,
        track: TrackId,
        capacity: u32,
    ) -> Result<(), SubstrateError> {
        if self.ports.contains_key(&id) {
            return Err(SubstrateError::DuplicatePort(id));
        }
        let orient = self
            .tracks
            .get(&track)
            .ok_or(SubstrateError::UnknownTrack(track))?
            .orient;
        if orient != side.required_orient() {
            return Err(SubstrateError::PortSideMismatch { side, orient });
        }
        let key = (node.to_string(), side, slot_index);
        if !self.port_keys.insert(key.clone()) {
            return Err(SubstrateError::DuplicatePortSlot {
                node: node.to_string(),
                side,
                slot_index,
            });
        }
        self.ports.insert(
            id,
            PortSlot {
                id,
                track,
                capacity,
                node: node.to_string(),
                side,
                slot_index,
            },
        );
        Ok(())
    }

    /// [`Self::attach_port`] 的别名（24 号文 API 命名）。
    pub fn attach_node_port(
        &mut self,
        id: PortSlotId,
        node: &str,
        side: PortSide,
        slot_index: u32,
        track: TrackId,
        capacity: u32,
    ) -> Result<(), SubstrateError> {
        self.attach_port(id, node, side, slot_index, track, capacity)
    }

    pub fn track(&self, id: TrackId) -> Option<&Track> {
        self.tracks.get(&id)
    }

    /// `scope` 是否为 `group` 的祖先作用域（沿 parent 链上溯，不含 group 自身）。
    /// `scope = None`（根）是任何组的祖先。
    pub fn is_ancestor_scope(&self, scope: Option<GroupId>, group: GroupId) -> bool {
        let mut cur = self.groups.get(&group).and_then(|g| g.parent);
        while let Some(c) = cur {
            if Some(c) == scope {
                return true;
            }
            cur = self.groups.get(&c).and_then(|g| g.parent);
        }
        scope.is_none()
    }

    /// `scope` 的作用域链（含自身，沿 parent 上溯到根；`None` 返回空）。
    /// L8 掩码构建用：`chain(g) = g 及其全部祖先组`。
    pub fn scope_chain(&self, scope: Option<GroupId>) -> Vec<GroupId> {
        let mut out = Vec::new();
        let mut cur = scope;
        while let Some(g) = cur {
            out.push(g);
            cur = self.groups.get(&g).and_then(|s| s.parent);
        }
        out
    }

    pub fn group(&self, id: GroupId) -> Option<&GroupScope> {
        self.groups.get(&id)
    }

    pub fn gate(&self, id: GateId) -> Option<&Gate> {
        self.gates.get(&id)
    }

    pub fn port(&self, id: PortSlotId) -> Option<&PortSlot> {
        self.ports.get(&id)
    }

    /// 节点的全部端口（确定性顺序：按 `PortSlotId` 升序，即挂接序）。
    pub fn ports_of_node(&self, node: &str) -> Vec<&PortSlot> {
        // BTreeMap 迭代天然按 id 升序，filter 后顺序保持。
        self.ports.values().filter(|p| p.node == node).collect()
    }

    /// 节点某一侧的端口（确定性顺序：按 `(slot_index, id)` 升序）。
    pub fn ports_of_node_side(&self, node: &str, side: PortSide) -> Vec<&PortSlot> {
        let mut out: Vec<&PortSlot> = self
            .ports
            .values()
            .filter(|p| p.node == node && p.side == side)
            .collect();
        out.sort_by_key(|p| (p.slot_index, p.id));
        out
    }

    /// 按语义身份 `(node, side, slot_index)` 查端口（24 号文 R1）。
    pub fn find_port(&self, node: &str, side: PortSide, slot_index: u32) -> Option<&PortSlot> {
        self.ports
            .values()
            .find(|p| p.node == node && p.side == side && p.slot_index == slot_index)
    }

    pub fn tracks(&self) -> impl Iterator<Item = &Track> {
        self.tracks.values()
    }

    pub fn gates(&self) -> impl Iterator<Item = &Gate> {
        self.gates.values()
    }

    pub fn groups(&self) -> impl Iterator<Item = &GroupScope> {
        self.groups.values()
    }

    /// 全部 link（归一化 (min, max) 无向对，有序集合，T1）。
    pub fn links(&self) -> &BTreeSet<(TrackId, TrackId)> {
        &self.links
    }

    /// L6 静态检查器：断言「无穿组」由构造成立，返回违规清单（空 = 全通过）。
    ///
    /// 对每段 s × 每组 G（G ∉ `scope_chain(s.scope)`，即 G 既非 s 的归属组
    /// 也非其祖先——子孙组的段合法居于 G 内部，由链排除）：
    /// - Cross 段（line = rank-gap）：`r0 < s.line ≤ r1` 且 `s.ext` 与 G 的
    ///   order 向内部奇偶区间 `[2·o0+1, 2·o1+1]` 相交 → 违规；
    /// - Main 段（line = order-gap）：对称（`o0 < s.line ≤ o1` × rank 向内部）。
    ///
    /// 「内部」用奇坐标端点表达：边界缝（偶坐标 `2·o0` / `2·(o1+1)`）不算内部，
    /// 因此贴边界的组外段（B8 边界缝归外段）不误报。
    pub fn verify_no_group_penetration(&self) -> Vec<PenetrationViolation> {
        let mut violations = Vec::new();
        for track in self.tracks.values() {
            let chain = self.scope_chain(track.scope);
            for group in self.groups.values() {
                if chain.contains(&group.id) {
                    continue;
                }
                let (r0, r1) = group.ranks;
                let (o0, o1) = group.orders;
                // (被穿越的 gap 区间, 内部奇偶区间)：按段走向选轴。
                let (cuts, interior) = match track.orient {
                    TrackOrient::Cross => ((r0, r1), (2 * o0 + 1, 2 * o1 + 1)),
                    TrackOrient::Main => ((o0, o1), (2 * r0 + 1, 2 * r1 + 1)),
                };
                let line_inside = cuts.0 < track.line && track.line <= cuts.1;
                let ext_overlaps =
                    track.ext.0.max(interior.0) <= track.ext.1.min(interior.1);
                if line_inside && ext_overlaps {
                    violations.push(PenetrationViolation {
                        track: track.id,
                        group: group.id,
                    });
                }
            }
        }
        violations
    }
}
