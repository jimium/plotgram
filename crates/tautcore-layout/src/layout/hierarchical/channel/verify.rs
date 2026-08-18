//! Path-level scope verifier (architecture §6.3 third line of defence).

use super::search::{ChannelPath, ScopeMask};
use super::substrate::Substrate;

/// Every track on the path must be allowed by the edge ScopeMask and must
/// not be a foreign-group through-highway (architecture §6.3).
pub fn verify_route_scope(
    substrate: &Substrate,
    mask: &ScopeMask,
    path: &ChannelPath,
) -> Result<(), String> {
    let foreign = super::search::foreign_groups(substrate, mask);
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
        if super::search::cross_covers_foreign_in(t, &foreign) {
            return Err(format!(
                "track {tid:?} is a foreign-group through-highway (architecture §6.3)"
            ));
        }
    }
    Ok(())
}
