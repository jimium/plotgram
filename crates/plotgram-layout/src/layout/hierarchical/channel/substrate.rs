//! Channel Substrate: discrete corridor skeleton (root-scope or group-cut).
//!
//! Odd-even extent encoding matches Atlas: `2j` = gap j, `2j+1` = node body j.
//! Cross-scope links are rejected at build time — group crossings use Gates.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[cfg(test)]
use crate::layout::hierarchical::model::PlanGraph;


/// Discrete track (segment) identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TrackId(pub u32);

/// Group identity on the substrate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GroupId(pub u32);

/// Gate identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GateId(pub u32);

/// Track orientation relative to the hierarchy axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackOrient {
    /// Along main (rank) axis — vertical corridor in TB.
    Main,
    /// Along cross (order) axis — horizontal layer-gap corridor in TB.
    Cross,
}

/// Group boundary side (isomorphic to [`PortSide`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GateSide {
    MainLow,
    MainHigh,
    CrossLow,
    CrossHigh,
}

impl GateSide {
    /// Orient of segments that cross this side's boundary.
    pub fn crossing_orient(self) -> TrackOrient {
        match self {
            GateSide::MainLow | GateSide::MainHigh => TrackOrient::Main,
            GateSide::CrossLow | GateSide::CrossHigh => TrackOrient::Cross,
        }
    }
}

/// Port side on the substrate (canonical TB mapping).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PortSide {
    MainLow,
    MainHigh,
    CrossLow,
    CrossHigh,
}

impl PortSide {
    pub fn from_algo_side(side: plotgram_algo::orientation::Side) -> Self {
        use plotgram_algo::orientation::Side;
        match side {
            Side::North => PortSide::MainLow,
            Side::South => PortSide::MainHigh,
            Side::West => PortSide::CrossLow,
            Side::East => PortSide::CrossHigh,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Track {
    pub id: TrackId,
    pub orient: TrackOrient,
    /// Deepest group that owns this segment (`None` = root).
    pub scope: Option<GroupId>,
    /// Gap line index (Cross = rank-gap, Main = order-gap).
    pub line: usize,
    /// Odd-even closed extent along the perpendicular axis.
    pub ext: (usize, usize),
    /// Logical length (≥ 1).
    pub span_weight: f64,
}

impl Track {
    pub fn covers_gap(&self, gap_line: usize) -> bool {
        let c = 2 * gap_line;
        self.ext.0 <= c && c <= self.ext.1
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // ranks/orders reserved for L6 penetration verifier
pub struct GroupScope {
    pub id: GroupId,
    pub parent: Option<GroupId>,
    pub ranks: (usize, usize),
    pub orders: (usize, usize),
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // group/side/line projected in debug / Demand later
pub struct Gate {
    pub id: GateId,
    pub group: GroupId,
    pub side: GateSide,
    pub line: usize,
    pub crossings: Vec<(TrackId, TrackId)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubstrateError {
    UnknownTrack(TrackId),
    UnknownGroup(GroupId),
    DuplicateTrack(TrackId),
    DuplicateGroup(GroupId),
    DuplicateGate(GateId),
    InvalidExtent { track: TrackId },
    InvalidLink { a: TrackId, b: TrackId },
    ParallelLink { a: TrackId, b: TrackId },
    NonIntersectingLink { a: TrackId, b: TrackId },
    CrossScopeConnection { a: TrackId, b: TrackId },
    InvalidGateScope { gate: GateId, group: GroupId },
    GatePairMismatch {
        gate: GateId,
        inner: TrackId,
        outer: TrackId,
    },
    DuplicateGateSide { group: GroupId, side: GateSide },
    EmptyGate { gate: GateId },
}

impl fmt::Display for SubstrateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

#[derive(Debug, Default, Clone)]
pub struct Substrate {
    tracks: BTreeMap<TrackId, Track>,
    groups: BTreeMap<GroupId, GroupScope>,
    gates: BTreeMap<GateId, Gate>,
    gate_keys: BTreeSet<(GroupId, GateSide)>,
    links: BTreeSet<(TrackId, TrackId)>,
    next_track: u32,
    next_group: u32,
    next_gate: u32,
}

impl Substrate {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn alloc_track_id(&mut self) -> TrackId {
        let id = TrackId(self.next_track);
        self.next_track += 1;
        id
    }

    pub fn alloc_group_id(&mut self) -> GroupId {
        let id = GroupId(self.next_group);
        self.next_group += 1;
        id
    }

    pub fn alloc_gate_id(&mut self) -> GateId {
        let id = GateId(self.next_gate);
        self.next_gate += 1;
        id
    }

    pub fn track(&self, id: TrackId) -> Option<&Track> {
        self.tracks.get(&id)
    }

    pub fn tracks(&self) -> impl Iterator<Item = &Track> {
        self.tracks.values()
    }

    #[allow(dead_code)] // used by future L6 / debug projectors
    pub fn group(&self, id: GroupId) -> Option<&GroupScope> {
        self.groups.get(&id)
    }

    pub fn gate(&self, id: GateId) -> Option<&Gate> {
        self.gates.get(&id)
    }

    pub fn gates(&self) -> impl Iterator<Item = &Gate> {
        self.gates.values()
    }

    pub fn links(&self) -> &BTreeSet<(TrackId, TrackId)> {
        &self.links
    }

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
                line,
                ext,
                span_weight: span_weight.max(1.0),
            },
        );
        Ok(())
    }

    /// Same-scope intersecting Main×Cross link.
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

    pub fn add_gate(
        &mut self,
        id: GateId,
        group: GroupId,
        side: GateSide,
        line: usize,
        crossings: Vec<(TrackId, TrackId)>,
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
            },
        );
        Ok(())
    }

    /// `scope` is an ancestor of `group` (root/`None` counts).
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

    pub fn scope_chain(&self, scope: Option<GroupId>) -> Vec<GroupId> {
        let mut out = Vec::new();
        let mut cur = scope;
        while let Some(g) = cur {
            out.push(g);
            cur = self.groups.get(&g).and_then(|s| s.parent);
        }
        out
    }
}

/// Lightweight segment handle for line indexes.
#[derive(Debug, Clone, Copy)]
pub struct SegmentRef {
    pub id: TrackId,
    pub ext: (usize, usize),
    pub scope: Option<GroupId>,
}

impl SegmentRef {
    pub fn covers(&self, coord: usize) -> bool {
        self.ext.0 <= coord && coord <= self.ext.1
    }
}

/// Line → ordered segment refs for port attachment + ScopeMask.
#[derive(Debug, Clone, Default)]
pub struct BlueprintIndex {
    pub cross_lines: BTreeMap<usize, Vec<SegmentRef>>,
    pub main_lines: BTreeMap<usize, Vec<SegmentRef>>,
    pub rank_count: usize,
    pub order_count: usize,
    /// Group string id → substrate GroupId.
    pub group_ids: BTreeMap<String, GroupId>,
    /// Node id → deepest member group name (`None` = root).
    pub node_region: BTreeMap<String, Option<String>>,
}

impl BlueprintIndex {
    pub fn cross_at(&self, rg: usize, order: usize) -> Option<TrackId> {
        self.cross_lines
            .get(&rg)?
            .iter()
            .find(|sg| sg.covers(2 * order + 1))
            .map(|sg| sg.id)
    }

    pub fn main_at(&self, og: usize, rank: usize) -> Option<TrackId> {
        self.main_lines
            .get(&og)?
            .iter()
            .find(|sg| sg.covers(2 * rank + 1))
            .map(|sg| sg.id)
    }

    pub fn resolve_host_track(
        &self,
        rank: usize,
        order: usize,
        side: PortSide,
    ) -> Option<TrackId> {
        match side {
            PortSide::MainLow => self.cross_at(rank, order),
            PortSide::MainHigh => self.cross_at(rank + 1, order),
            PortSide::CrossLow => self.main_at(order, rank),
            PortSide::CrossHigh => self.main_at(order + 1, rank),
        }
    }

    pub fn node_scope(&self, node: &str) -> Option<GroupId> {
        match self.node_region.get(node) {
            Some(Some(g)) => self.group_ids.get(g).copied(),
            _ => None,
        }
    }
}

/// Derive a root-scope Substrate from the properified plan (no group cuts).
///
/// Implemented in [`super::derive::derive_root_substrate`] so Main lines share
/// P5-1 node-body cuts with the group path.
pub use super::derive::derive_root_substrate;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::model::{Elem, ElemKey};

    fn two_layer_plan() -> PlanGraph {
        let elems = vec![
            Elem {
                key: ElemKey::Real("a".into()),
                group_path: vec![],
                rank: 0,
            },
            Elem {
                key: ElemKey::Real("b".into()),
                group_path: vec![],
                rank: 0,
            },
            Elem {
                key: ElemKey::Real("c".into()),
                group_path: vec![],
                rank: 1,
            },
        ];
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        PlanGraph {
            elems,
            index_of,
            decl_index: (0..3).collect(),
            segments: vec![],
            layers: vec![vec![0, 1], vec![2]],
            ..Default::default()
        }
    }

    #[test]
    fn root_substrate_has_cross_and_main_lines() {
        let plan = two_layer_plan();
        let (sub, idx) = derive_root_substrate(&plan);
        assert_eq!(idx.rank_count, 2);
        assert_eq!(idx.order_count, 2);
        assert_eq!(idx.cross_lines.len(), 3);
        assert_eq!(idx.main_lines.len(), 3);
        assert!(sub.tracks().count() >= 6);
        assert!(idx.resolve_host_track(0, 0, PortSide::MainHigh).is_some());
    }
}
