//! D1.2 Channel: group-cut Substrate + ScopeMask search + bounded rip-up.
//!
//! Atlas shape reference:
//! `docs/design/layout/hierarchical/atlas-reference/channel-*.md`.

mod bundle;
mod derive;
mod graph;
mod route_all;
mod search;
mod substrate;
mod verify;

pub use route_all::{route_edges_channel, ChannelRoutePlan, RouteTopology};
pub use search::ChannelPath;
pub use substrate::{derive_root_substrate, Substrate, TrackId, TrackOrient};

// Re-exported for ink unit fixtures / debug.
#[allow(unused_imports)]
pub use substrate::BlueprintIndex;
