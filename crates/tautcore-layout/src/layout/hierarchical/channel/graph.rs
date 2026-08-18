//! Channel adjacency graph + occupancy (links + gates).

use super::substrate::{GateId, Substrate, TrackId};
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
    /// Dense by `TrackId.0` (alloc ids are sequential; holes stay empty).
    adjacency: Vec<Vec<Transition>>,
}

impl<'s> ChannelGraph<'s> {
    pub fn from_substrate(substrate: &'s Substrate) -> Self {
        let mut adjacency: BTreeMap<TrackId, Vec<Transition>> = BTreeMap::new();
        for t in substrate.tracks() {
            adjacency.entry(t.id).or_default();
        }
        for &(a, b) in substrate.links() {
            adjacency.entry(a).or_default().push(Transition {
                to: b,
                via: Via::Link,
            });
            adjacency.entry(b).or_default().push(Transition {
                to: a,
                via: Via::Link,
            });
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
        let n = adjacency
            .last_key_value()
            .map(|(k, _)| k.0 as usize + 1)
            .unwrap_or(0);
        let mut dense: Vec<Vec<Transition>> = (0..n).map(|_| Vec::new()).collect();
        for (k, v) in adjacency {
            dense[k.0 as usize] = v;
        }
        Self {
            substrate,
            adjacency: dense,
        }
    }

    pub fn substrate(&self) -> &Substrate {
        self.substrate
    }

    pub fn neighbors(&self, track: TrackId) -> &[Transition] {
        self.adjacency
            .get(track.0 as usize)
            .map_or(&[], Vec::as_slice)
    }
}

/// Dense-by-id occupancy (TrackId/GateId are sequential alloc ids; reads of
/// never-touched ids fall through to zero without allocating).
#[derive(Debug, Clone, Default)]
pub struct Occupancy {
    track_usage: Vec<u32>,
    gate_usage: Vec<u32>,
    /// Per-track occupied plan-grid intervals (from committed paths).
    track_intervals: Vec<Vec<(usize, usize)>>,
}

impl Occupancy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn lane_demand(&self, track: TrackId) -> u32 {
        self.track_usage.get(track.0 as usize).copied().unwrap_or(0)
    }

    #[allow(dead_code)]
    pub fn gate_load(&self, gate: GateId) -> u32 {
        self.gate_usage.get(gate.0 as usize).copied().unwrap_or(0)
    }

    /// Gates are unbounded (P5-6 deleted `GateCapacity::Fixed`).
    pub fn gate_open(&self, substrate: &Substrate, gate: GateId) -> bool {
        substrate.gate(gate).is_some()
    }

    /// Count how many committed intervals on `track` properly overlap `ext`.
    pub fn crossing_count(&self, track: TrackId, ext: (usize, usize)) -> u32 {
        let Some(ivs) = self.track_intervals.get(track.0 as usize) else {
            return 0;
        };
        ivs.iter()
            .filter(|&&(a, b)| {
                let lo = a.max(ext.0);
                let hi = b.min(ext.1);
                hi > lo
            })
            .count() as u32
    }

    fn ensure_track_capacity(&mut self, i: usize) {
        if i >= self.track_usage.len() {
            self.track_usage.resize(i + 1, 0);
        }
        if i >= self.track_intervals.len() {
            self.track_intervals.resize_with(i + 1, Vec::new);
        }
    }

    pub fn commit(&mut self, substrate: &Substrate, tracks: &[TrackId], gates: &[GateId]) {
        for &t in tracks {
            let i = t.0 as usize;
            self.ensure_track_capacity(i);
            self.track_usage[i] += 1;
            if let Some(tr) = substrate.track(t) {
                self.track_intervals[i].push(tr.ext);
            }
        }
        let mut seen: BTreeSet<GateId> = BTreeSet::new();
        for &g in gates {
            if seen.insert(g) {
                let i = g.0 as usize;
                if i >= self.gate_usage.len() {
                    self.gate_usage.resize(i + 1, 0);
                }
                self.gate_usage[i] += 1;
            }
        }
    }

    pub fn release(&mut self, substrate: &Substrate, tracks: &[TrackId], gates: &[GateId]) {
        for &t in tracks {
            let i = t.0 as usize;
            if let Some(u) = self.track_usage.get_mut(i) {
                *u = u.saturating_sub(1);
            }
            if let Some(tr) = substrate.track(t) {
                if let Some(ivs) = self.track_intervals.get_mut(i) {
                    if let Some(pos) = ivs.iter().rposition(|&iv| iv == tr.ext) {
                        ivs.remove(pos);
                    }
                }
            }
        }
        let mut seen: BTreeSet<GateId> = BTreeSet::new();
        for &g in gates {
            if seen.insert(g) {
                if let Some(u) = self.gate_usage.get_mut(g.0 as usize) {
                    *u = u.saturating_sub(1);
                }
            }
        }
    }
}
