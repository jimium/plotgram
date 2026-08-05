//! Channel adjacency graph + occupancy (links + gates).

use super::substrate::{GateCapacity, GateId, Substrate, TrackId};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Via {
    Link,
    Gate(GateId),
}

#[derive(Debug, Clone, Copy)]
pub struct Transition {
    pub to: TrackId,
    pub via: Via,
}

#[derive(Debug, Clone)]
pub struct ChannelGraph<'s> {
    substrate: &'s Substrate,
    adjacency: BTreeMap<TrackId, Vec<Transition>>,
}

impl<'s> ChannelGraph<'s> {
    pub fn from_substrate(substrate: &'s Substrate) -> Self {
        let mut adjacency: BTreeMap<TrackId, Vec<Transition>> = BTreeMap::new();
        for t in substrate.tracks() {
            adjacency.entry(t.id).or_default();
        }
        for &(a, b) in substrate.links() {
            adjacency
                .entry(a)
                .or_default()
                .push(Transition { to: b, via: Via::Link });
            adjacency
                .entry(b)
                .or_default()
                .push(Transition { to: a, via: Via::Link });
        }
        for g in substrate.gates() {
            for &(inner, outer) in &g.crossings {
                adjacency.entry(inner).or_default().push(Transition {
                    to: outer,
                    via: Via::Gate(g.id),
                });
                adjacency.entry(outer).or_default().push(Transition {
                    to: inner,
                    via: Via::Gate(g.id),
                });
            }
        }
        for neighbors in adjacency.values_mut() {
            neighbors.sort_by_key(|t| {
                let via_key = match t.via {
                    Via::Link => (0u8, 0u32),
                    Via::Gate(g) => (1u8, g.0),
                };
                (t.to, via_key)
            });
        }
        Self {
            substrate,
            adjacency,
        }
    }

    pub fn substrate(&self) -> &Substrate {
        self.substrate
    }

    pub fn neighbors(&self, track: TrackId) -> &[Transition] {
        self.adjacency.get(&track).map_or(&[], Vec::as_slice)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Occupancy {
    track_usage: BTreeMap<TrackId, u32>,
    gate_usage: BTreeMap<GateId, u32>,
}

impl Occupancy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn lane_demand(&self, track: TrackId) -> u32 {
        self.track_usage.get(&track).copied().unwrap_or(0)
    }

    pub fn gate_load(&self, gate: GateId) -> u32 {
        self.gate_usage.get(&gate).copied().unwrap_or(0)
    }

    pub fn gate_open(&self, substrate: &Substrate, gate: GateId) -> bool {
        substrate.gate(gate).is_some_and(|g| match g.capacity {
            GateCapacity::Unbounded => true,
            GateCapacity::Fixed(c) => self.gate_load(gate) < c,
        })
    }

    pub fn commit(&mut self, tracks: &[TrackId], gates: &[GateId]) {
        for &t in tracks {
            *self.track_usage.entry(t).or_default() += 1;
        }
        let mut seen: BTreeSet<GateId> = BTreeSet::new();
        for &g in gates {
            if seen.insert(g) {
                *self.gate_usage.entry(g).or_default() += 1;
            }
        }
    }

    pub fn release(&mut self, tracks: &[TrackId], gates: &[GateId]) {
        for &t in tracks {
            if let Some(u) = self.track_usage.get_mut(&t) {
                *u = u.saturating_sub(1);
            }
        }
        let mut seen: BTreeSet<GateId> = BTreeSet::new();
        for &g in gates {
            if seen.insert(g) {
                if let Some(u) = self.gate_usage.get_mut(&g) {
                    *u = u.saturating_sub(1);
                }
            }
        }
    }
}
