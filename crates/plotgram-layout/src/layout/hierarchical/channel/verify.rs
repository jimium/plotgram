//! Path-level scope verifier (architecture §6.3 third line of defence).

use super::search::{ChannelPath, ScopeMask};
use super::substrate::Substrate;

/// Every track on the path must be allowed by the edge ScopeMask.
pub fn verify_route_scope(
    substrate: &Substrate,
    mask: &ScopeMask,
    path: &ChannelPath,
) -> Result<(), String> {
    for &tid in &path.tracks {
        let Some(t) = substrate.track(tid) else {
            return Err(format!("unknown track {tid:?}"));
        };
        if !mask.allows(t.scope) {
            return Err(format!(
                "track {tid:?} scope {:?} not in edge ScopeMask",
                t.scope
            ));
        }
    }
    Ok(())
}
